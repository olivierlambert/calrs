//! Background calendar sync (#388).
//!
//! `calrs serve` keeps every enabled source's cache fresh with a task of its
//! own, separate from the reminder loop, so reminders and the pending sweep
//! never wait on a slow calendar server. Once a minute it asks
//! [`sync::due_background_sources`] for the sources due a sync (daily full
//! resyncs first, then the stalest), as many as needed for every source to
//! come round about once an hour ([`tick_quota`]), and syncs them:
//!
//! - up to `CALRS_SYNC_CONCURRENCY` at once (default
//!   [`DEFAULT_CONCURRENCY`]),
//! - never two at once against the same server ([`DueSource::origin`]): an
//!   Exchange endpoint serving every managed source sees one sync at a time,
//! - never a source another sync is already running (the per-source lock
//!   on-demand syncs and the dashboard's "Sync" button take too),
//! - starting no new sync once [`TICK_BUDGET`] has passed, so a slow server
//!   delays the next tick rather than piling up.
//!
//! An instance with up to 60 sources syncs one per minute, as before.

use std::collections::{HashMap, HashSet, VecDeque};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use sqlx::SqlitePool;
use tokio::task::JoinSet;
use tokio::time::Instant;

use super::sync::{self, DueSource};

/// How often the background sync wakes up.
pub const TICK: Duration = Duration::from_secs(60);

/// How long into a tick new syncs may still be started. Syncs already
/// running are waited for.
pub const TICK_BUDGET: Duration = Duration::from_secs(50);

/// Ticks over which every source should come round once: the quota per tick
/// is the number of sources divided by this, so each one is synced about
/// once an hour (less often when `CALRS_SYNC_MIN_AGE` says so).
pub const CYCLE_TICKS: i64 = 60;

/// Syncs at once when `CALRS_SYNC_CONCURRENCY` is unset.
pub const DEFAULT_CONCURRENCY: usize = 4;

/// Upper bound for `CALRS_SYNC_CONCURRENCY`: the SQLite pool has five
/// connections, shared with web requests.
pub const MAX_CONCURRENCY: usize = 16;

/// What the background sync runs with, read once when it starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SchedulerConfig {
    /// `CALRS_SYNC_MIN_AGE`, in seconds (#310).
    pub min_age_secs: u64,
    /// `CALRS_SYNC_CONCURRENCY`.
    pub concurrency: usize,
    /// [`TICK_BUDGET`], shorter in tests.
    pub budget: Duration,
}

impl SchedulerConfig {
    pub fn from_env() -> Self {
        SchedulerConfig {
            min_age_secs: resolve_sync_min_age(crate::settings::sync_min_age_secs()),
            concurrency: resolve_concurrency(crate::settings::sync_concurrency()),
            budget: TICK_BUDGET,
        }
    }
}

/// An unusable value falls back to 0 (no minimum age) and one above a day is
/// capped, both with a warning, so a typo never stops calendars syncing and a
/// failing source is still retried, and its owner alerted, within days
/// rather than never.
pub fn resolve_sync_min_age(parsed: Result<u64, String>) -> u64 {
    use crate::settings::SYNC_MIN_AGE_MAX_SECS;
    match parsed {
        Ok(0) => 0,
        Ok(secs) if secs > SYNC_MIN_AGE_MAX_SECS => {
            tracing::warn!(
                requested = secs,
                used = SYNC_MIN_AGE_MAX_SECS,
                "CALRS_SYNC_MIN_AGE capped at one day"
            );
            SYNC_MIN_AGE_MAX_SECS
        }
        Ok(secs) => {
            tracing::info!(seconds = secs, "background calendar sync minimum age");
            secs
        }
        Err(e) => {
            tracing::warn!(error = %e, "ignoring CALRS_SYNC_MIN_AGE, no minimum age");
            0
        }
    }
}

/// Unset means [`DEFAULT_CONCURRENCY`]; an unusable value falls back to it
/// and one above [`MAX_CONCURRENCY`] is capped, both with a warning.
pub fn resolve_concurrency(parsed: Result<Option<usize>, String>) -> usize {
    match parsed {
        Ok(None) => DEFAULT_CONCURRENCY,
        Ok(Some(n)) if n > MAX_CONCURRENCY => {
            tracing::warn!(
                requested = n,
                used = MAX_CONCURRENCY,
                "CALRS_SYNC_CONCURRENCY capped"
            );
            MAX_CONCURRENCY
        }
        Ok(Some(n)) => {
            tracing::info!(concurrency = n, "background calendar sync concurrency");
            n
        }
        Err(e) => {
            tracing::warn!(
                error = %e,
                used = DEFAULT_CONCURRENCY,
                "ignoring CALRS_SYNC_CONCURRENCY"
            );
            DEFAULT_CONCURRENCY
        }
    }
}

