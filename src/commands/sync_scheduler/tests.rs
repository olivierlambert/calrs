use super::*;
use chrono::TimeZone;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use std::str::FromStr;
use std::sync::atomic::{AtomicI64, AtomicUsize, Ordering};
use std::sync::Mutex as StdMutex;

/// An in-memory database with the five connections production has.
async fn setup_pool() -> SqlitePool {
    let pool = SqlitePoolOptions::new()
        .max_connections(5)
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

/// A user and their scheduling account. Returns (user id, account id).
async fn seed_account(pool: &SqlitePool) -> (String, String) {
    let user_id = uuid::Uuid::new_v4().to_string();
    let account_id = uuid::Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO users (id, email, name, role, auth_provider, username, enabled) VALUES (?, 'host@example.com', 'Host', 'admin', 'local', 'host', 1)")
        .bind(&user_id).execute(pool).await.unwrap();
    sqlx::query("INSERT INTO accounts (id, name, email, timezone, user_id) VALUES (?, 'Host', 'host@example.com', 'UTC', ?)")
        .bind(&account_id).bind(&user_id).execute(pool).await.unwrap();
    (user_id, account_id)
}

async fn add_source(pool: &SqlitePool, account_id: &str, url: &str) -> String {
    let id = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO caldav_sources (id, account_id, name, url, username) VALUES (?, ?, 's', ?, 'u')",
    )
    .bind(&id)
    .bind(account_id)
    .bind(url)
    .execute(pool)
    .await
    .unwrap();
    id
}

fn stamp(t: DateTime<Utc>) -> String {
    t.format("%Y-%m-%d %H:%M:%S").to_string()
}

fn config(concurrency: usize) -> SchedulerConfig {
    SchedulerConfig {
        min_age_secs: 0,
        concurrency,
        source_timeout: Duration::from_secs(30),
    }
}

/// A sync that records the source, sets `last_synced` and does nothing else.
fn recording_sync(pool: &SqlitePool, calls: Arc<StdMutex<Vec<String>>>) -> SyncFn {
    let pool = pool.clone();
    Arc::new(move |id: String| {
        let (pool, calls) = (pool.clone(), calls.clone());
        Box::pin(async move {
            sqlx::query("UPDATE caldav_sources SET last_synced = datetime('now') WHERE id = ?")
                .bind(&id)
                .execute(&pool)
                .await
                .unwrap();
            calls.lock().unwrap().push(id);
        })
    })
}

#[test]
fn quota_keeps_one_per_tick_up_to_sixty_sources_and_scales_past_it() {
    assert_eq!(tick_quota(0), 1);
    assert_eq!(tick_quota(1), 1);
    assert_eq!(tick_quota(60), 1);
    assert_eq!(tick_quota(61), 2);
    assert_eq!(tick_quota(1_440), 24);
    assert_eq!(tick_quota(10_000), 167);
    assert_eq!(tick_quota(-3), 1);
}

#[test]
fn settings_fall_back_and_are_capped() {
    use crate::settings::SYNC_MIN_AGE_MAX_SECS;
    assert_eq!(resolve_sync_min_age(Ok(0)), 0);
    assert_eq!(resolve_sync_min_age(Ok(900)), 900);
    assert_eq!(
        resolve_sync_min_age(Ok(SYNC_MIN_AGE_MAX_SECS)),
        SYNC_MIN_AGE_MAX_SECS
    );
    assert_eq!(
        resolve_sync_min_age(Ok(SYNC_MIN_AGE_MAX_SECS + 1)),
        SYNC_MIN_AGE_MAX_SECS
    );
    assert_eq!(resolve_sync_min_age(Err("15m".to_string())), 0);

    assert_eq!(resolve_concurrency(Ok(None)), DEFAULT_CONCURRENCY);
    assert_eq!(resolve_concurrency(Ok(Some(1))), 1);
    assert_eq!(resolve_concurrency(Ok(Some(8))), 8);
    assert_eq!(resolve_concurrency(Ok(Some(500))), MAX_CONCURRENCY);
    assert_eq!(MAX_CONCURRENCY, 8);
    assert_eq!(
        resolve_concurrency(Err("0".to_string())),
        DEFAULT_CONCURRENCY
    );
}

