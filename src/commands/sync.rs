use anyhow::Result;
use chrono::Utc;
use colored::Colorize;
use sqlx::SqlitePool;
use std::collections::HashMap;
use std::sync::{Arc, OnceLock};
use tokio::sync::Mutex;
use uuid::Uuid;

use crate::caldav::{CaldavClient, RawEvent};
use crate::providers::{factory::kinds, RawEvent as ProviderRawEvent};
use crate::utils::{extract_vevent_field, extract_vevent_tzid, split_vevents};

/// Default staleness threshold: 5 minutes
const STALE_SECS: i64 = 300;

/// Look-back window for full-fetch path. Bounded so Google CalDAV, which
/// truncates the forward window of unfiltered REPORTs, returns future events
/// via the `time-range` filter. 90 days keeps recent history available for
/// orphan/cancellation detection without ballooning the response.
const FULL_FETCH_LOOKBACK_DAYS: i64 = 90;

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
    #[allow(clippy::type_complexity)]
    let sources: Vec<(
        String,
        String,
        String,
        String,
        Option<String>,
        String,
        Option<String>,
        Option<String>,
        String,
        i64,
        Option<String>,
    )> = sqlx::query_as(
        "SELECT id, name, url, username, password_enc, auth_type, access_token_enc, \
                token_expires_at, provider_type, managed, impersonate_email \
         FROM caldav_sources WHERE enabled = 1",
    )
    .fetch_all(pool)
    .await?;

    if sources.is_empty() {
        println!("No sources configured. Add one with `calrs source add`.");
        return Ok(());
    }

    // Load the global EWS config once for the whole sync run — managed rows
    // all share it. None means the feature is off (managed rows are skipped).
    let ews_global_cfg = crate::web::ews_global::load_ews_global_config(pool, key).await;

    for (
        source_id,
        name,
        url,
        username,
        password_enc,
        auth_type,
        access_token_enc,
        token_expires_at,
        provider_type,
        managed,
        impersonate_email,
    ) in &sources
    {
        // For managed Exchange rows, surface which mailbox we impersonate so
        // multiple "Exchange (managed)" rows are distinguishable in the output.
        if *managed != 0 {
            if let Some(mb) = impersonate_email.as_deref() {
                println!("{} Syncing '{}' (mailbox: {})…", "…".dimmed(), name, mb);
            } else {
                println!("{} Syncing '{}'…", "…".dimmed(), name);
            }
        } else {
            println!("{} Syncing '{}'…", "…".dimmed(), name);
        }

        if full {
            // Clear sync tokens to force a full fetch
            let _ = sqlx::query(
                "UPDATE calendars SET sync_token = NULL, ctag = NULL WHERE source_id = ?",
            )
            .bind(source_id)
            .execute(pool)
            .await;
        }

        // EWS sources go through the provider trait (no OAuth2, no CalDAV-only
        // sync-collection); CalDAV sources keep the existing flow.
        if provider_type == kinds::EWS {
            // Managed rows use the global service account + impersonation
            // header; per-user EWS rows keep their own credentials.
            let provider_result = if *managed != 0 {
                match ews_global_cfg.as_ref() {
                    Some(cfg) => crate::providers::build_provider(
                        provider_type,
                        &cfg.url,
                        &cfg.service_username,
                        &cfg.service_password,
                        cfg.impersonation_target(impersonate_email.as_deref())
                            .as_deref(),
                    ),
                    None => {
                        println!(
                            "  {} Managed Exchange source skipped: global EWS config is disabled.",
                            "…".dimmed()
                        );
                        continue;
                    }
                }
            } else {
                let password = match crate::crypto::decrypt_password(
                    key,
                    password_enc.as_deref().unwrap_or(""),
                ) {
                    Ok(p) => p,
                    Err(e) => {
                        println!("  {} Decrypt failed: {}", "✗".red(), e);
                        continue;
                    }
                };
                crate::providers::build_provider(provider_type, url, username, &password, None)
            };
            let provider = match provider_result {
                Ok(p) => p,
                Err(e) => {
                    println!("  {} Provider build failed: {}", "✗".red(), e);
                    continue;
                }
            };
            if let Err(e) = sync_ews_source(pool, key, provider.as_ref(), source_id).await {
                // A managed source pointing at a user without an Exchange
                // mailbox (e.g. the local admin account) can never sync.
                // Surface that as a clear note rather than an alarming error.
                if *managed != 0 && e.to_string().contains("ErrorNonExistentMailbox") {
                    println!(
                        "  {} No Exchange mailbox for {} — skipping this user.",
                        "⚠".yellow(),
                        impersonate_email.as_deref().unwrap_or("(unknown)")
                    );
                } else {
                    println!("  {} Sync failed: {}", "✗".red(), e);
                }
            }
            continue;
        }

        let client = crate::oauth2_caldav::build_client_for_source(
            pool,
            key,
            source_id,
            url,
            auth_type,
            username,
            password_enc.as_deref(),
            access_token_enc.as_deref(),
            token_expires_at.as_deref(),
        )
        .await?;

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
                        let deleted = delete_events_by_href(
                            pool,
                            key,
                            Some(client),
                            source_id,
                            &cal_id,
                            &result.deleted_hrefs,
                        )
                        .await;

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
            // Full fetch fallback. Bounded with a `time-range` filter so Google
            // CalDAV returns future events (its unfiltered REPORT truncates the
            // forward window). fetch_events_since falls back to the unfiltered
            // REPORT if the server rejects time-range, so other servers are
            // unaffected.
            let since_dt = Utc::now() - chrono::Duration::days(FULL_FETCH_LOOKBACK_DAYS);
            let since_iso = since_dt.format("%Y%m%dT%H%M%SZ").to_string();
            let since_prefix = since_dt.format("%Y%m%d").to_string();
            match client.fetch_events_since(&cal_info.href, &since_iso).await {
                Ok(raw_events) => {
                    let count = upsert_raw_events(pool, &cal_id, &raw_events).await;

                    // Remove events that no longer exist on the server, but only
                    // within the fetched window. Older events weren't in the
                    // response by design and must not be treated as orphans.
                    let deleted = remove_orphaned_events(
                        pool,
                        key,
                        Some(client),
                        source_id,
                        &cal_id,
                        &raw_events,
                        &since_prefix,
                    )
                    .await;
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
    cancel_orphaned_bookings(pool, key, Some(client), source_id).await;

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

    #[allow(clippy::type_complexity)]
    let stale_sources: Vec<(
        String,
        String,
        String,
        Option<String>,
        String,
        Option<String>,
        Option<String>,
        String,
        i64,
        Option<String>,
    )> = sqlx::query_as(
        "SELECT cs.id, cs.url, cs.username, cs.password_enc, cs.auth_type, \
                cs.access_token_enc, cs.token_expires_at, cs.provider_type, \
                cs.managed, cs.impersonate_email
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

    // Cache the global EWS config for the duration of this fan-out.
    let ews_global_cfg = crate::web::ews_global::load_ews_global_config(pool, key).await;

    for (
        source_id,
        url,
        username,
        password_enc,
        auth_type,
        access_token_enc,
        token_expires_at,
        provider_type,
        managed,
        impersonate_email,
    ) in &stale_sources
    {
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

        if provider_type == kinds::EWS {
            let provider_result = if *managed != 0 {
                match ews_global_cfg.as_ref() {
                    Some(cfg) => crate::providers::build_provider(
                        provider_type,
                        &cfg.url,
                        &cfg.service_username,
                        &cfg.service_password,
                        cfg.impersonation_target(impersonate_email.as_deref())
                            .as_deref(),
                    ),
                    None => continue,
                }
            } else {
                let password = match crate::crypto::decrypt_password(
                    key,
                    password_enc.as_deref().unwrap_or(""),
                ) {
                    Ok(p) => p,
                    Err(_) => continue,
                };
                crate::providers::build_provider(provider_type, url, username, &password, None)
            };
            let provider = match provider_result {
                Ok(p) => p,
                Err(_) => continue,
            };
            let _ = sync_ews_source(pool, key, provider.as_ref(), source_id).await;
            continue;
        }

        let client = match crate::oauth2_caldav::build_client_for_source(
            pool,
            key,
            source_id,
            url,
            auth_type,
            username,
            password_enc.as_deref(),
            access_token_enc.as_deref(),
            token_expires_at.as_deref(),
        )
        .await
        {
            Ok(c) => c,
            Err(_) => continue,
        };
        let _ = sync_source(pool, key, &client, source_id).await;
    }
}

/// Sync a single source by ID (for background sync loop).
/// Forces a full resync if last_full_sync is >24h ago (catches orphaned events).
pub async fn sync_source_by_id(pool: &SqlitePool, key: &[u8; 32], source_id: &str) {
    #[allow(clippy::type_complexity)]
    let source: Option<(
        String,
        String,
        Option<String>,
        Option<String>,
        String,
        Option<String>,
        Option<String>,
        String,
        i64,
        Option<String>,
    )> = sqlx::query_as(
        "SELECT url, username, password_enc, last_full_sync, auth_type, access_token_enc, \
                token_expires_at, provider_type, managed, impersonate_email \
         FROM caldav_sources WHERE id = ? AND enabled = 1",
    )
    .bind(source_id)
    .fetch_optional(pool)
    .await
    .unwrap_or(None);

    let Some((
        url,
        username,
        password_enc,
        last_full_sync,
        auth_type,
        access_token_enc,
        token_expires_at,
        provider_type,
        managed,
        impersonate_email,
    )) = source
    else {
        return;
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

    if provider_type == kinds::EWS {
        let provider_result = if managed != 0 {
            match crate::web::ews_global::load_ews_global_config(pool, key).await {
                Some(cfg) => crate::providers::build_provider(
                    &provider_type,
                    &cfg.url,
                    &cfg.service_username,
                    &cfg.service_password,
                    cfg.impersonation_target(impersonate_email.as_deref())
                        .as_deref(),
                ),
                None => return,
            }
        } else {
            let password =
                match crate::crypto::decrypt_password(key, password_enc.as_deref().unwrap_or("")) {
                    Ok(p) => p,
                    Err(_) => return,
                };
            crate::providers::build_provider(&provider_type, &url, &username, &password, None)
        };
        let provider = match provider_result {
            Ok(p) => p,
            Err(_) => return,
        };
        let _ = sync_ews_source(pool, key, provider.as_ref(), source_id).await;
        return;
    }

    let client = match crate::oauth2_caldav::build_client_for_source(
        pool,
        key,
        source_id,
        &url,
        &auth_type,
        &username,
        password_enc.as_deref(),
        access_token_enc.as_deref(),
        token_expires_at.as_deref(),
    )
    .await
    {
        Ok(c) => c,
        Err(_) => return,
    };
    let _ = sync_source(pool, key, &client, source_id).await;
}

/// EWS-specific sync path using the [`crate::providers::CalendarProvider`]
/// trait. CalDAV sources keep going through [`sync_source`], which retains
/// CalDAV-only optimisations (ctag, RFC 6578 sync-token, time-range queries,
/// hardened orphan reconciliation). The EWS path is intentionally simpler:
/// list folders, fetch each one in full, and reconcile by UID. Delta sync is a
/// known follow-up — see `EwsProvider::sync_delta`.
pub async fn sync_ews_source(
    pool: &SqlitePool,
    key: &[u8; 32],
    provider: &dyn crate::providers::CalendarProvider,
    source_id: &str,
) -> Result<()> {
    let calendars = provider.list_calendars().await?;

    // Bounded fetch window. Matches the CalDAV path's FULL_FETCH_LOOKBACK_DAYS:
    // 90 days back is plenty for orphan reconciliation and keeps EWS response
    // sizes predictable. The provider's fetch_events_since uses CalendarView,
    // which expands recurrences server-side within the window.
    let since_dt = Utc::now() - chrono::Duration::days(FULL_FETCH_LOOKBACK_DAYS);
    let since_iso = since_dt.to_rfc3339();
    let since_prefix = since_dt.format("%Y%m%d").to_string();

    for cal_info in &calendars {
        let (cal_id, _stored_change_marker, _stored_sync_state) =
            upsert_calendar_provider(pool, source_id, cal_info).await?;
        let cal_label = cal_info.display_name.as_deref().unwrap_or(&cal_info.id);

        match provider.fetch_events_since(&cal_info.id, &since_iso).await {
            Ok(raw_events) => {
                let count = upsert_provider_events(pool, &cal_id, &raw_events).await;
                let deleted =
                    remove_orphaned_ews_events(pool, key, &cal_id, &raw_events, &since_prefix)
                        .await;
                if deleted > 0 {
                    tracing::info!(
                        calendar_name = cal_label,
                        stale_events_removed = deleted,
                        "removed stale EWS events from local cache"
                    );
                }
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

    let _ = sqlx::query("UPDATE caldav_sources SET last_full_sync = datetime('now') WHERE id = ?")
        .bind(source_id)
        .execute(pool)
        .await;
    sqlx::query("UPDATE caldav_sources SET last_synced = datetime('now') WHERE id = ?")
        .bind(source_id)
        .execute(pool)
        .await?;
    tracing::info!(source_id = %source_id, "EWS sync completed");
    Ok(())
}

/// Provider-trait equivalent of [`upsert_calendar`]. EWS uses opaque folder
/// IDs in the `href` column; the `id` field on `RemoteCalendar` is reused.
async fn upsert_calendar_provider(
    pool: &SqlitePool,
    source_id: &str,
    cal_info: &crate::providers::RemoteCalendar,
) -> Result<(String, Option<String>, Option<String>)> {
    let existing: Option<(String, Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT id, ctag, sync_token FROM calendars WHERE source_id = ? AND href = ?",
    )
    .bind(source_id)
    .bind(&cal_info.id)
    .fetch_optional(pool)
    .await?;

    match existing {
        Some((id, ctag, sync_token)) => {
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
            .bind(&cal_info.id)
            .bind(&cal_info.display_name)
            .bind(&cal_info.color)
            .bind(&cal_info.change_marker)
            .execute(pool)
            .await?;
            Ok((id, None, None))
        }
    }
}

/// Provider-trait equivalent of [`upsert_raw_events`]. Splits the iCal blob
/// into VEVENTs and upserts into the `events` table (same composite key:
/// calendar_id + uid + recurrence_id).
async fn upsert_provider_events(
    pool: &SqlitePool,
    cal_id: &str,
    raw_events: &[ProviderRawEvent],
) -> u32 {
    let mut count = 0u32;
    for raw in raw_events {
        let vevent_blocks = split_vevents(&raw.ical);
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
            .bind(&raw.ical)
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

/// EWS variant of orphan reconciliation, scoped to the fetched window.
/// `since_prefix` is a `YYYYMMDD` lower bound matching the
/// `fetch_events_since` call: events with `start_at` before it weren't in
/// the response and must not be flagged as orphans. Pass an empty string to
/// reconcile against every local event.
///
/// `client = None` is implied: EWS sources can't be HTTP-verified against a
/// `CaldavClient`, so we go straight to DB cancellation when an event has
/// vanished from the server.
async fn remove_orphaned_ews_events(
    pool: &SqlitePool,
    key: &[u8; 32],
    cal_id: &str,
    raw_events: &[ProviderRawEvent],
    since_prefix: &str,
) -> u32 {
    let mut seen_uids: Vec<(String, String)> = Vec::new();
    for raw in raw_events {
        for vevent in split_vevents(&raw.ical) {
            let uid =
                extract_vevent_field(&vevent, "UID").unwrap_or_else(|| Uuid::new_v4().to_string());
            let recurrence_id = extract_vevent_field(&vevent, "RECURRENCE-ID");
            seen_uids.push((uid, recurrence_id.unwrap_or_default()));
        }
    }

    if seen_uids.is_empty() {
        return 0;
    }

    // Same window-scoping trick as the CalDAV path: compact ("YYYYMMDDTHHMMSS")
    // and all-day ("YYYYMMDD") start_at values both sort against a YYYYMMDD
    // lower bound.
    let local_events: Vec<(String, String, Option<String>)> = sqlx::query_as(
        "SELECT id, uid, recurrence_id FROM events
         WHERE calendar_id = ?
           AND (? = '' OR start_at >= ?)",
    )
    .bind(cal_id)
    .bind(since_prefix)
    .bind(since_prefix)
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
            cancel_orphaned_booking_simple(pool, key, uid).await;
            deleted += 1;
        }
    }
    deleted
}

/// Simplified booking-cancel for EWS orphan reconciliation: looks up a
/// confirmed booking by UID and marks it cancelled. Skips the
/// `cancel_orphaned_booking` HTTP confirm step (CalDAV-specific).
async fn cancel_orphaned_booking_simple(pool: &SqlitePool, _key: &[u8; 32], uid: &str) {
    let _ = sqlx::query(
        "UPDATE bookings SET status = 'cancelled' WHERE uid = ? AND status = 'confirmed'",
    )
    .bind(uid)
    .execute(pool)
    .await;
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
///
/// `client` is forwarded to `cancel_orphaned_booking` for confirm-before-cancel
/// verification. Tests pass `None` to bypass HTTP verification.
///
/// `source_id` scopes booking cancellations to event types owned by this source's
/// account (issue #106 defense-in-depth).
async fn delete_events_by_href(
    pool: &SqlitePool,
    key: &[u8; 32],
    client: Option<&CaldavClient>,
    source_id: &str,
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
        if uid.is_empty() {
            continue;
        }
        let rows = sqlx::query("DELETE FROM events WHERE calendar_id = ? AND uid = ?")
            .bind(cal_id)
            .bind(uid)
            .execute(pool)
            .await
            .map(|r| r.rows_affected())
            .unwrap_or(0);
        if rows > 0 {
            deleted += rows as u32;
            // Cancel any matching booking that was deleted on the CalDAV server.
            // Gated on rows_affected > 0: if the server reports an href as deleted
            // but we never had a matching local event, that's a server-side quirk
            // (e.g. BlueMind sync-collection emitting spurious 404 propstats) — not
            // proof the host deleted the event. Cancelling on that signal alone has
            // wrongly cancelled live bookings in production.
            cancel_orphaned_booking(pool, key, client, source_id, uid).await;
        } else {
            tracing::warn!(
                uid = %uid,
                href = %href,
                calendar_id = %cal_id,
                "sync-collection reported href as deleted but no matching local event; \
                 skipping booking cancellation (likely server-side false positive)"
            );
        }
    }
    deleted
}

/// Remove local events that no longer exist on the server (full sync orphan cleanup).
///
/// `client` is forwarded to `cancel_orphaned_booking` for confirm-before-cancel
/// verification. Tests pass `None` to bypass HTTP verification.
///
/// `source_id` scopes booking cancellations to event types owned by this source's
/// account (issue #106 defense-in-depth).
///
/// `since_prefix` bounds the orphan check to events whose start_at falls inside
/// the fetched window. Older events weren't in the response and must not be
/// deleted as orphans. Pass an empty string to consider all local events.
async fn remove_orphaned_events(
    pool: &SqlitePool,
    key: &[u8; 32],
    client: Option<&CaldavClient>,
    source_id: &str,
    cal_id: &str,
    raw_events: &[RawEvent],
    since_prefix: &str,
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

    // Scope the orphan check to events whose start_at falls inside the fetched
    // window. Both compact ("YYYYMMDDTHHMMSS") and all-day ("YYYYMMDD")
    // encodings sort correctly against an 8-char "YYYYMMDD" lower bound.
    let local_events: Vec<(String, String, Option<String>)> = sqlx::query_as(
        "SELECT id, uid, recurrence_id FROM events
         WHERE calendar_id = ?
           AND (? = '' OR start_at >= ?)",
    )
    .bind(cal_id)
    .bind(since_prefix)
    .bind(since_prefix)
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
            cancel_orphaned_booking(pool, key, client, source_id, uid).await;
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
///
/// `source_id` scopes the lookup: only bookings on event types whose account owns
/// `source_id` are eligible for cancellation. This is defense-in-depth — a sync
/// of source A must never be able to cancel a booking on source B (different
/// account, same UID by collision). See issue #106.
///
/// When `client` is `Some` and the booking has a stored `caldav_calendar_href`,
/// the resource is double-checked against the server via HEAD/PROPFIND before
/// the cancellation goes through. If the server says the event is still there
/// (or the verification can't conclude — network error, 5xx, auth failure),
/// the cancellation is skipped and a warning is logged. This is the safety net
/// against false positives in any orphan path: see issue #105.
///
/// Tests pass `None` for `client` to skip the HTTP verification and exercise the
/// DB-only cancellation behaviour directly.
async fn cancel_orphaned_booking(
    pool: &SqlitePool,
    key: &[u8; 32],
    client: Option<&CaldavClient>,
    source_id: &str,
    uid: &str,
) {
    // Fetch booking details before cancelling. Scoped by source_id via the
    // caldav_sources join: the source's account must match the booking's
    // event-type account.
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
        Option<String>,
        Option<String>,
    )> = sqlx::query_as(
        "SELECT b.id, b.guest_name, b.guest_email, COALESCE(b.guest_timezone, 'UTC'), b.start_at, b.end_at, b.uid,
                et.title, u.name, COALESCE(u.booking_email, u.email), b.caldav_calendar_href, u.timezone
         FROM bookings b
         JOIN event_types et ON et.id = b.event_type_id
         JOIN accounts a ON a.id = et.account_id
         JOIN users u ON u.id = a.user_id
         JOIN caldav_sources cs ON cs.account_id = a.id
         WHERE b.uid = ? AND b.status = 'confirmed' AND cs.id = ?",
    )
    .bind(uid)
    .bind(source_id)
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
        caldav_calendar_href,
        host_timezone,
    ) = booking;

    // Confirm-before-cancel: if we have a client and a stored calendar href for
    // this booking, verify that the resource is actually 404 on the server.
    // Any non-404 outcome (200 = still there, 5xx = server flake, network = down)
    // means we cannot prove the host deleted the event, so we skip the cancel.
    if let (Some(client), Some(cal_href)) = (client, caldav_calendar_href.as_deref()) {
        match client.event_exists(cal_href, uid).await {
            Ok(false) => {
                // Server confirms 404 — legitimate deletion, proceed.
            }
            Ok(true) => {
                tracing::warn!(
                    uid = %uid,
                    booking_id = %booking_id,
                    cal_href = %cal_href,
                    "skipping booking cancellation: CalDAV resource is still present on the server \
                     (a sync path reported it as deleted but verification disagrees)"
                );
                return;
            }
            Err(e) => {
                tracing::warn!(
                    uid = %uid,
                    booking_id = %booking_id,
                    cal_href = %cal_href,
                    error = %e,
                    "skipping booking cancellation: could not verify resource state on the server \
                     (treating inconclusive verification as not-deleted to avoid false positives)"
                );
                return;
            }
        }
    }

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
        host_timezone: host_timezone.unwrap_or_default(),
        ..Default::default()
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
async fn cancel_orphaned_bookings(
    pool: &SqlitePool,
    key: &[u8; 32],
    client: Option<&CaldavClient>,
    source_id: &str,
) {
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
        cancel_orphaned_booking(pool, key, client, source_id, uid).await;
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

        cancel_orphaned_bookings(&pool, &key, None, &source_id).await;

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

    /// Regression test for production incident 2026-05-14: a sync-collection delta
    /// reported an href as deleted, but the local `events` table had no matching row
    /// (the event lived on a different calendar — the booking's write calendar).
    /// Before the fix, `delete_events_by_href` still called `cancel_orphaned_booking`,
    /// which scans bookings globally by UID and wrongly cancelled a live booking.
    /// The fix gates the cancellation on the local DELETE having matched a row.
    #[tokio::test]
    async fn delete_events_by_href_skips_cancellation_when_no_local_event() {
        let pool = setup_test_db().await;
        let (source_id, et_id) = seed_fixtures(&pool).await;
        let key = [0u8; 32];

        // Seed a calendar on this source (the one that supposedly reported a
        // deletion via sync-collection delta).
        let cal_id = Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO calendars (id, source_id, href, display_name) \
             VALUES (?, ?, '/calendars/host/shared/', 'Shared')",
        )
        .bind(&cal_id)
        .bind(&source_id)
        .execute(&pool)
        .await
        .unwrap();

        // The confirmed booking. Note: NO matching row in `events` for this calendar
        // — the booking's CalDAV event lives elsewhere (or was never synced into
        // this particular calendar).
        let booking_uid = "live-booking-uid@calrs";
        let booking_id = Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO bookings (id, event_type_id, uid, guest_name, guest_email, guest_timezone,
                start_at, end_at, status, cancel_token, reschedule_token, caldav_calendar_href)
             VALUES (?, ?, ?, 'Guest', 'guest@example.com', 'UTC',
                '2030-06-15T10:00:00', '2030-06-15T10:30:00', 'confirmed', 'ctok', 'rtok',
                '/calendars/host/default/')",
        )
        .bind(&booking_id)
        .bind(&et_id)
        .bind(booking_uid)
        .execute(&pool)
        .await
        .unwrap();

        // Simulate BlueMind's sync-collection reporting this href as deleted on
        // the Shared calendar (false positive — the event isn't there locally).
        let href = format!("/calendars/host/shared/{}.ics", booking_uid);
        let deleted = delete_events_by_href(&pool, &key, None, &source_id, &cal_id, &[href]).await;
        assert_eq!(deleted, 0, "no local row to delete");

        let status: String = sqlx::query_scalar("SELECT status FROM bookings WHERE id = ?")
            .bind(&booking_id)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(
            status, "confirmed",
            "a sync-collection 'deleted' href with no local event MUST NOT cancel the booking"
        );
    }

    /// Positive case for `delete_events_by_href`: when the server reports a deletion
    /// AND we had a matching local event for that calendar, we both remove the event
    /// and cancel any booking with that UID. This is the legitimate signal.
    #[tokio::test]
    async fn delete_events_by_href_cancels_when_local_event_existed() {
        let pool = setup_test_db().await;
        let (source_id, et_id) = seed_fixtures(&pool).await;
        let key = [0u8; 32];

        let cal_id = Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO calendars (id, source_id, href, display_name) \
             VALUES (?, ?, '/calendars/host/default/', 'Default')",
        )
        .bind(&cal_id)
        .bind(&source_id)
        .execute(&pool)
        .await
        .unwrap();

        let booking_uid = "to-be-cancelled@calrs";

        // Seed the local event row (we knew about it before the server signaled deletion).
        sqlx::query(
            "INSERT INTO events (id, calendar_id, uid, summary, start_at, end_at) \
             VALUES (?, ?, ?, 'Demo', '2030-06-15T10:00:00', '2030-06-15T10:30:00')",
        )
        .bind(Uuid::new_v4().to_string())
        .bind(&cal_id)
        .bind(booking_uid)
        .execute(&pool)
        .await
        .unwrap();

        let booking_id = Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO bookings (id, event_type_id, uid, guest_name, guest_email, guest_timezone,
                start_at, end_at, status, cancel_token, reschedule_token, caldav_calendar_href)
             VALUES (?, ?, ?, 'Guest', 'guest@example.com', 'UTC',
                '2030-06-15T10:00:00', '2030-06-15T10:30:00', 'confirmed', 'ctok', 'rtok',
                '/calendars/host/default/')",
        )
        .bind(&booking_id)
        .bind(&et_id)
        .bind(booking_uid)
        .execute(&pool)
        .await
        .unwrap();

        let href = format!("/calendars/host/default/{}.ics", booking_uid);
        let deleted = delete_events_by_href(&pool, &key, None, &source_id, &cal_id, &[href]).await;
        assert_eq!(deleted, 1, "local row should have been removed");

        let status: String = sqlx::query_scalar("SELECT status FROM bookings WHERE id = ?")
            .bind(&booking_id)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(status, "cancelled");
    }

    /// Confirm-before-cancel (issue #105): when the verification HTTP call can't
    /// reach the server (unreachable host, timeout, 5xx), `cancel_orphaned_booking`
    /// must treat the result as inconclusive and SKIP the cancellation. The
    /// principle is "never cancel a customer booking unless the server confirms
    /// the event is gone." A flaky network minute is no basis for cancelling.
    ///
    /// This test points the CaldavClient at a closed port so the HEAD fails with
    /// a connection error; we set up a delta-path scenario where the booking
    /// would otherwise be cancelled (local event present, server reports deletion).
    #[tokio::test]
    async fn delete_events_by_href_skips_cancellation_when_verification_fails() {
        let pool = setup_test_db().await;
        let (source_id, et_id) = seed_fixtures(&pool).await;
        let key = [0u8; 32];

        let cal_id = Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO calendars (id, source_id, href, display_name) \
             VALUES (?, ?, '/calendars/host/default/', 'Default')",
        )
        .bind(&cal_id)
        .bind(&source_id)
        .execute(&pool)
        .await
        .unwrap();

        let booking_uid = "ambiguous-deletion@calrs";

        // Local event row exists — so the DELETE will report rows_affected > 0
        // and the hotfix gate alone wouldn't save us. Only the verification
        // layer should keep this booking confirmed.
        sqlx::query(
            "INSERT INTO events (id, calendar_id, uid, summary, start_at, end_at) \
             VALUES (?, ?, ?, 'Demo', '2030-06-15T10:00:00', '2030-06-15T10:30:00')",
        )
        .bind(Uuid::new_v4().to_string())
        .bind(&cal_id)
        .bind(booking_uid)
        .execute(&pool)
        .await
        .unwrap();

        let booking_id = Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO bookings (id, event_type_id, uid, guest_name, guest_email, guest_timezone,
                start_at, end_at, status, cancel_token, reschedule_token, caldav_calendar_href)
             VALUES (?, ?, ?, 'Guest', 'guest@example.com', 'UTC',
                '2030-06-15T10:00:00', '2030-06-15T10:30:00', 'confirmed', 'ctok', 'rtok',
                '/calendars/host/default/')",
        )
        .bind(&booking_id)
        .bind(&et_id)
        .bind(booking_uid)
        .execute(&pool)
        .await
        .unwrap();

        // Port 1 is reserved (tcpmux) and ~always closed on a dev box — HEAD will
        // fail with a connection refusal, exercising the Err(_) arm of the
        // verification gate.
        let client = CaldavClient::new("http://127.0.0.1:1", "u", "p");

        let href = format!("/calendars/host/default/{}.ics", booking_uid);
        let _ =
            delete_events_by_href(&pool, &key, Some(&client), &source_id, &cal_id, &[href]).await;

        let status: String = sqlx::query_scalar("SELECT status FROM bookings WHERE id = ?")
            .bind(&booking_id)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(
            status, "confirmed",
            "verification HTTP failure must NOT cancel the booking — \
             inconclusive evidence cannot justify a customer-visible cancellation"
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

        cancel_orphaned_bookings(&pool, &key, None, &source_id).await;

        let status: String = sqlx::query_scalar("SELECT status FROM bookings WHERE id = ?")
            .bind(&booking_id)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(status, "cancelled");
    }

    /// Issue #106 (cross-source isolation defense-in-depth): a sync of source A
    /// must NEVER cancel a booking that belongs to source B's account, even if
    /// the UIDs collide. UUIDs make natural collisions vanishingly unlikely, but
    /// a single-tenant install where an admin imports an iCal feed (or a future
    /// codepath that generates UIDs from a non-UUID source) could produce one.
    /// The lookup in `cancel_orphaned_booking` is scoped via a join on
    /// `caldav_sources.account_id = event_types.account_id`, with the source_id
    /// filter pinning the side we're acting on. This test verifies that scoping.
    #[tokio::test]
    async fn cancel_does_not_cross_source_account_boundary() {
        let pool = setup_test_db().await;
        // Account A: gets seeded by the helper. Source A, event type A.
        // We don't act ON source A in this test — we only verify a sync of
        // source B can't reach across to cancel a booking on account A.
        let (_source_a_id, et_a_id) = seed_fixtures(&pool).await;

        // Account B: separate user, account, event type, source.
        let user_b_id = Uuid::new_v4().to_string();
        let account_b_id = Uuid::new_v4().to_string();
        let et_b_id = Uuid::new_v4().to_string();
        let source_b_id = Uuid::new_v4().to_string();
        let cal_b_id = Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO users (id, email, name, role, auth_provider, username, enabled) VALUES (?, 'host-b@example.com', 'Host B', 'user', 'local', 'hostb', 1)")
            .bind(&user_b_id).execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO accounts (id, name, email, timezone, user_id) VALUES (?, 'Host B', 'host-b@example.com', 'UTC', ?)")
            .bind(&account_b_id).bind(&user_b_id).execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO event_types (id, account_id, slug, title, duration_min) VALUES (?, ?, 'intro-b', 'Intro B', 30)")
            .bind(&et_b_id).bind(&account_b_id).execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO caldav_sources (id, account_id, name, url, username, write_calendar_href) VALUES (?, ?, 'test-b', 'https://dav-b.example.com/', 'user-b', '/calendars/hostb/default/')")
            .bind(&source_b_id).bind(&account_b_id).execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO calendars (id, source_id, href, display_name) VALUES (?, ?, '/calendars/hostb/default/', 'Default B')")
            .bind(&cal_b_id).bind(&source_b_id).execute(&pool).await.unwrap();

        let shared_uid = "colliding-uid@calrs";
        let key = [0u8; 32];

        // Confirmed booking on ACCOUNT A — this is what we're protecting.
        let booking_a_id = Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO bookings (id, event_type_id, uid, guest_name, guest_email, guest_timezone,
                start_at, end_at, status, cancel_token, reschedule_token, caldav_calendar_href)
             VALUES (?, ?, ?, 'Guest', 'guest@example.com', 'UTC',
                '2030-06-15T10:00:00', '2030-06-15T10:30:00', 'confirmed', 'ctok', 'rtok',
                '/calendars/host/default/')",
        )
        .bind(&booking_a_id)
        .bind(&et_a_id)
        .bind(shared_uid)
        .execute(&pool)
        .await
        .unwrap();

        // Local event on SOURCE B's calendar with the same UID — so the
        // DELETE in `delete_events_by_href` will report rows_affected > 0
        // and the rows_affected gate alone can't save us. Only the
        // source-scoped lookup keeps booking A safe.
        sqlx::query(
            "INSERT INTO events (id, calendar_id, uid, summary, start_at, end_at) \
             VALUES (?, ?, ?, 'Colliding event on B', '2030-06-15T10:00:00', '2030-06-15T10:30:00')",
        )
        .bind(Uuid::new_v4().to_string())
        .bind(&cal_b_id)
        .bind(shared_uid)
        .execute(&pool)
        .await
        .unwrap();

        // Source B reports the colliding UID as deleted.
        let href = format!("/calendars/hostb/default/{}.ics", shared_uid);
        let deleted =
            delete_events_by_href(&pool, &key, None, &source_b_id, &cal_b_id, &[href]).await;
        assert_eq!(
            deleted, 1,
            "the local event row on source B should have been removed"
        );

        // Booking on ACCOUNT A must still be confirmed.
        let status: String = sqlx::query_scalar("SELECT status FROM bookings WHERE id = ?")
            .bind(&booking_a_id)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(
            status, "confirmed",
            "a sync of source B must not cancel a booking on account A — \
             source/account scoping is the defense-in-depth boundary"
        );
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
