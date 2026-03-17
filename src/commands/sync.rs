use anyhow::Result;
use chrono::Utc;
use colored::Colorize;
use sqlx::SqlitePool;
use uuid::Uuid;

use crate::caldav::{CaldavClient, RawEvent};
use crate::utils::{extract_vevent_field, extract_vevent_tzid, split_vevents};

/// Default staleness threshold: 5 minutes
const STALE_SECS: i64 = 300;

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

    // Update last_synced
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
        let password = match crate::crypto::decrypt_password(key, password_enc) {
            Ok(p) => p,
            Err(_) => continue,
        };
        let client = CaldavClient::new(url, username, &password);
        let _ = sync_source(pool, key, &client, source_id).await;
    }
}

/// Sync a single source by ID (for background sync loop).
/// Returns Ok(()) on success, silently handles errors.
pub async fn sync_source_by_id(pool: &SqlitePool, key: &[u8; 32], source_id: &str) {
    let source: Option<(String, String, String)> = sqlx::query_as(
        "SELECT url, username, password_enc FROM caldav_sources WHERE id = ? AND enabled = 1",
    )
    .bind(source_id)
    .fetch_optional(pool)
    .await
    .unwrap_or(None);

    if let Some((url, username, password_enc)) = source {
        let password = match crate::crypto::decrypt_password(key, &password_enc) {
            Ok(p) => p,
            Err(_) => return,
        };
        let client = CaldavClient::new(&url, &username, &password);
        let _ = sync_source(pool, key, &client, source_id).await;
    }
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
            let timezone = extract_vevent_tzid(vevent, "DTSTART");

            let event_id = Uuid::new_v4().to_string();
            let _ = sqlx::query(
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
        return 0;
    }

    let local_events: Vec<(String, String, Option<String>)> =
        sqlx::query_as("SELECT id, uid, recurrence_id FROM events WHERE calendar_id = ?")
            .bind(cal_id)
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
            cancel_orphaned_booking(pool, key, uid).await;
            deleted += 1;
        }
    }
    deleted
}

/// If a booking with this UID exists and is still active (confirmed/pending),
/// mark it as cancelled — the event was deleted on the CalDAV server side.
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
         WHERE b.uid = ? AND b.status IN ('confirmed', 'pending')",
    )
    .bind(uid)
    .fetch_optional(pool)
    .await
    .unwrap_or(None);

    let booking = match booking {
        Some(b) => b,
        None => return, // No active booking with this UID
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
        "UPDATE bookings SET status = 'cancelled' WHERE id = ? AND status IN ('confirmed', 'pending')",
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

/// Sweep for active bookings whose CalDAV event no longer exists in the events table.
/// This catches bookings that were orphaned before the per-event cancellation code was added.
async fn cancel_orphaned_bookings(pool: &SqlitePool, key: &[u8; 32], source_id: &str) {
    // Find active bookings written back to calendars under this source, whose UID
    // no longer appears in the events table.
    let orphans: Vec<(String,)> = sqlx::query_as(
        "SELECT b.uid FROM bookings b
         JOIN event_types et ON et.id = b.event_type_id
         JOIN accounts a ON a.id = et.account_id
         JOIN caldav_sources cs ON cs.account_id = a.id
         WHERE cs.id = ?
           AND b.status IN ('confirmed', 'pending')
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
