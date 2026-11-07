//! Pending bookings: expiry deadlines, the expiry/nudge sweep, pending settings.

use super::*;

/// Request expiry choices offered by the event type form, in hours. `None`
/// (stored as NULL) means a pending request never expires.
pub(super) const PENDING_EXPIRY_CHOICES: [i32; 4] = [4, 24, 48, 72];

/// Expiry given to event types created from the web form when the form does
/// not say otherwise.
const DEFAULT_PENDING_EXPIRY_HOURS: i32 = 24;

/// Deadline for a pending request created (or put back to pending) at `now`
/// for a meeting starting at `start`. `None` when the event type's requests
/// never expire.
///
/// Expiring a request is only useful while releasing its slot helps another
/// guest, i.e. until `release = start - min_notice`, the last moment the slot
/// can still be booked. So:
/// - `release` still ahead: `min(now + expiry, release)`.
/// - `release` already passed (the request was made, or rescheduled, inside
///   the notice window, where nobody else could book the slot anyway):
///   `min(now + expiry, start)`, which leaves the host until the meeting
///   instead of expiring the request minutes after it was made.
///
/// Never earlier than `now`.
pub(super) fn pending_expiry_deadline(
    now: chrono::DateTime<Utc>,
    start: chrono::DateTime<Utc>,
    expiry_hours: Option<i32>,
    min_notice_min: i32,
) -> Option<chrono::DateTime<Utc>> {
    let hours = expiry_hours.filter(|h| *h > 0)?;
    let by_expiry = now + Duration::hours(hours.into());
    let release = start - Duration::minutes(min_notice_min.max(0).into());
    let cap = if release > now { release } else { start };
    Some(by_expiry.min(cap).max(now))
}

/// Store the expiry deadline of a just-inserted booking, inside the insert's
/// transaction, computed from its event type. A booking that is not pending (or a legacy row without a UTC start)
/// gets NULL, which never expires. Best-effort: a failure only means the
/// request does not expire, which is the pre-#234 behaviour, so it is logged
/// rather than failing the booking.
/// `(pending_expires_at, pending_since)` for a booking entering pending at
/// `now` with UTC start `start_at`, from its event type's `expiry_hours` and
/// `min_notice_min`. Both `None` when requests never expire (or the start is
/// not a UTC instant). `pending_since` anchors the halfway host nudge, so a
/// request put back to pending by a reschedule is nudged halfway through its
/// new window.
pub(super) fn pending_window(
    now: chrono::DateTime<Utc>,
    start_at: &str,
    expiry_hours: Option<i32>,
    min_notice_min: i32,
) -> (Option<String>, Option<String>) {
    let deadline = chrono::DateTime::parse_from_rfc3339(start_at)
        .ok()
        .and_then(|start| {
            pending_expiry_deadline(now, start.with_timezone(&Utc), expiry_hours, min_notice_min)
        });
    match deadline {
        Some(d) => (
            Some(d.format("%Y-%m-%dT%H:%M:%SZ").to_string()),
            Some(now.format("%Y-%m-%dT%H:%M:%SZ").to_string()),
        ),
        None => (None, None),
    }
}

pub(super) async fn stamp_pending_deadline(conn: &mut sqlx::SqliteConnection, booking_id: &str) {
    let row: Option<(String, Option<i32>, i32)> = match sqlx::query_as(
        "SELECT b.start_at, et.pending_expiry_hours, et.min_notice_min
         FROM bookings b JOIN event_types et ON et.id = b.event_type_id
         WHERE b.id = ? AND b.status = 'pending' AND b.time_version = 1",
    )
    .bind(booking_id)
    .fetch_optional(&mut *conn)
    .await
    {
        Ok(row) => row,
        Err(e) => {
            tracing::warn!(booking_id, error = %e, "cannot read pending request expiry settings");
            return;
        }
    };
    let (deadline, since) = row
        .map(|(start, hours, min_notice)| pending_window(Utc::now(), &start, hours, min_notice))
        .unwrap_or_default();
    if let Err(e) = sqlx::query(
        "UPDATE bookings SET pending_expires_at = ?, pending_since = ?, pending_reminder_sent_at = NULL WHERE id = ?",
    )
    .bind(&deadline)
    .bind(&since)
    .bind(booking_id)
    .execute(&mut *conn)
    .await
    {
        tracing::warn!(booking_id, error = %e, "cannot store pending request deadline");
    }
}

/// What one pass of [`sweep_pending_requests`] did.
#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct PendingSweep {
    pub(super) expired: usize,
    pub(super) nudged: usize,
}

