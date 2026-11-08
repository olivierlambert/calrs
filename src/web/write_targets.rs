//! Per-event-type write destination (#47): which of a host's own calendars
//! the confirmed bookings of one event type are written to.
//!
//! Without a choice, a booking goes where it always went: the write calendar
//! of each of the host's sources (the source setting). With one, it goes to
//! that calendar only. The choice is per (event type, user), because the
//! calendar belongs to the host: on a team event type every member picks
//! their own, read for the assigned member or for each collective member.
//!
//! A choice that no longer resolves (its source disabled or removed) falls
//! back to the source setting, and so does a push the chosen calendar
//! definitively refuses (HTTP 403, 404 or 405: deleted on the server, or
//! read-only); the refusal is shown on the sources page and the next booking
//! tries the calendar again. A booking is never refused for its destination.
//! Copies written to a destination are recorded in `booking_write_copies`,
//! so a later push, cancellation or reschedule finds them where they were
//! actually written, even after the choice changed. Copies are written
//! before older ones are released, never the other way round.

use super::*;

/// A source to write a booking to, with the collection to write it in:
/// (source id, url, username, password_enc, calendar href, auth_type,
/// access_token_enc, token_expires_at, provider_type, oauth2_provider, name).
pub(super) type WriteSourceRow = (
    String,
    String,
    String,
    Option<String>,
    String,
    String,
    Option<String>,
    Option<String>,
    String,
    Option<String>,
    String,
);

/// Where one host's copy of a booking goes, and why.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum WritePlan {
    /// The write calendar of each of the host's sources, as before #47.
    Source,
    /// The calendar the host chose for this event type.
    Destination,
    /// The host owns the booking's Google Meet event, the only copy of the
    /// meeting on their calendar: it stays where it was created, on a chosen
    /// Google calendar (`on_destination`) or on the source setting.
    Meet { on_destination: bool },
}

impl WritePlan {
    /// Whether the copies written under this plan are recorded in
    /// `booking_write_copies`. The source setting records nothing, so an
    /// instance that never picks a destination behaves exactly as before.
    pub(super) fn tracks_copies(self) -> bool {
        matches!(
            self,
            WritePlan::Destination
                | WritePlan::Meet {
                    on_destination: true
                }
        )
    }
}

