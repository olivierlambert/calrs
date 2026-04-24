use anyhow::Result;
use chrono::Utc;
use colored::Colorize;
use sqlx::SqlitePool;
use std::collections::HashMap;
use std::sync::{Arc, OnceLock};
use tokio::sync::Mutex;
use uuid::Uuid;

use crate::caldav::{CaldavClient, RawEvent};
use crate::utils::{extract_vevent_field, extract_vevent_tzid, split_vevents};

/// Default staleness threshold: 5 minutes
const STALE_SECS: i64 = 300;

/// Per-source async mutexes used by `sync_if_stale` to dedupe in-flight
/// syncs. Without this, concurrent on-demand calls (e.g. several booking
/// pages loading at once, each fanning out over team members) could stack
/// multiple full CalDAV fetches for the same source, which each hold the
/// server's full iCal response in memory until parsing completes.
static SOURCE_LOCKS: OnceLock<Mutex<HashMap<String, Arc<Mutex<()>>>>> = OnceLock::new();

fn source_locks() -> &'static Mutex<HashMap<String, Arc<Mutex<()>>>> {
    SOURCE_LOCKS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Get (or create) the dedup mutex for a given source.
pub(crate) async fn source_lock(source_id: &str) -> Arc<Mutex<()>> {
    let mut map = source_locks().lock().await;
    map.entry(source_id.to_string())
        .or_insert_with(|| Arc::new(Mutex::new(())))
        .clone()
}

pub async fn run(pool: &SqlitePool, key: &[u8; 32], full: bool) -> Result<()> {
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

        if full {
            // Clear sync tokens to force a full fetch
            let _ = sqlx::query(
                "UPDATE calendars SET sync_token = NULL, ctag = NULL WHERE source_id = ?",
            )
            .bind(source_id)
            .execute(pool)
            .await;
        }

        if let Err(e) = sync_source(pool, key, &client, source_id).await {
            println!("  {} Sync failed: {}", "✗".red(), e);
            continue;
        }
    }

    println!("{} Sync complete.", "✓".green());
    Ok(())
}

