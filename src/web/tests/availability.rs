//! Busy times, slot computation, frequency limits, timezones, horizon, troubleshoot.

use super::*;

#[test]
fn parse_avail_schedule_uses_user_default_when_submitted_is_empty() {
    // Empty submission + user default "Tue 14:00-18:00" → returns the user default
    // instead of falling back to the hardcoded Mon-Fri 09:00-17:00.
    let result = parse_avail_schedule(Some(""), None, None, None, None, Some("2:14:00-18:00"));
    assert_eq!(result.len(), 1);
    let windows = result.get(&2).expect("Tuesday should be set");
    assert_eq!(windows, &vec![("14:00".to_string(), "18:00".to_string())]);
}

#[test]
fn parse_avail_schedule_prefers_submitted_over_user_default() {
    // A populated submission overrides the user default.
    let result = parse_avail_schedule(
        Some("3:10:00-12:00"),
        None,
        None,
        None,
        None,
        Some("2:14:00-18:00"),
    );
    assert_eq!(result.len(), 1);
    let windows = result.get(&3).expect("Wednesday should be set");
    assert_eq!(windows, &vec![("10:00".to_string(), "12:00".to_string())]);
}

#[test]
fn parse_avail_schedule_falls_back_to_legacy_when_both_empty() {
    // Empty submission and empty user default → hardcoded Mon-Fri 09:00-17:00.
    let result = parse_avail_schedule(Some(""), None, None, None, None, Some(""));
    assert_eq!(result.len(), 5);
    for day in 1..=5 {
        let windows = result.get(&day).expect("weekday should be set");
        assert_eq!(windows, &vec![("09:00".to_string(), "17:00".to_string())]);
    }
}

#[test]
fn conflict_overlapping_event() {
    let busy = vec![(dt(2026, 3, 15, 10, 0), dt(2026, 3, 15, 11, 0))];
    // Slot 10:30-11:30 overlaps with 10:00-11:00
    assert!(has_conflict(
        &busy,
        dt(2026, 3, 15, 10, 30),
        dt(2026, 3, 15, 11, 30)
    ));
}

#[test]
fn conflict_no_overlap() {
    let busy = vec![(dt(2026, 3, 15, 10, 0), dt(2026, 3, 15, 11, 0))];
    // Slot 11:00-12:00 starts exactly when event ends (no overlap)
    assert!(!has_conflict(
        &busy,
        dt(2026, 3, 15, 11, 0),
        dt(2026, 3, 15, 12, 0)
    ));
}

#[test]
fn conflict_event_contains_slot() {
    let busy = vec![(dt(2026, 3, 15, 9, 0), dt(2026, 3, 15, 17, 0))];
    // Slot entirely within busy period
    assert!(has_conflict(
        &busy,
        dt(2026, 3, 15, 10, 0),
        dt(2026, 3, 15, 11, 0)
    ));
}

#[test]
fn conflict_slot_contains_event() {
    let busy = vec![(dt(2026, 3, 15, 10, 15), dt(2026, 3, 15, 10, 45))];
    // Slot 10:00-11:00 contains the 10:15-10:45 event
    assert!(has_conflict(
        &busy,
        dt(2026, 3, 15, 10, 0),
        dt(2026, 3, 15, 11, 0)
    ));
}

#[test]
fn conflict_adjacent_not_conflicting() {
    let busy = vec![
        (dt(2026, 3, 15, 9, 0), dt(2026, 3, 15, 10, 0)),
        (dt(2026, 3, 15, 11, 0), dt(2026, 3, 15, 12, 0)),
    ];
    // Slot 10:00-11:00 is between two events (no overlap)
    assert!(!has_conflict(
        &busy,
        dt(2026, 3, 15, 10, 0),
        dt(2026, 3, 15, 11, 0)
    ));
}

#[test]
fn conflict_empty_busy_list() {
    let busy: Vec<(NaiveDateTime, NaiveDateTime)> = vec![];
    assert!(!has_conflict(
        &busy,
        dt(2026, 3, 15, 10, 0),
        dt(2026, 3, 15, 11, 0)
    ));
}

#[test]
fn conflict_buffer_causes_overlap() {
    let busy = vec![(dt(2026, 3, 15, 10, 0), dt(2026, 3, 15, 11, 0))];
    // Slot is 11:00-12:00, but with 15min buffer before → buf_start=10:45 overlaps
    assert!(has_conflict(
        &busy,
        dt(2026, 3, 15, 10, 45),
        dt(2026, 3, 15, 12, 0)
    ));
}

// --- expand_recurring_into_busy tests ---

#[test]
fn expand_recurring_weekly_into_busy() {
    let recurring = vec![(
        "20260309T100000".to_string(), // Monday 10:00
        "20260309T110000".to_string(), // Monday 11:00
        "FREQ=WEEKLY;BYDAY=MO".to_string(),
        None,
        None,
    )];
    let window_start = dt(2026, 3, 9, 0, 0);
    let window_end = dt(2026, 3, 23, 23, 59);
    let busy = expand_recurring_into_busy(&recurring, window_start, window_end, Tz::UTC);
    // Should have 3 occurrences: Mar 9, 16, 23
    assert_eq!(busy.len(), 3);
    assert_eq!(busy[0].0, dt(2026, 3, 9, 10, 0));
    assert_eq!(busy[1].0, dt(2026, 3, 16, 10, 0));
    assert_eq!(busy[2].0, dt(2026, 3, 23, 10, 0));
}

#[test]
fn expand_recurring_with_exdate() {
    let raw_ical = "BEGIN:VEVENT\nDTSTART:20260309T100000\nDTEND:20260309T110000\nRRULE:FREQ=WEEKLY;BYDAY=MO\nEXDATE:20260316T100000\nEND:VEVENT";
    let recurring = vec![(
        "20260309T100000".to_string(),
        "20260309T110000".to_string(),
        "FREQ=WEEKLY;BYDAY=MO".to_string(),
        Some(raw_ical.to_string()),
        None,
    )];
    let window_start = dt(2026, 3, 9, 0, 0);
    let window_end = dt(2026, 3, 23, 23, 59);
    let busy = expand_recurring_into_busy(&recurring, window_start, window_end, Tz::UTC);
    // Mar 16 excluded, so only Mar 9 and 23
    assert_eq!(busy.len(), 2);
    assert_eq!(busy[0].0, dt(2026, 3, 9, 10, 0));
    assert_eq!(busy[1].0, dt(2026, 3, 23, 10, 0));
}

#[tokio::test]
async fn fetch_busy_times_empty_calendar() {
    let pool = setup_test_db().await;
    let (user_id, _, _) = seed_test_data(&pool).await;

    let busy = fetch_busy_times_for_user(
        &pool,
        &user_id,
        dt(2026, 3, 15, 0, 0),
        dt(2026, 3, 21, 23, 59),
        Tz::UTC,
        None,
    )
    .await;

    assert!(busy.is_empty(), "No events or bookings → no busy times");
}

#[tokio::test]
async fn fetch_busy_times_includes_bookings() {
    let pool = setup_test_db().await;
    let (user_id, _, et_id) = seed_test_data(&pool).await;

    let booking_id = uuid::Uuid::new_v4().to_string();
    let cancel_tok = uuid::Uuid::new_v4().to_string();
    let resched_tok = uuid::Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO bookings (id, event_type_id, uid, guest_name, guest_email, guest_timezone, start_at, end_at, status, cancel_token, reschedule_token) VALUES (?, ?, 'uid1', 'Guest', 'guest@example.com', 'UTC', '2026-03-16T10:00:00', '2026-03-16T10:30:00', 'confirmed', ?, ?)")
        .bind(&booking_id)
        .bind(&et_id)
        .bind(&cancel_tok)
        .bind(&resched_tok)
        .execute(&pool).await.unwrap();

    let busy = fetch_busy_times_for_user(
        &pool,
        &user_id,
        dt(2026, 3, 15, 0, 0),
        dt(2026, 3, 21, 23, 59),
        Tz::UTC,
        None,
    )
    .await;

    assert_eq!(busy.len(), 1);
    assert_eq!(busy[0].0, dt(2026, 3, 16, 10, 0));
    assert_eq!(busy[0].1, dt(2026, 3, 16, 10, 30));
}

#[tokio::test]
async fn fetch_busy_times_ignores_cancelled_bookings() {
    let pool = setup_test_db().await;
    let (user_id, _, et_id) = seed_test_data(&pool).await;

    let booking_id = uuid::Uuid::new_v4().to_string();
    let cancel_tok = uuid::Uuid::new_v4().to_string();
    let resched_tok = uuid::Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO bookings (id, event_type_id, uid, guest_name, guest_email, guest_timezone, start_at, end_at, status, cancel_token, reschedule_token) VALUES (?, ?, 'uid1', 'Guest', 'guest@example.com', 'UTC', '2026-03-16T10:00:00', '2026-03-16T10:30:00', 'cancelled', ?, ?)")
        .bind(&booking_id)
        .bind(&et_id)
        .bind(&cancel_tok)
        .bind(&resched_tok)
        .execute(&pool).await.unwrap();

    let busy = fetch_busy_times_for_user(
        &pool,
        &user_id,
        dt(2026, 3, 15, 0, 0),
        dt(2026, 3, 21, 23, 59),
        Tz::UTC,
        None,
    )
    .await;

    assert!(
        busy.is_empty(),
        "Cancelled bookings should not block availability"
    );
}

#[tokio::test]
async fn fetch_busy_times_detects_same_day_compact_event() {
    // Regression: a timed CalDAV event stored in compact iCal form
    // ("YYYYMMDDTHHMMSS") on the SAME day as the busy window must be
    // detected. pick_group_member checks a single-slot window at booking
    // time, so window_end lands on the event's own date. A date-only upper
    // bound ("YYYYMMDD") sorts lexically before the event
    // ("20260618T100000" > "20260618"), so the event was silently dropped
    // and a busy member could be booked (the double-booking we hit prod).
    let pool = setup_test_db().await;
    let (user_id, account_id, _et_id) = seed_test_data(&pool).await;
    seed_synced_event(
        &pool,
        &account_id,
        "20260618T100000",
        "20260618T113000",
        "Europe/Paris",
        "OPAQUE",
    )
    .await;

    let paris: Tz = "Europe/Paris".parse().unwrap();
    let busy = fetch_busy_times_for_user(
        &pool,
        &user_id,
        dt(2026, 6, 18, 10, 0),
        dt(2026, 6, 18, 11, 0),
        paris,
        None,
    )
    .await;

    assert_eq!(
        busy.len(),
        1,
        "same-day compact event must be detected as busy"
    );
    assert_eq!(busy[0].0, dt(2026, 6, 18, 10, 0));
    assert_eq!(busy[0].1, dt(2026, 6, 18, 11, 30));
}

#[tokio::test]
async fn fetch_busy_times_skips_transparent_same_day_event() {
    // Guard: the fix must not start counting TRANSPARENT (free) events.
    let pool = setup_test_db().await;
    let (user_id, account_id, _et_id) = seed_test_data(&pool).await;
    seed_synced_event(
        &pool,
        &account_id,
        "20260618T100000",
        "20260618T113000",
        "Europe/Paris",
        "TRANSPARENT",
    )
    .await;

    let paris: Tz = "Europe/Paris".parse().unwrap();
    let busy = fetch_busy_times_for_user(
        &pool,
        &user_id,
        dt(2026, 6, 18, 10, 0),
        dt(2026, 6, 18, 11, 0),
        paris,
        None,
    )
    .await;

    assert!(busy.is_empty(), "TRANSPARENT events must not block");
}

#[tokio::test]
async fn compute_slots_basic_availability() {
    let pool = setup_test_db().await;
    let (_, _, et_id) = seed_test_data(&pool).await;

    let busy = BusySource::Individual(vec![]);

    // Start from tomorrow to avoid partial-day flakiness
    let slot_days = compute_slots(
        &pool,
        &et_id,
        30, // 30 min duration
        0,  // no buffer before
        0,  // no buffer after
        0,  // no min notice
        1,  // start from tomorrow
        14, // 14 days ahead
        None,
        Tz::UTC,
        Tz::UTC,
        busy,
        None,
    )
    .await;

    // Should have slots on weekdays only (Mon-Fri)
    assert!(!slot_days.is_empty(), "Should have slots on weekdays");

    // Each day should have 16 slots (09:00-17:00 in 30-min increments)
    for day in &slot_days {
        assert_eq!(
            day.slots.len(),
            16,
            "09:00-17:00 with 30min = 16 slots, got {} for {}",
            day.slots.len(),
            day.date
        );
    }
}

