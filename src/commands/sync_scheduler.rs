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
//!   while cloud providers that throttle per user (Microsoft Graph, Google)
//!   count each source on its own,
//! - never a source another sync is already running (the per-source lock
//!   on-demand syncs and the dashboard's "Sync" button take too),
//! - each sync stopped after [`SOURCE_TIMEOUT`], recorded as a failure.
//!
//! Syncs run across ticks: a slow source holds its own slot and its server,
//! and the other slots go on with the next due sources on other servers, so
//! one slow server never holds up the rest or the next tick.
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

use super::sync_health;

use super::sync::{self, DueSource};

/// How often the background sync wakes up.
pub const TICK: Duration = Duration::from_secs(60);

/// How long one background sync of a source may take. A sync still running
/// then is stopped and recorded as a failure of the source; each request
/// has its own, shorter timeout.
pub const SOURCE_TIMEOUT: Duration = Duration::from_secs(600);

/// How long a crashed background sync waits before it starts again.
pub const RESTART_DELAY: Duration = Duration::from_secs(30);

/// Ticks over which every source should come round once: the quota per tick
/// is the number of sources divided by this, so each one is synced about
/// once an hour (less often when `CALRS_SYNC_MIN_AGE` says so).
pub const CYCLE_TICKS: i64 = 60;

/// Syncs at once when `CALRS_SYNC_CONCURRENCY` is unset.
pub const DEFAULT_CONCURRENCY: usize = 4;

/// Upper bound for `CALRS_SYNC_CONCURRENCY`. A sync holds a database
/// connection only for each query, never across a request to the calendar
/// server, so more syncs than the pool's five connections can run; past
/// eight they would mostly queue for a connection, behind web requests.
pub const MAX_CONCURRENCY: usize = 8;

/// What the background sync runs with, read once when it starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SchedulerConfig {
    /// `CALRS_SYNC_MIN_AGE`, in seconds (#310).
    pub min_age_secs: u64,
    /// `CALRS_SYNC_CONCURRENCY`.
    pub concurrency: usize,
    /// [`SOURCE_TIMEOUT`], shorter in tests.
    pub source_timeout: Duration,
}

impl SchedulerConfig {
    pub fn from_env() -> Self {
        SchedulerConfig {
            min_age_secs: resolve_sync_min_age(crate::settings::sync_min_age_secs()),
            concurrency: resolve_concurrency(crate::settings::sync_concurrency()),
            source_timeout: SOURCE_TIMEOUT,
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
    /// Stopped after the source timeout, and recorded as a failure.
    TimedOut,
    /// Another sync of it was running: left for a later tick.
    Busy,
    /// Synced or removed by someone else since it was picked.
    AlreadyDone,
}

/// What a tick started and how those syncs ended, so far.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct TickReport {
    /// Sources synced, in the order they finished.
    pub synced: Vec<String>,
    pub timed_out: usize,
    pub busy: usize,
    pub already_done: usize,
}

/// Sync one picked source unless another sync holds it or has run it since
/// it was picked, for at most `limit`. The lock is held for the whole sync,
/// so no other sync of the source runs meanwhile.
pub async fn attempt(
    pool: SqlitePool,
    key: [u8; 32],
    source: DueSource,
    sync_fn: SyncFn,
    limit: Duration,
) -> Attempt {
    let lock = sync::source_lock(&source.id).await;
    let Ok(_guard) = lock.try_lock_owned() else {
        return Attempt::Busy;
    };
    let current = sync::source_last_attempt(&pool, &source.id).await;
    if current.as_deref() != Some(source.last_attempt.as_str()) {
        return Attempt::AlreadyDone;
    }
    if tokio::time::timeout(limit, sync_fn(source.id.clone()))
        .await
        .is_ok()
    {
        return Attempt::Synced;
    }
    let error = anyhow::anyhow!(
        "the sync took longer than {} seconds and was stopped",
        limit.as_secs()
    );
    sync_health::record_sync_failure(&pool, &key, &source.id, &error).await;
    Attempt::TimedOut
}

/// A running sync: the source, its server, and the tick that started it.
struct Running {
    source_id: String,
    origin: String,
    tick: u64,
}

/// The background sync's state: the syncs running, and the sources due in
/// the current tick that are still to start.
pub struct Scheduler {
    pool: SqlitePool,
    key: [u8; 32],
    cfg: SchedulerConfig,
    sync_fn: SyncFn,
    tasks: JoinSet<Attempt>,
    running: HashMap<tokio::task::Id, Running>,
    busy_origins: HashSet<String>,
    running_sources: HashSet<String>,
    queue: VecDeque<DueSource>,
    /// Syncs the current tick may still start.
    left: usize,
    tick: u64,
    /// The current tick's report.
    pub report: TickReport,
}

impl Scheduler {
    pub fn new(pool: SqlitePool, key: [u8; 32], cfg: SchedulerConfig, sync_fn: SyncFn) -> Self {
        Scheduler {
            pool,
            key,
            cfg,
            sync_fn,
            tasks: JoinSet::new(),
            running: HashMap::new(),
            busy_origins: HashSet::new(),
            running_sources: HashSet::new(),
            queue: VecDeque::new(),
            left: 0,
            tick: 0,
            report: TickReport::default(),
        }
    }

