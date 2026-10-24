//! Email dispatch, new-booking notifications, the reminder loop, watchers and SMS.

use super::*;

pub(super) type DueReminder = (
    String,
    String,
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
    Option<String>,
    Option<String>,
    Option<String>,
);

pub(super) async fn due_reminders(
    pool: &SqlitePool,
    now: chrono::DateTime<Utc>,
) -> Vec<DueReminder> {
    sqlx::query_as("SELECT b.id, b.guest_name, b.guest_email, b.guest_timezone, CASE b.time_version WHEN 1 THEN b.start_at ELSE rtrim(b.start_at, 'Z') END AS start_at, CASE b.time_version WHEN 1 THEN b.end_at ELSE rtrim(b.end_at, 'Z') END AS end_at, et.title, u.name, COALESCE(u.booking_email, u.email), COALESCE(NULLIF(b.meeting_url, ''), et.location_value), b.cancel_token, b.uid, b.language, u.language, COALESCE(NULLIF(et.timezone, ''), u.timezone)
             FROM bookings b
             JOIN event_types et ON et.id = b.event_type_id
             JOIN accounts a ON a.id = et.account_id
             JOIN users u ON u.id = COALESCE(b.assigned_user_id, a.user_id)
             WHERE b.status = 'confirmed'
               AND b.reminder_sent_at IS NULL
               AND et.reminder_minutes IS NOT NULL
               AND et.reminder_minutes > 0
               AND datetime(b.start_at, '-' || et.reminder_minutes || ' minutes') <= datetime(?1)
               AND datetime(b.start_at) > datetime(?1)",).bind(now.to_rfc3339()).fetch_all(pool).await.unwrap_or_default()
}

/// Log a failed email send with enough context to find the booking again.
/// Email is best-effort everywhere in the booking flow, but a failure that
/// leaves no trace is how #236 went unnoticed: `send_email` logs the SMTP
/// error, this adds which message it was and for which booking. `booking` is
/// the booking's iCal uid, or its row id where the uid is not at hand.
pub(super) fn log_email_result(result: anyhow::Result<()>, email: &'static str, booking: &str) {
    if let Err(e) = result {
        tracing::warn!(email, booking = %booking, error = format!("{e:#}"), "email send failed");
    }
}

/// Send booking-flow emails after the HTTP response instead of before it.
/// The booking (or its cancellation, approval, ...) is already committed when
/// this is called, so the guest must not wait on SMTP to see the result: a
/// slow or dead relay used to stall the request until the reverse proxy gave
/// up with a 504, and guests retried a booking that existed (#236). The
/// future owns everything it needs; failures are logged via
/// `log_email_result` inside it.
pub(crate) fn spawn_emails<F>(task: F)
where
    F: std::future::Future<Output = ()> + Send + 'static,
{
    EMAIL_TASKS.spawn(task);
}

/// Background email sends still in flight, so a graceful shutdown can wait
/// for them instead of dropping them with the runtime (see `drain_emails`).
static EMAIL_TASKS: TaskTracker = TaskTracker::new();

/// Counts spawned tasks and wakes waiters when the count drops to zero.
pub(super) struct TaskTracker {
    pub(super) pending: std::sync::atomic::AtomicUsize,
    drained: tokio::sync::Notify,
}

impl TaskTracker {
    pub(super) const fn new() -> Self {
        Self {
            pending: std::sync::atomic::AtomicUsize::new(0),
            drained: tokio::sync::Notify::const_new(),
        }
    }

    pub(super) fn spawn<F>(&'static self, task: F)
    where
        F: std::future::Future<Output = ()> + Send + 'static,
    {
        use std::sync::atomic::Ordering;
        // Decrement on drop, so a task that panics still counts as done.
        struct Done(&'static TaskTracker);
        impl Drop for Done {
            fn drop(&mut self) {
                if self.0.pending.fetch_sub(1, Ordering::AcqRel) == 1 {
                    self.0.drained.notify_waiters();
                }
            }
        }
        self.pending.fetch_add(1, Ordering::AcqRel);
        let done = Done(self);
        tokio::spawn(async move {
            let _done = done;
            task.await;
        });
    }

    /// Wait until every spawned task has finished, or `timeout` passes.
    /// Returns how many were still running.
    pub(super) async fn drain(&self, timeout: std::time::Duration) -> usize {
        use std::sync::atomic::Ordering;
        let deadline = tokio::time::Instant::now() + timeout;
        loop {
            let notified = self.drained.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            let left = self.pending.load(Ordering::Acquire);
            if left == 0 {
                return 0;
            }
            if tokio::time::timeout_at(deadline, notified).await.is_err() {
                return self.pending.load(Ordering::Acquire);
            }
        }
    }
}

/// Called once the HTTP server has stopped: give queued booking emails time
/// to go out. Each send is bounded by the SMTP deadline, so this cannot hang
/// past `timeout`.
pub async fn drain_emails(timeout: std::time::Duration) {
    if EMAIL_TASKS
        .pending
        .load(std::sync::atomic::Ordering::Acquire)
        == 0
    {
        return;
    }
    tracing::info!("waiting for queued emails before exiting");
    let left = EMAIL_TASKS.drain(timeout).await;
    if left > 0 {
        tracing::warn!(left, "shutdown: queued emails still sending were dropped");
    }
}

/// The guest-side emails of an approval, shared by the token and dashboard
/// paths. A new request gets the confirmation and its attendees "you've
/// been added". A reschedule of a booking confirmed before
/// (`reschedule_approval`, see `was_confirmed_before`) gets "your new time
/// is confirmed" with the moved invite, matching the "moved" SMS, and its
/// attendees the moved invite (#266, #267). `notice_min` is the event
/// type's (cancel, reschedule) notice window.
pub(super) async fn send_approval_guest_emails(
    smtp_config: &crate::email::SmtpConfig,
    details: &crate::email::BookingDetails,
    cancel_url: Option<&str>,
    reschedule_url: Option<&str>,
    notice_min: (Option<i32>, Option<i32>),
    reschedule_approval: bool,
) {
    let (cancel_notice_min, reschedule_notice_min) = notice_min;
    if reschedule_approval {
        log_email_result(
            crate::email::send_guest_reschedule_approved(
                smtp_config,
                details,
                cancel_url,
                reschedule_url,
                cancel_notice_min,
                reschedule_notice_min,
            )
            .await,
            "guest_reschedule_approved",
            &details.uid,
        );
        log_email_result(
            crate::email::send_guest_attendee_reschedule(smtp_config, details, None).await,
            "attendee_reschedule",
            &details.uid,
        );
    } else {
        log_email_result(
            crate::email::send_guest_confirmation_ex(
                smtp_config,
                details,
                cancel_url,
                reschedule_url,
                cancel_notice_min,
                reschedule_notice_min,
            )
            .await,
            "guest_confirmation",
            &details.uid,
        );
        log_email_result(
            crate::email::send_guest_attendee_confirmations(smtp_config, details).await,
            "attendee_confirmation",
            &details.uid,
        );
    }
}

/// Everything the new-booking emails need, owned so it can outlive the
/// request. Shared by the four guest booking handlers (personal, user-scoped,
/// dynamic group, team), which used to repeat the same send sequence inline.
pub(super) struct NewBookingEmails {
    pub(super) smtp_config: crate::email::SmtpConfig,
    pub(super) details: crate::email::BookingDetails,
    /// Host-side recipients as (name, email, language). Collective team
    /// bookings list every member, each in their own language; everything
    /// else lists the single host from `details`.
    pub(super) host_recipients: Vec<(String, String, Option<String>)>,
    pub(super) booking_id: String,
    pub(super) needs_approval: bool,
    pub(super) confirm_token: Option<String>,
    pub(super) base_url: Option<String>,
    pub(super) guest_cancel_url: Option<String>,
    pub(super) guest_reschedule_url: Option<String>,
    pub(super) cancel_notice_min: Option<i32>,
    pub(super) reschedule_notice_min: Option<i32>,
}

/// Send the guest and host emails for a just-committed booking in the
/// background (see `spawn_emails`).
pub(super) fn send_new_booking_emails(m: NewBookingEmails) {
    spawn_emails(async move {
        let uid = m.details.uid.as_str();
        let host_details =
            |name: &str, email: &str, language: &Option<String>| crate::email::BookingDetails {
                host_name: name.to_string(),
                host_email: email.to_string(),
                host_language: language.clone(),
                ..m.details.clone()
            };
        if m.needs_approval {
            for (name, email, language) in &m.host_recipients {
                log_email_result(
                    crate::email::send_host_approval_request(
                        &m.smtp_config,
                        &host_details(name, email, language),
                        &m.booking_id,
                        m.confirm_token.as_deref(),
                        m.base_url.as_deref(),
                    )
                    .await,
                    "host_approval_request",
                    uid,
                );
            }
            log_email_result(
                crate::email::send_guest_pending_notice_ex(
                    &m.smtp_config,
                    &m.details,
                    m.guest_cancel_url.as_deref(),
                    m.guest_reschedule_url.as_deref(),
                )
                .await,
                "guest_pending_notice",
                uid,
            );
        } else {
            log_email_result(
                crate::email::send_guest_confirmation_ex(
                    &m.smtp_config,
                    &m.details,
                    m.guest_cancel_url.as_deref(),
                    m.guest_reschedule_url.as_deref(),
                    m.cancel_notice_min,
                    m.reschedule_notice_min,
                )
                .await,
                "guest_confirmation",
                uid,
            );
            log_email_result(
                crate::email::send_guest_attendee_confirmations(&m.smtp_config, &m.details).await,
                "attendee_confirmation",
                uid,
            );
            for (name, email, language) in &m.host_recipients {
                log_email_result(
                    crate::email::send_host_notification(
                        &m.smtp_config,
                        &host_details(name, email, language),
                    )
                    .await,
                    "host_notification",
                    uid,
                );
            }
        }
    });
}

/// Background task that sends booking reminders on a 60-second tick. Also
/// piggybacks an hourly sweep of expired sessions so the `sessions` table
/// doesn't accumulate dead rows indefinitely.
pub async fn run_reminder_loop(pool: SqlitePool, secret_key: [u8; 32]) {
    let mut last_session_cleanup = std::time::Instant::now();
    loop {
        tokio::time::sleep(std::time::Duration::from_secs(60)).await;

        // Hourly: prune sessions whose `expires_at` has passed.
        if last_session_cleanup.elapsed() >= std::time::Duration::from_secs(3600) {
            match crate::auth::cleanup_expired_sessions(&pool).await {
                Ok(removed) if removed > 0 => {
                    tracing::info!(removed, "expired sessions pruned");
                }
                Ok(_) => {}
                Err(e) => tracing::warn!(error = %e, "session cleanup failed"),
            }
            last_session_cleanup = std::time::Instant::now();
            if let Err(e) = crate::oauth::cleanup(&pool).await {
                tracing::warn!(error = %e, "OAuth cleanup failed");
            }
        }

        // Pending requests: decline the ones past their deadline and nudge
        // hosts halfway there (#234).
        sweep_pending_requests(&pool, &secret_key, Utc::now()).await;

        // Reminders run in their own function: their early exits (nothing
        // due, no channel configured) used to `continue` this loop and skip
        // the background sync below on almost every tick.
        send_due_reminders(pool.clone(), secret_key).await;

        // Calendar write-backs that failed earlier and are due another try
        // (#162).
        writeback::retry_failed_writebacks(&pool, &secret_key, Utc::now()).await;

        // Background sync: pick the stalest enabled source and sync it.
        // With ctag + sync-token this is very cheap for unchanged calendars.
        let stalest: Option<(String,)> = sqlx::query_as(
            "SELECT cs.id
             FROM caldav_sources cs
             WHERE cs.enabled = 1
             ORDER BY MAX(COALESCE(cs.last_synced, '2000-01-01'), COALESCE(cs.last_sync_error_at, '2000-01-01')) ASC
             LIMIT 1",
        )
        .fetch_optional(&pool)
        .await
        .unwrap_or(None);

        if let Some((source_id,)) = stalest {
            crate::commands::sync::sync_source_by_id(&pool, &secret_key, &source_id).await;
            tracing::debug!(source_id = %source_id, "background sync completed");
        }
    }
}

/// One pass of the reminder batch: email and SMS reminders for confirmed
/// bookings whose reminder time has come.
async fn send_due_reminders(pool: SqlitePool, secret_key: [u8; 32]) {
    // Find bookings that need a reminder:
    // - status is confirmed
    // - reminder not yet sent
    // - event type has reminder_minutes set (> 0)
    // - start_at minus reminder_minutes <= now
    // - start_at > now (don't remind for past bookings)
    // `host_timezone` resolves like `get_host_tz`: prefer the explicit
    // event-type tz, fall back to the host user's tz. NULL falls through
    // to UTC at parse time.
    let due = due_reminders(&pool, Utc::now()).await;

    let base_url = crate::settings::base_url();

    if due.is_empty() {
        return;
    }

    // SMS is delivered independently of email, so a deployment with SMS
    // but no SMTP still gets its reminders out. Skip the batch only when
    // neither channel can deliver anything.
    let smtp_config = crate::email::load_smtp_config(&pool, &secret_key)
        .await
        .ok()
        .flatten();
    let sms_configured = crate::sms::load_config(&pool, &secret_key)
        .await
        .ok()
        .flatten()
        .is_some();
    if smtp_config.is_none() && !sms_configured {
        return;
    }

    for (
        bid,
        guest_name,
        guest_email,
        guest_timezone,
        start_at,
        end_at,
        event_title,
        host_name,
        host_email,
        location_value,
        cancel_token,
        uid,
        guest_language,
        host_language,
        host_timezone,
    ) in &due
    {
        // Legacy times use the event timezone; version 1 timestamps are UTC. Convert
        // to the guest's tz so `BookingDetails` carries guest-local wall-clock —
        // matches the contract used by cancel/confirm handlers, and lets
        // `host_time_display` convert back into the host's tz correctly for
        // the host reminder.
        let stored_tz_str = host_timezone.as_deref().unwrap_or("UTC");
        let stored_tz = stored_tz_str.parse::<Tz>().unwrap_or(Tz::UTC);
        let guest_tz_parsed = guest_timezone.parse::<Tz>().unwrap_or(Tz::UTC);
        let (date, start_time, end_time) =
            booking_strings_in_guest_tz(start_at, end_at, stored_tz, guest_tz_parsed);

        let location = location_value.as_ref().filter(|v| !v.is_empty()).cloned();

        let (guest_notes, guest_phone_shown) = booking_guest_input(&pool, uid).await;
        let details = crate::email::BookingDetails {
            utc_times: crate::booking_time::ics_times(start_at, end_at),
            event_title: event_title.clone(),
            date,
            start_time,
            end_time,
            guest_name: guest_name.clone(),
            guest_email: guest_email.clone(),
            guest_timezone: guest_timezone.clone(),
            host_name: host_name.clone(),
            host_email: host_email.clone(),
            uid: uid.clone(),
            notes: guest_notes,
            guest_phone: guest_phone_shown,
            location,
            reminder_minutes: None,
            additional_attendees: vec![],
            guest_language: guest_language.clone(),
            host_language: crate::email::supported_language(host_language.clone()),
            host_timezone: stored_tz_str.to_string(),
            resource_name: booking_resource_label(&pool, uid).await,
            sequence: booking_ics_sequence(&pool, uid).await,
            attendee_languages: Vec::new(),
        };

        let guest_cancel_url = cancel_token.as_ref().and_then(|t| {
            base_url
                .as_ref()
                .map(|base| format!("{}/booking/cancel/{}", base.trim_end_matches('/'), t))
        });

        // Tracks whether any channel could actually carry this reminder.
        // Stamping `reminder_sent_at` when neither could would silently
        // swallow it, so an instance whose SMTP is missing keeps the
        // booking pending until SMTP comes back, as it did before SMS.
        let mut notified = false;

        if let Some(smtp_config) = &smtp_config {
            // The guest's reminder names the host as their confirmation
            // did: for a collective booking the joined roster, with the ICS
            // organizer as sender and Reply-To (#221), not the owner `due`
            // joined on.
            let guest_details = match booking_host_identity(&pool, uid).await {
                Some((host_name, host_email, _)) => crate::email::BookingDetails {
                    host_name,
                    host_email,
                    ..details.clone()
                },
                None => details.clone(),
            };
            // Inline on purpose: this loop is already off the request
            // path, and `send_email` bounds each attempt.
            log_email_result(
                crate::email::send_guest_reminder(
                    smtp_config,
                    &guest_details,
                    guest_cancel_url.as_deref(),
                )
                .await,
                "guest_reminder",
                uid,
            );
            log_email_result(
                crate::email::send_host_reminder(smtp_config, &details).await,
                "host_reminder",
                uid,
            );
            notified = true;
        }

        // Reminder SMS to the guest. `details.guest_phone` is already the
        // number `booking_guest_input` keeps only when the event type
        // collects one, so it needs no second lookup (#266).
        if sms_configured {
            if let Some(ctx) =
                crate::sms::SmsContext::for_booking(&details, details.guest_phone.as_deref(), true)
            {
                crate::sms::notify_guest(&pool, &secret_key, crate::sms::SmsEvent::Reminder, ctx)
                    .await;
                notified = true;
            }
        }

        if !notified {
            tracing::debug!(
                booking_id = %bid,
                "reminder deferred: no channel can deliver it yet"
            );
            continue;
        }

        // Mark reminder as sent
        let _ = sqlx::query("UPDATE bookings SET reminder_sent_at = datetime('now') WHERE id = ?")
            .bind(bid)
            .execute(&pool)
            .await;

        tracing::info!(booking_id = %bid, "reminder sent");
    }
}

/// Confirmation SMS for a freshly submitted booking.
///
/// Bookings that need approval stay silent here: the guest hears from us when
/// the host approves, matching the email flow. `date` and `start_time` are the
/// guest's own submitted values, so the SMS shows the time the guest picked in
/// the timezone they picked it in.
#[allow(clippy::too_many_arguments)]
pub(super) async fn notify_new_booking_sms(
    state: &AppState,
    sms_enabled: bool,
    phone: Option<&str>,
    needs_approval: bool,
    event_title: &str,
    date: &str,
    start_time: &str,
    timezone: &str,
    lang: &str,
) {
    if needs_approval || !sms_enabled {
        return;
    }
    let Some(phone) = phone.map(str::trim).filter(|p| !p.is_empty()) else {
        return;
    };

    crate::sms::notify_guest(
        &state.pool,
        &state.secret_key,
        crate::sms::SmsEvent::Confirmed,
        crate::sms::SmsContext {
            phone,
            event_title,
            date,
            start_time,
            timezone,
            lang: Some(lang),
        },
    )
    .await;
}

// --- Booking watchers ---

/// Notify watcher team members that a booking is available to claim.
/// Generates a claim token per watcher member and sends notification emails.
pub(super) async fn notify_watchers(
    pool: &SqlitePool,
    key: &[u8; 32],
    booking_id: &str,
    event_type_id: &str,
    assigned_to_name: &str,
    details: &crate::email::BookingDetails,
) {
    // Find watcher teams for this event type
    let watcher_team_ids: Vec<(String,)> =
        sqlx::query_as("SELECT team_id FROM event_type_watchers WHERE event_type_id = ?")
            .bind(event_type_id)
            .fetch_all(pool)
            .await
            .unwrap_or_default();

    if watcher_team_ids.is_empty() {
        return;
    }

    let base_url = match crate::settings::base_url() {
        Some(u) => u,
        None => {
            tracing::warn!("base URL not configured (CALRS_BASE_URL or admin System settings), skipping watcher notifications");
            return;
        }
    };

    let smtp_config = match crate::email::load_smtp_config(pool, key).await {
        Ok(Some(c)) => c,
        _ => {
            tracing::warn!("SMTP not configured, skipping watcher notifications");
            return;
        }
    };

    // Claim tokens are written here, in the request; the emails carrying them
    // go out in the background so a slow relay cannot stall the booking.
    // Each watcher reads the offer in their own saved language (#209).
    let mut outgoing: Vec<(String, String, Option<String>, String)> = Vec::new();

    for (team_id,) in &watcher_team_ids {
        // Get all members of the watcher team
        let members: Vec<(String, String, String, Option<String>)> = sqlx::query_as(
            "SELECT u.id, u.name, COALESCE(u.booking_email, u.email), u.language \
             FROM users u JOIN team_members tm ON tm.user_id = u.id \
             WHERE tm.team_id = ? AND u.enabled = 1",
        )
        .bind(team_id)
        .fetch_all(pool)
        .await
        .unwrap_or_default();

        for (user_id, user_name, user_email, user_language) in &members {
            let token = uuid::Uuid::new_v4().to_string();
            let token_id = uuid::Uuid::new_v4().to_string();

            // Insert claim token with 7-day expiry
            let _ = sqlx::query(
                "INSERT INTO booking_claim_tokens (id, booking_id, user_id, token, expires_at) \
                 VALUES (?, ?, ?, ?, datetime('now', '+7 days'))",
            )
            .bind(&token_id)
            .bind(booking_id)
            .bind(user_id)
            .bind(&token)
            .execute(pool)
            .await;

            let claim_url = format!(
                "{}/booking/claim/{}?token={}",
                base_url.trim_end_matches('/'),
                booking_id,
                token
            );

            outgoing.push((
                user_name.clone(),
                user_email.clone(),
                user_language.clone(),
                claim_url,
            ));
        }
    }

    tracing::info!(
        booking_id = %booking_id,
        event_type_id = %event_type_id,
        recipients = outgoing.len(),
        "watcher claim notifications queued"
    );

    let details = details.clone();
    let assigned_to_name = assigned_to_name.to_string();
    spawn_emails(async move {
        for (user_name, user_email, user_language, claim_url) in &outgoing {
            log_email_result(
                crate::email::send_watcher_claim_notification(
                    &smtp_config,
                    &details,
                    user_name,
                    user_email,
                    user_language.as_deref(),
                    &assigned_to_name,
                    claim_url,
                )
                .await,
                "watcher_claim_notification",
                &details.uid,
            );
        }
    });
}
