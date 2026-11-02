//! Background loop: purge expired leads and alert hosts about abandoned ones.

use sqlx::SqlitePool;
use tokio::time::{sleep, Duration};

/// Leads older than this are not alerted on, so a first rollout does not
/// flood a host with a backlog.
const ABANDON_MAX_AGE_HOURS: i64 = 48;
/// Most alerts one host gets in an hour. The capture endpoint is public, so
/// without this a script could turn a host's inbox into its own mailing target.
const MAX_ALERTS_PER_HOST_PER_HOUR: i64 = 10;
/// Leads read per query, and how many queries one pass makes at most, so a
/// pile of leads that cannot be sent yet does not hide the ones that can.
const PAGE: i64 = 50;
const MAX_PAGES_PER_PASS: i64 = 10;

/// Runs for the life of `calrs serve`. The abandonment check runs every five
/// minutes; the purge once at start-up and then every six hours.
pub async fn run_lead_purge_loop(pool: SqlitePool, secret_key: [u8; 32]) {
    let mut last_purge: Option<std::time::Instant> = None;
    let mut clock = DigestClock::new();
    loop {
        if last_purge.is_none_or(|at| at.elapsed() >= Duration::from_secs(6 * 60 * 60)) {
            purge(&pool).await;
            last_purge = Some(std::time::Instant::now());
        }
        notify_abandoned(&pool, &secret_key, &mut clock).await;
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

/// When each recipient was last sent a digest. In memory, so a restart can send
/// one a little early; nothing is lost, the leads simply wait.
pub(in crate::web) type DigestClock = std::collections::HashMap<String, std::time::Instant>;

/// A lead waiting to be told, with what the email needs.
struct Pending {
    lead: crate::leads::db::DueLead,
    event_title: String,
    recipients: Vec<Recipient>,
}

/// Best-effort: an SMTP failure leaves the lead un-notified for the next pass.
pub(in crate::web) async fn notify_abandoned(
    pool: &SqlitePool,
    secret_key: &[u8; 32],
    clock: &mut DigestClock,
) {
    let settings = crate::leads::config::global_settings(pool).await;
    // No alerts while the admin switch is off.
    if !settings.enabled {
        return;
    }
    // Per-lead mode leaves out hosts already at their hourly cap; a digest has
    // its own window, so it asks for everything and skips the early ones below.
    let host_cap = if settings.digest_minutes == 0 {
        MAX_ALERTS_PER_HOST_PER_HOUR
    } else {
        i64::MAX
    };
    let mut due = Vec::new();
    for page in 0..MAX_PAGES_PER_PASS {
        let batch = crate::leads::due_for_notification(
            pool,
            settings.alert_after_minutes,
            ABANDON_MAX_AGE_HOURS,
            host_cap,
            PAGE,
            page * PAGE,
        )
        .await;
        let last = (batch.len() as i64) < PAGE;
        due.extend(batch);
        if last {
            break;
        }
    }
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

    let mut pending = Vec::new();
    for lead in due {
        let recipients = recipients(pool, &lead).await;
        // Nobody to tell, or an address not worth mailing about: settle it so
        // it is not looked at again.
        if recipients.is_empty() || !crate::leads::is_plain_email(&lead.email) {
            crate::leads::mark_notified(pool, &lead.id).await;
            continue;
        }
        let event_title: String = sqlx::query_scalar("SELECT title FROM event_types WHERE id = ?")
            .bind(&lead.event_type_id)
            .fetch_optional(pool)
            .await
            .unwrap_or(None)
            .unwrap_or_default();
        pending.push(Pending {
            lead,
            event_title,
            recipients,
        });
    }

    if settings.digest_minutes == 0 {
        one_email_per_lead(pool, &smtp, &pending, leads_url.as_deref()).await;
    } else {
        digests(
            pool,
            &smtp,
            &pending,
            settings.digest_minutes,
            clock,
            leads_url.as_deref(),
        )
        .await;
    }
}

async fn one_email_per_lead(
    pool: &SqlitePool,
    smtp: &crate::email::SmtpConfig,
    pending: &[Pending],
    leads_url: Option<&str>,
) {
    for item in pending {
        if crate::leads::alerts_sent_last_hour(pool, &item.lead.cap_key).await
            >= MAX_ALERTS_PER_HOST_PER_HOUR
        {
            continue;
        }
        let mut sent = false;
        for host in &item.recipients {
            let alert = crate::email::LeadAlert {
                host_name: &host.name,
                host_email: &host.email,
                host_language: host.language.as_deref(),
                event_title: &item.event_title,
                guest_name: item.lead.name.as_deref(),
                guest_email: &item.lead.email,
                guest_phone: item.lead.phone.as_deref(),
                leads_url,
            };
            match crate::email::send_lead_abandoned_alert(smtp, &alert).await {
                Ok(()) => sent = true,
                Err(e) => {
                    tracing::warn!(lead = %item.lead.id, error = %e, "lead capture: alert failed")
                }
            }
        }
        if sent {
            crate::leads::mark_notified(pool, &item.lead.id).await;
            tracing::info!(lead = %item.lead.id, "lead capture: abandonment alert sent");
        }
    }
}

/// One email per recipient listing what piled up, at most once per `minutes`.
/// A recipient inside their window keeps their leads waiting.
async fn digests(
    pool: &SqlitePool,
    smtp: &crate::email::SmtpConfig,
    pending: &[Pending],
    minutes: i64,
    clock: &mut DigestClock,
    leads_url: Option<&str>,
) {
    let window = std::time::Duration::from_secs(minutes as u64 * 60);
    let mut by_recipient: Vec<(&Recipient, Vec<&Pending>)> = Vec::new();
    for item in pending {
        for host in &item.recipients {
            match by_recipient.iter_mut().find(|(r, _)| r.email == host.email) {
                Some((_, leads)) => leads.push(item),
                None => by_recipient.push((host, vec![item])),
            }
        }
    }
    for (host, leads) in by_recipient {
        if clock
            .get(&host.email)
            .is_some_and(|last| last.elapsed() < window)
        {
            continue;
        }
        let items = leads
            .iter()
            .take(crate::email::DIGEST_MAX_ITEMS)
            .map(|p| crate::email::DigestItem {
                event_title: &p.event_title,
                guest_name: p.lead.name.as_deref(),
                guest_email: &p.lead.email,
                guest_phone: p.lead.phone.as_deref(),
            })
            .collect();
        let digest = crate::email::LeadDigest {
            host_name: &host.name,
            host_email: &host.email,
            host_language: host.language.as_deref(),
            items,
            total: leads.len(),
            leads_url,
        };
        match crate::email::send_lead_digest(smtp, &digest).await {
            Ok(()) => {
                clock.insert(host.email.clone(), std::time::Instant::now());
                for p in &leads {
                    crate::leads::mark_notified(pool, &p.lead.id).await;
                }
                tracing::info!(count = leads.len(), "lead capture: digest sent");
            }
            Err(e) => tracing::warn!(error = %e, "lead capture: digest failed"),
        }
    }
}

#[derive(Clone)]
struct Recipient {
    name: String,
    email: String,
    language: Option<String>,
}

/// Who is told about a lead: the user it belongs to. On a team event type that
/// user is whoever created it, so they are only told while still on the team;
/// otherwise the team's admins are.
async fn recipients(pool: &SqlitePool, lead: &crate::leads::db::DueLead) -> Vec<Recipient> {
    const COLUMNS: &str = "u.name, COALESCE(u.booking_email, u.email), u.language";
    let team: Option<String> = sqlx::query_scalar("SELECT team_id FROM event_types WHERE id = ?")
        .bind(&lead.event_type_id)
        .fetch_optional(pool)
        .await
        .unwrap_or(None)
        .flatten();
    // A combined page names several users: all of them are told.
    let participants: Vec<(String, String, Option<String>)> = sqlx::query_as(&format!(
        "SELECT {COLUMNS} FROM users u
         JOIN partial_booking_participants pp ON pp.user_id = u.id
         WHERE pp.lead_id = ? AND u.enabled = 1"
    ))
    .bind(&lead.id)
    .fetch_all(pool)
    .await
    .unwrap_or_default();
    let found: Result<Vec<(String, String, Option<String>)>, sqlx::Error> =
        if !participants.is_empty() {
            Ok(participants)
        } else {
            match team {
                None => {
                    sqlx::query_as(&format!(
                        "SELECT {COLUMNS} FROM users u WHERE u.id = ? AND u.enabled = 1"
                    ))
                    .bind(&lead.host_user_id)
                    .fetch_all(pool)
                    .await
                }
                Some(team_id) => {
                    let creator = sqlx::query_as(&format!(
                        "SELECT {COLUMNS} FROM users u
                 JOIN team_members tm ON tm.user_id = u.id
                 WHERE tm.team_id = ? AND u.id = ? AND u.enabled = 1"
                    ))
                    .bind(&team_id)
                    .bind(&lead.host_user_id)
                    .fetch_all(pool)
                    .await;
                    match creator {
                        Ok(rows) if !rows.is_empty() => Ok(rows),
                        _ => {
                            sqlx::query_as(&format!(
                                "SELECT {COLUMNS} FROM users u
                         JOIN team_members tm ON tm.user_id = u.id
                         WHERE tm.team_id = ? AND tm.role = 'admin' AND u.enabled = 1"
                            ))
                            .bind(&team_id)
                            .fetch_all(pool)
                            .await
                        }
                    }
                }
            }
        };
    let rows = found.unwrap_or_else(|e| {
        tracing::warn!(error = %e, "lead capture: could not look up who to alert");
        Vec::new()
    });
    rows.into_iter()
        .filter(|(_, email, _)| !email.is_empty())
        .map(|(name, email, language)| Recipient {
            name,
            email,
            language,
        })
        .collect()
}
