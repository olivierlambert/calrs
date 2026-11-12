//! Booking submissions under concurrent load, on a pool shaped like
//! production's, and submit-time availability reads that fail.

use super::*;

/// The production pool (`db::connect`: five connections, WAL, a file on
/// disk) with the usual test fixtures, and a router over it.
async fn production_pool_app() -> (Router, SqlitePool, String, TestDataDir) {
    let dir = TestDataDir::new();
    let pool = crate::db::connect(&dir).await.unwrap();
    crate::db::migrate(&pool).await.unwrap();
    let (_, _, et_id) = seed_test_data(&pool).await;
    let router = create_router(pool.clone(), dir.to_path_buf(), [0u8; 32])
        .await
        .layer(local_proxy_peer());
    (router, pool, et_id, dir)
}

/// A booking form submission from its own address, so the per-IP booking
/// rate limit does not refuse any of a burst.
fn booking_post(path: &str, ip: usize, date: &str, time: &str) -> axum::http::Request<Body> {
    let mut request = post_form_unauthed(
        path,
        "burst",
        &format!(
            "_csrf=burst&date={date}&time={time}&tz=UTC&name=Guest+{ip}&email=guest{ip}%40example.com"
        ),
    );
    request.headers_mut().insert(
        "x-forwarded-for",
        format!("198.51.100.{ip}").parse().unwrap(),
    );
    request
}

/// Twelve guests submit at once, more than the pool has connections, on two
/// event types of the same host: eight of them for the same time. Every
/// submission is answered within seconds (a booking used to hold a pool
/// connection for its transaction while its checks waited for another, so
/// five of them could stall every request until sqlx's 30 second acquire
/// timeout), and the host ends up with one booking per time, the overlap
/// across event types included, which no unique index covers.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_burst_of_bookings_is_answered_quickly_and_never_double_books_the_host() {
    let (app, pool, et_id, _dir) = production_pool_app().await;
    let other_et = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO event_types (id, account_id, slug, title, duration_min, buffer_before, buffer_after, min_notice_min, enabled)
         SELECT ?, account_id, 'other-meeting', 'Other Meeting', 30, 0, 0, 0, 1 FROM event_types WHERE id = ?",
    )
    .bind(&other_et)
    .bind(&et_id)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO availability_rules (id, event_type_id, day_of_week, start_time, end_time)
         SELECT lower(hex(randomblob(16))), ?, day_of_week, start_time, end_time
         FROM availability_rules WHERE event_type_id = ?",
    )
    .bind(&other_et)
    .bind(&et_id)
    .execute(&pool)
    .await
    .unwrap();

    let day = far_future_monday();
    let mut requests = Vec::new();
    for ip in 0..12 {
        let (path, time) = match ip {
            0..=3 => ("/u/testuser/test-meeting/book", "10:00"),
            4..=7 => ("/u/testuser/other-meeting/book", "10:00"),
            8 | 9 => ("/u/testuser/test-meeting/book", "13:00"),
            _ => ("/u/testuser/other-meeting/book", "14:00"),
        };
        let app = app.clone();
        let request = booking_post(path, ip, &day, time);
        requests.push(tokio::spawn(async move {
            app.oneshot(request).await.unwrap().status()
        }));
    }
    let statuses = tokio::time::timeout(std::time::Duration::from_secs(20), async {
        let mut statuses = Vec::new();
        for request in requests {
            statuses.push(request.await.unwrap());
        }
        statuses
    })
    .await
    .expect("the burst must not stall on the connection pool");
    assert!(
        statuses.iter().all(|s| !s.is_server_error()),
        "{statuses:?}"
    );

    let starts: Vec<String> = sqlx::query_scalar(
        "SELECT start_at FROM bookings WHERE status = 'confirmed' ORDER BY start_at",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    let date = NaiveDate::parse_from_str(&day, "%Y-%m-%d").unwrap();
    let at = |hour: u32| {
        crate::booking_time::encode(
            date.and_hms_opt(hour, 0, 0).unwrap(),
            chrono_tz::Tz::UTC,
            30,
        )
        .unwrap()
        .0
    };
    assert_eq!(starts, vec![at(10), at(13), at(14)]);
}