#[test]
fn sync_origin_names_the_server() {
    let a = sync::sync_origin("https://dav.example.com/remote.php/dav");
    assert_eq!(a, "https://dav.example.com");
    assert_eq!(sync::sync_origin(" https://DAV.example.com:443/x "), a);
    assert_ne!(sync::sync_origin("https://dav.example.com:8443/"), a);
    assert_ne!(sync::sync_origin("http://dav.example.com/"), a);
    assert_eq!(sync::sync_origin("Not A URL"), "not a url");
}

/// A managed Exchange source talks to the global endpoint whatever its own
/// row says, so it counts against that server, alongside a user's own
/// source on the same server.
#[tokio::test]
async fn a_managed_source_counts_against_the_global_endpoint() {
    let pool = setup_pool().await;
    let (_, account) = seed_account(&pool).await;
    sqlx::raw_sql(
        "UPDATE auth_config SET ews_global_enabled = 1, \
         ews_global_url = 'https://mail.example.com/EWS/Exchange.asmx', \
         ews_service_username = 'svc', ews_service_password_enc = 'x' WHERE id = 'singleton'; \
         UPDATE users SET email_trusted = 1",
    )
    .execute(&pool)
    .await
    .unwrap();
    let managed = add_source(&pool, &account, "https://old.example.com/EWS/Exchange.asmx").await;
    sqlx::query("UPDATE caldav_sources SET managed = 1, provider_type = 'ews' WHERE id = ?")
        .bind(&managed)
        .execute(&pool)
        .await
        .unwrap();
    let (_, other_account) = {
        let user = uuid::Uuid::new_v4().to_string();
        let account = uuid::Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO users (id, email, name, role, auth_provider, username, enabled) VALUES (?, 'b@example.com', 'B', 'user', 'local', 'b', 1)")
            .bind(&user).execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO accounts (id, name, email, timezone, user_id) VALUES (?, 'B', 'b@example.com', 'UTC', ?)")
            .bind(&account).bind(&user).execute(&pool).await.unwrap();
        (user, account)
    };
    add_source(
        &pool,
        &other_account,
        "https://MAIL.example.com/ews/exchange.asmx",
    )
    .await;
    let due = sync::due_background_sources(&pool, Utc::now(), 0, 10).await;
    assert_eq!(due.len(), 2);
    assert!(due.iter().all(|s| s.origin == "https://mail.example.com"));
}