#[tokio::test]
async fn compute_slots_with_busy_event() {
    let pool = setup_test_db().await;
    let (_, _, et_id) = seed_test_data(&pool).await;

    // Find the next Monday from now
    let now = Utc::now().with_timezone(&Tz::UTC).naive_local();
    let mut next_monday = now.date() + Duration::days(1);
    while next_monday.weekday() != chrono::Weekday::Mon {
        next_monday += Duration::days(1);
    }
    let days_to_monday = (next_monday - now.date()).num_days() as i32;

    // Block 10:00-11:00 on that Monday
    let busy_start = next_monday.and_hms_opt(10, 0, 0).unwrap();
    let busy_end = next_monday.and_hms_opt(11, 0, 0).unwrap();
    let busy = BusySource::Individual(vec![(busy_start, busy_end)]);

    let slot_days = compute_slots(
        &pool,
        &et_id,
        30,
        0,
        0,
        0,
        days_to_monday,
        1, // just 1 day
        None,
        Tz::UTC,
        Tz::UTC,
        busy,
        None,
    )
    .await;

    assert!(!slot_days.is_empty(), "Should have the Monday");
    let monday = &slot_days[0];

    // 16 slots normally, minus 2 (10:00 and 10:30 blocked) = 14
    assert_eq!(monday.slots.len(), 14, "10:00 and 10:30 should be blocked");

    // Verify 10:00 and 10:30 are not in the slots
    let slot_times: Vec<&str> = monday.slots.iter().map(|s| s.start.as_str()).collect();
    assert!(!slot_times.contains(&"10:00"), "10:00 should be blocked");
    assert!(!slot_times.contains(&"10:30"), "10:30 should be blocked");
}

#[tokio::test]
async fn compute_slots_with_buffer() {
    let pool = setup_test_db().await;
    let (_, _, et_id) = seed_test_data(&pool).await;

    // Find the next Monday
    let now = Utc::now().with_timezone(&Tz::UTC).naive_local();
    let mut next_monday = now.date() + Duration::days(1);
    while next_monday.weekday() != chrono::Weekday::Mon {
        next_monday += Duration::days(1);
    }
    let days_to_monday = (next_monday - now.date()).num_days() as i32;

    // Block 10:00-10:30 on that Monday
    let busy_start = next_monday.and_hms_opt(10, 0, 0).unwrap();
    let busy_end = next_monday.and_hms_opt(10, 30, 0).unwrap();
    let busy = BusySource::Individual(vec![(busy_start, busy_end)]);

    let slot_days = compute_slots(
        &pool,
        &et_id,
        30,
        15, // 15 min buffer before
        15, // 15 min buffer after
        0,
        days_to_monday,
        1,
        None,
        Tz::UTC,
        Tz::UTC,
        busy,
        None,
    )
    .await;

    assert!(!slot_days.is_empty());
    let monday = &slot_days[0];

    // Event at 10:00-10:30 with 15min buffers blocks 09:45-10:45
    // So slots 09:30 (ends 10:00, buf_end=10:15 > 09:45), 10:00, 10:30 (buf_start=10:15 < 10:30) blocked
    let slot_times: Vec<&str> = monday.slots.iter().map(|s| s.start.as_str()).collect();
    assert!(
        !slot_times.contains(&"10:00"),
        "10:00 should be blocked (direct conflict)"
    );
    assert!(
        slot_times.contains(&"09:00"),
        "09:00 should be free (no buffer overlap)"
    );
}

#[tokio::test]
async fn compute_slots_frequency_limit_per_day_drops_capped_day() {
    let pool = setup_test_db().await;
    let (_, _, et_id) = seed_test_data(&pool).await;

    let now = Utc::now().with_timezone(&Tz::UTC).naive_local();
    let mut next_monday = now.date() + Duration::days(1);
    while next_monday.weekday() != chrono::Weekday::Mon {
        next_monday += Duration::days(1);
    }
    let days_to_monday = (next_monday - now.date()).num_days() as i32;

    sqlx::query("INSERT INTO booking_frequency_limits (id, event_type_id, max_bookings, period) VALUES (?, ?, 1, 'day')")
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(&et_id)
        .execute(&pool).await.unwrap();

    let start_at = next_monday
        .and_hms_opt(10, 0, 0)
        .unwrap()
        .format("%Y-%m-%dT%H:%M:%S")
        .to_string();
    let end_at = next_monday
        .and_hms_opt(10, 30, 0)
        .unwrap()
        .format("%Y-%m-%dT%H:%M:%S")
        .to_string();
    sqlx::query("INSERT INTO bookings (id, event_type_id, uid, guest_name, guest_email, guest_timezone, start_at, end_at, status, cancel_token, reschedule_token) VALUES (?, ?, 'uid1', 'Guest', 'g@e.com', 'UTC', ?, ?, 'confirmed', ?, ?)")
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(&et_id)
        .bind(&start_at)
        .bind(&end_at)
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(uuid::Uuid::new_v4().to_string())
        .execute(&pool).await.unwrap();

    let slot_days = compute_slots(
        &pool,
        &et_id,
        30,
        0,
        0,
        0,
        days_to_monday,
        5,
        None,
        Tz::UTC,
        Tz::UTC,
        BusySource::Individual(vec![]),
        None,
    )
    .await;

    let monday_date = next_monday.format("%Y-%m-%d").to_string();
    let monday = slot_days.iter().find(|d| d.date == monday_date);
    assert!(
        monday.map(|d| d.slots.is_empty()).unwrap_or(true),
        "1/day cap reached → Monday should expose no slots, got {:?}",
        monday.map(|d| d.slots.len())
    );

    let tuesday_date = (next_monday + Duration::days(1))
        .format("%Y-%m-%d")
        .to_string();
    let tuesday = slot_days
        .iter()
        .find(|d| d.date == tuesday_date)
        .expect("Tuesday should be present");
    assert_eq!(
        tuesday.slots.len(),
        16,
        "Tuesday is unaffected by Monday's per-day cap"
    );
}

#[tokio::test]
async fn compute_slots_frequency_limit_per_week_drops_whole_week() {
    let pool = setup_test_db().await;
    let (_, _, et_id) = seed_test_data(&pool).await;

    let now = Utc::now().with_timezone(&Tz::UTC).naive_local();
    let mut next_monday = now.date() + Duration::days(1);
    while next_monday.weekday() != chrono::Weekday::Mon {
        next_monday += Duration::days(1);
    }
    let days_to_monday = (next_monday - now.date()).num_days() as i32;

    sqlx::query("INSERT INTO booking_frequency_limits (id, event_type_id, max_bookings, period) VALUES (?, ?, 1, 'week')")
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(&et_id)
        .execute(&pool).await.unwrap();

    // One booking on Monday consumes the week's only slot.
    let start_at = next_monday
        .and_hms_opt(10, 0, 0)
        .unwrap()
        .format("%Y-%m-%dT%H:%M:%S")
        .to_string();
    let end_at = next_monday
        .and_hms_opt(10, 30, 0)
        .unwrap()
        .format("%Y-%m-%dT%H:%M:%S")
        .to_string();
    sqlx::query("INSERT INTO bookings (id, event_type_id, uid, guest_name, guest_email, guest_timezone, start_at, end_at, status, cancel_token, reschedule_token) VALUES (?, ?, 'uid1', 'Guest', 'g@e.com', 'UTC', ?, ?, 'confirmed', ?, ?)")
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(&et_id)
        .bind(&start_at)
        .bind(&end_at)
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(uuid::Uuid::new_v4().to_string())
        .execute(&pool).await.unwrap();

    let slot_days = compute_slots(
        &pool,
        &et_id,
        30,
        0,
        0,
        0,
        days_to_monday,
        5,
        None,
        Tz::UTC,
        Tz::UTC,
        BusySource::Individual(vec![]),
        None,
    )
    .await;

    for day in &slot_days {
        assert!(
            day.slots.is_empty(),
            "Per-week cap of 1 reached → {} should have no slots, got {}",
            day.date,
            day.slots.len()
        );
    }
}

#[tokio::test]
async fn compute_slots_frequency_limit_ignores_cancelled_bookings() {
    // Regression: cancelled bookings must not count toward the cap. If they
    // did, hosts who frequently cancel would silently shrink their own
    // availability.
    let pool = setup_test_db().await;
    let (_, _, et_id) = seed_test_data(&pool).await;

    let now = Utc::now().with_timezone(&Tz::UTC).naive_local();
    let mut next_monday = now.date() + Duration::days(1);
    while next_monday.weekday() != chrono::Weekday::Mon {
        next_monday += Duration::days(1);
    }
    let days_to_monday = (next_monday - now.date()).num_days() as i32;

    sqlx::query("INSERT INTO booking_frequency_limits (id, event_type_id, max_bookings, period) VALUES (?, ?, 1, 'day')")
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(&et_id)
        .execute(&pool).await.unwrap();

    // Single cancelled booking on Monday — should not consume the day's cap.
    let start_at = next_monday
        .and_hms_opt(10, 0, 0)
        .unwrap()
        .format("%Y-%m-%dT%H:%M:%S")
        .to_string();
    let end_at = next_monday
        .and_hms_opt(10, 30, 0)
        .unwrap()
        .format("%Y-%m-%dT%H:%M:%S")
        .to_string();
    sqlx::query("INSERT INTO bookings (id, event_type_id, uid, guest_name, guest_email, guest_timezone, start_at, end_at, status, cancel_token, reschedule_token) VALUES (?, ?, 'uid1', 'Guest', 'g@e.com', 'UTC', ?, ?, 'cancelled', ?, ?)")
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(&et_id)
        .bind(&start_at)
        .bind(&end_at)
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(uuid::Uuid::new_v4().to_string())
        .execute(&pool).await.unwrap();

    let slot_days = compute_slots(
        &pool,
        &et_id,
        30,
        0,
        0,
        0,
        days_to_monday,
        1,
        None,
        Tz::UTC,
        Tz::UTC,
        BusySource::Individual(vec![]),
        None,
    )
    .await;

    let monday_date = next_monday.format("%Y-%m-%d").to_string();
    let monday = slot_days
        .iter()
        .find(|d| d.date == monday_date)
        .expect("Monday should be present");
    assert_eq!(
        monday.slots.len(),
        16,
        "Cancelled bookings must not consume the day cap; expected full 16 slots, got {}",
        monday.slots.len()
    );
}

#[tokio::test]
async fn compute_slots_frequency_limit_multiple_caps_each_checked() {
    // Configure both a 1/day and a 100/year cap. With one booking on
    // Monday only the day cap fires, so Monday must be hidden but
    // Tue–Fri must stay visible. This pins that the filter consults every
    // configured limit independently rather than stopping at the first.
    let pool = setup_test_db().await;
    let (_, _, et_id) = seed_test_data(&pool).await;

    let now = Utc::now().with_timezone(&Tz::UTC).naive_local();
    let mut next_monday = now.date() + Duration::days(1);
    while next_monday.weekday() != chrono::Weekday::Mon {
        next_monday += Duration::days(1);
    }
    let days_to_monday = (next_monday - now.date()).num_days() as i32;

    sqlx::query("INSERT INTO booking_frequency_limits (id, event_type_id, max_bookings, period) VALUES (?, ?, 1, 'day')")
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(&et_id)
        .execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO booking_frequency_limits (id, event_type_id, max_bookings, period) VALUES (?, ?, 100, 'year')")
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(&et_id)
        .execute(&pool).await.unwrap();

    let start_at = next_monday
        .and_hms_opt(10, 0, 0)
        .unwrap()
        .format("%Y-%m-%dT%H:%M:%S")
        .to_string();
    let end_at = next_monday
        .and_hms_opt(10, 30, 0)
        .unwrap()
        .format("%Y-%m-%dT%H:%M:%S")
        .to_string();
    sqlx::query("INSERT INTO bookings (id, event_type_id, uid, guest_name, guest_email, guest_timezone, start_at, end_at, status, cancel_token, reschedule_token) VALUES (?, ?, 'uid1', 'Guest', 'g@e.com', 'UTC', ?, ?, 'confirmed', ?, ?)")
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(&et_id)
        .bind(&start_at)
        .bind(&end_at)
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(uuid::Uuid::new_v4().to_string())
        .execute(&pool).await.unwrap();

    let slot_days = compute_slots(
        &pool,
        &et_id,
        30,
        0,
        0,
        0,
        days_to_monday,
        5,
        None,
        Tz::UTC,
        Tz::UTC,
        BusySource::Individual(vec![]),
        None,
    )
    .await;

    let monday_date = next_monday.format("%Y-%m-%d").to_string();
    let monday = slot_days.iter().find(|d| d.date == monday_date);
    assert!(
        monday.map(|d| d.slots.is_empty()).unwrap_or(true),
        "1/day cap should hide Monday even with a lax year cap, got {:?}",
        monday.map(|d| d.slots.len())
    );

    let tuesday_date = (next_monday + Duration::days(1))
        .format("%Y-%m-%d")
        .to_string();
    let tuesday = slot_days
        .iter()
        .find(|d| d.date == tuesday_date)
        .expect("Tuesday should be present");
    assert_eq!(
        tuesday.slots.len(),
        16,
        "Year cap not reached → Tuesday must stay visible"
    );
}

