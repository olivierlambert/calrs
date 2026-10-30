//! The per-event-type toggle, saved together with the event type form.

use minijinja::context;
use sqlx::SqlitePool;

/// Why a toggle was refused.
#[derive(Debug, PartialEq, Eq)]
pub(in crate::web) enum ToggleError {
    /// Turning capture on without ever having confirmed that bookers are told.
    NotAcknowledged,
    /// The row could not be written.
    Database,
}

impl ToggleError {
    /// Fluent key of the message shown above the form.
    pub(in crate::web) fn message_key(&self) -> &'static str {
        match self {
            ToggleError::NotAcknowledged => "event-type-form-lead-capture-ack-required",
            ToggleError::Database => "event-type-form-lead-capture-save-failed",
        }
    }
}

/// Save the lead-capture toggle of an event type.
///
/// Enabling needs the host's one-time acknowledgement that bookers are
/// informed (GDPR); `acknowledging_now` is the ticked checkbox of this very
/// submission. The first acknowledgement stamps `lead_capture_acknowledged_at`
/// and later saves never overwrite it, so the audit trail stays intact.
pub(in crate::web) async fn apply_toggle(
    pool: &SqlitePool,
    event_type_id: &str,
    want_on: bool,
    acknowledging_now: bool,
) -> Result<(), ToggleError> {
    let acknowledged: Option<String> = sqlx::query_scalar::<_, Option<String>>(
        "SELECT lead_capture_acknowledged_at FROM event_types WHERE id = ?",
    )
    .bind(event_type_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| {
        tracing::warn!(error = %e, "lead capture: could not read the acknowledgement");
        ToggleError::Database
    })?
    .flatten();

    if want_on && acknowledged.is_none() && !acknowledging_now {
        return Err(ToggleError::NotAcknowledged);
    }
    let result = sqlx::query(
        "UPDATE event_types
         SET lead_capture = ?,
             lead_capture_acknowledged_at = COALESCE(
                 lead_capture_acknowledged_at,
                 CASE WHEN ? THEN datetime('now') END)
         WHERE id = ?",
    )
    .bind(want_on)
    .bind(want_on && acknowledging_now)
    .bind(event_type_id)
    .execute(pool)
    .await
    .map_err(|e| {
        tracing::warn!(error = %e, "lead capture: could not save the toggle");
        ToggleError::Database
    })?;
    if result.rows_affected() == 0 {
        return Err(ToggleError::Database);
    }
    Ok(())
}

/// The `lead` value the event type form renders with: the admin switch, the
/// retention window, the saved toggle and the date it was acknowledged.
/// `event_type_id` is `None` on the form that creates an event type, where
/// nothing is saved yet. `posted` is the checkbox of a submission being shown
/// again after an error, so the host does not lose what they ticked.
pub(in crate::web) async fn form_ctx(
    pool: &SqlitePool,
    event_type_id: Option<&str>,
    posted: Option<bool>,
) -> minijinja::Value {
    let global = crate::leads::config::global_settings(pool).await;
    let row: Option<(i64, Option<String>)> = match event_type_id {
        Some(id) => sqlx::query_as(
            "SELECT lead_capture, lead_capture_acknowledged_at FROM event_types WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(pool)
        .await
        .unwrap_or(None),
        None => None,
    };
    let (enabled, acknowledged_at) = row.unwrap_or((0, None));
    context! {
        global_enabled => global.enabled,
        retention_days => global.retention_days,
        enabled => posted.unwrap_or(enabled != 0),
        acknowledged_on => acknowledged_at.as_deref().map(ack_date).unwrap_or_default(),
    }
}

/// A SQLite `datetime('now')` value as a bare `YYYY-MM-DD`. Anything in
/// another format is shown as stored rather than dropped.
fn ack_date(raw: &str) -> String {
    chrono::NaiveDateTime::parse_from_str(raw.trim(), "%Y-%m-%d %H:%M:%S")
        .map(|dt| dt.format("%Y-%m-%d").to_string())
        .unwrap_or_else(|_| raw.to_string())
}
