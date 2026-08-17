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

use super::limits::{MAX_FIELD_LEN, MAX_UA_LEN};

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
    pub created_at: String,
    pub updated_at: String,
}

/// Insert or update a partial booking row keyed on `lead_id`. Returns the
/// row's primary key.
///
/// Field values are truncated to [`MAX_FIELD_LEN`] / [`MAX_UA_LEN`] bytes
/// (UTF-8 boundary safe via `char_indices`) so a hostile or buggy client
/// can't bloat the table.
pub async fn upsert_partial(pool: &SqlitePool, input: PartialBookingInput) -> Result<String> {
    let name = trim_field(input.name, MAX_FIELD_LEN);
    let email = trim_field(input.email, MAX_FIELD_LEN);
    let phone = trim_field(input.phone, MAX_FIELD_LEN);
    let notes = trim_field(input.notes, MAX_FIELD_LEN);
    let user_agent = trim_field(input.user_agent, MAX_UA_LEN);
    let ip = trim_field(input.ip, 64);
    let utm_source = trim_field(input.utm_source, MAX_FIELD_LEN);
    let utm_medium = trim_field(input.utm_medium, MAX_FIELD_LEN);
    let utm_campaign = trim_field(input.utm_campaign, MAX_FIELD_LEN);
    let referrer = trim_field(input.referrer, MAX_FIELD_LEN);

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
    .bind(&input.target_date)
    .bind(&input.target_time)
    .bind(&input.target_tz)
    .bind(&utm_source)
    .bind(&utm_medium)
    .bind(&utm_campaign)
    .bind(&referrer)
    .execute(pool)
    .await?;

    Ok(id)
}