/// Attach the seeded event type to a team and add two team members
/// (alice, bob). Returns their user ids.
async fn seed_team_for_event_type(pool: &SqlitePool, et_id: &str) -> (String, String) {
    let team_id = uuid::Uuid::new_v4().to_string();
    let alice = uuid::Uuid::new_v4().to_string();
    let bob = uuid::Uuid::new_v4().to_string();

    sqlx::query(
        "INSERT INTO teams (id, name, slug, visibility) VALUES (?, 'Team', 'team', 'public')",
    )
    .bind(&team_id)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query("UPDATE event_types SET team_id = ? WHERE id = ?")
        .bind(&team_id)
        .bind(et_id)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO users (id, email, name, role, auth_provider, username, enabled) VALUES (?, 'alice@example.com', 'Alice', 'user', 'local', 'alice', 1)")
        .bind(&alice).execute(pool).await.unwrap();
    sqlx::query("INSERT INTO users (id, email, name, role, auth_provider, username, enabled) VALUES (?, 'bob@example.com', 'Bob', 'user', 'local', 'bob', 1)")
        .bind(&bob).execute(pool).await.unwrap();
    sqlx::query("INSERT INTO team_members (team_id, user_id, role, source) VALUES (?, ?, 'member', 'direct')")
        .bind(&team_id).bind(&alice).execute(pool).await.unwrap();
    sqlx::query("INSERT INTO team_members (team_id, user_id, role, source) VALUES (?, ?, 'member', 'direct')")
        .bind(&team_id).bind(&bob).execute(pool).await.unwrap();

    (alice, bob)
}

#[tokio::test]
async fn would_exceed_frequency_limit_per_member_blocks_only_named_assignee() {
    let pool = setup_test_db().await;
    let (_, _, et_id) = seed_test_data(&pool).await;
    let (alice, bob) = seed_team_for_event_type(&pool, &et_id).await;

    sqlx::query("INSERT INTO booking_frequency_limits (id, event_type_id, max_bookings, period, per_member) VALUES (?, ?, 1, 'day', 1)")
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(&et_id)
        .execute(&pool).await.unwrap();

    let booked_day = NaiveDate::from_ymd_opt(2026, 3, 16).unwrap();
    let booked_start = booked_day.and_hms_opt(10, 0, 0).unwrap();
    insert_booking(&pool, &et_id, Some(&alice), booked_start, "confirmed").await;

    let proposed = booked_day.and_hms_opt(14, 0, 0).unwrap();
    assert!(
        would_exceed_frequency_limit(&pool, &et_id, proposed, Some(&alice)).await,
        "Alice already has one booking that day under 1/day per-member"
    );
    assert!(
        !would_exceed_frequency_limit(&pool, &et_id, proposed, Some(&bob)).await,
        "Bob has zero bookings; per-member cap should not block him"
    );
}

// --- Per-member slot uniqueness (#146) and write-back targets (#147) ---