/// A source due its daily full resync goes first even when an on-demand sync
/// just refreshed it, but jumps the queue only once in a while (a sync can
/// succeed without completing one), and never while it is failing: a failing
/// source keeps the old backoff.
#[tokio::test]
async fn a_daily_full_resync_goes_first_once_and_never_for_a_failing_source() {
    let pool = setup_pool().await;
    let (_, account) = seed_account(&pool).await;
    let busy = add_source(&pool, &account, "https://a.example.com/").await;
    let stale = add_source(&pool, &account, "https://b.example.com/").await;
    let now = Utc::now();
    let set = |sql: String| {
        let pool = pool.clone();
        async move { sqlx::raw_sql(&sql).execute(&pool).await.unwrap() }
    };
    let ids = |due: Vec<DueSource>| due.into_iter().map(|s| s.id).collect::<Vec<_>>();
    set(format!(
        "UPDATE caldav_sources SET last_synced = '{}', last_full_sync = '{}' WHERE id = '{busy}'",
        stamp(now),
        stamp(now - chrono::Duration::hours(25))
    ))
    .await;
    set(format!(
        "UPDATE caldav_sources SET last_synced = '{}', last_full_sync = '{}' WHERE id = '{stale}'",
        stamp(now - chrono::Duration::minutes(30)),
        stamp(now)
    ))
    .await;

    let due = sync::due_background_sources(&pool, now, 0, 10).await;
    assert_eq!(ids(due), vec![busy.clone(), stale.clone()]);
    assert_eq!(
        ids(sync::due_background_sources(&pool, now, 900, 10).await),
        vec![busy.clone(), stale.clone()]
    );

    // Attempted an hour ago without completing: back to its turn by age.
    set(format!(
        "UPDATE caldav_sources SET last_full_attempt_at = '{}' WHERE id = '{busy}'",
        stamp(now - chrono::Duration::hours(1))
    ))
    .await;
    assert_eq!(
        ids(sync::due_background_sources(&pool, now, 0, 10).await),
        vec![stale.clone(), busy.clone()]
    );
    assert_eq!(
        ids(sync::due_background_sources(&pool, now, 900, 10).await),
        vec![stale.clone()]
    );
    // Some hours later it may go first again.
    set(format!(
        "UPDATE caldav_sources SET last_full_attempt_at = '{}' WHERE id = '{busy}'",
        stamp(now - chrono::Duration::hours(sync::FULL_RESYNC_RETRY_HOURS))
    ))
    .await;
    assert_eq!(
        ids(sync::due_background_sources(&pool, now, 0, 10).await),
        vec![busy.clone(), stale.clone()]
    );

    // Failing: ordered by its last attempt and held back by the minimum age,
    // exactly as before #388.
    set(format!(
        "UPDATE caldav_sources SET last_sync_error_at = '{}' WHERE id = '{busy}'",
        stamp(now)
    ))
    .await;
    assert_eq!(
        ids(sync::due_background_sources(&pool, now, 0, 10).await),
        vec![stale.clone(), busy.clone()]
    );
    assert_eq!(
        ids(sync::due_background_sources(&pool, now, 900, 10).await),
        vec![stale.clone()]
    );
    assert_eq!(
        ids(sync::due_background_sources(&pool, now, 0, 1).await),
        vec![stale.clone()],
        "the limit is the tick's quota"
    );
}

/// What `sync_source_by_id_locked` does to a source's timestamps, on a
/// simulated clock: a full resync when one is due, then `last_synced`.
/// Returns whether it was a full one.
fn simulate_sync(state: &mut sync::SourceSyncState, now: DateTime<Utc>) -> bool {
    let at = stamp(now);
    let full = sync::needs_full_resync(state.last_full_sync.as_deref(), now);
    if full {
        state.last_full_sync = Some(at.clone());
        state.last_full_attempt_at = Some(at.clone());
    }
    state.last_synced = Some(at);
    full
}

/// Thousands of sources, a simulated day and a bit, one tick a minute, run
/// through the selection rules themselves (`pick_due_sources`, which the
/// database query feeds): every source is synced about hourly and every
/// source gets its daily full resync on time, including the busy hosts whose
/// booking pages keep refreshing their sources on demand (which the old loop,
/// picking only the stalest source, never fully resynced). Before #388 a
/// source came round every N minutes, so 3,000 sources meant a sync every 50
/// hours.
#[test]
fn thousands_of_sources_each_sync_hourly_and_fully_every_day() {
    const SOURCES: i64 = 3_000;
    const TICKS: i64 = 25 * 60;
    let start = Utc.with_ymd_and_hms(2030, 1, 7, 0, 0, 0).unwrap();
    // Last syncs spread over the past hour, last full syncs over the past
    // day. Every tenth source belongs to a busy host.
    let seeded_sync = |i: i64| start.timestamp() - (i * 37) % 3600;
    let seeded_full = |i: i64| start.timestamp() - (i * 7919) % 86400;
    let at = |secs: i64| Utc.timestamp_opt(secs, 0).unwrap();
    let mut sources: Vec<sync::SourceSyncState> = (0..SOURCES)
        .map(|i| sync::SourceSyncState {
            id: format!("src-{i:05}"),
            sync_url: Some(format!("https://dav{}.example.com/", i % 5)),
            last_synced: Some(stamp(at(seeded_sync(i)))),
            last_full_sync: Some(stamp(at(seeded_full(i)))),
            ..Default::default()
        })
        .collect();
    let mut log: Vec<Vec<(i64, bool)>> = vec![Vec::new(); SOURCES as usize];
    let quota = tick_quota(SOURCES);
    assert_eq!(quota, 50);

    for tick in 0..TICKS {
        let now = start + chrono::Duration::minutes(tick);
        if tick % 5 == 0 {
            // Busy hosts' booking pages: a delta sync every five minutes.
            for source in sources.iter_mut().step_by(10) {
                source.last_synced = Some(stamp(now));
            }
        }
        for due in sync::pick_due_sources(&sources, now, 0, quota) {
            let i: usize = due.id[4..].parse().unwrap();
            let full = simulate_sync(&mut sources[i], now);
            log[i].push((now.timestamp(), full));
        }
    }

    let end = start.timestamp() + TICKS * 60;
    for (i, syncs) in log.iter().enumerate() {
        let i = i as i64;
        // Its full resync was due 24 hours after the seeded one.
        let mut full_due = seeded_full(i) + 86_400;
        for &(at, _) in syncs.iter().filter(|(_, full)| *full) {
            assert!(
                at <= full_due + 120,
                "{i}: full resync {}s late",
                at - full_due
            );
            full_due = at + 86_400;
        }
        // One due in the last two minutes may still be waiting its tick.
        assert!(full_due > end - 120, "{i}: full resync never ran");
        if i % 10 != 0 {
            let mut last = seeded_sync(i);
            for &(at, _) in syncs {
                assert!(at - last <= 62 * 60, "{i}: {}s without a sync", at - last);
                last = at;
            }
            assert!(end - last <= 62 * 60, "{i}: not synced at the end");
        }
    }
}

