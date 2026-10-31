//! Shared resources: slot blocking, pick at booking time, attachment rules.

use super::*;

/// Insert a cached feed event. Compact iCal stamps: "YYYYMMDD" for
/// all-day events, "YYYYMMDDTHHMMSS" for timed ones.
async fn insert_resource_event(
    pool: &SqlitePool,
    resource_id: &str,
    start_at: &str,
    end_at: &str,
    status: Option<&str>,
    transp: Option<&str>,
) {
    let all_day = start_at.len() == 8;
    sqlx::query(
        "INSERT INTO resource_events (id, resource_id, uid, start_at, end_at, all_day, status, transp) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(resource_id)
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(start_at)
    .bind(end_at)
    .bind(all_day as i32)
    .bind(status)
    .bind(transp)
    .execute(pool)
    .await
    .unwrap();
}

/// Insert a confirmed booking carrying a resource assignment.
async fn insert_resource_booking(
    pool: &SqlitePool,
    et_id: &str,
    resource_id: &str,
    start: NaiveDateTime,
) {
    let end = start + Duration::minutes(30);
    sqlx::query("INSERT INTO bookings (id, event_type_id, uid, guest_name, guest_email, guest_timezone, start_at, end_at, status, cancel_token, reschedule_token, assigned_resource_id) VALUES (?, ?, ?, 'G', 'g@e.com', 'UTC', ?, ?, 'confirmed', ?, ?, ?)")
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(et_id)
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(start.format("%Y-%m-%dT%H:%M:%S").to_string())
        .bind(end.format("%Y-%m-%dT%H:%M:%S").to_string())
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(resource_id)
        .execute(pool).await.unwrap();
}

#[tokio::test]
async fn compute_slots_resource_all_mode_blocks_covered_days() {
    let pool = setup_test_db().await;
    let (_, _, et_id) = seed_test_data(&pool).await;

    let now = Utc::now().with_timezone(&Tz::UTC).naive_local();
    let mut next_monday = now.date() + Duration::days(1);
    while next_monday.weekday() != chrono::Weekday::Mon {
        next_monday += Duration::days(1);
    }
    let days_to_monday = (next_monday - now.date()).num_days() as i32;

    let resource = insert_resource(&pool, "Demo Lab").await;
    attach_resource(&pool, &et_id, &resource).await;
    // All-day feed event with exclusive DTEND: covers Monday and Tuesday.
    insert_resource_event(
        &pool,
        &resource,
        &next_monday.format("%Y%m%d").to_string(),
        &(next_monday + Duration::days(2))
            .format("%Y%m%d")
            .to_string(),
        None,
        None,
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
        3,
        BookingWindow::default(),
        Tz::UTC,
        Tz::UTC,
        BusySource::Individual(vec![]),
        None,
    )
    .await;

    for offset in 0..2 {
        let date = (next_monday + Duration::days(offset))
            .format("%Y-%m-%d")
            .to_string();
        let day = slot_days.iter().find(|d| d.date == date);
        assert!(
            day.map(|d| d.slots.is_empty()).unwrap_or(true),
            "{} is covered by the resource event and must expose no slots, got {:?}",
            date,
            day.map(|d| d.slots.len())
        );
    }
    let wednesday = (next_monday + Duration::days(2))
        .format("%Y-%m-%d")
        .to_string();
    let day = slot_days
        .iter()
        .find(|d| d.date == wednesday)
        .expect("Wednesday should be present");
    assert_eq!(
        day.slots.len(),
        16,
        "Wednesday is not covered and must keep all slots"
    );
}

#[tokio::test]
async fn compute_slots_resource_round_robin_one_free_keeps_slots() {
    let pool = setup_test_db().await;
    let (_, _, et_id) = seed_test_data(&pool).await;
    sqlx::query("UPDATE event_types SET resource_scheduling_mode = 'round_robin' WHERE id = ?")
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

    let busy_res = insert_resource(&pool, "Lab A").await;
    let free_res = insert_resource(&pool, "Lab B").await;
    attach_resource(&pool, &et_id, &busy_res).await;
    attach_resource(&pool, &et_id, &free_res).await;
    // Only Lab A is busy on Monday, Lab B stays free.
    insert_resource_event(
        &pool,
        &busy_res,
        &next_monday.format("%Y%m%d").to_string(),
        &(next_monday + Duration::days(1))
            .format("%Y%m%d")
            .to_string(),
        None,
        None,
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
        BookingWindow::default(),
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
        "round_robin with one free resource must not block any slot"
    );
}

#[tokio::test]
async fn compute_slots_resource_round_robin_all_busy_blocks() {
    let pool = setup_test_db().await;
    let (_, _, et_id) = seed_test_data(&pool).await;
    sqlx::query("UPDATE event_types SET resource_scheduling_mode = 'round_robin' WHERE id = ?")
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

    let start = next_monday.format("%Y%m%d").to_string();
    let end = (next_monday + Duration::days(1))
        .format("%Y%m%d")
        .to_string();
    for name in ["Lab A", "Lab B"] {
        let rid = insert_resource(&pool, name).await;
        attach_resource(&pool, &et_id, &rid).await;
        insert_resource_event(&pool, &rid, &start, &end, None, None).await;
    }

    let slot_days = compute_slots(
        &pool,
        &et_id,
        30,
        0,
        0,
        0,
        days_to_monday,
        1,
        BookingWindow::default(),
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
        "every resource busy in round_robin mode must block the day, got {:?}",
        monday.map(|d| d.slots.len())
    );
}

#[tokio::test]
async fn check_and_pick_all_mode_busy_resource_returns_busy() {
    let pool = setup_test_db().await;
    let (_, _, et_id) = seed_test_data(&pool).await;

    let now = Utc::now().with_timezone(&Tz::UTC).naive_local();
    let mut next_monday = now.date() + Duration::days(1);
    while next_monday.weekday() != chrono::Weekday::Mon {
        next_monday += Duration::days(1);
    }

    let resource = insert_resource(&pool, "Demo Lab").await;
    attach_resource(&pool, &et_id, &resource).await;
    insert_resource_event(
        &pool,
        &resource,
        &next_monday.format("%Y%m%d").to_string(),
        &(next_monday + Duration::days(1))
            .format("%Y%m%d")
            .to_string(),
        None,
        None,
    )
    .await;

    let check = crate::resources::check_and_pick(
        &pool,
        &et_id,
        next_monday.and_hms_opt(10, 0, 0).unwrap(),
        next_monday.and_hms_opt(10, 30, 0).unwrap(),
        Tz::UTC,
        None,
    )
    .await;
    assert_eq!(
        check,
        crate::resources::ResourceCheck::Busy,
        "'all' mode with the only resource busy must refuse the slot"
    );
}

#[tokio::test]
async fn check_and_pick_round_robin_returns_the_free_resource() {
    let pool = setup_test_db().await;
    let (_, _, et_id) = seed_test_data(&pool).await;
    sqlx::query("UPDATE event_types SET resource_scheduling_mode = 'round_robin' WHERE id = ?")
        .bind(&et_id)
        .execute(&pool)
        .await
        .unwrap();

    let now = Utc::now().with_timezone(&Tz::UTC).naive_local();
    let mut next_monday = now.date() + Duration::days(1);
    while next_monday.weekday() != chrono::Weekday::Mon {
        next_monday += Duration::days(1);
    }

    let busy_res = insert_resource(&pool, "Lab A").await;
    let free_res = insert_resource(&pool, "Lab B").await;
    attach_resource(&pool, &et_id, &busy_res).await;
    attach_resource(&pool, &et_id, &free_res).await;
    insert_resource_event(
        &pool,
        &busy_res,
        &next_monday.format("%Y%m%d").to_string(),
        &(next_monday + Duration::days(1))
            .format("%Y%m%d")
            .to_string(),
        None,
        None,
    )
    .await;

    let check = crate::resources::check_and_pick(
        &pool,
        &et_id,
        next_monday.and_hms_opt(10, 0, 0).unwrap(),
        next_monday.and_hms_opt(10, 30, 0).unwrap(),
        Tz::UTC,
        None,
    )
    .await;
    assert_eq!(
        check,
        crate::resources::ResourceCheck::Free {
            assigned: Some(free_res)
        },
        "round_robin must pick the free resource"
    );
}

// --- Per-resource team allowlist (#153) ---

async fn seed_team_with_admin(pool: &SqlitePool, slug: &str, admin_user_id: &str) -> String {
    let team_id = uuid::Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO teams (id, name, slug, visibility) VALUES (?, ?, ?, 'public')")
        .bind(&team_id)
        .bind(slug)
        .bind(slug)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO team_members (team_id, user_id, role, source) VALUES (?, ?, 'admin', 'direct')",
    )
    .bind(&team_id)
    .bind(admin_user_id)
    .execute(pool)
    .await
    .unwrap();
    team_id
}