/// The pending-request step of the background loop (#234):
///
/// 1. A pending request past `pending_expires_at` is declined, never
///    accepted. The flip is guarded on `status = 'pending'` and on the
///    deadline, so an approval (or a reschedule that moved the deadline) that
///    lands first wins and nothing is sent. The guest gets an email (and an
///    SMS when the event type collects phones) asking them to pick another
///    time, and the host a short notice.
/// 2. A request still unanswered halfway between `pending_since` and its
///    deadline gets one host reminder with the Approve/Decline links. It is
///    deferred, not dropped, while SMTP is unconfigured.
///
/// The status flips and claims are inline; the emails and SMS they trigger
/// are spawned (see [`spawn_emails`]) so the reminders that follow in the
/// same tick are never held up by SMTP. Nothing needs cleaning up on expiry:
/// meeting links, resource reservations and CalDAV write-back only happen on
/// confirmation.
pub(super) async fn sweep_pending_requests(
    pool: &SqlitePool,
    secret_key: &[u8; 32],
    now: chrono::DateTime<Utc>,
) -> PendingSweep {
    let mut sweep = PendingSweep::default();
    let now_s = now.format("%Y-%m-%dT%H:%M:%SZ").to_string();
    let smtp_config = crate::email::load_smtp_config(pool, secret_key)
        .await
        .ok()
        .flatten();

    let due: Vec<(String,)> = sqlx::query_as(
        "SELECT id FROM bookings
         WHERE status = 'pending' AND pending_expires_at IS NOT NULL AND pending_expires_at <= ?",
    )
    .bind(&now_s)
    .fetch_all(pool)
    .await
    .unwrap_or_default();
    for (bid,) in &due {
        let flipped = sqlx::query(
            "UPDATE bookings SET status = 'declined'
             WHERE id = ? AND status = 'pending' AND pending_expires_at IS NOT NULL AND pending_expires_at <= ?",
        )
        .bind(bid)
        .bind(&now_s)
        .execute(pool)
        .await
        .map(|r| r.rows_affected())
        .unwrap_or(0);
        if flipped == 0 {
            continue;
        }
        // Declines do not store a reason (the host's optional one only rides
        // in the guest email); the log line is the record of why.
        tracing::info!(
            booking_id = %bid,
            reason = "expired: the host did not respond before the deadline",
            "pending request declined"
        );
        // Notifications go out in the background, like the booking flow's
        // (`spawn_emails`): a slow relay or a burst of expiries must not hold
        // up the reminders this loop sends next. The decline above is
        // already committed.
        let (pool, key, smtp, bid) = (pool.clone(), *secret_key, smtp_config.clone(), bid.clone());
        spawn_emails(async move {
            notify_pending_expired(&pool, &key, smtp.as_ref(), &bid).await;
        });
        sweep.expired += 1;
    }

    let Some(smtp_config) = smtp_config else {
        return sweep;
    };
    let candidates: Vec<(String, String, String)> = sqlx::query_as(
        "SELECT id, pending_since, pending_expires_at FROM bookings
         WHERE status = 'pending' AND pending_reminder_sent_at IS NULL
           AND pending_since IS NOT NULL AND pending_expires_at IS NOT NULL
           AND pending_expires_at > ?",
    )
    .bind(&now_s)
    .fetch_all(pool)
    .await
    .unwrap_or_default();
    for (bid, since, expires) in &candidates {
        let (Ok(since), Ok(expires)) = (
            chrono::DateTime::parse_from_rfc3339(since),
            chrono::DateTime::parse_from_rfc3339(expires),
        ) else {
            continue;
        };
        let halfway = since.with_timezone(&Utc) + (expires - since) / 2;
        if now < halfway {
            continue;
        }
        // Claim first so a slow SMTP send can never produce a second nudge.
        let claimed = sqlx::query(
            "UPDATE bookings SET pending_reminder_sent_at = ?
             WHERE id = ? AND status = 'pending' AND pending_reminder_sent_at IS NULL",
        )
        .bind(&now_s)
        .bind(bid)
        .execute(pool)
        .await
        .map(|r| r.rows_affected())
        .unwrap_or(0);
        if claimed == 0 {
            continue;
        }
        let (pool, smtp, bid) = (pool.clone(), smtp_config.clone(), bid.clone());
        let expires = expires.with_timezone(&Utc);
        spawn_emails(async move {
            send_pending_nudge(&pool, &smtp, &bid, expires).await;
        });
        sweep.nudged += 1;
    }
    sweep
}