/// Sync a single CalDAV source: discover calendars and fetch events.
/// Uses ctag comparison to skip unchanged calendars.
/// Uses sync-token (RFC 6578) for delta sync when available, with fallback to full fetch.
pub async fn sync_source(
    pool: &SqlitePool,
    key: &[u8; 32],
    client: &CaldavClient,
    source_id: &str,
) -> Result<()> {
    let principal = client.discover_principal().await?;
    let calendar_home = client.discover_calendar_home(&principal).await?;
    let calendars = client.list_calendars(&calendar_home).await?;

    let mut did_full_sync = false;

    for cal_info in &calendars {
        // Upsert calendar and get stored state
        let (cal_id, stored_ctag, stored_sync_token) =
            upsert_calendar(pool, source_id, cal_info).await?;

        let cal_label = cal_info.display_name.as_deref().unwrap_or(&cal_info.href);

        // ctag comparison: skip if unchanged
        if let (Some(remote), Some(local)) = (&cal_info.ctag, &stored_ctag) {
            if remote == local {
                tracing::debug!(calendar = %cal_label, "ctag unchanged, skipping");
                println!("  {} {} — unchanged", "✓".green(), cal_label);
                continue;
            }
        }

        // Try sync-token delta if we have one stored
        let delta_ok = if let Some(token) = &stored_sync_token {
            match client.sync_collection(&cal_info.href, Some(token)).await {
                Ok(result) => {
                    // If ctag changed but sync-collection reports nothing, the server's
                    // sync-token implementation is incomplete (e.g. BlueMind doesn't report
                    // deletions). Fall through to full sync to catch the changes.
                    if result.changed.is_empty() && result.deleted_hrefs.is_empty() {
                        tracing::info!(
                            calendar = %cal_label,
                            "ctag changed but sync-collection returned empty delta, falling back to full sync"
                        );
                        false
                    } else {
                        let changed = upsert_raw_events(pool, &cal_id, &result.changed).await;
                        let deleted =
                            delete_events_by_href(pool, key, &cal_id, &result.deleted_hrefs).await;

                        // Store new sync-token and ctag
                        update_calendar_sync_state(
                            pool,
                            &cal_id,
                            &cal_info.ctag,
                            &result.new_sync_token,
                        )
                        .await;

                        tracing::info!(
                            calendar = %cal_label,
                            changed = changed,
                            deleted = deleted,
                            "delta sync completed"
                        );
                        println!(
                            "  {} {} — {} changed, {} deleted (delta)",
                            "✓".green(),
                            cal_label,
                            changed,
                            deleted
                        );
                        true
                    }
                }
                Err(e) => {
                    tracing::info!(
                        calendar = %cal_label,
                        error = %e,
                        "sync-token delta failed, falling back to full sync"
                    );
                    false
                }
            }
        } else {
            false
        };

        if !delta_ok {
            did_full_sync = true;
            // Full fetch fallback
            match client.fetch_events(&cal_info.href).await {
                Ok(raw_events) => {
                    let count = upsert_raw_events(pool, &cal_id, &raw_events).await;

                    // Remove events that no longer exist on the server
                    let deleted = remove_orphaned_events(pool, key, &cal_id, &raw_events).await;
                    if deleted > 0 {
                        tracing::info!(
                            calendar_name = cal_label,
                            stale_events_removed = deleted,
                            "removed stale events from local cache"
                        );
                    }

                    // Store sync-token from PROPFIND (if server provided one) or try to get one
                    let new_token = if cal_info.sync_token.is_some() {
                        cal_info.sync_token.clone()
                    } else {
                        // Try an empty sync-collection to get initial token
                        client
                            .sync_collection(&cal_info.href, None)
                            .await
                            .ok()
                            .and_then(|r| r.new_sync_token)
                    };
                    update_calendar_sync_state(pool, &cal_id, &cal_info.ctag, &new_token).await;

                    println!(
                        "  {} {} — {} event(s) synced{}",
                        "✓".green(),
                        cal_label,
                        count,
                        if deleted > 0 {
                            format!(", {} removed", deleted)
                        } else {
                            String::new()
                        }
                    );
                }
                Err(e) => {
                    println!("  {} {} — failed: {}", "✗".red(), cal_label, e);
                }
            }
        }
    }

    // Cancel any active bookings whose CalDAV event no longer exists.
    // This catches bookings orphaned before the cancellation code was deployed,
    // or edge cases where the event was deleted in a previous sync cycle.
    cancel_orphaned_bookings(pool, key, source_id).await;

    // Update last_synced (and last_full_sync if we did a full fetch)
    if did_full_sync {
        let _ =
            sqlx::query("UPDATE caldav_sources SET last_full_sync = datetime('now') WHERE id = ?")
                .bind(source_id)
                .execute(pool)
                .await;
    }
    sqlx::query("UPDATE caldav_sources SET last_synced = datetime('now') WHERE id = ?")
        .bind(source_id)
        .execute(pool)
        .await?;

    tracing::info!(source_id = %source_id, "CalDAV sync completed");

    Ok(())
}

/// Sync calendars for a user if any of their sources are stale (last_synced > STALE_SECS ago).
/// Uses sync-token delta when available, with fallback to full fetch.
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

    for (source_id, url, username, password_enc) in &stale_sources {
        // Serialize on-demand syncs per source. If another task is already
        // syncing this source, we wait, then re-check staleness — almost
        // always the winner bumped last_synced and we can skip.
        let lock = source_lock(source_id).await;
        let _guard = lock.lock().await;

        let last_synced: Option<String> = sqlx::query_scalar::<_, Option<String>>(
            "SELECT last_synced FROM caldav_sources WHERE id = ?",
        )
        .bind(source_id)
        .fetch_optional(pool)
        .await
        .unwrap_or(None)
        .flatten();
        if last_synced
            .as_deref()
            .is_some_and(|ls| ls >= cutoff_str.as_str())
        {
            tracing::debug!(
                source_id = %source_id,
                "skipping on-demand sync, another task already refreshed this source"
            );
            continue;
        }

        let password = match crate::crypto::decrypt_password(key, password_enc) {
            Ok(p) => p,
            Err(_) => continue,
        };
        let client = CaldavClient::new(url, username, &password);
        let _ = sync_source(pool, key, &client, source_id).await;
    }
}

