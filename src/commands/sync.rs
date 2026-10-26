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
use crate::utils::{extract_vevent_field, split_vevents};

/// Default staleness threshold: 5 minutes
const STALE_SECS: i64 = 300;

/// Look-back window for full-fetch path. Bounded so Google CalDAV, which
/// truncates the forward window of unfiltered REPORTs, returns future events
/// via the `time-range` filter. 90 days keeps recent history available for
/// orphan/cancellation detection without ballooning the response.
pub(crate) const FULL_FETCH_LOOKBACK_DAYS: i64 = 90;

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
    let sources: Vec<(String, String, String, String, Option<String>, String, Option<String>, Option<String>, String)> = sqlx::query_as(
        "SELECT id, name, url, username, password_enc, auth_type, access_token_enc, token_expires_at, provider_type FROM caldav_sources WHERE enabled = 1",
    )
    .fetch_all(pool)
    .await?;

    if sources.is_empty() {
        println!("No sources configured. Add one with `calrs source add`.");
        return Ok(());
    }

    let mut failed: Vec<(String, String)> = Vec::new();
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
    ) in &sources
    {
        println!("{} Syncing '{}'…", "…".dimmed(), name);

        if full {
            // Clear sync tokens to force a full fetch
            let _ = sqlx::query(
                "UPDATE calendars SET sync_token = NULL, ctag = NULL WHERE source_id = ?",
            )
            .bind(source_id)
            .execute(pool)
            .await;
        }

        if let Err(e) = sync_one_source(
            pool,
            key,
            source_id,
            url,
            username,
            password_enc.as_deref(),
            auth_type,
            access_token_enc.as_deref(),
            token_expires_at.as_deref(),
            provider_type,
        )
        .await
        {
            println!("  {} Sync failed: {:#}", "✗".red(), e);
            failed.push((name.clone(), format!("{e:#}")));
        }
    }

    // A failing source may have queued an email to its owner.
    crate::web::drain_emails(std::time::Duration::from_secs(30)).await;

    if failed.is_empty() {
        println!("{} Sync complete.", "✓".green());
        return Ok(());
    }
    // #162: this used to print "Sync complete." after a failure and exit 0,
    // so a cron job or a monitoring check could not tell anything was wrong.
    println!(
        "{} Sync failed for {} of {} source(s):",
        "✗".red(),
        failed.len(),
        sources.len()
    );
    for (name, error) in &failed {
        println!("  {} {}: {}", "✗".red(), name, error);
    }
    println!(
        "  {}",
        "Availability keeps being served from the last successful sync of these sources.".dimmed()
    );
    anyhow::bail!(
        "{} of {} source(s) failed to sync",
        failed.len(),
        sources.len()
    )
}

/// Build the right client for a source row and sync it, recording the
/// outcome on the source (see [`crate::commands::sync_health`]), including
/// failures that happen before any request is sent. EWS sources go through
/// the provider trait (no OAuth2, no CalDAV-only sync-collection); CalDAV
/// sources keep the CalDAV flow.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn sync_one_source(
    pool: &SqlitePool,
    key: &[u8; 32],
    source_id: &str,
    url: &str,
    username: &str,
    password_enc: Option<&str>,
    auth_type: &str,
    access_token_enc: Option<&str>,
    token_expires_at: Option<&str>,
    provider_type: &str,
) -> Result<()> {
    if provider_type == kinds::MICROSOFT {
        let provider =
            match crate::microsoft_oauth::build_provider_for_source(pool, key, source_id).await {
                Ok(provider) => provider,
                Err(error) => {
                    crate::commands::sync_health::record_sync_failure(pool, key, source_id, &error)
                        .await;
                    return Err(error);
                }
            };
        return sync_provider_source(pool, key, provider.as_ref(), source_id).await;
    }
    if provider_type == kinds::EWS {
        let provider = crate::crypto::decrypt_password(key, password_enc.unwrap_or(""))
            .map_err(|e| anyhow::anyhow!("could not decrypt credentials: {e}"))
            .and_then(|password| {
                crate::providers::build_provider(provider_type, url, username, &password)
            });
        let provider = match provider {
            Ok(p) => p,
            Err(e) => {
                crate::commands::sync_health::record_sync_failure(pool, key, source_id, &e).await;
                return Err(e);
            }
        };
        return sync_provider_source(pool, key, provider.as_ref(), source_id).await;
    }

    let client = match crate::oauth2_caldav::build_client_for_source(
        pool,
        key,
        source_id,
        url,
        auth_type,
        username,
        password_enc,
        access_token_enc,
        token_expires_at,
    )
    .await
    {
        Ok(c) => c,
        Err(e) => {
            crate::commands::sync_health::record_sync_failure(pool, key, source_id, &e).await;
            return Err(e);
        }
    };
    sync_source(pool, key, &client, source_id).await
}

/// Sync a single CalDAV source: discover calendars and fetch events.
/// Uses ctag comparison to skip unchanged calendars.
/// Uses sync-token (RFC 6578) for delta sync when available, with fallback to full fetch.
///
/// The outcome is recorded on the source. A calendar that fails to fetch
/// fails the whole sync (after the others are synced), so the error is
/// visible instead of a silently stale calendar.
pub async fn sync_source(
    pool: &SqlitePool,
    key: &[u8; 32],
    client: &CaldavClient,
    source_id: &str,
) -> Result<()> {
    let result = sync_caldav_calendars(pool, key, client, source_id).await;
    crate::commands::sync_health::record_sync_outcome(pool, key, source_id, &result).await;
    result
}

/// Turn the calendars that failed during a sync into the sync's error.
fn calendar_failures_result(failures: &[String]) -> Result<()> {
    if failures.is_empty() {
        Ok(())
    } else {
        anyhow::bail!(
            "{} calendar(s) failed: {}",
            failures.len(),
            failures.join("; ")
        )
    }
}

async fn sync_caldav_calendars(
    pool: &SqlitePool,
    key: &[u8; 32],
    client: &CaldavClient,
    source_id: &str,
) -> Result<()> {
    let mut calendar_failures: Vec<String> = Vec::new();
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
                    calendar_failures.push(format!("{cal_label}: {e:#}"));
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

    calendar_failures_result(&calendar_failures)
}