/// The same rules through the database and `run_tick`, on fewer sources
/// over two simulated hours: each one is synced within the hour, and the
/// full resyncs falling due in that window run on time.
#[tokio::test]
async fn sources_in_the_database_each_sync_hourly_and_fully_on_time() {
    const SOURCES: i64 = 200;
    const TICKS: i64 = 120;
    let pool = setup_pool().await;
    let (_, account) = seed_account(&pool).await;
    let start = Utc.with_ymd_and_hms(2030, 1, 7, 0, 0, 0).unwrap();
    // Every fifth source's last full sync is 22 to 24 hours old, so due
    // within the window; the others' is fresh.
    sqlx::query(
        "WITH RECURSIVE n(i) AS (SELECT 0 UNION ALL SELECT i + 1 FROM n WHERE i < ?3 - 1)
         INSERT INTO caldav_sources (id, account_id, name, url, username, last_synced, last_full_sync)
         SELECT printf('src-%05d', i), ?1, 's', printf('https://dav%d.example.com/', i % 3), 'u',
                datetime(?2, printf('-%d seconds', (i * 37) % 3600)),
                CASE WHEN i % 5 = 0 THEN datetime(?2, printf('-%d seconds', 79300 + (i * 31) % 7000))
                     ELSE ?2 END
         FROM n",
    )
    .bind(&account)
    .bind(stamp(start))
    .bind(SOURCES)
    .execute(&pool)
    .await
    .unwrap();

    let clock = Arc::new(AtomicI64::new(start.timestamp()));
    let log: Arc<StdMutex<HashMap<String, Vec<(i64, bool)>>>> = Arc::default();
    let sync_fn: SyncFn = {
        let (pool, clock, log) = (pool.clone(), clock.clone(), log.clone());
        Arc::new(move |id: String| {
            let (pool, clock, log) = (pool.clone(), clock.clone(), log.clone());
            Box::pin(async move {
                let now = Utc.timestamp_opt(clock.load(Ordering::SeqCst), 0).unwrap();
                let mut state: sync::SourceSyncState = sqlx::query_as(
                    "SELECT id, url AS sync_url, provider_type, auth_type, last_synced, last_sync_error_at, last_full_sync, \
                     last_full_attempt_at FROM caldav_sources WHERE id = ?",
                )
                .bind(&id)
                .fetch_one(&pool)
                .await
                .unwrap();
                let full = simulate_sync(&mut state, now);
                sqlx::query(
                    "UPDATE caldav_sources SET last_synced = ?, last_full_sync = ?, \
                     last_full_attempt_at = ? WHERE id = ?",
                )
                .bind(&state.last_synced)
                .bind(&state.last_full_sync)
                .bind(&state.last_full_attempt_at)
                .bind(&id)
                .execute(&pool)
                .await
                .unwrap();
                log.lock()
                    .unwrap()
                    .entry(id)
                    .or_default()
                    .push((now.timestamp(), full));
            })
        })
    };

    let quota = tick_quota(sync::background_source_count(&pool).await);
    assert_eq!(quota, 4);
    let cfg = config(DEFAULT_CONCURRENCY);
    for tick in 0..TICKS {
        let now = start + chrono::Duration::minutes(tick);
        clock.store(now.timestamp(), Ordering::SeqCst);
        let report = run_tick(&pool, &cfg, now, quota, sync_fn.clone()).await;
        assert_eq!(report.synced.len(), quota);
    }

    let log = log.lock().unwrap();
    for i in 0..SOURCES {
        let id = format!("src-{i:05}");
        let syncs = log.get(&id).map(Vec::as_slice).unwrap_or_default();
        let full_due = start.timestamp() - 79_300 - (i * 31) % 7000 + 86_400;
        let fulls: Vec<i64> = syncs.iter().filter(|s| s.1).map(|s| s.0).collect();
        if i % 5 == 0 {
            assert_eq!(fulls.len(), 1, "{id}: one full resync");
            assert!(fulls[0] <= full_due + 120, "{id}: full resync late");
        } else {
            assert!(fulls.is_empty(), "{id}: no full resync due");
        }
        let mut last = start.timestamp() - (i * 37) % 3600;
        for &(at, _) in syncs {
            assert!(at - last <= 62 * 60, "{id}: {}s without a sync", at - last);
            last = at;
        }
    }
}

