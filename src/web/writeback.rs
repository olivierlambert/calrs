//! Calendar write-back failures: record, retry (#162).
//!
//! A confirmed booking whose event did not land on the host's calendar stays
//! confirmed. calrs's own `bookings` table already blocks the slot, and the
//! guest already has the invitation; refusing the booking after the fact
//! would leave the guest with an invite to a meeting that no longer exists.
//! What was missing is the signal: the failure is recorded on the booking,
//! retried from the background loop, and shown to the host.
//!
//! Every write-back goes through [`push_and_record`], whether it comes from a
//! new booking, an approval, a reschedule, a claim or a retry, so the
//! bookkeeping lives in one place.

use chrono::{DateTime, Utc};
use chrono_tz::Tz;
use sqlx::SqlitePool;

/// Minutes to wait before the next attempt, indexed by the number of failed
/// attempts so far minus one. The initial push is attempt 1, so a booking
/// gets up to `RETRY_DELAYS_MIN.len()` retries spread over about 16 hours
/// (5 min, 15 min, 30 min, 1 h, 2 h, 4 h, 8 h) and then stops: a calendar
/// still refusing the event by then needs a human, not another PUT.
pub(super) const RETRY_DELAYS_MIN: [i64; 7] = [5, 15, 30, 60, 120, 240, 480];

/// Total attempts, the initial push included.
pub(super) const MAX_ATTEMPTS: i64 = RETRY_DELAYS_MIN.len() as i64 + 1;

/// Whether this push is the first one for the booking's current state (a
/// new booking, an approval, a reschedule, a claim) or a background retry.
/// A fresh push restarts the retry schedule; a retry continues it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PushKind {
    Fresh,
    Retry,
}

/// SQLite-comparable UTC timestamp, the format `datetime('now')` produces.
fn stamp(t: DateTime<Utc>) -> String {
    t.format("%Y-%m-%d %H:%M:%S").to_string()
}

/// When to try again after `attempts` failed attempts, `None` once they are
/// used up.
pub(super) fn next_attempt_at(attempts: i64, now: DateTime<Utc>) -> Option<DateTime<Utc>> {
    let idx = usize::try_from(attempts.checked_sub(1)?).ok()?;
    RETRY_DELAYS_MIN
        .get(idx)
        .map(|m| now + chrono::Duration::minutes(*m))
}

/// The error summary stored on the booking and shown on the dashboard. Long
/// server bodies are cut: the host needs the gist, the log has the rest.
fn summarize(failures: &[(String, String)]) -> String {
    let joined = failures
        .iter()
        .map(|(_, msg)| msg.as_str())
        .collect::<Vec<_>>()
        .join("; ");
    if joined.chars().count() > 500 {
        let cut: String = joined.chars().take(500).collect();
        format!("{cut}...")
    } else {
        joined
    }
}

/// Push a booking to every calendar it belongs to (see
/// `booking_write_targets`) and record the outcome on the booking. Returns
/// the failures as (user id, message), empty on success.
pub(super) async fn push_and_record(
    pool: &SqlitePool,
    key: &[u8; 32],
    user_id: &str,
    booking_uid: &str,
    details: &crate::email::BookingDetails,
    kind: PushKind,
) -> Vec<(String, String)> {
    let mut failures = Vec::new();
    let targets = super::booking_write_targets(pool, booking_uid, user_id).await;
    for target in &targets {
        // The only host-language text in the event is the phone label, and
        // each copy belongs to its target: every collective member reads it
        // in their own language, and a caller that left `host_language`
        // unset (a claim) no longer falls back to English (#266).
        let localized;
        let details = if details.guest_phone.is_some() {
            localized = crate::email::BookingDetails {
                host_language: crate::email::user_language(pool, target).await,
                ..details.clone()
            };
            &localized
        } else {
            details
        };
        let owner = super::CopyOwner::Host;
        for msg in
            super::caldav_push_booking_for_user(pool, key, target, booking_uid, details, owner, &[])
                .await
        {
            failures.push((target.clone(), msg));
        }
    }
    if kind == PushKind::Fresh {
        push_claimant_copy(pool, key, booking_uid, details, &targets).await;
        // The booking may have ended while it was being written (a claim
        // racing a cancellation), after the cancellation looked for the
        // events: do not leave them behind. The retry sweep does the same
        // for its own pushes.
        let status: Option<String> =
            sqlx::query_scalar("SELECT status FROM bookings WHERE uid = ?")
                .bind(booking_uid)
                .fetch_optional(pool)
                .await
                .unwrap_or(None);
        if status.as_deref() != Some("confirmed") {
            super::caldav_delete_booking(pool, key, user_id, booking_uid).await;
        }
    }
    let attempts = record_outcome(pool, booking_uid, &failures, kind, Utc::now()).await;
    if kind == PushKind::Retry && attempts >= ALERT_AFTER_ATTEMPTS {
        alert_hosts_once(pool, key, booking_uid, details, &failures).await;
    }
    failures
}

