use anyhow::Result;
use chrono::Utc;
use colored::Colorize;
use sqlx::SqlitePool;
use uuid::Uuid;

use crate::caldav::CaldavClient;
use crate::utils::{extract_vevent_field, extract_vevent_tzid, split_vevents};

/// Default staleness threshold: 5 minutes
const STALE_SECS: i64 = 300;

pub async fn run(pool: &SqlitePool, key: &[u8; 32], _full: bool) -> Result<()> {
    let sources: Vec<(String, String, String, String, String)> = sqlx::query_as(
        "SELECT id, name, url, username, password_enc FROM caldav_sources WHERE enabled = 1",
    )
    .fetch_all(pool)
    .await?;

    if sources.is_empty() {
        println!("No sources configured. Add one with `calrs source add`.");
        return Ok(());
    }

    for (source_id, name, url, username, password_enc) in &sources {
        println!("{} Syncing '{}'…", "…".dimmed(), name);

        let password = crate::crypto::decrypt_password(key, password_enc)?;
        let client = CaldavClient::new(url, username, &password);

        if let Err(e) = sync_source(pool, &client, source_id, None).await {
            println!("  {} Sync failed: {}", "✗".red(), e);
            continue;
        }
    }

    println!("{} Sync complete.", "✓".green());
    Ok(())
}

/// Sync a single CalDAV source: discover calendars and fetch events.
/// If `since_utc` is provided, uses time-range filter for incremental sync.
pub async fn sync_source(
    pool: &SqlitePool,
    client: &CaldavClient,
    source_id: &str,
    since_utc: Option<&str>,
) -> Result<()> {
    let principal = client.discover_principal().await?;
    let calendar_home = client.discover_calendar_home(&principal).await?;
    let calendars = client.list_calendars(&calendar_home).await?;

    for cal_info in &calendars {
        // Upsert calendar
        let cal_id: String = match sqlx::query_scalar::<_, String>(
            "SELECT id FROM calendars WHERE source_id = ? AND href = ?",
        )
        .bind(source_id)
        .bind(&cal_info.href)
        .fetch_optional(pool)
        .await?
        {
            Some(id) => id,
            None => {
                let id = Uuid::new_v4().to_string();
                sqlx::query(
                    "INSERT INTO calendars (id, source_id, href, display_name, color, ctag) VALUES (?, ?, ?, ?, ?, ?)",
                )
                .bind(&id)
                .bind(source_id)
                .bind(&cal_info.href)
                .bind(&cal_info.display_name)
                .bind(&cal_info.color)
                .bind(&cal_info.ctag)
                .execute(pool)
                .await?;
                id
            }
        };

        let cal_label = cal_info.display_name.as_deref().unwrap_or(&cal_info.href);

        // Fetch events (with time-range if available)
        let raw_events = match since_utc {
            Some(since) => client.fetch_events_since(&cal_info.href, since).await,
            None => client.fetch_events(&cal_info.href).await,
        };

        match raw_events {
            Ok(raw_events) => {
                let mut count = 0;
                let mut seen_uids: Vec<(String, String)> = Vec::new();

                for raw in &raw_events {
                    let vevent_blocks = split_vevents(&raw.ical_data);

                    for vevent in &vevent_blocks {
                        let uid = extract_vevent_field(vevent, "UID")
                            .unwrap_or_else(|| Uuid::new_v4().to_string());
                        let summary = extract_vevent_field(vevent, "SUMMARY");
                        let start_at = extract_vevent_field(vevent, "DTSTART").unwrap_or_default();
                        let end_at = extract_vevent_field(vevent, "DTEND").unwrap_or_default();
                        let location = extract_vevent_field(vevent, "LOCATION");
                        let description = extract_vevent_field(vevent, "DESCRIPTION");
                        let status = extract_vevent_field(vevent, "STATUS");
                        let rrule = extract_vevent_field(vevent, "RRULE");
                        let recurrence_id = extract_vevent_field(vevent, "RECURRENCE-ID");
                        let timezone = extract_vevent_tzid(vevent, "DTSTART");

                        seen_uids.push((uid.clone(), recurrence_id.clone().unwrap_or_default()));

                        let event_id = Uuid::new_v4().to_string();

                        sqlx::query(
                            "INSERT INTO events (id, calendar_id, uid, summary, start_at, end_at, location, description, status, rrule, raw_ical, recurrence_id, timezone)
                             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                             ON CONFLICT(calendar_id, uid, COALESCE(recurrence_id, '')) DO UPDATE SET
                               summary = excluded.summary,
                               start_at = excluded.start_at,
                               end_at = excluded.end_at,
                               location = excluded.location,
                               description = excluded.description,
                               status = excluded.status,
                               rrule = excluded.rrule,
                               raw_ical = excluded.raw_ical,
                               recurrence_id = excluded.recurrence_id,
                               timezone = excluded.timezone,
                               synced_at = datetime('now')",
                        )
                        .bind(&event_id)
                        .bind(&cal_id)
                        .bind(&uid)
                        .bind(&summary)
                        .bind(&start_at)
                        .bind(&end_at)
                        .bind(&location)
                        .bind(&description)
                        .bind(&status)
                        .bind(&rrule)
                        .bind(&raw.ical_data)
                        .bind(&recurrence_id)
                        .bind(&timezone)
                        .execute(pool)
                        .await?;

                        count += 1;
                    }
                }

                // On full sync, remove events that no longer exist on the server
                if since_utc.is_none() && !seen_uids.is_empty() {
                    let local_events: Vec<(String, String, Option<String>)> = sqlx::query_as(
                        "SELECT id, uid, recurrence_id FROM events WHERE calendar_id = ?",
                    )
                    .bind(&cal_id)
                    .fetch_all(pool)
                    .await
                    .unwrap_or_default();

                    let mut deleted = 0u32;
                    for (event_id, uid, recurrence_id) in &local_events {
                        let rec_id = recurrence_id.clone().unwrap_or_default();
                        if !seen_uids.iter().any(|(u, r)| u == uid && r == &rec_id) {
                            let _ = sqlx::query("DELETE FROM events WHERE id = ?")
                                .bind(event_id)
                                .execute(pool)
                                .await;
                            deleted += 1;
                        }
                    }
                    if deleted > 0 {
                        tracing::info!(
                            calendar_name = cal_label,
                            stale_events_removed = deleted,
                            "removed stale events from local cache"
                        );
                    }
                }

                println!(
                    "  {} {} — {} event(s) synced",
                    "✓".green(),
                    cal_label,
                    count
                );
            }
            Err(e) => {
                println!("  {} {} — failed: {}", "✗".red(), cal_label, e);
            }
        }
    }

    // Update last_synced
    sqlx::query("UPDATE caldav_sources SET last_synced = datetime('now') WHERE id = ?")
        .bind(source_id)
        .execute(pool)
        .await?;

    tracing::info!(source_id = %source_id, "CalDAV sync completed");

    Ok(())
}

