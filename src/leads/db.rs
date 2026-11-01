//! DB access for the `partial_bookings` table.
//!
//! Operations here are pure persistence — gating, validation and HTTP
//! concerns live in `super::config` and the web handler. The intent is that
//! a future scheduled job, an alternate CLI command or a test fixture can
//! reuse these without dragging in axum.

use anyhow::Result;
use chrono::{Duration, Utc};
use sqlx::SqlitePool;
use uuid::Uuid;

use super::limits::{
    MAX_EMAIL_LEN, MAX_EXPORT_ROWS, MAX_FIELD_LEN, MAX_NAME_LEN,
    MAX_NEW_LEADS_PER_EVENT_TYPE_PER_HOUR, MAX_PHONE_LEN, MAX_SHORT_LEN, MAX_UA_LEN,
};

/// Input the HTTP handler hands us after server-side validation.
#[derive(Debug, Clone, Default)]
pub struct PartialBookingInput {
    pub event_type_id: String,
    pub host_user_id: Option<String>,
    pub lead_id: String,
    pub name: Option<String>,
    pub email: Option<String>,
    pub phone: Option<String>,
    pub notes: Option<String>,
    pub ip: Option<String>,
    pub user_agent: Option<String>,
    pub target_date: Option<String>,
    pub target_time: Option<String>,
    pub target_tz: Option<String>,
    pub utm_source: Option<String>,
    pub utm_medium: Option<String>,
    pub utm_campaign: Option<String>,
    pub referrer: Option<String>,
    /// Users named by a combined page (`/u/alice+bob/...`), the owner included;
    /// empty for every other page. Each of them sees the lead.
    pub participants: Vec<String>,
}

/// One row from `partial_bookings`, decorated with the matching event-type
/// title (and team name, when the event type belongs to a team) for
/// dashboard rendering.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct PartialBooking {
    pub id: String,
    pub event_type_id: String,
    pub event_type_title: Option<String>,
    pub team_name: Option<String>,
    pub lead_id: String,
    pub name: Option<String>,
    pub email: Option<String>,
    pub phone: Option<String>,
    pub notes: Option<String>,
    pub target_date: Option<String>,
    pub target_time: Option<String>,
    pub target_tz: Option<String>,
    pub utm_source: Option<String>,
    pub utm_medium: Option<String>,
    pub utm_campaign: Option<String>,
    pub referrer: Option<String>,
    pub contacted_at: Option<String>,
    pub completed_at: Option<String>,
    pub archived_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// Outcome of [`upsert_partial`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Upsert {
    Saved,
    /// The event type already took [`MAX_NEW_LEADS_PER_EVENT_TYPE_PER_HOUR`]
    /// new leads this hour, so this new one was not recorded.
    Throttled,
}

/// Insert or update a partial booking row keyed on `lead_id`.
///
/// Every value is cut to a per-field cap (name, email, phone, free text, user
/// agent; see `limits`) on a character boundary, so a hostile or buggy client
/// cannot bloat the table.
/// Updating a lead that exists is always allowed; only a *new* lead counts
/// against the per-event-type hourly cap.
pub async fn upsert_partial(pool: &SqlitePool, input: PartialBookingInput) -> Result<Upsert> {
    let name = trim_field(input.name, MAX_NAME_LEN);
    let email = trim_field(input.email, MAX_EMAIL_LEN);
    let phone = trim_field(input.phone, MAX_PHONE_LEN);
    let notes = trim_field(input.notes, MAX_FIELD_LEN);
    let user_agent = trim_field(input.user_agent, MAX_UA_LEN);
    let ip = trim_field(input.ip, 64);
    let utm_source = trim_field(input.utm_source, MAX_FIELD_LEN);
    let utm_medium = trim_field(input.utm_medium, MAX_FIELD_LEN);
    let utm_campaign = trim_field(input.utm_campaign, MAX_FIELD_LEN);
    let referrer = trim_field(input.referrer, MAX_FIELD_LEN);
    let target_date = trim_field(input.target_date, MAX_SHORT_LEN);
    let target_time = trim_field(input.target_time, MAX_SHORT_LEN);
    let target_tz = trim_field(input.target_tz, MAX_SHORT_LEN);

    let known: Option<(i64,)> = sqlx::query_as("SELECT 1 FROM partial_bookings WHERE lead_id = ?")
        .bind(&input.lead_id)
        .fetch_optional(pool)
        .await?;
    if known.is_none() {
        let (recent,): (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM partial_bookings
             WHERE event_type_id = ? AND created_at > datetime('now', '-1 hour')",
        )
        .bind(&input.event_type_id)
        .fetch_one(pool)
        .await?;
        if recent >= MAX_NEW_LEADS_PER_EVENT_TYPE_PER_HOUR {
            return Ok(Upsert::Throttled);
        }
    }

    let id = Uuid::new_v4().to_string();

    // Upsert on the unique lead_id. We deliberately reset event_type_id and
    // host_user_id from the request (covers the case where the guest
    // navigated between event types in the same browser session).
    //
    // Field preservation: `name`, `email`, `phone`, `notes` are COALESCE'd
    // (excluded.field FIRST, falling back to the stored value) so a guest who
    // types then clears a field doesn't wipe what was already captured.
    // `trim_field` turns empty/whitespace inputs into NULL, which makes
    // COALESCE fall back to the existing column value. Attribution
    // (utm_*/referrer) uses the opposite order — stored value first — so the
    // first non-null attribution sticks.
    sqlx::query(
        "INSERT INTO partial_bookings (
            id, event_type_id, host_user_id, lead_id,
            name, email, phone, notes, ip, user_agent,
            target_date, target_time, target_tz,
            utm_source, utm_medium, utm_campaign, referrer, updated_at
         ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, datetime('now'))
         ON CONFLICT(lead_id) DO UPDATE SET
            completed_at = CASE WHEN excluded.event_type_id = partial_bookings.event_type_id
                                THEN partial_bookings.completed_at END,
            notified_at = CASE WHEN excluded.event_type_id = partial_bookings.event_type_id
                               THEN partial_bookings.notified_at END,
            contacted_at = CASE WHEN excluded.event_type_id = partial_bookings.event_type_id
                                THEN partial_bookings.contacted_at END,
            archived_at = CASE WHEN excluded.event_type_id = partial_bookings.event_type_id
                               THEN partial_bookings.archived_at END,
            event_type_id = excluded.event_type_id,
            host_user_id = excluded.host_user_id,
            name = COALESCE(excluded.name, partial_bookings.name),
            email = COALESCE(excluded.email, partial_bookings.email),
            phone = COALESCE(excluded.phone, partial_bookings.phone),
            notes = COALESCE(excluded.notes, partial_bookings.notes),
            ip = COALESCE(excluded.ip, partial_bookings.ip),
            user_agent = COALESCE(excluded.user_agent, partial_bookings.user_agent),
            target_date = COALESCE(excluded.target_date, partial_bookings.target_date),
            target_time = COALESCE(excluded.target_time, partial_bookings.target_time),
            target_tz = COALESCE(excluded.target_tz, partial_bookings.target_tz),
            utm_source = COALESCE(partial_bookings.utm_source, excluded.utm_source),
            utm_medium = COALESCE(partial_bookings.utm_medium, excluded.utm_medium),
            utm_campaign = COALESCE(partial_bookings.utm_campaign, excluded.utm_campaign),
            referrer = COALESCE(partial_bookings.referrer, excluded.referrer),
            updated_at = datetime('now')",
    )
    .bind(&id)
    .bind(&input.event_type_id)
    .bind(&input.host_user_id)
    .bind(&input.lead_id)
    .bind(&name)
    .bind(&email)
    .bind(&phone)
    .bind(&notes)
    .bind(&ip)
    .bind(&user_agent)
    .bind(&target_date)
    .bind(&target_time)
    .bind(&target_tz)
    .bind(&utm_source)
    .bind(&utm_medium)
    .bind(&utm_campaign)
    .bind(&referrer)
    .execute(pool)
    .await?;

    // The users a combined page names are replaced each time, so a lead that
    // moves to another page does not keep the old audience.
    sqlx::query(
        "DELETE FROM partial_booking_participants
         WHERE lead_id = (SELECT id FROM partial_bookings WHERE lead_id = ?)",
    )
    .bind(&input.lead_id)
    .execute(pool)
    .await?;
    for user_id in &input.participants {
        sqlx::query(
            "INSERT OR IGNORE INTO partial_booking_participants (lead_id, user_id)
             SELECT id, ? FROM partial_bookings WHERE lead_id = ?",
        )
        .bind(user_id)
        .bind(&input.lead_id)
        .execute(pool)
        .await?;
    }

    Ok(Upsert::Saved)
}