    /// Start a tick at `now`: every due source, in order, of which `quota`
    /// may be started before the next tick. Syncs still running from earlier
    /// ticks carry on, keeping their slots.
    pub async fn begin_tick(&mut self, now: DateTime<Utc>, quota: usize) {
        self.queue =
            sync::due_background_sources(&self.pool, now, self.cfg.min_age_secs, usize::MAX)
                .await
                .into();
        self.left = quota;
        self.tick += 1;
        self.report = TickReport::default();
        self.dispatch();
    }

    /// Syncs running now.
    pub fn running(&self) -> usize {
        self.tasks.len()
    }

    /// Fill the free slots with the first due sources whose server is free
    /// and that are not running already, so a busy or slow server never
    /// keeps the others waiting.
    fn dispatch(&mut self) {
        while self.tasks.len() < self.cfg.concurrency.max(1) && self.left > 0 {
            let Some(next) = self.queue.iter().position(|s| {
                !self.busy_origins.contains(&s.origin) && !self.running_sources.contains(&s.id)
            }) else {
                break;
            };
            let Some(source) = self.queue.remove(next) else {
                break;
            };
            self.left -= 1;
            self.busy_origins.insert(source.origin.clone());
            self.running_sources.insert(source.id.clone());
            let running = Running {
                source_id: source.id.clone(),
                origin: source.origin.clone(),
                tick: self.tick,
            };
            let handle = self.tasks.spawn(attempt(
                self.pool.clone(),
                self.key,
                source,
                self.sync_fn.clone(),
                self.cfg.source_timeout,
            ));
            self.running.insert(handle.id(), running);
        }
    }

    /// Wait for a running sync to end, then start the next ones. Never
    /// resolves while nothing runs, so it can sit in a `select!` with the
    /// tick timer.
    pub async fn next_done(&mut self) {
        if self.tasks.is_empty() {
            std::future::pending::<()>().await;
        }
        let Some(joined) = self.tasks.join_next_with_id().await else {
            return;
        };
        let task_id = match &joined {
            Ok((id, _)) => *id,
            Err(e) => e.id(),
        };
        if let Some(done) = self.running.remove(&task_id) {
            self.busy_origins.remove(&done.origin);
            self.running_sources.remove(&done.source_id);
            let this_tick = done.tick == self.tick;
            match joined {
                Ok((_, Attempt::Synced)) => self.report.synced.push(done.source_id),
                Ok((_, Attempt::TimedOut)) => self.report.timed_out += 1,
                Ok((_, outcome)) => {
                    if outcome == Attempt::Busy {
                        self.report.busy += 1;
                    } else {
                        self.report.already_done += 1;
                    }
                    // Nothing was synced: the slot goes to the next source.
                    if this_tick {
                        self.left += 1;
                    }
                }
                Err(e) => tracing::warn!(
                    source_id = %done.source_id,
                    error = %e,
                    "background sync task failed"
                ),
            }
        }
        self.dispatch();
    }

    /// Wait until no sync runs and the tick can start no other.
    #[cfg(test)]
    pub async fn settle(&mut self) {
        while !self.tasks.is_empty() {
            self.next_done().await;
        }
    }
}

/// One tick, run to the end: start it at `now` and wait for every sync it
/// starts. The background task never waits like this.
#[cfg(test)]
pub async fn run_tick(
    pool: &SqlitePool,
    cfg: &SchedulerConfig,
    now: DateTime<Utc>,
    quota: usize,
    sync_fn: SyncFn,
) -> TickReport {
    let mut scheduler = Scheduler::new(pool.clone(), [0u8; 32], *cfg, sync_fn);
    scheduler.begin_tick(now, quota).await;
    scheduler.settle().await;
    scheduler.report
}

/// The background sync task of `calrs serve`: [`background_sync`], started
/// again after [`RESTART_DELAY`] if it ever panics.
pub async fn run_background_sync_loop(pool: SqlitePool, secret_key: [u8; 32]) {
    supervise(RESTART_DELAY, move || {
        background_sync(pool.clone(), secret_key)
    })
    .await;
}

/// Run the task `make` builds; when it panics, log it and start a new one
/// after `delay`. Returns when a task ends normally.
pub async fn supervise<F, Fut>(delay: Duration, mut make: F)
where
    F: FnMut() -> Fut,
    Fut: Future<Output = ()> + Send + 'static,
{
    loop {
        match tokio::spawn(make()).await {
            Ok(()) => return,
            Err(e) => {
                tracing::error!(
                    error = %e,
                    delay_secs = delay.as_secs(),
                    "background calendar sync stopped, starting it again"
                );
                tokio::time::sleep(delay).await;
            }
        }
    }
}

async fn background_sync(pool: SqlitePool, secret_key: [u8; 32]) {
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
    let mut scheduler = Scheduler::new(pool.clone(), secret_key, cfg, sync_fn);
    let mut ticks = tokio::time::interval_at(Instant::now() + TICK, TICK);
    ticks.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        tokio::select! {
            _ = ticks.tick() => {
                let report = &scheduler.report;
                if !report.synced.is_empty() || report.timed_out > 0 {
                    tracing::debug!(
                        synced = report.synced.len(),
                        timed_out = report.timed_out,
                        busy = report.busy,
                        already_done = report.already_done,
                        "background sync tick"
                    );
                }
                let sources = sync::background_source_count(&pool).await;
                scheduler.begin_tick(Utc::now(), tick_quota(sources)).await;
                tracing::debug!(
                    sources,
                    running = scheduler.running(),
                    "background sync tick started"
                );
            }
            _ = scheduler.next_done() => {}
        }
    }
}

#[cfg(test)]
mod tests;