async fn allow_resource_for_team(pool: &SqlitePool, resource_id: &str, team_id: &str) {
    sqlx::query("INSERT INTO resource_teams (resource_id, team_id) VALUES (?, ?)")
        .bind(resource_id)
        .bind(team_id)
        .execute(pool)
        .await
        .unwrap();
}

#[tokio::test]
async fn attachable_resources_respects_allowlist_and_roles() {
    let pool = setup_test_db().await;
    let admin = insert_role_user(&pool, "admin@allow.test", "admin").await;
    let team_admin = insert_role_user(&pool, "ta@allow.test", "user").await;
    let member = insert_role_user(&pool, "member@allow.test", "user").await;
    let team_id = seed_team_with_admin(&pool, "allow-sales", &team_admin).await;
    sqlx::query(
        "INSERT INTO team_members (team_id, user_id, role, source) VALUES (?, ?, 'member', 'direct')",
    )
    .bind(&team_id)
    .bind(&member)
    .execute(&pool)
    .await
    .unwrap();
    let allowed = insert_resource(&pool, "Lab A").await;
    let _other = insert_resource(&pool, "Lab B").await;
    allow_resource_for_team(&pool, &allowed, &team_id).await;

    let all = attachable_resources(&pool, &admin, true, None).await;
    assert_eq!(all.len(), 2, "global admin sees every resource");

    let ta = attachable_resources(&pool, &team_admin, false, Some(&team_id)).await;
    assert_eq!(
        ta.iter().map(|(id, _)| id.as_str()).collect::<Vec<_>>(),
        vec![allowed.as_str()],
        "team admin sees only the team's allowlist"
    );

    assert!(
        attachable_resources(&pool, &member, false, Some(&team_id))
            .await
            .is_empty(),
        "plain team member may not attach even allowlisted resources"
    );
    assert!(
        attachable_resources(&pool, &team_admin, false, None)
            .await
            .is_empty(),
        "personal event types stay global-admin only"
    );

    let any = attachable_resources_any_team(&pool, &team_admin).await;
    assert_eq!(any.len(), 1, "union helper covers the create forms");
    assert!(attachable_resources_any_team(&pool, &member)
        .await
        .is_empty());
}