/// Mark a partial booking as completed. Called from the booking handlers
/// once the booking row exists.
///
/// `lead_id` is whatever the browser sent, so it only counts when the lead was
/// captured on the very event type that was booked. No match is a no-op: the
/// form may have been submitted without capture active, or the row purged.
pub async fn mark_completed(pool: &SqlitePool, lead_id: &str, event_type_id: &str) {
    if lead_id.is_empty() {
        return;
    }
    // With the admin's anonymise setting on, the person goes and the count stays.
    let anonymize =
        "(SELECT lead_anonymize_on_booking FROM auth_config WHERE id = 'singleton') = 1";
    if let Err(e) = sqlx::query(&format!(
        "UPDATE partial_bookings
         SET completed_at = datetime('now'), updated_at = datetime('now'),
             name = CASE WHEN {anonymize} THEN NULL ELSE name END,
             email = CASE WHEN {anonymize} THEN NULL ELSE email END,
             phone = CASE WHEN {anonymize} THEN NULL ELSE phone END,
             notes = CASE WHEN {anonymize} THEN NULL ELSE notes END,
             ip = CASE WHEN {anonymize} THEN NULL ELSE ip END,
             user_agent = CASE WHEN {anonymize} THEN NULL ELSE user_agent END
         WHERE lead_id = ? AND event_type_id = ? AND completed_at IS NULL"
    ))
    .bind(lead_id)
    .bind(event_type_id)
    .execute(pool)
    .await
    {
        tracing::warn!(error = %e, "lead capture: could not mark a lead completed");
    }
}

/// Delete partial bookings older than `retention_days`. Returns the number
/// of rows removed.
pub async fn purge_expired(pool: &SqlitePool, retention_days: i64) -> Result<u64> {
    let cutoff = Utc::now() - Duration::days(retention_days.max(1));
    let cutoff_str = cutoff.format("%Y-%m-%d %H:%M:%S").to_string();

    let r = sqlx::query("DELETE FROM partial_bookings WHERE updated_at < ?")
        .bind(&cutoff_str)
        .execute(pool)
        .await?;
    Ok(r.rows_affected())
}

/// Which leads `?1` (a user id) may see: those of their own event types, and
/// those of the teams they belong to *now*. A team event type's `host_user_id`
/// is whoever created it, so it alone must not keep a lead visible to someone
/// who has since left the team.
const OWNED: &str = "((et.team_id IS NULL AND pb.host_user_id = ?1)
        OR et.team_id IN (SELECT team_id FROM team_members WHERE user_id = ?1)
        OR EXISTS (SELECT 1 FROM partial_booking_participants pp
                   WHERE pp.lead_id = pb.id AND pp.user_id = ?1))";

/// Columns shared by the dashboard list query, aliased so they map onto
/// [`PartialBooking`] via `FromRow`.
const PARTIAL_SELECT: &str = "SELECT pb.id, pb.event_type_id,
            et.title AS event_type_title, t.name AS team_name, pb.lead_id,
            pb.name, pb.email, pb.phone, pb.notes,
            pb.target_date, pb.target_time, pb.target_tz,
            pb.utm_source, pb.utm_medium, pb.utm_campaign, pb.referrer,
            pb.contacted_at, pb.completed_at, pb.archived_at, pb.created_at, pb.updated_at
     FROM partial_bookings pb
     LEFT JOIN event_types et ON et.id = pb.event_type_id
     LEFT JOIN teams t ON t.id = et.team_id";