/// Write the claimant's copy of a claimed booking (#269), so it follows the
/// booking through the claim itself, reschedules and approvals the way the
/// host's does. The claimant is a watcher, not a host: they see the notes
/// on their dashboard but never the guest phone, so their copy leaves it
/// out. Its failures are logged only: the retries and the alert are about
/// the host's calendar, which is what the slot is booked against.
async fn push_claimant_copy(
    pool: &SqlitePool,
    key: &[u8; 32],
    booking_uid: &str,
    details: &crate::email::BookingDetails,
    targets: &[String],
) {
    // A claimant who is also a host already has the host's copy.
    let Some(claimant) = super::booking_claimant(pool, booking_uid)
        .await
        .filter(|c| !targets.contains(c))
    else {
        return;
    };
    // A collection the claimant shares with a host (a team calendar) already
    // holds the host's event: a second PUT of the uid would replace it,
    // phone included, with the claimant's copy.
    let mut host_collections = Vec::new();
    for target in targets {
        host_collections.extend(super::write_collections(pool, target).await);
    }
    let details = crate::email::BookingDetails {
        guest_phone: None,
        ..details.clone()
    };
    let owner = super::CopyOwner::Claimant;
    for msg in super::caldav_push_booking_for_user(
        pool,
        key,
        &claimant,
        booking_uid,
        &details,
        owner,
        &host_collections,
    )
    .await
    {
        tracing::warn!(uid = %booking_uid, claimant = %claimant, error = %msg, "claimant calendar copy not written");
    }
}

/// Failed attempts after which the host is emailed: the initial push and two
/// retries, about 20 minutes after the booking. Late enough that a brief
/// outage resolves on its own without bothering anyone, early enough that a
/// meeting later the same day is not missed. Retries go on after the email.
pub(super) const ALERT_AFTER_ATTEMPTS: i64 = 3;

/// Email every host whose calendar is still missing the booking, once per
/// booking: `writeback_alerted_at` is claimed atomically before sending, and
/// only cleared when a write-back succeeds. Left unclaimed while SMTP is not
/// configured, so a later retry can still send it.
async fn alert_hosts_once(
    pool: &SqlitePool,
    key: &[u8; 32],
    booking_uid: &str,
    details: &crate::email::BookingDetails,
    failures: &[(String, String)],
) {
    let Ok(Some(smtp)) = crate::email::load_smtp_config(pool, key).await else {
        tracing::warn!(
            uid = %booking_uid,
            "calendar write-back still failing and SMTP is unconfigured, so the host cannot be emailed"
        );
        return;
    };
    let claimed = sqlx::query(
        "UPDATE bookings SET writeback_alerted_at = datetime('now') \
         WHERE uid = ? AND writeback_alerted_at IS NULL",
    )
    .bind(booking_uid)
    .execute(pool)
    .await
    .map(|r| r.rows_affected() == 1)
    .unwrap_or(false);
    if !claimed {
        return;
    }

    let mut hosts: Vec<&str> = failures.iter().map(|(user, _)| user.as_str()).collect();
    hosts.sort_unstable();
    hosts.dedup();
    for host_id in hosts {
        let error = summarize(
            &failures
                .iter()
                .filter(|(user, _)| user == host_id)
                .cloned()
                .collect::<Vec<_>>(),
        );
        let recipient: Option<(String, String, Option<String>)> = sqlx::query_as(
            "SELECT name, COALESCE(booking_email, email), language FROM users WHERE id = ?",
        )
        .bind(host_id)
        .fetch_optional(pool)
        .await
        .unwrap_or(None);
        let Some((name, email, language)) = recipient else {
            continue;
        };
        let host_details = crate::email::BookingDetails {
            host_name: name,
            host_email: email,
            host_language: crate::email::supported_language(language),
            ..details.clone()
        };
        // Inline: this runs from the background loop, off any request path,
        // and `send_email` bounds each attempt.
        super::log_email_result(
            crate::email::send_host_writeback_failure(&smtp, &host_details, &error).await,
            "host_writeback_failure",
            booking_uid,
        );
    }
}