/// A submit-time busy read that fails refuses the booking: the host's
/// calendar could not be read, so they are not known to be free.
#[tokio::test]
async fn an_unreadable_calendar_refuses_the_booking() {
    let _lock = crate::test_support::ENV_LOCK.lock().await;
    let _env = crate::test_support::EnvGuard::new();
    let (app, pool, _session, et_id) = setup_test_app().await;
    sqlx::query("ALTER TABLE events RENAME TO events_gone")
        .execute(&pool)
        .await
        .unwrap();
    let day = far_future_monday();
    let response = app
        .oneshot(booking_post(
            "/u/testuser/test-meeting/book",
            1,
            &day,
            "10:00",
        ))
        .await
        .unwrap();
    assert_eq!(
        response.status(),
        axum::http::StatusCode::INTERNAL_SERVER_ERROR
    );
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM bookings WHERE event_type_id = ?")
        .bind(&et_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        count, 0,
        "no booking over a calendar that could not be read"
    );
}

/// Every strict reader used by the booking, round-robin, reschedule and
/// approval checks reports the failure, where the slot grids' lenient
/// reader skips the rows and carries on.
#[tokio::test]
async fn strict_busy_readers_report_an_unreadable_table() {
    let pool = setup_test_db().await;
    let (user, _, et_id) = seed_test_data(&pool).await;
    sqlx::query("INSERT INTO teams (id, name, slug, visibility) VALUES ('t', 'T', 't', 'public')")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO team_members (team_id, user_id, role, source) VALUES ('t', ?, 'admin', 'direct')",
    )
    .bind(&user)
    .execute(&pool)
    .await
    .unwrap();
    let start = NaiveDate::parse_from_str(&far_future_monday(), "%Y-%m-%d")
        .unwrap()
        .and_hms_opt(10, 0, 0)
        .unwrap();
    let end = start + Duration::minutes(30);
    let utc = chrono_tz::Tz::UTC;
    sqlx::query("ALTER TABLE events RENAME TO events_gone")
        .execute(&pool)
        .await
        .unwrap();

    assert!(
        fetch_busy_times_for_user(&pool, &user, start, end, utc, Some(&et_id))
            .await
            .is_empty(),
        "the slot grid degrades softly"
    );
    assert!(
        try_fetch_busy_times_for_user(&pool, &user, start, end, utc, Some(&et_id))
            .await
            .is_err()
    );
    assert!(
        pick_group_member(&pool, "t", &et_id, start, end, start, 0, 0, utc)
            .await
            .is_err()
    );
    assert!(
        try_team_busy_source(&pool, &et_id, "t", true, start, end, utc)
            .await
            .is_err()
    );
    let hosts = RescheduleHosts {
        user_ids: vec![user.clone()],
        team_id: None,
        collective: false,
        participants: Vec::new(),
        claimant: None,
    };
    assert!(
        reschedule_busy_source(&pool, &hosts, &et_id, start, end, utc, "", "", true)
            .await
            .is_err()
    );
    let (start_at, end_at) = (
        format!("{}Z", start.format("%Y-%m-%dT%H:%M:%S")),
        format!("{}Z", end.format("%Y-%m-%dT%H:%M:%S")),
    );
    assert!(
        approval_hosts_free(&pool, &hosts, "b", "u", &et_id, &start_at, &end_at)
            .await
            .is_err()
    );
}

/// The resource check at booking time fails closed too.
#[tokio::test]
async fn an_unreadable_resource_calendar_is_an_error_at_booking_time() {
    let pool = setup_test_db().await;
    let (_, _, et_id) = seed_test_data(&pool).await;
    let resource = insert_resource(&pool, "Lab").await;
    attach_resource(&pool, &et_id, &resource).await;
    sqlx::query("ALTER TABLE resource_events RENAME TO resource_events_gone")
        .execute(&pool)
        .await
        .unwrap();
    let start = NaiveDate::parse_from_str(&far_future_monday(), "%Y-%m-%d")
        .unwrap()
        .and_hms_opt(10, 0, 0)
        .unwrap();
    assert!(crate::resources::check_and_pick(
        &pool,
        &et_id,
        start,
        start + Duration::minutes(30),
        chrono_tz::Tz::UTC,
        None,
    )
    .await
    .is_err());
}