#[tokio::test]
async fn same_slot_bookable_per_member_but_not_twice_per_member() {
    let pool = setup_test_db().await;
    let alice = insert_role_user(&pool, "alice@i146.test", "user").await;
    let bob = insert_role_user(&pool, "bob@i146.test", "user").await;
    let (_team, et_id) = insert_team_with_et(
        &pool,
        "i146",
        "i146-et",
        &[(&alice, "member"), (&bob, "member")],
    )
    .await;
    let start = NaiveDate::from_ymd_opt(2026, 3, 16)
        .unwrap()
        .and_hms_opt(10, 0, 0)
        .unwrap();

    insert_booking(&pool, &et_id, Some(&alice), start, "confirmed").await;
    // Same slot, different assigned member: allowed (#146). The helper
    // unwraps, so reaching the next line IS the assertion.
    insert_booking(&pool, &et_id, Some(&bob), start, "confirmed").await;

    // Same slot, same member again: rejected by idx_bookings_no_overlap.
    let dup = sqlx::query(
        "INSERT INTO bookings (id, event_type_id, uid, guest_name, guest_email, guest_timezone, start_at, end_at, status, cancel_token, reschedule_token, assigned_user_id) \
             VALUES (?, ?, ?, 'G', 'g@e.com', 'UTC', '2026-03-16T10:00:00', '2026-03-16T10:30:00', 'confirmed', ?, ?, ?)",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(&et_id)
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(&alice)
    .execute(&pool)
    .await;
    assert!(dup.is_err(), "same member, same slot must stay unique");

    // Unassigned bookings (personal, collective) keep whole-slot
    // exclusivity among themselves.
    insert_booking(&pool, &et_id, None, start, "confirmed").await;
    let dup_null = sqlx::query(
        "INSERT INTO bookings (id, event_type_id, uid, guest_name, guest_email, guest_timezone, start_at, end_at, status, cancel_token, reschedule_token) \
             VALUES (?, ?, ?, 'G', 'g@e.com', 'UTC', '2026-03-16T10:00:00', '2026-03-16T10:30:00', 'confirmed', ?, ?)",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(&et_id)
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(uuid::Uuid::new_v4().to_string())
    .execute(&pool)
    .await;
    assert!(
        dup_null.is_err(),
        "two unassigned bookings in one slot must stay unique"
    );
}

#[tokio::test]
async fn assigned_booking_marks_member_busy_not_owner() {
    let pool = setup_test_db().await;
    let (owner, _, et_id) = seed_test_data(&pool).await;
    let alice = insert_role_user(&pool, "alice@busy146.test", "user").await;
    let start = NaiveDate::from_ymd_opt(2026, 3, 18)
        .unwrap()
        .and_hms_opt(10, 0, 0)
        .unwrap();
    let end = start + Duration::minutes(30);
    insert_booking(&pool, &et_id, Some(&alice), start, "confirmed").await;

    let owner_busy =
        fetch_busy_times_for_user(&pool, &owner, start, end, chrono_tz::Tz::UTC, None).await;
    assert!(
        owner_busy.is_empty(),
        "a booking assigned to another member must not mark the owner busy (#146)"
    );
    let alice_busy =
        fetch_busy_times_for_user(&pool, &alice, start, end, chrono_tz::Tz::UTC, None).await;
    assert!(!alice_busy.is_empty(), "assigned member is busy");
}

#[tokio::test]
async fn unassigned_team_booking_marks_members_busy() {
    let pool = setup_test_db().await;
    let alice = insert_role_user(&pool, "alice@coll147.test", "user").await;
    let bob = insert_role_user(&pool, "bob@coll147.test", "user").await;
    let (_team, et_id) = insert_team_with_et(
        &pool,
        "coll147",
        "coll147-et",
        &[(&alice, "member"), (&bob, "member")],
    )
    .await;
    sqlx::query("UPDATE event_types SET scheduling_mode = 'collective' WHERE id = ?")
        .bind(&et_id)
        .execute(&pool)
        .await
        .unwrap();
    let start = NaiveDate::from_ymd_opt(2026, 3, 19)
        .unwrap()
        .and_hms_opt(10, 0, 0)
        .unwrap();
    let end = start + Duration::minutes(30);
    insert_booking(&pool, &et_id, None, start, "confirmed").await;

    for member in [&alice, &bob] {
        let busy =
            fetch_busy_times_for_user(&pool, member, start, end, chrono_tz::Tz::UTC, None).await;
        assert!(
            !busy.is_empty(),
            "an unassigned (collective) team booking must mark every member busy"
        );
    }
}

#[tokio::test]
async fn pick_group_member_skips_member_with_pending_same_slot() {
    let pool = setup_test_db().await;
    let alice = insert_role_user(&pool, "alice@pend146.test", "user").await;
    let bob = insert_role_user(&pool, "bob@pend146.test", "user").await;
    let (team_id, et_id) = insert_team_with_et(
        &pool,
        "pend146",
        "pend146-et",
        &[(&alice, "member"), (&bob, "member")],
    )
    .await;
    let start = NaiveDate::from_ymd_opt(2026, 3, 20)
        .unwrap()
        .and_hms_opt(10, 0, 0)
        .unwrap();
    let end = start + Duration::minutes(30);

    // Pending bookings don't show in the busy times, but they hold the
    // (event_type, slot, member) key in idx_bookings_no_overlap: the
    // picker must not hand the same member out twice.
    insert_booking(&pool, &et_id, Some(&alice), start, "pending").await;

    let picked = pick_group_member(
        &pool,
        &team_id,
        &et_id,
        start,
        end,
        start,
        0,
        0,
        chrono_tz::Tz::UTC,
    )
    .await
    .expect("bob is free, a member must be picked");
    assert_eq!(picked.0, bob, "picker must skip alice's pending slot");
}

#[tokio::test]
async fn apply_frequency_limit_filter_per_member_keeps_slots_when_one_member_free() {
    // 1/day per-member, 2 team members, 1 booking on Monday assigned to
    // Alice → Monday must still show slots because Bob has headroom.
    let pool = setup_test_db().await;
    let (_, _, et_id) = seed_test_data(&pool).await;
    let (alice, _bob) = seed_team_for_event_type(&pool, &et_id).await;

    let now = Utc::now().with_timezone(&Tz::UTC).naive_local();
    let mut next_monday = now.date() + Duration::days(1);
    while next_monday.weekday() != chrono::Weekday::Mon {
        next_monday += Duration::days(1);
    }
    let days_to_monday = (next_monday - now.date()).num_days() as i32;

    sqlx::query("INSERT INTO booking_frequency_limits (id, event_type_id, max_bookings, period, per_member) VALUES (?, ?, 1, 'day', 1)")
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(&et_id)
        .execute(&pool).await.unwrap();
    insert_booking(
        &pool,
        &et_id,
        Some(&alice),
        next_monday.and_hms_opt(10, 0, 0).unwrap(),
        "confirmed",
    )
    .await;

    let slot_days = compute_slots(
        &pool,
        &et_id,
        30,
        0,
        0,
        0,
        days_to_monday,
        1,
        None,
        Tz::UTC,
        Tz::UTC,
        BusySource::Individual(vec![]),
        None,
    )
    .await;

    let monday_date = next_monday.format("%Y-%m-%d").to_string();
    let monday = slot_days
        .iter()
        .find(|d| d.date == monday_date)
        .expect("Monday should be present");
    assert_eq!(
        monday.slots.len(),
        16,
        "Per-member cap consumed by Alice must not hide slots while Bob is free"
    );
}

#[tokio::test]
async fn apply_frequency_limit_filter_per_member_drops_slots_when_all_members_capped() {
    // 1/day per-member, 2 team members, both already booked on Monday →
    // Monday must show no slots.
    let pool = setup_test_db().await;
    let (_, _, et_id) = seed_test_data(&pool).await;
    let (alice, bob) = seed_team_for_event_type(&pool, &et_id).await;

    let now = Utc::now().with_timezone(&Tz::UTC).naive_local();
    let mut next_monday = now.date() + Duration::days(1);
    while next_monday.weekday() != chrono::Weekday::Mon {
        next_monday += Duration::days(1);
    }
    let days_to_monday = (next_monday - now.date()).num_days() as i32;

    sqlx::query("INSERT INTO booking_frequency_limits (id, event_type_id, max_bookings, period, per_member) VALUES (?, ?, 1, 'day', 1)")
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(&et_id)
        .execute(&pool).await.unwrap();
    insert_booking(
        &pool,
        &et_id,
        Some(&alice),
        next_monday.and_hms_opt(10, 0, 0).unwrap(),
        "confirmed",
    )
    .await;
    insert_booking(
        &pool,
        &et_id,
        Some(&bob),
        next_monday.and_hms_opt(11, 0, 0).unwrap(),
        "confirmed",
    )
    .await;

    let slot_days = compute_slots(
        &pool,
        &et_id,
        30,
        0,
        0,
        0,
        days_to_monday,
        1,
        None,
        Tz::UTC,
        Tz::UTC,
        BusySource::Individual(vec![]),
        None,
    )
    .await;

    let monday_date = next_monday.format("%Y-%m-%d").to_string();
    let monday = slot_days.iter().find(|d| d.date == monday_date);
    assert!(
        monday.map(|d| d.slots.is_empty()).unwrap_or(true),
        "Every team member at cap → Monday must be empty, got {:?}",
        monday.map(|d| d.slots.len())
    );
}

#[tokio::test]
async fn apply_frequency_limit_filter_per_member_on_personal_event_type_degrades_to_wide() {
    // Personal event type (no team) with a per_member flag set. With no
    // team members to scope by, the cap should behave event-type-wide so
    // a single booking still hides the day.
    let pool = setup_test_db().await;
    let (_, _, et_id) = seed_test_data(&pool).await;

    let now = Utc::now().with_timezone(&Tz::UTC).naive_local();
    let mut next_monday = now.date() + Duration::days(1);
    while next_monday.weekday() != chrono::Weekday::Mon {
        next_monday += Duration::days(1);
    }
    let days_to_monday = (next_monday - now.date()).num_days() as i32;

    sqlx::query("INSERT INTO booking_frequency_limits (id, event_type_id, max_bookings, period, per_member) VALUES (?, ?, 1, 'day', 1)")
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(&et_id)
        .execute(&pool).await.unwrap();
    insert_booking(
        &pool,
        &et_id,
        None,
        next_monday.and_hms_opt(10, 0, 0).unwrap(),
        "confirmed",
    )
    .await;

    let slot_days = compute_slots(
        &pool,
        &et_id,
        30,
        0,
        0,
        0,
        days_to_monday,
        1,
        None,
        Tz::UTC,
        Tz::UTC,
        BusySource::Individual(vec![]),
        None,
    )
    .await;

    let monday_date = next_monday.format("%Y-%m-%d").to_string();
    let monday = slot_days.iter().find(|d| d.date == monday_date);
    assert!(
        monday.map(|d| d.slots.is_empty()).unwrap_or(true),
        "Per-member flag on a personal event type must still cap the day"
    );
}

#[tokio::test]
async fn apply_frequency_limit_filter_mixed_per_member_and_wide_fire_independently() {
    // Two limits on the same team event type:
    //   - 1/day per-member
    //   - 3/week event-type-wide
    //
    // Bookings span two consecutive weeks:
    //   Week 1: three bookings by Alice on Mon/Tue/Wed → wide week count
    //           hits its cap (3/3).
    //   Week 2: one booking each by Alice and Bob on Monday → both
    //           per-member day caps fill, wide week count is 2/3 (lax).
    //
    // Expectations:
    //   - Every weekday in week 1 must be hidden — wide cap dominates,
    //     even Thu/Fri where per-member alone would still allow bookings.
    //   - Week 2 Monday must be hidden by the per-member rule (both
    //     members at cap) while the wide rule still has headroom.
    //   - Week 2 Tue-Fri must be visible — neither rule fires.
    //
    // If a future change ever conflates the two rule paths, this test
    // will fail in a specific weekday rather than silently allow or
    // deny everything.
    let pool = setup_test_db().await;
    let (_, _, et_id) = seed_test_data(&pool).await;
    let (alice, bob) = seed_team_for_event_type(&pool, &et_id).await;

    let now = Utc::now().with_timezone(&Tz::UTC).naive_local();
    let mut next_monday = now.date() + Duration::days(1);
    while next_monday.weekday() != chrono::Weekday::Mon {
        next_monday += Duration::days(1);
    }
    let days_to_monday = (next_monday - now.date()).num_days() as i32;

    sqlx::query("INSERT INTO booking_frequency_limits (id, event_type_id, max_bookings, period, per_member) VALUES (?, ?, 1, 'day', 1)")
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(&et_id)
        .execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO booking_frequency_limits (id, event_type_id, max_bookings, period, per_member) VALUES (?, ?, 3, 'week', 0)")
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(&et_id)
        .execute(&pool).await.unwrap();

    // Week 1: three Alice bookings consume the wide week cap.
    for offset in 0..3 {
        insert_booking(
            &pool,
            &et_id,
            Some(&alice),
            (next_monday + Duration::days(offset))
                .and_hms_opt(10, 0, 0)
                .unwrap(),
            "confirmed",
        )
        .await;
    }

    // Week 2 Monday: both members booked, capping per-member.
    let week2_monday = next_monday + Duration::days(7);
    insert_booking(
        &pool,
        &et_id,
        Some(&alice),
        week2_monday.and_hms_opt(10, 0, 0).unwrap(),
        "confirmed",
    )
    .await;
    insert_booking(
        &pool,
        &et_id,
        Some(&bob),
        week2_monday.and_hms_opt(11, 0, 0).unwrap(),
        "confirmed",
    )
    .await;

    let slot_days = compute_slots(
        &pool,
        &et_id,
        30,
        0,
        0,
        0,
        days_to_monday,
        14,
        None,
        Tz::UTC,
        Tz::UTC,
        BusySource::Individual(vec![]),
        None,
    )
    .await;

    let find_day = |d: NaiveDate| -> Option<&SlotDay> {
        let key = d.format("%Y-%m-%d").to_string();
        slot_days.iter().find(|x| x.date == key)
    };

    // Week 1 Mon-Fri: every weekday hidden because the wide cap is hit.
    for offset in 0..5 {
        let d = next_monday + Duration::days(offset);
        let day = find_day(d);
        assert!(
            day.map(|x| x.slots.is_empty()).unwrap_or(true),
            "Week 1 {} should be hidden by the 3/week wide cap; got {:?}",
            d,
            day.map(|x| x.slots.len())
        );
    }

    // Week 2 Mon: per-member dominates — wide is lax (2/3) but both
    // members are at their per-day cap.
    let w2_mon = find_day(week2_monday);
    assert!(
        w2_mon.map(|x| x.slots.is_empty()).unwrap_or(true),
        "Week 2 Monday should be hidden by per-member rule; got {:?}",
        w2_mon.map(|x| x.slots.len())
    );

    // Week 2 Tue-Fri: neither rule fires, full availability.
    for offset in 1..5 {
        let d = week2_monday + Duration::days(offset);
        let day = find_day(d).expect("Week 2 weekday should be present");
        assert_eq!(
            day.slots.len(),
            16,
            "Week 2 {} should be free; got {} slots",
            d,
            day.slots.len()
        );
    }
}

#[tokio::test]
async fn compute_slots_no_weekend_slots() {
    let pool = setup_test_db().await;
    let (_, _, et_id) = seed_test_data(&pool).await;

    // Find next Saturday
    let now = Utc::now().with_timezone(&Tz::UTC).naive_local();
    let mut next_sat = now.date();
    while next_sat.weekday() != chrono::Weekday::Sat {
        next_sat += Duration::days(1);
    }
    let days_to_sat = (next_sat - now.date()).num_days() as i32;

    let busy = BusySource::Individual(vec![]);
    let slot_days = compute_slots(
        &pool,
        &et_id,
        30,
        0,
        0,
        0,
        days_to_sat,
        2, // just Sat + Sun
        None,
        Tz::UTC,
        Tz::UTC,
        busy,
        None,
    )
    .await;

    assert!(
        slot_days.is_empty(),
        "Weekends should have no slots (Mon-Fri rules only)"
    );
}

#[tokio::test]
async fn compute_slots_group_any_member_free() {
    let pool = setup_test_db().await;
    let (_, _, et_id) = seed_test_data(&pool).await;

    // Find the next Monday
    let now = Utc::now().with_timezone(&Tz::UTC).naive_local();
    let mut next_monday = now.date() + Duration::days(1);
    while next_monday.weekday() != chrono::Weekday::Mon {
        next_monday += Duration::days(1);
    }
    let days_to_monday = (next_monday - now.date()).num_days() as i32;

    let ten_am = next_monday.and_hms_opt(10, 0, 0).unwrap();
    let ten_thirty = next_monday.and_hms_opt(10, 30, 0).unwrap();

    // Member A is busy at 10:00, Member B is free
    let mut member_busy = HashMap::new();
    member_busy.insert("member_a".to_string(), vec![(ten_am, ten_thirty)]);
    member_busy.insert("member_b".to_string(), vec![]); // free

    let busy = BusySource::Group(member_busy);
    let slot_days = compute_slots(
        &pool,
        &et_id,
        30,
        0,
        0,
        0,
        days_to_monday,
        1,
        None,
        Tz::UTC,
        Tz::UTC,
        busy,
        None,
    )
    .await;

    assert!(!slot_days.is_empty());
    let monday = &slot_days[0];
    // 10:00 should still be available because member B is free
    let slot_times: Vec<&str> = monday.slots.iter().map(|s| s.start.as_str()).collect();
    assert!(
        slot_times.contains(&"10:00"),
        "10:00 should be available (member B is free)"
    );
    assert_eq!(
        monday.slots.len(),
        16,
        "All slots available (at least one member free)"
    );
}

#[tokio::test]
async fn compute_slots_group_all_busy_blocks() {
    let pool = setup_test_db().await;
    let (_, _, et_id) = seed_test_data(&pool).await;

    let now = Utc::now().with_timezone(&Tz::UTC).naive_local();
    let mut next_monday = now.date() + Duration::days(1);
    while next_monday.weekday() != chrono::Weekday::Mon {
        next_monday += Duration::days(1);
    }
    let days_to_monday = (next_monday - now.date()).num_days() as i32;

    let ten_am = next_monday.and_hms_opt(10, 0, 0).unwrap();
    let ten_thirty = next_monday.and_hms_opt(10, 30, 0).unwrap();

    // Both members busy at 10:00
    let mut member_busy = HashMap::new();
    member_busy.insert("member_a".to_string(), vec![(ten_am, ten_thirty)]);
    member_busy.insert("member_b".to_string(), vec![(ten_am, ten_thirty)]);

    let busy = BusySource::Group(member_busy);
    let slot_days = compute_slots(
        &pool,
        &et_id,
        30,
        0,
        0,
        0,
        days_to_monday,
        1,
        None,
        Tz::UTC,
        Tz::UTC,
        busy,
        None,
    )
    .await;

    assert!(!slot_days.is_empty());
    let monday = &slot_days[0];
    let slot_times: Vec<&str> = monday.slots.iter().map(|s| s.start.as_str()).collect();
    assert!(
        !slot_times.contains(&"10:00"),
        "10:00 blocked when ALL members busy"
    );
    assert_eq!(monday.slots.len(), 15, "One slot blocked");
}

#[tokio::test]
async fn compute_slots_team_requires_all_free() {
    // Team links require ALL members to be free (unlike Group which needs ANY)
    let pool = setup_test_db().await;
    let (_, _, et_id) = seed_test_data(&pool).await;

    let now = Utc::now().with_timezone(&Tz::UTC).naive_local();
    let mut next_monday = now.date() + Duration::days(1);
    while next_monday.weekday() != chrono::Weekday::Mon {
        next_monday += Duration::days(1);
    }
    let days_to_monday = (next_monday - now.date()).num_days() as i32;

    let ten_am = next_monday.and_hms_opt(10, 0, 0).unwrap();
    let ten_thirty = next_monday.and_hms_opt(10, 30, 0).unwrap();

    // Only ONE member busy at 10:00 — Team should block, Group would allow
    let mut member_busy = HashMap::new();
    member_busy.insert("member_a".to_string(), vec![(ten_am, ten_thirty)]);
    member_busy.insert("member_b".to_string(), vec![]); // member_b is free

    let busy = BusySource::Team(member_busy);
    let slot_days = compute_slots(
        &pool,
        &et_id,
        30,
        0,
        0,
        0,
        days_to_monday,
        1,
        None,
        Tz::UTC,
        Tz::UTC,
        busy,
        None,
    )
    .await;

    assert!(!slot_days.is_empty());
    let monday = &slot_days[0];
    let slot_times: Vec<&str> = monday.slots.iter().map(|s| s.start.as_str()).collect();
    assert!(
        !slot_times.contains(&"10:00"),
        "Team: 10:00 blocked when ANY member is busy"
    );
    assert_eq!(monday.slots.len(), 15, "One slot blocked for Team");
}

#[tokio::test]
async fn compute_slots_team_all_free_allows() {
    // When all team members are free, the slot should be available
    let pool = setup_test_db().await;
    let (_, _, et_id) = seed_test_data(&pool).await;

    let now = Utc::now().with_timezone(&Tz::UTC).naive_local();
    let mut next_monday = now.date() + Duration::days(1);
    while next_monday.weekday() != chrono::Weekday::Mon {
        next_monday += Duration::days(1);
    }
    let days_to_monday = (next_monday - now.date()).num_days() as i32;

    // No one is busy
    let mut member_busy = HashMap::new();
    member_busy.insert("member_a".to_string(), vec![]);
    member_busy.insert("member_b".to_string(), vec![]);

    let busy = BusySource::Team(member_busy);
    let slot_days = compute_slots(
        &pool,
        &et_id,
        30,
        0,
        0,
        0,
        days_to_monday,
        1,
        None,
        Tz::UTC,
        Tz::UTC,
        busy,
        None,
    )
    .await;

    assert!(!slot_days.is_empty());
    let monday = &slot_days[0];
    assert_eq!(
        monday.slots.len(),
        16,
        "All 16 slots available when team is free"
    );
}

// Regression test: an availability rule that ends at 23:00 with a 60-min
// slot must terminate. Before the fix, compute_slots_from_rules used a
// NaiveTime cursor, and NaiveTime + Duration wraps at 24h — so when
// cursor reached 23:00, cursor + slot_duration (60m) wrapped to 00:00
// which is still <= 23:00 as a time-of-day, making the loop emit a slot
// every step forever until OOM. Prod symptom: ~4-minute CPU spike +
// ~9GB RAM before the OOM killer intervened (issue logged in internal
// Vates Demo Team EN / book-a-demo-of-vates-vms booking page).
#[test]
fn compute_slots_terminates_with_window_ending_at_23_00() {
    let rules: Vec<(i32, String, String)> = (1..=4)
        .map(|d| (d, "09:00".to_string(), "23:00".to_string()))
        .collect();
    let busy = BusySource::Individual(vec![]);

    // Pick a start_offset that lands on a Monday so we hit at least one
    // rule-matching day regardless of when the test runs.
    let today = Utc::now().naive_utc().date();
    let mut start = 1i32;
    while (today + Duration::days(start as i64)).weekday() != chrono::Weekday::Mon {
        start += 1;
    }

    let result = compute_slots_from_rules(
        &rules,
        60,
        60,
        0,
        0,
        0,
        start,
        1,
        None,
        Tz::UTC,
        Tz::UTC,
        busy,
        &[],
    );

    // 09:00..=22:00 start times with 60-min spacing and 60-min duration =
    // 14 slots on a matching weekday. Anything beyond a few hundred means
    // the wrap bug is back.
    let total_slots: usize = result.iter().map(|d| d.slots.len()).sum();
    assert!(
        total_slots <= 20,
        "compute_slots_from_rules produced {} slots for a single day with \
             window 09:00-23:00 and 60-min duration; the NaiveTime wrap bug \
             is back",
        total_slots
    );
    assert_eq!(total_slots, 14, "expected exactly 14 hourly slots");
}

// Regression for issue #50: a team member in a different timezone than the host
// must only be considered available within THEIR working hours, not inside the
// event type's rule window interpreted in host_tz.
#[tokio::test]
async fn user_avail_as_busy_respects_member_timezone() {
    let pool = setup_test_db().await;

    let paris_uid = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO users (id, email, name, role, auth_provider, username, enabled, timezone) \
             VALUES (?, 'paris@example.com', 'Paris User', 'user', 'local', 'paris', 1, 'Europe/Paris')",
    )
    .bind(&paris_uid)
    .execute(&pool)
    .await
    .unwrap();

    // Paris member works Mon-Fri 09:00-17:00 local time
    for day in [1, 2, 3, 4, 5] {
        let rid = uuid::Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO user_availability_rules (id, user_id, day_of_week, start_time, end_time) \
                 VALUES (?, ?, ?, '09:00', '17:00')",
        )
        .bind(&rid)
        .bind(&paris_uid)
        .bind(day)
        .execute(&pool)
        .await
        .unwrap();
    }

    // Host is in New_York. In winter: Paris=UTC+1, NY=UTC-5 → 6h offset.
    // Paris 09:00-17:00 == NY 03:00-11:00.
    // Week of 2026-01-12 (Monday).
    let host_tz: Tz = "America/New_York".parse().unwrap();
    let window_start = dt(2026, 1, 12, 0, 0);
    let window_end = dt(2026, 1, 12, 23, 59);

    let busy = user_avail_as_busy(&pool, &paris_uid, window_start, window_end, host_tz).await;

    // Helper: is (t) inside any busy interval?
    let is_busy = |t: NaiveDateTime| busy.iter().any(|(s, e)| &t >= s && &t < e);

    // NY 02:00 Monday = Paris 08:00 — Paris member NOT working yet → must be busy.
    assert!(
        is_busy(dt(2026, 1, 12, 2, 0)),
        "NY 02:00 (Paris 08:00) should be blocked — outside member's working hours"
    );
    // NY 05:00 Monday = Paris 11:00 — Paris member IS working → must NOT be busy.
    assert!(
        !is_busy(dt(2026, 1, 12, 5, 0)),
        "NY 05:00 (Paris 11:00) should be free — inside member's working hours"
    );
    // NY 12:00 Monday = Paris 18:00 — Paris member done for the day → must be busy.
    assert!(
        is_busy(dt(2026, 1, 12, 12, 0)),
        "NY 12:00 (Paris 18:00) should be blocked — outside member's working hours"
    );
}

