//! Per-event-type write destination (#47): which of a host's own calendars
//! the confirmed bookings of one event type are written to.
//!
//! Without a choice, a booking goes where it always went: the write calendar
//! of each of the host's sources (the source setting). With one, it goes to
//! that calendar only. The choice is per (event type, user), because the
//! calendar belongs to the host: on a team event type every member picks
//! their own, read for the assigned member or for each collective member.
//!
//! A choice that no longer resolves (its source disabled or removed, the
//! calendar gone) falls back to the source setting, so a booking is never
//! refused for it. Copies written to a destination are recorded in
//! `booking_write_copies`, so a later push, cancellation or reschedule finds
//! them where they were actually written, even after the choice changed.

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
/// source each lives on, as rows ready to delete from.
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
         WHERE bc.booking_uid = ? AND bc.user_id = ?",
    )
    .bind(booking_uid)
    .bind(user_id)
    .fetch_all(pool)
    .await
    .unwrap_or_default()
}

/// Delete the booking from one collection of one of the user's sources, and
/// its cached copy there, so it stops blocking availability.
pub(super) async fn delete_from(
    pool: &SqlitePool,
    key: &[u8; 32],
    row: &WriteSourceRow,
    booking_uid: &str,
) {
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
    match write_provider_for_source(
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
        Ok(provider) => {
            if let Err(e) = provider.delete_event(href, booking_uid).await {
                tracing::error!(uid = %booking_uid, calendar = %href, error = %e, "calendar event delete failed");
            }
        }
        Err(e) => {
            tracing::error!(uid = %booking_uid, calendar = %href, error = format!("{e:#}"), "calendar event delete failed: could not build client");
        }
    }
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

/// Delete every recorded destination copy of `user_id` for a booking and
/// forget them. Returns the hrefs they were in.
pub(super) async fn delete_tracked_copies(
    pool: &SqlitePool,
    key: &[u8; 32],
    user_id: &str,
    booking_uid: &str,
) -> Vec<String> {
    let rows = tracked_copies(pool, booking_uid, user_id).await;
    for row in &rows {
        delete_from(pool, key, row, booking_uid).await;
    }
    // Rows whose source is no longer the user's cannot be reached: they go too.
    let _ = sqlx::query("DELETE FROM booking_write_copies WHERE booking_uid = ? AND user_id = ?")
        .bind(booking_uid)
        .bind(user_id)
        .execute(pool)
        .await;
    rows.into_iter().map(|row| row.4).collect()
}

/// Before a booking that was already written is pushed again, remove the
/// copies of `user_id` that are no longer where it is going: the
/// destination changed, was chosen, or was cleared since the last push.
///
/// `keep` holds every collection this push writes to, for every host, so a
/// calendar shared by two members never loses the copy the other one just
/// wrote. A Google Meet event never moves.
pub(super) async fn release_moved_copies(
    pool: &SqlitePool,
    key: &[u8; 32],
    booking_uid: &str,
    user_id: &str,
    plan: WritePlan,
    keep: &[Collection],
) {
    if matches!(plan, WritePlan::Meet { .. }) {
        return;
    }
    let tracked = tracked_copies(pool, booking_uid, user_id).await;
    let mut stale: Vec<WriteSourceRow> = Vec::new();
    if plan == WritePlan::Destination && tracked.is_empty() {
        // First push to a destination of a booking written before under the
        // source setting: that copy sits on the source write calendars.
        stale.extend(default_write_sources(pool, user_id).await);
    }
    for row in tracked {
        if keep.contains(&row_collection(&row)) {
            continue;
        }
        forget_copy(pool, booking_uid, user_id, &row).await;
        stale.push(row);
    }
    for row in stale
        .iter()
        .filter(|row| !keep.contains(&row_collection(row)))
    {
        tracing::info!(uid = %booking_uid, user_id = %user_id, calendar = %row.4, "booking moved to another calendar: deleting the earlier copy");
        delete_from(pool, key, row, booking_uid).await;
    }
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
         ON CONFLICT(event_type_id, user_id) DO UPDATE SET calendar_id = excluded.calendar_id",
    )
    .bind(event_type_id)
    .bind(user_id)
    .bind(calendar_id)
    .execute(pool)
    .await;
    tracing::info!(user_id = %user_id, event_type_id = %event_type_id, calendar_id = %calendar_id, "write destination chosen");
    Ok(())
}

/// The destination select of the event type form for `user_id`: the choices
/// and the selected one. `None` when they have no calendar to choose from,
/// which hides the field (the form then posts nothing and changes nothing).
pub(super) async fn destination_form_ctx(
    pool: &SqlitePool,
    user_id: &str,
    selected: &str,
) -> Option<minijinja::Value> {
    let choices = destination_choices(pool, user_id).await;
    if choices.is_empty() {
        return None;
    }
    let options: Vec<minijinja::Value> = choices
        .iter()
        .map(|(id, label)| context! { id => id, name => label })
        .collect();
    Some(context! { choices => options, selected => selected })
}

/// The event types `user_id` hosts, for the sources page: (id, title, team
/// name for a team event type, chosen calendar id or "").
pub(super) async fn hosted_event_types(
    pool: &SqlitePool,
    user_id: &str,
) -> Vec<(String, String, Option<String>, String)> {
    sqlx::query_as(
        "SELECT et.id, et.title, t.name, COALESCE(w.calendar_id, '') FROM event_types et
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