/// The source setting: every enabled source of `user_id` with a write
/// calendar.
pub(super) async fn default_write_sources(pool: &SqlitePool, user_id: &str) -> Vec<WriteSourceRow> {
    sqlx::query_as(
        "SELECT cs.id, cs.url, cs.username, cs.password_enc, cs.write_calendar_href, cs.auth_type, cs.access_token_enc, cs.token_expires_at, cs.provider_type, cs.oauth2_provider, cs.name
         FROM caldav_sources cs
         JOIN accounts a ON a.id = cs.account_id
         WHERE a.user_id = ? AND cs.enabled = 1 AND cs.write_calendar_href IS NOT NULL",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
    .unwrap_or_default()
}

/// The calendar `user_id` chose for event type `event_type_id`, while it
/// still resolves: a calendar of one of their own enabled sources. The
/// ownership is checked again here, not only when the choice was saved.
pub(super) async fn destination_source(
    pool: &SqlitePool,
    event_type_id: &str,
    user_id: &str,
) -> Option<WriteSourceRow> {
    sqlx::query_as(
        "SELECT cs.id, cs.url, cs.username, cs.password_enc, c.href, cs.auth_type, cs.access_token_enc, cs.token_expires_at, cs.provider_type, cs.oauth2_provider, cs.name
         FROM event_type_write_targets t
         JOIN calendars c ON c.id = t.calendar_id
         JOIN caldav_sources cs ON cs.id = c.source_id
         JOIN accounts a ON a.id = cs.account_id AND a.user_id = t.user_id
         WHERE t.event_type_id = ? AND t.user_id = ? AND cs.enabled = 1",
    )
    .bind(event_type_id)
    .bind(user_id)
    .fetch_optional(pool)
    .await
    .unwrap_or(None)
}

/// One enabled source of `user_id`, with `href` as the collection.
async fn source_row(
    pool: &SqlitePool,
    user_id: &str,
    source_id: &str,
    href: &str,
) -> Option<WriteSourceRow> {
    sqlx::query_as(
        "SELECT cs.id, cs.url, cs.username, cs.password_enc, ?, cs.auth_type, cs.access_token_enc, cs.token_expires_at, cs.provider_type, cs.oauth2_provider, cs.name
         FROM caldav_sources cs
         JOIN accounts a ON a.id = cs.account_id
         WHERE cs.id = ? AND a.user_id = ? AND cs.enabled = 1",
    )
    .bind(href)
    .bind(source_id)
    .bind(user_id)
    .fetch_optional(pool)
    .await
    .unwrap_or(None)
}

/// Where `user_id`'s copy of booking `booking_uid` is written.
pub(super) async fn host_write_sources(
    pool: &SqlitePool,
    booking_uid: &str,
    user_id: &str,
) -> (Vec<WriteSourceRow>, WritePlan) {
    if crate::google_meet::holds_the_meet(pool, booking_uid, user_id).await {
        if let Some((source_id, href)) =
            crate::google_meet::meet_destination_calendar(pool, booking_uid, user_id).await
        {
            if let Some(row) = source_row(pool, user_id, &source_id, &href).await {
                return (
                    vec![row],
                    WritePlan::Meet {
                        on_destination: true,
                    },
                );
            }
        }
        return (
            default_write_sources(pool, user_id).await,
            WritePlan::Meet {
                on_destination: false,
            },
        );
    }
    let event_type_id: Option<String> =
        sqlx::query_scalar("SELECT event_type_id FROM bookings WHERE uid = ?")
            .bind(booking_uid)
            .fetch_optional(pool)
            .await
            .unwrap_or(None);
    if let Some(et_id) = event_type_id {
        if let Some(row) = destination_source(pool, &et_id, user_id).await {
            return (vec![row], WritePlan::Destination);
        }
    }
    (
        default_write_sources(pool, user_id).await,
        WritePlan::Source,
    )
}

/// The collection a row writes to, for comparing writes across sources.
pub(super) fn row_collection(row: &WriteSourceRow) -> Collection {
    (crate::resources::url_origin(&row.1), row.4.clone())
}

/// Record that `user_id`'s copy of the booking was written (or may have
/// been) to `href` on `source_id`.
pub(super) async fn record_copy(
    pool: &SqlitePool,
    booking_uid: &str,
    user_id: &str,
    source_id: &str,
    href: &str,
) {
    let _ = sqlx::query(
        "INSERT OR IGNORE INTO booking_write_copies (booking_uid, user_id, source_id, calendar_href) \
         VALUES (?, ?, ?, ?)",
    )
    .bind(booking_uid)
    .bind(user_id)
    .bind(source_id)
    .bind(href)
    .execute(pool)
    .await;
}

/// The recorded destination copies of `user_id` for a booking, with the
/// source each lives on, as rows ready to delete from. Copies on a disabled
/// source are left alone, as writes are, and stay recorded.
async fn tracked_copies(
    pool: &SqlitePool,
    booking_uid: &str,
    user_id: &str,
) -> Vec<WriteSourceRow> {
    sqlx::query_as(
        "SELECT cs.id, cs.url, cs.username, cs.password_enc, bc.calendar_href, cs.auth_type, cs.access_token_enc, cs.token_expires_at, cs.provider_type, cs.oauth2_provider, cs.name
         FROM booking_write_copies bc
         JOIN caldav_sources cs ON cs.id = bc.source_id
         JOIN accounts a ON a.id = cs.account_id AND a.user_id = bc.user_id
         WHERE bc.booking_uid = ? AND bc.user_id = ? AND cs.enabled = 1",
    )
    .bind(booking_uid)
    .bind(user_id)
    .fetch_all(pool)
    .await
    .unwrap_or_default()
}

/// Delete the booking from one collection of one of the user's sources, and
/// its cached copy there, so it stops blocking availability. Returns whether
/// the copy is gone: deleted, or already absent (404 and 410 count as gone).
/// A failure leaves the cached copy alone, since the event is still there.
pub(super) async fn delete_from(
    pool: &SqlitePool,
    key: &[u8; 32],
    row: &WriteSourceRow,
    booking_uid: &str,
) -> bool {
    let (
        source_id,
        url,
        username,
        password_enc,
        href,
        auth_type,
        access_token_enc,
        expires,
        provider_type,
        _,
        _,
    ) = row;
    let deleted = match write_provider_for_source(
        pool,
        key,
        source_id,
        url,
        username,
        password_enc.as_deref(),
        auth_type,
        access_token_enc.as_deref(),
        expires.as_deref(),
        provider_type,
    )
    .await
    {
        Ok(provider) => match provider.delete_event(href, booking_uid).await {
            Ok(()) => true,
            Err(e) => {
                tracing::error!(uid = %booking_uid, calendar = %href, error = %e, "calendar event delete failed");
                false
            }
        },
        Err(e) => {
            tracing::error!(uid = %booking_uid, calendar = %href, error = format!("{e:#}"), "calendar event delete failed: could not build client");
            false
        }
    };
    if deleted {
        let _ = sqlx::query(
            "DELETE FROM events WHERE uid = ? AND calendar_id IN (
                SELECT id FROM calendars WHERE source_id = ? AND href = ?
            )",
        )
        .bind(booking_uid)
        .bind(source_id)
        .bind(href)
        .execute(pool)
        .await;
    }
    deleted
}