/// One sync at a time per server, at most the concurrency overall, and
/// syncs on different servers run side by side.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn one_sync_per_server_at_a_time_within_the_concurrency() {
    let pool = setup_pool().await;
    let (_, account) = seed_account(&pool).await;
    let mut origin_of = HashMap::new();
    for server in 0..3 {
        for _ in 0..4 {
            let url = format!("https://dav{server}.example.com/");
            let id = add_source(&pool, &account, &url).await;
            origin_of.insert(id, server);
        }
    }
    let origin_of = Arc::new(origin_of);

    for (concurrency, most) in [(4, 3), (2, 2)] {
        sqlx::query("UPDATE caldav_sources SET last_synced = NULL")
            .execute(&pool)
            .await
            .unwrap();
        let per_server: Arc<StdMutex<[usize; 3]>> = Arc::default();
        let worst_per_server = Arc::new(AtomicUsize::new(0));
        let running = Arc::new(AtomicUsize::new(0));
        let worst = Arc::new(AtomicUsize::new(0));
        let sync_fn: SyncFn = {
            let pool = pool.clone();
            let (origin_of, per_server, worst_per_server, running, worst) = (
                origin_of.clone(),
                per_server.clone(),
                worst_per_server.clone(),
                running.clone(),
                worst.clone(),
            );
            Arc::new(move |id: String| {
                let pool = pool.clone();
                let server = origin_of[&id];
                let (per_server, worst_per_server, running, worst) = (
                    per_server.clone(),
                    worst_per_server.clone(),
                    running.clone(),
                    worst.clone(),
                );
                Box::pin(async move {
                    let on_server = {
                        let mut counts = per_server.lock().unwrap();
                        counts[server] += 1;
                        counts[server]
                    };
                    worst_per_server.fetch_max(on_server, Ordering::SeqCst);
                    let now_running = running.fetch_add(1, Ordering::SeqCst) + 1;
                    worst.fetch_max(now_running, Ordering::SeqCst);
                    tokio::time::sleep(Duration::from_millis(40)).await;
                    sqlx::query(
                        "UPDATE caldav_sources SET last_synced = datetime('now') WHERE id = ?",
                    )
                    .bind(&id)
                    .execute(&pool)
                    .await
                    .unwrap();
                    running.fetch_sub(1, Ordering::SeqCst);
                    per_server.lock().unwrap()[server] -= 1;
                })
            })
        };
        let report = run_tick(&pool, &config(concurrency), Utc::now(), 12, sync_fn).await;
        assert_eq!(report.synced.len(), 12, "concurrency {concurrency}");
        assert_eq!(worst_per_server.load(Ordering::SeqCst), 1);
        let worst = worst.load(Ordering::SeqCst);
        assert!(
            (2..=most).contains(&worst),
            "concurrency {concurrency}: {worst} at once"
        );
    }
}