/// Sync a single source by ID (for background sync loop).
/// Forces a full resync if last_full_sync is >24h ago (catches orphaned events).
pub async fn sync_source_by_id(pool: &SqlitePool, key: &[u8; 32], source_id: &str) {
    let source: Option<(String, String, String, Option<String>)> = sqlx::query_as(
        "SELECT url, username, password_enc, last_full_sync FROM caldav_sources WHERE id = ? AND enabled = 1",
    )
    .bind(source_id)
    .fetch_optional(pool)
    .await
    .unwrap_or(None);

    let Some((url, username, password_enc, last_full_sync)) = source else {
        return;
    };
    let password = match crate::crypto::decrypt_password(key, &password_enc) {
        Ok(p) => p,
        Err(_) => return,
    };

    // Force full resync if last_full_sync is >24h ago or never done
    let needs_full = match &last_full_sync {
        None => true,
        Some(ts) => {
            let cutoff = Utc::now() - chrono::Duration::hours(24);
            let cutoff_str = cutoff.format("%Y-%m-%d %H:%M:%S").to_string();
            ts < &cutoff_str
        }
    };
    if needs_full {
        tracing::info!(source_id = %source_id, "forcing full resync (>24h since last full sync)");
        let _ =
            sqlx::query("UPDATE calendars SET sync_token = NULL, ctag = NULL WHERE source_id = ?")
                .bind(source_id)
                .execute(pool)
                .await;
    }

    let client = CaldavClient::new(&url, &username, &password);
    let _ = sync_source(pool, key, &client, source_id).await;
}

// --- Helper functions ---

/// Upsert a calendar record and return (cal_id, stored_ctag, stored_sync_token)
async fn upsert_calendar(
    pool: &SqlitePool,
    source_id: &str,
    cal_info: &crate::caldav::CalendarInfo,
) -> Result<(String, Option<String>, Option<String>)> {
    let existing: Option<(String, Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT id, ctag, sync_token FROM calendars WHERE source_id = ? AND href = ?",
    )
    .bind(source_id)
    .bind(&cal_info.href)
    .fetch_optional(pool)
    .await?;

    match existing {
        Some((id, ctag, sync_token)) => {
            // Update display_name and color (may have changed on server)
            sqlx::query("UPDATE calendars SET display_name = ?, color = ? WHERE id = ?")
                .bind(&cal_info.display_name)
                .bind(&cal_info.color)
                .bind(&id)
                .execute(pool)
                .await?;
            Ok((id, ctag, sync_token))
        }
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
            Ok((id, None, None))
        }
    }
}

/// Upsert events from raw CalDAV data. Returns count of events processed.
async fn upsert_raw_events(pool: &SqlitePool, cal_id: &str, raw_events: &[RawEvent]) -> u32 {
    let mut count = 0u32;
    for raw in raw_events {
        let vevent_blocks = split_vevents(&raw.ical_data);
        for vevent in &vevent_blocks {
            let uid =
                extract_vevent_field(vevent, "UID").unwrap_or_else(|| Uuid::new_v4().to_string());
            let summary = extract_vevent_field(vevent, "SUMMARY");
            let start_at = extract_vevent_field(vevent, "DTSTART").unwrap_or_default();
            let end_at = extract_vevent_field(vevent, "DTEND").unwrap_or_default();
            let location = extract_vevent_field(vevent, "LOCATION");
            let description = extract_vevent_field(vevent, "DESCRIPTION");
            let status = extract_vevent_field(vevent, "STATUS");
            let rrule = extract_vevent_field(vevent, "RRULE");
            let recurrence_id = extract_vevent_field(vevent, "RECURRENCE-ID");
            let transp = extract_vevent_field(vevent, "TRANSP");
            let timezone = extract_vevent_tzid(vevent, "DTSTART");

            let event_id = Uuid::new_v4().to_string();
            let _ = sqlx::query(
                "INSERT INTO events (id, calendar_id, uid, summary, start_at, end_at, location, description, status, rrule, raw_ical, recurrence_id, timezone, transp)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
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
                   transp = excluded.transp,
                   synced_at = datetime('now')",
            )
            .bind(&event_id)
            .bind(cal_id)
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
            .bind(&transp)
            .execute(pool)
            .await;

            count += 1;
        }
    }
    count
}