/// Forget one recorded copy.
async fn forget_copy(pool: &SqlitePool, booking_uid: &str, user_id: &str, row: &WriteSourceRow) {
    let _ = sqlx::query(
        "DELETE FROM booking_write_copies \
         WHERE booking_uid = ? AND user_id = ? AND source_id = ? AND calendar_href = ?",
    )
    .bind(booking_uid)
    .bind(user_id)
    .bind(&row.0)
    .bind(&row.4)
    .execute(pool)
    .await;
}

/// A (source id, href) pair: where one copy lives.
pub(super) type CopyPlace = (String, String);

fn place(row: &WriteSourceRow) -> CopyPlace {
    (row.0.clone(), row.4.clone())
}

/// Delete one copy and forget its row once it is gone. A failed delete keeps
/// the row, so a later push, cancellation or the background sweep tries again.
async fn delete_tracked(
    pool: &SqlitePool,
    key: &[u8; 32],
    booking_uid: &str,
    user_id: &str,
    row: &WriteSourceRow,
) -> bool {
    let deleted = delete_from(pool, key, row, booking_uid).await;
    if deleted {
        forget_copy(pool, booking_uid, user_id, row).await;
    }
    deleted
}

/// Delete every recorded destination copy of `user_id` for a booking.
/// Returns where they were, deleted or not, so the caller does not try the
/// same place twice.
pub(super) async fn delete_tracked_copies(
    pool: &SqlitePool,
    key: &[u8; 32],
    user_id: &str,
    booking_uid: &str,
) -> Vec<CopyPlace> {
    let rows = tracked_copies(pool, booking_uid, user_id).await;
    for row in &rows {
        delete_tracked(pool, key, booking_uid, user_id, row).await;
    }
    // A row whose source is no longer the user's cannot be reached at all.
    let _ = sqlx::query(
        "DELETE FROM booking_write_copies WHERE booking_uid = ? AND user_id = ?
           AND NOT EXISTS (SELECT 1 FROM caldav_sources cs
                           JOIN accounts a ON a.id = cs.account_id
                           WHERE cs.id = booking_write_copies.source_id
                             AND a.user_id = booking_write_copies.user_id)",
    )
    .bind(booking_uid)
    .bind(user_id)
    .execute(pool)
    .await;
    rows.iter().map(place).collect()
}