/// A source another sync holds (an on-demand sync, the dashboard's button)
/// is left for a later tick, and one synced since it was picked is not
/// synced again.
#[tokio::test]
async fn a_source_already_being_synced_is_not_synced_twice() {
    let pool = setup_pool().await;
    let (_, account) = seed_account(&pool).await;
    let held = add_source(&pool, &account, "https://a.example.com/").await;
    let free = add_source(&pool, &account, "https://b.example.com/").await;
    let calls: Arc<StdMutex<Vec<String>>> = Arc::default();
    let sync_fn = recording_sync(&pool, calls.clone());

    let guard = sync::source_lock(&held).await.lock_owned().await;
    let report = run_tick(&pool, &config(4), Utc::now(), 5, sync_fn.clone()).await;
    assert_eq!(report.synced, vec![free.clone()]);
    assert_eq!(report.busy, 1);
    drop(guard);
    let report = run_tick(&pool, &config(4), Utc::now(), 1, sync_fn.clone()).await;
    assert_eq!(report.synced, vec![held.clone()]);

    // Picked, then synced by someone else before its turn came.
    let picked = DueSource {
        id: free.clone(),
        origin: "https://b.example.com".to_string(),
        last_attempt: "2000-01-01".to_string(),
    };
    assert_eq!(
        attempt(
            pool.clone(),
            [0u8; 32],
            picked.clone(),
            sync_fn.clone(),
            Duration::from_secs(5)
        )
        .await,
        Attempt::AlreadyDone
    );
    // Or removed.
    sqlx::query("DELETE FROM caldav_sources WHERE id = ?")
        .bind(&free)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        attempt(
            pool.clone(),
            [0u8; 32],
            picked,
            sync_fn,
            Duration::from_secs(5)
        )
        .await,
        Attempt::AlreadyDone
    );
    assert_eq!(*calls.lock().unwrap(), vec![free, held]);
}

/// An on-demand sync of a source the background sync is running does not
/// wait for it: it returns at once, contacting nobody, and the page is served
/// from the cache the background sync is refreshing.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_on_demand_sync_skips_a_source_the_background_is_syncing() {
    let pool = setup_pool().await;
    let (user, account) = seed_account(&pool).await;
    // A private address: a real attempt would fail and be recorded.
    let source = add_source(&pool, &account, "http://127.0.0.1:9/").await;
    let started = Arc::new(tokio::sync::Notify::new());
    let sync_fn: SyncFn = {
        let (pool, started) = (pool.clone(), started.clone());
        Arc::new(move |id: String| {
            let (pool, started) = (pool.clone(), started.clone());
            Box::pin(async move {
                started.notify_one();
                tokio::time::sleep(Duration::from_millis(500)).await;
                sqlx::query("UPDATE caldav_sources SET last_synced = datetime('now') WHERE id = ?")
                    .bind(&id)
                    .execute(&pool)
                    .await
                    .unwrap();
            })
        })
    };
    let background = {
        let pool = pool.clone();
        tokio::spawn(async move { run_tick(&pool, &config(1), Utc::now(), 1, sync_fn).await })
    };
    tokio::time::timeout(std::time::Duration::from_secs(10), started.notified())
        .await
        .expect("the background sync started");
    tokio::time::timeout(
        Duration::from_millis(150),
        sync::sync_if_stale(&pool, &[0u8; 32], &user),
    )
    .await
    .expect("the on-demand sync did not wait");
    assert!(!background.is_finished());
    assert_eq!(background.await.unwrap().synced, vec![source.clone()]);
    let (synced, error): (Option<String>, Option<String>) =
        sqlx::query_as("SELECT last_synced, last_sync_error_at FROM caldav_sources WHERE id = ?")
            .bind(&source)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(synced.is_some());
    assert_eq!(error, None, "the on-demand sync did not run a second sync");
}