/// Sync calendars for a user if any of their sources are stale (last_synced > STALE_SECS ago).
/// Uses time-range filter to only fetch future events (with 1-day lookback for ongoing events).
/// Silently skips on errors (best-effort for guest-facing pages).
pub async fn sync_if_stale(pool: &SqlitePool, key: &[u8; 32], user_id: &str) {
    let cutoff = Utc::now() - chrono::Duration::seconds(STALE_SECS);
    // Must match SQLite datetime('now') format: "YYYY-MM-DD HH:MM:SS" (space, not T)
    let cutoff_str = cutoff.format("%Y-%m-%d %H:%M:%S").to_string();

    let stale_sources: Vec<(String, String, String, String)> = sqlx::query_as(
        "SELECT cs.id, cs.url, cs.username, cs.password_enc
         FROM caldav_sources cs
         JOIN accounts a ON a.id = cs.account_id
         WHERE a.user_id = ? AND cs.enabled = 1
           AND (cs.last_synced IS NULL OR cs.last_synced < ?)",
    )
    .bind(user_id)
    .bind(&cutoff_str)
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    if stale_sources.is_empty() {
        return;
    }

    tracing::debug!(user_id = %user_id, "on-demand CalDAV sync triggered (stale >5min)");

    // Time-range: from 1 day ago (catch ongoing events) in UTC
    let since = (Utc::now() - chrono::Duration::days(1))
        .format("%Y%m%dT%H%M%SZ")
        .to_string();

    for (source_id, url, username, password_enc) in &stale_sources {
        let password = match crate::crypto::decrypt_password(key, password_enc) {
            Ok(p) => p,
            Err(_) => continue,
        };
        let client = CaldavClient::new(url, username, &password);
        let _ = sync_source(pool, &client, source_id, Some(&since)).await;
    }
}