/// The users with a recorded copy of the booking, hosts or not any more (a
/// collective member removed, excluded or disabled after it was written).
pub(super) async fn copy_users(pool: &SqlitePool, booking_uid: &str) -> Vec<String> {
    sqlx::query_scalar(
        "SELECT DISTINCT user_id FROM booking_write_copies WHERE booking_uid = ? ORDER BY user_id",
    )
    .bind(booking_uid)
    .fetch_all(pool)
    .await
    .unwrap_or_default()
}

/// Record the copies a push under the source setting left on `user_id`'s
/// source write calendars, before the booking's first push to a chosen
/// destination: they are then released like any other copy once that push
/// lands, and found again by a cancellation if it does not.
pub(super) async fn record_source_copies(pool: &SqlitePool, booking_uid: &str, user_id: &str) {
    for row in default_write_sources(pool, user_id).await {
        record_copy(pool, booking_uid, user_id, &row.0, &row.4).await;
    }
}

/// After a push to a booking that was already written, remove the copies of
/// `user_id` that are no longer where it went: the destination changed, was
/// chosen, or was cleared since the last push. Only called once the user's
/// own push succeeded, so the booking is never left without a copy on the
/// calendar the orphan sweep watches. Returns the copies released.
///
/// `keep` holds every collection this push wrote to, for every host, so a
/// calendar shared by two members never loses the copy the other one just
/// wrote. A Google Meet event never moves. A copy that cannot be deleted
/// stays recorded, so it is tried again later.
pub(super) async fn release_moved_copies(
    pool: &SqlitePool,
    key: &[u8; 32],
    booking_uid: &str,
    user_id: &str,
    plan: WritePlan,
    keep: &[Collection],
) -> Vec<WriteSourceRow> {
    if matches!(plan, WritePlan::Meet { .. }) {
        return Vec::new();
    }
    let mut released = Vec::new();
    for row in tracked_copies(pool, booking_uid, user_id).await {
        if keep.contains(&row_collection(&row)) {
            continue;
        }
        tracing::info!(uid = %booking_uid, user_id = %user_id, calendar = %row.4, "booking moved to another calendar: deleting the earlier copy");
        delete_tracked(pool, key, booking_uid, user_id, &row).await;
        released.push(row);
    }
    released
}

/// Release every recorded copy of users who no longer host the booking.
pub(super) async fn release_former_hosts(
    pool: &SqlitePool,
    key: &[u8; 32],
    booking_uid: &str,
    targets: &[String],
    keep: &[Collection],
) -> Vec<WriteSourceRow> {
    let mut released = Vec::new();
    for user in copy_users(pool, booking_uid).await {
        if targets.contains(&user) {
            continue;
        }
        for row in tracked_copies(pool, booking_uid, &user).await {
            if keep.contains(&row_collection(&row)) {
                continue;
            }
            delete_tracked(pool, key, booking_uid, &user, &row).await;
            released.push(row);
        }
    }
    released
}