/// A slow server holds one slot and its own line, never the others: while
/// its first source drags, the sources on other servers are synced, a new
/// tick starts without waiting for it, and it is never started twice.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_slow_server_holds_only_its_own_slot() {
    let pool = setup_pool().await;
    let (_, account) = seed_account(&pool).await;
    let mut slow = Vec::new();
    for _ in 0..3 {
        slow.push(add_source(&pool, &account, "https://slow.example.com/").await);
    }
    let mut fast = Vec::new();
    for server in ["a", "a", "b", "b", "c"] {
        let url = format!("https://{server}.example.com/");
        fast.push(add_source(&pool, &account, &url).await);
    }
    // The slow server's sources are the stalest, so they come first.
    sqlx::query("UPDATE caldav_sources SET last_synced = datetime('now', '-2 hours') WHERE url LIKE '%slow%'")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE caldav_sources SET last_synced = datetime('now', '-1 hours') WHERE url NOT LIKE '%slow%'")
        .execute(&pool)
        .await
        .unwrap();

    let running: Arc<StdMutex<HashMap<String, usize>>> = Arc::default();
    let twice = Arc::new(AtomicUsize::new(0));
    let slow_ids: Arc<HashSet<String>> = Arc::new(slow.iter().cloned().collect());
    let sync_fn: SyncFn = {
        let (pool, running, twice, slow_ids) = (
            pool.clone(),
            running.clone(),
            twice.clone(),
            slow_ids.clone(),
        );
        Arc::new(move |id: String| {
            let (pool, running, twice, slow_ids) = (
                pool.clone(),
                running.clone(),
                twice.clone(),
                slow_ids.clone(),
            );
            Box::pin(async move {
                {
                    let mut now_running = running.lock().unwrap();
                    let count = now_running.entry(id.clone()).or_default();
                    *count += 1;
                    if *count > 1 {
                        twice.fetch_add(1, Ordering::SeqCst);
                    }
                }
                if slow_ids.contains(&id) {
                    tokio::time::sleep(Duration::from_millis(600)).await;
                }
                sqlx::query("UPDATE caldav_sources SET last_synced = datetime('now') WHERE id = ?")
                    .bind(&id)
                    .execute(&pool)
                    .await
                    .unwrap();
                *running.lock().unwrap().get_mut(&id).unwrap() -= 1;
            })
        })
    };
    let mut scheduler = Scheduler::new(pool.clone(), [0u8; 32], config(2), sync_fn);
    scheduler.begin_tick(Utc::now(), 8).await;
    let fast_done = tokio::time::timeout(Duration::from_secs(5), async {
        while !fast.iter().all(|id| scheduler.report.synced.contains(id)) {
            scheduler.next_done().await;
        }
    })
    .await;
    assert!(fast_done.is_ok(), "the other servers' sources were synced");
    assert!(
        scheduler
            .report
            .synced
            .iter()
            .all(|id| !slow_ids.contains(id)),
        "while the slow server's first source was still running"
    );
    assert_eq!(scheduler.running(), 1);

    // The next tick does not wait for it, and does not start it again.
    scheduler.begin_tick(Utc::now(), 8).await;
    assert!(scheduler.running() >= 1);
    tokio::time::timeout(Duration::from_secs(10), scheduler.settle())
        .await
        .unwrap();
    assert_eq!(
        twice.load(Ordering::SeqCst),
        0,
        "no source ran twice at once"
    );
}