/// Delete events by their CalDAV href (used for sync-collection 404 deletions).
/// Extracts UID from href pattern: /path/to/{uid}.ics
/// Also cancels any calrs bookings whose UID matches a deleted event and notifies the guest.
async fn delete_events_by_href(
    pool: &SqlitePool,
    key: &[u8; 32],
    cal_id: &str,
    hrefs: &[String],
) -> u32 {
    let mut deleted = 0u32;
    for href in hrefs {
        // Extract UID from href: /calendars/alice/default/abc123.ics -> abc123
        let uid = href
            .rsplit('/')
            .next()
            .unwrap_or("")
            .trim_end_matches(".ics");
        if !uid.is_empty() {
            let result = sqlx::query("DELETE FROM events WHERE calendar_id = ? AND uid = ?")
                .bind(cal_id)
                .bind(uid)
                .execute(pool)
                .await;
            if let Ok(r) = result {
                deleted += r.rows_affected() as u32;
            }
            // Cancel any matching booking that was deleted on the CalDAV server
            cancel_orphaned_booking(pool, key, uid).await;
        }
    }
    deleted
}

/// Remove local events that no longer exist on the server (full sync orphan cleanup).
async fn remove_orphaned_events(
    pool: &SqlitePool,
    key: &[u8; 32],
    cal_id: &str,
    raw_events: &[RawEvent],
) -> u32 {
    // Build set of seen (uid, recurrence_id) pairs
    let mut seen_uids: Vec<(String, String)> = Vec::new();
    for raw in raw_events {
        let vevent_blocks = split_vevents(&raw.ical_data);
        for vevent in &vevent_blocks {
            let uid =
                extract_vevent_field(vevent, "UID").unwrap_or_else(|| Uuid::new_v4().to_string());
            let recurrence_id = extract_vevent_field(vevent, "RECURRENCE-ID");
            seen_uids.push((uid, recurrence_id.unwrap_or_default()));
        }
    }

    if seen_uids.is_empty() {
        tracing::debug!(calendar_id = %cal_id, "orphan check skipped: server returned no events");
        return 0;
    }

    let local_events: Vec<(String, String, Option<String>)> =
        sqlx::query_as("SELECT id, uid, recurrence_id FROM events WHERE calendar_id = ?")
            .bind(cal_id)
            .fetch_all(pool)
            .await
            .unwrap_or_default();

    tracing::debug!(
        calendar_id = %cal_id,
        remote_events = seen_uids.len(),
        local_events = local_events.len(),
        "orphan detection: comparing remote vs local event sets"
    );

    let mut deleted = 0u32;
    for (event_id, uid, recurrence_id) in &local_events {
        let rec_id = recurrence_id.clone().unwrap_or_default();
        if !seen_uids.iter().any(|(u, r)| u == uid && r == &rec_id) {
            tracing::info!(uid = %uid, recurrence_id = %rec_id, "removing orphaned event (no longer on server)");
            let _ = sqlx::query("DELETE FROM events WHERE id = ?")
                .bind(event_id)
                .execute(pool)
                .await;
            cancel_orphaned_booking(pool, key, uid).await;
            deleted += 1;
        }
    }
    deleted
}