/// After copies were released, make sure `bookings.caldav_calendar_href`
/// does not name one of them: the orphan sweep would find the event gone
/// there and cancel a live booking. Places are compared as (source, href),
/// since two servers can use the same path. It is pointed at a place this
/// push wrote to, or cleared when there is none.
pub(super) async fn repoint_href(
    pool: &SqlitePool,
    booking_uid: &str,
    released: &[WriteSourceRow],
    written: &[CopyPlace],
) {
    let stored: Option<(Option<String>, Option<String>)> =
        sqlx::query_as("SELECT caldav_calendar_href, caldav_source_id FROM bookings WHERE uid = ?")
            .bind(booking_uid)
            .fetch_optional(pool)
            .await
            .unwrap_or(None);
    let Some((Some(href), source)) = stored else {
        return;
    };
    let names = |(sid, h): &CopyPlace| {
        *h == href && source.as_deref().is_none_or(|stored_sid| stored_sid == sid)
    };
    let released_places: Vec<CopyPlace> = released.iter().map(place).collect();
    if !released_places.iter().any(names) {
        return;
    }
    if source.is_some() && written.iter().any(names) {
        return;
    }
    let next = written.first();
    let _ = sqlx::query(
        "UPDATE bookings SET caldav_calendar_href = ?, caldav_source_id = ? WHERE uid = ?",
    )
    .bind(next.map(|(_, h)| h))
    .bind(next.map(|(sid, _)| sid))
    .bind(booking_uid)
    .execute(pool)
    .await;
}

/// Note on the user's choice that its calendar refused a booking, for the
/// sources page. The booking then goes to the source setting.
pub(super) async fn note_refusal(pool: &SqlitePool, booking_uid: &str, user_id: &str, error: &str) {
    // A short description (a status line, an error code), never a body.
    let error: String = error.chars().take(120).collect();
    let _ = sqlx::query(
        "UPDATE event_type_write_targets SET last_refused_at = datetime('now'), last_refused_error = ?
         WHERE user_id = ? AND event_type_id = (SELECT event_type_id FROM bookings WHERE uid = ?)",
    )
    .bind(&error)
    .bind(user_id)
    .bind(booking_uid)
    .execute(pool)
    .await;
}

/// A write landed on the chosen calendar: forget an earlier refusal.
pub(super) async fn clear_refusal(pool: &SqlitePool, booking_uid: &str, user_id: &str) {
    let _ = sqlx::query(
        "UPDATE event_type_write_targets SET last_refused_at = NULL, last_refused_error = NULL
         WHERE user_id = ? AND last_refused_at IS NOT NULL
           AND event_type_id = (SELECT event_type_id FROM bookings WHERE uid = ?)",
    )
    .bind(user_id)
    .bind(booking_uid)
    .execute(pool)
    .await;
}

/// Copies the background sweep deletes per pass.
const SWEEP_BATCH: i64 = 20;

/// Background pass over recorded copies. Rows of bookings that are over are
/// dropped: nothing will delete those copies any more, and they are in the
/// past. For bookings still ahead that ended (cancelled, declined), a copy
/// whose delete failed is tried again on the write-back retry schedule
/// (`RETRY_DELAYS_MIN`), soonest first, and given up after the last attempt.
/// Returns how many deletes were tried.
pub(crate) async fn sweep_leftover_copies(
    pool: &SqlitePool,
    key: &[u8; 32],
    now: chrono::DateTime<Utc>,
) -> usize {
    let _ = sqlx::query(
        "DELETE FROM booking_write_copies WHERE booking_uid IN (
             SELECT uid FROM bookings WHERE datetime(end_at) <= datetime(?)
         )",
    )
    .bind(now.to_rfc3339())
    .execute(pool)
    .await;
    let stamp = now.format("%Y-%m-%d %H:%M:%S").to_string();
    let due: Vec<(String, String, String, String, i64)> = sqlx::query_as(
        "SELECT bc.booking_uid, bc.user_id, bc.source_id, bc.calendar_href, bc.delete_attempts
         FROM booking_write_copies bc
         JOIN bookings b ON b.uid = bc.booking_uid
         WHERE b.status IN ('cancelled', 'declined')
           AND (bc.next_delete_at IS NULL OR bc.next_delete_at <= ?)
         ORDER BY bc.next_delete_at IS NOT NULL, bc.next_delete_at
         LIMIT ?",
    )
    .bind(&stamp)
    .bind(SWEEP_BATCH)
    .fetch_all(pool)
    .await
    .unwrap_or_default();
    let mut tried = 0;
    for (uid, user, source_id, href, attempts) in &due {
        // Its source disabled or no longer the user's: left for later.
        let Some(row) = source_row(pool, user, source_id, href).await else {
            continue;
        };
        tried += 1;
        if delete_tracked(pool, key, uid, user, &row).await {
            continue;
        }
        let attempts = attempts + 1;
        match super::writeback::next_attempt_at(attempts, now) {
            Some(next) => {
                let _ = sqlx::query(
                    "UPDATE booking_write_copies SET delete_attempts = ?, next_delete_at = ?
                     WHERE booking_uid = ? AND user_id = ? AND source_id = ? AND calendar_href = ?",
                )
                .bind(attempts)
                .bind(next.format("%Y-%m-%d %H:%M:%S").to_string())
                .bind(uid)
                .bind(user)
                .bind(source_id)
                .bind(href)
                .execute(pool)
                .await;
            }
            None => {
                tracing::warn!(uid = %uid, user_id = %user, calendar = %href, attempts, "giving up deleting a booking copy from its calendar; remove it by hand");
                forget_copy(pool, uid, user, &row).await;
            }
        }
    }
    tried
}

