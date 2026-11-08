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
    let router = create_router(pool.clone(), dir.to_path_buf(), [0u8; 32]).await;
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