#[tokio::test]
async fn save_resources_authorized_filters_and_preserves() {
    let pool = setup_test_db().await;
    let (_, _, et_id) = seed_test_data(&pool).await;
    let team_admin = insert_role_user(&pool, "ta2@allow.test", "user").await;
    let team_id = seed_team_with_admin(&pool, "allow-support", &team_admin).await;
    let allowed = insert_resource(&pool, "Allowed Lab").await;
    let preexisting = insert_resource(&pool, "Admin-only Lab").await;
    let smuggled = insert_resource(&pool, "Smuggled Lab").await;
    allow_resource_for_team(&pool, &allowed, &team_id).await;
    // A global admin attached a non-allowlisted resource earlier.
    attach_resource(&pool, &et_id, &preexisting).await;

    // Team admin submits an allowlisted id plus a smuggled one.
    save_event_type_resources_authorized(
        &pool,
        &et_id,
        &team_admin,
        false,
        Some(&team_id),
        &Some(format!("{},{}", allowed, smuggled)),
        &Some("round_robin".to_string()),
    )
    .await;

    let mut attached: Vec<String> = sqlx::query_as::<_, (String,)>(
        "SELECT resource_id FROM event_type_resources WHERE event_type_id = ?",
    )
    .bind(&et_id)
    .fetch_all(&pool)
    .await
    .unwrap()
    .into_iter()
    .map(|(id,)| id)
    .collect();
    attached.sort();
    let mut expected = vec![allowed.clone(), preexisting.clone()];
    expected.sort();
    assert_eq!(
        attached, expected,
        "smuggled id dropped, invisible attachment preserved"
    );
    let mode: String =
        sqlx::query_scalar("SELECT resource_scheduling_mode FROM event_types WHERE id = ?")
            .bind(&et_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(mode, "round_robin");

    // Deselecting everything detaches the allowlisted resource but
    // still preserves the one outside the allowlist.
    save_event_type_resources_authorized(
        &pool,
        &et_id,
        &team_admin,
        false,
        Some(&team_id),
        &Some(String::new()),
        &Some("all".to_string()),
    )
    .await;
    let attached: Vec<(String,)> =
        sqlx::query_as("SELECT resource_id FROM event_type_resources WHERE event_type_id = ?")
            .bind(&et_id)
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(attached, vec![(preexisting.clone(),)]);
}

#[tokio::test]
async fn save_resources_authorized_noop_without_allowlist() {
    let pool = setup_test_db().await;
    let (_, _, et_id) = seed_test_data(&pool).await;
    let team_admin = insert_role_user(&pool, "ta3@allow.test", "user").await;
    let team_id = seed_team_with_admin(&pool, "allow-empty", &team_admin).await;
    let resource = insert_resource(&pool, "Locked Lab").await;
    attach_resource(&pool, &et_id, &resource).await;

    // No allowlist for the team: the save is skipped entirely, mode
    // included, exactly like before the allowlist existed.
    save_event_type_resources_authorized(
        &pool,
        &et_id,
        &team_admin,
        false,
        Some(&team_id),
        &Some(String::new()),
        &Some("round_robin".to_string()),
    )
    .await;

    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM event_type_resources WHERE event_type_id = ?")
            .bind(&et_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(count, 1, "attachments untouched");
    let mode: String =
        sqlx::query_scalar("SELECT resource_scheduling_mode FROM event_types WHERE id = ?")
            .bind(&et_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(mode, "all", "mode untouched");
}

#[tokio::test]
async fn booking_resource_label_reflects_mode() {
    let pool = setup_test_db().await;
    let (_, _, et_id) = seed_test_data(&pool).await;
    let res_a = insert_resource(&pool, "Lab A").await;
    let res_b = insert_resource(&pool, "Lab B").await;
    attach_resource(&pool, &et_id, &res_a).await;
    attach_resource(&pool, &et_id, &res_b).await;

    let uid = "label-test@calrs";
    sqlx::query(
        "INSERT INTO bookings (id, event_type_id, uid, guest_name, guest_email, guest_timezone, start_at, end_at, status, cancel_token, reschedule_token)
             VALUES ('lb1', ?, ?, 'G', 'g@e.com', 'UTC', '2026-08-03T10:00:00', '2026-08-03T10:30:00', 'confirmed', 'ct', 'rt')",
    )
    .bind(&et_id)
    .bind(uid)
    .execute(&pool)
    .await
    .unwrap();

    // 'all' mode: every attached resource, sorted by name.
    assert_eq!(
        booking_resource_label(&pool, uid).await.as_deref(),
        Some("Lab A, Lab B")
    );

    // round_robin: only the assigned resource.
    sqlx::query("UPDATE event_types SET resource_scheduling_mode = 'round_robin' WHERE id = ?")
        .bind(&et_id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE bookings SET assigned_resource_id = ? WHERE uid = ?")
        .bind(&res_b)
        .bind(uid)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        booking_resource_label(&pool, uid).await.as_deref(),
        Some("Lab B")
    );

    // no resources attached: None.
    sqlx::query("DELETE FROM event_type_resources WHERE event_type_id = ?")
        .bind(&et_id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE bookings SET assigned_resource_id = NULL WHERE uid = ?")
        .bind(uid)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE event_types SET resource_scheduling_mode = 'all' WHERE id = ?")
        .bind(&et_id)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(booking_resource_label(&pool, uid).await, None);
}

#[tokio::test]
async fn sync_failure_records_error_and_success_would_clear_it() {
    let pool = setup_test_db().await;
    let rid = insert_resource(&pool, "Dead Lab").await;
    // Feed URL that fails validation instantly (no network involved).
    let res = crate::resources::sync_resource(&pool, &rid, "not-a-url").await;
    assert!(res.is_err());
    let err: Option<String> =
        sqlx::query_scalar("SELECT last_sync_error FROM resources WHERE id = ?")
            .bind(&rid)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(
        err.is_some(),
        "failed sync must record last_sync_error for the admin panel"
    );
}

#[tokio::test]
async fn check_and_pick_without_resources_returns_no_resources() {
    let pool = setup_test_db().await;
    let (_, _, et_id) = seed_test_data(&pool).await;

    let check = crate::resources::check_and_pick(
        &pool,
        &et_id,
        dt(2026, 8, 3, 10, 0),
        dt(2026, 8, 3, 10, 30),
        Tz::UTC,
        None,
    )
    .await;
    assert_eq!(check, crate::resources::ResourceCheck::NoResources);
}

#[tokio::test]
async fn check_and_pick_round_robin_picks_least_loaded_resource() {
    let pool = setup_test_db().await;
    let (_, _, et_id) = seed_test_data(&pool).await;
    sqlx::query("UPDATE event_types SET resource_scheduling_mode = 'round_robin' WHERE id = ?")
        .bind(&et_id)
        .execute(&pool)
        .await
        .unwrap();

    let now = Utc::now().with_timezone(&Tz::UTC).naive_local();
    let mut next_monday = now.date() + Duration::days(1);
    while next_monday.weekday() != chrono::Weekday::Mon {
        next_monday += Duration::days(1);
    }

    // Both resources are free at the checked slot, but Lab A already
    // carries two upcoming confirmed assignments at other times.
    let loaded_res = insert_resource(&pool, "Lab A").await;
    let idle_res = insert_resource(&pool, "Lab B").await;
    attach_resource(&pool, &et_id, &loaded_res).await;
    attach_resource(&pool, &et_id, &idle_res).await;
    insert_resource_booking(
        &pool,
        &et_id,
        &loaded_res,
        next_monday.and_hms_opt(9, 0, 0).unwrap(),
    )
    .await;
    insert_resource_booking(
        &pool,
        &et_id,
        &loaded_res,
        next_monday.and_hms_opt(11, 0, 0).unwrap(),
    )
    .await;

    let check = crate::resources::check_and_pick(
        &pool,
        &et_id,
        next_monday.and_hms_opt(10, 0, 0).unwrap(),
        next_monday.and_hms_opt(10, 30, 0).unwrap(),
        Tz::UTC,
        None,
    )
    .await;
    assert_eq!(
        check,
        crate::resources::ResourceCheck::Free {
            assigned: Some(idle_res)
        },
        "round_robin must prefer the resource with fewer upcoming assignments"
    );
}

#[tokio::test]
async fn check_and_pick_cross_event_type_booking_blocks_shared_resource() {
    // Two event types in 'all' mode share the same resource. A confirmed
    // booking on A must block B at the same time even without any
    // resource_event rows (no write-back involved).
    let pool = setup_test_db().await;
    let (_, account_id, et_a) = seed_test_data(&pool).await;
    let et_b = uuid::Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO event_types (id, account_id, slug, title, duration_min, buffer_before, buffer_after, min_notice_min, enabled) VALUES (?, ?, 'meeting-b', 'Meeting B', 30, 0, 0, 0, 1)")
        .bind(&et_b)
        .bind(&account_id)
        .execute(&pool).await.unwrap();

    let resource = insert_resource(&pool, "Demo Lab").await;
    attach_resource(&pool, &et_a, &resource).await;
    attach_resource(&pool, &et_b, &resource).await;

    let now = Utc::now().with_timezone(&Tz::UTC).naive_local();
    let mut next_monday = now.date() + Duration::days(1);
    while next_monday.weekday() != chrono::Weekday::Mon {
        next_monday += Duration::days(1);
    }
    insert_booking(
        &pool,
        &et_a,
        None,
        next_monday.and_hms_opt(10, 0, 0).unwrap(),
        "confirmed",
    )
    .await;

    let overlapping = crate::resources::check_and_pick(
        &pool,
        &et_b,
        next_monday.and_hms_opt(10, 0, 0).unwrap(),
        next_monday.and_hms_opt(10, 30, 0).unwrap(),
        Tz::UTC,
        None,
    )
    .await;
    assert_eq!(
        overlapping,
        crate::resources::ResourceCheck::Busy,
        "booking on A must block B at the overlapping time"
    );

    let disjoint = crate::resources::check_and_pick(
        &pool,
        &et_b,
        next_monday.and_hms_opt(14, 0, 0).unwrap(),
        next_monday.and_hms_opt(14, 30, 0).unwrap(),
        Tz::UTC,
        None,
    )
    .await;
    assert_eq!(
        disjoint,
        crate::resources::ResourceCheck::Free { assigned: None },
        "B must stay free outside A's booking"
    );
}

#[tokio::test]
async fn transparent_and_cancelled_resource_events_do_not_block() {
    let pool = setup_test_db().await;
    let (_, _, et_id) = seed_test_data(&pool).await;

    let now = Utc::now().with_timezone(&Tz::UTC).naive_local();
    let mut next_monday = now.date() + Duration::days(1);
    while next_monday.weekday() != chrono::Weekday::Mon {
        next_monday += Duration::days(1);
    }
    let start = next_monday.format("%Y%m%d").to_string();
    let end = (next_monday + Duration::days(1))
        .format("%Y%m%d")
        .to_string();

    let resource = insert_resource(&pool, "Demo Lab").await;
    attach_resource(&pool, &et_id, &resource).await;
    insert_resource_event(&pool, &resource, &start, &end, None, Some("TRANSPARENT")).await;
    insert_resource_event(&pool, &resource, &start, &end, Some("CANCELLED"), None).await;

    let busy = crate::resources::busy_for_resource(
        &pool,
        &resource,
        next_monday.and_hms_opt(0, 0, 0).unwrap(),
        (next_monday + Duration::days(1))
            .and_hms_opt(0, 0, 0)
            .unwrap(),
        Tz::UTC,
        None,
    )
    .await;
    assert!(
        busy.is_empty(),
        "TRANSPARENT and CANCELLED feed events must not produce busy intervals"
    );

    let check = crate::resources::check_and_pick(
        &pool,
        &et_id,
        next_monday.and_hms_opt(10, 0, 0).unwrap(),
        next_monday.and_hms_opt(10, 30, 0).unwrap(),
        Tz::UTC,
        None,
    )
    .await;
    assert_eq!(
        check,
        crate::resources::ResourceCheck::Free { assigned: None },
        "the slot must stay bookable"
    );
}

#[tokio::test]
async fn booking_form_post_assigns_round_robin_resource() {
    let (app, pool, _, et_id) = setup_test_app().await;

    let busy_res = insert_resource(&pool, "Lab A").await;
    let free_res = insert_resource(&pool, "Lab B").await;
    attach_resource(&pool, &et_id, &busy_res).await;
    attach_resource(&pool, &et_id, &free_res).await;
    sqlx::query("UPDATE event_types SET resource_scheduling_mode = 'round_robin' WHERE id = ?")
        .bind(&et_id)
        .execute(&pool)
        .await
        .unwrap();

    let now = Utc::now().with_timezone(&Tz::UTC).naive_local();
    let mut next_monday = now.date() + Duration::days(1);
    while next_monday.weekday() != chrono::Weekday::Mon {
        next_monday += Duration::days(1);
    }
    let date_str = next_monday.format("%Y-%m-%d").to_string();
    // Lab A is busy the whole Monday, so the pick must land on Lab B.
    insert_resource_event(
        &pool,
        &busy_res,
        &next_monday.format("%Y%m%d").to_string(),
        &(next_monday + Duration::days(1))
            .format("%Y%m%d")
            .to_string(),
        None,
        None,
    )
    .await;

    let csrf = "test-csrf-resource-book";
    let body = format!(
        "_csrf={}&date={}&time=10%3A00&name=Res+Guest&email=res%40test.com&notes=",
        csrf, date_str
    );
    let response = app
        .oneshot(post_form_unauthed(
            "/u/testuser/test-meeting/book",
            csrf,
            &body,
        ))
        .await
        .unwrap();

    let status = response.status();
    assert!(
        status == 200 || status.is_redirection(),
        "Booking should succeed, got {}",
        status
    );

    let assigned: (Option<String>,) = sqlx::query_as(
        "SELECT assigned_resource_id FROM bookings WHERE guest_email = 'res@test.com'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        assigned.0.as_deref(),
        Some(free_res.as_str()),
        "the booking must record the free round-robin resource"
    );
}