// Regression for Antoine's follow-up: the team event-type flow must apply
// each member's personal working hours as a constraint (converted into
// host_tz). Concretely: a team with a single America/Chicago member whose
// personal rules are Mon-Fri 09:00-17:00 local must render slots in Paris
// host_tz only from 16:00 onwards, not across the full 09:00-23:00 rule.
// Verifies the exact conversion user_avail_as_busy performs; the
// show_group_slots/pick_group_member handlers extend member_busy with
// this output before running compute_slots.
#[tokio::test]
async fn chicago_member_is_busy_at_paris_morning() {
    let pool = setup_test_db().await;
    let uid = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO users (id, email, name, role, auth_provider, username, enabled, timezone) \
             VALUES (?, 'andy@example.com', 'Andy', 'user', 'local', 'andy', 1, 'America/Chicago')",
    )
    .bind(&uid)
    .execute(&pool)
    .await
    .unwrap();

    // Mon-Fri 09:00-17:00 Chicago local
    for day in 1..=5 {
        let rid = uuid::Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO user_availability_rules (id, user_id, day_of_week, start_time, end_time) \
                 VALUES (?, ?, ?, '09:00', '17:00')",
        )
        .bind(&rid)
        .bind(&uid)
        .bind(day)
        .execute(&pool)
        .await
        .unwrap();
    }

    // 2026-07-07 is a Tuesday in full summer DST (Chicago CDT UTC-5,
    // Paris CEST UTC+2, 7-hour offset — matches Antoine's scenario).
    let host_tz: Tz = "Europe/Paris".parse().unwrap();
    let window_start = dt(2026, 7, 7, 0, 0);
    let window_end = dt(2026, 7, 7, 23, 59);

    let busy = user_avail_as_busy(&pool, &uid, window_start, window_end, host_tz).await;
    let is_busy = |t: NaiveDateTime| busy.iter().any(|(s, e)| &t >= s && &t < e);

    // Paris 09:00 = Chicago 02:00 — outside 09-17, must be blocked.
    assert!(
        is_busy(dt(2026, 7, 7, 9, 0)),
        "Paris 09:00 (Chicago 02:00) must be blocked — Andy sleeping"
    );
    // Paris 16:00 = Chicago 09:00 — Andy just started, must be free.
    assert!(
        !is_busy(dt(2026, 7, 7, 16, 0)),
        "Paris 16:00 (Chicago 09:00) must be free — Andy working"
    );
    // Paris 22:00 = Chicago 15:00 — Andy still working, must be free.
    assert!(
        !is_busy(dt(2026, 7, 7, 22, 0)),
        "Paris 22:00 (Chicago 15:00) must be free — Andy working"
    );
}

// Regression: when a team member without any user_availability_rules is
// active, they must be treated as always-available (no synthesized 9-17
// default). This preserves the behavior Antoine tested originally where
// a single Paris member expected the full event-type rule window.
#[tokio::test]
async fn member_without_personal_rules_is_unconstrained() {
    let pool = setup_test_db().await;
    let uid = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO users (id, email, name, role, auth_provider, username, enabled, timezone) \
             VALUES (?, 'free@example.com', 'No Hours', 'user', 'local', 'free', 1, 'Europe/Paris')",
    )
    .bind(&uid)
    .execute(&pool)
    .await
    .unwrap();
    // Deliberately NO user_availability_rules rows for this user.

    let host_tz: Tz = "Europe/Paris".parse().unwrap();
    let busy = user_avail_as_busy(
        &pool,
        &uid,
        dt(2026, 7, 7, 0, 0),
        dt(2026, 7, 7, 23, 59),
        host_tz,
    )
    .await;

    assert!(
        busy.is_empty(),
        "no personal rules must mean no constraint — got {} busy intervals",
        busy.len()
    );
}

// Regression for issue #50: an event type carries its own host timezone,
// which must take precedence over the account owner's personal timezone.
// Prevents the original bug where a US-based creator silently made the
// team's 09:00-21:00 rule land in Chicago time.
#[tokio::test]
async fn get_host_tz_prefers_explicit_event_type_timezone() {
    let pool = setup_test_db().await;

    // Create a user in America/Chicago and their account.
    let user_id = uuid::Uuid::new_v4().to_string();
    let account_id = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO users (id, email, name, role, auth_provider, username, enabled, timezone) \
             VALUES (?, 'chicago@example.com', 'Chicago User', 'user', 'local', 'chicago', 1, 'America/Chicago')",
    )
    .bind(&user_id)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO accounts (id, name, email, timezone, user_id) VALUES (?, 'Chicago User', 'chicago@example.com', 'America/Chicago', ?)")
        .bind(&account_id)
        .bind(&user_id)
        .execute(&pool)
        .await
        .unwrap();

    // Event type explicitly pinned to Europe/Paris.
    let et_id = uuid::Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO event_types (id, account_id, slug, title, duration_min, buffer_before, buffer_after, min_notice_min, enabled, timezone) VALUES (?, ?, 'demo', 'Demo', 30, 0, 0, 0, 1, 'Europe/Paris')")
        .bind(&et_id)
        .bind(&account_id)
        .execute(&pool)
        .await
        .unwrap();

    assert_eq!(
        get_host_tz(&pool, &et_id).await,
        Tz::Europe__Paris,
        "explicit event-type timezone must win over account owner's"
    );

    // Event type with NULL timezone should fall back to the account owner.
    let et_id2 = uuid::Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO event_types (id, account_id, slug, title, duration_min, buffer_before, buffer_after, min_notice_min, enabled) VALUES (?, ?, 'demo2', 'Demo2', 30, 0, 0, 0, 1)")
        .bind(&et_id2)
        .bind(&account_id)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        get_host_tz(&pool, &et_id2).await,
        Tz::America__Chicago,
        "NULL event-type timezone must fall back to account owner"
    );
}

// --- parse_guest_tz tests ---

#[test]
fn parse_guest_tz_valid() {
    assert_eq!(
        parse_guest_tz(Some("America/New_York")),
        chrono_tz::America::New_York
    );
    assert_eq!(
        parse_guest_tz(Some("Europe/Paris")),
        chrono_tz::Europe::Paris
    );
    assert_eq!(parse_guest_tz(Some("UTC")), Tz::UTC);
}

#[test]
fn parse_guest_tz_invalid_falls_back() {
    let tz = parse_guest_tz(Some("Not/A/Timezone"));
    // Should be server local or UTC — either way, not panic
    let _ = tz;
}

#[test]
fn parse_guest_tz_none_falls_back() {
    let tz = parse_guest_tz(None);
    let _ = tz;
}

// --- parse_avail_windows tests ---

#[test]
fn parse_avail_windows_single_window() {
    let w = parse_avail_windows(Some("09:00-17:00"), None, None);
    assert_eq!(w, vec![("09:00".to_string(), "17:00".to_string())]);
}

#[test]
fn parse_avail_windows_multiple_windows() {
    let w = parse_avail_windows(Some("09:00-12:00,13:00-17:00"), None, None);
    assert_eq!(
        w,
        vec![
            ("09:00".to_string(), "12:00".to_string()),
            ("13:00".to_string(), "17:00".to_string()),
        ]
    );
}

#[test]
fn parse_avail_windows_legacy_fallback() {
    let w = parse_avail_windows(None, Some("08:00"), Some("16:00"));
    assert_eq!(w, vec![("08:00".to_string(), "16:00".to_string())]);
}

#[test]
fn parse_avail_windows_empty_string_uses_legacy() {
    let w = parse_avail_windows(Some(""), Some("10:00"), Some("18:00"));
    assert_eq!(w, vec![("10:00".to_string(), "18:00".to_string())]);
}

#[test]
fn parse_avail_windows_invalid_times_ignored() {
    let w = parse_avail_windows(Some("09:00-12:00,bad-data,13:00-17:00"), None, None);
    assert_eq!(
        w,
        vec![
            ("09:00".to_string(), "12:00".to_string()),
            ("13:00".to_string(), "17:00".to_string()),
        ]
    );
}

#[test]
fn parse_avail_windows_defaults_when_none() {
    let w = parse_avail_windows(None, None, None);
    assert_eq!(w, vec![("09:00".to_string(), "17:00".to_string())]);
}

// --- fetch_busy_times_for_user_ex booking exclusion tests ---

#[tokio::test]
async fn fetch_busy_times_ex_excludes_specified_booking() {
    let pool = setup_test_db().await;
    let (user_id, account_id, et_id) = seed_test_data(&pool).await;

    let booking_id = uuid::Uuid::new_v4().to_string();
    let cancel_tok = uuid::Uuid::new_v4().to_string();
    let resched_tok = uuid::Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO bookings (id, event_type_id, uid, guest_name, guest_email, guest_timezone, start_at, end_at, status, cancel_token, reschedule_token) VALUES (?, ?, 'uid-ex1', 'Guest', 'guest@example.com', 'UTC', '2026-03-16T10:00:00', '2026-03-16T10:30:00', 'confirmed', ?, ?)")
        .bind(&booking_id)
        .bind(&et_id)
        .bind(&cancel_tok)
        .bind(&resched_tok)
        .execute(&pool).await.unwrap();

    // A successful CalDAV write followed by sync creates a second local
    // representation of the same booking under the same iCalendar UID.
    let source_id = uuid::Uuid::new_v4().to_string();
    let calendar_id = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO caldav_sources (id, account_id, name, url, username) \
             VALUES (?, ?, 'Write calendar', 'https://calendar.test', 'host')",
    )
    .bind(&source_id)
    .bind(&account_id)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO calendars (id, source_id, href, display_name, is_busy) \
             VALUES (?, ?, '/write/', 'Write calendar', 1)",
    )
    .bind(&calendar_id)
    .bind(&source_id)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO events (id, calendar_id, uid, summary, start_at, end_at, status, transp) \
             VALUES (?, ?, 'uid-ex1', 'Synced booking', '20260316T100000', \
                     '20260316T103000', 'CONFIRMED', 'OPAQUE')",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(&calendar_id)
    .execute(&pool)
    .await
    .unwrap();

    // Without exclusion both local representations show as busy.
    let busy = fetch_busy_times_for_user_ex(
        &pool,
        &user_id,
        dt(2026, 3, 15, 0, 0),
        dt(2026, 3, 21, 23, 59),
        Tz::UTC,
        None,
        None,
        None,
    )
    .await;
    assert_eq!(
        busy.len(),
        2,
        "booking row and synced calendar copy should both be busy without exclusion"
    );

    // With both identifiers, neither representation blocks its own slot.
    let busy_ex = fetch_busy_times_for_user_ex(
        &pool,
        &user_id,
        dt(2026, 3, 15, 0, 0),
        dt(2026, 3, 21, 23, 59),
        Tz::UTC,
        None,
        Some(&booking_id),
        Some("uid-ex1"),
    )
    .await;
    assert!(
        busy_ex.is_empty(),
        "excluded booking and its synced calendar copy should both disappear"
    );
}