/// Whether the booking was written to any host calendar before: an href is
/// stored, or a destination copy is recorded.
pub(super) async fn was_written(pool: &SqlitePool, booking_uid: &str) -> bool {
    sqlx::query_scalar::<_, i64>(
        "SELECT (SELECT COUNT(*) FROM bookings WHERE uid = ? AND caldav_calendar_href IS NOT NULL)
              + (SELECT COUNT(*) FROM booking_write_copies WHERE booking_uid = ?)",
    )
    .bind(booking_uid)
    .bind(booking_uid)
    .fetch_one(pool)
    .await
    .unwrap_or(0)
        > 0
}

// --- Choosing a destination ---

/// Whether `user_id` hosts bookings of event type `event_type_id`: its owner
/// for a personal one, a member of its team for a team one. Only a host has
/// a calendar the event type's bookings can be written to.
pub(super) async fn hosts_event_type(
    pool: &SqlitePool,
    user_id: &str,
    event_type_id: &str,
) -> bool {
    sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM event_types et
         LEFT JOIN accounts a ON a.id = et.account_id
         WHERE et.id = ? AND (
             (et.team_id IS NULL AND a.user_id = ?) OR
             (et.team_id IS NOT NULL AND EXISTS (
                 SELECT 1 FROM team_members tm WHERE tm.team_id = et.team_id AND tm.user_id = ?
             ))
         )",
    )
    .bind(event_type_id)
    .bind(user_id)
    .bind(user_id)
    .fetch_one(pool)
    .await
    .unwrap_or(0)
        > 0
}

/// The calendars `user_id` can choose from: every calendar of their own
/// enabled sources, synced or not, as a source's write calendar can be
/// (#324). Each as (calendar id, "calendar (source)").
pub(super) async fn destination_choices(pool: &SqlitePool, user_id: &str) -> Vec<(String, String)> {
    let rows: Vec<(String, Option<String>, String, String)> = sqlx::query_as(
        "SELECT c.id, c.display_name, c.href, cs.name FROM calendars c
         JOIN caldav_sources cs ON cs.id = c.source_id
         JOIN accounts a ON a.id = cs.account_id
         WHERE a.user_id = ? AND cs.enabled = 1
         ORDER BY cs.name, c.display_name, c.href",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
    .unwrap_or_default();
    rows.into_iter()
        .map(|(id, name, href, source)| {
            let name = name.filter(|n| !n.trim().is_empty()).unwrap_or(href);
            (id, format!("{name} ({source})"))
        })
        .collect()
}

/// Whether `calendar_id` is a calendar `user_id` may choose (see
/// [`destination_choices`]). An empty id, the source setting, always is.
pub(super) async fn is_destination_choice(
    pool: &SqlitePool,
    user_id: &str,
    calendar_id: &str,
) -> bool {
    if calendar_id.is_empty() {
        return true;
    }
    sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM calendars c
         JOIN caldav_sources cs ON cs.id = c.source_id
         JOIN accounts a ON a.id = cs.account_id
         WHERE c.id = ? AND a.user_id = ? AND cs.enabled = 1",
    )
    .bind(calendar_id)
    .bind(user_id)
    .fetch_one(pool)
    .await
    .unwrap_or(0)
        > 0
}

