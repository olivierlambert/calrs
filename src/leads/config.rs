//! Read the global + per-event-type gating flags that decide whether lead
//! capture is active.

use anyhow::Result;
use sqlx::SqlitePool;

/// Default retention window if `auth_config.lead_retention_days` isn't set
/// (or is zero, which we treat as "use default").
pub const GLOBAL_DEFAULT_RETENTION_DAYS: i64 = 30;

#[derive(Debug, Clone, Copy)]
pub struct GlobalSettings {
    pub enabled: bool,
    pub retention_days: i64,
}

/// Load the global flags. Falls back to (`disabled`, `30 days`) when the
/// `auth_config` row is missing or when the columns are out of range.
pub async fn global_settings(pool: &SqlitePool) -> GlobalSettings {
    let row: Option<(i64, i64)> = sqlx::query_as(
        "SELECT lead_capture_enabled, lead_retention_days FROM auth_config WHERE id = 'singleton'",
    )
    .fetch_optional(pool)
    .await
    .unwrap_or(None);

    match row {
        Some((enabled, retention)) => GlobalSettings {
            enabled: enabled != 0,
            retention_days: if retention > 0 {
                retention
            } else {
                GLOBAL_DEFAULT_RETENTION_DAYS
            },
        },
        None => GlobalSettings {
            enabled: false,
            retention_days: GLOBAL_DEFAULT_RETENTION_DAYS,
        },
    }
}

/// Update the global switch and retention window from the admin panel.
/// `retention_days` is clamped to 1..=365 so the purge stays meaningful.
pub async fn set_global_settings(
    pool: &SqlitePool,
    enabled: bool,
    retention_days: i64,
) -> Result<()> {
    let result = sqlx::query(
        "UPDATE auth_config SET lead_capture_enabled = ?, lead_retention_days = ?,
         updated_at = datetime('now') WHERE id = 'singleton'",
    )
    .bind(enabled)
    .bind(retention_days.clamp(1, 365))
    .execute(pool)
    .await?;
    if result.rows_affected() == 0 {
        anyhow::bail!("auth_config has no singleton row");
    }
    Ok(())
}

/// Per-event-type flag.
pub async fn event_type_capture_enabled(pool: &SqlitePool, event_type_id: &str) -> bool {
    let row: Option<(i64,)> = sqlx::query_as("SELECT lead_capture FROM event_types WHERE id = ?")
        .bind(event_type_id)
        .fetch_optional(pool)
        .await
        .unwrap_or(None);
    row.map(|(v,)| v != 0).unwrap_or(false)
}

/// Whether the event type collects a guest phone number for SMS
/// (`sms_phone_mode` other than `off`). Lead capture reuses that setting to
/// decide if its gate asks for a number.
pub async fn event_type_collects_phone(pool: &SqlitePool, event_type_id: &str) -> bool {
    let mode: Option<String> =
        sqlx::query_scalar("SELECT sms_phone_mode FROM event_types WHERE id = ?")
            .bind(event_type_id)
            .fetch_optional(pool)
            .await
            .unwrap_or(None);
    mode.is_some_and(|m| crate::sms::collects_phone(&m))
}

/// Convenience: is capture active for this event type *right now*? Returns
/// `true` only when both the admin global toggle and the host's per-event
/// toggle are on.
pub async fn is_capture_active(pool: &SqlitePool, event_type_id: &str) -> bool {
    let g = global_settings(pool).await;
    if !g.enabled {
        return false;
    }
    event_type_capture_enabled(pool, event_type_id).await
}

/// Pure helper used by the auto-purge job — exposed here for the timer task.
pub async fn retention_days(pool: &SqlitePool) -> i64 {
    global_settings(pool).await.retention_days
}