/// Which slice of the leads the page lists.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LeadView {
    /// Started and not finished, not archived: the worklist.
    #[default]
    Open,
    /// Taken off the worklist by a host.
    Archived,
    /// The guest went on to book.
    Completed,
}

impl LeadView {
    /// The `?view=` value of the leads page; anything unknown is the worklist.
    pub fn parse(value: Option<&str>) -> Self {
        match value {
            Some("archived") => Self::Archived,
            Some("completed") => Self::Completed,
            _ => Self::Open,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Archived => "archived",
            Self::Completed => "completed",
        }
    }

    fn condition(self) -> &'static str {
        match self {
            Self::Open => "pb.completed_at IS NULL AND pb.archived_at IS NULL",
            Self::Archived => "pb.archived_at IS NOT NULL",
            Self::Completed => "pb.completed_at IS NOT NULL AND pb.archived_at IS NULL",
        }
    }
}

/// `text` as a `LIKE` pattern matching anywhere, with `%`, `_` and the escape
/// character itself taken literally. Empty text matches everything.
fn contains_pattern(text: &str) -> String {
    let mut pattern = String::from("%");
    for c in text.trim().chars() {
        if matches!(c, '%' | '_' | '\\') {
            pattern.push('\\');
        }
        pattern.push(c);
    }
    pattern.push('%');
    pattern
}

/// Leads for the dashboard, most recently touched first.
///
/// Scope: `Some(uid)` returns the leads of that user's own event types plus
/// those of the teams they belong to; `None` is the admin-wide list. `search`
/// matches the name, email or phone anywhere in the text, which is how a host
/// finds the lead of someone who asks to be erased.
pub async fn list_leads(
    pool: &SqlitePool,
    scope: Option<&str>,
    view: LeadView,
    search: &str,
    limit: i64,
    offset: i64,
) -> Result<Vec<PartialBooking>> {
    let sql = format!(
        "{PARTIAL_SELECT}
         WHERE (?1 IS NULL OR {OWNED}) AND {}
           AND (COALESCE(pb.name, '') || ' ' || COALESCE(pb.email, '') || ' ' || COALESCE(pb.phone, ''))
               LIKE ?2 ESCAPE '\\'
         ORDER BY pb.updated_at DESC, pb.id
         LIMIT ?3 OFFSET ?4",
        view.condition()
    );
    Ok(sqlx::query_as(&sql)
        .bind(scope)
        .bind(contains_pattern(search))
        .bind(limit.clamp(1, MAX_EXPORT_ROWS))
        .bind(offset.max(0))
        .fetch_all(pool)
        .await?)
}

/// How many leads [`list_leads`] would return without paging.
pub async fn count_leads(
    pool: &SqlitePool,
    scope: Option<&str>,
    view: LeadView,
    search: &str,
) -> Result<i64> {
    let sql = format!(
        "SELECT COUNT(*) FROM partial_bookings pb
         LEFT JOIN event_types et ON et.id = pb.event_type_id
         WHERE (?1 IS NULL OR {OWNED}) AND {}
           AND (COALESCE(pb.name, '') || ' ' || COALESCE(pb.email, '') || ' ' || COALESCE(pb.phone, ''))
               LIKE ?2 ESCAPE '\\'",
        view.condition()
    );
    Ok(sqlx::query_scalar(&sql)
        .bind(scope)
        .bind(contains_pattern(search))
        .fetch_one(pool)
        .await?)
}

/// Open leads nobody has marked as contacted yet: what the sidebar badge counts.
pub async fn count_needing_attention(pool: &SqlitePool, scope: Option<&str>) -> i64 {
    let sql = format!(
        "SELECT COUNT(*) FROM partial_bookings pb
         LEFT JOIN event_types et ON et.id = pb.event_type_id
         WHERE (?1 IS NULL OR {OWNED}) AND {} AND pb.contacted_at IS NULL",
        LeadView::Open.condition()
    );
    sqlx::query_scalar(&sql)
        .bind(scope)
        .fetch_one(pool)
        .await
        .unwrap_or(0)
}

/// Erase every lead [`list_leads`] would show for this scope, view and search
/// (not just the first page of them). Returns how many went.
pub async fn delete_leads(
    pool: &SqlitePool,
    scope: Option<&str>,
    view: LeadView,
    search: &str,
) -> Result<u64> {
    let sql = format!(
        "DELETE FROM partial_bookings WHERE id IN (
             SELECT pb.id FROM partial_bookings pb
             LEFT JOIN event_types et ON et.id = pb.event_type_id
             WHERE (?1 IS NULL OR {OWNED}) AND {}
               AND (COALESCE(pb.name, '') || ' ' || COALESCE(pb.email, '') || ' ' || COALESCE(pb.phone, ''))
                   LIKE ?2 ESCAPE '\\')",
        view.condition()
    );
    let result = sqlx::query(&sql)
        .bind(scope)
        .bind(contains_pattern(search))
        .execute(pool)
        .await?;
    Ok(result.rows_affected())
}

/// The open worklist of a user (or of everyone for `None`).
pub async fn list_recent_for_user(
    pool: &SqlitePool,
    scope: Option<&str>,
    limit: i64,
) -> Result<Vec<PartialBooking>> {
    list_leads(pool, scope, LeadView::Open, "", limit, 0).await
}

/// Aggregate conversion counts for the dashboard summary tiles, scoped the
/// same way as [`list_recent_for_user`]. `started` counts every captured
/// lead (completed or not); `completed` counts those that turned into a
/// booking; `abandoned` is the open worklist (not completed, not archived).
#[derive(Debug, Clone, Copy, Default)]
pub struct LeadStats {
    pub started: i64,
    pub completed: i64,
    pub abandoned: i64,
}