/// Store the outcome of a write-back attempt. Success clears every trace of
/// earlier failures. A failure counts the attempt and schedules the next
/// one; a fresh push restarts the count. Returns the attempt count.
pub(super) async fn record_outcome(
    pool: &SqlitePool,
    booking_uid: &str,
    failures: &[(String, String)],
    kind: PushKind,
    now: DateTime<Utc>,
) -> i64 {
    if failures.is_empty() {
        let _ = sqlx::query(
            "UPDATE bookings SET writeback_error = NULL, writeback_attempts = 0, \
             writeback_next_attempt_at = NULL, writeback_alerted_at = NULL \
             WHERE uid = ? AND (writeback_error IS NOT NULL OR writeback_attempts > 0)",
        )
        .bind(booking_uid)
        .execute(pool)
        .await;
        return 0;
    }
    let previous: i64 = match kind {
        PushKind::Fresh => 0,
        PushKind::Retry => {
            sqlx::query_scalar("SELECT writeback_attempts FROM bookings WHERE uid = ?")
                .bind(booking_uid)
                .fetch_optional(pool)
                .await
                .ok()
                .flatten()
                .unwrap_or(0)
        }
    };
    let attempts = previous + 1;
    let next = next_attempt_at(attempts, now).map(stamp);
    let summary = summarize(failures);
    tracing::warn!(
        uid = %booking_uid,
        attempts,
        next_attempt_at = next.as_deref().unwrap_or("none"),
        error = %summary,
        "calendar write-back failed; booking stays confirmed"
    );
    let _ = sqlx::query(
        "UPDATE bookings SET writeback_error = ?, writeback_attempts = ?, \
         writeback_next_attempt_at = ? WHERE uid = ?",
    )
    .bind(&summary)
    .bind(attempts)
    .bind(&next)
    .bind(booking_uid)
    .execute(pool)
    .await;
    attempts
}