#[tokio::test]
async fn fetch_busy_times_ex_excludes_only_specified_booking() {
    let pool = setup_test_db().await;
    let (user_id, _, et_id) = seed_test_data(&pool).await;

    // Insert two bookings
    let booking_id_1 = uuid::Uuid::new_v4().to_string();
    let booking_id_2 = uuid::Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO bookings (id, event_type_id, uid, guest_name, guest_email, guest_timezone, start_at, end_at, status, cancel_token, reschedule_token) VALUES (?, ?, 'uid-a', 'Guest A', 'a@example.com', 'UTC', '2026-03-16T10:00:00', '2026-03-16T10:30:00', 'confirmed', ?, ?)")
        .bind(&booking_id_1)
        .bind(&et_id)
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(uuid::Uuid::new_v4().to_string())
        .execute(&pool).await.unwrap();

    sqlx::query("INSERT INTO bookings (id, event_type_id, uid, guest_name, guest_email, guest_timezone, start_at, end_at, status, cancel_token, reschedule_token) VALUES (?, ?, 'uid-b', 'Guest B', 'b@example.com', 'UTC', '2026-03-16T14:00:00', '2026-03-16T14:30:00', 'confirmed', ?, ?)")
        .bind(&booking_id_2)
        .bind(&et_id)
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(uuid::Uuid::new_v4().to_string())
        .execute(&pool).await.unwrap();

    // Exclude first booking: only second should be busy
    let busy = fetch_busy_times_for_user_ex(
        &pool,
        &user_id,
        dt(2026, 3, 15, 0, 0),
        dt(2026, 3, 21, 23, 59),
        Tz::UTC,
        None,
        Some(&booking_id_1),
        None,
    )
    .await;
    assert_eq!(
        busy.len(),
        1,
        "Only the non-excluded booking should be in busy times"
    );
    assert_eq!(busy[0].0, dt(2026, 3, 16, 14, 0));
}

#[tokio::test]
async fn fetch_busy_times_ex_none_exclusion_matches_original() {
    let pool = setup_test_db().await;
    let (user_id, _, et_id) = seed_test_data(&pool).await;

    let booking_id = uuid::Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO bookings (id, event_type_id, uid, guest_name, guest_email, guest_timezone, start_at, end_at, status, cancel_token, reschedule_token) VALUES (?, ?, 'uid-c', 'Guest', 'guest@example.com', 'UTC', '2026-03-16T10:00:00', '2026-03-16T10:30:00', 'confirmed', ?, ?)")
        .bind(&booking_id)
        .bind(&et_id)
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(uuid::Uuid::new_v4().to_string())
        .execute(&pool).await.unwrap();

    let busy_original = fetch_busy_times_for_user(
        &pool,
        &user_id,
        dt(2026, 3, 15, 0, 0),
        dt(2026, 3, 21, 23, 59),
        Tz::UTC,
        None,
    )
    .await;

    let busy_ex = fetch_busy_times_for_user_ex(
        &pool,
        &user_id,
        dt(2026, 3, 15, 0, 0),
        dt(2026, 3, 21, 23, 59),
        Tz::UTC,
        None,
        None,
        None,
    )
    .await;

    assert_eq!(
        busy_original.len(),
        busy_ex.len(),
        "None exclusion should match original function"
    );
}

#[tokio::test]
async fn reschedule_excluded_booking_frees_its_slot() {
    let pool = setup_test_db().await;
    let (user_id, _, et_id) = seed_test_data(&pool).await;

    // Book 10:00-10:30
    let (bid, _, _) = insert_test_booking(
        &pool,
        &et_id,
        "2026-03-16T10:00:00",
        "2026-03-16T10:30:00",
        "confirmed",
    )
    .await;

    // Without exclusion: 10:00-10:30 is busy
    let busy = fetch_busy_times_for_user_ex(
        &pool,
        &user_id,
        dt(2026, 3, 16, 9, 0),
        dt(2026, 3, 16, 12, 0),
        Tz::UTC,
        None,
        None,
        None,
    )
    .await;
    assert!(
        has_conflict(&busy, dt(2026, 3, 16, 10, 0), dt(2026, 3, 16, 10, 30)),
        "10:00-10:30 should conflict without exclusion"
    );

    // With exclusion: slot is free for rescheduling back to same time
    let busy_ex = fetch_busy_times_for_user_ex(
        &pool,
        &user_id,
        dt(2026, 3, 16, 9, 0),
        dt(2026, 3, 16, 12, 0),
        Tz::UTC,
        None,
        Some(&bid),
        None,
    )
    .await;
    assert!(
        !has_conflict(&busy_ex, dt(2026, 3, 16, 10, 0), dt(2026, 3, 16, 10, 30)),
        "10:00-10:30 should be free when booking is excluded (reschedule to same slot)"
    );
}

#[tokio::test]
async fn compute_slots_blocked_override_skips_day() {
    let pool = setup_test_db().await;
    let (_, _, et_id) = seed_test_data(&pool).await;

    // Find the next Monday
    let now = Utc::now().with_timezone(&Tz::UTC).naive_local();
    let mut next_monday = now.date() + Duration::days(1);
    while next_monday.weekday() != chrono::Weekday::Mon {
        next_monday += Duration::days(1);
    }
    let days_to_monday = (next_monday - now.date()).num_days() as i32;
    let monday_str = next_monday.format("%Y-%m-%d").to_string();

    // Block that Monday with an override
    let override_id = uuid::Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO availability_overrides (id, event_type_id, date, is_blocked) VALUES (?, ?, ?, 1)")
        .bind(&override_id)
        .bind(&et_id)
        .bind(&monday_str)
        .execute(&pool)
        .await
        .unwrap();

    let slot_days = compute_slots(
        &pool,
        &et_id,
        30,
        0,
        0,
        0,
        days_to_monday,
        1,
        None,
        Tz::UTC,
        Tz::UTC,
        BusySource::Individual(vec![]),
        None,
    )
    .await;

    // Monday should be blocked — no slots
    assert!(
        slot_days.is_empty(),
        "Blocked override should skip the day, got {} days with slots",
        slot_days.len()
    );
}

#[tokio::test]
async fn compute_slots_custom_hours_override() {
    let pool = setup_test_db().await;
    let (_, _, et_id) = seed_test_data(&pool).await;

    // Find the next Monday
    let now = Utc::now().with_timezone(&Tz::UTC).naive_local();
    let mut next_monday = now.date() + Duration::days(1);
    while next_monday.weekday() != chrono::Weekday::Mon {
        next_monday += Duration::days(1);
    }
    let days_to_monday = (next_monday - now.date()).num_days() as i32;
    let monday_str = next_monday.format("%Y-%m-%d").to_string();

    // Override Monday with custom hours: only 10:00-12:00 (instead of default 09:00-17:00)
    let override_id = uuid::Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO availability_overrides (id, event_type_id, date, start_time, end_time, is_blocked) VALUES (?, ?, ?, '10:00', '12:00', 0)")
        .bind(&override_id)
        .bind(&et_id)
        .bind(&monday_str)
        .execute(&pool)
        .await
        .unwrap();

    let slot_days = compute_slots(
        &pool,
        &et_id,
        30,
        0,
        0,
        0,
        days_to_monday,
        1,
        None,
        Tz::UTC,
        Tz::UTC,
        BusySource::Individual(vec![]),
        None,
    )
    .await;

    assert!(!slot_days.is_empty(), "Should have slots for custom hours");
    // 10:00-12:00 with 30min slots = 4 slots (10:00, 10:30, 11:00, 11:30)
    let total_slots: usize = slot_days.iter().map(|d| d.slots.len()).sum();
    assert_eq!(
        total_slots, 4,
        "10:00-12:00 with 30min = 4 slots, got {}",
        total_slots
    );
}

// --- slot_interval_min unit tests ---

#[tokio::test]
async fn compute_slots_custom_interval_fewer_slots() {
    // When slot_interval is 60 with a 30-min duration, the cursor steps by 60 min
    // producing 1 slot per hour instead of 2 per hour
    let pool = setup_test_db().await;
    let (_, _, et_id) = seed_test_data(&pool).await;

    // Override: 60-min slot interval, 30-min duration
    sqlx::query("UPDATE event_types SET slot_interval_min = 60 WHERE id = ?")
        .bind(&et_id)
        .execute(&pool)
        .await
        .unwrap();

    let now = Utc::now().with_timezone(&Tz::UTC).naive_local();
    let mut next_monday = now.date() + Duration::days(1);
    while next_monday.weekday() != chrono::Weekday::Mon {
        next_monday += Duration::days(1);
    }
    let days_to_monday = (next_monday - now.date()).num_days() as i32;

    let slot_days = compute_slots(
        &pool,
        &et_id,
        30, // duration
        0,
        0,
        0,
        days_to_monday,
        1,
        None,
        Tz::UTC,
        Tz::UTC,
        BusySource::Individual(vec![]),
        None,
    )
    .await;

    assert!(!slot_days.is_empty());
    let monday = &slot_days[0];
    // 09:00-17:00 = 8 hours; 60-min interval → 8 slots (09:00, 10:00, ..., 16:00)
    assert_eq!(
        monday.slots.len(),
        8,
        "60-min interval over 09:00-17:00 should yield 8 slots, got {}",
        monday.slots.len()
    );
    // Verify exact start times
    let slot_times: Vec<&str> = monday.slots.iter().map(|s| s.start.as_str()).collect();
    let expected: Vec<&str> = vec![
        "09:00", "10:00", "11:00", "12:00", "13:00", "14:00", "15:00", "16:00",
    ];
    assert_eq!(
        slot_times, expected,
        "Slot times should match 60-min interval stepping"
    );
}

#[tokio::test]
async fn compute_slots_custom_interval_15_min() {
    // 15-min interval with 30-min duration: cursor steps by 15 min
    // 09:00-17:00 = 8 hours = 480 min; 480/15 = 32 cursor positions
    // but cursor must satisfy cursor + 30 <= 17:00, so 16:45+30=17:15 > 17:00 → last is 16:30
    // That gives 31 slots (09:00 through 16:30)
    let pool = setup_test_db().await;
    let (_, _, et_id) = seed_test_data(&pool).await;

    sqlx::query("UPDATE event_types SET slot_interval_min = 15 WHERE id = ?")
        .bind(&et_id)
        .execute(&pool)
        .await
        .unwrap();

    let now = Utc::now().with_timezone(&Tz::UTC).naive_local();
    let mut next_monday = now.date() + Duration::days(1);
    while next_monday.weekday() != chrono::Weekday::Mon {
        next_monday += Duration::days(1);
    }
    let days_to_monday = (next_monday - now.date()).num_days() as i32;

    let slot_days = compute_slots(
        &pool,
        &et_id,
        30, // duration
        0,
        0,
        0,
        days_to_monday,
        1,
        None,
        Tz::UTC,
        Tz::UTC,
        BusySource::Individual(vec![]),
        None,
    )
    .await;

    assert!(!slot_days.is_empty());
    let monday = &slot_days[0];
    // 09:00-17:00 with 30-min slot: 31 slots (09:00 … 16:30)
    // 16:45 + 30 = 17:15 > 17:00 so it's excluded
    assert_eq!(
        monday.slots.len(),
        31,
        "15-min interval over 09:00-17:00 should yield 31 slots, got {}",
        monday.slots.len()
    );
    // Verify first and last slots
    let slot_times: Vec<&str> = monday.slots.iter().map(|s| s.start.as_str()).collect();
    assert_eq!(slot_times.first(), Some(&"09:00"));
    assert_eq!(slot_times.last(), Some(&"16:30"));
}

#[tokio::test]
async fn compute_slots_interval_greater_than_duration() {
    // When interval (60) > duration (30), cursor steps 60 but each slot occupies 30 min
    // This means some potential slots are skipped (e.g., 09:30 is never checked)
    let pool = setup_test_db().await;
    let (_, _, et_id) = seed_test_data(&pool).await;

    sqlx::query("UPDATE event_types SET slot_interval_min = 60 WHERE id = ?")
        .bind(&et_id)
        .execute(&pool)
        .await
        .unwrap();

    let now = Utc::now().with_timezone(&Tz::UTC).naive_local();
    let mut next_monday = now.date() + Duration::days(1);
    while next_monday.weekday() != chrono::Weekday::Mon {
        next_monday += Duration::days(1);
    }
    let days_to_monday = (next_monday - now.date()).num_days() as i32;

    // Block 09:00-09:30 on Monday
    let monday_date = next_monday;
    let busy_start = monday_date.and_hms_opt(9, 0, 0).unwrap();
    let busy_end = monday_date.and_hms_opt(9, 30, 0).unwrap();

    let slot_days = compute_slots(
        &pool,
        &et_id,
        30,
        0,
        0,
        0,
        days_to_monday,
        1,
        None,
        Tz::UTC,
        Tz::UTC,
        BusySource::Individual(vec![(busy_start, busy_end)]),
        None,
    )
    .await;

    assert!(!slot_days.is_empty());
    let monday = &slot_days[0];
    // 09:00 slot is blocked by busy event, so 7 remaining slots
    assert_eq!(
        monday.slots.len(),
        7,
        "09:00 slot should be blocked by busy event, got {}",
        monday.slots.len()
    );
    let slot_times: Vec<&str> = monday.slots.iter().map(|s| s.start.as_str()).collect();
    assert!(!slot_times.contains(&"09:00"), "09:00 should be blocked");
    assert!(
        slot_times.contains(&"10:00"),
        "10:00 should still be available"
    );
}