impl LeadStats {
    /// Completed / started as a 0–100 percentage (0 when nothing started).
    pub fn conversion_pct(&self) -> i64 {
        if self.started <= 0 {
            0
        } else {
            (self.completed * 100) / self.started
        }
    }
}

pub async fn stats_for_user(pool: &SqlitePool, host_user_id: Option<&str>) -> LeadStats {
    let row: Option<(i64, i64, i64)> = match host_user_id {
        Some(uid) => sqlx::query_as(&format!(
            "SELECT
                COUNT(*),
                COUNT(completed_at),
                SUM(CASE WHEN completed_at IS NULL AND archived_at IS NULL THEN 1 ELSE 0 END)
             FROM partial_bookings pb
             LEFT JOIN event_types et ON et.id = pb.event_type_id
             WHERE {OWNED}"
        ))
        .bind(uid)
        .fetch_optional(pool)
        .await
        .unwrap_or(None),
        None => sqlx::query_as(
            "SELECT
                COUNT(*),
                COUNT(completed_at),
                SUM(CASE WHEN completed_at IS NULL AND archived_at IS NULL THEN 1 ELSE 0 END)
             FROM partial_bookings",
        )
        .fetch_optional(pool)
        .await
        .unwrap_or(None),
    };
    match row {
        Some((started, completed, abandoned)) => LeadStats {
            started,
            completed,
            abandoned,
        },
        None => LeadStats::default(),
    }
}

/// True when `user_id` is allowed to act on the lead `id` — either they own
/// it (host_user_id) or they're a member of the team owning its event type.
/// Admins bypass this check at the handler level.
pub async fn user_can_access(pool: &SqlitePool, id: &str, user_id: &str) -> bool {
    let row: Option<(i64,)> = sqlx::query_as(&format!(
        "SELECT 1
             FROM partial_bookings pb
             LEFT JOIN event_types et ON et.id = pb.event_type_id
             WHERE pb.id = ?2 AND {OWNED}"
    ))
    .bind(user_id)
    .bind(id)
    .fetch_optional(pool)
    .await
    .unwrap_or(None);
    row.is_some()
}

/// Flip the "contacted" flag of a lead: set when clear, cleared when set.
pub async fn toggle_contacted(pool: &SqlitePool, id: &str) -> Result<()> {
    sqlx::query(
        "UPDATE partial_bookings
         SET contacted_at = CASE WHEN contacted_at IS NULL THEN datetime('now') END
         WHERE id = ?",
    )
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}