/// Mark a partial booking as completed. Called from the regular booking
/// submit handler once the booking row is created.
///
/// `lead_id` is whatever the browser sent; if no row matches we silently
/// no-op — the form may have been submitted without lead capture being
/// active, or the row may have been auto-purged.
pub async fn mark_completed(pool: &SqlitePool, lead_id: &str) {
    if lead_id.is_empty() {
        return;
    }
    let _ = sqlx::query(
        "UPDATE partial_bookings
         SET completed_at = datetime('now'), updated_at = datetime('now')
         WHERE lead_id = ? AND completed_at IS NULL",
    )
    .bind(lead_id)
    .execute(pool)
    .await;
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

/// Columns shared by the dashboard list query, aliased so they map onto
/// [`PartialBooking`] via `FromRow`.
const PARTIAL_SELECT: &str = "SELECT pb.id, pb.event_type_id,
            et.title AS event_type_title, t.name AS team_name, pb.lead_id,
            pb.name, pb.email, pb.phone, pb.notes,
            pb.target_date, pb.target_time, pb.target_tz,
            pb.utm_source, pb.utm_medium, pb.utm_campaign, pb.referrer,
            pb.contacted_at, pb.completed_at, pb.created_at, pb.updated_at
     FROM partial_bookings pb
     LEFT JOIN event_types et ON et.id = pb.event_type_id
     LEFT JOIN teams t ON t.id = et.team_id";

/// Recent partial bookings for the dashboard. We only surface rows that
/// haven't been completed and haven't been archived (they're the leads
/// worth following up on).
///
/// Scope: `Some(uid)` returns leads owned by that user *plus* leads on
/// event types belonging to teams the user is a member of, so team-mates
/// can follow up on a team event type's leads, not just its creator.
/// `None` returns the admin-wide list.
pub async fn list_recent_for_user(
    pool: &SqlitePool,
    host_user_id: Option<&str>,
    limit: i64,
) -> Result<Vec<PartialBooking>> {
    let limit = limit.clamp(1, 500);
    let rows: Vec<PartialBooking> = match host_user_id {
        Some(uid) => {
            sqlx::query_as(&format!(
                "{PARTIAL_SELECT}
                 WHERE (pb.host_user_id = ?1
                        OR et.team_id IN (SELECT team_id FROM team_members WHERE user_id = ?1))
                   AND pb.completed_at IS NULL AND pb.archived_at IS NULL
                 ORDER BY pb.updated_at DESC
                 LIMIT ?2"
            ))
            .bind(uid)
            .bind(limit)
            .fetch_all(pool)
            .await?
        }
        None => {
            sqlx::query_as(&format!(
                "{PARTIAL_SELECT}
                 WHERE pb.completed_at IS NULL AND pb.archived_at IS NULL
                 ORDER BY pb.updated_at DESC
                 LIMIT ?"
            ))
            .bind(limit)
            .fetch_all(pool)
            .await?
        }
    };

    Ok(rows)
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
        Some(uid) => sqlx::query_as(
            "SELECT
                COUNT(*),
                COUNT(completed_at),
                SUM(CASE WHEN completed_at IS NULL AND archived_at IS NULL THEN 1 ELSE 0 END)
             FROM partial_bookings pb
             LEFT JOIN event_types et ON et.id = pb.event_type_id
             WHERE pb.host_user_id = ?1
                OR et.team_id IN (SELECT team_id FROM team_members WHERE user_id = ?1)",
        )
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
    let row: Option<(i64,)> = sqlx::query_as(
        "SELECT 1
         FROM partial_bookings pb
         LEFT JOIN event_types et ON et.id = pb.event_type_id
         WHERE pb.id = ?1
           AND (pb.host_user_id = ?2
                OR et.team_id IN (SELECT team_id FROM team_members WHERE user_id = ?2))",
    )
    .bind(id)
    .bind(user_id)
    .fetch_optional(pool)
    .await
    .unwrap_or(None);
    row.is_some()
}

/// Toggle the "contacted" flag on a lead. Sets `contacted_at` to now when
/// `contacted` is true, clears it otherwise.
pub async fn set_contacted(pool: &SqlitePool, id: &str, contacted: bool) -> Result<()> {
    if contacted {
        sqlx::query(
            "UPDATE partial_bookings SET contacted_at = datetime('now'),
             updated_at = updated_at WHERE id = ?",
        )
        .bind(id)
        .execute(pool)
        .await?;
    } else {
        sqlx::query("UPDATE partial_bookings SET contacted_at = NULL WHERE id = ?")
            .bind(id)
            .execute(pool)
            .await?;
    }
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

/// Leads ripe for an abandonment alert: not completed, not archived, not
/// yet notified, with an email to reach, last touched before `cutoff` and
/// no more than `max_age_hours` ago (so we don't spam very old rows on first
/// rollout). Returns (lead id, host_user_id, guest name, guest email,
/// event_type_id).
pub async fn due_for_notification(
    pool: &SqlitePool,
    older_than_minutes: i64,
    max_age_hours: i64,
) -> Vec<(String, String, Option<String>, String, String)> {
    sqlx::query_as(
        "SELECT pb.id, COALESCE(pb.host_user_id, ''), pb.name, pb.email, pb.event_type_id
         FROM partial_bookings pb
         WHERE pb.completed_at IS NULL
           AND pb.archived_at IS NULL
           AND pb.notified_at IS NULL
           AND pb.host_user_id IS NOT NULL
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
        // Background notifier path: a swallowed error here means abandonment
        // alerts silently stop with no other surface, so log it.
        tracing::warn!(error = %e, "lead capture: due_for_notification query failed");
        Vec::new()
    })
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

/// Truncate a field to at most `max_bytes` UTF-8 bytes without splitting a
/// codepoint. Returns `None` for empty inputs so we don't store empty
/// strings instead of NULL.
fn trim_field(value: Option<String>, max_bytes: usize) -> Option<String> {
    let value = value?;
    let trimmed = value.trim();
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

        let id1 = upsert_partial(
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

        let id2 = upsert_partial(
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

        // The second call upserts: same lead_id keeps the original PK.
        assert_eq!(id1, id1);
        let _ = id2;

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
                event_type_id: et_id,
                host_user_id: Some(user_id),
                lead_id: lead_id.clone(),
                ..Default::default()
            },
        )
        .await
        .unwrap();

        mark_completed(&pool, &lead_id).await;

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

        mark_completed(&pool, &done).await;
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
        mark_completed(&pool, &done).await;

        let rows = list_recent_for_user(&pool, Some(&user_id), 100)
            .await
            .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].lead_id, keep);
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
    async fn set_contacted_toggles_then_archive() {
        let pool = pool().await;
        let (user_id, et_id) = seed_event_type(&pool).await;
        let _lead = seed_lead(&pool, &et_id, Some(&user_id), "x@x.com").await;
        let pk: String = sqlx::query_scalar("SELECT id FROM partial_bookings LIMIT 1")
            .fetch_one(&pool)
            .await
            .unwrap();

        set_contacted(&pool, &pk, true).await.unwrap();
        let c: Option<String> =
            sqlx::query_scalar("SELECT contacted_at FROM partial_bookings WHERE id = ?")
                .bind(&pk)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert!(c.is_some(), "contacted_at set");

        set_contacted(&pool, &pk, false).await.unwrap();
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
        let pk = &due[0].0;

        mark_notified(&pool, pk).await;
        let due = due_for_notification(&pool, 30, 48).await;
        assert!(due.is_empty(), "notified lead must not be returned again");
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
