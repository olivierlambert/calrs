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

/// SQL condition on a `caldav_sources cs` row: false for a Microsoft source
/// whose owner is suspended (`users.enabled = 0`). Such a source is never
/// synced, so its stored refresh token is not used while the owner cannot
/// sign in. Other kinds of source keep syncing as before.
const NOT_SUSPENDED_MICROSOFT: &str = "NOT (cs.provider_type = 'microsoft' AND EXISTS (
         SELECT 1 FROM accounts sa JOIN users su ON su.id = sa.user_id
         WHERE sa.id = cs.account_id AND su.enabled = 0))";

pub async fn run(pool: &SqlitePool, key: &[u8; 32], full: bool) -> Result<()> {
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
    )> = sqlx::query_as(&format!(
        "SELECT cs.id, cs.name, cs.url, cs.username, cs.password_enc, cs.auth_type, \
                    cs.access_token_enc, cs.token_expires_at, cs.provider_type \
             FROM caldav_sources cs WHERE cs.enabled = 1 AND {} AND {NOT_SUSPENDED_MICROSOFT}",
        crate::ews::global::ACTIVE_SOURCE_SQL
    ))
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
        let provider = crate::ews::global::provider_for_source(
            pool,
            key,
            source_id,
            url,
            username,
            password_enc,
        )
        .await;
        let provider = match provider {
            Ok(Some(p)) => p,
            Ok(None) => {
                // A managed source with nothing to do (global Exchange off
                // or incomplete, its user suspended or not trusted): nothing
                // to sync, and nothing to record or alert about.
                tracing::info!(source_id = %source_id, "managed Exchange source skipped");
                return Ok(());
            }
            Err(e) => {
                crate::commands::sync_health::record_sync_failure(pool, key, source_id, &e).await;
                return Err(e);
            }
        };
        return sync_ews_source(pool, key, provider.as_ref(), source_id).await;
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
        let (cal_id, stored_ctag, stored_sync_token, sync_enabled) =
            upsert_calendar(pool, source_id, cal_info).await?;
        let generation = calendar_generation(pool, &cal_id).await;

        let cal_label = cal_info.display_name.as_deref().unwrap_or(&cal_info.href);

        // #324: the owner chose not to sync this calendar. It stays listed
        // (so it can be turned back on), but nothing is fetched from it.
        if !sync_enabled {
            tracing::debug!(calendar = %cal_label, "calendar not synced, skipping");
            println!("  {} {}: not synced", "…".dimmed(), cal_label);
            forget_calendar_events(pool, &cal_id).await;
            continue;
        }

        // Google status events are only supported on the primary calendar and
        // are read through the Calendar API independently of CalDAV change
        // markers. Refresh even when the CalDAV ctag is unchanged.
        // A failure fails the source's sync, like a calendar that cannot
        // be fetched, so the host hears of absences that may be stale.
        if let Some((api_base, token)) = client.google_calendar_api() {
            if let Some(failure) = crate::google_calendar::refresh_out_of_office(
                pool,
                &cal_id,
                client.base_url(),
                &cal_info.href,
                api_base,
                token,
                crate::google_calendar::REFRESH_BUDGET,
            )
            .await
            {
                println!("  {} {}: {}", "✗".red(), cal_label, failure);
                calendar_failures.push(format!("{cal_label}: {failure}"));
            }
        }

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

                        // Store the new sync-token and ctag. A truncated
                        // answer (a 507, RFC 6578 section 3.6) keeps the
                        // old ctag, so the next sync does not skip the
                        // calendar as unchanged, and stores the token, which
                        // marks where the server stopped: the next delta
                        // continues from there instead of reading the same
                        // first page again.
                        let ctag = if result.truncated {
                            tracing::info!(
                                calendar = %cal_label,
                                "sync-collection truncated by the server, continuing next sync"
                            );
                            &stored_ctag
                        } else {
                            &cal_info.ctag
                        };
                        update_calendar_sync_state(
                            pool,
                            &cal_id,
                            generation,
                            ctag,
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
                    update_calendar_sync_state(
                        pool,
                        &cal_id,
                        generation,
                        &cal_info.ctag,
                        &new_token,
                    )
                    .await;

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
    // A write calendar the owner does not sync (#324) holds no cached
    // events, which must not read as "every booking was deleted": the sweep
    // skips it.
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

    let stale_sources: Vec<(String, String, String, Option<String>, String, Option<String>, Option<String>, String)> = sqlx::query_as(&format!(
        "SELECT cs.id, cs.url, cs.username, cs.password_enc, cs.auth_type, cs.access_token_enc, cs.token_expires_at, cs.provider_type
         FROM caldav_sources cs
         JOIN accounts a ON a.id = cs.account_id
         WHERE a.user_id = ?1 AND cs.enabled = 1 AND {}
           AND (cs.last_synced IS NULL OR cs.last_synced < ?2)
           AND (cs.last_sync_error_at IS NULL OR cs.last_sync_error_at < ?2)
           AND {NOT_SUSPENDED_MICROSOFT}",
        crate::ews::global::ACTIVE_SOURCE_SQL
    ))
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

/// The enabled source the background loop syncs next: the one whose last
/// attempt (success or failure) is oldest, provided that attempt is at least
/// `min_age_secs` old (`CALRS_SYNC_MIN_AGE`, #310). With 0 the stalest source
/// is always picked, as before the setting existed. `None` when every source
/// is fresher than that.
///
/// A healthy source due its daily full resync (`last_full_sync` unset or over
/// 24h old) is picked whatever its age: on-demand syncs from booking pages
/// keep `last_synced` fresh but only fetch the delta, and this loop is the
/// only place the full resync runs, so a busy host's source would otherwise
/// never get one. A failing source still waits the minimum age.
///
/// A managed Exchange source is left out while it has nothing to do (global
/// Exchange off or incomplete, its user suspended or not trusted, see
/// [`crate::ews::global::ACTIVE_SOURCE_SQL`]): its sync is skipped without
/// recording an attempt, so it would stay the stalest candidate and be picked
/// on every tick, starving the other sources.
pub async fn next_background_source(pool: &SqlitePool, min_age_secs: u64) -> Option<String> {
    sqlx::query_scalar(&format!(
        "SELECT id FROM (
             SELECT cs.id, cs.last_full_sync, cs.last_sync_error_at,
                    MAX(COALESCE(cs.last_synced, '2000-01-01'),
                        COALESCE(cs.last_sync_error_at, '2000-01-01')) AS last_attempt
             FROM caldav_sources cs
             WHERE cs.enabled = 1 AND {} AND {NOT_SUSPENDED_MICROSOFT}
         )
         WHERE ?1 = 0
            OR last_attempt <= datetime('now', ?2)
            OR ((last_full_sync IS NULL OR last_full_sync < datetime('now', '-24 hours'))
                AND (last_sync_error_at IS NULL OR last_sync_error_at <= datetime('now', ?2)))
         ORDER BY last_attempt ASC
         LIMIT 1",
        crate::ews::global::ACTIVE_SOURCE_SQL
    ))
    .bind(min_age_secs as i64)
    .bind(format!("-{min_age_secs} seconds"))
    .fetch_optional(pool)
    .await
    .unwrap_or(None)
}