/// A sync that runs past the source timeout is stopped and recorded as a
/// failure of the source, and its lock is released.
#[tokio::test]
async fn a_sync_past_the_timeout_is_stopped_and_recorded() {
    let pool = setup_pool().await;
    let (_, account) = seed_account(&pool).await;
    let source = add_source(&pool, &account, "https://dav.example.com/").await;
    let sync_fn: SyncFn = Arc::new(|_id: String| {
        Box::pin(async move { tokio::time::sleep(Duration::from_secs(30)).await })
    });
    let cfg = SchedulerConfig {
        source_timeout: Duration::from_millis(100),
        ..config(1)
    };
    let report = tokio::time::timeout(
        Duration::from_secs(5),
        run_tick(&pool, &cfg, Utc::now(), 1, sync_fn),
    )
    .await
    .expect("the tick waited for the slow sync");
    assert_eq!(report.timed_out, 1);
    assert!(report.synced.is_empty());
    let (error, failures): (Option<String>, i64) = sqlx::query_as(
        "SELECT last_sync_error, sync_failure_count FROM caldav_sources WHERE id = ?",
    )
    .bind(&source)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(error.unwrap().contains("took longer"));
    assert_eq!(failures, 1);
    assert!(sync::source_lock(&source).await.try_lock().is_ok());
}

/// Cloud calendars reached through OAuth throttle per user: each source is
/// a line of its own, while self-hosted servers keep one line each.
#[test]
fn oauth_cloud_sources_are_keyed_per_source() {
    let row = |id: &str, url: &str, provider: &str, auth: &str| sync::SourceSyncState {
        id: id.to_string(),
        sync_url: Some(url.to_string()),
        provider_type: provider.to_string(),
        auth_type: auth.to_string(),
        ..Default::default()
    };
    let graph = "https://graph.microsoft.com/v1.0";
    let google = "https://apidata.googleusercontent.com/caldav/v2/";
    let ms1 = row("m1", graph, "microsoft", "oauth2").sync_key();
    let ms2 = row("m2", graph, "microsoft", "oauth2").sync_key();
    let g1 = row("g1", google, "caldav", "oauth2").sync_key();
    let g2 = row("g2", google, "caldav", "oauth2").sync_key();
    assert_ne!(ms1, ms2);
    assert_ne!(g1, g2);
    assert_ne!(ms1, g1);
    let dav1 = row("d1", "https://dav.example.com/a", "caldav", "basic").sync_key();
    let dav2 = row("d2", "https://dav.example.com/b", "caldav", "basic").sync_key();
    assert_eq!(dav1, dav2);
    let ews = row(
        "e1",
        "https://mail.example.com/EWS/Exchange.asmx",
        "ews",
        "basic",
    )
    .sync_key();
    assert_eq!(ews, "https://mail.example.com");
}

/// The background sync is started again after a panic.
#[tokio::test]
async fn the_background_sync_is_restarted_after_a_panic() {
    let starts = Arc::new(AtomicUsize::new(0));
    let counter = starts.clone();
    tokio::time::timeout(
        Duration::from_secs(5),
        supervise(Duration::from_millis(10), move || {
            let run = counter.fetch_add(1, Ordering::SeqCst);
            async move {
                if run == 0 {
                    panic!("first run fails");
                }
            }
        }),
    )
    .await
    .expect("the second run ended normally");
    assert_eq!(starts.load(Ordering::SeqCst), 2);
}

/// The production sync runs through the per-source lock too: a sync of a
/// source by id waits for one already running.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn sync_by_id_waits_for_a_running_sync() {
    let pool = setup_pool().await;
    let (_, account) = seed_account(&pool).await;
    let source = add_source(&pool, &account, "http://127.0.0.1:9/").await;
    let guard = sync::source_lock(&source).await.lock_owned().await;
    let waiting = {
        let (pool, source) = (pool.clone(), source.clone());
        tokio::spawn(async move { sync::sync_source_by_id(&pool, &[0u8; 32], &source).await })
    };
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(!waiting.is_finished(), "waits for the lock");
    drop(guard);
    waiting.await.unwrap();
    let attempted: Option<String> =
        sqlx::query_scalar("SELECT last_full_attempt_at FROM caldav_sources WHERE id = ?")
            .bind(&source)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(attempted.is_some(), "a first sync is a full one, and noted");
}