/// The calendar `user_id` chose for an event type, as stored.
pub(super) async fn stored_destination(
    pool: &SqlitePool,
    event_type_id: &str,
    user_id: &str,
) -> Option<String> {
    sqlx::query_scalar(
        "SELECT calendar_id FROM event_type_write_targets WHERE event_type_id = ? AND user_id = ?",
    )
    .bind(event_type_id)
    .bind(user_id)
    .fetch_optional(pool)
    .await
    .unwrap_or(None)
}

/// Save `user_id`'s destination for an event type: a calendar id, or empty
/// for the source setting. Refused (`Err`) unless the user hosts the event
/// type and the calendar is one of their own.
pub(super) async fn set_destination(
    pool: &SqlitePool,
    user_id: &str,
    event_type_id: &str,
    calendar_id: &str,
) -> Result<(), ()> {
    let calendar_id = calendar_id.trim();
    if !hosts_event_type(pool, user_id, event_type_id).await {
        return Err(());
    }
    if stored_destination(pool, event_type_id, user_id)
        .await
        .as_deref()
        == Some(calendar_id)
    {
        // Posted back unchanged: kept as it is, even when its source is
        // disabled for now (it then shows as unavailable).
        return Ok(());
    }
    if calendar_id.is_empty() {
        let _ = sqlx::query(
            "DELETE FROM event_type_write_targets WHERE event_type_id = ? AND user_id = ?",
        )
        .bind(event_type_id)
        .bind(user_id)
        .execute(pool)
        .await;
        return Ok(());
    }
    if !is_destination_choice(pool, user_id, calendar_id).await {
        return Err(());
    }
    let _ = sqlx::query(
        "INSERT INTO event_type_write_targets (event_type_id, user_id, calendar_id) VALUES (?, ?, ?)
         ON CONFLICT(event_type_id, user_id) DO UPDATE SET calendar_id = excluded.calendar_id,
             last_refused_at = NULL, last_refused_error = NULL",
    )
    .bind(event_type_id)
    .bind(user_id)
    .bind(calendar_id)
    .execute(pool)
    .await;
    tracing::info!(user_id = %user_id, event_type_id = %event_type_id, calendar_id = %calendar_id, "write destination chosen");
    Ok(())
}

/// Whether a posted destination may be saved: a calendar the user may
/// choose, or the one already stored for this event type (kept even while
/// its source is disabled).
pub(super) async fn acceptable_destination(
    pool: &SqlitePool,
    user_id: &str,
    event_type_id: Option<&str>,
    calendar_id: &str,
) -> bool {
    if is_destination_choice(pool, user_id, calendar_id).await {
        return true;
    }
    match event_type_id {
        Some(et_id) => {
            stored_destination(pool, et_id, user_id).await.as_deref() == Some(calendar_id)
        }
        None => false,
    }
}

/// The name of one of the user's own calendars, whatever the state of its
/// source, as "calendar (source)".
async fn calendar_label(pool: &SqlitePool, user_id: &str, calendar_id: &str) -> Option<String> {
    let (name, href, source): (Option<String>, String, String) = sqlx::query_as(
        "SELECT c.display_name, c.href, cs.name FROM calendars c
         JOIN caldav_sources cs ON cs.id = c.source_id
         JOIN accounts a ON a.id = cs.account_id
         WHERE c.id = ? AND a.user_id = ?",
    )
    .bind(calendar_id)
    .bind(user_id)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten()?;
    let name = name.filter(|n| !n.trim().is_empty()).unwrap_or(href);
    Some(format!("{name} ({source})"))
}