/// If a confirmed booking with this UID exists, mark it as cancelled — the event was
/// deleted on the CalDAV server side (host removed it directly in their calendar app).
/// Pending bookings are intentionally excluded: they haven't been pushed to CalDAV yet,
/// so "missing from server" is the normal state, not a cancellation signal.
/// Sends cancellation email to the guest (and host) if SMTP is configured.
async fn cancel_orphaned_booking(pool: &SqlitePool, key: &[u8; 32], uid: &str) {
    // Fetch booking details before cancelling
    let booking: Option<(
        String,
        String,
        String,
        String,
        String,
        String,
        String,
        String,
        String,
        String,
    )> = sqlx::query_as(
        "SELECT b.id, b.guest_name, b.guest_email, COALESCE(b.guest_timezone, 'UTC'), b.start_at, b.end_at, b.uid,
                et.title, u.name, COALESCE(u.booking_email, u.email)
         FROM bookings b
         JOIN event_types et ON et.id = b.event_type_id
         JOIN accounts a ON a.id = et.account_id
         JOIN users u ON u.id = a.user_id
         WHERE b.uid = ? AND b.status = 'confirmed'",
    )
    .bind(uid)
    .fetch_optional(pool)
    .await
    .unwrap_or(None);

    let booking = match booking {
        Some(b) => b,
        None => return, // No confirmed booking with this UID
    };

    let (
        booking_id,
        guest_name,
        guest_email,
        guest_timezone,
        start_at,
        end_at,
        booking_uid,
        event_title,
        host_name,
        host_email,
    ) = booking;

    // Cancel the booking
    let updated = sqlx::query(
        "UPDATE bookings SET status = 'cancelled' WHERE id = ? AND status = 'confirmed'",
    )
    .bind(&booking_id)
    .execute(pool)
    .await;

    let cancelled = matches!(updated, Ok(r) if r.rows_affected() > 0);
    if !cancelled {
        return;
    }

    tracing::info!(
        uid = %uid,
        booking_id = %booking_id,
        "booking cancelled: CalDAV event deleted externally"
    );

    // Send cancellation emails
    let smtp_config = match crate::email::load_smtp_config(pool, key).await {
        Ok(Some(cfg)) => cfg,
        _ => return, // No SMTP configured, skip email
    };

    let date = start_at.get(..10).unwrap_or(&start_at).to_string();
    let start_time = extract_time(&start_at);
    let end_time = extract_time(&end_at);

    let details = crate::email::CancellationDetails {
        event_title,
        date,
        start_time,
        end_time,
        guest_name,
        guest_email,
        guest_timezone,
        host_name,
        host_email,
        uid: booking_uid,
        reason: Some("The calendar event was deleted by the host.".to_string()),
        cancelled_by_host: true,
    };

    if let Err(e) = crate::email::send_guest_cancellation(&smtp_config, &details).await {
        tracing::warn!(error = %e, "failed to send external cancellation email to guest");
    }
    if let Err(e) = crate::email::send_host_cancellation(&smtp_config, &details).await {
        tracing::warn!(error = %e, "failed to send external cancellation email to host");
    }
}

/// Extract HH:MM time from a datetime string.
fn extract_time(dt_str: &str) -> String {
    // Try "YYYY-MM-DDTHH:MM:SS" or "YYYY-MM-DD HH:MM:SS"
    if dt_str.len() >= 16 {
        dt_str[11..16].to_string()
    } else {
        "00:00".to_string()
    }
}

/// Sweep for confirmed bookings whose CalDAV event no longer exists in the events table.
/// This catches bookings cancelled by the host deleting the event directly in their
/// calendar app. Pending bookings are excluded: they're awaiting host approval and must
/// not be auto-cancelled by sync — a guest-initiated reschedule that requires approval
/// deletes the prior CalDAV event on purpose, and the orphan sweep would otherwise race
/// the approval flow and cancel the booking before the host clicks approve.
async fn cancel_orphaned_bookings(pool: &SqlitePool, key: &[u8; 32], source_id: &str) {
    let orphans: Vec<(String,)> = sqlx::query_as(
        "SELECT b.uid FROM bookings b
         JOIN event_types et ON et.id = b.event_type_id
         JOIN accounts a ON a.id = et.account_id
         JOIN caldav_sources cs ON cs.account_id = a.id
         WHERE cs.id = ?
           AND b.status = 'confirmed'
           AND b.caldav_calendar_href IS NOT NULL
           AND b.uid NOT IN (SELECT uid FROM events)",
    )
    .bind(source_id)
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    for (uid,) in &orphans {
        cancel_orphaned_booking(pool, key, uid).await;
    }
}

