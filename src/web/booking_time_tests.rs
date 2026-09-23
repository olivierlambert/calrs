// Included in web::tests so these use the same real router and fixtures.
#[tokio::test]
async fn utc_booking_http_create_display_calendar_and_cancel() {
    let (app, pool, session, et) = setup_test_app().await;
    sqlx::query("UPDATE event_types SET timezone='Europe/Paris', min_notice_min=0 WHERE id=?")
        .bind(&et)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE users SET timezone='Europe/Paris'")
        .execute(&pool)
        .await
        .unwrap();
    let mut day = Utc::now().date_naive() + Duration::days(8);
    while day.weekday() != chrono::Weekday::Mon {
        day += Duration::days(1);
    }
    let response=app.clone().oneshot(post_form_unauthed("/u/testuser/test-meeting/book","utc",&format!("_csrf=utc&date={day}&time=10:00&tz=Europe%2FParis&name=UTC+Guest&email=utc%40test.com"))).await.unwrap();
    let response_body = body_string(response).await;
    let booking: Option<(String,String,String,String,i64)>=sqlx::query_as("SELECT id,start_at,end_at,cancel_token,time_version FROM bookings WHERE guest_email='utc@test.com'").fetch_optional(&pool).await.unwrap();
    assert!(booking.is_some(), "{response_body}");
    let (id, start, end, cancel, version) = booking.unwrap();
    let expected = crate::booking_time::encode(
        day.and_hms_opt(10, 0, 0).unwrap(),
        "Europe/Paris".parse().unwrap(),
        30,
    )
    .unwrap();
    assert_eq!((&start, &end, version), (&expected.0, &expected.1, 1));
    // Changing the event timezone must not reinterpret this stored instant.
    sqlx::query("UPDATE event_types SET timezone='Pacific/Honolulu' WHERE id=?")
        .bind(&et)
        .execute(&pool)
        .await
        .unwrap();
    let body = body_string(
        app.clone()
            .oneshot(get_authed("/dashboard/bookings", &session))
            .await
            .unwrap(),
    )
    .await;
    assert!(body.contains("10:00 AM"), "{body}");
    let body = body_string(
        app.clone()
            .oneshot(get(&format!("/booking/cancel/{cancel}")))
            .await
            .unwrap(),
    )
    .await;
    assert!(body.contains("10:00"), "{body}");
    let response = app
        .clone()
        .oneshot(get(&format!("/booking/ics/{cancel}")))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let ics = body_string(response).await;
    assert!(
        ics.contains(&format!(
            "DTSTART:{}",
            crate::booking_time::ics_times(&start, &end).unwrap().0
        )),
        "{ics}"
    );
    app.oneshot(post_form_unauthed(
        &format!("/booking/cancel/{cancel}"),
        "utc",
        "_csrf=utc",
    ))
    .await
    .unwrap();
    let after: (String, String, i64, String) =
        sqlx::query_as("SELECT start_at,end_at,time_version,status FROM bookings WHERE id=?")
            .bind(&id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(after, (start, end, 1, "cancelled".into()));
}

#[tokio::test]
async fn utc_reminders_fire_before_start_and_never_after() {
    let pool = setup_test_db().await;
    let (_, _, et) = seed_test_data(&pool).await;
    sqlx::query("UPDATE event_types SET timezone='Europe/Paris', reminder_minutes=60 WHERE id=?")
        .bind(&et)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO bookings(id,event_type_id,uid,guest_name,guest_email,guest_timezone,start_at,end_at,cancel_token,reschedule_token,time_version) VALUES ('utc',?,'utc','Guest','g@test.com','Europe/Paris','2026-07-01T12:00:00Z','2026-07-01T12:30:00Z','cancel','resched',1)").bind(&et).execute(&pool).await.unwrap();
    for (clock, expected) in [
        ("2026-07-01T10:59:59Z", 0),
        ("2026-07-01T11:00:00Z", 1),
        ("2026-07-01T11:59:59Z", 1),
        ("2026-07-01T12:00:00Z", 0),
    ] {
        assert_eq!(
            due_reminders(
                &pool,
                chrono::DateTime::parse_from_rfc3339(clock)
                    .unwrap()
                    .with_timezone(&Utc)
            )
            .await
            .len(),
            expected,
            "{clock}"
        );
    }
    sqlx::query("UPDATE bookings SET reminder_sent_at='2026-07-01 11:00:00'")
        .execute(&pool)
        .await
        .unwrap();
    assert!(due_reminders(
        &pool,
        chrono::DateTime::parse_from_rfc3339("2026-07-01T11:30:00Z")
            .unwrap()
            .with_timezone(&Utc)
    )
    .await
    .is_empty());
}

#[tokio::test]
async fn utc_and_legacy_bookings_block_same_local_availability() {
    let pool = setup_test_db().await;
    let (user, _, et) = seed_test_data(&pool).await;
    let paris = "Europe/Paris".parse::<Tz>().unwrap();
    sqlx::query("UPDATE event_types SET timezone='Europe/Paris' WHERE id=?")
        .bind(&et)
        .execute(&pool)
        .await
        .unwrap();
    for (id, start, end, version) in [
        ("legacy", "2026-07-01T10:00:00", "2026-07-01T10:30:00", 0),
        ("utc", "2026-07-01T09:00:00Z", "2026-07-01T09:30:00Z", 1),
    ] {
        sqlx::query("INSERT INTO bookings(id,event_type_id,uid,guest_name,guest_email,guest_timezone,start_at,end_at,cancel_token,reschedule_token,time_version) VALUES (?, ?, ?, 'G','g@test.com','Europe/Paris',?,?,?, ?,?)")
            .bind(id).bind(&et).bind(id).bind(start).bind(end).bind(id).bind(id).bind(version).execute(&pool).await.unwrap();
    }
    let from = crate::booking_time::naive("2026-07-01T09:00:00").unwrap();
    let to = crate::booking_time::naive("2026-07-01T12:00:00").unwrap();
    let busy = fetch_busy_times_for_user(&pool, &user, from, to, paris, Some(&et)).await;
    for hour in [10, 11] {
        let start = from.date().and_hms_opt(hour, 0, 0).unwrap();
        assert!(
            has_conflict(&busy, start, start + Duration::minutes(30)),
            "{hour}: {busy:?}"
        );
    }
    // Pending legacy rows must retain the unique-slot protection as well.
    sqlx::query("UPDATE bookings SET status='pending' WHERE id='legacy'")
        .execute(&pool)
        .await
        .unwrap();
    assert!(
        crate::booking_time::legacy_slot_taken(&pool, &et, None, "2026-07-01T08:00:00Z", "")
            .await
            .unwrap()
    );
    assert!(
        !crate::booking_time::legacy_slot_taken(&pool, &et, None, "2026-07-01T09:00:00Z", "")
            .await
            .unwrap()
    );
    let counts = crate::booking_time::period_counts(
        &pool,
        &et,
        from.date().and_hms_opt(0, 0, 0).unwrap(),
        from.date()
            .succ_opt()
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap(),
    )
    .await
    .unwrap();
    assert_eq!(counts.iter().map(|(_, n)| n).sum::<i64>(), 2);
}

#[tokio::test]
async fn utc_reschedule_upgrades_only_the_edited_legacy_booking() {
    let (app, pool, _, et) = setup_test_app().await;
    sqlx::query("UPDATE event_types SET timezone='Europe/Paris' WHERE id=?")
        .bind(&et)
        .execute(&pool)
        .await
        .unwrap();
    for id in ["edited", "untouched"] {
        let start = if id == "edited" {
            "2026-07-01T10:00:00"
        } else {
            "2026-07-02T10:00:00"
        };
        let end = if id == "edited" {
            "2026-07-01T10:30:00"
        } else {
            "2026-07-02T10:30:00"
        };
        sqlx::query("INSERT INTO bookings(id,event_type_id,uid,guest_name,guest_email,guest_timezone,start_at,end_at,cancel_token,reschedule_token) VALUES (?,?,?,'Guest','g@test.com','Europe/Paris',?,?,?,?)")
            .bind(id).bind(&et).bind(id).bind(start).bind(end).bind(id).bind(id).execute(&pool).await.unwrap();
    }
    let mut day = Utc::now().date_naive() + Duration::days(9);
    while day.weekday() != chrono::Weekday::Mon {
        day += Duration::days(1);
    }
    let response = app
        .oneshot(post_form_unauthed(
            "/booking/reschedule/edited",
            "utc",
            &format!("_csrf=utc&date={day}&time=10:00&tz=Europe%2FParis"),
        ))
        .await
        .unwrap();
    let body = body_string(response).await;
    let edited: (String, i64) =
        sqlx::query_as("SELECT start_at,time_version FROM bookings WHERE id='edited'")
            .fetch_one(&pool)
            .await
            .unwrap();
    let expected = crate::booking_time::encode(
        day.and_hms_opt(10, 0, 0).unwrap(),
        "Europe/Paris".parse().unwrap(),
        30,
    )
    .unwrap()
    .0;
    assert_eq!(edited, (expected, 1), "{body}");
    let old: (String, String, i64) =
        sqlx::query_as("SELECT start_at,end_at,time_version FROM bookings WHERE id='untouched'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        old,
        (
            "2026-07-02T10:00:00".into(),
            "2026-07-02T10:30:00".into(),
            0
        )
    );
}

#[tokio::test]
async fn utc_period_counts_follow_local_midnight_and_resources_convert() {
    let pool = setup_test_db().await;
    let (_, _, et) = seed_test_data(&pool).await;
    sqlx::query("UPDATE event_types SET timezone='Pacific/Auckland' WHERE id=?")
        .bind(&et)
        .execute(&pool)
        .await
        .unwrap();
    // 00:30 in Auckland is on the previous UTC day.
    sqlx::query("INSERT INTO bookings(id,event_type_id,uid,guest_name,guest_email,guest_timezone,start_at,end_at,cancel_token,reschedule_token,time_version) VALUES ('utc',?,'utc','Guest','g@test.com','Pacific/Auckland','2026-07-01T12:30:00Z','2026-07-01T13:00:00Z','cancel','resched',1)").bind(&et).execute(&pool).await.unwrap();
    let from = crate::booking_time::naive("2026-07-02T00:00:00").unwrap();
    let to = from + Duration::days(1);
    assert_eq!(
        crate::booking_time::period_counts(&pool, &et, from, to)
            .await
            .unwrap()
            .iter()
            .map(|(_, n)| n)
            .sum::<i64>(),
        1
    );
    assert!(
        crate::booking_time::period_counts(&pool, &et, from - Duration::days(1), from)
            .await
            .unwrap()
            .is_empty()
    );
    let resource = insert_resource(&pool, "Timezone resource").await;
    attach_resource(&pool, &et, &resource).await;
    let busy = crate::resources::busy_for_resource(
        &pool,
        &resource,
        from,
        to,
        "Pacific/Auckland".parse().unwrap(),
        None,
    )
    .await;
    assert!(
        has_conflict(
            &busy,
            from + Duration::minutes(30),
            from + Duration::minutes(60)
        ),
        "{busy:?}"
    );
}

#[tokio::test]
async fn utc_storage_version_preserves_legacy_z_suffix_interpretation() {
    let (app, pool, session, et) = setup_test_app().await;
    sqlx::query("UPDATE event_types SET timezone='Europe/Paris' WHERE id=?")
        .bind(&et)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE users SET timezone='Europe/Paris'")
        .execute(&pool)
        .await
        .unwrap();
    let day = Utc::now().date_naive() + Duration::days(3);
    sqlx::query("INSERT INTO bookings(id,event_type_id,uid,guest_name,guest_email,guest_timezone,start_at,end_at,cancel_token,reschedule_token) VALUES ('legacy',?,'legacy','Legacy','g@test.com','Europe/Paris',?,?,'cancel','resched')")
        .bind(&et).bind(format!("{day}T10:00:00Z")).bind(format!("{day}T10:30:00Z")).execute(&pool).await.unwrap();
    let html = body_string(
        app.oneshot(get_authed("/dashboard/bookings", &session))
            .await
            .unwrap(),
    )
    .await;
    assert!(
        html.contains("10:00 AM"),
        "Version 0 must retain naive interpretation, even with an old Z suffix: {html}"
    );
    let stored: (String, i64) =
        sqlx::query_as("SELECT start_at,time_version FROM bookings WHERE id='legacy'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(stored, (format!("{day}T10:00:00Z"), 0));
}

#[tokio::test]
async fn redteam_clock_rollback_cannot_hide_a_busy_booking() {
    let pool = setup_test_db().await;
    let (user, _, et) = seed_test_data(&pool).await;
    sqlx::query("UPDATE event_types SET timezone='Europe/Paris' WHERE id=?")
        .bind(&et)
        .execute(&pool)
        .await
        .unwrap();
    // 02:45 CEST -> 02:15 CET is a real 30-minute meeting, not an empty range.
    sqlx::query("INSERT INTO bookings(id,event_type_id,uid,guest_name,guest_email,guest_timezone,start_at,end_at,cancel_token,reschedule_token,time_version) VALUES ('fold',?,'fold','G','g@test.com','UTC','2026-10-25T00:45:00Z','2026-10-25T01:15:00Z','cancel','resched',1)").bind(&et).execute(&pool).await.unwrap();
    let from = crate::booking_time::naive("2026-10-25T02:00:00").unwrap();
    let to = crate::booking_time::naive("2026-10-25T03:30:00").unwrap();
    let tz = "Europe/Paris".parse().unwrap();
    let busy = fetch_busy_times_for_user(&pool, &user, from, to, tz, Some(&et)).await;
    assert!(
        has_conflict(
            &busy,
            from + Duration::minutes(50),
            from + Duration::minutes(55)
        ),
        "First 02:50 is occupied: {busy:?}"
    );
    assert!(
        has_conflict(
            &busy,
            from + Duration::minutes(5),
            from + Duration::minutes(10)
        ),
        "Second 02:05 is occupied: {busy:?}"
    );
}

#[tokio::test]
async fn redteam_clock_rollback_cannot_hide_a_resource_reservation() {
    let pool = setup_test_db().await;
    let (_, _, et) = seed_test_data(&pool).await;
    let resource = insert_resource(&pool, "DST resource").await;
    attach_resource(&pool, &et, &resource).await;
    sqlx::query("INSERT INTO bookings(id,event_type_id,uid,guest_name,guest_email,guest_timezone,start_at,end_at,cancel_token,reschedule_token,time_version) VALUES ('fold',?,'fold','G','g@test.com','UTC','2026-10-25T00:45:00Z','2026-10-25T01:15:00Z','cancel','resched',1)").bind(&et).execute(&pool).await.unwrap();
    let start = crate::booking_time::naive("2026-10-25T02:50:00").unwrap();
    let busy = crate::resources::busy_for_resource(
        &pool,
        &resource,
        start,
        start + Duration::minutes(5),
        "Europe/Paris".parse().unwrap(),
        None,
    )
    .await;
    assert!(
        has_conflict(&busy, start, start + Duration::minutes(5)),
        "Resource must not be double-booked: {busy:?}"
    );
}

#[test]
fn redteam_spring_slots_use_elapsed_duration_and_real_start_times() {
    let host: Tz = "Europe/Paris".parse().unwrap();
    let today = Utc::now().with_timezone(&host).date_naive();
    // Find a future spring transition Sunday, so this regression never expires.
    let year = today.year() + 1;
    let mut date = NaiveDate::from_ymd_opt(year, 3, 31).unwrap();
    while date.weekday() != chrono::Weekday::Sun {
        date -= Duration::days(1);
    }
    let offset = (date - today).num_days() as i32;
    let days = compute_slots_from_rules(
        &[(0, "01:00".into(), "05:00".into())],
        120,
        30,
        0,
        0,
        0,
        offset,
        1,
        None,
        host,
        Tz::UTC,
        BusySource::Individual(vec![]),
        &[],
    );
    let slots: Vec<_> = days.iter().flat_map(|d| &d.slots).collect();
    let slot = slots.iter().find(|s| s.host_time == "01:30").unwrap();
    assert_eq!((&slot.start[..], &slot.end[..]), ("00:30", "02:30"));
    assert!(slots.iter().all(|s| !s.host_time.starts_with("02:")));
}

#[tokio::test]
async fn review_timezone_query_errors_are_not_utc_defaults() {
    let pool = setup_test_db().await;
    let (_, _, et) = seed_test_data(&pool).await;
    sqlx::query("UPDATE event_types SET timezone='Europe/Paris' WHERE id=?")
        .bind(&et)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        crate::booking_time::event_timezone(&pool, &et)
            .await
            .unwrap(),
        "Europe/Paris".parse::<Tz>().unwrap()
    );
    sqlx::query("UPDATE event_types SET timezone='invalid' WHERE id=?")
        .bind(&et)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        crate::booking_time::event_timezone(&pool, &et)
            .await
            .unwrap(),
        Tz::UTC
    );
    pool.close().await;
    assert!(crate::booking_time::event_timezone(&pool, &et)
        .await
        .is_err());
    let start = Utc::now().naive_utc();
    assert!(
        crate::booking_time::period_counts(&pool, &et, start, start + Duration::days(1))
            .await
            .is_err()
    );
    assert!(
        crate::booking_time::legacy_slot_taken(&pool, &et, None, "2026-07-01T08:00:00Z", "")
            .await
            .is_err()
    );
}