/// How many sources one tick syncs at most: enough for each of `sources` to
/// come round once every [`CYCLE_TICKS`], and at least one, which is what the
/// background loop did before #388.
pub fn tick_quota(sources: i64) -> usize {
    let per_tick = (sources.max(0) + CYCLE_TICKS - 1) / CYCLE_TICKS;
    usize::try_from(per_tick.max(1)).unwrap_or(usize::MAX)
}

/// Syncs one source, given its id. In production
/// [`sync::sync_source_by_id_locked`]; the caller holds the source's lock.
pub type SyncFn = Arc<dyn Fn(String) -> Pin<Box<dyn Future<Output = ()> + Send>> + Send + Sync>;

/// What happened to one source picked by a tick.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Attempt {
    Synced,
    /// Another sync of it was running: left for a later tick.
    Busy,
    /// Synced or removed by someone else since it was picked.
    AlreadyDone,
}

/// The outcome of one tick.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct TickReport {
    /// Sources synced, in the order they were started.
    pub synced: Vec<String>,
    pub busy: usize,
    pub already_done: usize,
    /// Picked, but not started before the budget ran out.
    pub not_reached: usize,
}

/// Sync one picked source unless another sync holds it or has run it since
/// it was picked. The lock is held for the whole sync, so an on-demand sync
/// of the same source waits for it and then finds it fresh.
pub async fn attempt(pool: SqlitePool, source: DueSource, sync_fn: SyncFn) -> Attempt {
    let lock = sync::source_lock(&source.id).await;
    let Ok(_guard) = lock.try_lock_owned() else {
        return Attempt::Busy;
    };
    let current = sync::source_last_attempt(&pool, &source.id).await;
    if current.as_deref() != Some(source.last_attempt.as_str()) {
        return Attempt::AlreadyDone;
    }
    sync_fn(source.id).await;
    Attempt::Synced
}

/// One tick: pick up to `quota` due sources at `now` and sync them under the
/// limits of `cfg`, one per server at a time, in the order picked. Returns
/// once every sync it started has finished.
pub async fn run_tick(
    pool: &SqlitePool,
    cfg: &SchedulerConfig,
    now: DateTime<Utc>,
    quota: usize,
    sync_fn: SyncFn,
) -> TickReport {
    let due = sync::due_background_sources(pool, now, cfg.min_age_secs, quota).await;
    let deadline = Instant::now() + cfg.budget;
    let concurrency = cfg.concurrency.max(1);
    let mut pending: VecDeque<DueSource> = due.into();
    let mut busy_origins: HashSet<String> = HashSet::new();
    let mut running: HashMap<tokio::task::Id, (String, String)> = HashMap::new();
    let mut tasks = JoinSet::new();
    let mut report = TickReport::default();

    loop {
        while tasks.len() < concurrency && Instant::now() < deadline {
            let Some(next) = pending
                .iter()
                .position(|s| !busy_origins.contains(&s.origin))
            else {
                break;
            };
            let Some(source) = pending.remove(next) else {
                break;
            };
            busy_origins.insert(source.origin.clone());
            let ids = (source.id.clone(), source.origin.clone());
            let handle = tasks.spawn(attempt(pool.clone(), source, sync_fn.clone()));
            running.insert(handle.id(), ids);
        }
        let Some(joined) = tasks.join_next_with_id().await else {
            break;
        };
        let task_id = match &joined {
            Ok((id, _)) => *id,
            Err(e) => e.id(),
        };
        let Some((source_id, origin)) = running.remove(&task_id) else {
            continue;
        };
        busy_origins.remove(&origin);
        match joined {
            Ok((_, Attempt::Synced)) => report.synced.push(source_id),
            Ok((_, Attempt::Busy)) => report.busy += 1,
            Ok((_, Attempt::AlreadyDone)) => report.already_done += 1,
            Err(e) => {
                tracing::warn!(source_id = %source_id, error = %e, "background sync task failed")
            }
        }
    }
    report.not_reached = pending.len();
    report
}

/// The background sync task of `calrs serve`.
pub async fn run_background_sync_loop(pool: SqlitePool, secret_key: [u8; 32]) {
    let cfg = SchedulerConfig::from_env();
    let sync_fn: SyncFn = {
        let pool = pool.clone();
        Arc::new(move |source_id: String| {
            let pool = pool.clone();
            Box::pin(async move {
                sync::sync_source_by_id_locked(&pool, &secret_key, &source_id).await;
            })
        })
    };
    let mut ticks = tokio::time::interval_at(Instant::now() + TICK, TICK);
    ticks.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        ticks.tick().await;
        let sources = sync::background_source_count(&pool).await;
        let report = run_tick(
            &pool,
            &cfg,
            Utc::now(),
            tick_quota(sources),
            sync_fn.clone(),
        )
        .await;
        if !report.synced.is_empty() || report.not_reached > 0 {
            tracing::debug!(
                synced = report.synced.len(),
                busy = report.busy,
                already_done = report.already_done,
                not_reached = report.not_reached,
                sources,
                "background sync tick"
            );
        }
    }
}

#[cfg(test)]
mod tests;