/// The host side of a pending booking as (name, email, language): the
/// assigned member, every eligible member of a collective booking, or the
/// owner, i.e. whoever was asked to approve it.
pub(super) async fn pending_host_recipients(
    pool: &SqlitePool,
    uid: &str,
    owner_user_id: &str,
) -> Vec<(String, String, Option<String>)> {
    let hosts = reschedule_hosts(pool, uid, owner_user_id).await;
    let mut ids = hosts.user_ids;
    if ids.is_empty() {
        ids.push(owner_user_id.to_string());
    }
    let mut out = Vec::new();
    for id in &ids {
        let row: Option<(String, String, Option<String>)> = sqlx::query_as(
            "SELECT name, COALESCE(booking_email, email), language FROM users WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(pool)
        .await
        .unwrap_or(None);
        out.extend(row.map(|(name, email, language)| {
            (name, email, crate::email::supported_language(language))
        }));
    }
    out
}

/// Whether a pending booking was confirmed once already, i.e. this request is
/// a guest reschedule awaiting approval rather than a new booking. Read
/// before the approval flips the status, which stamps `first_confirmed_at`.
/// Its additional attendees were emailed when it was first confirmed, so
/// approving the new time sends them the move rather than "added" (#266,
/// #267), and the guest is texted "moved".
pub(super) async fn was_confirmed_before(pool: &SqlitePool, booking_id: &str) -> bool {
    sqlx::query_scalar::<_, bool>(
        "SELECT first_confirmed_at IS NOT NULL FROM bookings WHERE id = ?",
    )
    .bind(booking_id)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten()
    .unwrap_or(false)
}

/// Public booking page of an event type, for the "pick another time" link in
/// the expiry email. Only for enabled public event types: internal and
/// private ones need a per-guest invite the guest no longer has. A public
/// event type on a private team is only reachable with the team's invite
/// token (`?invite=`, as checked by the team slot page), so the link carries
/// it, and there is no link when the team has none.
pub(super) async fn event_type_rebook_url(pool: &SqlitePool, et_id: &str) -> Option<String> {
    let base = crate::settings::base_url()?;
    #[allow(clippy::type_complexity)]
    let (slug, visibility, enabled, team_slug, team_visibility, team_invite, username): (
        String,
        String,
        i32,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
    ) = sqlx::query_as(
        "SELECT et.slug, et.visibility, et.enabled, t.slug, t.visibility, t.invite_token, u.username
         FROM event_types et
         JOIN accounts a ON a.id = et.account_id
         LEFT JOIN users u ON u.id = a.user_id
         LEFT JOIN teams t ON t.id = et.team_id
         WHERE et.id = ?",
    )
    .bind(et_id)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten()?;
    if visibility != "public" || enabled == 0 {
        return None;
    }
    let base = base.trim_end_matches('/');
    match (team_slug, username) {
        (Some(team), _) => {
            let url = format!("{}/team/{}/{}", base, team, slug);
            if team_visibility.as_deref() == Some("private") {
                let token = team_invite.filter(|t| !t.is_empty())?;
                Some(format!("{}?invite={}", url, urlencoding::encode(&token)))
            } else {
                Some(url)
            }
        }
        (None, Some(user)) => Some(format!("{}/u/{}/{}", base, user, slug)),
        (None, None) => None,
    }
}

type PendingRow = (
    String,
    String,
    String,
    String,
    String,
    String,
    String,
    Option<String>,
    Option<String>,
    String,
    String,
    String,
    Option<String>,
);

async fn load_pending_row(pool: &SqlitePool, booking_id: &str) -> Option<PendingRow> {
    sqlx::query_as(
        "SELECT b.uid, b.guest_name, b.guest_email,
                CASE b.time_version WHEN 1 THEN b.start_at ELSE rtrim(b.start_at, 'Z') END AS start_at,
                CASE b.time_version WHEN 1 THEN b.end_at ELSE rtrim(b.end_at, 'Z') END AS end_at,
                et.title, COALESCE(b.guest_timezone, 'UTC'), b.language, b.guest_phone,
                et.sms_phone_mode, et.id, a.user_id, b.confirm_token
         FROM bookings b
         JOIN event_types et ON et.id = b.event_type_id
         JOIN accounts a ON a.id = et.account_id
         WHERE b.id = ?",
    )
    .bind(booking_id)
    .fetch_optional(pool)
    .await
    .unwrap_or(None)
}

/// Emails and SMS for a request the sweep just declined as expired.
async fn notify_pending_expired(
    pool: &SqlitePool,
    secret_key: &[u8; 32],
    smtp_config: Option<&crate::email::SmtpConfig>,
    booking_id: &str,
) {
    let Some((
        uid,
        guest_name,
        guest_email,
        start_at,
        end_at,
        event_title,
        guest_timezone,
        guest_language,
        guest_phone,
        sms_phone_mode,
        et_id,
        owner_user_id,
        _confirm_token,
    )) = load_pending_row(pool, booking_id).await
    else {
        return;
    };
    let stored_tz = get_host_tz(pool, &et_id).await;
    let guest_tz = guest_timezone.parse::<Tz>().unwrap_or(Tz::UTC);
    let (date, start_time, end_time) =
        booking_strings_in_guest_tz(&start_at, &end_at, stored_tz, guest_tz);
    let hosts = pending_host_recipients(pool, &uid, &owner_user_id).await;
    // The guest's email names the host the way every other guest email does
    // (joined roster and ICS organizer for collective bookings), so its
    // sender and Reply-To match the confirmation they would have got (#221).
    let (host_name, host_email) = match booking_host_identity(pool, &uid).await {
        Some((name, email, _)) => (name, email),
        None => hosts
            .first()
            .map(|(n, e, _)| (n.clone(), e.clone()))
            .unwrap_or_default(),
    };
    let details = crate::email::CancellationDetails {
        utc_times: crate::booking_time::ics_times(&start_at, &end_at),
        event_title,
        date,
        start_time,
        end_time,
        guest_name,
        guest_email,
        guest_timezone,
        host_name,
        host_email,
        uid: uid.clone(),
        reason: None,
        cancelled_by_host: true,
        guest_language,
        host_language: None,
        host_timezone: stored_tz.name().to_string(),
        sequence: booking_ics_sequence(pool, &uid).await,
        ..Default::default()
    };
    // An expired reschedule of a confirmed booking withdraws the invite the
    // guest and the attendees hold (#267).
    let details = with_invite_holders(pool, details, true).await;

    if let Some(smtp_config) = smtp_config {
        let rebook_url = event_type_rebook_url(pool, &et_id).await;
        log_email_result(
            crate::email::send_guest_pending_expired(smtp_config, &details, rebook_url.as_deref())
                .await,
            "guest_pending_expired",
            &uid,
        );
        log_email_result(
            crate::email::send_guest_attendee_cancellations(smtp_config, &details).await,
            "attendee_cancellation",
            &uid,
        );
        for (name, email, language) in &hosts {
            let host_details = crate::email::CancellationDetails {
                host_name: name.clone(),
                host_email: email.clone(),
                host_language: language.clone(),
                ..details.clone()
            };
            log_email_result(
                crate::email::send_host_pending_expired(smtp_config, &host_details).await,
                "host_pending_expired",
                &uid,
            );
        }
    }

    if let Some(ctx) = crate::sms::SmsContext::for_cancellation(
        &details,
        guest_phone.as_deref(),
        crate::sms::collects_phone(&sms_phone_mode),
    ) {
        crate::sms::notify_guest(pool, secret_key, crate::sms::SmsEvent::Expired, ctx).await;
    }
}

/// The one host reminder for a request still unanswered halfway to its
/// deadline: the approval request again, with the deadline spelled out.
async fn send_pending_nudge(
    pool: &SqlitePool,
    smtp_config: &crate::email::SmtpConfig,
    booking_id: &str,
    expires: chrono::DateTime<Utc>,
) {
    let Some((
        uid,
        guest_name,
        guest_email,
        start_at,
        end_at,
        event_title,
        guest_timezone,
        guest_language,
        _guest_phone,
        _sms_phone_mode,
        et_id,
        owner_user_id,
        confirm_token,
    )) = load_pending_row(pool, booking_id).await
    else {
        return;
    };
    let stored_tz = get_host_tz(pool, &et_id).await;
    let guest_tz = guest_timezone.parse::<Tz>().unwrap_or(Tz::UTC);
    let (date, start_time, end_time) =
        booking_strings_in_guest_tz(&start_at, &end_at, stored_tz, guest_tz);
    let base_url = crate::settings::base_url();
    // Read like the approval request it repeats, so the two cannot differ.
    let (notes, guest_phone) = booking_guest_input(pool, &uid).await;
    let details = crate::email::BookingDetails {
        utc_times: crate::booking_time::ics_times(&start_at, &end_at),
        event_title,
        date,
        start_time,
        end_time,
        guest_name,
        guest_email,
        guest_timezone,
        uid: uid.clone(),
        notes,
        guest_phone,
        guest_language,
        host_timezone: stored_tz.name().to_string(),
        resource_name: booking_resource_label(pool, &uid).await,
        ..Default::default()
    };
    for (name, email, language) in pending_host_recipients(pool, &uid, &owner_user_id).await {
        let host_details = crate::email::BookingDetails {
            host_name: name,
            host_email: email,
            host_language: language,
            ..details.clone()
        };
        log_email_result(
            crate::email::send_host_approval_reminder(
                smtp_config,
                &host_details,
                confirm_token.as_deref(),
                base_url.as_deref(),
                expires,
            )
            .await,
            "host_approval_reminder",
            &uid,
        );
    }
}

/// The pending-request section of the event type form (#234): whether a
/// pending request blocks the slot, and how long it may wait (`None` = never).
/// The one place that maps between stored values, form values and the
/// template, so the defaults and the "never" spelling live nowhere else.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct PendingSettings {
    pub(super) blocks_slot: bool,
    pub(super) expiry_hours: Option<i32>,
}