#[tokio::test]
async fn review_frequency_query_errors_block_slots_and_member_selection() {
    // A broken count query and a broken limits query must both fail closed.
    for broken_column in [
        "bookings.time_version",
        "booking_frequency_limits.max_bookings",
    ] {
        let pool = setup_test_db().await;
        let (user, _, et) = seed_test_data(&pool).await;
        sqlx::query("UPDATE event_types SET timezone='UTC' WHERE id=?")
            .bind(&et)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO teams(id,name,slug,visibility) VALUES ('review-team','Review','review-team','public')").execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO team_members(team_id,user_id,role,source) VALUES ('review-team',?,'admin','direct')").bind(&user).execute(&pool).await.unwrap();
        sqlx::query("UPDATE event_types SET team_id='review-team' WHERE id=?")
            .bind(&et)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO booking_frequency_limits(id,event_type_id,max_bookings,period,per_member) VALUES ('review-limit',?,1,'day',1)").bind(&et).execute(&pool).await.unwrap();
        let start = (Utc::now().date_naive() + Duration::days(10))
            .and_hms_opt(10, 0, 0)
            .unwrap();
        let end = start + Duration::minutes(30);
        assert!(!would_exceed_frequency_limit(&pool, &et, start, Some(&user)).await);
        assert!(
            pick_group_member(&pool, "review-team", &et, start, end, start, 0, 0, Tz::UTC)
                .await
                .is_some()
        );
        let mut days = vec![SlotDay {
            date: start.date().to_string(),
            label: "Test day".into(),
            slots: vec![SlotTime {
                start: "10:00".into(),
                end: "10:30".into(),
                host_date: start.date().to_string(),
                host_time: "10:00".into(),
                guest_date: start.date().to_string(),
            }],
        }];
        apply_frequency_limit_filter(&pool, &et, &mut days).await;
        assert_eq!(days[0].slots.len(), 1);
        let (table, column) = broken_column.split_once('.').unwrap();
        sqlx::query(&format!(
            "ALTER TABLE {table} RENAME COLUMN {column} TO unavailable"
        ))
        .execute(&pool)
        .await
        .unwrap();
        assert!(
            would_exceed_frequency_limit(&pool, &et, start, Some(&user)).await,
            "{broken_column}"
        );
        apply_frequency_limit_filter(&pool, &et, &mut days).await;
        assert!(days[0].slots.is_empty(), "{broken_column}");
        assert!(
            pick_group_member(&pool, "review-team", &et, start, end, start, 0, 0, Tz::UTC)
                .await
                .is_none(),
            "{broken_column}"
        );
        if table == "bookings" {
            assert!(crate::booking_time::period_counts(&pool, &et, start, end)
                .await
                .is_err());
        }
    }
}
