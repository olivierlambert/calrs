//! Read the global + per-event-type gating flags that decide whether lead
//! capture is active.

use anyhow::Result;
use sqlx::SqlitePool;

/// Default retention window if `auth_config.lead_retention_days` isn't set
/// (or is zero, which we treat as "use default").
pub const GLOBAL_DEFAULT_RETENTION_DAYS: i64 = 30;

/// Minutes a guest stays silent before the host is told, unless set otherwise.
pub const DEFAULT_ALERT_AFTER_MINUTES: i64 = 30;

#[derive(Debug, Clone, Copy)]
pub struct GlobalSettings {
    pub enabled: bool,
    pub retention_days: i64,
    /// Blank the personal details of a lead once the guest has booked.
    pub anonymize_on_booking: bool,
    /// Silence, in minutes, after which a lead counts as abandoned.
    pub alert_after_minutes: i64,
    /// 0 sends one email per lead; N sends one per host every N minutes.
    pub digest_minutes: i64,
}

impl Default for GlobalSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            retention_days: GLOBAL_DEFAULT_RETENTION_DAYS,
            anonymize_on_booking: false,
            alert_after_minutes: DEFAULT_ALERT_AFTER_MINUTES,
            digest_minutes: 0,
        }
    }
}

/// Load the global settings. Falls back to the defaults (capture off) when the
/// `auth_config` row is missing or a column is out of range.
pub async fn global_settings(pool: &SqlitePool) -> GlobalSettings {
    let row: Option<(i64, i64, i64, i64, i64)> = sqlx::query_as(
        "SELECT lead_capture_enabled, lead_retention_days, lead_anonymize_on_booking,
                lead_alert_after_minutes, lead_digest_minutes
         FROM auth_config WHERE id = 'singleton'",
    )
    .fetch_optional(pool)
    .await
    .unwrap_or(None);
    let Some((enabled, retention, anonymize, alert_after, digest)) = row else {
        return GlobalSettings::default();
    };
    GlobalSettings {
        enabled: enabled != 0,
        retention_days: if retention > 0 {
            retention
        } else {
            GLOBAL_DEFAULT_RETENTION_DAYS
        },
        anonymize_on_booking: anonymize != 0,
        alert_after_minutes: if alert_after > 0 {
            alert_after
        } else {
            DEFAULT_ALERT_AFTER_MINUTES
        },
        digest_minutes: digest.max(0),
    }
}

/// Save the admin card. Every number is clamped to a range that keeps the
/// feature meaningful: retention 1..=365 days, silence 5..=1440 minutes, digest
/// 0..=1440 minutes.
pub async fn set_global_settings(pool: &SqlitePool, settings: GlobalSettings) -> Result<()> {
    let result = sqlx::query(
        "UPDATE auth_config SET lead_capture_enabled = ?, lead_retention_days = ?,
                lead_anonymize_on_booking = ?, lead_alert_after_minutes = ?,
                lead_digest_minutes = ?, updated_at = datetime('now')
         WHERE id = 'singleton'",
    )
    .bind(settings.enabled)
    .bind(settings.retention_days.clamp(1, 365))
    .bind(settings.anonymize_on_booking)
    .bind(settings.alert_after_minutes.clamp(5, 1440))
    .bind(settings.digest_minutes.clamp(0, 1440))
    .execute(pool)
    .await?;
    if result.rows_affected() == 0 {
        anyhow::bail!("auth_config has no singleton row");
    }
    Ok(())
}

/// Whether the event type's own toggle is on *and* its public page can be
/// recorded at all: an enabled, public event type, and for a team event type a
/// public team (a private team's pages need an invite, which an anonymous
/// request to the capture endpoint would otherwise bypass).
pub async fn event_type_capture_enabled(pool: &SqlitePool, event_type_id: &str) -> bool {
    let row: Option<(i64,)> = sqlx::query_as(
        "SELECT 1 FROM event_types et
         LEFT JOIN teams t ON t.id = et.team_id
         WHERE et.id = ? AND et.lead_capture = 1 AND et.enabled = 1
           AND et.visibility = 'public'
           AND (et.team_id IS NULL OR t.visibility = 'public')",
    )
    .bind(event_type_id)
    .fetch_optional(pool)
    .await
    .unwrap_or(None);
    row.is_some()
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