impl PendingSettings {
    /// What a new event type gets from the web form.
    pub(super) const DEFAULT: Self = Self {
        blocks_slot: true,
        expiry_hours: Some(DEFAULT_PENDING_EXPIRY_HOURS),
    };

    /// Longest expiry accepted from the form, one year. Anything positive up
    /// to that is kept, so a stored value outside [`PENDING_EXPIRY_CHOICES`]
    /// survives a save instead of being reset to "never".
    const MAX_EXPIRY_HOURS: i32 = 24 * 365;

    /// The select value for an expiry: hours as a number, or "never".
    pub(super) fn expiry_value(expiry_hours: Option<i32>) -> String {
        expiry_hours
            .map(|h| h.to_string())
            .unwrap_or_else(|| "never".to_string())
    }

    /// Parse a select value back. "never", and anything that is not a
    /// positive hour count within [`Self::MAX_EXPIRY_HOURS`], is `None`.
    pub(super) fn parse_expiry(raw: &str) -> Option<i32> {
        raw.trim()
            .parse::<i32>()
            .ok()
            .filter(|h| (1..=Self::MAX_EXPIRY_HOURS).contains(h))
    }

    /// The settings a submitted form carries, or `None` when the form did not
    /// include the section at all (no `pending_expiry_hours` field, e.g. a
    /// script posting the old field set).
    pub(super) fn from_form(form: &EventTypeForm) -> Option<Self> {
        let raw = form.pending_expiry_hours.as_deref()?;
        Some(Self {
            blocks_slot: form.pending_blocks_slot.as_deref() == Some("on"),
            expiry_hours: Self::parse_expiry(raw),
        })
    }