/// The submit-time slot check runs under the booking lock, so it reads
/// resource feeds from the cache and never fetches one, even a stale one;
/// the slot grid still refreshes it. A fetch, failed or not, stamps
/// `last_synced_at`, which is how this tells the two apart.
#[tokio::test]
async fn the_submit_time_slot_check_never_fetches_a_resource_feed() {
    // No test may allowlist the loopback feed below meanwhile.
    let _lock = crate::test_support::ENV_LOCK.lock().await;
    let pool = setup_test_db().await;
    let (_, _, et_id) = seed_test_data(&pool).await;
    let resource = insert_resource(&pool, "Lab").await;
    attach_resource(&pool, &et_id, &resource).await;
    let make_stale = || async {
        sqlx::query("UPDATE resources SET last_synced_at = NULL, feed_url = 'http://127.0.0.1:9/feed.ics' WHERE id = ?")
            .bind(&resource)
            .execute(&pool)
            .await
            .unwrap();
    };
    let synced = || async {
        sqlx::query_scalar::<_, Option<String>>("SELECT last_synced_at FROM resources WHERE id = ?")
            .bind(&resource)
            .fetch_one(&pool)
            .await
            .unwrap()
            .is_some()
    };
    let utc = chrono_tz::Tz::UTC;
    let slot = NaiveDate::parse_from_str(&far_future_monday(), "%Y-%m-%d")
        .unwrap()
        .and_hms_opt(10, 0, 0)
        .unwrap();
    let window = offered_slot_window(utc, slot);

    make_stale().await;
    let offered = is_offered_slot(
        &pool,
        &et_id,
        30,
        0,
        0,
        0,
        utc,
        utc,
        &window,
        BusySource::Individual(Vec::new()),
        None,
        slot,
    )
    .await
    .unwrap();
    assert!(offered, "the cached feed is empty, so the slot is free");
    assert!(!synced().await, "no fetch under the booking lock");

    compute_slots(
        &pool,
        &et_id,
        30,
        0,
        0,
        0,
        window.start_offset,
        window.days_ahead,
        BookingWindow::default(),
        utc,
        utc,
        BusySource::Individual(Vec::new()),
        None,
    )
    .await;
    assert!(synced().await, "the slot grid refreshes a stale feed");
}

/// Blocked days, the slot interval and first-slot-only are part of what the
/// page offers: unreadable at submit time, the booking is refused, while the
/// grid degrades and keeps showing slots.
#[tokio::test]
async fn unreadable_slot_settings_refuse_the_booking() {
    let _lock = crate::test_support::ENV_LOCK.lock().await;
    let _env = crate::test_support::EnvGuard::new();
    let (app, pool, _session, et_id) = setup_test_app().await;
    sqlx::query("ALTER TABLE availability_overrides RENAME TO availability_overrides_gone")
        .execute(&pool)
        .await
        .unwrap();
    let utc = chrono_tz::Tz::UTC;
    let day = far_future_monday();
    let slot = NaiveDate::parse_from_str(&day, "%Y-%m-%d")
        .unwrap()
        .and_hms_opt(10, 0, 0)
        .unwrap();
    let window = offered_slot_window(utc, slot);
    let grid = compute_slots(
        &pool,
        &et_id,
        30,
        0,
        0,
        0,
        window.start_offset,
        window.days_ahead,
        BookingWindow::default(),
        utc,
        utc,
        BusySource::Individual(Vec::new()),
        None,
    )
    .await;
    assert!(
        grid.iter().any(|d| !d.slots.is_empty()),
        "the grid degrades"
    );
    assert!(is_offered_slot(
        &pool,
        &et_id,
        30,
        0,
        0,
        0,
        utc,
        utc,
        &window,
        BusySource::Individual(Vec::new()),
        None,
        slot,
    )
    .await
    .is_err());

    let response = app
        .oneshot(booking_post(
            "/u/testuser/test-meeting/book",
            2,
            &day,
            "10:00",
        ))
        .await
        .unwrap();
    assert_eq!(
        response.status(),
        axum::http::StatusCode::INTERNAL_SERVER_ERROR
    );
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM bookings")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
}