/// The option to show for a stored choice that is not among the choices
/// (its source is disabled): kept, selected, and marked unavailable, so
/// saving the form unchanged keeps it.
pub(super) async fn unavailable_option(
    pool: &SqlitePool,
    user_id: &str,
    lang: &str,
    selected: &str,
    choices: &[(String, String)],
) -> Option<minijinja::Value> {
    if selected.is_empty() || choices.iter().any(|(id, _)| id == selected) {
        return None;
    }
    let label = calendar_label(pool, user_id, selected).await?;
    let name = tr1(lang, "write-destination-unavailable", "name", &label);
    Some(context! { id => selected, name => name })
}

/// The refusal noted on a choice, if its calendar refused the last booking.
pub(super) async fn refusal(
    pool: &SqlitePool,
    event_type_id: &str,
    user_id: &str,
) -> Option<String> {
    sqlx::query_scalar(
        "SELECT last_refused_error FROM event_type_write_targets
         WHERE event_type_id = ? AND user_id = ? AND last_refused_at IS NOT NULL",
    )
    .bind(event_type_id)
    .bind(user_id)
    .fetch_optional(pool)
    .await
    .unwrap_or(None)
    .flatten()
}

/// The destination select of the event type form for `user_id`: the choices,
/// the selected one, a stored choice that is unavailable, and the refusal
/// noted on it. `None` when they have no calendar to choose from, which hides
/// the field (the form then posts nothing and changes nothing).
pub(super) async fn destination_form_ctx(
    pool: &SqlitePool,
    user_id: &str,
    lang: &str,
    event_type_id: Option<&str>,
    selected: &str,
) -> Option<minijinja::Value> {
    let choices = destination_choices(pool, user_id).await;
    let unavailable = unavailable_option(pool, user_id, lang, selected, &choices).await;
    if choices.is_empty() && unavailable.is_none() {
        return None;
    }
    let options: Vec<minijinja::Value> = choices
        .iter()
        .map(|(id, label)| context! { id => id, name => label })
        .collect();
    let refused = match event_type_id {
        Some(et_id) if !selected.is_empty() => refusal(pool, et_id, user_id).await,
        _ => None,
    };
    Some(context! {
        choices => options,
        selected => selected,
        unavailable => unavailable,
        refused => refused,
    })
}

/// The event types `user_id` hosts, for the sources page: (id, title, team
/// name for a team event type, chosen calendar id or "", the refusal noted
/// on the choice).
pub(super) async fn hosted_event_types(
    pool: &SqlitePool,
    user_id: &str,
) -> Vec<(String, String, Option<String>, String, Option<String>)> {
    sqlx::query_as(
        "SELECT et.id, et.title, t.name, COALESCE(w.calendar_id, ''),
                CASE WHEN w.last_refused_at IS NOT NULL THEN w.last_refused_error END
         FROM event_types et
         LEFT JOIN accounts a ON a.id = et.account_id
         LEFT JOIN teams t ON t.id = et.team_id
         LEFT JOIN event_type_write_targets w ON w.event_type_id = et.id AND w.user_id = ?
         WHERE (et.team_id IS NULL AND a.user_id = ?)
            OR (et.team_id IS NOT NULL AND EXISTS (
                SELECT 1 FROM team_members tm WHERE tm.team_id = et.team_id AND tm.user_id = ?
            ))
         ORDER BY (et.team_id IS NOT NULL), t.name, et.title",
    )
    .bind(user_id)
    .bind(user_id)
    .bind(user_id)
    .fetch_all(pool)
    .await
    .unwrap_or_default()
}