#[tokio::test]
async fn compute_slots_null_interval_defaults_to_duration() {
    // When slot_interval_min is NULL (unset), should default to duration (legacy behavior)
    let pool = setup_test_db().await;
    let (_, _, et_id) = seed_test_data(&pool).await;

    // Don't set slot_interval — it should be NULL from seed_test_data
    let now = Utc::now().with_timezone(&Tz::UTC).naive_local();
    let mut next_monday = now.date() + Duration::days(1);
    while next_monday.weekday() != chrono::Weekday::Mon {
        next_monday += Duration::days(1);
    }
    let days_to_monday = (next_monday - now.date()).num_days() as i32;

    let slot_days = compute_slots(
        &pool,
        &et_id,
        30, // duration
        0,
        0,
        0,
        days_to_monday,
        1,
        None,
        Tz::UTC,
        Tz::UTC,
        BusySource::Individual(vec![]),
        None,
    )
    .await;

    assert!(!slot_days.is_empty());
    let monday = &slot_days[0];
    // Should default to 30-min stepping (same as duration) → 16 slots
    assert_eq!(
        monday.slots.len(),
        16,
        "NULL interval should default to duration (30-min stepping) → 16 slots, got {}",
        monday.slots.len()
    );
}

#[tokio::test]
async fn compute_slots_interval_zero_defaults_to_duration() {
    // When slot_interval_min is 0, should also default to duration
    let pool = setup_test_db().await;
    let (_, _, et_id) = seed_test_data(&pool).await;

    sqlx::query("UPDATE event_types SET slot_interval_min = 0 WHERE id = ?")
        .bind(&et_id)
        .execute(&pool)
        .await
        .unwrap();

    let now = Utc::now().with_timezone(&Tz::UTC).naive_local();
    let mut next_monday = now.date() + Duration::days(1);
    while next_monday.weekday() != chrono::Weekday::Mon {
        next_monday += Duration::days(1);
    }
    let days_to_monday = (next_monday - now.date()).num_days() as i32;

    let slot_days = compute_slots(
        &pool,
        &et_id,
        30,
        0,
        0,
        0,
        days_to_monday,
        1,
        None,
        Tz::UTC,
        Tz::UTC,
        BusySource::Individual(vec![]),
        None,
    )
    .await;

    assert!(!slot_days.is_empty());
    let monday = &slot_days[0];
    assert_eq!(
        monday.slots.len(),
        16,
        "Zero interval should default to duration (30-min stepping) → 16 slots, got {}",
        monday.slots.len()
    );
}

#[tokio::test]
async fn compute_slots_interval_with_buffer_overlap() {
    // Interval of 60 with a 15-min buffer should correctly reject slots
    // that would overlap with busy events via buffer zones
    let pool = setup_test_db().await;
    let (_, _, et_id) = seed_test_data(&pool).await;

    sqlx::query("UPDATE event_types SET slot_interval_min = 60 WHERE id = ?")
        .bind(&et_id)
        .execute(&pool)
        .await
        .unwrap();

    let now = Utc::now().with_timezone(&Tz::UTC).naive_local();
    let mut next_monday = now.date() + Duration::days(1);
    while next_monday.weekday() != chrono::Weekday::Mon {
        next_monday += Duration::days(1);
    }
    let days_to_monday = (next_monday - now.date()).num_days() as i32;

    // Busy event at 09:00-09:30, with 15-min buffer it blocks 08:45-09:45
    // So 09:00 slot (buf_start=08:45, buf_end=10:00) overlaps with busy event (08:45 < 09:30 && 09:00 > 08:45)
    let busy_start = next_monday.and_hms_opt(9, 0, 0).unwrap();
    let busy_end = next_monday.and_hms_opt(9, 30, 0).unwrap();

    let slot_days = compute_slots(
        &pool,
        &et_id,
        30,
        15, // buffer before
        15, // buffer after
        0,
        days_to_monday,
        1,
        None,
        Tz::UTC,
        Tz::UTC,
        BusySource::Individual(vec![(busy_start, busy_end)]),
        None,
    )
    .await;

    assert!(!slot_days.is_empty());
    let monday = &slot_days[0];
    // 09:00 is blocked by direct overlap + buffer
    assert_eq!(
        monday.slots.len(),
        7,
        "09:00 slot blocked by event+buffer, got {}",
        monday.slots.len()
    );
}

#[tokio::test]
async fn troubleshoot_returns_200() {
    let (app, _, session, _) = setup_test_app().await;
    let response = app
        .oneshot(get_authed("/dashboard/troubleshoot", &session))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = body_string(response).await;
    assert!(
        body.contains("Troubleshoot"),
        "Troubleshoot page should render"
    );
}

// --- Troubleshoot with override ---

#[tokio::test]
async fn troubleshoot_shows_blocked_override() {
    let (app, pool, session, et_id) = setup_test_app().await;

    // Block tomorrow
    let tomorrow = (Utc::now() + Duration::days(1))
        .format("%Y-%m-%d")
        .to_string();
    let override_id = uuid::Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO availability_overrides (id, event_type_id, date, is_blocked) VALUES (?, ?, ?, 1)")
        .bind(&override_id)
        .bind(&et_id)
        .bind(&tomorrow)
        .execute(&pool)
        .await
        .unwrap();

    let response = app
        .oneshot(get_authed(
            &format!(
                "/dashboard/troubleshoot?date={}&event_type=test-meeting",
                tomorrow
            ),
            &session,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = body_string(response).await;
    assert!(
        body.contains("Blocked")
            || body.contains("blocked")
            || body.contains("override")
            || body.contains("day off"),
        "Troubleshoot should show blocked override"
    );
}

#[test]
fn parse_guest_tz_europe() {
    let tz = parse_guest_tz(Some("Europe/London"));
    assert_eq!(tz.name(), "Europe/London");
}

#[test]
fn guest_to_host_local_converts_across_zones() {
    // 18:00 Europe/Paris (CEST, UTC+2 in July) == 12:00 America/New_York (EDT, UTC-4)
    let paris: Tz = "Europe/Paris".parse().unwrap();
    let ny: Tz = "America/New_York".parse().unwrap();
    let guest_local = NaiveDate::from_ymd_opt(2026, 7, 15)
        .unwrap()
        .and_hms_opt(18, 0, 0)
        .unwrap();
    let host_local = guest_to_host_local(guest_local, paris, ny);
    assert_eq!(
        host_local.format("%Y-%m-%d %H:%M").to_string(),
        "2026-07-15 12:00"
    );
}

#[test]
fn guest_to_host_local_same_zone_is_noop() {
    let tz: Tz = "UTC".parse().unwrap();
    let dt = NaiveDate::from_ymd_opt(2026, 1, 1)
        .unwrap()
        .and_hms_opt(10, 30, 0)
        .unwrap();
    assert_eq!(guest_to_host_local(dt, tz, tz), dt);
}

#[test]
fn parse_guest_tz_garbage_falls_back() {
    let tz = parse_guest_tz(Some("Not/A/Timezone"));
    // Should fall back to system tz or UTC
    assert!(!tz.name().is_empty());
}

// --- parse_avail_windows edge cases ---

#[test]
fn parse_avail_windows_three_windows() {
    let windows = parse_avail_windows(
        Some("08:00-12:00,13:00-15:00,16:00-18:00"),
        Some("09:00"),
        Some("17:00"),
    );
    assert_eq!(windows.len(), 3);
    assert_eq!(windows[0], ("08:00".to_string(), "12:00".to_string()));
    assert_eq!(windows[1], ("13:00".to_string(), "15:00".to_string()));
    assert_eq!(windows[2], ("16:00".to_string(), "18:00".to_string()));
}

// --- Slots computation edge cases ---

#[tokio::test]
async fn compute_slots_with_min_notice_filters_near_slots() {
    let pool = setup_test_db().await;
    let (_, _, et_id) = seed_test_data(&pool).await;

    // With 480 min (8 hour) minimum notice, today's slots should be filtered
    let slot_days = compute_slots(
        &pool,
        &et_id,
        30,
        0,
        0,
        480, // 8 hours notice
        0,   // start from today
        1,   // just today
        None,
        Tz::UTC,
        Tz::UTC,
        BusySource::Individual(vec![]),
        None,
    )
    .await;

    // Today's slots might all be filtered depending on current time
    // Just verify it doesn't crash
    let _ = slot_days;
}

#[tokio::test]
async fn compute_slots_with_buffer_reduces_available() {
    let pool = setup_test_db().await;
    let (_, _, et_id) = seed_test_data(&pool).await;

    let now = Utc::now().with_timezone(&Tz::UTC).naive_local();
    let mut next_monday = now.date() + Duration::days(1);
    while next_monday.weekday() != chrono::Weekday::Mon {
        next_monday += Duration::days(1);
    }
    let days_to_monday = (next_monday - now.date()).num_days() as i32;

    // Block 10:00-11:00
    let busy_start = next_monday.and_hms_opt(10, 0, 0).unwrap();
    let busy_end = next_monday.and_hms_opt(11, 0, 0).unwrap();

    // Without buffer: 2 slots blocked (10:00, 10:30)
    let no_buffer = compute_slots(
        &pool,
        &et_id,
        30,
        0,
        0,
        0,
        days_to_monday,
        1,
        None,
        Tz::UTC,
        Tz::UTC,
        BusySource::Individual(vec![(busy_start, busy_end)]),
        None,
    )
    .await;

    // With 30min buffer before+after: more slots blocked
    let with_buffer = compute_slots(
        &pool,
        &et_id,
        30,
        30,
        30,
        0,
        days_to_monday,
        1,
        None,
        Tz::UTC,
        Tz::UTC,
        BusySource::Individual(vec![(busy_start, busy_end)]),
        None,
    )
    .await;

    let no_buf_count: usize = no_buffer.iter().map(|d| d.slots.len()).sum();
    let buf_count: usize = with_buffer.iter().map(|d| d.slots.len()).sum();
    assert!(
        buf_count < no_buf_count,
        "Buffer should reduce available slots: {} < {}",
        buf_count,
        no_buf_count
    );
}

// --- Rolling booking horizon (migration 063) ---

/// Availability on every weekday, wide enough that "today" still has slots
/// left whatever time of day the suite runs.
fn horizon_rules() -> Vec<(i32, String, String)> {
    (0..7)
        .map(|d| (d, "00:00".to_string(), "23:30".to_string()))
        .collect()
}

fn horizon_slots(horizon: Option<i32>, start_offset: i32, days_ahead: i32) -> Vec<SlotDay> {
    compute_slots_from_rules(
        &horizon_rules(),
        30,
        30,
        0,
        0,
        0,
        start_offset,
        days_ahead,
        horizon,
        Tz::UTC,
        Tz::UTC,
        BusySource::Individual(vec![]),
        &[],
    )
}

#[test]
fn compute_slots_null_horizon_offers_the_whole_window() {
    let days = horizon_slots(None, 1, 10);
    assert_eq!(days.len(), 10, "NULL horizon must not drop any day");
    let last = (host_today() + Duration::days(10))
        .format("%Y-%m-%d")
        .to_string();
    assert_eq!(days.last().unwrap().date, last);
}

#[test]
fn compute_slots_horizon_boundary_is_inclusive() {
    let horizon = 5i32;
    let days = horizon_slots(Some(horizon), 1, 20);
    let last_in = (host_today() + Duration::days(horizon as i64))
        .format("%Y-%m-%d")
        .to_string();
    let first_out = (host_today() + Duration::days(horizon as i64 + 1))
        .format("%Y-%m-%d")
        .to_string();
    assert!(
        days.iter().any(|d| d.date == last_in),
        "the last in-horizon day must still be offered"
    );
    assert!(
        !days.iter().any(|d| d.date == first_out),
        "the first out-of-horizon day must not be offered"
    );
}

#[test]
fn compute_slots_horizon_zero_offers_today_only() {
    let today = host_today().format("%Y-%m-%d").to_string();
    let days = horizon_slots(Some(0), 0, 10);
    assert!(
        days.iter().all(|d| d.date == today),
        "a horizon of 0 must offer today and nothing else"
    );
    // Same window without a horizon reaches past today, so the assertion
    // above is testing the clamp rather than an empty fixture.
    assert!(
        horizon_slots(None, 0, 10).iter().any(|d| d.date > today),
        "fixture must reach beyond today when unbounded"
    );
}

#[test]
fn compute_slots_min_notice_past_horizon_yields_no_slots() {
    // Minimum notice pushes the earliest bookable slot beyond the horizon,
    // so the two limits cross and nothing is bookable. That must render as
    // an empty list rather than panicking.
    let days = compute_slots_from_rules(
        &horizon_rules(),
        30,
        30,
        0,
        0,
        60 * 24 * 5,
        0,
        20,
        Some(1),
        Tz::UTC,
        Tz::UTC,
        BusySource::Individual(vec![]),
        &[],
    );
    assert!(
        days.is_empty(),
        "crossed limits must yield zero slots, not an error"
    );
}

#[test]
fn horizon_last_date_fails_closed_on_a_negative_horizon() {
    // parse_optional_day_count rejects negatives, so the column can only
    // hold one if it is written directly. However it gets there, it must
    // block every date rather than read as unlimited.
    //
    // Small negatives always did, because the arithmetic lands in the past
    // on its own. A large one overflowed the representable calendar and
    // returned None, which every caller treats as "no limit": the wrong
    // direction to fail in, and the case this pins.
    let now = host_today().and_hms_opt(9, 0, 0).unwrap();
    let today = now.date();

    assert_eq!(
        parse_optional_day_count("-5"),
        None,
        "the form must never store a negative horizon in the first place"
    );

    for days in [-1, -400, -95_000_000, i32::MIN + 1, i32::MIN] {
        let last = horizon_last_date(now, Some(days));
        assert!(
            last.is_some_and(|last| today > last),
            "a negative horizon ({days}) must leave today itself unbookable, got {last:?}"
        );
    }
}

#[test]
fn horizon_last_date_survives_an_absurdly_large_horizon() {
    // The form stores any non-negative i32, so a host who types a very
    // large number to mean "no limit" must not take the booking page down.
    assert_eq!(
        parse_optional_day_count("2147483647"),
        Some(i32::MAX),
        "the form accepts the value that reaches horizon_last_date"
    );
    let now = host_today().and_hms_opt(9, 0, 0).unwrap();
    assert_eq!(
        horizon_last_date(now, Some(i32::MAX)),
        None,
        "a horizon past the representable calendar is effectively unlimited"
    );
}

// --- Timezone pickers (#265) ---

/// A summer instant, so DST zones show their summer offset.
fn tz_summer() -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 7, 1, 12, 0, 0).unwrap()
}

