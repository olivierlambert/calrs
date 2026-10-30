//! Background loop: purge expired leads and alert hosts about abandoned ones.

use sqlx::SqlitePool;
use tokio::time::{sleep, Duration};

/// How long a lead must sit untouched before the host is alerted.
const ABANDON_AFTER_MIN: i64 = 30;
/// Leads older than this are not alerted on, so a first rollout does not
/// flood a host with a backlog.
const ABANDON_MAX_AGE_HOURS: i64 = 48;
/// Most alerts one host gets in an hour. The capture endpoint is public, so
/// without this a script could turn a host's inbox into its own mailing target.
const MAX_ALERTS_PER_HOST_PER_HOUR: i64 = 10;

/// Runs for the life of `calrs serve`. The abandonment check runs every five
/// minutes; the purge once at start-up and then every six hours.
pub async fn run_lead_purge_loop(pool: SqlitePool, secret_key: [u8; 32]) {
    let mut last_purge: Option<std::time::Instant> = None;
    loop {
        if last_purge.is_none_or(|at| at.elapsed() >= Duration::from_secs(6 * 60 * 60)) {
            purge(&pool).await;
            last_purge = Some(std::time::Instant::now());
        }
        notify_abandoned(&pool, &secret_key).await;
        sleep(Duration::from_secs(5 * 60)).await;
    }
}

async fn purge(pool: &SqlitePool) {
    let retention = crate::leads::retention_days(pool).await;
    match crate::leads::purge_expired(pool, retention).await {
        Ok(0) => {}
        Ok(count) => tracing::info!(count, retention_days = retention, "lead capture: purged"),
        Err(e) => tracing::warn!(error = %e, "lead capture: purge failed"),
    }
}

/// A guest address worth mailing the host about: one `@`, no whitespace or
/// control characters.
fn plausible_email(email: &str) -> bool {
    let email = email.trim();
    email.matches('@').count() == 1
        && !email.starts_with('@')
        && !email.ends_with('@')
        && !email.chars().any(|c| c.is_whitespace() || c.is_control())
}

/// Best-effort: an SMTP failure leaves the lead un-notified for the next pass.
pub(in crate::web) async fn notify_abandoned(pool: &SqlitePool, secret_key: &[u8; 32]) {
    // No alerts while the admin switch is off.
    if !crate::leads::config::global_settings(pool).await.enabled {
        return;
    }
    let due =
        crate::leads::due_for_notification(pool, ABANDON_AFTER_MIN, ABANDON_MAX_AGE_HOURS).await;
    if due.is_empty() {
        return;
    }
    let smtp = match crate::email::load_smtp_config(pool, secret_key).await {
        Ok(Some(cfg)) => cfg,
        Ok(None) => return,
        Err(e) => {
            tracing::warn!(error = %e, "lead capture: could not load the SMTP settings");
            return;
        }
    };
    let leads_url = crate::settings::base_url()
        .map(|base| format!("{}/dashboard/leads", base.trim_end_matches('/')));

    for lead in due {
        let host: Option<(String, String, Option<String>)> = sqlx::query_as(
            "SELECT name, COALESCE(booking_email, email), language FROM users
             WHERE id = ? AND enabled = 1",
        )
        .bind(&lead.host_user_id)
        .fetch_optional(pool)
        .await
        .unwrap_or(None);
        let Some((host_name, host_email, host_language)) =
            host.filter(|(_, email, _)| !email.is_empty())
        else {
            // Nobody to tell: settle it so it is not retried for ever.
            crate::leads::mark_notified(pool, &lead.id).await;
            continue;
        };
        if !plausible_email(&lead.email) {
            crate::leads::mark_notified(pool, &lead.id).await;
            continue;
        }
        if crate::leads::alerts_sent_last_hour(pool, &lead.host_user_id).await
            >= MAX_ALERTS_PER_HOST_PER_HOUR
        {
            continue;
        }
        let event_title: String = sqlx::query_scalar("SELECT title FROM event_types WHERE id = ?")
            .bind(&lead.event_type_id)
            .fetch_optional(pool)
            .await
            .unwrap_or(None)
            .unwrap_or_default();

        let alert = crate::email::LeadAlert {
            host_name: &host_name,
            host_email: &host_email,
            host_language: host_language.as_deref(),
            event_title: &event_title,
            guest_name: lead.name.as_deref(),
            guest_email: &lead.email,
            guest_phone: lead.phone.as_deref(),
            leads_url: leads_url.as_deref(),
        };
        match crate::email::send_lead_abandoned_alert(&smtp, &alert).await {
            Ok(()) => {
                crate::leads::mark_notified(pool, &lead.id).await;
                tracing::info!(lead = %lead.id, "lead capture: abandonment alert sent");
            }
            Err(e) => tracing::warn!(lead = %lead.id, error = %e, "lead capture: alert failed"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::plausible_email;

    #[test]
    fn only_a_single_clean_address_is_mailed() {
        assert!(plausible_email("pat@example.com"));
        assert!(!plausible_email("pat@"));
        assert!(!plausible_email("@example.com"));
        assert!(!plausible_email("a@b@c.com"));
        assert!(!plausible_email("pat@example.com, boss@example.com"));
        assert!(!plausible_email("pat@exa\nmple.com"));
    }
}