/// Archive a lead (drops it from the default dashboard view).
pub async fn archive(pool: &SqlitePool, id: &str) -> Result<()> {
    sqlx::query("UPDATE partial_bookings SET archived_at = datetime('now') WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Put an archived lead back on the worklist.
pub async fn unarchive(pool: &SqlitePool, id: &str) -> Result<()> {
    sqlx::query("UPDATE partial_bookings SET archived_at = NULL WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Erase a lead for good. Returns whether a row was there.
pub async fn delete_lead(pool: &SqlitePool, id: &str) -> Result<bool> {
    let result = sqlx::query("DELETE FROM partial_bookings WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(result.rows_affected() > 0)
}

/// A lead ripe for an abandonment alert.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct DueLead {
    pub id: String,
    pub host_user_id: String,
    pub event_type_id: String,
    pub name: Option<String>,
    pub email: String,
    pub phone: Option<String>,
}

/// Leads ripe for an abandonment alert: not completed, not archived, not yet
/// notified, with an email to reach, last touched more than `older_than_minutes`
/// ago and no more than `max_age_hours` ago (so a first rollout does not alert
/// on very old rows).
pub async fn due_for_notification(
    pool: &SqlitePool,
    older_than_minutes: i64,
    max_age_hours: i64,
) -> Vec<DueLead> {
    sqlx::query_as(
        "SELECT pb.id, COALESCE(pb.host_user_id, '') AS host_user_id, pb.event_type_id, pb.name, pb.email, pb.phone
         FROM partial_bookings pb
         JOIN event_types et ON et.id = pb.event_type_id
         WHERE pb.completed_at IS NULL
           AND pb.archived_at IS NULL
           AND pb.notified_at IS NULL
           AND (pb.host_user_id IS NOT NULL OR et.team_id IS NOT NULL)

           AND pb.email IS NOT NULL AND pb.email != ''
           AND pb.updated_at < datetime('now', ?)
           AND pb.updated_at > datetime('now', ?)
         ORDER BY pb.updated_at ASC
         LIMIT 50",
    )
    .bind(format!("-{} minutes", older_than_minutes.max(1)))
    .bind(format!("-{} hours", max_age_hours.max(1)))
    .fetch_all(pool)
    .await
    .unwrap_or_else(|e| {
        // A swallowed error here would stop the alerts with no other trace.
        tracing::warn!(error = %e, "lead capture: due_for_notification query failed");
        Vec::new()
    })
}

/// How many alerts this host was sent in the last hour.
pub async fn alerts_sent_last_hour(pool: &SqlitePool, host_user_id: &str) -> i64 {
    sqlx::query_scalar(
        "SELECT COUNT(*) FROM partial_bookings
         WHERE host_user_id = ? AND notified_at > datetime('now', '-1 hour')",
    )
    .bind(host_user_id)
    .fetch_one(pool)
    .await
    .unwrap_or(0)
}

/// Mark a lead as notified so the background task emails the host at most
/// once per abandoned lead.
pub async fn mark_notified(pool: &SqlitePool, id: &str) {
    if let Err(e) =
        sqlx::query("UPDATE partial_bookings SET notified_at = datetime('now') WHERE id = ?")
            .bind(id)
            .execute(pool)
            .await
    {
        // If this silently fails the host gets a duplicate abandonment alert
        // on the next loop tick, so surface it.
        tracing::warn!(lead_id = %id, error = %e, "lead capture: mark_notified failed");
    }
}

/// Characters that never belong in something a guest typed: control codes
/// (a NUL cuts a SQLite text short), and the bidirectional and zero-width
/// marks that can make a name read as something else on the leads page.
fn is_unwanted(c: char) -> bool {
    (c.is_control() && c != '\n')
        || matches!(c, '\u{200B}'..='\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2060}'..='\u{2064}' | '\u{2066}'..='\u{2069}' | '\u{FEFF}')
}

/// Clean a field and cut it to at most `max_bytes` UTF-8 bytes without
/// splitting a codepoint. Returns `None` for empty input so we store NULL, not
/// an empty string.
fn trim_field(value: Option<String>, max_bytes: usize) -> Option<String> {
    let cleaned: String = value?.chars().filter(|c| !is_unwanted(*c)).collect();
    let trimmed = cleaned.trim();
    if trimmed.is_empty() {
        return None;
    }
    if trimmed.len() <= max_bytes {
        return Some(trimmed.to_string());
    }
    // Walk back to a codepoint boundary at or before `max_bytes`.
    let mut idx = max_bytes;
    while idx > 0 && !trimmed.is_char_boundary(idx) {
        idx -= 1;
    }
    Some(trimmed[..idx].to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
    use std::str::FromStr;

    async fn pool() -> SqlitePool {
        let pool = SqlitePoolOptions::new()
            .max_connections(2)
            .connect_with(
                SqliteConnectOptions::from_str("sqlite::memory:")
                    .unwrap()
                    .foreign_keys(true),
            )
            .await
            .unwrap();
        crate::db::migrate(&pool).await.unwrap();
        pool
    }

    async fn seed_event_type(pool: &SqlitePool) -> (String, String) {
        let user_id = Uuid::new_v4().to_string();
        let account_id = Uuid::new_v4().to_string();
        let et_id = Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO users (id, email, name, role, auth_provider, username, enabled) VALUES (?, 'h@example.com', 'H', 'admin', 'local', 'h', 1)")
            .bind(&user_id).execute(pool).await.unwrap();
        sqlx::query("INSERT INTO accounts (id, name, email, timezone, user_id) VALUES (?, 'H', 'h@example.com', 'UTC', ?)")
            .bind(&account_id).bind(&user_id).execute(pool).await.unwrap();
        sqlx::query("INSERT INTO event_types (id, account_id, slug, title, duration_min, lead_capture, created_by_user_id) VALUES (?, ?, 'intro', 'Intro', 30, 1, ?)")
            .bind(&et_id).bind(&account_id).bind(&user_id).execute(pool).await.unwrap();
        (user_id, et_id)
    }

    #[tokio::test]
    async fn upsert_inserts_then_updates_same_lead_id() {
        let pool = pool().await;
        let (user_id, et_id) = seed_event_type(&pool).await;
        let lead_id = Uuid::new_v4().to_string();

        let first = upsert_partial(
            &pool,
            PartialBookingInput {
                event_type_id: et_id.clone(),
                host_user_id: Some(user_id.clone()),
                lead_id: lead_id.clone(),
                email: Some("partial@".to_string()),
                ..Default::default()
            },
        )
        .await
        .unwrap();

        let second = upsert_partial(
            &pool,
            PartialBookingInput {
                event_type_id: et_id.clone(),
                host_user_id: Some(user_id.clone()),
                lead_id: lead_id.clone(),
                email: Some("partial@example.com".to_string()),
                name: Some("Partial Pat".to_string()),
                ..Default::default()
            },
        )
        .await
        .unwrap();

        assert_eq!(first, Upsert::Saved);
        assert_eq!(second, Upsert::Saved);

        let count: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM partial_bookings WHERE lead_id = ?")
                .bind(&lead_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(count.0, 1, "second upsert must update, not insert");

        let row: (String, Option<String>) =
            sqlx::query_as("SELECT email, name FROM partial_bookings WHERE lead_id = ?")
                .bind(&lead_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(row.0, "partial@example.com");
        assert_eq!(row.1.as_deref(), Some("Partial Pat"));
    }

    #[tokio::test]
    async fn upsert_preserves_captured_fields_when_blanked() {
        // A guest who types their name then deletes the field shouldn't
        // wipe the captured row — we keep the latest non-empty value.
        let pool = pool().await;
        let (user_id, et_id) = seed_event_type(&pool).await;
        let lead_id = Uuid::new_v4().to_string();

        upsert_partial(
            &pool,
            PartialBookingInput {
                event_type_id: et_id.clone(),
                host_user_id: Some(user_id.clone()),
                lead_id: lead_id.clone(),
                name: Some("Pat".to_string()),
                email: Some("pat@example.com".to_string()),
                phone: Some("+33600000000".to_string()),
                notes: Some("about pricing".to_string()),
                ..Default::default()
            },
        )
        .await
        .unwrap();

        // Now post a "cleared form" event (all empty strings) — trim_field
        // turns them into None, COALESCE preserves the prior values.
        upsert_partial(
            &pool,
            PartialBookingInput {
                event_type_id: et_id,
                host_user_id: Some(user_id),
                lead_id: lead_id.clone(),
                name: Some("".to_string()),
                email: Some("   ".to_string()),
                phone: Some("".to_string()),
                notes: Some("".to_string()),
                ..Default::default()
            },
        )
        .await
        .unwrap();

        let row: (
            Option<String>,
            Option<String>,
            Option<String>,
            Option<String>,
        ) = sqlx::query_as(
            "SELECT name, email, phone, notes FROM partial_bookings WHERE lead_id = ?",
        )
        .bind(&lead_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(row.0.as_deref(), Some("Pat"));
        assert_eq!(row.1.as_deref(), Some("pat@example.com"));
        assert_eq!(row.2.as_deref(), Some("+33600000000"));
        assert_eq!(row.3.as_deref(), Some("about pricing"));
    }

    #[tokio::test]
    async fn mark_completed_sets_timestamp() {
        let pool = pool().await;
        let (user_id, et_id) = seed_event_type(&pool).await;
        let lead_id = Uuid::new_v4().to_string();

        upsert_partial(
            &pool,
            PartialBookingInput {
                event_type_id: et_id.clone(),
                host_user_id: Some(user_id),
                lead_id: lead_id.clone(),
                ..Default::default()
            },
        )
        .await
        .unwrap();

        mark_completed(&pool, &lead_id, &et_id).await;

        let completed: Option<String> =
            sqlx::query_scalar("SELECT completed_at FROM partial_bookings WHERE lead_id = ?")
                .bind(&lead_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert!(completed.is_some(), "completed_at must be populated");
    }

    #[tokio::test]
    async fn purge_expired_removes_old_rows_only() {
        let pool = pool().await;
        let (user_id, et_id) = seed_event_type(&pool).await;
        let lead_old = Uuid::new_v4().to_string();
        let lead_new = Uuid::new_v4().to_string();

        upsert_partial(
            &pool,
            PartialBookingInput {
                event_type_id: et_id.clone(),
                host_user_id: Some(user_id.clone()),
                lead_id: lead_old.clone(),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        upsert_partial(
            &pool,
            PartialBookingInput {
                event_type_id: et_id,
                host_user_id: Some(user_id),
                lead_id: lead_new.clone(),
                ..Default::default()
            },
        )
        .await
        .unwrap();

        // Backdate the "old" row to 90 days ago.
        sqlx::query("UPDATE partial_bookings SET updated_at = datetime('now', '-90 days') WHERE lead_id = ?")
            .bind(&lead_old)
            .execute(&pool)
            .await
            .unwrap();

        let removed = purge_expired(&pool, 30).await.unwrap();
        assert_eq!(removed, 1);

        let still: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM partial_bookings WHERE lead_id IN (?, ?)")
                .bind(&lead_old)
                .bind(&lead_new)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(still.0, 1, "the recent row must remain");
    }

    #[test]
    fn trim_field_caps_size_at_codepoint_boundary() {
        // 5 multibyte chars × 4 bytes each = 20 bytes; cap at 6 bytes.
        let s = "🌍🌎🌏🌐🗺".to_string();
        let trimmed = trim_field(Some(s.clone()), 6).unwrap();
        // Each emoji is 4 bytes — only the first one fits within 6 bytes.
        assert_eq!(trimmed.chars().count(), 1);
        // Make sure we never split a codepoint (would panic on slicing).
        assert!(trimmed.is_char_boundary(trimmed.len()));
    }

    #[test]
    fn trim_field_drops_blank_input() {
        assert!(trim_field(Some("   ".to_string()), 100).is_none());
        assert!(trim_field(Some(String::new()), 100).is_none());
        assert!(trim_field(None, 100).is_none());
    }

    /// Insert a lead for the given event type/host and return its lead_id.
    async fn seed_lead(pool: &SqlitePool, et_id: &str, host: Option<&str>, email: &str) -> String {
        let lead_id = Uuid::new_v4().to_string();
        upsert_partial(
            pool,
            PartialBookingInput {
                event_type_id: et_id.to_string(),
                host_user_id: host.map(str::to_string),
                lead_id: lead_id.clone(),
                email: Some(email.to_string()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        lead_id
    }

    #[tokio::test]
    async fn stats_counts_started_completed_abandoned() {
        let pool = pool().await;
        let (user_id, et_id) = seed_event_type(&pool).await;

        let done = seed_lead(&pool, &et_id, Some(&user_id), "done@x.com").await;
        let _open = seed_lead(&pool, &et_id, Some(&user_id), "open@x.com").await;
        let arch = seed_lead(&pool, &et_id, Some(&user_id), "arch@x.com").await;

        mark_completed(&pool, &done, &et_id).await;
        // Archive the third lead via its PK.
        let arch_pk: String =
            sqlx::query_scalar("SELECT id FROM partial_bookings WHERE lead_id = ?")
                .bind(&arch)
                .fetch_one(&pool)
                .await
                .unwrap();
        archive(&pool, &arch_pk).await.unwrap();

        let s = stats_for_user(&pool, Some(&user_id)).await;
        assert_eq!(s.started, 3, "all three count as started");
        assert_eq!(s.completed, 1);
        assert_eq!(s.abandoned, 1, "only the open, non-archived lead");
        assert_eq!(s.conversion_pct(), 33);
    }

    #[tokio::test]
    async fn archived_and_completed_drop_from_list() {
        let pool = pool().await;
        let (user_id, et_id) = seed_event_type(&pool).await;
        let keep = seed_lead(&pool, &et_id, Some(&user_id), "keep@x.com").await;
        let done = seed_lead(&pool, &et_id, Some(&user_id), "done@x.com").await;
        mark_completed(&pool, &done, &et_id).await;

        let rows = list_recent_for_user(&pool, Some(&user_id), 100)
            .await
            .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].lead_id, keep);
    }

    #[tokio::test]
    async fn a_member_who_left_the_team_no_longer_sees_its_leads() {
        let pool = pool().await;
        let (creator, et_id) = seed_event_type(&pool).await;
        let team = Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO teams (id, name, slug) VALUES (?, 'T', 't')")
            .bind(&team)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("UPDATE event_types SET team_id = ? WHERE id = ?")
            .bind(&team)
            .bind(&et_id)
            .execute(&pool)
            .await
            .unwrap();
        seed_lead(&pool, &et_id, Some(&creator), "x@x.com").await;
        let pk: String = sqlx::query_scalar("SELECT id FROM partial_bookings LIMIT 1")
            .fetch_one(&pool)
            .await
            .unwrap();

        // The creator is not (or no longer) a member of the team.
        assert!(!user_can_access(&pool, &pk, &creator).await);
        assert!(list_recent_for_user(&pool, Some(&creator), 10)
            .await
            .unwrap()
            .is_empty());
        assert_eq!(stats_for_user(&pool, Some(&creator)).await.started, 0);

        sqlx::query("INSERT INTO team_members (team_id, user_id, role) VALUES (?, ?, 'member')")
            .bind(&team)
            .bind(&creator)
            .execute(&pool)
            .await
            .unwrap();
        assert!(user_can_access(&pool, &pk, &creator).await);
        assert_eq!(
            list_recent_for_user(&pool, Some(&creator), 10)
                .await
                .unwrap()
                .len(),
            1
        );
    }

    #[tokio::test]
    async fn contact_fields_have_their_own_caps() {
        let pool = pool().await;
        let (user_id, et_id) = seed_event_type(&pool).await;
        let lead_id = Uuid::new_v4().to_string();
        upsert_partial(
            &pool,
            PartialBookingInput {
                event_type_id: et_id,
                host_user_id: Some(user_id),
                lead_id: lead_id.clone(),
                name: Some("n".repeat(5000)),
                email: Some("e".repeat(5000)),
                phone: Some("1".repeat(5000)),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        let (name, email, phone): (String, String, String) =
            sqlx::query_as("SELECT name, email, phone FROM partial_bookings WHERE lead_id = ?")
                .bind(&lead_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(name.len(), MAX_NAME_LEN);
        assert_eq!(email.len(), MAX_EMAIL_LEN);
        assert_eq!(phone.len(), MAX_PHONE_LEN);
    }

    #[tokio::test]
    async fn search_finds_text_anywhere_and_treats_wildcards_literally() {
        let pool = pool().await;
        let (user_id, et_id) = seed_event_type(&pool).await;
        seed_lead(&pool, &et_id, Some(&user_id), "pat@arch.example").await;
        let find = |q: &'static str| {
            let (pool, uid) = (pool.clone(), user_id.clone());
            async move {
                list_leads(&pool, Some(&uid), LeadView::Open, q, 10, 0)
                    .await
                    .unwrap()
                    .len()
            }
        };
        assert_eq!(find("").await, 1);
        assert_eq!(find("pat").await, 1);
        assert_eq!(find("ARCH").await, 1, "case-insensitive");
        assert_eq!(find("@arch.ex").await, 1, "anywhere in the text");
        assert_eq!(find("nobody").await, 0);
        assert_eq!(find("%").await, 0, "a percent sign is not a wildcard");
        assert_eq!(find("_").await, 0);
        assert_eq!(find("p_t").await, 0);
        assert_eq!(find("\\").await, 0);
    }

    #[test]
    fn captured_text_loses_control_and_direction_marks() {
        assert_eq!(
            trim_field(Some("a\u{0}b".into()), 50).as_deref(),
            Some("ab")
        );
        assert_eq!(
            trim_field(Some("evil\u{202E}gpj.exe".into()), 50).as_deref(),
            Some("evilgpj.exe")
        );
        assert_eq!(
            trim_field(Some("zero\u{200B}width".into()), 50).as_deref(),
            Some("zerowidth")
        );
        assert_eq!(
            trim_field(Some("two\nlines\ttab".into()), 50).as_deref(),
            Some("two\nlinestab")
        );
        assert_eq!(trim_field(Some("\u{0}\u{202E} ".into()), 50), None);
        assert_eq!(
            trim_field(Some("héllo wörld".into()), 2).as_deref(),
            Some("h")
        );
    }

    #[tokio::test]
    async fn bulk_delete_takes_exactly_what_the_list_shows() {
        let pool = pool().await;
        let (user_id, et_id) = seed_event_type(&pool).await;
        for i in 0..5 {
            seed_lead(&pool, &et_id, Some(&user_id), &format!("keep{i}@x.com")).await;
        }
        for i in 0..3 {
            seed_lead(&pool, &et_id, Some(&user_id), &format!("junk{i}@x.com")).await;
        }
        let done = seed_lead(&pool, &et_id, Some(&user_id), "junkdone@x.com").await;
        mark_completed(&pool, &done, &et_id).await;
        let n = delete_leads(&pool, Some(&user_id), LeadView::Open, "junk")
            .await
            .unwrap();
        assert_eq!(n, 3, "the open ones matching, not the booked one");
        let left: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM partial_bookings")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(left, 6);
        assert_eq!(
            delete_leads(&pool, Some("someone-else"), LeadView::Open, "")
                .await
                .unwrap(),
            0,
            "a user with no access deletes nothing"
        );
        assert_eq!(
            delete_leads(&pool, None, LeadView::Open, "").await.unwrap(),
            5
        );
    }

    #[tokio::test]
    async fn user_can_access_own_but_not_others() {
        let pool = pool().await;
        let (user_id, et_id) = seed_event_type(&pool).await;
        let _lead = seed_lead(&pool, &et_id, Some(&user_id), "x@x.com").await;
        let pk: String = sqlx::query_scalar("SELECT id FROM partial_bookings LIMIT 1")
            .fetch_one(&pool)
            .await
            .unwrap();

        assert!(user_can_access(&pool, &pk, &user_id).await);
        assert!(
            !user_can_access(&pool, &pk, "someone-else").await,
            "a stranger must not access another host's lead"
        );
    }

    #[tokio::test]
    async fn toggle_contacted_flips_then_archive() {
        let pool = pool().await;
        let (user_id, et_id) = seed_event_type(&pool).await;
        let _lead = seed_lead(&pool, &et_id, Some(&user_id), "x@x.com").await;
        let pk: String = sqlx::query_scalar("SELECT id FROM partial_bookings LIMIT 1")
            .fetch_one(&pool)
            .await
            .unwrap();

        toggle_contacted(&pool, &pk).await.unwrap();
        let c: Option<String> =
            sqlx::query_scalar("SELECT contacted_at FROM partial_bookings WHERE id = ?")
                .bind(&pk)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert!(c.is_some(), "contacted_at set");

        toggle_contacted(&pool, &pk).await.unwrap();
        let c: Option<String> =
            sqlx::query_scalar("SELECT contacted_at FROM partial_bookings WHERE id = ?")
                .bind(&pk)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert!(c.is_none(), "contacted_at cleared");
    }

    #[tokio::test]
    async fn team_member_sees_team_event_type_lead() {
        let pool = pool().await;
        let (creator, _et_personal) = seed_event_type(&pool).await;

        // A second user who is a team member but not the creator.
        let member = Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO users (id, email, name, role, auth_provider, username, enabled) VALUES (?, 'm@example.com', 'M', 'user', 'local', 'm', 1)")
            .bind(&member).execute(&pool).await.unwrap();

        let team_id = Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO teams (id, name, slug, visibility, created_by) VALUES (?, 'T', 't', 'public', ?)")
            .bind(&team_id).bind(&creator).execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO team_members (team_id, user_id, role, source) VALUES (?, ?, 'member', 'direct')")
            .bind(&team_id).bind(&member).execute(&pool).await.unwrap();

        // Team event type created by `creator`, with a lead attributed to creator.
        let team_account = Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO accounts (id, name, email, timezone, user_id) VALUES (?, 'C', 'c@x.com', 'UTC', ?)")
            .bind(&team_account).bind(&creator).execute(&pool).await.unwrap();
        let team_et = Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO event_types (id, account_id, slug, title, duration_min, lead_capture, team_id, created_by_user_id) VALUES (?, ?, 'team-intro', 'Team Intro', 30, 1, ?, ?)")
            .bind(&team_et).bind(&team_account).bind(&team_id).bind(&creator).execute(&pool).await.unwrap();
        let lead = seed_lead(&pool, &team_et, Some(&creator), "lead@x.com").await;

        // The member (not the creator) should see the team lead.
        let rows = list_recent_for_user(&pool, Some(&member), 100)
            .await
            .unwrap();
        assert!(
            rows.iter().any(|r| r.lead_id == lead),
            "team member must see the team event type's lead"
        );
        assert!(user_can_access(&pool, &rows[0].id, &member).await);
    }

    #[tokio::test]
    async fn due_for_notification_windows_and_marks() {
        let pool = pool().await;
        let (user_id, et_id) = seed_event_type(&pool).await;
        let lead = seed_lead(&pool, &et_id, Some(&user_id), "abandon@x.com").await;

        // Fresh lead (updated_at = now): not yet due (needs to be >30 min old).
        let due = due_for_notification(&pool, 30, 48).await;
        assert!(due.is_empty(), "fresh lead is not abandoned yet");

        // Backdate to 1 hour ago: now within the (30min, 48h) window.
        sqlx::query("UPDATE partial_bookings SET updated_at = datetime('now', '-60 minutes') WHERE lead_id = ?")
            .bind(&lead)
            .execute(&pool)
            .await
            .unwrap();
        let due = due_for_notification(&pool, 30, 48).await;
        assert_eq!(due.len(), 1, "abandoned lead is due");
        let pk = &due[0].id;

        mark_notified(&pool, pk).await;
        let due = due_for_notification(&pool, 30, 48).await;
        assert!(due.is_empty(), "notified lead must not be returned again");
    }

    #[tokio::test]
    async fn completing_a_lead_needs_the_event_type_it_was_captured_on() {
        let pool = pool().await;
        let (user_id, et_id) = seed_event_type(&pool).await;
        let lead = seed_lead(&pool, &et_id, Some(&user_id), "pat@x.com").await;

        mark_completed(&pool, &lead, "another-event-type").await;
        let done: Option<String> =
            sqlx::query_scalar("SELECT completed_at FROM partial_bookings WHERE lead_id = ?")
                .bind(&lead)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert!(
            done.is_none(),
            "a lead from another event type is untouched"
        );
    }

    #[tokio::test]
    async fn an_event_type_stops_taking_new_leads_past_the_hourly_cap() {
        let pool = pool().await;
        let (user_id, et_id) = seed_event_type(&pool).await;
        for i in 0..MAX_NEW_LEADS_PER_EVENT_TYPE_PER_HOUR {
            seed_lead(&pool, &et_id, Some(&user_id), &format!("g{i}@x.com")).await;
        }
        let new = upsert_partial(
            &pool,
            PartialBookingInput {
                event_type_id: et_id.clone(),
                host_user_id: Some(user_id.clone()),
                lead_id: Uuid::new_v4().to_string(),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(new, Upsert::Throttled);

        // A guest already on file keeps being updated.
        let known: String = sqlx::query_scalar("SELECT lead_id FROM partial_bookings LIMIT 1")
            .fetch_one(&pool)
            .await
            .unwrap();
        let again = upsert_partial(
            &pool,
            PartialBookingInput {
                event_type_id: et_id,
                host_user_id: Some(user_id),
                lead_id: known,
                name: Some("Pat".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(again, Upsert::Saved);
    }

    #[tokio::test]
    async fn long_fields_are_cut_not_stored_whole() {
        let pool = pool().await;
        let (user_id, et_id) = seed_event_type(&pool).await;
        let lead_id = Uuid::new_v4().to_string();
        upsert_partial(
            &pool,
            PartialBookingInput {
                event_type_id: et_id,
                host_user_id: Some(user_id),
                lead_id: lead_id.clone(),
                target_date: Some("d".repeat(100_000)),
                target_tz: Some("é".repeat(100_000)),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        let (date, tz): (String, String) =
            sqlx::query_as("SELECT target_date, target_tz FROM partial_bookings WHERE lead_id = ?")
                .bind(&lead_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert!(date.len() <= MAX_SHORT_LEN);
        assert!(tz.len() <= MAX_SHORT_LEN);
    }

    #[tokio::test]
    async fn due_for_notification_skips_too_old() {
        let pool = pool().await;
        let (user_id, et_id) = seed_event_type(&pool).await;
        let lead = seed_lead(&pool, &et_id, Some(&user_id), "ancient@x.com").await;
        // 5 days old — beyond the 48h backlog cap.
        sqlx::query(
            "UPDATE partial_bookings SET updated_at = datetime('now', '-5 days') WHERE lead_id = ?",
        )
        .bind(&lead)
        .execute(&pool)
        .await
        .unwrap();
        let due = due_for_notification(&pool, 30, 48).await;
        assert!(due.is_empty(), "leads older than the cap are skipped");
    }
}