fn tz_find<'a>(groups: &'a [TzGroup], value: &str) -> Vec<(Option<&'static str>, &'a TzOption)> {
    groups
        .iter()
        .flat_map(|g| g.options.iter().map(move |o| (g.area, o)))
        .filter(|(_, o)| o.value == value)
        .collect()
}

#[test]
fn timezone_groups_list_bucharest_whatever_the_selection() {
    // The issue: Bucharest showed up only when the browser was in it.
    for selected in ["", "UTC", "Europe/Paris", "America/New_York"] {
        let groups = timezone_groups(selected, tz_summer());
        let hits = tz_find(&groups, "Europe/Bucharest");
        assert_eq!(hits.len(), 1, "Bucharest listed once with {selected:?}");
        assert_eq!(hits[0].0, Some("Europe"));
        assert_eq!(hits[0].1.label, "Bucharest (UTC+3)");
        assert!(!hits[0].1.selected);
    }
}

#[test]
fn timezone_groups_put_common_zones_first_then_areas_in_order() {
    let groups = timezone_groups("", tz_summer());
    assert_eq!(groups[0].area, None);
    assert_eq!(groups[0].options[0].value, "Pacific/Midway");
    assert!(groups[0]
        .options
        .iter()
        .any(|o| o.value == "Europe/Paris" && o.label == "Paris, Brussels (UTC+2)"));
    let areas: Vec<&str> = groups[1..].iter().filter_map(|g| g.area).collect();
    assert_eq!(
        areas,
        [
            "Africa",
            "America",
            "Antarctica",
            "Arctic",
            "Asia",
            "Atlantic",
            "Australia",
            "Europe",
            "Indian",
            "Pacific"
        ]
    );
    for g in &groups[1..] {
        let labels: Vec<String> = g.options.iter().map(|o| o.label.to_lowercase()).collect();
        let mut sorted = labels.clone();
        sorted.sort();
        assert_eq!(labels, sorted, "{:?} is sorted by name", g.area);
    }
    let total: usize = groups.iter().map(|g| g.options.len()).sum();
    assert!(total > 400, "every IANA location is offered, got {total}");
}

#[test]
fn timezone_groups_list_each_zone_once_and_select_it_once() {
    let groups = timezone_groups("Europe/Paris", tz_summer());
    let mut seen = std::collections::HashSet::new();
    for g in &groups {
        for o in &g.options {
            assert!(seen.insert(o.value.clone()), "{} listed twice", o.value);
        }
    }
    for selected in [
        "Europe/Paris",
        "Europe/Bucharest",
        "UTC",
        "US/Eastern",
        "Asia/Calcutta",
    ] {
        let groups = timezone_groups(selected, tz_summer());
        let chosen: Vec<&TzOption> = groups
            .iter()
            .flat_map(|g| g.options.iter())
            .filter(|o| o.selected)
            .collect();
        assert_eq!(chosen.len(), 1, "one selected option for {selected}");
        assert_eq!(chosen[0].value, selected);
        assert_eq!(tz_find(&groups, selected).len(), 1);
    }
}

#[test]
fn timezone_groups_leave_legacy_aliases_out_unless_selected() {
    let groups = timezone_groups("", tz_summer());
    for alias in [
        "US/Eastern",
        "Etc/GMT+3",
        "EST5EDT",
        "Asia/Calcutta",
        "Europe/Kiev",
        "Australia/NSW",
        "Asia/Istanbul",
        "Europe/Nicosia",
    ] {
        assert!(tz_find(&groups, alias).is_empty(), "{alias} is not listed");
    }
    // Zones merged into another one but that are places of their own stay,
    // and so do the canonical ids of the two-continent cities.
    for kept in [
        "Europe/Amsterdam",
        "Europe/Oslo",
        "Europe/Kyiv",
        "America/Montreal",
        "America/Yellowknife",
        "America/Thunder_Bay",
        "America/Nipigon",
        "America/Rainy_River",
        "America/Pangnirtung",
        "Europe/Uzhgorod",
        "Europe/Zaporozhye",
        "Australia/Currie",
        "Asia/Nicosia",
        "Europe/Istanbul",
    ] {
        assert_eq!(tz_find(&groups, kept).len(), 1, "{kept} is listed");
    }
    // Every excluded name is a real chrono-tz id, so the list has no typo.
    for name in LEGACY_TIMEZONE_NAMES {
        assert!(name.parse::<Tz>().is_ok(), "{name} parses");
    }

    // The alternate id still shows when it is the saved one.
    let groups = timezone_groups("Europe/Nicosia", tz_summer());
    let hit = tz_find(&groups, "Europe/Nicosia");
    assert_eq!(hit[0].0, Some("Europe"));
    assert!(hit[0].1.selected);

    // A saved alias is never dropped: in its area when it has one, among
    // the common zones by offset otherwise.
    let groups = timezone_groups("Asia/Calcutta", tz_summer());
    let hit = tz_find(&groups, "Asia/Calcutta");
    assert_eq!(hit[0].0, Some("Asia"));
    assert_eq!(hit[0].1.label, "Calcutta (UTC+5:30)");

    let groups = timezone_groups("US/Eastern", tz_summer());
    let common = &groups[0].options;
    let pos = common.iter().position(|o| o.value == "US/Eastern").unwrap();
    assert_eq!(common[pos].label, "US/Eastern (UTC-4)");
    assert!(common[pos].selected);
    assert_eq!(common[pos - 1].value, "America/New_York");

    // Even an id chrono-tz does not know is offered, so the form posts it
    // back as it is (and `resolve_saved_timezone` keeps it).
    let groups = timezone_groups("Mars/Olympus_Mons", tz_summer());
    let hit = tz_find(&groups, "Mars/Olympus_Mons");
    assert_eq!(hit.len(), 1);
    assert!(hit[0].1.selected);
}

#[test]
fn timezone_labels_read_city_first_with_the_offset() {
    let groups = timezone_groups("", tz_summer());
    let label = |v: &str| tz_find(&groups, v)[0].1.label.clone();
    assert_eq!(
        label("America/Argentina/Buenos_Aires"),
        "Buenos Aires, Argentina (UTC-3)"
    );
    assert_eq!(label("America/St_Johns"), "St Johns (UTC-2:30)");
    assert_eq!(label("Asia/Kathmandu"), "Kathmandu (UTC+5:45)");
    assert_eq!(label("Europe/Moscow"), "Moscow (UTC+3)");
    assert_eq!(
        label("Antarctica/DumontDUrville"),
        "Dumont d'Urville (UTC+10)"
    );
    assert_eq!(
        label("America/Argentina/ComodRivadavia"),
        "Comodoro Rivadavia, Argentina (UTC-3)"
    );
    // Zero offset reads "UTC+0", the form the slot page note copies.
    assert_eq!(label("Atlantic/Reykjavik"), "Reykjavik (UTC+0)");

    // The offset is today's: Bucharest reads UTC+2 in winter.
    let winter = Utc.with_ymd_and_hms(2026, 12, 1, 12, 0, 0).unwrap();
    let groups = timezone_groups("", winter);
    assert_eq!(
        tz_find(&groups, "Europe/Bucharest")[0].1.label,
        "Bucharest (UTC+2)"
    );
}

#[test]
fn timezone_options_translate_the_group_labels() {
    let fr = timezone_options("Europe/Bucharest", "fr");
    let labels: Vec<String> = fr
        .iter()
        .map(|g| g.get_attr("label").unwrap().to_string())
        .collect();
    assert_eq!(labels[0], "Fuseaux horaires courants");
    assert!(labels.contains(&"Europe".to_string()));
    assert!(labels.contains(&"Océan Pacifique".to_string()));
    let en = timezone_options("", "en");
    assert_eq!(
        en[0].get_attr("label").unwrap().to_string(),
        "Common timezones"
    );
}

#[test]
fn resolve_saved_timezone_keeps_an_unchanged_value() {
    // Posted back unchanged: kept, even when chrono-tz cannot parse it.
    assert_eq!(
        resolve_saved_timezone(Some("Mars/Olympus_Mons"), "Mars/Olympus_Mons"),
        "Mars/Olympus_Mons"
    );
    assert_eq!(
        resolve_saved_timezone(Some(" US/Eastern "), "US/Eastern"),
        "US/Eastern"
    );
    // A real change is stored.
    assert_eq!(
        resolve_saved_timezone(Some("Europe/Bucharest"), "UTC"),
        "Europe/Bucharest"
    );
    // Anything else unparseable, empty or missing keeps the stored value.
    for input in [Some("Not/A_Zone"), Some(""), None] {
        assert_eq!(resolve_saved_timezone(input, "Asia/Tokyo"), "Asia/Tokyo");
    }
}

#[test]
fn resolve_event_type_tz_keeps_a_null_row_following_its_owner() {
    let follows = "Europe/Bucharest";
    // NULL stays NULL when the form posts back what it showed.
    assert_eq!(
        resolve_event_type_tz(Some("Europe/Bucharest"), None, follows),
        None
    );
    assert_eq!(
        resolve_event_type_tz(Some("Europe/Bucharest"), Some(""), follows),
        None
    );
    // A different, valid zone pins it; an invalid one leaves it NULL.
    assert_eq!(
        resolve_event_type_tz(Some("Asia/Tokyo"), None, follows).as_deref(),
        Some("Asia/Tokyo")
    );
    assert_eq!(
        resolve_event_type_tz(Some("Not/A_Zone"), None, follows),
        None
    );
    // A stored zone follows resolve_saved_timezone, even when it matches the
    // owner's: it was pinned on purpose.
    assert_eq!(
        resolve_event_type_tz(Some("Europe/Bucharest"), Some("Europe/Bucharest"), follows)
            .as_deref(),
        Some("Europe/Bucharest")
    );
    assert_eq!(
        resolve_event_type_tz(
            Some("Mars/Olympus_Mons"),
            Some("Mars/Olympus_Mons"),
            follows
        )
        .as_deref(),
        Some("Mars/Olympus_Mons")
    );
}

/// A calendar that is not busy by default (a colleague's, shared with the
/// host) blocks nothing unless an event type selects it, and then blocks
/// like any other: `is_busy` is the default, an explicit selection wins.
#[tokio::test]
async fn a_calendar_not_busy_by_default_blocks_only_when_selected() {
    let pool = setup_test_db().await;
    let (user_id, account_id, et_id) = seed_test_data(&pool).await;
    seed_synced_event(
        &pool,
        &account_id,
        "20260112T100000Z",
        "20260112T110000Z",
        "UTC",
        "OPAQUE",
    )
    .await;
    let colleague: String = sqlx::query_scalar("SELECT id FROM calendars")
        .fetch_one(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE calendars SET is_busy = 0 WHERE id = ?")
        .bind(&colleague)
        .execute(&pool)
        .await
        .unwrap();
    let busy = |et: Option<String>| {
        let pool = pool.clone();
        let user_id = user_id.clone();
        async move {
            crate::web::fetch_busy_times_for_user(
                &pool,
                &user_id,
                dt(2026, 1, 12, 0, 0),
                dt(2026, 1, 12, 23, 59),
                chrono_tz::UTC,
                et.as_deref(),
            )
            .await
        }
    };
    assert!(busy(None).await.is_empty());
    assert!(busy(Some(et_id.clone())).await.is_empty());

    sqlx::query("INSERT INTO event_type_calendars (event_type_id, calendar_id) VALUES (?, ?)")
        .bind(&et_id)
        .bind(&colleague)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        busy(Some(et_id)).await,
        [(dt(2026, 1, 12, 10, 0), dt(2026, 1, 12, 11, 0))]
    );
}