/// An unreadable resource attachment list blocks the slot grid, as an
/// unreadable resource calendar does, and is an error at booking time:
/// never "this event type needs no resource".
#[tokio::test]
async fn an_unreadable_resource_list_blocks_and_refuses() {
    let pool = setup_test_db().await;
    let (_, _, et_id) = seed_test_data(&pool).await;
    sqlx::query("ALTER TABLE event_type_resources RENAME TO event_type_resources_gone")
        .execute(&pool)
        .await
        .unwrap();
    let start = NaiveDate::parse_from_str(&far_future_monday(), "%Y-%m-%d")
        .unwrap()
        .and_hms_opt(10, 0, 0)
        .unwrap();
    let end = start + Duration::minutes(30);
    let utc = chrono_tz::Tz::UTC;
    assert_eq!(
        crate::resources::blocking_intervals_for_event_type(&pool, &et_id, start, end, utc, None)
            .await,
        vec![(start, end)]
    );
    assert!(
        crate::resources::try_cached_blocking_intervals_for_event_type(
            &pool, &et_id, start, end, utc, None
        )
        .await
        .is_err()
    );
    assert!(
        crate::resources::check_and_pick(&pool, &et_id, start, end, utc, None)
            .await
            .is_err()
    );
}

/// A reschedule or an approval checks the booking's real hosts: when they
/// cannot be read, it is an error, not a check of the event type owner
/// without the co-hosts.
#[tokio::test]
async fn unreadable_reschedule_hosts_are_an_error() {
    let pool = setup_test_db().await;
    let (_, _, et_id) = seed_test_data(&pool).await;
    let day = far_future_monday();
    let (booking_id, _, _) = insert_test_booking(
        &pool,
        &et_id,
        &format!("{day}T10:00:00"),
        &format!("{day}T10:30:00"),
        "confirmed",
    )
    .await;
    let uid: String = sqlx::query_scalar("SELECT uid FROM bookings WHERE id = ?")
        .bind(&booking_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    let owner: String = sqlx::query_scalar("SELECT id FROM users WHERE username = 'testuser'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(reschedule_hosts(&pool, &uid, &owner).await.is_ok());
    sqlx::query("ALTER TABLE booking_participants RENAME TO booking_participants_gone")
        .execute(&pool)
        .await
        .unwrap();
    assert!(reschedule_hosts(&pool, &uid, &owner).await.is_err());
}

/// Six guests submit at once through the same single-use invite link, each
/// for a different time, so nothing but the invite can refuse them (#373).
/// The check and the count used to be two steps, so several could pass the
/// check before any was counted. Exactly one booking is made, the invite is
/// counted once, and the others are refused as a used invite.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_single_use_invite_books_once_under_concurrent_submissions() {
    let (app, pool, et_id, _dir) = production_pool_app().await;
    sqlx::query("UPDATE event_types SET visibility = 'private' WHERE id = ?")
        .bind(&et_id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO booking_invites (id, event_type_id, token, guest_name, guest_email, max_uses, created_by_user_id)
         SELECT 'inv373', ?, 'tok373', '', '', 1, id FROM users WHERE username = 'testuser'",
    )
    .bind(&et_id)
    .execute(&pool)
    .await
    .unwrap();

    // Hold the booking lock while the six read the invite, so every one of
    // them has passed the early check before the first can book: the old
    // two-step count then let all six through. The early check comes before
    // the lock on this path.
    let lock = crate::resources::booking_lock().await;
    let day = far_future_monday();
    let mut requests = Vec::new();
    for (ip, time) in ["09:00", "10:00", "11:00", "13:00", "14:00", "15:00"]
        .into_iter()
        .enumerate()
    {
        let app = app.clone();
        let mut request = post_form_unauthed(
            "/u/testuser/test-meeting/book",
            "burst",
            &format!(
                "_csrf=burst&date={day}&time={time}&tz=UTC&name=Guest+{ip}&email=guest{ip}%40example.com&invite_token=tok373"
            ),
        );
        request.headers_mut().insert(
            "x-forwarded-for",
            format!("198.51.100.{ip}").parse().unwrap(),
        );
        requests.push(tokio::spawn(async move {
            app.oneshot(request).await.unwrap().status()
        }));
    }
    // Release the lock only once all six have passed the early check: each
    // then waits on the lock, so none can have spent the invite yet.
    tokio::time::timeout(std::time::Duration::from_secs(20), async {
        loop {
            let passed = INVITE_CHECKS_PASSED
                .lock()
                .unwrap()
                .iter()
                .filter(|t| t.as_str() == "tok373")
                .count();
            if passed == 6 {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("all six submissions pass the early invite check");
    drop(lock);
    for request in requests {
        let status = request.await.unwrap();
        assert!(!status.is_server_error(), "{status}");
    }

    let bookings: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM bookings WHERE event_type_id = ?")
        .bind(&et_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(bookings, 1, "a single-use invite books once");
    let used: i64 =
        sqlx::query_scalar("SELECT used_count FROM booking_invites WHERE id = 'inv373'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(used, 1);
}

/// Starts a guest reschedule to 11:00 UTC, lets it read the booking and wait
/// at the held booking lock, runs `meanwhile` (which commits a competing
/// change), then releases the lock and returns the reschedule's response.
async fn reschedule_overtaken_by<F, Fut>(
    app: &Router,
    pool: &SqlitePool,
    day: &str,
    meanwhile: F,
) -> axum::response::Response
where
    F: FnOnce(Router, SqlitePool) -> Fut,
    Fut: std::future::Future<Output = ()>,
{
    let guard = crate::resources::booking_lock().await;
    // Armed while holding the lock, so no other test's reschedule with this
    // token is in flight.
    let reached = crate::test_support::arm("reschedule:stale-reschedule");
    let mut request = Box::pin(app.clone().oneshot(post_form_unauthed(
        "/booking/reschedule/stale-reschedule",
        "stale",
        &format!("_csrf=stale&date={day}&time=11:00&tz=UTC"),
    )));
    // Wait until the handler has done its first read and is about to wait
    // for the held lock: the competing change then lands between the two,
    // which is the case the guard under the lock exists for.
    tokio::select! {
        _ = reached.notify.notified() => {}
        response = &mut request => panic!(
            "the reschedule answered before reaching the booking lock: {:?}",
            response.map(|r| r.status())
        ),
        _ = tokio::time::sleep(std::time::Duration::from_secs(30)) => {
            panic!("the reschedule never reached the booking lock")
        }
    }
    meanwhile(app.clone(), pool.clone()).await;
    drop(guard);
    request.await.unwrap()
}

fn nine_utc(day: &str) -> chrono::DateTime<Utc> {
    NaiveDate::parse_from_str(day, "%Y-%m-%d")
        .unwrap()
        .and_hms_opt(9, 0, 0)
        .unwrap()
        .and_utc()
}

fn stale_tokens() -> Vec<(&'static str, Option<String>)> {
    vec![
        ("reschedule_token", Some("stale-reschedule".into())),
        ("cancel_token", Some("stale-cancel".into())),
    ]
}

#[tokio::test]
async fn reschedule_waiting_for_the_lock_does_not_revive_a_cancelled_booking() {
    let (app, pool, _, et) = setup_test_app().await;
    let day = far_future_monday();
    let bid = insert_v1_booking(
        &pool,
        &et,
        "guest@example.com",
        nine_utc(&day),
        30,
        "confirmed",
        &stale_tokens(),
    )
    .await;
    let response = reschedule_overtaken_by(&app, &pool, &day, |app, pool| {
        let bid = bid.clone();
        async move {
            let cancelled = app
                .oneshot(post_form_unauthed(
                    "/booking/cancel/stale-cancel",
                    "stale",
                    "_csrf=stale",
                ))
                .await
                .unwrap();
            assert!(!cancelled.status().is_server_error());
            assert_eq!(booking_status(&pool, &bid).await, "cancelled");
        }
    })
    .await;
    assert_eq!(
        booking_status(&pool, &bid).await,
        "cancelled",
        "reschedule returned {} and revived a cancelled booking",
        response.status()
    );
    let (start, token): (String, String) =
        sqlx::query_as("SELECT start_at, reschedule_token FROM bookings WHERE id = ?")
            .bind(&bid)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(start, utc_stamp(nine_utc(&day)));
    assert_eq!(token, "stale-reschedule");
}

#[tokio::test]
async fn reschedule_waiting_for_the_lock_does_not_revive_a_declined_request() {
    let (app, pool, _, et) = setup_test_app().await;
    let day = far_future_monday();
    let bid = insert_v1_booking(
        &pool,
        &et,
        "guest@example.com",
        nine_utc(&day),
        30,
        "pending",
        &stale_tokens(),
    )
    .await;
    let response = reschedule_overtaken_by(&app, &pool, &day, |_, pool| {
        let bid = bid.clone();
        async move {
            sqlx::query("UPDATE bookings SET status = 'declined' WHERE id = ?")
                .bind(&bid)
                .execute(&pool)
                .await
                .unwrap();
        }
    })
    .await;
    assert_eq!(
        booking_status(&pool, &bid).await,
        "declined",
        "reschedule returned {}",
        response.status()
    );
}

#[tokio::test]
async fn reschedule_with_a_token_another_reschedule_rotated_is_refused() {
    let (app, pool, _, et) = setup_test_app().await;
    let day = far_future_monday();
    let bid = insert_v1_booking(
        &pool,
        &et,
        "guest@example.com",
        nine_utc(&day),
        30,
        "confirmed",
        &stale_tokens(),
    )
    .await;
    let response = reschedule_overtaken_by(&app, &pool, &day, |_, pool| {
        let (bid, day) = (bid.clone(), day.clone());
        async move {
        sqlx::query(
            "UPDATE bookings SET reschedule_token = 'rotated', start_at = ?, end_at = ? WHERE id = ?",
        )
        .bind(utc_stamp(nine_utc(&day) + Duration::hours(5)))
        .bind(utc_stamp(nine_utc(&day) + Duration::hours(5) + Duration::minutes(30)))
        .bind(&bid)
        .execute(&pool)
        .await
        .unwrap();
        }
    })
    .await;
    assert_eq!(response.status(), axum::http::StatusCode::OK);
    let (start, token): (String, String) =
        sqlx::query_as("SELECT start_at, reschedule_token FROM bookings WHERE id = ?")
            .bind(&bid)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(token, "rotated");
    assert_eq!(start, utc_stamp(nine_utc(&day) + Duration::hours(5)));
}

#[tokio::test]
async fn an_undisturbed_reschedule_moves_the_booking_and_rotates_the_token() {
    let (app, pool, _, et) = setup_test_app().await;
    let day = far_future_monday();
    let bid = insert_v1_booking(
        &pool,
        &et,
        "guest@example.com",
        nine_utc(&day),
        30,
        "confirmed",
        &stale_tokens(),
    )
    .await;
    let response = app
        .oneshot(post_form_unauthed(
            "/booking/reschedule/stale-reschedule",
            "stale",
            &format!("_csrf=stale&date={day}&time=11:00&tz=UTC"),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), axum::http::StatusCode::OK);
    let (status, start, token): (String, String, String) =
        sqlx::query_as("SELECT status, start_at, reschedule_token FROM bookings WHERE id = ?")
            .bind(&bid)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(status, "confirmed");
    assert_eq!(
        start,
        utc_stamp(nine_utc(&day) + Duration::hours(2)),
        "the booking moved to 11:00"
    );
    assert_ne!(token, "stale-reschedule");
}