/// Update stored ctag and sync_token for a calendar.
async fn update_calendar_sync_state(
    pool: &SqlitePool,
    cal_id: &str,
    ctag: &Option<String>,
    sync_token: &Option<String>,
) {
    let _ = sqlx::query("UPDATE calendars SET ctag = ?, sync_token = ? WHERE id = ?")
        .bind(ctag)
        .bind(sync_token)
        .bind(cal_id)
        .execute(pool)
        .await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
    use std::str::FromStr;

    async fn setup_test_db() -> SqlitePool {
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

    /// Seed the minimum fixtures needed to exercise the orphan sweep:
    /// one user + account + event type + caldav source. Returns the source id.
    async fn seed_fixtures(pool: &SqlitePool) -> (String, String) {
        let user_id = Uuid::new_v4().to_string();
        let account_id = Uuid::new_v4().to_string();
        let et_id = Uuid::new_v4().to_string();
        let source_id = Uuid::new_v4().to_string();

        sqlx::query("INSERT INTO users (id, email, name, role, auth_provider, username, enabled) VALUES (?, 'host@example.com', 'Host', 'admin', 'local', 'host', 1)")
            .bind(&user_id).execute(pool).await.unwrap();
        sqlx::query("INSERT INTO accounts (id, name, email, timezone, user_id) VALUES (?, 'Host', 'host@example.com', 'UTC', ?)")
            .bind(&account_id).bind(&user_id).execute(pool).await.unwrap();
        sqlx::query("INSERT INTO event_types (id, account_id, slug, title, duration_min) VALUES (?, ?, 'intro', 'Intro', 30)")
            .bind(&et_id).bind(&account_id).execute(pool).await.unwrap();
        sqlx::query("INSERT INTO caldav_sources (id, account_id, name, url, username, write_calendar_href) VALUES (?, ?, 'test', 'https://dav.example.com/', 'user', '/calendars/host/default/')")
            .bind(&source_id).bind(&account_id).execute(pool).await.unwrap();

        (source_id, et_id)
    }

    /// Regression test for issue #44: a pending booking whose CalDAV event was deleted
    /// during a guest-initiated reschedule must NOT be cancelled by the orphan sweep.
    /// Before the fix, this scenario caused the reschedule request to be cancelled
    /// before the host could click approve — only the previous meeting was cancelled
    /// and no new one was created.
    #[tokio::test]
    async fn orphan_sweep_skips_pending_booking_awaiting_approval() {
        let pool = setup_test_db().await;
        let (source_id, et_id) = seed_fixtures(&pool).await;
        let key = [0u8; 32];

        // Booking in the exact state produced by guest_reschedule_booking's pending
        // branch: status='pending', caldav_calendar_href still set from the prior
        // confirmed push (fix A in web/mod.rs clears this — but the sweep must also
        // be safe even if a legacy booking row still has the href set).
        let booking_id = Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO bookings (id, event_type_id, uid, guest_name, guest_email, guest_timezone,
                start_at, end_at, status, cancel_token, reschedule_token, caldav_calendar_href)
             VALUES (?, ?, 'orphaned-uid', 'Guest', 'guest@example.com', 'UTC',
                '2030-06-15T10:00:00', '2030-06-15T10:30:00', 'pending', 'ctok', 'rtok',
                '/calendars/host/default/')",
        )
        .bind(&booking_id)
        .bind(&et_id)
        .execute(&pool)
        .await
        .unwrap();
        // Note: no matching row in `events` — the CalDAV event was deleted during reschedule.

        cancel_orphaned_bookings(&pool, &key, &source_id).await;

        let status: String = sqlx::query_scalar("SELECT status FROM bookings WHERE id = ?")
            .bind(&booking_id)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(
            status, "pending",
            "pending bookings must not be auto-cancelled by the orphan sweep — \
             they're awaiting host approval, not tracking a CalDAV event"
        );
    }

    /// Positive case: the orphan sweep still cancels a confirmed booking whose CalDAV
    /// event has been deleted (host removed it directly in their calendar app). This
    /// is the original intent of the feature and must keep working.
    #[tokio::test]
    async fn orphan_sweep_cancels_confirmed_booking_with_missing_event() {
        let pool = setup_test_db().await;
        let (source_id, et_id) = seed_fixtures(&pool).await;
        let key = [0u8; 32];

        let booking_id = Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO bookings (id, event_type_id, uid, guest_name, guest_email, guest_timezone,
                start_at, end_at, status, cancel_token, reschedule_token, caldav_calendar_href)
             VALUES (?, ?, 'deleted-uid', 'Guest', 'guest@example.com', 'UTC',
                '2030-06-15T10:00:00', '2030-06-15T10:30:00', 'confirmed', 'ctok', 'rtok',
                '/calendars/host/default/')",
        )
        .bind(&booking_id)
        .bind(&et_id)
        .execute(&pool)
        .await
        .unwrap();

        cancel_orphaned_bookings(&pool, &key, &source_id).await;

        let status: String = sqlx::query_scalar("SELECT status FROM bookings WHERE id = ?")
            .bind(&booking_id)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(status, "cancelled");
    }

    #[tokio::test]
    async fn source_lock_identity() {
        // Same id returns the same Arc so concurrent callers contend on one
        // mutex; different ids are independent so unrelated sources can sync
        // in parallel.
        let id_a = format!("lock-test-a-{}", Uuid::new_v4());
        let id_b = format!("lock-test-b-{}", Uuid::new_v4());

        let a1 = source_lock(&id_a).await;
        let a2 = source_lock(&id_a).await;
        let b1 = source_lock(&id_b).await;

        assert!(
            Arc::ptr_eq(&a1, &a2),
            "same id must resolve to the same mutex"
        );
        assert!(
            !Arc::ptr_eq(&a1, &b1),
            "different ids must resolve to different mutexes"
        );
    }

    #[tokio::test]
    async fn sync_if_stale_serializes_on_per_source_lock() {
        // Dedup regression: while one task holds the per-source lock (the
        // "winner" of a race), a second sync_if_stale call for the same
        // source must wait. After the winner bumps last_synced and releases,
        // the waiter re-checks, sees a fresh timestamp, and skips without
        // attempting to hit CalDAV.
        let pool = setup_test_db().await;
        let (source_id, _et_id) = seed_fixtures(&pool).await;

        // Mark stale and give the source a non-null password_enc so the
        // stale_sources SELECT deserializes successfully (the value itself
        // doesn't matter — decrypt would fail but we never reach it).
        sqlx::query(
            "UPDATE caldav_sources SET last_synced = '2000-01-01 00:00:00', password_enc = 'deadbeef' WHERE id = ?",
        )
        .bind(&source_id)
        .execute(&pool)
        .await
        .unwrap();

        let user_id: String = sqlx::query_scalar(
            "SELECT a.user_id FROM caldav_sources cs JOIN accounts a ON a.id = cs.account_id WHERE cs.id = ?",
        )
        .bind(&source_id)
        .fetch_one(&pool)
        .await
        .unwrap();

        // Acquire the per-source lock before spawning the waiter.
        let lock = source_lock(&source_id).await;
        let guard = lock.lock().await;

        let pool_clone = pool.clone();
        let uid_clone = user_id.clone();
        let key = [0u8; 32];
        let waiter = tokio::spawn(async move {
            sync_if_stale(&pool_clone, &key, &uid_clone).await;
        });

        // Give the waiter a chance to reach the lock acquisition.
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        assert!(
            !waiter.is_finished(),
            "sync_if_stale must block while the per-source mutex is held"
        );

        // Simulate the winner completing its sync: bump last_synced to now.
        sqlx::query("UPDATE caldav_sources SET last_synced = datetime('now') WHERE id = ?")
            .bind(&source_id)
            .execute(&pool)
            .await
            .unwrap();

        // Release the lock. The waiter re-checks, sees fresh state, and
        // returns quickly without doing any network work.
        drop(guard);

        tokio::time::timeout(std::time::Duration::from_secs(2), waiter)
            .await
            .expect("sync_if_stale did not return after the lock was released")
            .expect("sync_if_stale task panicked");
    }
}