/// Sync a single source by ID (for background sync loop).
/// Forces a full resync if last_full_sync is >24h ago (catches orphaned events).
pub async fn sync_source_by_id(pool: &SqlitePool, key: &[u8; 32], source_id: &str) {
    let source: Option<(String, String, Option<String>, Option<String>, String, Option<String>, Option<String>, String)> = sqlx::query_as(
        &format!("SELECT url, username, password_enc, last_full_sync, auth_type, access_token_enc, token_expires_at, provider_type FROM caldav_sources cs WHERE id = ? AND enabled = 1 AND {NOT_SUSPENDED_MICROSOFT}"),
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

/// EWS-specific sync path using the [`crate::providers::CalendarProvider`]
/// trait. CalDAV sources keep going through [`sync_source`], which retains
/// CalDAV-only optimisations (ctag, RFC 6578 sync-token, time-range queries,
/// hardened orphan reconciliation). The EWS path is intentionally simpler:
/// list folders, fetch each one in full, and reconcile by UID. Delta sync is a
/// known follow-up — see `EwsProvider::sync_delta`.
///
/// The outcome is recorded on the source, as for [`sync_source`].
pub async fn sync_ews_source(
    pool: &SqlitePool,
    key: &[u8; 32],
    provider: &dyn crate::providers::CalendarProvider,
    source_id: &str,
) -> Result<()> {
    let result = sync_ews_calendars(pool, key, provider, source_id).await;
    crate::commands::sync_health::record_sync_outcome(pool, key, source_id, &result).await;
    result
}

async fn sync_ews_calendars(
    pool: &SqlitePool,
    key: &[u8; 32],
    provider: &dyn crate::providers::CalendarProvider,
    source_id: &str,
) -> Result<()> {
    let mut calendar_failures: Vec<String> = Vec::new();
    // A managed source reads someone's mailbox through the service account:
    // it keeps busy times only, never what the meetings are about, nor the
    // names of their calendar folders.
    let busy_only = crate::ews::global::is_managed(pool, source_id).await;

    // Phase 1, network only: what the mailbox holds. Nothing is written yet,
    // since the source may be pointed at another mailbox while this runs.
    let mut calendars = provider.list_calendars().await?;
    if busy_only {
        for (n, cal) in calendars.iter_mut().enumerate() {
            cal.display_name = Some(crate::ews::global::neutral_calendar_name(n));
        }
    }
    let not_synced: Vec<String> =
        sqlx::query_scalar("SELECT href FROM calendars WHERE source_id = ? AND sync_enabled = 0")
            .bind(source_id)
            .fetch_all(pool)
            .await
            .unwrap_or_default();
    // A source that has no calendar yet is meeting its mailbox for the first
    // time (or again, after `forget_mailbox`): the moment to pick where
    // bookings go.
    let first_discovery: bool =
        sqlx::query_scalar("SELECT COUNT(*) = 0 FROM calendars WHERE source_id = ?")
            .bind(source_id)
            .fetch_one(pool)
            .await
            .unwrap_or(false);

    // Bounded fetch window. Matches the CalDAV path's FULL_FETCH_LOOKBACK_DAYS:
    // 90 days back is plenty for orphan reconciliation and keeps EWS response
    // sizes predictable. The provider's fetch_events_since uses CalendarView,
    // which expands recurrences server-side within the window.
    let since_dt = Utc::now() - chrono::Duration::days(FULL_FETCH_LOOKBACK_DAYS);
    let since_iso = since_dt.to_rfc3339();
    let since_prefix = since_dt.format("%Y%m%d").to_string();

    let mut fetched = Vec::with_capacity(calendars.len());
    for cal_info in &calendars {
        if not_synced.contains(&cal_info.id) {
            fetched.push(None);
            continue;
        }
        fetched.push(Some(
            provider.fetch_events_since(&cal_info.id, &since_iso).await,
        ));
    }
    let default_href = if first_discovery {
        default_write_calendar(pool, provider, source_id, &calendars).await
    } else {
        None
    };

    // Phase 2: keep it, under the source's mailbox lock, and only if the
    // source still reads the mailbox this client impersonated. Otherwise the
    // old mailbox's calendars would come back after `forget_mailbox`.
    let lock = crate::ews::global::mailbox_lock(source_id);
    let _guard = lock.lock().await;
    let row: Option<Option<String>> =
        sqlx::query_scalar("SELECT impersonate_email FROM caldav_sources WHERE id = ?")
            .bind(source_id)
            .fetch_optional(pool)
            .await?;
    let Some(target) = row else {
        tracing::info!(source_id = %source_id, "EWS sync discarded: the source was removed meanwhile");
        return Ok(());
    };
    if target.as_deref() != provider.impersonated_mailbox() {
        tracing::warn!(source_id = %source_id, "EWS sync discarded: the source now reads another mailbox");
        return Ok(());
    }

    let listed: Vec<&str> = calendars.iter().map(|c| c.id.as_str()).collect();
    for (cal_info, result) in calendars.iter().zip(fetched) {
        let (cal_id, _stored_change_marker, _stored_sync_state, sync_enabled) =
            upsert_calendar_provider(pool, source_id, cal_info).await?;
        let cal_label = cal_info.display_name.as_deref().unwrap_or(&cal_info.id);
        let result = match result {
            Some(r) if sync_enabled => r,
            _ => {
                tracing::debug!(calendar = %cal_label, "calendar not synced, skipping");
                println!("  {} {}: not synced", "…".dimmed(), cal_label);
                forget_calendar_events(pool, &cal_id).await;
                continue;
            }
        };

        match result {
            Ok(raw_events) => {
                let count =
                    upsert_provider_events(pool, &cal_id, &raw_events, busy_only, None).await;
                let deleted = remove_orphaned_ews_events(
                    pool,
                    key,
                    provider,
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
                        "removed stale EWS events from local cache"
                    );
                }
                println!(
                    "  {} {}: {} event(s) synced{}",
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
                println!("  {} {}: failed: {}", "✗".red(), cal_label, e);
                calendar_failures.push(format!("{cal_label}: {e:#}"));
            }
        }
    }

    // A folder the mailbox stops listing is dropped, with its cached events
    // and the event types' selections of it (they cascade), only once it has
    // been missing from two successful listings in a row; a write calendar
    // pointing at it is then cleared. An empty listing proves nothing (a
    // mailbox always has its calendar folder), so it never drops anything.
    if !listed.is_empty() {
        let stored: Vec<(String, String, i64)> =
            sqlx::query_as("SELECT id, href, ews_missing_count FROM calendars WHERE source_id = ?")
                .bind(source_id)
                .fetch_all(pool)
                .await?;
        for (id, href, misses) in stored {
            if listed.contains(&href.as_str()) {
                if misses > 0 {
                    sqlx::query("UPDATE calendars SET ews_missing_count = 0 WHERE id = ?")
                        .bind(&id)
                        .execute(pool)
                        .await?;
                }
            } else if misses >= 1 {
                sqlx::query("DELETE FROM calendars WHERE id = ?")
                    .bind(&id)
                    .execute(pool)
                    .await?;
                tracing::info!(source_id = %source_id, calendar_id = %id, "EWS calendar missing from two listings in a row: removed");
            } else {
                sqlx::query("UPDATE calendars SET ews_missing_count = 1 WHERE id = ?")
                    .bind(&id)
                    .execute(pool)
                    .await?;
                tracing::info!(source_id = %source_id, calendar_id = %id, "EWS calendar not listed by the server: kept until the next listing");
            }
        }
    }
    sqlx::query(
        "UPDATE caldav_sources SET write_calendar_href = NULL WHERE id = ? \
         AND write_calendar_href IS NOT NULL \
         AND NOT EXISTS (SELECT 1 FROM calendars c WHERE c.source_id = caldav_sources.id \
                         AND c.href = caldav_sources.write_calendar_href)",
    )
    .bind(source_id)
    .execute(pool)
    .await?;

    if let Some(href) = default_href {
        let _ = sqlx::query(
            "UPDATE caldav_sources SET write_calendar_href = ? \
             WHERE id = ? AND write_calendar_href IS NULL",
        )
        .bind(&href)
        .bind(source_id)
        .execute(pool)
        .await;
        tracing::info!(source_id = %source_id, "managed source writes bookings to the mailbox's default calendar");
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
    calendar_failures_result(&calendar_failures)
}

/// The calendar a managed source's bookings should go to: the mailbox's
/// default calendar, so a user who never opens the sources page still gets
/// their bookings written to their agenda. Only for a managed source that has
/// no write calendar yet, and only when its calendars were just discovered:
/// after that, "do not write" is a choice the user may have made and a later
/// sync must not undo. The caller applies it.
///
/// The default calendar is recognised by id, never by name (it is "Calendrier"
/// on a French mailbox). When the server cannot name it, a mailbox with a
/// single calendar is unambiguous; with several, nothing is guessed.
async fn default_write_calendar(
    pool: &SqlitePool,
    provider: &dyn crate::providers::CalendarProvider,
    source_id: &str,
    calendars: &[crate::providers::RemoteCalendar],
) -> Option<String> {
    let eligible: bool = sqlx::query_scalar(
        "SELECT managed = 1 AND write_calendar_href IS NULL FROM caldav_sources WHERE id = ?",
    )
    .bind(source_id)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten()
    .unwrap_or(false);
    if !eligible {
        return None;
    }
    let default = match provider.default_calendar().await {
        Ok(id) => id,
        Err(e) => {
            tracing::warn!(source_id = %source_id, error = %e, "could not ask the server for the default calendar");
            None
        }
    };
    let chosen = default
        .filter(|id| calendars.iter().any(|c| &c.id == id))
        .or_else(|| match calendars {
            [only] => Some(only.id.clone()),
            _ => None,
        });
    if chosen.is_none() {
        tracing::info!(source_id = %source_id, calendars = calendars.len(), "no default calendar to write bookings to; the user picks one");
    }
    chosen
}

/// What a busy-only event keeps of the iCal text of its resource: nothing,
/// unless it recurs (an RRULE or an RDATE). Then one bare VEVENT per VEVENT
/// of the resource, in order, carrying only what
/// [`crate::rrule::extract_exdates`] and [`crate::rrule::extract_rdates`]
/// read: the first one's start, `EXDATE` and `RDATE` lines (the series'
/// cancelled occurrences and extra dates),
/// and each other one's `RECURRENCE-ID` (its moved occurrences, which the
/// series must not block twice). No title, description, location, attendees
/// or notes.
fn busy_only_ical(resource: &str, recurs: bool) -> Option<String> {
    if !recurs {
        return None;
    }
    let unfolded = crate::utils::unfold_ical(resource);
    let mut out = String::new();
    let mut index = 0usize;
    let mut inside = false;
    for line in unfolded.lines() {
        let line = line.trim_end_matches('\r');
        let name = line
            .split([':', ';'])
            .next()
            .unwrap_or("")
            .to_ascii_uppercase();
        if line.eq_ignore_ascii_case("BEGIN:VEVENT") {
            inside = true;
            out.push_str("BEGIN:VEVENT\r\n");
            continue;
        }
        if line.eq_ignore_ascii_case("END:VEVENT") {
            inside = false;
            index += 1;
            out.push_str("END:VEVENT\r\n");
            continue;
        }
        let keep = inside
            && match index {
                0 => name == "DTSTART" || name == "EXDATE" || name == "RDATE",
                _ => name == "RECURRENCE-ID",
            };
        if keep {
            out.push_str(line);
            out.push_str("\r\n");
        }
    }
    Some(out)
}

/// EWS variant of orphan reconciliation, scoped to the fetched window.
/// `since_prefix` is a `YYYYMMDD` lower bound matching the
/// `fetch_events_since` call: events with `start_at` before it weren't in
/// the response and must not be flagged as orphans. Pass an empty string to
/// reconcile against every local event.
///
/// A vanished event's booking goes through [`cancel_orphaned_booking`] like
/// on CalDAV: scoped to this source and host, confirmed gone with `provider`
/// first, and both sides emailed.
#[allow(clippy::too_many_arguments)]
async fn remove_orphaned_ews_events(
    pool: &SqlitePool,
    key: &[u8; 32],
    provider: &dyn crate::providers::CalendarProvider,
    source_id: &str,
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
            // Decide before dropping the cached event: EWS has no
            // booking-level sweep, so when Exchange could not say whether the
            // booking's event is gone, the cached row is what brings it back
            // for another check on the next sync.
            let outcome =
                cancel_orphaned_booking(pool, key, OrphanCheck::Provider(provider), source_id, uid)
                    .await;
            if outcome == OrphanOutcome::Inconclusive {
                continue;
            }
            let _ = sqlx::query("DELETE FROM events WHERE id = ?")
                .bind(event_id)
                .execute(pool)
                .await;
            deleted += 1;
        }
    }
    deleted
}

/// Full-snapshot sync for Microsoft Graph through [`crate::providers::CalendarProvider`].
/// EWS has its own, [`sync_ews_source`].
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
    let strict = strict_snapshots(&provider_type);
    // Every listed calendar gets its row first, so the generations read
    // next cover the whole listing.
    let mut listed = Vec::with_capacity(calendars.len());
    for cal_info in &calendars {
        let (cal_id, _stored_change_marker, _stored_sync_state, sync_enabled) =
            upsert_calendar_provider(pool, source_id, cal_info).await?;
        listed.push((cal_info, cal_id, sync_enabled));
    }
    // The calendars' sync generations and count once the listing is
    // applied: a sync that ends with others saw a calendar's setting change
    // (#324), or a calendar appear, and must not mark the source fresh.
    let generations = source_generations(pool, source_id).await;
    // On Graph: whether every calendar that counts for availability, and the
    // write calendar, returned a snapshot. Only then has the source been
    // read (see [`microsoft_coverage_gap`]).
    let mut complete = true;
    // A write calendar Graph no longer lists is kept (its bookings are
    // tracked by it), but nothing reads it any more and every write to it
    // fails: the owner has to pick another one.
    if strict && !calendars.is_empty() {
        if let Some(write) = write_calendar.as_deref() {
            if !calendars.iter().any(|c| c.id == write) {
                complete = false;
                calendar_failures.push(
                    "the calendar bookings are written to is no longer listed by Microsoft; \
                     choose another one under Write bookings to"
                        .to_string(),
                );
            }
        }
    }
    let mut unreadable_calendars = 0usize;
    // Whether Graph could not say, for some vanished booking event, whether
    // it is gone (see `orphan_check_inconclusive_since`).
    let mut inconclusive = false;
    // Calendars this sync tried to fetch: the ones not turned off (#324).
    let mut fetched_calendars = 0usize;
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

    for (cal_info, cal_id, sync_enabled) in listed {
        let generation = calendar_generation(pool, &cal_id).await;
        let cal_label = cal_info.display_name.as_deref().unwrap_or(&cal_info.id);
        if !sync_enabled {
            tracing::debug!(calendar = %cal_label, "calendar not synced, skipping");
            println!("  {} {}: not synced", "…".dimmed(), cal_label);
            forget_calendar_events(pool, &cal_id).await;
            continue;
        }
        fetched_calendars += 1;
        let needed = write_calendar.as_deref() == Some(cal_info.id.as_str())
            || calendar_counts_for_availability(pool, &cal_id).await;

        let snapshot = async {
            let raw_events = provider
                .fetch_events_since(&cal_info.id, &since_iso)
                .await?;
            // The owner turned the calendar off (or off and on again) while
            // it was being fetched (#324): its events may have been dropped
            // meanwhile, so this snapshot neither fills the cache nor proves
            // anything gone. The next sync fetches it again.
            if calendar_generation(pool, &cal_id).await != generation {
                return Ok::<_, anyhow::Error>(None);
            }
            // EWS keeps the rules it was field-tested with: every item is
            // cached as it comes, none is held against the calendar.
            let (raw_events, unreadable) = if strict {
                partition_provider_snapshot(raw_events)
            } else {
                (raw_events, 0)
            };
            let count = upsert_provider_events(pool, &cal_id, &raw_events, false, Some(generation)).await;
            // Written only while the generation held (see
            // upsert_provider_events); a change since then means this
            // snapshot proves nothing gone either.
            if calendar_generation(pool, &cal_id).await != generation {
                return Ok(None);
            }
            // An item calrs could not read may be any cached event, so a
            // snapshot with one cannot prove that a cached event is gone.
            let deleted = if unreadable == 0 {
                remove_orphaned_provider_events_noting(
                    pool,
                    key,
                    provider,
                    source_id,
                    &cal_id,
                    &raw_events,
                    (since_dt, until_dt),
                    &mut inconclusive,
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
            Ok(Some((count, deleted, unreadable)))
        }
        .await;
        // A snapshot with unreadable items still came back, as on later
        // syncs; one discarded or refused did not.
        if needed && !matches!(snapshot, Ok(Some(_))) {
            complete = false;
        }
        match snapshot {
            Ok(None) => {
                tracing::info!(
                    calendar_name = cal_label,
                    "calendar sync setting changed during the sync, snapshot discarded"
                );
                println!(
                    "  {} {}: changed during sync, skipped",
                    "…".dimmed(),
                    cal_label
                );
            }
            Ok(Some((count, deleted, unreadable))) => {
                if deleted > 0 {
                    tracing::info!(
                        calendar_name = cal_label,
                        stale_events_removed = deleted,
                        "removed stale provider events from local cache"
                    );
                }
                println!(
                    "  {} {}: {} event(s) synced{}{}",
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
            // source for good. The write calendar is never skipped, and one
            // that counts for availability (busy, or selected by an event
            // type) is reported: new events on it are missed meanwhile.
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
                println!("  {} {}: skipped, {}", "…".dimmed(), cal_label, e);
                if calendar_counts_for_availability(pool, &cal_id).await {
                    calendar_failures.push(format!(
                        "{cal_label}: {e}; it counts for availability, so its new events are missed \
                         until it can be read or is no longer checked"
                    ));
                }
            }
            Err(e) => {
                println!("  {} {}: failed, {}", "✗".red(), cal_label, e);
                calendar_failures.push(format!("{cal_label}: {e:#}"));
            }
        }
    }
    if unreadable_calendars > 0 && unreadable_calendars == fetched_calendars {
        calendar_failures.push(crate::providers::CalendarUnreadable.to_string());
    }

    // A booking whose event vanished stays confirmed while Graph cannot
    // confirm the event is gone. That is right for a moment, but if it
    // lasts, deletions on this source no longer cancel anything: past a day,
    // the sync reports it (and sync health alerts the owner once).
    if strict {
        if let Some(message) = note_orphan_checks(pool, source_id, inconclusive).await {
            calendar_failures.push(message);
        }
    }
    // On Graph, `last_full_sync` also says the source has delivered a
    // complete snapshot at least once: until then its calendars cannot
    // vouch for any time (see [`microsoft_coverage_gap`]).
    if !strict || complete {
        let _ = sqlx::query(
            "UPDATE caldav_sources SET last_full_sync = datetime('now') WHERE id = ?1
               AND (SELECT COALESCE(SUM(sync_generation), 0) || ':' || COUNT(*) FROM calendars WHERE source_id = ?1) = ?2",
        )
        .bind(source_id)
        .bind(&generations)
        .execute(pool)
        .await;
    }
    // In one statement with the generation check: a calendar turned on
    // during this sync left the source stale, and it stays so.
    let fresh = sqlx::query(
        "UPDATE caldav_sources SET last_synced = datetime('now') WHERE id = ?1
           AND (SELECT COALESCE(SUM(sync_generation), 0) || ':' || COUNT(*) FROM calendars WHERE source_id = ?1) = ?2",
    )
    .bind(source_id)
    .bind(&generations)
    .execute(pool)
    .await?;
    if fresh.rows_affected() == 0 {
        tracing::info!(source_id = %source_id, "a calendar sync setting changed during the sync; the source stays stale");
    }
    tracing::info!(source_id = %source_id, "calendar provider sync completed");
    calendar_failures_result(&calendar_failures)
}

/// Records whether this sync's orphan checks were inconclusive, and returns
/// the failure to report when they have been for over a day.
async fn note_orphan_checks(
    pool: &SqlitePool,
    source_id: &str,
    inconclusive: bool,
) -> Option<String> {
    if !inconclusive {
        let _ = sqlx::query(
            "UPDATE caldav_sources SET orphan_check_inconclusive_since = NULL WHERE id = ?",
        )
        .bind(source_id)
        .execute(pool)
        .await;
        return None;
    }
    let lasting: bool = sqlx::query_scalar(
        "UPDATE caldav_sources
         SET orphan_check_inconclusive_since = COALESCE(orphan_check_inconclusive_since, datetime('now'))
         WHERE id = ?
         RETURNING orphan_check_inconclusive_since <= datetime('now', '-1 day')",
    )
    .bind(source_id)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten()
    .unwrap_or(false);
    lasting.then(|| {
        "for over a day, Microsoft could not confirm whether events deleted from this \
         calendar are gone, so their bookings were not cancelled: check that calrs can \
         read every calendar of this account"
            .to_string()
    })
}

/// The sum of the sync generations of a source's calendars and their
/// count, as `sum:count`. Generations only grow, so the sum changes whenever
/// a calendar's sync setting does; the count catches a calendar added (at
/// generation 0) or removed meanwhile.
async fn source_generations(pool: &SqlitePool, source_id: &str) -> String {
    sqlx::query_scalar(
        "SELECT COALESCE(SUM(sync_generation), 0) || ':' || COUNT(*) FROM calendars WHERE source_id = ?",
    )
    .bind(source_id)
    .fetch_one(pool)
    .await
    .unwrap_or_default()
}

/// Whether a calendar's events can block a slot: it is busy, or an event
/// type selects it.
async fn calendar_counts_for_availability(pool: &SqlitePool, cal_id: &str) -> bool {
    sqlx::query_scalar(
        "SELECT c.is_busy = 1 OR EXISTS (SELECT 1 FROM event_type_calendars WHERE calendar_id = c.id)
         FROM calendars c WHERE c.id = ?",
    )
    .bind(cal_id)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten()
    .unwrap_or(true)
}

/// Why a host's Microsoft calendars cannot vouch for a time (see
/// [`microsoft_coverage_gap`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CoverageGap {
    /// A Microsoft source has not delivered its first complete snapshot.
    NotYetSynced,
    /// The time lies past the end of what the last complete snapshot
    /// covered: nothing read it.
    BeyondWindow(chrono::DateTime<Utc>),
}

/// The end of the time a Microsoft source's last complete snapshot covered:
/// its `last_full_sync` plus the Graph sync window, less a day for the time
/// the sync itself took (the window is counted from each fetch, which comes
/// before the stamp).
fn microsoft_horizon(last_full_sync: &str) -> Option<chrono::DateTime<Utc>> {
    let stamped =
        chrono::NaiveDateTime::parse_from_str(last_full_sync, "%Y-%m-%d %H:%M:%S").ok()?;
    Some(stamped.and_utc() + chrono::Duration::days(crate::providers::graph::SYNC_WINDOW_DAYS - 1))
}

/// Whether the host's enabled Microsoft sources can vouch for a time ending
/// at `end`. A host with no Microsoft source has no gap. Busy computations
/// treat a gap as busy, so a guest is never offered a time the host's
/// Outlook calendar was never read for.
pub(crate) async fn microsoft_coverage_gap(
    pool: &SqlitePool,
    user_id: &str,
    end: chrono::DateTime<Utc>,
) -> Option<CoverageGap> {
    try_microsoft_coverage_gap(pool, user_id, end)
        .await
        .unwrap_or_else(|error| {
            tracing::error!(%error, %user_id, "cannot read Microsoft sources; assuming no gap");
            None
        })
}

/// [`microsoft_coverage_gap`], with an unreadable source list as an error.
pub(crate) async fn try_microsoft_coverage_gap(
    pool: &SqlitePool,
    user_id: &str,
    end: chrono::DateTime<Utc>,
) -> Result<Option<CoverageGap>, sqlx::Error> {
    let sources: Vec<Option<String>> = sqlx::query_scalar(
        "SELECT cs.last_full_sync FROM caldav_sources cs JOIN accounts a ON a.id = cs.account_id
         WHERE a.user_id = ? AND cs.enabled = 1 AND cs.provider_type = 'microsoft'",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;
    if sources.is_empty() {
        return Ok(None);
    }
    let mut horizon: Option<chrono::DateTime<Utc>> = None;
    for stamp in &sources {
        let Some(end_of_source) = stamp.as_deref().and_then(microsoft_horizon) else {
            return Ok(Some(CoverageGap::NotYetSynced));
        };
        horizon = Some(horizon.map_or(end_of_source, |h| h.min(end_of_source)));
    }
    let Some(horizon) = horizon else {
        return Ok(None);
    };
    Ok((end > horizon).then_some(CoverageGap::BeyondWindow(horizon)))
}

/// The parts of `[window_start, window_end)` (local times in `host_tz`) the
/// user's Microsoft calendars cannot vouch for, as busy intervals with the
/// reason: the whole window before the first complete snapshot, the part
/// past its horizon after. Shared by the web busy times and the CLI, so the
/// two never disagree.
pub(crate) async fn microsoft_gap_intervals(
    pool: &SqlitePool,
    user_id: &str,
    window_start: chrono::NaiveDateTime,
    window_end: chrono::NaiveDateTime,
    host_tz: chrono_tz::Tz,
) -> Vec<(chrono::NaiveDateTime, chrono::NaiveDateTime, CoverageGap)> {
    try_microsoft_gap_intervals(pool, user_id, window_start, window_end, host_tz)
        .await
        .unwrap_or_else(|error| {
            tracing::error!(%error, %user_id, "cannot read Microsoft sources; assuming no gap");
            Vec::new()
        })
}

/// [`microsoft_gap_intervals`], with an unreadable source list as an error:
/// the submit-time booking checks refuse rather than assume no gap.
pub(crate) async fn try_microsoft_gap_intervals(
    pool: &SqlitePool,
    user_id: &str,
    window_start: chrono::NaiveDateTime,
    window_end: chrono::NaiveDateTime,
    host_tz: chrono_tz::Tz,
) -> Result<Vec<(chrono::NaiveDateTime, chrono::NaiveDateTime, CoverageGap)>, sqlx::Error> {
    use chrono::TimeZone;
    let window_end_utc = host_tz
        .from_local_datetime(&window_end)
        .latest()
        .map(|end| end.with_timezone(&Utc))
        .unwrap_or_else(|| window_end.and_utc());
    Ok(
        match try_microsoft_coverage_gap(pool, user_id, window_end_utc).await? {
            Some(gap @ CoverageGap::NotYetSynced) => vec![(window_start, window_end, gap)],
            Some(gap @ CoverageGap::BeyondWindow(horizon)) => {
                let horizon = horizon.with_timezone(&host_tz).naive_local();
                vec![(horizon.max(window_start), window_end, gap)]
            }
            None => Vec::new(),
        },
    )
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

/// Whether a provider's snapshots are held to the stricter rules: an item
/// calrs cannot read fails its calendar and stops orphan removal, an empty
/// snapshot proves the calendar empty, and a calendar no longer listed is
/// dropped. Microsoft Graph only. EWS keeps the rules it was field-tested
/// with: every item is cached as it comes, an empty snapshot removes
/// nothing, and calendars are never deleted, since one FindFolder listing
/// that misses a folder (new folder ids after a mailbox move, no paging)
/// would cascade away its events and event-type selections and bring it back
/// as a new calendar, synced again even if the owner had turned it off.
fn strict_snapshots(provider_type: &str) -> bool {
    provider_type == kinds::MICROSOFT
}

/// Drops the cached calendars a Microsoft source no longer lists: deleted,
/// or shared by someone who stopped sharing it. Their events would otherwise
/// keep blocking slots, since nothing else reads that calendar again. Its
/// events and event-type selections go with it (ON DELETE CASCADE); an event
/// type left with no selected calendar checks every busy calendar again.
/// The source's write calendar is never dropped, and EWS sources are left
/// alone (see [`strict_snapshots`]).
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
    let source: Option<(String, Option<String>)> = sqlx::query_as(
        "SELECT provider_type, write_calendar_href FROM caldav_sources WHERE id = ?",
    )
    .bind(source_id)
    .fetch_optional(pool)
    .await?;
    let Some((provider_type, write_calendar)) = source else {
        return Ok(());
    };
    if !strict_snapshots(&provider_type) {
        return Ok(());
    }
    let listed: std::collections::HashSet<&str> = calendars.iter().map(|c| c.id.as_str()).collect();
    let cached: Vec<(String, String, Option<String>)> =
        sqlx::query_as("SELECT id, href, display_name FROM calendars WHERE source_id = ?")
            .bind(source_id)
            .fetch_all(pool)
            .await?;
    for (id, href, name) in cached {
        if listed.contains(href.as_str()) || write_calendar.as_deref() == Some(href.as_str()) {
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

/// Orphan reconciliation for snapshot providers. On Microsoft Graph only a
/// readable snapshot reaches this point, so an empty one (the last event was
/// deleted) removes cached events too. On EWS an empty snapshot removes
/// nothing, as before Graph support (see [`strict_snapshots`]), and an item
/// without a UID matches no cached event. Removal is restricted to starts inside
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
    window: (chrono::DateTime<Utc>, chrono::DateTime<Utc>),
) -> Result<u32> {
    let mut inconclusive = false;
    remove_orphaned_provider_events_noting(
        pool,
        key,
        provider,
        source_id,
        cal_id,
        raw_events,
        window,
        &mut inconclusive,
    )
    .await
}

/// [`remove_orphaned_provider_events`], setting `inconclusive` when the
/// provider could not say whether a vanished booking's event is gone.
#[allow(clippy::too_many_arguments)]
async fn remove_orphaned_provider_events_noting(
    pool: &SqlitePool,
    key: &[u8; 32],
    provider: &dyn crate::providers::CalendarProvider,
    source_id: &str,
    cal_id: &str,
    raw_events: &[ProviderRawEvent],
    (since, until): (chrono::DateTime<Utc>, chrono::DateTime<Utc>),
    inconclusive: &mut bool,
) -> Result<u32> {
    // A calendar bookings go to: the source's write calendar, or one a
    // copy was recorded on as an event type's destination (#47).
    let source: Option<(bool, String, String)> = sqlx::query_as(
        "SELECT COALESCE(c.href = cs.write_calendar_href, 0)
                OR EXISTS (SELECT 1 FROM booking_write_copies bc
                           WHERE bc.source_id = cs.id AND bc.calendar_href = c.href),
                cs.provider_type, c.href
         FROM calendars c JOIN caldav_sources cs ON cs.id = c.source_id
         WHERE c.id = ? AND cs.id = ?",
    )
    .bind(cal_id)
    .bind(source_id)
    .fetch_optional(pool)
    .await?;
    let Some((is_write_calendar, provider_type, cal_href)) = source else {
        return Ok(0);
    };
    let strict = strict_snapshots(&provider_type);
    let mut seen_uids = std::collections::HashSet::new();
    let mut seen_events = 0usize;
    for raw in raw_events {
        for vevent in split_vevents(&raw.ical) {
            seen_events += 1;
            let fields = crate::utils::unfold_ical(&vevent);
            let Some(uid) = extract_vevent_field(&fields, "UID") else {
                if strict {
                    anyhow::bail!("provider event has no UID");
                }
                continue;
            };
            let recurrence_id = extract_vevent_field(&fields, "RECURRENCE-ID");
            seen_uids.insert((uid, recurrence_id.unwrap_or_default()));
        }
    }
    if !strict && seen_events == 0 {
        return Ok(0);
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
        // Only a booking written to this very calendar of this source (its
        // stored copy, or one recorded at a destination, #47) can be
        // orphaned by the uid vanishing here: a copy of the same uid on
        // another calendar proves nothing about the booking's own.
        let mut cancelled = false;
        if is_write_calendar && booking_written_here(pool, &uid, source_id, &cal_href).await {
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
                *inconclusive = true;
                continue;
            }
            cancelled = outcome == OrphanOutcome::Cancelled;
        }
        // A cancellation already dropped the host's cached copies with the
        // booking's other copies; the row counts as removed either way.
        let result = sqlx::query("DELETE FROM events WHERE id = ?")
            .bind(&event_id)
            .execute(pool)
            .await?;
        deleted += u32::from(cancelled || result.rows_affected() > 0);
    }
    Ok(deleted)
}

/// Whether the booking with `uid` was written to calendar `cal_href` of
/// source `source_id`: its stored copy (`caldav_source_id`, or none recorded
/// for bookings written before #47, and `caldav_calendar_href`), or a copy
/// recorded in `booking_write_copies`.
async fn booking_written_here(
    pool: &SqlitePool,
    uid: &str,
    source_id: &str,
    cal_href: &str,
) -> bool {
    sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM bookings b
                        WHERE b.uid = ?1 AND b.caldav_calendar_href = ?3
                          AND (b.caldav_source_id = ?2 OR b.caldav_source_id IS NULL))
             OR EXISTS (SELECT 1 FROM booking_write_copies bc
                        WHERE bc.booking_uid = ?1 AND bc.source_id = ?2 AND bc.calendar_href = ?3)",
    )
    .bind(uid)
    .bind(source_id)
    .bind(cal_href)
    .fetch_one(pool)
    .await
    .unwrap_or(false)
}

/// [`cancel_orphaned_booking`] without a server check, for tests of what a
/// cancellation of an orphaned booking does next.
#[cfg(test)]
pub(crate) async fn cancel_orphan_unverified(
    pool: &SqlitePool,
    key: &[u8; 32],
    source_id: &str,
    uid: &str,
) -> bool {
    cancel_orphaned_booking(pool, key, OrphanCheck::Caldav(None), source_id, uid).await
        == OrphanOutcome::Cancelled
}

/// Provider-trait equivalent of [`upsert_calendar`]. EWS uses opaque folder
/// IDs in the `href` column; the `id` field on `RemoteCalendar` is reused.
async fn upsert_calendar_provider(
    pool: &SqlitePool,
    source_id: &str,
    cal_info: &crate::providers::RemoteCalendar,
) -> Result<(String, Option<String>, Option<String>, bool)> {
    let existing: Option<(String, Option<String>, Option<String>, bool)> = sqlx::query_as(
        "SELECT id, ctag, sync_token, sync_enabled FROM calendars WHERE source_id = ? AND href = ?",
    )
    .bind(source_id)
    .bind(&cal_info.id)
    .fetch_optional(pool)
    .await?;

    match existing {
        Some((id, ctag, sync_token, sync_enabled)) => {
            sqlx::query(
                "UPDATE calendars SET display_name = ?, color = ?, read_only = ? WHERE id = ?",
            )
            .bind(&cal_info.display_name)
            .bind(&cal_info.color)
            .bind(cal_info.read_only)
            .bind(&id)
            .execute(pool)
            .await?;
            Ok((id, ctag, sync_token, sync_enabled))
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
            Ok((id, None, None, true))
        }
    }
}

/// Provider-trait equivalent of [`upsert_raw_events`]. Splits the iCal blob
/// into VEVENTs and upserts into the `events` table (same composite key:
/// calendar_id + uid + recurrence_id).
/// With `busy_only` (a managed Exchange source), the title, description,
/// location and iCal text are not stored: only what availability needs (UID,
/// times, timezone, recurrence, status, transparency), plus, for a recurring
/// event, its exception dates in [`busy_only_ical`].
///
/// With a `generation` (Graph), each row is written only while the
/// calendar's sync generation is still that, in the same statement: a
/// calendar turned off (or off and on) mid-way gets no part of a snapshot
/// fetched before the change (#324). EWS passes `None`: its sync keeps the
/// rules it was field-tested with.
async fn upsert_provider_events(
    pool: &SqlitePool,
    cal_id: &str,
    raw_events: &[ProviderRawEvent],
    busy_only: bool,
    generation: Option<i64>,
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
            if let Some(rule) = rrule.as_deref() {
                crate::rrule::warn_if_unexpandable(rule, &start_at, &uid, cal_id);
            }
            let recurrence_id = extract_vevent_field(&fields, "RECURRENCE-ID");
            // Extra dates of the series (#376), never of a moved occurrence.
            let rdate = recurrence_id.is_none() && crate::rrule::has_rdate(&fields);
            let transp = extract_vevent_field(&fields, "TRANSP");
            let timezone = zones.event_timezone(&fields);
            let end_at = crate::utils::extract_vevent_end(
                &fields,
                timezone.as_deref().and_then(crate::tzid::resolve_or_offset),
            )
            .unwrap_or_default();

            let (summary, location, description, raw_ical) = if busy_only {
                (
                    None,
                    None,
                    None,
                    busy_only_ical(&raw.ical, rrule.is_some() || rdate),
                )
            } else {
                (summary, location, description, Some(raw.ical.clone()))
            };

            let event_id = Uuid::new_v4().to_string();
            let written = sqlx::query(
                "INSERT INTO events (id, calendar_id, uid, summary, start_at, end_at, location, description, status, rrule, raw_ical, recurrence_id, timezone, transp, rdate)
                 SELECT ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?
                 WHERE ? IS NULL OR (SELECT sync_generation FROM calendars WHERE id = ?) = ?
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
                   rdate = excluded.rdate,
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
            .bind(&raw_ical)
            .bind(&recurrence_id)
            .bind(&timezone)
            .bind(&transp)
            .bind(rdate)
            .bind(generation)
            .bind(cal_id)
            .bind(generation)
            .execute(pool)
            .await;

            if let Err(error) = &written {
                tracing::warn!(%error, "could not cache a provider event");
            }
            if matches!(written, Ok(ref r) if r.rows_affected() > 0) {
                count += 1;
            }
        }
    }
    count
}

// --- Helper functions ---

/// Upsert a calendar record and return (cal_id, stored_ctag,
/// stored_sync_token, sync_enabled)
async fn upsert_calendar(
    pool: &SqlitePool,
    source_id: &str,
    cal_info: &crate::caldav::CalendarInfo,
) -> Result<(String, Option<String>, Option<String>, bool)> {
    let existing: Option<(String, Option<String>, Option<String>, bool)> = sqlx::query_as(
        "SELECT id, ctag, sync_token, sync_enabled FROM calendars WHERE source_id = ? AND href = ?",
    )
    .bind(source_id)
    .bind(&cal_info.href)
    .fetch_optional(pool)
    .await?;

    match existing {
        Some((id, ctag, sync_token, sync_enabled)) => {
            // Update display_name and color (may have changed on server)
            sqlx::query("UPDATE calendars SET display_name = ?, color = ? WHERE id = ?")
                .bind(&cal_info.display_name)
                .bind(&cal_info.color)
                .bind(&id)
                .execute(pool)
                .await?;
            Ok((id, ctag, sync_token, sync_enabled))
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
            Ok((id, None, None, true))
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
            if let Some(rule) = rrule.as_deref() {
                crate::rrule::warn_if_unexpandable(rule, &start_at, &uid, cal_id);
            }
            let recurrence_id = extract_vevent_field(&fields, "RECURRENCE-ID");
            // Extra dates of the series (#376), never of a moved occurrence.
            let rdate = recurrence_id.is_none() && crate::rrule::has_rdate(&fields);
            let transp = extract_vevent_field(&fields, "TRANSP");
            let timezone = zones.event_timezone(&fields);
            let end_at = crate::utils::extract_vevent_end(
                &fields,
                timezone.as_deref().and_then(crate::tzid::resolve_or_offset),
            )
            .unwrap_or_default();

            let event_id = Uuid::new_v4().to_string();
            let _ = sqlx::query(
                "INSERT INTO events (id, calendar_id, uid, summary, start_at, end_at, location, description, status, rrule, raw_ical, recurrence_id, timezone, transp, rdate)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
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
                   rdate = excluded.rdate,
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
            .bind(rdate)
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
        // Calendar API snapshots have their own deletion lifecycle, even when
        // a CalDAV href happens to carry the same UID.
        let rows = sqlx::query(
            "DELETE FROM events WHERE calendar_id = ? AND uid = ? AND google_out_of_office = 0",
        )
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
           AND google_out_of_office = 0
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
pub(crate) enum OrphanCheck<'a> {
    /// CalDAV: a HEAD/PROPFIND on the event. `None` skips the check (paths
    /// with no client at hand).
    Caldav(Option<&'a CaldavClient>),
    /// Any other provider (EWS): a lookup by UID in the write calendar.
    Provider(&'a dyn crate::providers::CalendarProvider),
}

/// What [`cancel_orphaned_booking`] did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OrphanOutcome {
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

pub(crate) async fn cancel_orphaned_booking(
    pool: &SqlitePool,
    key: &[u8; 32],
    check: OrphanCheck<'_>,
    source_id: &str,
    uid: &str,
) -> OrphanOutcome {
    // Fetch booking details before cancelling. Scoped by source_id via the
    // caldav_sources join: the booking must have been pushed to this
    // source's write calendar, or recorded as written to a synced calendar
    // of this source its host chose for the event type (#47). The host contact is the assigned member
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
        String,
    )> = sqlx::query_as(
        "SELECT b.id, b.guest_name, b.guest_email, COALESCE(b.guest_timezone, 'UTC'), CASE b.time_version WHEN 1 THEN b.start_at ELSE rtrim(b.start_at, 'Z') END AS start_at, CASE b.time_version WHEN 1 THEN b.end_at ELSE rtrim(b.end_at, 'Z') END AS end_at, b.uid,
                et.title, u.name, COALESCE(u.booking_email, u.email), b.caldav_calendar_href, u.timezone,
                u.language, u.id
         FROM bookings b
         JOIN event_types et ON et.id = b.event_type_id
         JOIN accounts a ON a.id = et.account_id
         JOIN users u ON u.id = COALESCE(b.assigned_user_id, a.user_id)
         JOIN caldav_sources cs ON cs.id = ?
         JOIN accounts sa ON sa.id = cs.account_id AND sa.user_id = u.id
         WHERE b.uid = ? AND b.status = 'confirmed'
           -- The href names a copy on this source (#47): the same path on
           -- another server is another calendar.
           AND (b.caldav_source_id = cs.id
                OR (b.caldav_source_id IS NULL
                    AND NOT EXISTS (SELECT 1 FROM booking_write_copies oc
                                    WHERE oc.booking_uid = b.uid
                                      AND oc.calendar_href = b.caldav_calendar_href
                                      AND oc.source_id <> cs.id)))
           AND (b.caldav_calendar_href = cs.write_calendar_href
                OR EXISTS (SELECT 1 FROM booking_write_copies bc
                           JOIN calendars wc ON wc.source_id = bc.source_id
                                            AND wc.href = bc.calendar_href
                           WHERE bc.booking_uid = b.uid
                             AND bc.user_id = u.id
                             AND bc.source_id = cs.id
                             AND bc.calendar_href = b.caldav_calendar_href
                             AND wc.sync_enabled = 1))",
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
        host_user_id,
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
            // Anywhere the provider can look: on Graph, a host who moved
            // the event to another calendar of the mailbox did not delete it.
            OrphanCheck::Provider(provider) => {
                Some(provider.event_exists_anywhere(cal_href, uid).await)
            }
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
    // The host deleted the event: a cancellation by the host for the
    // booking webhooks (#168). Every provider's sync ends up here.
    crate::web::webhooks::enqueue(
        pool,
        &booking_id,
        crate::web::webhooks::Event::Cancelled,
        crate::web::webhooks::Context::cancelled(crate::web::webhooks::Actor::Host, None),
    )
    .await;
    // The host's event is gone from this calendar; its other copies are
    // not: a claimant's (#269), copies recorded at a destination (#47) and
    // the host's other write calendars. Same delete as any host
    // cancellation; the copy already gone is simply not found.
    crate::web::caldav_delete_booking(pool, key, &host_user_id, &booking_uid).await;

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
        guest_footer: crate::email::guest_footer_for_booking(pool, &booking_id).await,
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
    let host_notes =
        crate::web::host_notes_for_reader(pool, &host_details.uid, Some(&host_user_id)).await;
    if let Err(e) =
        crate::email::send_host_cancellation(&smtp_config, &host_details, host_notes.as_deref())
            .await
    {
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
pub(crate) async fn cancel_orphaned_bookings(
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
    // synced ones must not read as "everything orphaned". A copy recorded
    // on a synced calendar the host chose for the event type (#47) counts
    // like the write calendar.
    let orphans: Vec<(String,)> = sqlx::query_as(
        "SELECT b.uid FROM bookings b
         JOIN event_types et ON et.id = b.event_type_id
         JOIN accounts a ON a.id = et.account_id
         JOIN caldav_sources cs ON cs.id = ?
         JOIN accounts sa ON sa.id = cs.account_id
                         AND sa.user_id = COALESCE(b.assigned_user_id, a.user_id)
         WHERE b.status = 'confirmed'
           AND b.caldav_calendar_href IS NOT NULL
           -- The href names a copy on this source (#47): the same path on
           -- another server is another calendar.
           AND (b.caldav_source_id = cs.id
                OR (b.caldav_source_id IS NULL
                    AND NOT EXISTS (SELECT 1 FROM booking_write_copies oc
                                    WHERE oc.booking_uid = b.uid
                                      AND oc.calendar_href = b.caldav_calendar_href
                                      AND oc.source_id <> cs.id)))
           AND ((b.caldav_calendar_href = cs.write_calendar_href
                 AND NOT EXISTS (SELECT 1 FROM calendars wc
                                 WHERE wc.source_id = cs.id
                                   AND wc.href = cs.write_calendar_href
                                   AND wc.sync_enabled = 0))
                OR EXISTS (SELECT 1 FROM booking_write_copies bc
                           JOIN calendars wc ON wc.source_id = bc.source_id
                                            AND wc.href = bc.calendar_href
                           WHERE bc.booking_uid = b.uid
                             AND bc.user_id = sa.user_id
                             AND bc.source_id = cs.id
                             AND bc.calendar_href = b.caldav_calendar_href
                             AND wc.sync_enabled = 1))
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

/// Drop the cached events of a calendar that is not synced, and its sync
/// state, so turning it back on starts with a full fetch. Plain deletes:
/// these events were not removed on the server, so no booking is cancelled
/// for them. Called when the owner turns a calendar off and again on every
/// sync that skips it, which also clears whatever a sync already in flight
/// when it was turned off wrote back.
async fn forget_calendar_events(pool: &SqlitePool, cal_id: &str) {
    // Out-of-office rows go too: the next sync must fetch them again.
    crate::google_calendar::forget_refresh(cal_id);
    let _ = sqlx::query("DELETE FROM events WHERE calendar_id = ?")
        .bind(cal_id)
        .execute(pool)
        .await;
    let _ = sqlx::query("UPDATE calendars SET ctag = NULL, sync_token = NULL WHERE id = ?")
        .bind(cal_id)
        .execute(pool)
        .await;
}

/// Turn syncing of one calendar of `source_id` on or off (#324). Returns
/// whether the calendar was found under that source. Turning it off drops
/// its cached events at once, so they stop blocking availability before the
/// next sync; turning it on clears the sync state, so the next sync fetches
/// it in full rather than trusting a ctag recorded before it was skipped.
pub(crate) async fn set_calendar_sync_enabled(
    pool: &SqlitePool,
    source_id: &str,
    calendar_id: &str,
    enabled: bool,
) -> Result<bool> {
    let current: Option<bool> =
        sqlx::query_scalar("SELECT sync_enabled FROM calendars WHERE id = ? AND source_id = ?")
            .bind(calendar_id)
            .bind(source_id)
            .fetch_optional(pool)
            .await?;
    match current {
        None => return Ok(false),
        Some(current) if current == enabled => return Ok(true),
        Some(_) => {}
    }
    sqlx::query(
        "UPDATE calendars SET sync_enabled = ?, ctag = NULL, sync_token = NULL, \
         sync_generation = sync_generation + 1 WHERE id = ?",
    )
    .bind(enabled)
    .bind(calendar_id)
    .execute(pool)
    .await?;
    if enabled {
        // Mark the source stale, so the next slot page fetches the
        // calendar instead of serving availability without it.
        sqlx::query("UPDATE caldav_sources SET last_synced = NULL WHERE id = ?")
            .bind(source_id)
            .execute(pool)
            .await?;
    }
    if !enabled {
        sqlx::query("DELETE FROM events WHERE calendar_id = ?")
            .bind(calendar_id)
            .execute(pool)
            .await?;
    }
    // Its out-of-office rows went with the events (or never came while it
    // was off), so the next sync must read them again rather than trust a
    // refresh from the last five minutes, as `forget_calendar_events` does.
    crate::google_calendar::forget_refresh(calendar_id);
    tracing::info!(source_id = %source_id, calendar_id = %calendar_id, enabled, "calendar sync setting changed");
    Ok(true)
}

/// The calendar's sync generation, read when a sync starts on it.
async fn calendar_generation(pool: &SqlitePool, cal_id: &str) -> i64 {
    sqlx::query_scalar("SELECT sync_generation FROM calendars WHERE id = ?")
        .bind(cal_id)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten()
        .unwrap_or(0)
}

/// Update stored ctag and sync_token for a calendar, unless its sync
/// setting changed since this sync read `generation`: the events it
/// fetched may have been dropped meanwhile, so the next sync must refetch.
async fn update_calendar_sync_state(
    pool: &SqlitePool,
    cal_id: &str,
    generation: i64,
    ctag: &Option<String>,
    sync_token: &Option<String>,
) {
    let _ = sqlx::query(
        "UPDATE calendars SET ctag = ?, sync_token = ? \
         WHERE id = ? AND sync_enabled = 1 AND sync_generation = ?",
    )
    .bind(ctag)
    .bind(sync_token)
    .bind(cal_id)
    .bind(generation)
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

    /// A series' RDATEs are flagged on its row (#376), so availability
    /// expands it even without an RRULE; a moved occurrence never is.
    #[tokio::test]
    async fn sync_flags_a_series_with_extra_dates() {
        let pool = setup_test_db().await;
        let (source_id, _) = seed_fixtures(&pool).await;
        sqlx::query("INSERT INTO calendars (id, source_id, href) VALUES ('c', ?, '/cal/')")
            .bind(&source_id)
            .execute(&pool)
            .await
            .unwrap();
        let raw = RawEvent {
            href: "/cal/r.ics".into(),
            ical_data: "BEGIN:VCALENDAR\nBEGIN:VEVENT\nUID:r\nDTSTART:20300615T120000\n\
                        DTEND:20300615T130000\nRDATE:20300620T120000\nEND:VEVENT\n\
                        BEGIN:VEVENT\nUID:r\nRECURRENCE-ID:20300620T120000\n\
                        DTSTART:20300620T160000\nDTEND:20300620T170000\nEND:VEVENT\n\
                        END:VCALENDAR"
                .into(),
        };
        upsert_raw_events(&pool, "c", std::slice::from_ref(&raw)).await;
        let rows: Vec<(Option<String>, bool)> =
            sqlx::query_as("SELECT recurrence_id, rdate = 1 FROM events ORDER BY recurrence_id")
                .fetch_all(&pool)
                .await
                .unwrap();
        assert_eq!(
            rows,
            vec![(None, true), (Some("20300620T120000".into()), false)]
        );
    }

    #[test]
    fn busy_only_ical_keeps_extra_dates() {
        let resource = "BEGIN:VEVENT\r\nUID:u\r\nSUMMARY:Private\r\n\
                        DTSTART:20300107T100000\r\nRDATE:20300109T100000,20300111T100000\r\n\
                        END:VEVENT\r\n";
        let kept = busy_only_ical(resource, true).unwrap();
        assert!(!kept.contains("Private"), "{kept}");
        assert_eq!(
            crate::rrule::extract_rdates(&kept, None),
            crate::rrule::extract_rdates(resource, None)
        );
        assert_eq!(crate::rrule::extract_rdates(&kept, None).len(), 2);
    }

    #[tokio::test]
    async fn caldav_orphan_sweep_preserves_google_out_of_office_snapshot() {
        let pool = setup_test_db().await;
        let (source_id, _) = seed_fixtures(&pool).await;
        sqlx::query("INSERT INTO calendars (id, source_id, href) VALUES ('c', ?, '/cal/')")
            .bind(&source_id)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO events (id, calendar_id, uid, start_at, end_at, google_out_of_office) VALUES ('ooo', 'c', 'google-out-of-office:absence', '20300615T000000Z', '20300616T000000Z', 1), ('stale', 'c', 'stale', '20300615T100000', '20300615T110000', 0)")
            .execute(&pool).await.unwrap();
        let raw = RawEvent {
            href: "/cal/normal.ics".into(),
            ical_data: "BEGIN:VEVENT\nUID:normal\nDTSTART:20300615T120000\nDTEND:20300615T130000\nEND:VEVENT".into(),
        };
        upsert_raw_events(&pool, "c", std::slice::from_ref(&raw)).await;
        assert_eq!(
            remove_orphaned_events(&pool, &[0; 32], None, &source_id, "c", &[raw], "20300101")
                .await,
            1
        );
        let uids: Vec<String> = sqlx::query_scalar("SELECT uid FROM events ORDER BY uid")
            .fetch_all(&pool)
            .await
            .unwrap();
        assert_eq!(uids, vec!["google-out-of-office:absence", "normal"]);
    }

    #[tokio::test]
    async fn caldav_delta_deletion_preserves_google_out_of_office_snapshot() {
        let pool = setup_test_db().await;
        let (source_id, _) = seed_fixtures(&pool).await;
        sqlx::query("INSERT INTO calendars (id, source_id, href) VALUES ('c', ?, '/cal/'), ('other', ?, '/other/')")
            .bind(&source_id)
            .bind(&source_id)
            .execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO events (id, calendar_id, uid, start_at, end_at, google_out_of_office) VALUES ('ooo', 'c', 'google-out-of-office:absence', '20300615T000000Z', '20300616T000000Z', 1), ('normal', 'c', 'normal', '20300615T100000', '20300615T110000', 0), ('other-normal', 'other', 'normal', '20300615T100000', '20300615T110000', 0)")
            .execute(&pool).await.unwrap();

        let deleted = delete_events_by_href(
            &pool,
            &[0; 32],
            None,
            &source_id,
            "c",
            &[
                "/cal/google-out-of-office:absence.ics".into(),
                "/cal/normal.ics".into(),
            ],
        )
        .await;
        assert_eq!(deleted, 1);
        let remaining: Vec<(String, i64)> =
            sqlx::query_as("SELECT id, google_out_of_office FROM events ORDER BY id")
                .fetch_all(&pool)
                .await
                .unwrap();
        assert_eq!(
            remaining,
            vec![("ooo".into(), 1), ("other-normal".into(), 0)]
        );
    }

    /// A managed Exchange source is a candidate only while it has something to
    /// do: global Exchange on and complete, its user enabled and trusted.
    /// Otherwise its skipped sync would keep it first in line for ever.
    #[tokio::test]
    async fn next_background_source_skips_idle_managed_sources() {
        let pool = setup_test_db().await;
        let (source, _) = seed_fixtures(&pool).await;
        sqlx::query("UPDATE caldav_sources SET managed = 1 WHERE id = ?")
            .bind(&source)
            .execute(&pool)
            .await
            .unwrap();
        let step = |sql: &'static str| {
            let pool = pool.clone();
            async move { sqlx::raw_sql(sql).execute(&pool).await.unwrap() }
        };
        assert_eq!(next_background_source(&pool, 0).await, None, "feature off");
        step("UPDATE auth_config SET ews_global_enabled = 1 WHERE id = 'singleton'").await;
        assert_eq!(next_background_source(&pool, 0).await, None, "incomplete");
        step(
            "UPDATE auth_config SET ews_global_url = 'https://mail.example.com/EWS/Exchange.asmx', \
             ews_service_username = 'svc', ews_service_password_enc = 'x' WHERE id = 'singleton'",
        )
        .await;
        assert_eq!(next_background_source(&pool, 0).await, None, "untrusted");
        step("UPDATE users SET email_trusted = 1, enabled = 0").await;
        assert_eq!(next_background_source(&pool, 0).await, None, "suspended");
        step("UPDATE users SET enabled = 1").await;
        assert_eq!(
            next_background_source(&pool, 0).await.as_deref(),
            Some(source.as_str())
        );
    }

    /// A managed source keeps busy times only: of a recurring event, the
    /// series' start and exception dates and its moved occurrences'
    /// RECURRENCE-IDs, which availability reads, nothing else.
    #[test]
    fn busy_only_ical_keeps_timing_and_drops_content() {
        let resource = "BEGIN:VCALENDAR\r\nBEGIN:VEVENT\r\nUID:u1\r\nSUMMARY:Salary review\r\n\
                        DESCRIPTION:Private\r\nLOCATION:Room 1\r\n\
                        DTSTART;TZID=Europe/Paris:20300107T100000\r\n\
                        RRULE:FREQ=WEEKLY\r\nEXDATE;TZID=Europe/Paris:20300114T100000\r\n\
                        ATTENDEE:mailto:boss@example.com\r\nEND:VEVENT\r\n\
                        BEGIN:VEVENT\r\nUID:u1\r\nSUMMARY:Salary review (moved)\r\n\
                        RECURRENCE-ID;TZID=Europe/Paris:20300121T100000\r\n\
                        DTSTART;TZID=Europe/Paris:20300121T150000\r\nEND:VEVENT\r\n\
                        END:VCALENDAR\r\n";
        assert_eq!(
            busy_only_ical(resource, false),
            None,
            "no rrule: nothing kept"
        );
        let kept = busy_only_ical(resource, true).unwrap();
        assert_eq!(
            kept,
            "BEGIN:VEVENT\r\nDTSTART;TZID=Europe/Paris:20300107T100000\r\n\
             EXDATE;TZID=Europe/Paris:20300114T100000\r\nEND:VEVENT\r\n\
             BEGIN:VEVENT\r\nRECURRENCE-ID;TZID=Europe/Paris:20300121T100000\r\n\
             END:VEVENT\r\n"
        );
        let exdates = crate::rrule::extract_exdates(&kept, Some("Europe/Paris"));
        assert_eq!(
            exdates,
            crate::rrule::extract_exdates(resource, Some("Europe/Paris")),
            "availability reads the same exceptions as from the full text"
        );
        assert_eq!(exdates.len(), 2);
    }

    /// An EWS client listing whatever calendars a test sets, with no events.
    struct ListingProvider(std::sync::Mutex<Vec<&'static str>>);

    #[async_trait::async_trait]
    impl crate::providers::CalendarProvider for ListingProvider {
        async fn check_connection(&self) -> Result<bool> {
            Ok(true)
        }
        async fn list_calendars(&self) -> Result<Vec<crate::providers::RemoteCalendar>> {
            Ok(self
                .0
                .lock()
                .unwrap()
                .iter()
                .map(|id| crate::providers::RemoteCalendar {
                    id: id.to_string(),
                    display_name: Some(id.to_string()),
                    color: None,
                    change_marker: None,
                    sync_state: None,
                    read_only: false,
                    shared: false,
                })
                .collect())
        }
        async fn fetch_events(&self, _: &str) -> Result<Vec<crate::providers::RawEvent>> {
            Ok(vec![])
        }
        async fn fetch_events_since(
            &self,
            _: &str,
            _: &str,
        ) -> Result<Vec<crate::providers::RawEvent>> {
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
            Ok(false)
        }
    }

    /// A calendar an EWS server stops listing is kept through one listing and
    /// dropped at the second in a row, with its event-type selection; one
    /// listed again in between starts over; an empty listing drops nothing.
    /// The source here is the user's own EWS source, not a managed one.
    #[tokio::test]
    async fn an_unlisted_ews_calendar_goes_only_after_two_listings() {
        let pool = setup_test_db().await;
        let (source, et_id) = seed_fixtures(&pool).await;
        sqlx::query("UPDATE caldav_sources SET provider_type = 'ews', write_calendar_href = 'B' WHERE id = ?")
            .bind(&source)
            .execute(&pool)
            .await
            .unwrap();
        let provider = ListingProvider(std::sync::Mutex::new(vec!["A", "B"]));
        let key = [0u8; 32];
        sync_ews_source(&pool, &key, &provider, &source)
            .await
            .unwrap();
        let b: String =
            sqlx::query_scalar("SELECT id FROM calendars WHERE source_id = ? AND href = 'B'")
                .bind(&source)
                .fetch_one(&pool)
                .await
                .unwrap();
        sqlx::query("INSERT INTO event_type_calendars (event_type_id, calendar_id) VALUES (?, ?)")
            .bind(&et_id)
            .bind(&b)
            .execute(&pool)
            .await
            .unwrap();
        let hrefs = || async {
            sqlx::query_scalar::<_, String>(
                "SELECT href FROM calendars WHERE source_id = ? ORDER BY href",
            )
            .bind(&source)
            .fetch_all(&pool)
            .await
            .unwrap()
        };

        // An empty answer proves nothing.
        *provider.0.lock().unwrap() = vec![];
        sync_ews_source(&pool, &key, &provider, &source)
            .await
            .unwrap();
        assert_eq!(hrefs().await, ["A", "B"]);

        // Missing once: kept. Listed again: the count starts over.
        *provider.0.lock().unwrap() = vec!["A"];
        sync_ews_source(&pool, &key, &provider, &source)
            .await
            .unwrap();
        assert_eq!(hrefs().await, ["A", "B"]);
        *provider.0.lock().unwrap() = vec!["A", "B"];
        sync_ews_source(&pool, &key, &provider, &source)
            .await
            .unwrap();
        *provider.0.lock().unwrap() = vec!["A"];
        sync_ews_source(&pool, &key, &provider, &source)
            .await
            .unwrap();
        assert_eq!(hrefs().await, ["A", "B"], "one miss after a listing");

        // Missing twice in a row: gone, with its selection and the write
        // calendar that pointed at it.
        sync_ews_source(&pool, &key, &provider, &source)
            .await
            .unwrap();
        assert_eq!(hrefs().await, ["A"]);
        let (links, write): (i64, Option<String>) = sqlx::query_as(
            "SELECT (SELECT COUNT(*) FROM event_type_calendars), \
                    (SELECT write_calendar_href FROM caldav_sources WHERE id = ?)",
        )
        .bind(&source)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!((links, write), (0, None));
    }

    /// #310: the background loop only picks a source whose last attempt,
    /// success or failure, is at least the minimum age; the stalest first.
    #[tokio::test]
    async fn next_background_source_honours_the_minimum_age() {
        let pool = setup_test_db().await;
        let (recent, _) = seed_fixtures(&pool).await;
        let account: String =
            sqlx::query_scalar("SELECT account_id FROM caldav_sources WHERE id = ?")
                .bind(&recent)
                .fetch_one(&pool)
                .await
                .unwrap();
        let older = Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO caldav_sources (id, account_id, name, url, username) VALUES (?, ?, 'older', 'https://dav.example.com/', 'user')")
            .bind(&older).bind(&account).execute(&pool).await.unwrap();
        sqlx::query(
            "UPDATE caldav_sources SET last_synced = datetime('now', '-1 minutes'), last_full_sync = datetime('now') WHERE id = ?",
        )
        .bind(&recent)
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "UPDATE caldav_sources SET last_synced = datetime('now', '-10 minutes'), last_full_sync = datetime('now') WHERE id = ?",
        )
        .bind(&older)
        .execute(&pool)
        .await
        .unwrap();

        // 0 keeps the historic behaviour: the stalest, however fresh.
        assert_eq!(
            next_background_source(&pool, 0).await.as_deref(),
            Some(older.as_str())
        );
        assert_eq!(
            next_background_source(&pool, 300).await.as_deref(),
            Some(older.as_str())
        );
        assert_eq!(next_background_source(&pool, 900).await, None);

        // A failed attempt counts as an attempt: no retry before the minimum age.
        sqlx::query("UPDATE caldav_sources SET last_sync_error_at = datetime('now') WHERE id = ?")
            .bind(&older)
            .execute(&pool)
            .await
            .unwrap();
        assert_eq!(next_background_source(&pool, 300).await, None);
        assert_eq!(
            next_background_source(&pool, 0).await.as_deref(),
            Some(recent.as_str())
        );

        // Recently synced, but due its daily full resync: picked anyway, since
        // on-demand syncs refresh last_synced without ever running one.
        sqlx::query("UPDATE caldav_sources SET last_sync_error_at = NULL, last_synced = datetime('now'), last_full_sync = datetime('now', '-25 hours') WHERE id = ?")
            .bind(&older).execute(&pool).await.unwrap();
        sqlx::query("UPDATE caldav_sources SET last_synced = datetime('now'), last_full_sync = datetime('now') WHERE id = ?")
            .bind(&recent).execute(&pool).await.unwrap();
        assert_eq!(
            next_background_source(&pool, 900).await.as_deref(),
            Some(older.as_str())
        );
        // ...unless it is failing: then it waits the minimum age like any other.
        sqlx::query("UPDATE caldav_sources SET last_sync_error_at = datetime('now') WHERE id = ?")
            .bind(&older)
            .execute(&pool)
            .await
            .unwrap();
        assert_eq!(next_background_source(&pool, 900).await, None);
        // 0 ignores timestamps entirely, even one in the future (clock step).
        sqlx::query(
            "UPDATE caldav_sources SET last_synced = datetime('now', '+1 hours') WHERE id = ?",
        )
        .bind(&recent)
        .execute(&pool)
        .await
        .unwrap();
        assert!(next_background_source(&pool, 0).await.is_some());

        // A disabled source is never picked; a never-synced one is due at once.
        sqlx::query("UPDATE caldav_sources SET enabled = 0 WHERE id = ?")
            .bind(&older)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("UPDATE caldav_sources SET last_synced = NULL WHERE id = ?")
            .bind(&recent)
            .execute(&pool)
            .await
            .unwrap();
        assert_eq!(
            next_background_source(&pool, 900).await.as_deref(),
            Some(recent.as_str())
        );
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
        /// The event left the write calendar but is elsewhere in the mailbox
        /// (moved in Outlook).
        moved: bool,
        /// Every calendar a fetch was asked for, in order.
        fetched: std::sync::Mutex<Vec<String>>,
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
            self.fetched.lock().unwrap().push(calendar.to_string());
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
        async fn event_exists_anywhere(&self, calendar: &str, uid: &str) -> Result<bool> {
            if self.moved {
                return Ok(true);
            }
            self.event_exists(calendar, uid).await
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
            moved: false,
            fetched: Default::default(),
        }
    }

    /// A snapshot of the write calendar holding one unrelated event, so that
    /// EWS, which reads nothing into an empty snapshot, reconciles it.
    fn snapshot_with_other_event(href: &str) -> SnapshotProvider {
        let mut provider = snapshot_provider(href);
        provider.events = other_event_snapshot();
        provider
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
        let (calendar, _, _, _) =
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
        let (_source_a, et_a, _, _) = provider_fixture(&pool, kinds::MICROSOFT).await;
        let (source_b, _, _, cal_b) = provider_fixture(&pool, kinds::MICROSOFT).await;
        let booking = provider_booking(&pool, &et_a, "colliding-uid").await;
        cached_provider_event(
            &pool,
            &cal_b,
            "colliding-uid",
            &Utc::now().format("%Y%m%dT%H%M%SZ").to_string(),
        )
        .await;

        sync_provider_calendars(
            &pool,
            &[0; 32],
            &snapshot_with_other_event("write"),
            &source_b,
        )
        .await
        .unwrap();

        assert_eq!(cached_uids(&pool, &cal_b).await, ["still-there"]);
        assert_eq!(
            booking_status_seq(&pool, &booking).await,
            ("confirmed".into(), 0)
        );
    }

    #[tokio::test]
    async fn provider_orphan_only_cancels_on_designated_write_calendar() {
        let pool = setup_test_db().await;
        let (source, et, _, write_calendar) = provider_fixture(&pool, kinds::MICROSOFT).await;
        let booking = provider_booking(&pool, &et, "owned-uid").await;
        let (secondary, _, _, _) =
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
        let (source_a, et_a, _, cal_a) = provider_fixture(&pool, kinds::MICROSOFT).await;
        let (source_b, _, user_b, cal_b) = provider_fixture(&pool, kinds::MICROSOFT).await;
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

        sync_provider_calendars(
            &pool,
            &[0; 32],
            &snapshot_with_other_event("write"),
            &source_a,
        )
        .await
        .unwrap();
        assert_eq!(
            booking_status_seq(&pool, &booking).await,
            ("confirmed".into(), 0)
        );
        sync_provider_calendars(
            &pool,
            &[0; 32],
            &snapshot_with_other_event("write"),
            &source_b,
        )
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
        let (source, _, _, calendar) = provider_fixture(&pool, kinds::MICROSOFT).await;
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
        let (source, et, _, calendar) = provider_fixture(&pool, kinds::MICROSOFT).await;
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
        // `moved`: the host moved the event to another calendar in Outlook.
        for (exists, moved, status, cached) in [
            (Some(true), false, "confirmed", 0),
            (None, false, "confirmed", 1),
            (Some(false), false, "cancelled", 0),
            (Some(false), true, "confirmed", 0),
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
            provider.moved = moved;
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
        let (source, event_type, _, write_calendar) =
            provider_fixture(&pool, kinds::MICROSOFT).await;
        let mut colleague = remote_calendar("shared");
        colleague.shared = true;
        let (shared, _, _, _) = upsert_calendar_provider(&pool, &source, &colleague)
            .await
            .unwrap();
        let start = Utc::now().format("%Y%m%dT%H%M%SZ").to_string();
        cached_provider_event(&pool, &shared, "colleague-meeting", &start).await;

        let mut provider = snapshot_provider("write");
        provider.others.push(colleague);
        provider.unreadable.push("shared".into());
        sync_provider_calendars(&pool, &[0; 32], &provider, &source)
            .await
            .unwrap();
        assert_eq!(cached_uids(&pool, &shared).await, ["colleague-meeting"]);
        assert!(cached_uids(&pool, &write_calendar).await.is_empty());

        // Once an event type checks it, its new events would be missed: the
        // sync reports it, and still keeps the cache and syncs the rest.
        sqlx::query("INSERT INTO event_type_calendars (event_type_id, calendar_id) VALUES (?, ?)")
            .bind(&event_type)
            .bind(&shared)
            .execute(&pool)
            .await
            .unwrap();
        let error = sync_provider_calendars(&pool, &[0; 32], &provider, &source)
            .await
            .unwrap_err();
        assert!(
            error.to_string().contains("counts for availability"),
            "{error}"
        );
        assert_eq!(cached_uids(&pool, &shared).await, ["colleague-meeting"]);
        sqlx::query("DELETE FROM event_type_calendars")
            .execute(&pool)
            .await
            .unwrap();

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
        let (gone, _, _, _) = upsert_calendar_provider(&pool, &source, &remote_calendar("gone"))
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

    async fn sync_enabled_of(pool: &SqlitePool, source: &str, href: &str) -> Option<bool> {
        sqlx::query_scalar("SELECT sync_enabled FROM calendars WHERE source_id = ? AND href = ?")
            .bind(source)
            .bind(href)
            .fetch_optional(pool)
            .await
            .unwrap()
    }

    /// #324 and the unlisted-calendar cleanup: a calendar the owner turned
    /// off is still listed by the provider, so it is neither fetched nor
    /// deleted, and keeps its setting through syncs and discovery. Only
    /// once the provider stops listing it does it go.
    #[tokio::test]
    async fn a_provider_calendar_turned_off_is_kept_until_it_is_no_longer_listed() {
        let pool = setup_test_db().await;
        let (source, event_type, _, _) = provider_fixture(&pool, kinds::MICROSOFT).await;
        let (colleague, _, _, _) =
            upsert_calendar_provider(&pool, &source, &remote_calendar("colleague"))
                .await
                .unwrap();
        sqlx::query("INSERT INTO event_type_calendars (event_type_id, calendar_id) VALUES (?, ?)")
            .bind(&event_type)
            .bind(&colleague)
            .execute(&pool)
            .await
            .unwrap();
        assert!(set_calendar_sync_enabled(&pool, &source, &colleague, false)
            .await
            .unwrap());
        // Written back by a sync that was in flight when it was turned off.
        let start = Utc::now().format("%Y%m%dT%H%M%SZ").to_string();
        cached_provider_event(&pool, &colleague, "late", &start).await;

        let mut provider = snapshot_provider("write");
        provider.others.push(remote_calendar("colleague"));
        sync_provider_calendars(&pool, &[0; 32], &provider, &source)
            .await
            .unwrap();
        assert_eq!(*provider.fetched.lock().unwrap(), ["write"]);
        assert_eq!(
            sync_enabled_of(&pool, &source, "colleague").await,
            Some(false)
        );
        assert!(cached_uids(&pool, &colleague).await.is_empty());
        let selected: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM event_type_calendars WHERE calendar_id = ?")
                .bind(&colleague)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(selected, 1, "the event type keeps its selection");

        // Discovery (the OAuth callback, a reconnect) keeps the setting too.
        discover_provider_calendars(&pool, &provider, &source)
            .await
            .unwrap();
        assert_eq!(
            sync_enabled_of(&pool, &source, "colleague").await,
            Some(false)
        );

        // Gone from the provider: removed, and there is nothing left to toggle.
        sync_provider_calendars(&pool, &[0; 32], &snapshot_provider("write"), &source)
            .await
            .unwrap();
        assert_eq!(sync_enabled_of(&pool, &source, "colleague").await, None);
        assert!(!set_calendar_sync_enabled(&pool, &source, &colleague, true)
            .await
            .unwrap());
    }

    /// "No calendar of this source can be read" counts the calendars the
    /// sync fetched: one turned off does not hide that every synced one is
    /// unreadable.
    #[tokio::test]
    async fn every_synced_calendar_unreadable_fails_despite_one_turned_off() {
        let pool = setup_test_db().await;
        let (source, _, _, _) = provider_fixture(&pool, kinds::MICROSOFT).await;
        sqlx::query("UPDATE caldav_sources SET write_calendar_href = NULL WHERE id = ?")
            .bind(&source)
            .execute(&pool)
            .await
            .unwrap();
        let (off, _, _, _) = upsert_calendar_provider(&pool, &source, &remote_calendar("off"))
            .await
            .unwrap();
        set_calendar_sync_enabled(&pool, &source, &off, false)
            .await
            .unwrap();
        let mut provider = snapshot_provider("write");
        provider.others = vec![remote_calendar("shared"), remote_calendar("off")];
        provider.unreadable = vec!["write".into(), "shared".into()];
        assert!(sync_provider_calendars(&pool, &[0; 32], &provider, &source)
            .await
            .is_err());
        assert_eq!(*provider.fetched.lock().unwrap(), ["write", "shared"]);
    }

    /// Turns the calendar off and on again while it is being fetched, as
    /// the owner saving the calendar selection mid-sync would.
    struct TogglingProvider {
        inner: SnapshotProvider,
        pool: SqlitePool,
        source: String,
        calendar: String,
    }

    #[async_trait::async_trait]
    impl crate::providers::CalendarProvider for TogglingProvider {
        async fn check_connection(&self) -> Result<bool> {
            Ok(true)
        }
        async fn list_calendars(&self) -> Result<Vec<crate::providers::RemoteCalendar>> {
            self.inner.list_calendars().await
        }
        async fn fetch_events(&self, calendar: &str) -> Result<Vec<ProviderRawEvent>> {
            self.inner.fetch_events(calendar).await
        }
        async fn fetch_events_since(
            &self,
            calendar: &str,
            since: &str,
        ) -> Result<Vec<ProviderRawEvent>> {
            for enabled in [false, true] {
                set_calendar_sync_enabled(&self.pool, &self.source, &self.calendar, enabled)
                    .await?;
            }
            self.inner.fetch_events_since(calendar, since).await
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
            self.inner.event_exists("", "").await
        }
    }

    /// The generation guard of #324 on snapshot providers: a snapshot
    /// fetched across a change of the calendar's sync setting is discarded,
    /// so it does not refill the cache the change emptied; the next sync
    /// fetches the calendar again.
    #[tokio::test]
    async fn a_provider_snapshot_fetched_across_a_sync_setting_change_is_discarded() {
        let pool = setup_test_db().await;
        let (source, _, _, calendar) = provider_fixture(&pool, kinds::MICROSOFT).await;
        let mut inner = snapshot_provider("write");
        inner.events = other_event_snapshot();
        let provider = TogglingProvider {
            inner,
            pool: pool.clone(),
            source: source.clone(),
            calendar: calendar.clone(),
        };
        sync_provider_calendars(&pool, &[0; 32], &provider, &source)
            .await
            .unwrap();
        assert!(cached_uids(&pool, &calendar).await.is_empty());
        // Turning it on marked the source stale; the discarded sync must not
        // mark it fresh again, or the next slot page would skip the refetch.
        assert_eq!(source_stamps(&pool, &source).await, (None, None));

        let mut provider = snapshot_provider("write");
        provider.events = other_event_snapshot();
        sync_provider_calendars(&pool, &[0; 32], &provider, &source)
            .await
            .unwrap();
        assert_eq!(cached_uids(&pool, &calendar).await, ["still-there"]);
        let (synced, full) = source_stamps(&pool, &source).await;
        assert!(synced.is_some() && full.is_some());
    }

    async fn source_stamps(pool: &SqlitePool, source: &str) -> (Option<String>, Option<String>) {
        sqlx::query_as("SELECT last_synced, last_full_sync FROM caldav_sources WHERE id = ?")
            .bind(source)
            .fetch_one(pool)
            .await
            .unwrap()
    }

    /// The generation check and the write are one statement: a snapshot
    /// read for an older generation writes nothing, whatever happened
    /// between the check and the write.
    #[tokio::test]
    async fn a_snapshot_for_an_older_generation_writes_nothing() {
        let pool = setup_test_db().await;
        let (source, _, _, calendar) = provider_fixture(&pool, kinds::MICROSOFT).await;
        let generation = calendar_generation(&pool, &calendar).await;
        set_calendar_sync_enabled(&pool, &source, &calendar, false)
            .await
            .unwrap();
        set_calendar_sync_enabled(&pool, &source, &calendar, true)
            .await
            .unwrap();
        let written = upsert_provider_events(
            &pool,
            &calendar,
            &other_event_snapshot(),
            false,
            Some(generation),
        )
        .await;
        assert_eq!(written, 0);
        assert!(cached_uids(&pool, &calendar).await.is_empty());
        let current = calendar_generation(&pool, &calendar).await;
        upsert_provider_events(
            &pool,
            &calendar,
            &other_event_snapshot(),
            false,
            Some(current),
        )
        .await;
        assert_eq!(cached_uids(&pool, &calendar).await, ["still-there"]);
    }

    /// A Graph source counts as read only once a sync completed without a
    /// calendar failure; until then its hosts are busy (see
    /// `microsoft_coverage_gap`). EWS stamps every completed sync, as before.
    #[tokio::test]
    async fn only_a_clean_graph_snapshot_makes_the_source_ready() {
        let pool = setup_test_db().await;
        let (source, _, user, _) = provider_fixture(&pool, kinds::MICROSOFT).await;
        let mut failing = snapshot_provider("write");
        failing.fetch_fails = true;
        assert!(sync_provider_calendars(&pool, &[0; 32], &failing, &source)
            .await
            .is_err());
        assert_eq!(source_stamps(&pool, &source).await.1, None);
        assert_eq!(
            microsoft_coverage_gap(&pool, &user, Utc::now()).await,
            Some(CoverageGap::NotYetSynced)
        );
        // A colleague's calendar nothing checks failing, and an item calrs
        // cannot read, are reported but do not hold the source back.
        let mut colleague = remote_calendar("colleague");
        colleague.shared = true;
        let mut partial = snapshot_provider("write");
        partial.others.push(colleague);
        partial.unreadable.push("colleague".into());
        partial.events.push(ProviderRawEvent {
            remote_id: "odd".into(),
            ical: String::new(),
        });
        assert!(sync_provider_calendars(&pool, &[0; 32], &partial, &source)
            .await
            .is_err());
        assert!(source_stamps(&pool, &source).await.1.is_some());
        assert_eq!(microsoft_coverage_gap(&pool, &user, Utc::now()).await, None);
        sync_provider_calendars(&pool, &[0; 32], &snapshot_provider("write"), &source)
            .await
            .unwrap();
        assert!(source_stamps(&pool, &source).await.1.is_some());
        assert_eq!(microsoft_coverage_gap(&pool, &user, Utc::now()).await, None);
    }

    /// A booking whose Graph event the host deleted before any snapshot
    /// cached it: cancelled once Graph confirms it gone, on the same
    /// evidence as the cached path; never when the lookup is inconclusive,
    /// the write was never verified, the snapshot still holds it, or on EWS.
    /// The never-cached sweep follows #47's record of where a booking was
    /// written: a booking stored on another source, under the same href,
    /// is not this calendar's to cancel.
    /// A booking's uid vanishing from a calendar the booking was not written
    /// to (here a destination holding another booking's copy, #47) does not
    /// route the booking into a cancellation, whatever the provider says.
    #[tokio::test]
    async fn a_uid_vanishing_from_another_calendar_cancels_nothing() {
        let pool = setup_test_db().await;
        let (source, et, user, _) = provider_fixture(&pool, kinds::MICROSOFT).await;
        let booking = provider_booking(&pool, &et, "b1").await;
        let et2 = Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO event_types (id, account_id, slug, title, duration_min) SELECT ?, account_id, 'other', 'Other', 30 FROM event_types WHERE id = ?")
            .bind(&et2)
            .bind(&et)
            .execute(&pool)
            .await
            .unwrap();
        let other = provider_booking(&pool, &et2, "b2").await;
        let (dest, _, _, _) = upsert_calendar_provider(&pool, &source, &remote_calendar("dest"))
            .await
            .unwrap();
        sqlx::query("INSERT INTO booking_write_copies (booking_uid, user_id, source_id, calendar_href) VALUES ('b2', ?, ?, 'dest')")
            .bind(&user)
            .bind(&source)
            .execute(&pool)
            .await
            .unwrap();
        let start = Utc::now().format("%Y%m%dT%H%M%SZ").to_string();
        cached_provider_event(&pool, &dest, "b1", &start).await;
        // Still on the write calendar, gone from `dest`; the provider would
        // even say "gone" if asked.
        let mut provider = snapshot_provider("write");
        provider.others.push(remote_calendar("dest"));
        provider.events.push(ProviderRawEvent {
            remote_id: "b1".into(),
            ical: format!(
                "BEGIN:VEVENT\r\nUID:b1\r\nDTSTART:{start}\r\nDTEND:{start}\r\nEND:VEVENT\r\n"
            ),
        });
        sync_provider_calendars(&pool, &[0; 32], &provider, &source)
            .await
            .unwrap();
        assert!(cached_uids(&pool, &dest).await.is_empty());
        assert_eq!(booking_status(&pool, &booking).await, "confirmed");
        assert_eq!(booking_status(&pool, &other).await, "confirmed");
    }

    /// Orphan checks that stay inconclusive (a calendar Graph will not let
    /// calrs read, say) would silently stop cancellations on the source: past
    /// a day, the sync reports it; one conclusive sync clears it.
    #[tokio::test]
    async fn orphan_checks_inconclusive_for_a_day_fail_the_sync() {
        let pool = setup_test_db().await;
        let (source, et, _, calendar) = provider_fixture(&pool, kinds::MICROSOFT).await;
        provider_booking(&pool, &et, "doubt").await;
        let start = Utc::now().format("%Y%m%dT%H%M%SZ").to_string();
        let since = |pool: SqlitePool, source: String| async move {
            sqlx::query_scalar::<_, Option<String>>(
                "SELECT orphan_check_inconclusive_since FROM caldav_sources WHERE id = ?",
            )
            .bind(&source)
            .fetch_one(&pool)
            .await
            .unwrap()
        };
        let mut provider = snapshot_provider("write");
        provider.exists = None;
        cached_provider_event(&pool, &calendar, "doubt", &start).await;
        sync_provider_calendars(&pool, &[0; 32], &provider, &source)
            .await
            .unwrap();
        assert!(since(pool.clone(), source.clone()).await.is_some());

        sqlx::query("UPDATE caldav_sources SET orphan_check_inconclusive_since = datetime('now', '-2 days') WHERE id = ?")
            .bind(&source)
            .execute(&pool)
            .await
            .unwrap();
        let error = sync_provider_calendars(&pool, &[0; 32], &provider, &source)
            .await
            .unwrap_err();
        assert!(error.to_string().contains("could not confirm"), "{error}");

        provider.exists = Some(true);
        sync_provider_calendars(&pool, &[0; 32], &provider, &source)
            .await
            .unwrap();
        assert_eq!(since(pool.clone(), source.clone()).await, None);
    }

    /// A Graph orphan cancellation queues the `booking.cancelled` webhook
    /// (#168) as a cancellation by the host, once: a later sync finds
    /// nothing more to cancel.
    #[tokio::test]
    async fn a_graph_orphan_cancellation_queues_one_webhook() {
        let pool = setup_test_db().await;
        let (source, et, user, calendar) = provider_fixture(&pool, kinds::MICROSOFT).await;
        let booking = provider_booking(&pool, &et, "gone-in-outlook").await;
        // Webhooks are an admin's unless the admin opens them to everyone.
        sqlx::query("UPDATE users SET role = 'admin' WHERE id = ?")
            .bind(&user)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO webhooks (id, user_id, url, secret_enc) SELECT 'wh', a.user_id, 'https://93.184.215.14/h', 'x' FROM event_types et JOIN accounts a ON a.id = et.account_id WHERE et.id = ?")
            .bind(&et)
            .execute(&pool)
            .await
            .unwrap();
        let start = Utc::now().format("%Y%m%dT%H%M%SZ").to_string();
        cached_provider_event(&pool, &calendar, "gone-in-outlook", &start).await;
        for _ in 0..2 {
            sync_provider_calendars(&pool, &[0; 32], &snapshot_provider("write"), &source)
                .await
                .unwrap();
        }
        assert_eq!(booking_status(&pool, &booking).await, "cancelled");
        let payloads: Vec<String> = sqlx::query_scalar("SELECT payload FROM webhook_deliveries")
            .fetch_all(&pool)
            .await
            .unwrap();
        assert_eq!(payloads.len(), 1, "{payloads:?}");
        let v: serde_json::Value = serde_json::from_str(&payloads[0]).unwrap();
        assert_eq!(v["event"], "booking.cancelled");
        assert_eq!(v["booking"]["cancelled_by"], "host");
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

    /// EWS never deletes a calendar, as before Graph support: one FindFolder
    /// listing that misses a folder must not cascade away its events and
    /// event-type selections, nor bring it back synced when the owner had
    /// turned it off.
    /// On Graph a calendar no longer listed goes, except the write calendar:
    /// the bookings written there are tracked by it.
    #[tokio::test]
    async fn graph_never_deletes_the_write_calendar_missing_from_a_listing() {
        let pool = setup_test_db().await;
        let (source, _, _, write_calendar) = provider_fixture(&pool, kinds::MICROSOFT).await;
        let start = Utc::now().format("%Y%m%dT%H%M%SZ").to_string();
        cached_provider_event(&pool, &write_calendar, "booked", &start).await;
        upsert_calendar_provider(&pool, &source, &remote_calendar("gone"))
            .await
            .unwrap();

        // Kept, but the owner is told to pick another one.
        let error = sync_provider_calendars(&pool, &[0; 32], &snapshot_provider("other"), &source)
            .await
            .unwrap_err();
        assert!(error.to_string().contains("no longer listed"), "{error}");
        discover_provider_calendars(&pool, &snapshot_provider("other"), &source)
            .await
            .unwrap();
        assert_eq!(sync_enabled_of(&pool, &source, "write").await, Some(true));
        assert_eq!(sync_enabled_of(&pool, &source, "gone").await, None);
        assert_eq!(cached_uids(&pool, &write_calendar).await, ["booked"]);
    }

    /// EWS keeps the snapshot rules it was field-tested with: an empty
    /// snapshot removes nothing, and an item calrs reads badly neither fails
    /// the sync nor stops the orphan sweep. The stricter rules are Graph's.
    /// A suspended user's Microsoft source is not synced, in the background
    /// or on demand, so their refresh token goes unused. Their other sources,
    /// and other users' Microsoft sources, sync as before.
    #[tokio::test]
    async fn a_suspended_users_microsoft_source_is_not_synced() {
        let pool = setup_test_db().await;
        let (graph_source, _, graph_user, _) = provider_fixture(&pool, kinds::MICROSOFT).await;
        sqlx::query("UPDATE users SET enabled = 0 WHERE id = ?")
            .bind(&graph_user)
            .execute(&pool)
            .await
            .unwrap();
        assert_eq!(next_background_source(&pool, 0).await, None);
        sync_source_by_id(&pool, &[0; 32], &graph_source).await;
        sync_if_stale(&pool, &[0; 32], &graph_user).await;
        // Any attempt would have failed (no tokens) and been recorded.
        let attempted: Option<String> =
            sqlx::query_scalar("SELECT last_sync_error_at FROM caldav_sources WHERE id = ?")
                .bind(&graph_source)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(attempted, None);

        let (ews_source, _, ews_user, _) = provider_fixture(&pool, kinds::EWS).await;
        sqlx::query("UPDATE users SET enabled = 0 WHERE id = ?")
            .bind(&ews_user)
            .execute(&pool)
            .await
            .unwrap();
        assert_eq!(
            next_background_source(&pool, 0).await.as_deref(),
            Some(ews_source.as_str())
        );

        sqlx::query("UPDATE users SET enabled = 1 WHERE id = ?")
            .bind(&graph_user)
            .execute(&pool)
            .await
            .unwrap();
        sync_if_stale(&pool, &[0; 32], &graph_user).await;
        let attempted: Option<String> =
            sqlx::query_scalar("SELECT last_sync_error_at FROM caldav_sources WHERE id = ?")
                .bind(&graph_source)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert!(attempted.is_some());
    }

    #[tokio::test]
    async fn graph_sync_respects_its_forward_window() {
        let pool = setup_test_db().await;
        let (graph_source, _, _, graph_calendar) = provider_fixture(&pool, kinds::MICROSOFT).await;
        let now = Utc::now();
        for (uid, days) in [
            ("past", -91),
            ("in-window", 1),
            ("in-window-late", 700),
            ("future", 731),
        ] {
            cached_provider_event(
                &pool,
                &graph_calendar,
                uid,
                &(now + chrono::Duration::days(days))
                    .format("%Y%m%dT%H%M%SZ")
                    .to_string(),
            )
            .await;
        }
        sync_provider_calendars(&pool, &[0; 32], &snapshot_provider("write"), &graph_source)
            .await
            .unwrap();
        assert_eq!(
            cached_uids(&pool, &graph_calendar).await,
            ["future", "past"]
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

        // A webhook on the host's event types (#168).
        sqlx::query("INSERT INTO webhooks (id, user_id, url, secret_enc) SELECT 'wh', a.user_id, 'https://93.184.215.14/h', 'x' FROM event_types et JOIN accounts a ON a.id = et.account_id WHERE et.id = ?")
            .bind(&et_id)
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

        // Queued as a cancellation by the host.
        let payload: String = sqlx::query_scalar("SELECT payload FROM webhook_deliveries")
            .fetch_one(&pool)
            .await
            .unwrap();
        let v: serde_json::Value = serde_json::from_str(&payload).unwrap();
        assert_eq!(v["event"], "booking.cancelled");
        assert_eq!(v["booking"]["cancelled_by"], "host");

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

    // --- #324: calendars the owner does not sync ---

    /// A CalDAV server with two calendars, `/cals/mine/` (one event) and
    /// `/cals/colleague/`. Returns its base URL and the requests it got,
    /// as "METHOD path".
    async fn spawn_two_calendar_dav() -> (String, Arc<std::sync::Mutex<Vec<String>>>) {
        use axum::http::{Method, StatusCode, Uri};
        let requests: Arc<std::sync::Mutex<Vec<String>>> = Arc::default();
        let log = requests.clone();
        let app = axum::Router::new().fallback(move |method: Method, uri: Uri| {
            let log = log.clone();
            async move {
                let path = uri.path().to_string();
                log.lock().unwrap().push(format!("{method} {path}"));
                let ms = |inner: &str| {
                    format!(
                        "<?xml version=\"1.0\"?><d:multistatus xmlns:d=\"DAV:\" \
                         xmlns:cal=\"urn:ietf:params:xml:ns:caldav\" \
                         xmlns:cs=\"http://calendarserver.org/ns/\">{inner}</d:multistatus>"
                    )
                };
                let body = match (method.as_str(), path.trim_end_matches('/')) {
                    ("PROPFIND", "/dav") => {
                        ms("<d:response><d:href>/dav/</d:href><d:propstat><d:prop>\
                         <d:current-user-principal><d:href>/p/</d:href></d:current-user-principal>\
                         </d:prop></d:propstat></d:response>")
                    }
                    ("PROPFIND", "/p") => {
                        ms("<d:response><d:href>/p/</d:href><d:propstat><d:prop>\
                         <cal:calendar-home-set><d:href>/cals/</d:href></cal:calendar-home-set>\
                         </d:prop></d:propstat></d:response>")
                    }
                    ("PROPFIND", "/cals") => ms(
                        "<d:response><d:href>/cals/mine/</d:href><d:propstat><d:prop>\
                         <d:displayname>Mine</d:displayname>\
                         <d:resourcetype><d:collection/><cal:calendar/></d:resourcetype>\
                         <cs:getctag>m1</cs:getctag></d:prop></d:propstat></d:response>\
                         <d:response><d:href>/cals/colleague/</d:href><d:propstat><d:prop>\
                         <d:displayname>Colleague (renamed)</d:displayname>\
                         <d:resourcetype><d:collection/><cal:calendar/></d:resourcetype>\
                         <cs:getctag>c1</cs:getctag></d:prop></d:propstat></d:response>",
                    ),
                    ("REPORT", "/cals/mine") => ms(
                        "<d:response><d:href>/cals/mine/e1.ics</d:href><d:propstat><d:prop>\
                         <cal:calendar-data>BEGIN:VCALENDAR\r\nBEGIN:VEVENT\r\nUID:e1\r\n\
                         DTSTART:20300101T100000Z\r\nDTEND:20300101T110000Z\r\nSUMMARY:Mine\r\n\
                         END:VEVENT\r\nEND:VCALENDAR\r\n</cal:calendar-data>\
                         </d:prop></d:propstat></d:response><d:sync-token>s1</d:sync-token>",
                    ),
                    // An empty calendar is an empty multistatus: an empty
                    // 404 is an error since #325.
                    ("REPORT", "/cals/colleague") => ms("<d:sync-token>c1</d:sync-token>"),
                    _ => return (StatusCode::NOT_FOUND, String::new()),
                };
                (StatusCode::MULTI_STATUS, body)
            }
        });
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });
        (format!("http://127.0.0.1:{port}/dav/"), requests)
    }

    async fn calendar_event_count(pool: &SqlitePool, source_id: &str, href: &str) -> i64 {
        sqlx::query_scalar(
            "SELECT COUNT(*) FROM events e JOIN calendars c ON c.id = e.calendar_id
             WHERE c.source_id = ? AND c.href = ?",
        )
        .bind(source_id)
        .bind(href)
        .fetch_one(pool)
        .await
        .unwrap()
    }

    /// A CalDAV server with one calendar, `/cals/mine/`, holding one event.
    /// Once `broken` holds an answer, every REPORT gets it (status, content
    /// type, body) and the listing advertises a new ctag, as a server whose
    /// calendar changed but whose REPORTs fail would.
    #[allow(clippy::type_complexity)]
    async fn spawn_breakable_dav() -> (
        String,
        Arc<std::sync::Mutex<Option<(u16, &'static str, &'static str)>>>,
    ) {
        use axum::http::{header, Method, StatusCode, Uri};
        let broken: Arc<std::sync::Mutex<Option<(u16, &'static str, &'static str)>>> =
            Arc::default();
        let state = broken.clone();
        let app = axum::Router::new().fallback(move |method: Method, uri: Uri| {
            let failure = *state.lock().unwrap();
            async move {
                let ms = |inner: &str| {
                    format!(
                        "<?xml version=\"1.0\"?><d:multistatus xmlns:d=\"DAV:\" \
                         xmlns:cal=\"urn:ietf:params:xml:ns:caldav\" \
                         xmlns:cs=\"http://calendarserver.org/ns/\">{inner}</d:multistatus>"
                    )
                };
                let ctag = if failure.is_some() { "m2" } else { "m1" };
                let reply = |status: u16, content_type: &'static str, body: String| {
                    (
                        StatusCode::from_u16(status).unwrap(),
                        [(header::CONTENT_TYPE, content_type)],
                        body,
                    )
                };
                match (method.as_str(), uri.path().trim_end_matches('/')) {
                    ("PROPFIND", "/dav") => reply(207, "application/xml", ms(
                        "<d:response><d:href>/dav/</d:href><d:propstat><d:prop>\
                         <d:current-user-principal><d:href>/p/</d:href></d:current-user-principal>\
                         </d:prop></d:propstat></d:response>",
                    )),
                    ("PROPFIND", "/p") => reply(207, "application/xml", ms(
                        "<d:response><d:href>/p/</d:href><d:propstat><d:prop>\
                         <cal:calendar-home-set><d:href>/cals/</d:href></cal:calendar-home-set>\
                         </d:prop></d:propstat></d:response>",
                    )),
                    ("PROPFIND", "/cals") => reply(207, "application/xml", ms(&format!(
                        "<d:response><d:href>/cals/mine/</d:href><d:propstat><d:prop>\
                         <d:displayname>Mine</d:displayname>\
                         <d:resourcetype><d:collection/><cal:calendar/></d:resourcetype>\
                         <cs:getctag>{ctag}</cs:getctag></d:prop></d:propstat></d:response>"
                    ))),
                    ("REPORT", "/cals/mine") => match failure {
                        Some((status, content_type, body)) => {
                            reply(status, content_type, body.to_string())
                        }
                        None => reply(207, "application/xml", ms(
                            "<d:response><d:href>/cals/mine/e1.ics</d:href><d:propstat><d:prop>\
                             <cal:calendar-data>BEGIN:VCALENDAR\r\nBEGIN:VEVENT\r\nUID:e1\r\n\
                             DTSTART:20300101T100000Z\r\nDTEND:20300101T110000Z\r\nSUMMARY:Mine\r\n\
                             END:VEVENT\r\nEND:VCALENDAR\r\n</cal:calendar-data>\
                             </d:prop></d:propstat></d:response><d:sync-token>s1</d:sync-token>",
                        )),
                    },
                    _ => reply(404, "text/plain", String::new()),
                }
            }
        });
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });
        (format!("http://127.0.0.1:{port}/dav/"), broken)
    }

    /// #325: a failing REPORT (an empty 500, an HTML 503, a 403, or a 200
    /// with no multistatus) fails the calendar's sync. The cached events stay
    /// and the new ctag is not stored, so the next sync fetches again
    /// instead of reading the calendar as unchanged.
    #[tokio::test]
    async fn caldav_sync_keeps_cache_and_ctag_when_a_report_fails() {
        let pool = setup_test_db().await;
        let (source_id, _) = seed_fixtures(&pool).await;
        let key = [0u8; 32];
        let (base, broken) = spawn_breakable_dav().await;
        let client = CaldavClient::new(&base, "user", "pw");

        sync_source(&pool, &key, &client, &source_id).await.unwrap();
        assert_eq!(
            calendar_event_count(&pool, &source_id, "/cals/mine/").await,
            1
        );

        for failure in [
            (500, "text/plain", ""),
            (
                503,
                "text/html",
                "<html><body><h1>Service Unavailable</h1></body></html>",
            ),
            (403, "application/xml", ""),
            (200, "text/html", ""),
        ] {
            *broken.lock().unwrap() = Some(failure);
            let err = sync_source(&pool, &key, &client, &source_id)
                .await
                .unwrap_err()
                .to_string();
            assert!(err.contains("Mine"), "{failure:?}: {err}");
            assert_eq!(
                calendar_event_count(&pool, &source_id, "/cals/mine/").await,
                1,
                "{failure:?}"
            );
            let ctag: Option<String> = sqlx::query_scalar(
                "SELECT ctag FROM calendars WHERE source_id = ? AND href = '/cals/mine/'",
            )
            .bind(&source_id)
            .fetch_one(&pool)
            .await
            .unwrap();
            assert_eq!(ctag.as_deref(), Some("m1"), "{failure:?}");
        }
    }

    /// A CalDAV server whose one calendar advertises `ctag`, and whose
    /// sync-collection answers depend on the token sent: from `s1` it
    /// reports one change and truncates (a 507, RFC 6578 section 3.6) at
    /// `s2`; from `s2` it reports the rest and ends at `s3`.
    async fn spawn_truncating_dav() -> (String, Arc<std::sync::Mutex<&'static str>>) {
        use axum::http::{Method, StatusCode, Uri};
        let ctag: Arc<std::sync::Mutex<&'static str>> = Arc::new(std::sync::Mutex::new("m1"));
        let state = ctag.clone();
        let app = axum::Router::new().fallback(move |method: Method, uri: Uri, body: String| {
            let ctag = *state.lock().unwrap();
            async move {
                let ms = |inner: &str| {
                    format!(
                        "<?xml version=\"1.0\"?><d:multistatus xmlns:d=\"DAV:\" \
                         xmlns:cal=\"urn:ietf:params:xml:ns:caldav\" \
                         xmlns:cs=\"http://calendarserver.org/ns/\">{inner}</d:multistatus>"
                    )
                };
                let event = |uid: &str| {
                    format!(
                        "<d:response><d:href>/cals/mine/{uid}.ics</d:href><d:propstat><d:prop>\
                         <cal:calendar-data>BEGIN:VCALENDAR\r\nBEGIN:VEVENT\r\nUID:{uid}\r\n\
                         DTSTART:20300101T100000Z\r\nDTEND:20300101T110000Z\r\nSUMMARY:{uid}\r\n\
                         END:VEVENT\r\nEND:VCALENDAR\r\n</cal:calendar-data>\
                         </d:prop><d:status>HTTP/1.1 200 OK</d:status></d:propstat></d:response>"
                    )
                };
                let reply = match (method.as_str(), uri.path().trim_end_matches('/')) {
                    ("PROPFIND", "/dav") => {
                        ms("<d:response><d:href>/dav/</d:href><d:propstat><d:prop>\
                         <d:current-user-principal><d:href>/p/</d:href></d:current-user-principal>\
                         </d:prop></d:propstat></d:response>")
                    }
                    ("PROPFIND", "/p") => {
                        ms("<d:response><d:href>/p/</d:href><d:propstat><d:prop>\
                         <cal:calendar-home-set><d:href>/cals/</d:href></cal:calendar-home-set>\
                         </d:prop></d:propstat></d:response>")
                    }
                    ("PROPFIND", "/cals") => ms(&format!(
                        "<d:response><d:href>/cals/mine/</d:href><d:propstat><d:prop>\
                         <d:displayname>Mine</d:displayname>\
                         <d:resourcetype><d:collection/><cal:calendar/></d:resourcetype>\
                         <cs:getctag>{ctag}</cs:getctag></d:prop></d:propstat></d:response>"
                    )),
                    ("REPORT", "/cals/mine") if body.contains("sync-collection") => {
                        if body.contains("<d:sync-token>s1</d:sync-token>") {
                            ms(&format!(
                                "{}<d:response><d:href>/cals/mine/</d:href>\
                                 <d:status>HTTP/1.1 507 Insufficient Storage</d:status>\
                                 </d:response><d:sync-token>s2</d:sync-token>",
                                event("e2")
                            ))
                        } else if body.contains("<d:sync-token>s2</d:sync-token>") {
                            ms(&format!("{}<d:sync-token>s3</d:sync-token>", event("e3")))
                        } else {
                            ms("<d:sync-token>s1</d:sync-token>")
                        }
                    }
                    ("REPORT", "/cals/mine") => ms(&event("e1")),
                    _ => return (StatusCode::NOT_FOUND, String::new()),
                };
                (StatusCode::MULTI_STATUS, reply)
            }
        });
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });
        (format!("http://127.0.0.1:{port}/dav/"), ctag)
    }

    /// #325: a truncated sync-collection keeps the old ctag (so the next
    /// sync is not skipped as unchanged) and stores the server's token (so
    /// it continues where the server stopped).
    #[tokio::test]
    async fn caldav_sync_continues_after_a_truncated_delta() {
        let pool = setup_test_db().await;
        let (source_id, _) = seed_fixtures(&pool).await;
        let key = [0u8; 32];
        let (base, ctag) = spawn_truncating_dav().await;
        let client = CaldavClient::new(&base, "user", "pw");
        let state = |pool: SqlitePool, source_id: String| async move {
            sqlx::query_as::<_, (Option<String>, Option<String>)>(
                "SELECT ctag, sync_token FROM calendars WHERE source_id = ? AND href = '/cals/mine/'",
            )
            .bind(&source_id)
            .fetch_one(&pool)
            .await
            .unwrap()
        };

        // Full sync: e1, ctag m1, the initial token s1.
        sync_source(&pool, &key, &client, &source_id).await.unwrap();
        assert_eq!(
            state(pool.clone(), source_id.clone()).await,
            (Some("m1".into()), Some("s1".into()))
        );

        // The calendar changed: the delta from s1 is truncated at s2.
        *ctag.lock().unwrap() = "m2";
        sync_source(&pool, &key, &client, &source_id).await.unwrap();
        assert_eq!(
            calendar_event_count(&pool, &source_id, "/cals/mine/").await,
            2
        );
        assert_eq!(
            state(pool.clone(), source_id.clone()).await,
            (Some("m1".into()), Some("s2".into())),
            "a truncated delta must not store the new ctag"
        );

        // The next sync is not skipped, continues from s2, and completes.
        sync_source(&pool, &key, &client, &source_id).await.unwrap();
        assert_eq!(
            calendar_event_count(&pool, &source_id, "/cals/mine/").await,
            3
        );
        assert_eq!(
            state(pool.clone(), source_id.clone()).await,
            (Some("m2".into()), Some("s3".into()))
        );
    }

    #[tokio::test]
    async fn caldav_sync_never_fetches_a_calendar_that_is_not_synced() {
        let pool = setup_test_db().await;
        let (source_id, _) = seed_fixtures(&pool).await;
        let key = [0u8; 32];
        let (base, requests) = spawn_two_calendar_dav().await;
        let client = CaldavClient::new(&base, "user", "pw");

        // First sync: both calendars are discovered, synced by default.
        sync_source(&pool, &key, &client, &source_id).await.unwrap();
        assert!(requests
            .lock()
            .unwrap()
            .iter()
            .any(|r| r.starts_with("REPORT /cals/colleague")));
        assert_eq!(
            calendar_event_count(&pool, &source_id, "/cals/mine/").await,
            1
        );

        let colleague: String = sqlx::query_scalar(
            "SELECT id FROM calendars WHERE source_id = ? AND href = '/cals/colleague/'",
        )
        .bind(&source_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(
            set_calendar_sync_enabled(&pool, &source_id, &colleague, false)
                .await
                .unwrap()
        );
        // A row a sync in flight wrote after it was turned off.
        sqlx::query("INSERT INTO events (id, calendar_id, uid, summary, start_at, end_at) VALUES ('late', ?, 'late', 'Late', '20300101T100000', '20300101T110000')")
            .bind(&colleague)
            .execute(&pool)
            .await
            .unwrap();

        requests.lock().unwrap().clear();
        sync_source(&pool, &key, &client, &source_id).await.unwrap();
        let seen = requests.lock().unwrap().clone();
        assert!(
            !seen.iter().any(|r| r.contains("/cals/colleague")),
            "the unsynced calendar was requested: {seen:?}"
        );
        assert_eq!(
            calendar_event_count(&pool, &source_id, "/cals/colleague/").await,
            0
        );
        let (enabled, name, ctag): (bool, Option<String>, Option<String>) =
            sqlx::query_as("SELECT sync_enabled, display_name, ctag FROM calendars WHERE id = ?")
                .bind(&colleague)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert!(!enabled, "discovery keeps the owner's choice");
        assert_eq!(
            name.as_deref(),
            Some("Colleague (renamed)"),
            "still listed and refreshed"
        );
        assert!(ctag.is_none());

        // Turned back on: fetched again, in full.
        assert!(
            set_calendar_sync_enabled(&pool, &source_id, &colleague, true)
                .await
                .unwrap()
        );
        requests.lock().unwrap().clear();
        sync_source(&pool, &key, &client, &source_id).await.unwrap();
        assert!(requests
            .lock()
            .unwrap()
            .iter()
            .any(|r| r.starts_with("REPORT /cals/colleague")));
    }

    /// Turning a calendar off drops its out-of-office rows, so it also
    /// forgets their last refresh: turned back on within five minutes, the
    /// next sync reads them again instead of leaving the absences out.
    #[tokio::test]
    async fn turning_a_calendar_off_forgets_its_out_of_office_refresh() {
        let pool = setup_test_db().await;
        let (source_id, _) = seed_fixtures(&pool).await;
        let cal_id = Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO calendars (id, source_id, href) VALUES (?, ?, '/ooo/')")
            .bind(&cal_id)
            .bind(&source_id)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO events (id, calendar_id, uid, start_at, end_at, google_out_of_office) VALUES (?, ?, 'google-out-of-office:a', '20300615T000000Z', '20300616T000000Z', 1)")
            .bind(Uuid::new_v4().to_string())
            .bind(&cal_id)
            .execute(&pool)
            .await
            .unwrap();
        crate::google_calendar::mark_refreshed("https://google.test/api", &cal_id);
        assert!(crate::google_calendar::refreshed_recently(&cal_id));

        assert!(set_calendar_sync_enabled(&pool, &source_id, &cal_id, false)
            .await
            .unwrap());
        let left: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM events WHERE calendar_id = ?")
            .bind(&cal_id)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(left, 0);
        assert!(!crate::google_calendar::refreshed_recently(&cal_id));

        // A refresh recorded while it was off is forgotten when it comes back.
        crate::google_calendar::mark_refreshed("https://google.test/api", &cal_id);
        assert!(set_calendar_sync_enabled(&pool, &source_id, &cal_id, true)
            .await
            .unwrap());
        assert!(!crate::google_calendar::refreshed_recently(&cal_id));
    }

    #[tokio::test]
    async fn set_calendar_sync_enabled_is_scoped_to_the_source() {
        let pool = setup_test_db().await;
        let (source_id, _) = seed_fixtures(&pool).await;
        let (other_source, _) = {
            // A second source, under another account.
            let pool2 = &pool;
            let user_id = Uuid::new_v4().to_string();
            let account_id = Uuid::new_v4().to_string();
            let sid = Uuid::new_v4().to_string();
            sqlx::query("INSERT INTO users (id, email, name, role, auth_provider, username, enabled) VALUES (?, 'b@example.com', 'B', 'user', 'local', 'b', 1)")
                .bind(&user_id).execute(pool2).await.unwrap();
            sqlx::query("INSERT INTO accounts (id, name, email, timezone, user_id) VALUES (?, 'B', 'b@example.com', 'UTC', ?)")
                .bind(&account_id).bind(&user_id).execute(pool2).await.unwrap();
            sqlx::query("INSERT INTO caldav_sources (id, account_id, name, url, username) VALUES (?, ?, 'b', 'https://b.example.com/', 'b')")
                .bind(&sid).bind(&account_id).execute(pool2).await.unwrap();
            (sid, ())
        };
        let cal_id = Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO calendars (id, source_id, href, display_name) VALUES (?, ?, '/b/', 'B')",
        )
        .bind(&cal_id)
        .bind(&other_source)
        .execute(&pool)
        .await
        .unwrap();

        assert!(
            !set_calendar_sync_enabled(&pool, &source_id, &cal_id, false)
                .await
                .unwrap()
        );
        let enabled: bool = sqlx::query_scalar("SELECT sync_enabled FROM calendars WHERE id = ?")
            .bind(&cal_id)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert!(enabled);
    }

    /// A calendar turned off and on again while a sync runs must not keep
    /// the ctag that sync fetched: its events were dropped meanwhile, and a
    /// matching ctag would skip the refetch until the daily full resync.
    /// Turning it on also marks the source stale for the next slot page.
    #[tokio::test]
    async fn toggling_during_a_sync_discards_its_sync_state() {
        let pool = setup_test_db().await;
        let (source_id, _) = seed_fixtures(&pool).await;
        let cal_id = Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO calendars (id, source_id, href, display_name) VALUES (?, ?, '/c/', 'C')",
        )
        .bind(&cal_id)
        .bind(&source_id)
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query("UPDATE caldav_sources SET last_synced = datetime('now') WHERE id = ?")
            .bind(&source_id)
            .execute(&pool)
            .await
            .unwrap();

        // The sync starts and reads the generation...
        let generation = calendar_generation(&pool, &cal_id).await;
        // ...the owner turns the calendar off, then on again...
        set_calendar_sync_enabled(&pool, &source_id, &cal_id, false)
            .await
            .unwrap();
        set_calendar_sync_enabled(&pool, &source_id, &cal_id, true)
            .await
            .unwrap();
        // ...and the sync finishes.
        let ctag = Some("ctag-from-server".to_string());
        let token = Some("token-from-server".to_string());
        update_calendar_sync_state(&pool, &cal_id, generation, &ctag, &token).await;

        let (stored_ctag, stored_token): (Option<String>, Option<String>) =
            sqlx::query_as("SELECT ctag, sync_token FROM calendars WHERE id = ?")
                .bind(&cal_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert!(stored_ctag.is_none() && stored_token.is_none());
        let last_synced: Option<String> =
            sqlx::query_scalar("SELECT last_synced FROM caldav_sources WHERE id = ?")
                .bind(&source_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert!(
            last_synced.is_none(),
            "turning a calendar on marks the source stale"
        );

        // A sync that started after the change stores its state as usual.
        let generation = calendar_generation(&pool, &cal_id).await;
        update_calendar_sync_state(&pool, &cal_id, generation, &ctag, &token).await;
        let stored: Option<String> = sqlx::query_scalar("SELECT ctag FROM calendars WHERE id = ?")
            .bind(&cal_id)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(stored.as_deref(), Some("ctag-from-server"));
    }

    /// A write calendar that is not synced holds no cached events: the
    /// orphan sweep must not read that as every booking being deleted.
    #[tokio::test]
    async fn orphan_sweep_skips_a_write_calendar_that_is_not_synced() {
        let pool = setup_test_db().await;
        let (source_id, et_id) = seed_fixtures(&pool).await;
        let key = [0u8; 32];
        sqlx::query("INSERT INTO calendars (id, source_id, href, display_name, sync_enabled) VALUES (?, ?, '/calendars/host/default/', 'Default', 0)")
            .bind(Uuid::new_v4().to_string())
            .bind(&source_id)
            .execute(&pool)
            .await
            .unwrap();
        let booking_id = Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO bookings (id, event_type_id, uid, guest_name, guest_email, guest_timezone,
                start_at, end_at, status, cancel_token, reschedule_token, caldav_calendar_href)
             VALUES (?, ?, 'unsynced-uid', 'Guest', 'guest@example.com', 'UTC',
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
        assert_eq!(status, "confirmed");
    }

    /// An EWS provider with two folders that records which ones it fetched.
    struct TwoFolderProvider(std::sync::Mutex<Vec<String>>);

    #[async_trait::async_trait]
    impl crate::providers::CalendarProvider for TwoFolderProvider {
        async fn check_connection(&self) -> Result<bool> {
            Ok(true)
        }
        async fn list_calendars(&self) -> Result<Vec<crate::providers::RemoteCalendar>> {
            Ok(["mine", "colleague"]
                .iter()
                .map(|id| crate::providers::RemoteCalendar {
                    id: id.to_string(),
                    display_name: Some(id.to_string()),
                    color: None,
                    change_marker: None,
                    sync_state: None,
                    read_only: false,
                    shared: false,
                })
                .collect())
        }
        async fn fetch_events(&self, _: &str) -> Result<Vec<ProviderRawEvent>> {
            Ok(vec![])
        }
        async fn fetch_events_since(&self, id: &str, _: &str) -> Result<Vec<ProviderRawEvent>> {
            self.0.lock().unwrap().push(id.to_string());
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
            Ok(true)
        }
    }

    #[tokio::test]
    async fn ews_sync_never_fetches_a_folder_that_is_not_synced() {
        let pool = setup_test_db().await;
        let (source_id, _) = seed_fixtures(&pool).await;
        let key = [0u8; 32];
        sqlx::query("INSERT INTO calendars (id, source_id, href, display_name, sync_enabled) VALUES (?, ?, 'colleague', 'colleague', 0)")
            .bind(Uuid::new_v4().to_string())
            .bind(&source_id)
            .execute(&pool)
            .await
            .unwrap();
        let provider = TwoFolderProvider(Default::default());

        sync_provider_source(&pool, &key, &provider, &source_id)
            .await
            .unwrap();

        assert_eq!(*provider.0.lock().unwrap(), vec!["mine".to_string()]);
    }
}