    pub(super) async fn load(pool: &SqlitePool, et_id: &str) -> Self {
        let row: Option<(i32, Option<i32>)> = sqlx::query_as(
            "SELECT pending_blocks_slot, pending_expiry_hours FROM event_types WHERE id = ?",
        )
        .bind(et_id)
        .fetch_optional(pool)
        .await
        .unwrap_or(None);
        match row {
            Some((blocks, hours)) => Self {
                blocks_slot: blocks != 0,
                expiry_hours: hours,
            },
            None => Self::DEFAULT,
        }
    }

    /// Template context: the checkbox state, the selected value, and the
    /// options to render. A stored expiry that is not one of the offered
    /// choices is listed too, so it stays selected.
    pub(super) fn context(&self) -> minijinja::Value {
        let mut hours: Vec<i32> = PENDING_EXPIRY_CHOICES.to_vec();
        if let Some(h) = self.expiry_hours {
            if !hours.contains(&h) {
                hours.push(h);
                hours.sort_unstable();
            }
        }
        let mut options = vec![context! { value => "never", hours => () }];
        options.extend(
            hours
                .into_iter()
                .map(|h| context! { value => h.to_string(), hours => h }),
        );
        context! {
            blocks_slot => self.blocks_slot,
            selected => Self::expiry_value(self.expiry_hours),
            options => options,
        }
    }
}

/// Persist the pending-request settings from the event type form. A
/// submission without the section keeps the stored values on edit and gets
/// [`PendingSettings::DEFAULT`] on create.
pub(super) async fn save_pending_settings(
    pool: &SqlitePool,
    et_id: &str,
    form: &EventTypeForm,
    creating: bool,
) {
    let settings = match PendingSettings::from_form(form) {
        Some(settings) => settings,
        None if creating => PendingSettings::DEFAULT,
        None => return,
    };
    if let Err(e) = sqlx::query(
        "UPDATE event_types SET pending_blocks_slot = ?, pending_expiry_hours = ? WHERE id = ?",
    )
    .bind(settings.blocks_slot as i32)
    .bind(settings.expiry_hours)
    .bind(et_id)
    .execute(pool)
    .await
    {
        tracing::warn!(event_type_id = %et_id, error = %e, "cannot save pending request settings");
    }
}
