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

/// The lock serialising pushes of one booking, created on first use and
/// dropped once nobody holds it.
fn booking_push_lock(booking_uid: &str) -> std::sync::Arc<tokio::sync::Mutex<()>> {
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex, OnceLock, Weak};
    static LOCKS: OnceLock<Mutex<HashMap<String, Weak<tokio::sync::Mutex<()>>>>> = OnceLock::new();
    let mut locks = LOCKS
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    locks.retain(|_, lock| lock.strong_count() > 0);
    if let Some(lock) = locks.get(booking_uid).and_then(Weak::upgrade) {
        return lock;
    }
    let lock = Arc::new(tokio::sync::Mutex::new(()));
    locks.insert(booking_uid.to_string(), Arc::downgrade(&lock));
    lock
}

/// Hold the push lock of a booking, for tests.
#[cfg(test)]
pub(super) async fn hold_push_lock(booking_uid: &str) -> tokio::sync::OwnedMutexGuard<()> {
    booking_push_lock(booking_uid).lock_owned().await
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
    push_and_record_within(
        pool,
        key,
        user_id,
        booking_uid,
        details,
        kind,
        INLINE_PUSH_BUDGET,
    )
    .await
}

/// How long a fresh push, which runs inside a booking request, keeps
/// writing to further hosts' calendars (each Google one may wait on the
/// Calendar API). The hosts left once it is spent are recorded as failed
/// and written by the write-back retry a few minutes later.
pub(super) const INLINE_PUSH_BUDGET: std::time::Duration = std::time::Duration::from_secs(45);

/// [`push_and_record`] with a time budget for a fresh push: past it, the
/// remaining hosts are left to the retry. The first host is always written.
pub(super) async fn push_and_record_within(
    pool: &SqlitePool,
    key: &[u8; 32],
    user_id: &str,
    booking_uid: &str,
    details: &crate::email::BookingDetails,
    kind: PushKind,
    budget: std::time::Duration,
) -> Vec<(String, String)> {
    // One push per booking at a time: two pushes racing a change of
    // destination could each release the copy the other just wrote (#47).
    let lock = booking_push_lock(booking_uid);
    let _guard = lock.lock().await;

    let mut failures = Vec::new();
    let targets = super::booking_write_targets(pool, booking_uid, user_id).await;
    // Read before anything is written: whether earlier copies may need to
    // move once this push has landed (#47).
    let was_written = super::was_written(pool, booking_uid).await;
    let had_copies = super::copy_users(pool, booking_uid).await;
    let mut plans = Vec::with_capacity(targets.len());
    let mut pushed_ok = Vec::with_capacity(targets.len());
    let started = std::time::Instant::now();
    for (i, target) in targets.iter().enumerate() {
        let (mut sources, mut plan) = super::host_write_sources(pool, booking_uid, target).await;
        if kind == PushKind::Fresh && i > 0 && started.elapsed() >= budget {
            // Not written, so nothing of theirs is released below.
            failures.push((
                target.clone(),
                "not written yet: the calendar writes of this booking took too long, the write-back retries".to_string(),
            ));
            pushed_ok.push(false);
            plans.push((sources, plan));
            continue;
        }
        if was_written && plan == super::WritePlan::Destination && !had_copies.contains(target) {
            // The booking's first push to a chosen destination: what the
            // source setting wrote before is recorded first, so it is
            // released once this push lands, or deleted by a cancellation
            // if it never does.
            super::record_source_copies(pool, booking_uid, target).await;
        }
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
        let mut pushed = super::push_to_sources(
            pool,
            key,
            target,
            booking_uid,
            details,
            owner,
            &[],
            &sources,
            plan,
        )
        .await;
        if plan == super::WritePlan::Destination {
            let refusal = match pushed.as_slice() {
                [only] => only.refusal.clone(),
                _ => None,
            };
            if let Some(refusal) = refusal {
                // The chosen calendar is gone or read-only: tell the host on
                // the sources page, and write to the source setting instead
                // when there is one. With none, the refusal stays a failure,
                // so the badge and the alert still reach the host.
                super::note_refusal(pool, booking_uid, target, &refusal).await;
                let fallback = super::default_write_sources(pool, target).await;
                if fallback.is_empty() {
                    tracing::warn!(uid = %booking_uid, user_id = %target, refusal = %refusal, "chosen write calendar refused the booking and there is no source write calendar to fall back to");
                } else {
                    tracing::warn!(uid = %booking_uid, user_id = %target, refusal = %refusal, "chosen write calendar refused the booking; writing it to the source setting");
                    sources = fallback;
                    plan = super::WritePlan::Source;
                    pushed = super::push_to_sources(
                        pool,
                        key,
                        target,
                        booking_uid,
                        details,
                        owner,
                        &[],
                        &sources,
                        plan,
                    )
                    .await;
                }
            } else if pushed.is_empty() {
                super::clear_refusal(pool, booking_uid, target).await;
            }
        }
        pushed_ok.push(pushed.is_empty());
        for failure in pushed {
            failures.push((target.clone(), failure.message));
        }
        plans.push((sources, plan));
    }
    // Every collection this push wrote to (or tried to), for every host.
    let host_collections: Vec<super::Collection> = plans
        .iter()
        .flat_map(|(rows, _)| rows.iter().map(super::row_collection))
        .collect();

    // Only now, with the new copies written, release the copies no longer
    // where the booking goes, and only for hosts whose write landed: a
    // failed write keeps the old copy, which the orphan sweep still sees.
    let mut released = Vec::new();
    if was_written {
        for ((target, (_, plan)), ok) in targets.iter().zip(&plans).zip(&pushed_ok) {
            if *ok {
                released.extend(
                    super::release_moved_copies(
                        pool,
                        key,
                        booking_uid,
                        target,
                        *plan,
                        &host_collections,
                    )
                    .await,
                );
            }
        }
        // Former hosts' copies go only once every current host's copy landed.
        if pushed_ok.iter().all(|ok| *ok) {
            released.extend(
                super::release_former_hosts(pool, key, booking_uid, &targets, &host_collections)
                    .await,
            );
        }
    }
    if !released.is_empty() {
        let written: Vec<super::CopyPlace> = plans
            .iter()
            .zip(&pushed_ok)
            .filter(|(_, ok)| **ok)
            .flat_map(|((rows, _), _)| rows.iter().map(|row| (row.0.clone(), row.4.clone())))
            .collect();
        super::repoint_href(pool, booking_uid, &released, &written).await;
    }

    // A released copy may have been the one a claimant shared with a host,
    // and a retry writes no claimant copy otherwise: write it again then.
    if kind == PushKind::Fresh || !released.is_empty() {
        push_claimant_copy(pool, key, booking_uid, details, &targets, &host_collections).await;
    }
    if kind == PushKind::Fresh {
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
    host_collections: &[super::Collection],
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
    // phone included, with the claimant's copy. `host_collections` is where
    // the hosts' copies went, their chosen destinations included (#47).
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
        host_collections,
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