/// Sync calendars for a user if any of their sources are stale (last_synced > STALE_SECS ago).
/// Uses sync-token delta when available, with fallback to full fetch.
/// Errors do not reach the caller (best-effort for guest-facing pages): slots
/// are served from the last good sync, and the failure is recorded on the
/// source. A source that failed within the last `STALE_SECS` is not retried
/// on demand, so a dead server costs a guest page one timeout every five
/// minutes rather than one per page load.
pub async fn sync_if_stale(pool: &SqlitePool, key: &[u8; 32], user_id: &str) {
    let cutoff = Utc::now() - chrono::Duration::seconds(STALE_SECS);
    // Must match SQLite datetime('now') format: "YYYY-MM-DD HH:MM:SS" (space, not T)
    let cutoff_str = cutoff.format("%Y-%m-%d %H:%M:%S").to_string();

    let stale_sources: Vec<(String, String, String, Option<String>, String, Option<String>, Option<String>, String)> = sqlx::query_as(
        "SELECT cs.id, cs.url, cs.username, cs.password_enc, cs.auth_type, cs.access_token_enc, cs.token_expires_at, cs.provider_type
         FROM caldav_sources cs
         JOIN accounts a ON a.id = cs.account_id
         WHERE a.user_id = ? AND cs.enabled = 1
           AND (cs.last_synced IS NULL OR cs.last_synced < ?2)
           AND (cs.last_sync_error_at IS NULL OR cs.last_sync_error_at < ?2)",
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

    for (
        source_id,
        url,
        username,
        password_enc,
        auth_type,
        access_token_enc,
        token_expires_at,
        provider_type,
    ) in &stale_sources
    {
        // Serialize on-demand syncs per source. If another task is already
        // syncing this source, we wait, then re-check staleness — almost
        // always the winner bumped last_synced and we can skip.
        let lock = source_lock(source_id).await;
        let _guard = lock.lock().await;

        // MAX() of the two: whichever of the last success and the last
        // failure is more recent says whether the source was just tried.
        let last_attempt: Option<String> = sqlx::query_scalar::<_, Option<String>>(
            "SELECT MAX(COALESCE(last_synced, ''), COALESCE(last_sync_error_at, '')) \
             FROM caldav_sources WHERE id = ?",
        )
        .bind(source_id)
        .fetch_optional(pool)
        .await
        .unwrap_or(None)
        .flatten();
        if last_attempt
            .as_deref()
            .is_some_and(|ls| ls >= cutoff_str.as_str())
        {
            tracing::debug!(
                source_id = %source_id,
                "skipping on-demand sync, another task already tried this source"
            );
            continue;
        }

        let _ = sync_one_source(
            pool,
            key,
            source_id,
            url,
            username,
            password_enc.as_deref(),
            auth_type,
            access_token_enc.as_deref(),
            token_expires_at.as_deref(),
            provider_type,
        )
        .await;
    }
}

/// Sync a single source by ID (for background sync loop).
/// Forces a full resync if last_full_sync is >24h ago (catches orphaned events).
pub async fn sync_source_by_id(pool: &SqlitePool, key: &[u8; 32], source_id: &str) {
    let source: Option<(String, String, Option<String>, Option<String>, String, Option<String>, Option<String>, String)> = sqlx::query_as(
        "SELECT url, username, password_enc, last_full_sync, auth_type, access_token_enc, token_expires_at, provider_type FROM caldav_sources WHERE id = ? AND enabled = 1",
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

    let _ = sync_one_source(
        pool,
        key,
        source_id,
        &url,
        &username,
        password_enc.as_deref(),
        &auth_type,
        access_token_enc.as_deref(),
        token_expires_at.as_deref(),
        &provider_type,
    )
    .await;
}

/// Full-snapshot sync for EWS and Microsoft Graph through [`crate::providers::CalendarProvider`].
/// CalDAV retains its protocol-specific ctag and RFC 6578 sync-token path.
/// These providers expand recurrences server-side, then reconcile the cache by UID.
///
/// The outcome is recorded on the source, as for [`sync_source`].
pub async fn sync_provider_source(
    pool: &SqlitePool,
    key: &[u8; 32],
    provider: &dyn crate::providers::CalendarProvider,
    source_id: &str,
) -> Result<()> {
    let result = sync_provider_calendars(pool, key, provider, source_id).await;
    crate::commands::sync_health::record_sync_outcome(pool, key, source_id, &result).await;
    result
}

async fn sync_provider_calendars(
    pool: &SqlitePool,
    key: &[u8; 32],
    provider: &dyn crate::providers::CalendarProvider,
    source_id: &str,
) -> Result<()> {
    let mut calendar_failures: Vec<String> = Vec::new();
    let calendars = provider.list_calendars().await?;
    remove_unlisted_provider_calendars(pool, source_id, &calendars).await?;

    let (provider_type, write_calendar): (String, Option<String>) = sqlx::query_as(
        "SELECT provider_type, write_calendar_href FROM caldav_sources WHERE id = ?",
    )
    .bind(source_id)
    .fetch_one(pool)
    .await?;
    let mut unreadable_calendars = 0usize;
    // Bounded fetch window. Matches the CalDAV path's FULL_FETCH_LOOKBACK_DAYS:
    // 90 days back is plenty for orphan reconciliation and keeps response
    // sizes predictable. Absence from a snapshot proves nothing outside
    // [since, until): EWS fetches SYNC_WINDOW_DAYS from since, Graph up to
    // its own now + SYNC_WINDOW_DAYS, taken after this `now`, so a subset.
    let now = Utc::now();
    let since_dt = now - chrono::Duration::days(FULL_FETCH_LOOKBACK_DAYS);
    let since_iso = since_dt.to_rfc3339();
    let until_dt = if provider_type == kinds::MICROSOFT {
        now + chrono::Duration::days(crate::providers::graph::SYNC_WINDOW_DAYS)
    } else {
        since_dt + chrono::Duration::days(crate::ews::SYNC_WINDOW_DAYS)
    };

    for cal_info in &calendars {
        let (cal_id, _stored_change_marker, _stored_sync_state) =
            upsert_calendar_provider(pool, source_id, cal_info).await?;
        let cal_label = cal_info.display_name.as_deref().unwrap_or(&cal_info.id);

        let snapshot = async {
            let raw_events = provider
                .fetch_events_since(&cal_info.id, &since_iso)
                .await?;
            let (raw_events, unreadable) = partition_provider_snapshot(raw_events);
            let count = upsert_provider_events(pool, &cal_id, &raw_events).await;
            // An item calrs could not read may be any cached event, so a
            // snapshot with one cannot prove that a cached event is gone.
            let deleted = if unreadable == 0 {
                remove_orphaned_provider_events(
                    pool,
                    key,
                    provider,
                    source_id,
                    &cal_id,
                    &raw_events,
                    (since_dt, until_dt),
                )
                .await?
            } else {
                tracing::warn!(
                    calendar_name = cal_label,
                    unreadable_events = unreadable,
                    "skipped calendar events calrs could not read; kept cached events until a clean snapshot"
                );
                0
            };
            Ok::<_, anyhow::Error>((count, deleted, unreadable))
        }
        .await;
        match snapshot {
            Ok((count, deleted, unreadable)) => {
                if deleted > 0 {
                    tracing::info!(
                        calendar_name = cal_label,
                        stale_events_removed = deleted,
                        "removed stale provider events from local cache"
                    );
                }
                println!(
                    "  {} {} — {} event(s) synced{}{}",
                    "✓".green(),
                    cal_label,
                    count,
                    if deleted > 0 {
                        format!(", {} removed", deleted)
                    } else {
                        String::new()
                    },
                    if unreadable > 0 {
                        format!(", {} unreadable skipped", unreadable)
                    } else {
                        String::new()
                    }
                );
                // The readable events are cached, but until every event can
                // be read, deletions on this calendar are not reconciled and
                // its vanished bookings are not cancelled. A permanent
                // condition must reach the owner, not only the log.
                if unreadable > 0 {
                    calendar_failures.push(format!(
                        "{cal_label}: {unreadable} event(s) calrs cannot read; \
                         events deleted from this calendar are kept until it can read them all"
                    ));
                }
            }
            // A calendar the granted access cannot read (shared by someone
            // else) is skipped, and its cache kept, rather than failing the
            // source for good. The write calendar is never skipped.
            Err(e)
                if e.downcast_ref::<crate::providers::CalendarUnreadable>()
                    .is_some()
                    && write_calendar.as_deref() != Some(cal_info.id.as_str()) =>
            {
                unreadable_calendars += 1;
                tracing::warn!(
                    calendar_name = cal_label,
                    "skipped a calendar calrs cannot read with the granted access"
                );
                println!("  {} {} — skipped: {}", "…".dimmed(), cal_label, e);
            }
            Err(e) => {
                println!("  {} {} — failed: {}", "✗".red(), cal_label, e);
                calendar_failures.push(format!("{cal_label}: {e:#}"));
            }
        }
    }
    if unreadable_calendars > 0 && unreadable_calendars == calendars.len() {
        calendar_failures.push(crate::providers::CalendarUnreadable.to_string());
    }

    let _ = sqlx::query("UPDATE caldav_sources SET last_full_sync = datetime('now') WHERE id = ?")
        .bind(source_id)
        .execute(pool)
        .await;
    sqlx::query("UPDATE caldav_sources SET last_synced = datetime('now') WHERE id = ?")
        .bind(source_id)
        .execute(pool)
        .await?;
    tracing::info!(source_id = %source_id, "calendar provider sync completed");
    calendar_failures_result(&calendar_failures)
}

/// Lists a provider source's calendars into the cache without fetching any
/// events, so the write calendar can be chosen while the first sync runs.
pub async fn discover_provider_calendars(
    pool: &SqlitePool,
    provider: &dyn crate::providers::CalendarProvider,
    source_id: &str,
) -> Result<usize> {
    let calendars = provider.list_calendars().await?;
    for cal_info in &calendars {
        upsert_calendar_provider(pool, source_id, cal_info).await?;
    }
    remove_unlisted_provider_calendars(pool, source_id, &calendars).await?;
    Ok(calendars.len())
}

/// Drops the cached calendars a provider no longer lists: deleted, or shared
/// by someone who stopped sharing it. Their events would otherwise keep
/// blocking slots, since nothing else reads that calendar again. Its events
/// and event-type selections go with it (ON DELETE CASCADE); an event type
/// left with no selected calendar checks every busy calendar again.
async fn remove_unlisted_provider_calendars(
    pool: &SqlitePool,
    source_id: &str,
    calendars: &[crate::providers::RemoteCalendar],
) -> Result<()> {
    // Every mailbox has a calendar: an empty listing is not proof that all
    // of them went away.
    if calendars.is_empty() {
        return Ok(());
    }
    let listed: std::collections::HashSet<&str> = calendars.iter().map(|c| c.id.as_str()).collect();
    let cached: Vec<(String, String, Option<String>)> =
        sqlx::query_as("SELECT id, href, display_name FROM calendars WHERE source_id = ?")
            .bind(source_id)
            .fetch_all(pool)
            .await?;
    for (id, href, name) in cached {
        if listed.contains(href.as_str()) {
            continue;
        }
        sqlx::query("DELETE FROM calendars WHERE id = ?")
            .bind(&id)
            .execute(pool)
            .await?;
        tracing::info!(
            source_id = %source_id,
            calendar_name = name.as_deref().unwrap_or(&href),
            "removed a calendar the provider no longer lists"
        );
    }
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
            sqlx::query(
                "UPDATE calendars SET display_name = ?, color = ?, read_only = ? WHERE id = ?",
            )
            .bind(&cal_info.display_name)
            .bind(&cal_info.color)
            .bind(cal_info.read_only)
            .bind(&id)
            .execute(pool)
            .await?;
            Ok((id, ctag, sync_token))
        }
        None => {
            let id = Uuid::new_v4().to_string();
            // Someone else's calendar does not make the user busy until they
            // say so: a colleague's meetings are not the host's. Only the
            // first discovery decides; the user's later choice stands.
            sqlx::query(
                "INSERT INTO calendars (id, source_id, href, display_name, color, ctag, is_busy, read_only) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&id)
            .bind(source_id)
            .bind(&cal_info.id)
            .bind(&cal_info.display_name)
            .bind(&cal_info.color)
            .bind(&cal_info.change_marker)
            .bind(!cal_info.shared)
            .bind(cal_info.read_only)
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
        let mut zones = crate::tzid::ResourceZones::new(&raw.ical);
        for vevent in &vevent_blocks {
            // Unfolded once for the field reads below (#267).
            let fields = crate::utils::unfold_ical(vevent);
            let uid =
                extract_vevent_field(&fields, "UID").unwrap_or_else(|| Uuid::new_v4().to_string());
            let summary = extract_vevent_field(&fields, "SUMMARY");
            let start_at = extract_vevent_field(&fields, "DTSTART").unwrap_or_default();
            let location = extract_vevent_field(&fields, "LOCATION");
            let description = extract_vevent_field(&fields, "DESCRIPTION");
            let status = extract_vevent_field(&fields, "STATUS");
            let rrule = extract_vevent_field(&fields, "RRULE");
            let recurrence_id = extract_vevent_field(&fields, "RECURRENCE-ID");
            let transp = extract_vevent_field(&fields, "TRANSP");
            let timezone = zones.event_timezone(&fields);
            let end_at = crate::utils::extract_vevent_end(
                &fields,
                timezone.as_deref().and_then(crate::tzid::resolve_or_offset),
            )
            .unwrap_or_default();

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

/// Split a snapshot into the items calrs can cache and the number it cannot:
/// an incomplete VEVENT, no UID, a start or end that does not parse, or an
/// item the provider listed but could not convert (an empty `ical`).
fn partition_provider_snapshot(
    raw_events: Vec<ProviderRawEvent>,
) -> (Vec<ProviderRawEvent>, usize) {
    let (readable, unreadable): (Vec<_>, Vec<_>) =
        raw_events.into_iter().partition(provider_event_is_readable);
    (readable, unreadable.len())
}

fn provider_event_is_readable(raw: &ProviderRawEvent) -> bool {
    let events = split_vevents(&raw.ical);
    let count = |marker: &str| raw.ical.lines().filter(|line| *line == marker).count();
    if events.is_empty()
        || count("BEGIN:VEVENT") != events.len()
        || count("END:VEVENT") != events.len()
    {
        return false;
    }
    events.iter().all(|event| {
        let fields = crate::utils::unfold_ical(event);
        let parses = |value: Option<String>| {
            value
                .and_then(|value| crate::utils::parse_ical_datetime(&value))
                .is_some()
        };
        extract_vevent_field(&fields, "UID").is_some_and(|uid| !uid.is_empty())
            && parses(extract_vevent_field(&fields, "DTSTART"))
            && parses(crate::utils::extract_vevent_end(&fields, None))
    })
}

/// Orphan reconciliation for snapshot providers. Only a readable snapshot
/// reaches this point, so an empty one (the last event was deleted) removes
/// cached events too. Removal is restricted to starts inside
/// `[since, until)`: absence outside the fetched window proves nothing.
/// Timestamps are compared as instants rather than date prefixes, which would
/// include the unfetched part of the first day.
///
/// A vanished event's booking goes through [`cancel_orphaned_booking`] like
/// on CalDAV, and only on the source's write calendar: scoped to this source
/// and host, confirmed gone with `provider` first, and both sides emailed.
async fn remove_orphaned_provider_events(
    pool: &SqlitePool,
    key: &[u8; 32],
    provider: &dyn crate::providers::CalendarProvider,
    source_id: &str,
    cal_id: &str,
    raw_events: &[ProviderRawEvent],
    (since, until): (chrono::DateTime<Utc>, chrono::DateTime<Utc>),
) -> Result<u32> {
    let is_write_calendar: Option<bool> = sqlx::query_scalar(
        "SELECT COALESCE(c.href = cs.write_calendar_href, 0)
         FROM calendars c JOIN caldav_sources cs ON cs.id = c.source_id
         WHERE c.id = ? AND cs.id = ?",
    )
    .bind(cal_id)
    .bind(source_id)
    .fetch_optional(pool)
    .await?;
    let Some(is_write_calendar) = is_write_calendar else {
        return Ok(0);
    };
    let mut seen_uids = std::collections::HashSet::new();
    for raw in raw_events {
        for vevent in split_vevents(&raw.ical) {
            let fields = crate::utils::unfold_ical(&vevent);
            let uid = extract_vevent_field(&fields, "UID")
                .ok_or_else(|| anyhow::anyhow!("provider event has no UID"))?;
            let recurrence_id = extract_vevent_field(&fields, "RECURRENCE-ID");
            seen_uids.insert((uid, recurrence_id.unwrap_or_default()));
        }
    }

    let local_events: Vec<(String, String, Option<String>, String, Option<String>)> =
        sqlx::query_as(
            "SELECT id, uid, recurrence_id, start_at, timezone FROM events WHERE calendar_id = ?",
        )
        .bind(cal_id)
        .fetch_all(pool)
        .await?;
    let mut deleted = 0u32;
    for (event_id, uid, recurrence_id, start_at, timezone) in local_events {
        let identity = (uid, recurrence_id.unwrap_or_default());
        if seen_uids.contains(&identity) {
            continue;
        }
        let (uid, _) = identity;
        let Some(start) = crate::utils::parse_ical_datetime(&start_at) else {
            continue;
        };
        let start = match timezone.as_deref().filter(|_| !start_at.ends_with('Z')) {
            Some(tzid) => {
                use chrono::TimeZone;
                let Some(start) = crate::tzid::resolve_or_offset(tzid)
                    .and_then(|tz| tz.from_local_datetime(&start).earliest())
                else {
                    continue;
                };
                start.with_timezone(&Utc)
            }
            None => start.and_utc(),
        };
        if start < since || start >= until {
            continue;
        }
        if is_write_calendar {
            // Decide before dropping the cached event: there is no
            // booking-level sweep here, so when the provider could not say
            // whether the booking's event is gone, the cached row is what
            // brings it back for another check on the next sync.
            let outcome = cancel_orphaned_booking(
                pool,
                key,
                OrphanCheck::Provider(provider),
                source_id,
                &uid,
            )
            .await;
            if outcome == OrphanOutcome::Inconclusive {
                continue;
            }
        }
        let result = sqlx::query("DELETE FROM events WHERE id = ?")
            .bind(&event_id)
            .execute(pool)
            .await?;
        deleted += result.rows_affected() as u32;
    }
    Ok(deleted)
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
        let mut zones = crate::tzid::ResourceZones::new(&raw.ical_data);
        for vevent in &vevent_blocks {
            // Unfolded once for the field reads below (#267).
            let fields = crate::utils::unfold_ical(vevent);
            let uid =
                extract_vevent_field(&fields, "UID").unwrap_or_else(|| Uuid::new_v4().to_string());
            let summary = extract_vevent_field(&fields, "SUMMARY");
            let start_at = extract_vevent_field(&fields, "DTSTART").unwrap_or_default();
            let location = extract_vevent_field(&fields, "LOCATION");
            let description = extract_vevent_field(&fields, "DESCRIPTION");
            let status = extract_vevent_field(&fields, "STATUS");
            let rrule = extract_vevent_field(&fields, "RRULE");
            let recurrence_id = extract_vevent_field(&fields, "RECURRENCE-ID");
            let transp = extract_vevent_field(&fields, "TRANSP");
            let timezone = zones.event_timezone(&fields);
            let end_at = crate::utils::extract_vevent_end(
                &fields,
                timezone.as_deref().and_then(crate::tzid::resolve_or_offset),
            )
            .unwrap_or_default();

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
            let _ = cancel_orphaned_booking(pool, key, OrphanCheck::Caldav(client), source_id, uid)
                .await;
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
            let _ = cancel_orphaned_booking(pool, key, OrphanCheck::Caldav(client), source_id, uid)
                .await;
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
/// What confirms, before an orphaned booking is cancelled, that its event is
/// really gone from the host's calendar.
#[derive(Clone, Copy)]
enum OrphanCheck<'a> {
    /// CalDAV: a HEAD/PROPFIND on the event. `None` skips the check (paths
    /// with no client at hand).
    Caldav(Option<&'a CaldavClient>),
    /// Any other provider (EWS): a lookup by UID in the write calendar.
    Provider(&'a dyn crate::providers::CalendarProvider),
}

/// What [`cancel_orphaned_booking`] did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OrphanOutcome {
    /// Nothing to do: no confirmed booking of this source's host has this
    /// UID, the meeting is already over, or another path changed it first.
    NoAction,
    /// The booking was cancelled.
    Cancelled,
    /// The server says the booking's event is still there.
    StillPresent,
    /// The server could not say: the booking is left as it is.
    Inconclusive,
}

async fn cancel_orphaned_booking(
    pool: &SqlitePool,
    key: &[u8; 32],
    check: OrphanCheck<'_>,
    source_id: &str,
    uid: &str,
) -> OrphanOutcome {
    // Fetch booking details before cancelling. Scoped by source_id via the
    // caldav_sources join: the booking must have been pushed to this
    // source's write calendar. The host contact is the assigned member
    // when set (#147), the event type owner otherwise.
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
        Option<String>,
    )> = sqlx::query_as(
        "SELECT b.id, b.guest_name, b.guest_email, COALESCE(b.guest_timezone, 'UTC'), CASE b.time_version WHEN 1 THEN b.start_at ELSE rtrim(b.start_at, 'Z') END AS start_at, CASE b.time_version WHEN 1 THEN b.end_at ELSE rtrim(b.end_at, 'Z') END AS end_at, b.uid,
                et.title, u.name, COALESCE(u.booking_email, u.email), b.caldav_calendar_href, u.timezone,
                u.language
         FROM bookings b
         JOIN event_types et ON et.id = b.event_type_id
         JOIN accounts a ON a.id = et.account_id
         JOIN users u ON u.id = COALESCE(b.assigned_user_id, a.user_id)
         JOIN caldav_sources cs ON cs.id = ?
         JOIN accounts sa ON sa.id = cs.account_id AND sa.user_id = u.id
         WHERE b.uid = ? AND b.status = 'confirmed'
           AND b.caldav_calendar_href = cs.write_calendar_href",
    )
    .bind(source_id)
    .bind(uid)
    .fetch_optional(pool)
    .await
    .unwrap_or(None);

    let booking = match booking {
        Some(b) => b,
        None => return OrphanOutcome::NoAction, // No confirmed booking of ours with this UID
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
        host_language,
    ) = booking;
    let host_language = crate::email::supported_language(host_language);

    // A meeting that is already over is left alone: a host clearing past
    // events from their calendar must not send the guest a cancellation.
    // Legacy rows (no UTC timestamp) keep the former behaviour.
    let ended = crate::booking_time::ics_times(&start_at, &end_at)
        .and_then(|(_, end)| chrono::NaiveDateTime::parse_from_str(&end, "%Y%m%dT%H%M%SZ").ok())
        .is_some_and(|end| end <= Utc::now().naive_utc());
    if ended {
        return OrphanOutcome::NoAction;
    }

    // Confirm-before-cancel: if we can check and have a stored calendar href
    // for this booking, verify that the event is really gone on the server
    // (404 on CalDAV, no item with this UID on EWS). Any other outcome (still
    // there, server flake, network down) means we cannot prove the host
    // deleted the event, so we skip the cancel.
    if let Some(cal_href) = caldav_calendar_href.as_deref() {
        let exists = match check {
            OrphanCheck::Caldav(Some(client)) => Some(client.event_exists(cal_href, uid).await),
            OrphanCheck::Caldav(None) => None,
            OrphanCheck::Provider(provider) => Some(provider.event_exists(cal_href, uid).await),
        };
        match exists {
            None => {}
            Some(Ok(false)) => {
                // The server confirms it is gone: legitimate deletion, proceed.
            }
            Some(Ok(true)) => {
                tracing::warn!(
                    uid = %uid,
                    booking_id = %booking_id,
                    cal_href = %cal_href,
                    "skipping booking cancellation: the calendar event is still present on the server \
                     (a sync path reported it as deleted but verification disagrees)"
                );
                return OrphanOutcome::StillPresent;
            }
            Some(Err(e)) => {
                tracing::warn!(
                    uid = %uid,
                    booking_id = %booking_id,
                    cal_href = %cal_href,
                    error = %e,
                    "skipping booking cancellation: could not verify resource state on the server \
                     (treating inconclusive verification as not-deleted to avoid false positives)"
                );
                return OrphanOutcome::Inconclusive;
            }
        }
    }

    // Cancel the booking. The SEQUENCE bump makes the CANCEL outrank the
    // copy its holders have (RFC 5546, migration 072).
    let updated = sqlx::query(
        "UPDATE bookings SET status = 'cancelled', ics_sequence = ics_sequence + 1 WHERE id = ? AND status = 'confirmed'",
    )
    .bind(&booking_id)
    .execute(pool)
    .await;

    let cancelled = matches!(updated, Ok(r) if r.rows_affected() > 0);
    if !cancelled {
        return OrphanOutcome::NoAction;
    }

    tracing::info!(
        uid = %uid,
        booking_id = %booking_id,
        "booking cancelled: calendar event deleted externally"
    );
    // The host's event is already gone; a claimant's copy is not (#269).
    crate::web::claimant_delete_booking(pool, key, &booking_uid).await;

    // Send cancellation emails
    let smtp_config = match crate::email::load_smtp_config(pool, key).await {
        Ok(Some(cfg)) => cfg,
        _ => return OrphanOutcome::Cancelled, // No SMTP configured, skip email
    };

    let utc_times = crate::booking_time::ics_times(&start_at, &end_at);
    let (start_at, end_at) = crate::booking_time::wall_strings(
        &start_at,
        &end_at,
        guest_timezone.parse().unwrap_or(chrono_tz::Tz::UTC),
    );
    let date = start_at.get(..10).unwrap_or(&start_at).to_string();
    let start_time = extract_time(&start_at);
    let end_time = extract_time(&end_at);

    // The guest and the host each read the reason in their own language.
    let guest_language = crate::email::stored_booking_language(pool, &booking_id).await;
    let sequence = crate::web::booking_ics_sequence(pool, &booking_uid).await;
    let details = crate::email::CancellationDetails {
        utc_times,
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
        reason: Some(crate::email::calendar_deleted_reason(
            guest_language.as_deref(),
        )),
        cancelled_by_host: true,
        guest_language,
        host_timezone: host_timezone.unwrap_or_default(),
        sequence,
        ..Default::default()
    };
    let host_details = crate::email::CancellationDetails {
        reason: Some(crate::email::calendar_deleted_reason(
            host_language.as_deref(),
        )),
        host_language,
        ..details.clone()
    };
    // The guest's CANCEL must carry the invite's ORGANIZER, or calendar
    // clients that match on UID + ORGANIZER ignore it. For a collective
    // booking that is the stored organizer (#161), not whichever member's
    // calendar lost the event.
    let details = match crate::web::booking_host_identity(pool, &details.uid).await {
        Some((host_name, host_email, _)) => crate::email::CancellationDetails {
            host_name,
            host_email,
            ..details
        },
        None => details,
    };
    // Its attendees hold the invite too (#267).
    let details = crate::web::with_invite_holders(pool, details, false).await;

    if let Err(e) = crate::email::send_guest_cancellation(&smtp_config, &details).await {
        tracing::warn!(
            error = format!("{e:#}"),
            "failed to send external cancellation email to guest"
        );
    }
    let _ = crate::email::send_guest_attendee_cancellations(&smtp_config, &details).await;
    if let Err(e) = crate::email::send_host_cancellation(&smtp_config, &host_details).await {
        tracing::warn!(
            error = format!("{e:#}"),
            "failed to send external cancellation email to host"
        );
    }
    OrphanOutcome::Cancelled
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
    // Scope by the calendar the event was actually pushed to, not by the
    // event type owner's account: team bookings are written to the
    // assigned member's calendar (#147), and sweeping them from the
    // owner's source would auto-cancel bookings whose event simply hasn't
    // been synced into `events` by the member's source yet. Hrefs are not
    // globally unique across servers, so the source must also belong to
    // the booking's effective host (assigned member, else owner). The uid
    // lookup stays deliberately global: confirm-before-cancel is not
    // available on every path, and a write calendar that is not among the
    // synced ones must not read as "everything orphaned".
    let orphans: Vec<(String,)> = sqlx::query_as(
        "SELECT b.uid FROM bookings b
         JOIN event_types et ON et.id = b.event_type_id
         JOIN accounts a ON a.id = et.account_id
         JOIN caldav_sources cs ON cs.id = ?
         JOIN accounts sa ON sa.id = cs.account_id
                         AND sa.user_id = COALESCE(b.assigned_user_id, a.user_id)
         WHERE b.status = 'confirmed'
           AND b.caldav_calendar_href IS NOT NULL
           AND b.caldav_calendar_href = cs.write_calendar_href
           AND b.uid NOT IN (SELECT uid FROM events)",
    )
    .bind(source_id)
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    for (uid,) in &orphans {
        let _ =
            cancel_orphaned_booking(pool, key, OrphanCheck::Caldav(client), source_id, uid).await;
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

    /// #282: Fastmail writes DURATION instead of DTEND. The stored end must be
    /// derived, or the event never blocks a slot (the busy queries compare
    /// end_at), and computed in the zone the TZID resolves to.
    #[tokio::test]
    async fn sync_stores_an_end_derived_from_duration() {
        let pool = setup_test_db().await;
        let (source_id, _) = seed_fixtures(&pool).await;
        let cal_id = Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO calendars (id, source_id, href, display_name, is_busy) VALUES (?, ?, '/c/', 'C', 1)")
            .bind(&cal_id).bind(&source_id).execute(&pool).await.unwrap();

        let ical = "BEGIN:VCALENDAR\r\nBEGIN:VEVENT\r\nUID:fastmail-1\r\nDTSTART;TZID=\"W. Europe Standard Time\":20261024T120000\r\nDURATION:PT24H\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";
        let raw = vec![RawEvent {
            href: "/c/fastmail-1.ics".to_string(),
            ical_data: ical.to_string(),
        }];
        assert_eq!(upsert_raw_events(&pool, &cal_id, &raw).await, 1);

        let (end_at, timezone): (String, Option<String>) =
            sqlx::query_as("SELECT end_at, timezone FROM events WHERE uid = 'fastmail-1'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(end_at, "20261025T110000");
        assert_eq!(timezone.as_deref(), Some("Europe/Berlin"));
    }

    /// A provider whose only job is answering `event_exists`: `Some(found)`,
    /// or `None` for a lookup that fails.
    struct ExistsProvider(Option<bool>);

    #[async_trait::async_trait]
    impl crate::providers::CalendarProvider for ExistsProvider {
        async fn check_connection(&self) -> Result<bool> {
            Ok(true)
        }
        async fn list_calendars(&self) -> Result<Vec<crate::providers::RemoteCalendar>> {
            Ok(vec![])
        }
        async fn fetch_events(&self, _: &str) -> Result<Vec<ProviderRawEvent>> {
            Ok(vec![])
        }
        async fn fetch_events_since(&self, _: &str, _: &str) -> Result<Vec<ProviderRawEvent>> {
            Ok(vec![])
        }
        async fn sync_delta(
            &self,
            _: &str,
            _: Option<&str>,
        ) -> Result<crate::providers::DeltaResult> {
            Ok(Default::default())
        }
        async fn put_event(&self, _: &str, _: &str, _: &str) -> Result<()> {
            Ok(())
        }
        async fn delete_event(&self, _: &str, _: &str) -> Result<()> {
            Ok(())
        }
        async fn event_exists(&self, _: &str, _: &str) -> Result<bool> {
            self.0
                .ok_or_else(|| anyhow::anyhow!("EWS lookup timed out"))
        }
    }

    /// An EWS calendar whose cached event `uid` is missing from the latest
    /// snapshot (which still holds another event), with a confirmed booking
    /// under that uid written to the source's write calendar. Returns
    /// (source_id, calendar_id, booking_id).
    async fn seed_vanished_ews_booking(pool: &SqlitePool, uid: &str) -> (String, String, String) {
        let (source_id, et_id) = seed_fixtures(pool).await;
        let cal_id = Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO calendars (id, source_id, href, display_name, is_busy) VALUES (?, ?, '/calendars/host/default/', 'C', 1)")
            .bind(&cal_id).bind(&source_id).execute(pool).await.unwrap();
        sqlx::query("INSERT INTO events (id, calendar_id, uid, start_at, end_at) VALUES (?, ?, ?, '20300615T100000', '20300615T103000')")
            .bind(Uuid::new_v4().to_string()).bind(&cal_id).bind(uid).execute(pool).await.unwrap();
        let booking_id = Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO bookings (id, event_type_id, uid, guest_name, guest_email, guest_timezone,
                start_at, end_at, status, cancel_token, reschedule_token, caldav_calendar_href)
             VALUES (?, ?, ?, 'Guest', 'guest@example.com', 'UTC',
                '2030-06-15T10:00:00', '2030-06-15T10:30:00', 'confirmed', ?, ?,
                '/calendars/host/default/')",
        )
        .bind(&booking_id)
        .bind(&et_id)
        .bind(uid)
        .bind(Uuid::new_v4().to_string())
        .bind(Uuid::new_v4().to_string())
        .execute(pool)
        .await
        .unwrap();
        (source_id, cal_id, booking_id)
    }

    fn other_event_snapshot() -> Vec<ProviderRawEvent> {
        vec![ProviderRawEvent {
            remote_id: "AAMk-other".to_string(),
            ical: "BEGIN:VCALENDAR\r\nBEGIN:VEVENT\r\nUID:still-there\r\nDTSTART:20300616T100000Z\r\nDTEND:20300616T110000Z\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n".to_string(),
        }]
    }

    /// A reconciliation window no cached event falls outside of.
    fn whole_window() -> (chrono::DateTime<Utc>, chrono::DateTime<Utc>) {
        (
            chrono::DateTime::<Utc>::MIN_UTC,
            chrono::DateTime::<Utc>::MAX_UTC,
        )
    }

    async fn booking_status(pool: &SqlitePool, id: &str) -> String {
        sqlx::query_scalar("SELECT status FROM bookings WHERE id = ?")
            .bind(id)
            .fetch_one(pool)
            .await
            .unwrap()
    }

    /// EWS used to cancel any confirmed booking with the vanished uid
    /// straight in the database: no check that the event was really gone,
    /// no scoping to the source or host, no email.
    #[tokio::test]
    async fn ews_sweep_cancels_only_when_exchange_confirms_the_event_is_gone() {
        for (exists, expected) in [
            (Some(true), "confirmed"),
            (None, "confirmed"),
            (Some(false), "cancelled"),
        ] {
            let pool = setup_test_db().await;
            let key = [0u8; 32];
            let (source_id, cal_id, booking_id) =
                seed_vanished_ews_booking(&pool, "booking-uid").await;
            let removed = remove_orphaned_provider_events(
                &pool,
                &key,
                &ExistsProvider(exists),
                &source_id,
                &cal_id,
                &other_event_snapshot(),
                whole_window(),
            )
            .await
            .unwrap();
            assert_eq!(
                booking_status(&pool, &booking_id).await,
                expected,
                "{exists:?}"
            );
            // When Exchange could not tell, the cached event stays, so the
            // next sync checks again rather than leaving it confirmed for good.
            let cached: i64 =
                sqlx::query_scalar("SELECT COUNT(*) FROM events WHERE uid = 'booking-uid'")
                    .fetch_one(&pool)
                    .await
                    .unwrap();
            let gone = exists.is_some();
            assert_eq!(removed, u32::from(gone), "{exists:?}");
            assert_eq!(cached, i64::from(!gone), "{exists:?}");
        }
    }

    #[tokio::test]
    async fn orphan_sweep_does_not_cancel_a_meeting_that_is_over() {
        let pool = setup_test_db().await;
        let key = [0u8; 32];
        let (source_id, cal_id, booking_id) = seed_vanished_ews_booking(&pool, "past-uid").await;
        sqlx::query("UPDATE bookings SET time_version = 1, start_at = '2020-06-15T10:00:00Z', end_at = '2020-06-15T10:30:00Z' WHERE id = ?")
            .bind(&booking_id).execute(&pool).await.unwrap();
        let removed = remove_orphaned_provider_events(
            &pool,
            &key,
            &ExistsProvider(Some(false)),
            &source_id,
            &cal_id,
            &other_event_snapshot(),
            whole_window(),
        )
        .await
        .unwrap();
        assert_eq!(booking_status(&pool, &booking_id).await, "confirmed");
        assert_eq!(removed, 1, "the cached copy of a past meeting is dropped");
    }

    #[tokio::test]
    async fn ews_sweep_leaves_another_hosts_booking_alone() {
        let pool = setup_test_db().await;
        let key = [0u8; 32];
        let (source_id, cal_id, _) = seed_vanished_ews_booking(&pool, "own-uid").await;

        // Another host's booking, written to a calendar with the same href on
        // their own server, whose invite copy sat in this host's calendar.
        let (user, account, et) = (
            Uuid::new_v4().to_string(),
            Uuid::new_v4().to_string(),
            Uuid::new_v4().to_string(),
        );
        sqlx::query("INSERT INTO users (id, email, name, role, auth_provider, username, enabled) VALUES (?, 'other@example.com', 'Other', 'user', 'local', 'other', 1)")
            .bind(&user).execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO accounts (id, name, email, timezone, user_id) VALUES (?, 'Other', 'other@example.com', 'UTC', ?)")
            .bind(&account).bind(&user).execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO event_types (id, account_id, slug, title, duration_min) VALUES (?, ?, 'demo', 'Demo', 30)")
            .bind(&et).bind(&account).execute(&pool).await.unwrap();
        let other_booking = Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO bookings (id, event_type_id, uid, guest_name, guest_email, guest_timezone,
                start_at, end_at, status, cancel_token, reschedule_token, caldav_calendar_href)
             VALUES (?, ?, 'shared-uid', 'Guest', 'guest@example.com', 'UTC',
                '2030-06-17T10:00:00', '2030-06-17T10:30:00', 'confirmed', 'ct2', 'rt2',
                '/calendars/host/default/')",
        )
        .bind(&other_booking)
        .bind(&et)
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query("INSERT INTO events (id, calendar_id, uid, start_at, end_at) VALUES (?, ?, 'shared-uid', '20300617T100000', '20300617T103000')")
            .bind(Uuid::new_v4().to_string()).bind(&cal_id).execute(&pool).await.unwrap();

        remove_orphaned_provider_events(
            &pool,
            &key,
            &ExistsProvider(Some(false)),
            &source_id,
            &cal_id,
            &other_event_snapshot(),
            whole_window(),
        )
        .await
        .unwrap();
        assert_eq!(booking_status(&pool, &other_booking).await, "confirmed");
    }

    struct SnapshotProvider {
        calendar: crate::providers::RemoteCalendar,
        /// Listed after `calendar`, each with no events.
        others: Vec<crate::providers::RemoteCalendar>,
        /// Calendars whose fetch answers "cannot be read with this access".
        unreadable: Vec<String>,
        events: Vec<ProviderRawEvent>,
        fetch_fails: bool,
        exists: Option<bool>,
    }

    #[async_trait::async_trait]
    impl crate::providers::CalendarProvider for SnapshotProvider {
        async fn check_connection(&self) -> Result<bool> {
            Ok(true)
        }
        async fn list_calendars(&self) -> Result<Vec<crate::providers::RemoteCalendar>> {
            Ok(std::iter::once(self.calendar.clone())
                .chain(self.others.iter().cloned())
                .collect())
        }
        async fn fetch_events(&self, calendar: &str) -> Result<Vec<ProviderRawEvent>> {
            if self.unreadable.iter().any(|id| id == calendar) {
                return Err(crate::providers::CalendarUnreadable.into());
            }
            if self.fetch_fails {
                anyhow::bail!("later page failed; partial snapshot discarded");
            }
            if calendar != self.calendar.id {
                return Ok(Vec::new());
            }
            Ok(self.events.clone())
        }
        async fn fetch_events_since(
            &self,
            calendar: &str,
            _: &str,
        ) -> Result<Vec<ProviderRawEvent>> {
            self.fetch_events(calendar).await
        }
        async fn sync_delta(
            &self,
            _: &str,
            _: Option<&str>,
        ) -> Result<crate::providers::DeltaResult> {
            unreachable!("snapshot sync must not request a delta")
        }
        async fn put_event(&self, _: &str, _: &str, _: &str) -> Result<()> {
            unreachable!("sync must not write remote events")
        }
        async fn delete_event(&self, _: &str, _: &str) -> Result<()> {
            unreachable!("sync must not delete remote events")
        }
        async fn event_exists(&self, _: &str, _: &str) -> Result<bool> {
            self.exists
                .ok_or_else(|| anyhow::anyhow!("read-back inconclusive"))
        }
    }

    fn remote_calendar(href: &str) -> crate::providers::RemoteCalendar {
        crate::providers::RemoteCalendar {
            id: href.to_string(),
            display_name: None,
            color: None,
            change_marker: None,
            sync_state: None,
            read_only: false,
            shared: false,
        }
    }

    fn snapshot_provider(href: &str) -> SnapshotProvider {
        SnapshotProvider {
            calendar: remote_calendar(href),
            others: Vec::new(),
            unreadable: Vec::new(),
            events: Vec::new(),
            fetch_fails: false,
            exists: Some(false),
        }
    }

    async fn provider_fixture(pool: &SqlitePool, kind: &str) -> (String, String, String, String) {
        let user = Uuid::new_v4().to_string();
        let account = Uuid::new_v4().to_string();
        let source = Uuid::new_v4().to_string();
        let event_type = Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO users (id, email, name, username) VALUES (?, ?, 'Host', ?)")
            .bind(&user)
            .bind(format!("{user}@example.com"))
            .bind(&user)
            .execute(pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO accounts (id, name, email, timezone, user_id) VALUES (?, 'Host', ?, 'UTC', ?)")
            .bind(&account).bind(format!("{user}@example.com")).bind(&user)
            .execute(pool).await.unwrap();
        sqlx::query("INSERT INTO event_types (id, account_id, slug, title, duration_min) VALUES (?, ?, 'intro', 'Intro', 30)")
            .bind(&event_type).bind(&account).execute(pool).await.unwrap();
        sqlx::query("INSERT INTO caldav_sources (id, account_id, name, url, username, write_calendar_href, provider_type) VALUES (?, ?, 'Provider', 'https://example.com/', 'host', 'write', ?)")
            .bind(&source).bind(&account).bind(kind).execute(pool).await.unwrap();
        let (calendar, _, _) =
            upsert_calendar_provider(pool, &source, &snapshot_provider("write").calendar)
                .await
                .unwrap();
        (source, event_type, user, calendar)
    }

    async fn provider_booking(pool: &SqlitePool, event_type: &str, uid: &str) -> String {
        let id = Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO bookings (id, event_type_id, uid, guest_name, guest_email, guest_timezone,
             start_at, end_at, status, cancel_token, reschedule_token, caldav_calendar_href)
             VALUES (?, ?, ?, 'Guest', 'guest@example.com', 'UTC',
             '2030-06-15T10:00:00Z', '2030-06-15T10:30:00Z', 'confirmed', ?, ?, 'write')",
        )
        .bind(&id)
        .bind(event_type)
        .bind(uid)
        .bind(Uuid::new_v4().to_string())
        .bind(Uuid::new_v4().to_string())
        .execute(pool)
        .await
        .unwrap();
        id
    }

    async fn cached_provider_event(pool: &SqlitePool, calendar: &str, uid: &str, start: &str) {
        sqlx::query(
            "INSERT INTO events (id, calendar_id, uid, start_at, end_at) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(Uuid::new_v4().to_string())
        .bind(calendar)
        .bind(uid)
        .bind(start)
        .bind(start)
        .execute(pool)
        .await
        .unwrap();
    }

    async fn booking_status_seq(pool: &SqlitePool, booking: &str) -> (String, i64) {
        sqlx::query_as("SELECT status, ics_sequence FROM bookings WHERE id = ?")
            .bind(booking)
            .fetch_one(pool)
            .await
            .unwrap()
    }

    async fn cached_uids(pool: &SqlitePool, calendar: &str) -> Vec<String> {
        sqlx::query_scalar("SELECT uid FROM events WHERE calendar_id = ? ORDER BY uid")
            .bind(calendar)
            .fetch_all(pool)
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn provider_orphan_does_not_cross_owner_boundary_even_with_matching_calendar_ids() {
        let pool = setup_test_db().await;
        let (_source_a, et_a, _, _) = provider_fixture(&pool, kinds::EWS).await;
        let (source_b, _, _, cal_b) = provider_fixture(&pool, kinds::EWS).await;
        let booking = provider_booking(&pool, &et_a, "colliding-uid").await;
        cached_provider_event(
            &pool,
            &cal_b,
            "colliding-uid",
            &Utc::now().format("%Y%m%dT%H%M%SZ").to_string(),
        )
        .await;

        sync_provider_calendars(&pool, &[0; 32], &snapshot_provider("write"), &source_b)
            .await
            .unwrap();

        assert!(cached_uids(&pool, &cal_b).await.is_empty());
        assert_eq!(
            booking_status_seq(&pool, &booking).await,
            ("confirmed".into(), 0)
        );
    }

    #[tokio::test]
    async fn provider_orphan_only_cancels_on_designated_write_calendar() {
        let pool = setup_test_db().await;
        let (source, et, _, write_calendar) = provider_fixture(&pool, kinds::EWS).await;
        let booking = provider_booking(&pool, &et, "owned-uid").await;
        let (secondary, _, _) =
            upsert_calendar_provider(&pool, &source, &snapshot_provider("secondary").calendar)
                .await
                .unwrap();
        let start = Utc::now().format("%Y%m%dT%H%M%SZ").to_string();
        cached_provider_event(&pool, &secondary, "owned-uid", &start).await;
        cached_provider_event(&pool, &write_calendar, "owned-uid", &start).await;

        // The copy is gone from the secondary calendar but still on the
        // write calendar.
        let mut provider = snapshot_provider("write");
        provider.others.push(remote_calendar("secondary"));
        provider.events.push(ProviderRawEvent {
            remote_id: "owned".into(),
            ical: format!("BEGIN:VEVENT\r\nUID:owned-uid\r\nDTSTART:{start}\r\nDTEND:{start}\r\nEND:VEVENT\r\n"),
        });
        sync_provider_calendars(&pool, &[0; 32], &provider, &source)
            .await
            .unwrap();
        assert!(cached_uids(&pool, &secondary).await.is_empty());
        assert_eq!(
            booking_status_seq(&pool, &booking).await,
            ("confirmed".into(), 0)
        );
        assert_eq!(cached_uids(&pool, &write_calendar).await, ["owned-uid"]);

        sync_provider_calendars(&pool, &[0; 32], &snapshot_provider("write"), &source)
            .await
            .unwrap();
        assert!(cached_uids(&pool, &write_calendar).await.is_empty());
        assert_eq!(
            booking_status_seq(&pool, &booking).await,
            ("cancelled".into(), 1)
        );
    }

    #[tokio::test]
    async fn provider_orphan_uses_assigned_host_not_event_type_owner() {
        let pool = setup_test_db().await;
        let (source_a, et_a, _, cal_a) = provider_fixture(&pool, kinds::EWS).await;
        let (source_b, _, user_b, cal_b) = provider_fixture(&pool, kinds::EWS).await;
        let booking = provider_booking(&pool, &et_a, "assigned-uid").await;
        sqlx::query("UPDATE bookings SET assigned_user_id = ? WHERE id = ?")
            .bind(user_b)
            .bind(&booking)
            .execute(&pool)
            .await
            .unwrap();
        let start = Utc::now().format("%Y%m%dT%H%M%SZ").to_string();
        cached_provider_event(&pool, &cal_a, "assigned-uid", &start).await;
        cached_provider_event(&pool, &cal_b, "assigned-uid", &start).await;

        sync_provider_calendars(&pool, &[0; 32], &snapshot_provider("write"), &source_a)
            .await
            .unwrap();
        assert_eq!(
            booking_status_seq(&pool, &booking).await,
            ("confirmed".into(), 0)
        );
        sync_provider_calendars(&pool, &[0; 32], &snapshot_provider("write"), &source_b)
            .await
            .unwrap();
        assert_eq!(
            booking_status_seq(&pool, &booking).await,
            ("cancelled".into(), 1)
        );
    }

    #[tokio::test]
    async fn provider_empty_snapshot_removes_last_event_but_preserves_window_boundaries() {
        let pool = setup_test_db().await;
        let (source, _, _, calendar) = provider_fixture(&pool, kinds::EWS).await;
        let since = chrono::DateTime::parse_from_rfc3339("2030-06-15T10:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let until = since + chrono::Duration::days(730);
        for (uid, start) in [
            ("before", "20300615T095959Z".to_string()),
            ("first", "20300615T100000Z".to_string()),
            (
                "last",
                (until - chrono::Duration::seconds(1))
                    .format("%Y%m%dT%H%M%SZ")
                    .to_string(),
            ),
            ("after", until.format("%Y%m%dT%H%M%SZ").to_string()),
        ] {
            cached_provider_event(&pool, &calendar, uid, &start).await;
        }
        let removed = remove_orphaned_provider_events(
            &pool,
            &[0; 32],
            &snapshot_provider("write"),
            &source,
            &calendar,
            &[],
            (since, until),
        )
        .await
        .unwrap();
        assert_eq!(removed, 2);
        assert_eq!(cached_uids(&pool, &calendar).await, ["after", "before"]);
    }

    #[tokio::test]
    async fn provider_failed_and_unreadable_snapshots_preserve_cache_and_booking() {
        let pool = setup_test_db().await;
        let (source, et, _, calendar) = provider_fixture(&pool, kinds::EWS).await;
        let booking = provider_booking(&pool, &et, "cached").await;
        let start = Utc::now().format("%Y%m%dT%H%M%SZ").to_string();
        cached_provider_event(&pool, &calendar, "cached", &start).await;
        let mut provider = snapshot_provider("write");
        provider.events.push(ProviderRawEvent {
            remote_id: "first-page".into(),
            ical: format!("BEGIN:VEVENT\r\nUID:first-page\r\nDTSTART:{start}\r\nDTEND:{start}\r\nEND:VEVENT\r\n"),
        });
        provider.fetch_fails = true;
        assert!(sync_provider_calendars(&pool, &[0; 32], &provider, &source)
            .await
            .is_err());
        assert_eq!(cached_uids(&pool, &calendar).await, ["cached"]);
        assert_eq!(
            booking_status_seq(&pool, &booking).await,
            ("confirmed".into(), 0)
        );

        // One unreadable item: the readable ones still reach the cache, but
        // nothing is removed, since the unreadable one may be any cached event.
        // The sync reports it, so the owner learns that deletions on this
        // calendar are not reconciled.
        provider.fetch_fails = false;
        provider.events.push(ProviderRawEvent {
            remote_id: "invalid-page".into(),
            ical: format!("BEGIN:VEVENT\r\nUID:invalid\r\nDTSTART:{start}\r\nDTEND:{start}\r\n"),
        });
        assert!(sync_provider_source(&pool, &[0; 32], &provider, &source)
            .await
            .is_err());
        let error: Option<String> =
            sqlx::query_scalar("SELECT last_sync_error FROM caldav_sources WHERE id = ?")
                .bind(&source)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert!(error.unwrap().contains("1 event(s) calrs cannot read"));
        assert_eq!(
            cached_uids(&pool, &calendar).await,
            ["cached", "first-page"]
        );
        assert_eq!(
            booking_status_seq(&pool, &booking).await,
            ("confirmed".into(), 0)
        );
    }

    /// Absence from a Graph snapshot is not enough: items can move between
    /// pages while nextLink is followed, and a reschedule approved during a
    /// sync is newer than the snapshot. Graph confirms by UID first.
    #[tokio::test]
    async fn graph_sweep_cancels_only_when_graph_confirms_the_event_is_gone() {
        for (exists, status, cached) in [
            (Some(true), "confirmed", 0),
            (None, "confirmed", 1),
            (Some(false), "cancelled", 0),
        ] {
            let pool = setup_test_db().await;
            let (source, et, _, calendar) = provider_fixture(&pool, kinds::MICROSOFT).await;
            let booking = provider_booking(&pool, &et, "graph-uid").await;
            cached_provider_event(
                &pool,
                &calendar,
                "graph-uid",
                &Utc::now().format("%Y%m%dT%H%M%SZ").to_string(),
            )
            .await;
            let mut provider = snapshot_provider("write");
            provider.exists = exists;
            sync_provider_calendars(&pool, &[0; 32], &provider, &source)
                .await
                .unwrap();
            assert_eq!(booking_status(&pool, &booking).await, status, "{exists:?}");
            // An inconclusive lookup keeps the cached row for the next sync.
            assert_eq!(
                cached_uids(&pool, &calendar).await.len(),
                cached,
                "{exists:?}"
            );
        }
    }

    /// The OAuth callback lists calendars for the write-calendar choice and
    /// leaves the events to the background sync.
    #[tokio::test]
    async fn discovery_lists_calendars_without_fetching_events() {
        let pool = setup_test_db().await;
        let (source, _, _, _) = provider_fixture(&pool, kinds::MICROSOFT).await;
        let mut provider = snapshot_provider("write");
        provider.others.push(remote_calendar("second"));
        provider.fetch_fails = true;
        assert_eq!(
            discover_provider_calendars(&pool, &provider, &source)
                .await
                .unwrap(),
            2
        );
        let hrefs: Vec<String> =
            sqlx::query_scalar("SELECT href FROM calendars WHERE source_id = ? ORDER BY href")
                .bind(&source)
                .fetch_all(&pool)
                .await
                .unwrap();
        assert_eq!(hrefs, ["second", "write"]);
    }

    #[tokio::test]
    async fn a_calendar_shared_without_read_access_is_skipped_not_failed() {
        let pool = setup_test_db().await;
        let (source, _, _, write_calendar) = provider_fixture(&pool, kinds::MICROSOFT).await;
        let (shared, _, _) = upsert_calendar_provider(&pool, &source, &remote_calendar("shared"))
            .await
            .unwrap();
        let start = Utc::now().format("%Y%m%dT%H%M%SZ").to_string();
        cached_provider_event(&pool, &shared, "colleague-meeting", &start).await;

        let mut provider = snapshot_provider("write");
        provider.others.push(remote_calendar("shared"));
        provider.unreadable.push("shared".into());
        sync_provider_calendars(&pool, &[0; 32], &provider, &source)
            .await
            .unwrap();
        assert_eq!(cached_uids(&pool, &shared).await, ["colleague-meeting"]);
        assert!(cached_uids(&pool, &write_calendar).await.is_empty());

        // The write calendar is where bookings go: never skipped.
        provider.unreadable = vec!["write".into()];
        assert!(sync_provider_calendars(&pool, &[0; 32], &provider, &source)
            .await
            .is_err());
        // Nor is a source none of whose calendars can be read.
        provider.unreadable = vec!["write".into(), "shared".into()];
        sqlx::query("UPDATE caldav_sources SET write_calendar_href = NULL WHERE id = ?")
            .bind(&source)
            .execute(&pool)
            .await
            .unwrap();
        assert!(sync_provider_calendars(&pool, &[0; 32], &provider, &source)
            .await
            .is_err());
    }

    async fn calendar_flags(pool: &SqlitePool, source: &str) -> Vec<(String, bool, bool)> {
        sqlx::query_as(
            "SELECT href, is_busy, read_only FROM calendars WHERE source_id = ? ORDER BY href",
        )
        .bind(source)
        .fetch_all(pool)
        .await
        .unwrap()
    }

    /// A colleague's calendar accepted in Outlook is listed and readable like
    /// one of the user's own: it must not make the host busy by default, and
    /// cannot take bookings.
    #[tokio::test]
    async fn a_colleagues_shared_calendar_starts_not_busy_and_read_only() {
        let pool = setup_test_db().await;
        let (source, _, _, _) = provider_fixture(&pool, kinds::MICROSOFT).await;
        let mut provider = snapshot_provider("write");
        let mut holidays = remote_calendar("holidays");
        holidays.read_only = true;
        let mut colleague = remote_calendar("colleague");
        colleague.read_only = true;
        colleague.shared = true;
        provider.others = vec![holidays, colleague];
        sync_provider_calendars(&pool, &[0; 32], &provider, &source)
            .await
            .unwrap();
        assert_eq!(
            calendar_flags(&pool, &source).await,
            [
                ("colleague".into(), false, true),
                ("holidays".into(), true, true),
                ("write".into(), true, false),
            ]
        );

        // The user's choice stands: a later sync does not reset it.
        sqlx::query("UPDATE calendars SET is_busy = 1 WHERE href = 'colleague'")
            .execute(&pool)
            .await
            .unwrap();
        provider.others[1].read_only = false;
        sync_provider_calendars(&pool, &[0; 32], &provider, &source)
            .await
            .unwrap();
        assert_eq!(
            calendar_flags(&pool, &source).await[0],
            ("colleague".into(), true, false)
        );
    }

    /// A calendar the provider stops listing (deleted, or no longer shared)
    /// is dropped with its events, which would otherwise block forever.
    #[tokio::test]
    async fn a_calendar_no_longer_listed_is_removed_with_its_events() {
        let pool = setup_test_db().await;
        let (source, event_type, _, _) = provider_fixture(&pool, kinds::MICROSOFT).await;
        let (gone, _, _) = upsert_calendar_provider(&pool, &source, &remote_calendar("gone"))
            .await
            .unwrap();
        let start = Utc::now().format("%Y%m%dT%H%M%SZ").to_string();
        cached_provider_event(&pool, &gone, "colleague-meeting", &start).await;
        sqlx::query("INSERT INTO event_type_calendars (event_type_id, calendar_id) VALUES (?, ?)")
            .bind(&event_type)
            .bind(&gone)
            .execute(&pool)
            .await
            .unwrap();

        sync_provider_calendars(&pool, &[0; 32], &provider_without_calendars(), &source)
            .await
            .unwrap();
        assert_eq!(cached_uids(&pool, &gone).await, ["colleague-meeting"]);

        sync_provider_calendars(&pool, &[0; 32], &snapshot_provider("write"), &source)
            .await
            .unwrap();
        assert_eq!(calendar_flags(&pool, &source).await.len(), 1);
        let left: i64 = sqlx::query_scalar(
            "SELECT (SELECT COUNT(*) FROM events WHERE calendar_id = ?1)
                  + (SELECT COUNT(*) FROM event_type_calendars WHERE calendar_id = ?1)",
        )
        .bind(&gone)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(left, 0);
    }

    /// Lists no calendar at all, which a mailbox never really has.
    fn provider_without_calendars() -> impl crate::providers::CalendarProvider {
        struct Empty;
        #[async_trait::async_trait]
        impl crate::providers::CalendarProvider for Empty {
            async fn check_connection(&self) -> Result<bool> {
                Ok(true)
            }
            async fn list_calendars(&self) -> Result<Vec<crate::providers::RemoteCalendar>> {
                Ok(Vec::new())
            }
            async fn fetch_events(&self, _: &str) -> Result<Vec<ProviderRawEvent>> {
                unreachable!()
            }
            async fn fetch_events_since(&self, _: &str, _: &str) -> Result<Vec<ProviderRawEvent>> {
                unreachable!()
            }
            async fn sync_delta(
                &self,
                _: &str,
                _: Option<&str>,
            ) -> Result<crate::providers::DeltaResult> {
                unreachable!()
            }
            async fn put_event(&self, _: &str, _: &str, _: &str) -> Result<()> {
                unreachable!()
            }
            async fn delete_event(&self, _: &str, _: &str) -> Result<()> {
                unreachable!()
            }
            async fn event_exists(&self, _: &str, _: &str) -> Result<bool> {
                unreachable!()
            }
        }
        Empty
    }

    #[tokio::test]
    async fn provider_sync_respects_each_providers_forward_window() {
        let pool = setup_test_db().await;
        let (graph_source, _, _, graph_calendar) = provider_fixture(&pool, kinds::MICROSOFT).await;
        let (ews_source, _, _, ews_calendar) = provider_fixture(&pool, kinds::EWS).await;
        let now = Utc::now();
        for calendar in [&graph_calendar, &ews_calendar] {
            for (uid, days) in [
                ("past", -91),
                ("in-window", 1),
                ("ews-unfetched", 700),
                ("future", 731),
            ] {
                cached_provider_event(
                    &pool,
                    calendar,
                    uid,
                    &(now + chrono::Duration::days(days))
                        .format("%Y%m%dT%H%M%SZ")
                        .to_string(),
                )
                .await;
            }
        }
        sync_provider_calendars(&pool, &[0; 32], &snapshot_provider("write"), &graph_source)
            .await
            .unwrap();
        sync_provider_calendars(&pool, &[0; 32], &snapshot_provider("write"), &ews_source)
            .await
            .unwrap();
        assert_eq!(
            cached_uids(&pool, &graph_calendar).await,
            ["future", "past"]
        );
        assert_eq!(
            cached_uids(&pool, &ews_calendar).await,
            ["ews-unfetched", "future", "past"]
        );
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
        // confirmed push (fix A in web/booking_actions.rs clears this — but the sweep must also
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

    /// #269: the host's event is already gone when an orphan is cancelled,
    /// but a claimant's copy is not. Its cached row going away shows the
    /// claimant's calendar was cleaned up too. Called directly: the CalDAV
    /// sweep's global uid lookup would see that cached copy and not count
    /// the booking as an orphan in the first place, while the EWS sweep
    /// asks per event.
    #[tokio::test]
    async fn orphan_sweep_deletes_the_claimants_copy() {
        let pool = setup_test_db().await;
        let (source_id, et_id) = seed_fixtures(&pool).await;
        let key = [0u8; 32];

        let claimant = Uuid::new_v4().to_string();
        let claimant_account = Uuid::new_v4().to_string();
        let claimant_source = Uuid::new_v4().to_string();
        let claimant_cal = Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO users (id, email, name, role, auth_provider, username, enabled) VALUES (?, 'watcher@example.com', 'Watcher', 'user', 'local', 'watcher', 1)")
            .bind(&claimant).execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO accounts (id, name, email, timezone, user_id) VALUES (?, 'Watcher', 'watcher@example.com', 'UTC', ?)")
            .bind(&claimant_account).bind(&claimant).execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO caldav_sources (id, account_id, name, url, username, write_calendar_href) VALUES (?, ?, 'w', 'http://127.0.0.1:9/', 'w', '/w/')")
            .bind(&claimant_source).bind(&claimant_account).execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO calendars (id, source_id, href, display_name, is_busy) VALUES (?, ?, '/w/', 'W', 1)")
            .bind(&claimant_cal).bind(&claimant_source).execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO events (id, calendar_id, uid, summary, start_at, end_at) VALUES (?, ?, 'deleted-uid', 'Intro', '20300615T100000Z', '20300615T103000Z')")
            .bind(Uuid::new_v4().to_string()).bind(&claimant_cal).execute(&pool).await.unwrap();

        sqlx::query(
            "INSERT INTO bookings (id, event_type_id, uid, guest_name, guest_email, guest_timezone,
                start_at, end_at, status, cancel_token, reschedule_token, caldav_calendar_href,
                claimed_by_user_id)
             VALUES (?, ?, 'deleted-uid', 'Guest', 'guest@example.com', 'UTC',
                '2030-06-15T10:00:00', '2030-06-15T10:30:00', 'confirmed', 'ctok', 'rtok',
                '/calendars/host/default/', ?)",
        )
        .bind(Uuid::new_v4().to_string())
        .bind(&et_id)
        .bind(&claimant)
        .execute(&pool)
        .await
        .unwrap();

        let outcome = cancel_orphaned_booking(
            &pool,
            &key,
            OrphanCheck::Caldav(None),
            &source_id,
            "deleted-uid",
        )
        .await;
        assert!(matches!(outcome, OrphanOutcome::Cancelled));

        let copies: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM events WHERE uid = 'deleted-uid' AND calendar_id = ?",
        )
        .bind(&claimant_cal)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(copies, 0);
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