/// Everything a retry needs to rebuild the event from the booking row: the
/// same fields the original push carried into the ICS. Returns the details
/// and the event type owner (the fallback write target).
pub(super) async fn writeback_details(
    pool: &SqlitePool,
    booking_uid: &str,
) -> Option<(crate::email::BookingDetails, String)> {
    #[allow(clippy::type_complexity)]
    let row: Option<(
        String,
        String,
        String,
        String,
        String,
        Option<String>,
        Option<String>,
        Option<String>,
        String,
        Option<i32>,
        Option<String>,
        Option<String>,
        Option<String>,
        String,
    )> = sqlx::query_as(
        "SELECT b.guest_name, b.guest_email, COALESCE(NULLIF(b.guest_timezone, ''), 'UTC'), \
                CASE b.time_version WHEN 1 THEN b.start_at ELSE rtrim(b.start_at, 'Z') END, \
                CASE b.time_version WHEN 1 THEN b.end_at ELSE rtrim(b.end_at, 'Z') END, \
                u.name, COALESCE(u.booking_email, u.email), \
                COALESCE(NULLIF(b.meeting_url, ''), et.location_value), et.id, \
                et.reminder_minutes, b.language, u.language, a.user_id, et.title \
         FROM bookings b \
         JOIN event_types et ON et.id = b.event_type_id \
         LEFT JOIN accounts a ON a.id = et.account_id \
         LEFT JOIN users u ON u.id = COALESCE(b.assigned_user_id, a.user_id) \
         WHERE b.uid = ?",
    )
    .bind(booking_uid)
    .fetch_optional(pool)
    .await
    .unwrap_or(None);
    let (
        guest_name,
        guest_email,
        guest_timezone,
        start_at,
        end_at,
        host_name,
        host_email,
        location,
        et_id,
        reminder_minutes,
        guest_language,
        host_language,
        owner_id,
        event_title,
    ) = row?;

    let stored_tz = super::get_host_tz(pool, &et_id).await;
    let guest_tz = guest_timezone.parse::<Tz>().unwrap_or(Tz::UTC);
    let (date, start_time, end_time) =
        super::booking_strings_in_guest_tz(&start_at, &end_at, stored_tz, guest_tz);
    // The same guest input every other push carries: without the phone, a
    // booking whose first push failed reached the host's calendar without
    // it once a retry landed (#266).
    let (notes, guest_phone) = super::booking_guest_input(pool, booking_uid).await;
    let additional_attendees = super::booking_additional_attendees(pool, booking_uid).await;

    // The organizer the event was created with: the stored snapshot for a
    // multi-host booking (#161), the assigned member or owner otherwise. The
    // row's own join only knows the latter, which put the owner (or nobody,
    // for a team's placeholder account) on a collective retry.
    let (host_name, host_email, host_language) =
        match super::booking_host_identity(pool, booking_uid).await {
            Some((name, email, language)) => (name, email, language),
            None => (
                host_name.unwrap_or_default(),
                host_email.unwrap_or_default(),
                crate::email::supported_language(host_language),
            ),
        };

    let details = crate::email::BookingDetails {
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
        uid: booking_uid.to_string(),
        notes,
        guest_phone,
        location: location.filter(|l| !l.is_empty()),
        reminder_minutes,
        additional_attendees,
        guest_language,
        host_language,
        host_timezone: stored_tz.name().to_string(),
        sequence: super::booking_ics_sequence(pool, booking_uid).await,
        ..Default::default()
    };
    Some((details, owner_id.unwrap_or_default()))
}

/// One pass of the retry sweep, run from the background loop: every
/// confirmed booking still ahead whose next attempt is due. Cancelled,
/// declined and past bookings are never retried. Returns how many bookings
/// were attempted.
pub(crate) async fn retry_failed_writebacks(
    pool: &SqlitePool,
    key: &[u8; 32],
    now: DateTime<Utc>,
) -> usize {
    let due: Vec<(String,)> = sqlx::query_as(
        "SELECT uid FROM bookings \
         WHERE status = 'confirmed' \
           AND writeback_next_attempt_at IS NOT NULL \
           AND writeback_next_attempt_at <= ? \
           AND datetime(start_at) > datetime(?) \
         ORDER BY writeback_next_attempt_at \
         LIMIT 20",
    )
    .bind(stamp(now))
    .bind(now.to_rfc3339())
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    let mut attempted = 0;
    for (uid,) in &due {
        let Some((details, owner_id)) = writeback_details(pool, uid).await else {
            continue;
        };
        attempted += 1;
        let failures = push_and_record(pool, key, &owner_id, uid, &details, PushKind::Retry).await;
        if failures.is_empty() {
            tracing::info!(uid = %uid, "calendar write-back retry succeeded");
        }

        // The booking may have been cancelled while the PUT was in flight.
        // The cancel path deletes the event only when it saw an href, which
        // this retry may have stored after it looked: clean up here so a
        // retry never leaves a ghost event behind.
        let status: Option<String> =
            sqlx::query_scalar("SELECT status FROM bookings WHERE uid = ?")
                .bind(uid)
                .fetch_optional(pool)
                .await
                .unwrap_or(None);
        if status.as_deref() != Some("confirmed") {
            super::caldav_delete_booking(pool, key, &owner_id, uid).await;
        }
    }
    attempted
}
