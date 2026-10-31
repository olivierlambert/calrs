//! Event type forms, permissions, overrides and SMS policy.

use super::*;

async fn user_for(pool: &SqlitePool, user_id: &str) -> crate::models::User {
    sqlx::query_as("SELECT * FROM users WHERE id = ?")
        .bind(user_id)
        .fetch_one(pool)
        .await
        .unwrap()
}

#[tokio::test]
async fn can_manage_event_type_global_admin_yes_for_any_event() {
    let pool = setup_test_db().await;
    let admin_id = insert_role_user(&pool, "admin@x", "admin").await;
    let owner_id = insert_role_user(&pool, "owner@x", "user").await;
    let personal_et = insert_personal_et(&pool, &owner_id, "personal").await;
    let (_, team_et) = insert_team_with_et(&pool, "team1", "team-et", &[]).await;
    let admin = user_for(&pool, &admin_id).await;
    assert!(can_manage_event_type(&pool, &admin, &personal_et).await);
    assert!(can_manage_event_type(&pool, &admin, &team_et).await);
}

#[tokio::test]
async fn can_manage_event_type_personal_owner_yes_others_no() {
    let pool = setup_test_db().await;
    let owner_id = insert_role_user(&pool, "owner@x", "user").await;
    let other_id = insert_role_user(&pool, "other@x", "user").await;
    let et = insert_personal_et(&pool, &owner_id, "personal").await;
    let owner = user_for(&pool, &owner_id).await;
    let other = user_for(&pool, &other_id).await;
    assert!(can_manage_event_type(&pool, &owner, &et).await);
    assert!(!can_manage_event_type(&pool, &other, &et).await);
}

#[tokio::test]
async fn can_manage_event_type_team_admin_yes_member_no() {
    let pool = setup_test_db().await;
    let admin_id = insert_role_user(&pool, "ta@x", "user").await;
    let member_id = insert_role_user(&pool, "tm@x", "user").await;
    let outsider_id = insert_role_user(&pool, "out@x", "user").await;
    let (_, et) = insert_team_with_et(
        &pool,
        "team1",
        "team-et",
        &[(&admin_id, "admin"), (&member_id, "member")],
    )
    .await;
    let admin = user_for(&pool, &admin_id).await;
    let member = user_for(&pool, &member_id).await;
    let outsider = user_for(&pool, &outsider_id).await;
    assert!(can_manage_event_type(&pool, &admin, &et).await);
    assert!(!can_manage_event_type(&pool, &member, &et).await);
    assert!(!can_manage_event_type(&pool, &outsider, &et).await);
}

#[tokio::test]
async fn can_manage_event_type_team_admin_cannot_manage_others_personal_event() {
    // Being a team admin somewhere does not grant access to an unrelated
    // user's personal event types.
    let pool = setup_test_db().await;
    let team_admin_id = insert_role_user(&pool, "ta@x", "user").await;
    let stranger_id = insert_role_user(&pool, "s@x", "user").await;
    let stranger_et = insert_personal_et(&pool, &stranger_id, "p").await;
    let _ = insert_team_with_et(&pool, "team1", "te", &[(&team_admin_id, "admin")]).await;
    let team_admin = user_for(&pool, &team_admin_id).await;
    assert!(!can_manage_event_type(&pool, &team_admin, &stranger_et).await);
}

#[tokio::test]
async fn can_manage_event_type_team_member_cannot_manage_others_personal_event() {
    let pool = setup_test_db().await;
    let team_member_id = insert_role_user(&pool, "tm@x", "user").await;
    let stranger_id = insert_role_user(&pool, "s@x", "user").await;
    let stranger_et = insert_personal_et(&pool, &stranger_id, "p").await;
    let _ = insert_team_with_et(&pool, "team1", "te", &[(&team_member_id, "member")]).await;
    let team_member = user_for(&pool, &team_member_id).await;
    assert!(!can_manage_event_type(&pool, &team_member, &stranger_et).await);
}

#[tokio::test]
async fn can_manage_event_type_personal_owner_cannot_manage_unrelated_team_event() {
    let pool = setup_test_db().await;
    // Owner of a personal event with the same slug as some team event —
    // owning the personal event must not grant access to the team one.
    let owner_id = insert_role_user(&pool, "owner@x", "user").await;
    let _ = insert_personal_et(&pool, &owner_id, "demo").await;
    let (_, team_et) = insert_team_with_et(&pool, "team1", "demo", &[]).await;
    let owner = user_for(&pool, &owner_id).await;
    assert!(!can_manage_event_type(&pool, &owner, &team_et).await);
}

#[tokio::test]
async fn find_manageable_event_type_by_slug_personal_wins_collision() {
    // If the user has both a personal event and a team-admin event sharing
    // the same slug, the personal event must resolve first (deterministic).
    let pool = setup_test_db().await;
    let user_id = insert_role_user(&pool, "u@x", "user").await;
    let personal_et = insert_personal_et(&pool, &user_id, "demo").await;
    let (_, team_et) = insert_team_with_et(&pool, "team1", "demo", &[(&user_id, "admin")]).await;
    let resolved = find_manageable_event_type_by_slug(&pool, &user_id, "demo")
        .await
        .expect("should resolve");
    assert_eq!(resolved.0, personal_et);
    assert_ne!(resolved.0, team_et);
    assert!(resolved.2.is_none(), "personal event has no team_id");
}

#[tokio::test]
async fn find_manageable_event_type_by_slug_member_does_not_resolve() {
    // Regular team members should not resolve team event types via this
    // helper — only team admins do.
    let pool = setup_test_db().await;
    let user_id = insert_role_user(&pool, "m@x", "user").await;
    let _ = insert_team_with_et(&pool, "team1", "demo", &[(&user_id, "member")]).await;
    assert!(find_manageable_event_type_by_slug(&pool, &user_id, "demo")
        .await
        .is_none());
}

#[tokio::test]
async fn overrides_page_returns_200() {
    let (app, _, session, _) = setup_test_app().await;
    let response = app
        .oneshot(get_authed(
            "/dashboard/event-types/test-meeting/overrides",
            &session,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = body_string(response).await;
    assert!(
        body.contains("Date overrides"),
        "Overrides page should render"
    );
    assert!(
        body.contains("Test Meeting"),
        "Should show event type title"
    );
}

#[tokio::test]
async fn toggle_event_type_disables_and_enables() {
    let (app, pool, session, _) = setup_test_app().await;
    let csrf = "test-csrf-toggle";

    // Disable
    let response = app
        .oneshot(post_form(
            "/dashboard/event-types/test-meeting/toggle",
            &session,
            csrf,
            &format!("_csrf={}", csrf),
        ))
        .await
        .unwrap();
    assert!(
        response.status().is_redirection(),
        "Toggle should redirect, got {}",
        response.status()
    );

    let enabled: Option<(i32,)> =
        sqlx::query_as("SELECT enabled FROM event_types WHERE slug = 'test-meeting'")
            .fetch_optional(&pool)
            .await
            .unwrap();
    assert_eq!(enabled.unwrap().0, 0, "Event type should be disabled");

    // Re-enable (need a fresh router since oneshot consumes)
    let app2 = create_router(pool.clone(), std::env::temp_dir(), [0u8; 32]).await;
    let response = app2
        .oneshot(post_form(
            "/dashboard/event-types/test-meeting/toggle",
            &session,
            csrf,
            &format!("_csrf={}", csrf),
        ))
        .await
        .unwrap();
    assert!(response.status().is_redirection());

    let enabled: Option<(i32,)> =
        sqlx::query_as("SELECT enabled FROM event_types WHERE slug = 'test-meeting'")
            .fetch_optional(&pool)
            .await
            .unwrap();
    assert_eq!(enabled.unwrap().0, 1, "Event type should be re-enabled");
}

#[tokio::test]
async fn create_override_blocked_day() {
    let (app, pool, session, _) = setup_test_app().await;
    let csrf = "test-csrf-override";

    let response = app
        .oneshot(post_form(
            "/dashboard/event-types/test-meeting/overrides",
            &session,
            csrf,
            &format!("_csrf={}&date=2026-06-15&override_type=blocked", csrf),
        ))
        .await
        .unwrap();
    assert!(
        response.status().is_redirection(),
        "Create override should redirect"
    );

    let count: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM availability_overrides WHERE date = '2026-06-15' AND is_blocked = 1",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(count.0, 1, "Blocked override should be created");
}

#[tokio::test]
async fn create_override_custom_hours() {
    let (app, pool, session, _) = setup_test_app().await;
    let csrf = "test-csrf-custom";

    let response = app
        .oneshot(post_form(
            "/dashboard/event-types/test-meeting/overrides",
            &session,
            csrf,
            &format!(
                "_csrf={}&date=2026-06-16&override_type=custom&start_time=10%3A00&end_time=14%3A00",
                csrf
            ),
        ))
        .await
        .unwrap();
    assert!(response.status().is_redirection());

    let row: Option<(String, String, i32)> = sqlx::query_as(
        "SELECT start_time, end_time, is_blocked FROM availability_overrides WHERE date = '2026-06-16'",
    )
    .fetch_optional(&pool)
    .await
    .unwrap();
    let (start, end, blocked) = row.unwrap();
    assert_eq!(start, "10:00");
    assert_eq!(end, "14:00");
    assert_eq!(blocked, 0);
}

#[tokio::test]
async fn create_override_custom_hours_invalid_range_rejected() {
    let (app, pool, session, _) = setup_test_app().await;
    let csrf = "test-csrf-inv";

    // end_time before start_time
    let response = app
        .oneshot(post_form(
            "/dashboard/event-types/test-meeting/overrides",
            &session,
            csrf,
            &format!(
                "_csrf={}&date=2026-06-17&override_type=custom&start_time=14%3A00&end_time=10%3A00",
                csrf
            ),
        ))
        .await
        .unwrap();
    assert!(response.status().is_redirection());

    let count: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM availability_overrides WHERE date = '2026-06-17'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(count.0, 0, "Invalid time range should not create override");
}

#[tokio::test]
async fn delete_override_removes_from_db() {
    let (app, pool, session, _) = setup_test_app().await;

    // Insert an override to delete
    let override_id = uuid::Uuid::new_v4().to_string();
    let et_id: String =
        sqlx::query_scalar("SELECT id FROM event_types WHERE slug = 'test-meeting'")
            .fetch_one(&pool)
            .await
            .unwrap();
    sqlx::query("INSERT INTO availability_overrides (id, event_type_id, date, is_blocked) VALUES (?, ?, '2026-07-01', 1)")
        .bind(&override_id)
        .bind(&et_id)
        .execute(&pool)
        .await
        .unwrap();

    let csrf = "test-csrf-del";
    let response = app
        .oneshot(post_form(
            &format!(
                "/dashboard/event-types/test-meeting/overrides/{}/delete",
                override_id
            ),
            &session,
            csrf,
            &format!("_csrf={}", csrf),
        ))
        .await
        .unwrap();
    assert!(response.status().is_redirection());

    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM availability_overrides WHERE id = ?")
        .bind(&override_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count.0, 0, "Override should be deleted");
}

// --- Dashboard pages: new event type form ---

/// #265: the event type form offers every zone and preselects the host's,
/// even one outside the common list.
#[tokio::test]
async fn new_event_type_form_preselects_a_timezone_outside_the_common_list() {
    let (app, pool, session, _) = setup_test_app().await;
    sqlx::query("UPDATE users SET timezone = 'Europe/Bucharest' WHERE email = 'test@example.com'")
        .execute(&pool)
        .await
        .unwrap();
    let response = app
        .oneshot(get_authed("/dashboard/event-types/new", &session))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = body_string(response).await;
    assert_eq!(
        selected_options(&body, "event_type_timezone"),
        ["Europe/Bucharest"]
    );
    assert!(body.contains(r#"value="Asia&#x2f;Tokyo""#));
}

#[tokio::test]
async fn new_event_type_form_returns_200() {
    let (app, _, session, _) = setup_test_app().await;
    let response = app
        .oneshot(get_authed("/dashboard/event-types/new", &session))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = body_string(response).await;
    assert!(body.contains("event type") || body.contains("Event type") || body.contains("Create"));
}

#[tokio::test]
async fn edit_event_type_form_returns_200() {
    let (app, _, session, _) = setup_test_app().await;
    let response = app
        .oneshot(get_authed(
            "/dashboard/event-types/test-meeting/edit",
            &session,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = body_string(response).await;
    assert!(body.contains("Test Meeting"));
}

// --- Event type CRUD ---

#[tokio::test]
async fn create_event_type_via_post() {
    let (app, pool, session, _) = setup_test_app().await;
    let csrf = "test-csrf-create-et";
    let body = format!(
        "_csrf={}&title=New+Meeting&slug=new-meeting&duration_min=45&location_value=https%3A%2F%2Fmeet.example.com&avail_days=1,2,3,4,5&avail_start=09:00&avail_end=17:00",
        csrf
    );
    let response = app
        .oneshot(post_form(
            "/dashboard/event-types/new",
            &session,
            csrf,
            &body,
        ))
        .await
        .unwrap();
    assert!(
        response.status().is_redirection(),
        "Create event type should redirect, got {}",
        response.status()
    );

    let count: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM event_types WHERE slug = 'new-meeting'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(count.0, 1, "Event type should be created");
}

#[tokio::test]
async fn delete_event_type_with_no_bookings() {
    let (app, pool, session, _) = setup_test_app().await;
    let csrf = "test-csrf-del-et";

    // Create an event type to delete (no bookings)
    let et_id = uuid::Uuid::new_v4().to_string();
    let account_id: String = sqlx::query_scalar("SELECT id FROM accounts LIMIT 1")
        .fetch_one(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO event_types (id, account_id, slug, title, duration_min, enabled) VALUES (?, ?, 'deletable', 'Deletable', 30, 1)")
        .bind(&et_id)
        .bind(&account_id)
        .execute(&pool)
        .await
        .unwrap();

    let response = app
        .oneshot(post_form(
            "/dashboard/event-types/deletable/delete",
            &session,
            csrf,
            &format!("_csrf={}", csrf),
        ))
        .await
        .unwrap();
    assert!(response.status().is_redirection());

    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM event_types WHERE slug = 'deletable'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count.0, 0, "Event type should be deleted");
}

// --- Update event type ---

/// Saves the event type edit form at `uri` with only the timezone varying,
/// and returns the stored column.
async fn save_et_timezone(
    app: &Router,
    pool: &SqlitePool,
    session: &str,
    uri: &str,
    slug: &str,
    et_id: &str,
    tz: &str,
) -> Option<String> {
    let csrf = "test-csrf-et-tz-265";
    let body = format!(
        "_csrf={csrf}&title=Tz+Test&slug={slug}&duration_min=30&location_value=https%3A%2F%2Fmeet.example.com&avail_days=1,2,3,4,5&avail_start=09:00&avail_end=17:00&timezone={}",
        tz.replace('/', "%2F")
    );
    let response = app
        .clone()
        .oneshot(post_form(uri, session, csrf, &body))
        .await
        .unwrap();
    assert!(response.status().is_redirection() || response.status() == 200);
    sqlx::query_scalar("SELECT timezone FROM event_types WHERE id = ?")
        .bind(et_id)
        .fetch_one(pool)
        .await
        .unwrap()
}

/// #265: an event type with no timezone of its own follows its owner's.
/// Saving the edit form without touching the timezone used to pin it to
/// the owner's current one; it now stays NULL. A changed timezone is
/// stored, and a stored one chrono-tz cannot parse survives a save that
/// posts it back, while other unparseable input is refused.
#[tokio::test]
async fn edit_event_type_keeps_a_timezone_the_form_did_not_change() {
    let (app, pool, session, et_id) = setup_test_app().await;
    let (team_id, team_et) = seed_team_event_type(&pool).await;
    sqlx::query("UPDATE users SET timezone = 'Europe/Bucharest' WHERE username = 'testuser'")
        .execute(&pool)
        .await
        .unwrap();
    let personal = "/dashboard/event-types/test-meeting/edit".to_string();
    let team = format!("/dashboard/group-event-types/{team_id}/team-sync/edit");

    for (uri, slug, id) in [
        (&personal, "test-meeting", &et_id),
        (&team, "team-sync", &team_et),
    ] {
        let stored: Option<String> =
            sqlx::query_scalar("SELECT timezone FROM event_types WHERE id = ?")
                .bind(id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(stored, None, "{uri} starts without a timezone");

        // The form shows the timezone the event type follows.
        let response = app
            .clone()
            .oneshot(get_authed(uri, &session))
            .await
            .unwrap();
        let body = body_string(response).await;
        let shown = selected_options(&body, "event_type_timezone");
        assert_eq!(shown, ["Europe/Bucharest"], "{uri}");

        let save = |tz: &'static str| save_et_timezone(&app, &pool, &session, uri, slug, id, tz);
        assert_eq!(
            save("Europe/Bucharest").await,
            None,
            "{uri} keeps following"
        );
        assert_eq!(save("Asia/Tokyo").await.as_deref(), Some("Asia/Tokyo"));
        assert_eq!(save("Not/A_Zone").await.as_deref(), Some("Asia/Tokyo"));

        sqlx::query("UPDATE event_types SET timezone = 'Mars/Olympus_Mons' WHERE id = ?")
            .bind(id)
            .execute(&pool)
            .await
            .unwrap();
        assert_eq!(
            save("Mars/Olympus_Mons").await.as_deref(),
            Some("Mars/Olympus_Mons"),
            "{uri} keeps a stored value posted back unchanged"
        );
        assert_eq!(save("Europe/Paris").await.as_deref(), Some("Europe/Paris"));
    }
}

#[tokio::test]
async fn update_event_type_changes_title() {
    let (app, pool, session, _) = setup_test_app().await;
    let csrf = "test-csrf-update-et";
    let body = format!(
        "_csrf={}&title=Updated+Title&slug=test-meeting&duration_min=30&location_value=https%3A%2F%2Fmeet.example.com&avail_days=1,2,3,4,5&avail_start=09:00&avail_end=17:00",
        csrf
    );
    let response = app
        .oneshot(post_form(
            "/dashboard/event-types/test-meeting/edit",
            &session,
            csrf,
            &body,
        ))
        .await
        .unwrap();
    assert!(response.status().is_redirection() || response.status() == 200);

    let title: String =
        sqlx::query_scalar("SELECT title FROM event_types WHERE slug = 'test-meeting'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(title, "Updated Title");
}

#[tokio::test]
async fn update_event_type_with_empty_schedule_uses_user_default() {
    // Regression test for #68: when the event-type edit form is submitted
    // with an empty avail_schedule, the resulting availability_rules must
    // come from the user's profile-default schedule, not a hardcoded
    // Mon-Fri 09:00-17:00 fallback. Locks in the behaviour wired through
    // update_event_type so a future refactor can't silently regress it.
    let (app, pool, session, _) = setup_test_app().await;

    // Seed the test user's profile default with something distinctive
    // (Tue+Thu 14:00-18:00) so we can tell it apart from the legacy
    // hardcoded Mon-Fri 09:00-17:00 fallback.
    let user_id: String = sqlx::query_scalar("SELECT id FROM users WHERE username = 'testuser'")
        .fetch_one(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM user_availability_rules WHERE user_id = ?")
        .bind(&user_id)
        .execute(&pool)
        .await
        .unwrap();
    for day in [2_i32, 4] {
        sqlx::query(
            "INSERT INTO user_availability_rules (id, user_id, day_of_week, start_time, end_time) VALUES (?, ?, ?, '14:00', '18:00')",
        )
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(&user_id)
        .bind(day)
        .execute(&pool)
        .await
        .unwrap();
    }

    let csrf = "test-csrf-empty-schedule";
    // Submit with avail_schedule explicitly empty (matches what the form
    // sends when all days are unchecked) and no legacy fields.
    let body = format!(
        "_csrf={}&title=Test+Meeting&slug=test-meeting&duration_min=30&location_value=https%3A%2F%2Fmeet.example.com&avail_schedule=",
        csrf
    );
    let response = app
        .oneshot(post_form(
            "/dashboard/event-types/test-meeting/edit",
            &session,
            csrf,
            &body,
        ))
        .await
        .unwrap();
    assert!(response.status().is_redirection() || response.status() == 200);

    // Inspect the persisted availability_rules. Expect Tue+Thu 14:00-18:00
    // (the user's profile default), not Mon-Fri 09:00-17:00.
    let rules: Vec<(i32, String, String)> = sqlx::query_as(
        "SELECT day_of_week, start_time, end_time FROM availability_rules \
             WHERE event_type_id = (SELECT id FROM event_types WHERE slug = 'test-meeting') \
             ORDER BY day_of_week, start_time",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(
        rules,
        vec![
            (2, "14:00".to_string(), "18:00".to_string()),
            (4, "14:00".to_string(), "18:00".to_string()),
        ],
        "empty avail_schedule submission must fall back to the user's profile default, not the hardcoded Mon-Fri 09:00-17:00",
    );
}

#[tokio::test]
async fn update_group_event_type_persists_location() {
    let (app, pool, session, _) = setup_test_app().await;

    // Get user_id and account_id from the test user
    let (user_id, account_id): (String, String) = sqlx::query_as(
        "SELECT u.id, a.id FROM users u JOIN accounts a ON a.user_id = u.id WHERE u.username = 'testuser'",
    )
    .bind("testuser")
    .fetch_one(&pool)
    .await
    .unwrap();

    // Create a team with the test user as admin
    let team_id = uuid::Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO teams (id, name, slug, visibility, created_by) VALUES (?, 'Test Team', 'test-team', 'public', ?)")
        .bind(&team_id)
        .bind(&user_id)
        .execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO team_members (team_id, user_id, role, source) VALUES (?, ?, 'admin', 'direct')")
        .bind(&team_id)
        .bind(&user_id)
        .execute(&pool).await.unwrap();

    // Create a team event type with a location
    let et_id = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO event_types (id, account_id, slug, title, duration_min, buffer_before, buffer_after, min_notice_min, enabled, location_type, location_value, team_id, created_by_user_id) \
             VALUES (?, ?, 'team-meeting', 'Team Meeting', 30, 0, 0, 0, 1, 'link', 'https://meet.example.com/room', ?, ?)",
    )
    .bind(&et_id)
    .bind(&account_id)
    .bind(&team_id)
    .bind(&user_id)
    .execute(&pool).await.unwrap();

    // Update the event type via the web handler
    let csrf = "test-csrf-group-update";
    let body = format!(
        "_csrf={}&title=Team+Meeting+Updated&slug=team-meeting&duration_min=45&location_type=link&location_value=https%3A%2F%2Fmeet.example.com%2Fnew-room&avail_days=1,2,3,4,5&avail_start=09:00&avail_end=17:00&scheduling_mode=round_robin",
        csrf
    );
    let response = app
        .oneshot(post_form(
            &format!("/dashboard/group-event-types/{}/team-meeting/edit", team_id),
            &session,
            csrf,
            &body,
        ))
        .await
        .unwrap();
    assert!(
        response.status().is_redirection(),
        "Update should redirect, got {}",
        response.status()
    );

    // Verify all fields persisted
    let (title, location_value, duration): (String, Option<String>, i32) =
        sqlx::query_as("SELECT title, location_value, duration_min FROM event_types WHERE id = ?")
            .bind(&et_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(title, "Team Meeting Updated");
    assert_eq!(
        location_value.as_deref(),
        Some("https://meet.example.com/new-room")
    );
    assert_eq!(duration, 45);
}

// --- Multiple availability windows ---

#[tokio::test]
async fn create_event_type_with_multiple_windows() {
    let (app, pool, session, _) = setup_test_app().await;
    let csrf = "test-csrf-multi-win";
    let body = format!(
        "_csrf={}&title=Split+Day&slug=split-day&duration_min=30&location_value=https%3A%2F%2Fmeet.example.com&avail_days=1,2,3,4,5&avail_windows=09%3A00-12%3A00%2C13%3A00-17%3A00",
        csrf
    );
    let response = app
        .oneshot(post_form(
            "/dashboard/event-types/new",
            &session,
            csrf,
            &body,
        ))
        .await
        .unwrap();
    assert!(response.status().is_redirection() || response.status() == 200);

    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM event_types WHERE slug = 'split-day'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count.0, 1);

    // Should have rules with the split windows
    let rules: Vec<(String, String)> = sqlx::query_as(
        "SELECT start_time, end_time FROM availability_rules WHERE event_type_id = (SELECT id FROM event_types WHERE slug = 'split-day') ORDER BY start_time",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert!(rules.len() >= 5, "Should have rules for weekdays");
}

// --- Overrides page for nonexistent event type ---

#[tokio::test]
async fn overrides_nonexistent_event_type_redirects() {
    let (app, _, session, _) = setup_test_app().await;
    let response = app
        .oneshot(get_authed(
            "/dashboard/event-types/nonexistent/overrides",
            &session,
        ))
        .await
        .unwrap();
    assert!(
        response.status().is_redirection(),
        "Should redirect for nonexistent event type"
    );
}

// --- Overrides: multiple custom hours on same date ---

#[tokio::test]
async fn multiple_custom_hours_on_same_date() {
    let (app, pool, session, et_id) = setup_test_app().await;
    let csrf = "test-csrf-multi-override";

    // Add morning override
    let response = app
        .oneshot(post_form(
            "/dashboard/event-types/test-meeting/overrides",
            &session,
            csrf,
            &format!(
                "_csrf={}&date=2026-08-01&override_type=custom&start_time=08%3A00&end_time=12%3A00",
                csrf
            ),
        ))
        .await
        .unwrap();
    assert!(response.status().is_redirection());

    // Add afternoon override (need fresh router)
    let app2 = create_router(pool.clone(), std::env::temp_dir(), [0u8; 32]).await;
    let response = app2
        .oneshot(post_form(
            "/dashboard/event-types/test-meeting/overrides",
            &session,
            csrf,
            &format!(
                "_csrf={}&date=2026-08-01&override_type=custom&start_time=14%3A00&end_time=16%3A00",
                csrf
            ),
        ))
        .await
        .unwrap();
    assert!(response.status().is_redirection());

    let count: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM availability_overrides WHERE event_type_id = ? AND date = '2026-08-01'",
    )
    .bind(&et_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(count.0, 2, "Should have 2 custom hour overrides");
}

// --- Event type with all options ---

#[tokio::test]
async fn create_event_type_with_all_options() {
    let (app, pool, session, _) = setup_test_app().await;
    let csrf = "test-csrf-full-et";
    let body = format!(
        "_csrf={}&title=Full+Options&slug=full-options&duration_min=60&buffer_before=10&buffer_after=10&min_notice_min=120&requires_confirmation=on&location_type=link&location_value=https%3A%2F%2Fzoom.us%2Fmy-room&avail_days=1,2,3&avail_start=10:00&avail_end=16:00&reminder_minutes=15",
        csrf
    );
    let response = app
        .oneshot(post_form(
            "/dashboard/event-types/new",
            &session,
            csrf,
            &body,
        ))
        .await
        .unwrap();
    assert!(response.status().is_redirection() || response.status() == 200);

    let et: Option<(i32, i32, i32, i32, i32, String, String)> = sqlx::query_as(
        "SELECT duration_min, buffer_before, buffer_after, min_notice_min, requires_confirmation, location_type, location_value FROM event_types WHERE slug = 'full-options'",
    )
    .fetch_optional(&pool)
    .await
    .unwrap();
    let (dur, bb, ba, mn, rc, lt, lv) = et.unwrap();
    assert_eq!(dur, 60);
    assert_eq!(bb, 10);
    assert_eq!(ba, 10);
    assert_eq!(mn, 120);
    assert_eq!(rc, 1);
    assert_eq!(lt, "link");
    assert_eq!(lv, "https://zoom.us/my-room");
}

// --- Only permitted users may put an event type into an SMS mode ---

#[tokio::test]
async fn non_admins_cannot_enable_sms_unless_the_policy_allows_it() {
    let pool = setup_test_db().await;
    let (_user_id, account_id, et_id) = seed_test_data(&pool).await;

    // A plain member of the same instance.
    let member_id = uuid::Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO users (id, email, name, role, auth_provider, username, enabled) VALUES (?, 'member@example.com', 'Member', 'user', 'local', 'member', 1)")
        .bind(&member_id)
        .execute(&pool)
        .await
        .unwrap();
    let member: crate::models::User = sqlx::query_as("SELECT * FROM users WHERE id = ?")
        .bind(&member_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    let admin: crate::models::User =
        sqlx::query_as("SELECT * FROM users WHERE role = 'admin' LIMIT 1")
            .fetch_one(&pool)
            .await
            .unwrap();
    let _ = account_id;

    // Default policy: admins only.
    assert!(can_enable_sms(&pool, &admin).await);
    assert!(!can_enable_sms(&pool, &member).await);

    // A member's post is ignored on create, and carried forward on edit.
    assert_eq!(
        resolve_phone_mode(&pool, &member, Some("required"), None).await,
        "off"
    );
    sqlx::query("UPDATE event_types SET sms_phone_mode = 'optional' WHERE id = ?")
        .bind(&et_id)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        resolve_phone_mode(&pool, &member, Some("off"), Some(&et_id)).await,
        "optional",
        "a member must not be able to turn an admin's SMS setting off either"
    );

    // Opening the policy up lets members choose.
    sqlx::query("UPDATE auth_config SET sms_allow_all_users = 1 WHERE id = 'singleton'")
        .execute(&pool)
        .await
        .unwrap();
    assert!(can_enable_sms(&pool, &member).await);
    assert_eq!(
        resolve_phone_mode(&pool, &member, Some("required"), None).await,
        "required"
    );
}

// --- Form round-trip, personal and team ---

#[tokio::test]
async fn team_event_type_form_round_trip_persists_horizon() {
    // Reads config that CALRS_* env blocks override (see CLAUDE.md).
    let _env = crate::test_support::ENV_LOCK.lock().await;
    // PR #114 shipped a limit that only persisted on personal event types.
    // Assert the team UPDATE path carries the horizon too.
    let (app, pool, session, _) = setup_test_app().await;
    let (team_id, team_et_id) = seed_team_event_type(&pool).await;

    let csrf = "csrf-horizon-team-form";
    let body = format!(
        "_csrf={}&title=Team+Sync&slug=team-sync&duration_min=30&booking_horizon_days=14&location_type=link&location_value=https%3A%2F%2Fmeet.example.com%2Froom&avail_days=1,2,3,4,5&avail_start=09:00&avail_end=17:00&scheduling_mode=round_robin",
        csrf
    );
    let response = app
        .oneshot(post_form(
            &format!("/dashboard/group-event-types/{}/team-sync/edit", team_id),
            &session,
            csrf,
            &body,
        ))
        .await
        .unwrap();
    assert!(
        response.status().is_redirection(),
        "team update should redirect, got {}",
        response.status()
    );

    let stored: Option<i32> =
        sqlx::query_scalar("SELECT booking_horizon_days FROM event_types WHERE id = ?")
            .bind(&team_et_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        stored,
        Some(14),
        "the horizon must survive a team form round-trip"
    );
}

#[test]
fn parse_optional_day_count_treats_zero_as_a_value() {
    assert_eq!(parse_optional_day_count(""), None);
    assert_eq!(parse_optional_day_count("   "), None);
    assert_eq!(parse_optional_day_count("0"), Some(0));
    assert_eq!(parse_optional_day_count("14"), Some(14));
    assert_eq!(parse_optional_day_count("-1"), None);
    assert_eq!(parse_optional_day_count("abc"), None);
}

// --- Server-side validation of the event type form ---
//
// The red-team run created an event type with an empty title, the slug
// "Bad Slug!" silently rewritten to "bad-slug", and a duration of -5.

/// A valid event type form body, with `overrides` replacing (or adding)
/// fields. Values are URL-encoded here.
fn et_form_body(csrf: &str, overrides: &[(&str, &str)]) -> String {
    let mut fields: Vec<(&str, &str)> = vec![
        ("_csrf", csrf),
        ("title", "Checked Meeting"),
        ("slug", "checked-meeting"),
        ("duration_min", "45"),
        ("buffer_before", "0"),
        ("buffer_after", "0"),
        ("min_notice_min", "60"),
        ("location_value", "https://meet.example.com"),
        ("avail_days", "1,2,3,4,5"),
        ("avail_start", "09:00"),
        ("avail_end", "17:00"),
    ];
    for (key, value) in overrides {
        match fields.iter_mut().find(|(k, _)| k == key) {
            Some(field) => field.1 = value,
            None => fields.push((key, value)),
        }
    }
    fields
        .iter()
        .map(|(k, v)| format!("{k}={}", urlencoding::encode(v)))
        .collect::<Vec<_>>()
        .join("&")
}

/// Each bad input, the Fluent key it must be refused with.
const INVALID_EVENT_TYPE_INPUTS: &[(&str, &str, &str)] = &[
    ("title", "   ", "form-error-event-type-title-required"),
    ("slug", "Bad Slug!", "form-error-slug-charset"),
    ("slug", "caf\u{e9}", "form-error-slug-charset"),
    ("slug", "---", "form-error-slug-charset"),
    ("duration_min", "-5", "form-error-event-type-duration"),
    ("duration_min", "0", "form-error-event-type-duration"),
    ("duration_min", "1441", "form-error-event-type-duration"),
    ("duration_min", "abc", "form-error-event-type-duration"),
    ("buffer_before", "-10", "form-error-event-type-number"),
    ("buffer_after", "-1", "form-error-event-type-number"),
    ("min_notice_min", "-60", "form-error-event-type-number"),
    ("reminder_minutes", "-15", "form-error-event-type-number"),
    (
        "max_additional_guests",
        "-1",
        "form-error-event-type-number",
    ),
    (
        "booking_horizon_days",
        "-3",
        "form-error-event-type-horizon",
    ),
    (
        "booking_horizon_days",
        "soon",
        "form-error-event-type-horizon",
    ),
];

async fn event_type_count(pool: &SqlitePool) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM event_types")
        .fetch_one(pool)
        .await
        .unwrap()
}

/// The error page must show the message, and keep what was typed.
fn assert_form_error(page: &str, key: &str, context: &str) {
    let message = crate::i18n::translate("en", key, None);
    let escaped = message.replace('\'', "&#x27;");
    assert!(
        page.contains(&message) || page.contains(&escaped),
        "{context}: expected the {key} message on the re-rendered form"
    );
}

#[tokio::test]
async fn create_event_type_refuses_invalid_input() {
    let (app, pool, session, _) = setup_test_app().await;
    let before = event_type_count(&pool).await;
    let long_title = "x".repeat(256);
    let mut cases: Vec<(&str, &str, &str)> = INVALID_EVENT_TYPE_INPUTS.to_vec();
    cases.push(("title", &long_title, "form-error-event-type-title-length"));

    for (field, value, key) in cases {
        let csrf = "csrf-et-invalid";
        let overrides = if field == "title" {
            vec![(field, value)]
        } else {
            vec![("title", "Keep Me"), (field, value)]
        };
        let response = app
            .clone()
            .oneshot(post_form(
                "/dashboard/event-types/new",
                &session,
                csrf,
                &et_form_body(csrf, &overrides),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), 200, "{field}={value:?}");
        let page = body_string(response).await;
        assert_form_error(&page, key, &format!("{field}={value:?}"));
        if field != "title" {
            assert!(
                page.contains("Keep Me"),
                "{field}: the form keeps the title"
            );
        }
        assert_eq!(event_type_count(&pool).await, before, "{field}={value:?}");
    }

    // The exact red-team submission.
    let csrf = "csrf-et-redteam";
    let response = app
        .clone()
        .oneshot(post_form(
            "/dashboard/event-types/new",
            &session,
            csrf,
            &et_form_body(
                csrf,
                &[("title", ""), ("slug", "Bad Slug!"), ("duration_min", "-5")],
            ),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    assert_eq!(event_type_count(&pool).await, before);
}

#[tokio::test]
async fn create_event_type_normalizes_case_and_spaces_and_derives_a_blank_slug() {
    let (app, pool, session, _) = setup_test_app().await;
    for (slug, title, expected) in [
        ("  Team Sync ", "Whatever", "team-sync"),
        ("", "Quarterly Review: Q3!", "quarterly-review-q3"),
    ] {
        let csrf = "csrf-et-normalize";
        let response = app
            .clone()
            .oneshot(post_form(
                "/dashboard/event-types/new",
                &session,
                csrf,
                &et_form_body(csrf, &[("slug", slug), ("title", title)]),
            ))
            .await
            .unwrap();
        assert!(response.status().is_redirection(), "{slug:?}");
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM event_types WHERE slug = ?")
            .bind(expected)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 1, "{slug:?} should be stored as {expected}");
    }
}

#[tokio::test]
async fn edit_event_type_refuses_invalid_input_and_keeps_the_row() {
    let (app, pool, session, et_id) = setup_test_app().await;
    for (field, value, key) in INVALID_EVENT_TYPE_INPUTS {
        let csrf = "csrf-et-edit-invalid";
        let response = app
            .clone()
            .oneshot(post_form(
                "/dashboard/event-types/test-meeting/edit",
                &session,
                csrf,
                &et_form_body(csrf, &[("slug", "test-meeting"), (field, value)]),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), 200, "{field}={value:?}");
        assert_form_error(&body_string(response).await, key, field);
        let (slug, title, duration): (String, String, i32) =
            sqlx::query_as("SELECT slug, title, duration_min FROM event_types WHERE id = ?")
                .bind(&et_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(
            (slug.as_str(), title.as_str(), duration),
            ("test-meeting", "Test Meeting", 30),
            "{field}={value:?} must not change the event type"
        );
    }
}

#[tokio::test]
async fn team_event_type_create_and_edit_refuse_invalid_input() {
    let (app, pool, session, _) = setup_test_app().await;
    let (team_id, team_et_id) = seed_team_event_type(&pool).await;
    let before = event_type_count(&pool).await;

    for (field, value, key) in INVALID_EVENT_TYPE_INPUTS {
        let csrf = "csrf-team-et-invalid";
        let response = app
            .clone()
            .oneshot(post_form(
                "/dashboard/group-event-types/new",
                &session,
                csrf,
                &et_form_body(csrf, &[("team_id", &team_id), (field, value)]),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), 200, "create {field}={value:?}");
        assert_form_error(&body_string(response).await, key, field);
        assert_eq!(event_type_count(&pool).await, before, "{field}={value:?}");

        let response = app
            .clone()
            .oneshot(post_form(
                &format!("/dashboard/group-event-types/{team_id}/team-sync/edit"),
                &session,
                csrf,
                &et_form_body(csrf, &[("slug", "team-sync"), (field, value)]),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), 200, "edit {field}={value:?}");
        let (title, duration): (String, i32) =
            sqlx::query_as("SELECT title, duration_min FROM event_types WHERE id = ?")
                .bind(&team_et_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!((title.as_str(), duration), ("Team Sync", 30), "{field}");
    }

    // Control: a valid team create goes through.
    let csrf = "csrf-team-et-ok";
    let response = app
        .oneshot(post_form(
            "/dashboard/group-event-types/new",
            &session,
            csrf,
            &et_form_body(csrf, &[("team_id", &team_id)]),
        ))
        .await
        .unwrap();
    assert!(response.status().is_redirection());
    assert_eq!(event_type_count(&pool).await, before + 1);
}

// --- Slug uniqueness across personal and team event types (#313) ---

/// A plain user with an account of their own; returns the account id.
async fn user_with_account(pool: &SqlitePool, email: &str) -> String {
    let user = insert_role_user(pool, email, "user").await;
    let account = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO accounts (id, name, email, timezone, user_id) VALUES (?, ?, ?, 'UTC', ?)",
    )
    .bind(&account)
    .bind(email)
    .bind(email)
    .bind(&user)
    .execute(pool)
    .await
    .unwrap();
    account
}

/// A team event type is stored under its creator's account, so it cannot
/// reuse a slug the creator already has: the form says so instead of
/// redirecting as if it had saved.
#[tokio::test]
async fn team_create_refuses_a_slug_the_creator_already_uses() {
    let (app, pool, session, _) = setup_test_app().await;
    let (team_id, _) = seed_team_event_type(&pool).await;
    let before = event_type_count(&pool).await;

    let csrf = "csrf-team-slug-account";
    let response = app
        .oneshot(post_form(
            "/dashboard/group-event-types/new",
            &session,
            csrf,
            &et_form_body(csrf, &[("team_id", &team_id), ("slug", "test-meeting")]),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let page = body_string(response).await;
    assert_form_error(&page, "form-error-event-type-slug-taken", "team create");
    assert!(
        page.contains("Checked Meeting"),
        "the form keeps what was typed"
    );
    assert_eq!(event_type_count(&pool).await, before);
}

/// A slug taken inside the team re-renders the form with the team message,
/// rather than a bare text page.
#[tokio::test]
async fn team_create_refuses_a_slug_taken_in_the_team_on_the_form() {
    let (app, pool, session, _) = setup_test_app().await;
    let (team_id, _) = seed_team_event_type(&pool).await;
    let before = event_type_count(&pool).await;

    let csrf = "csrf-team-slug-team";
    let response = app
        .oneshot(post_form(
            "/dashboard/group-event-types/new",
            &session,
            csrf,
            &et_form_body(csrf, &[("team_id", &team_id), ("slug", "team-sync")]),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let page = body_string(response).await;
    assert_form_error(
        &page,
        "form-error-event-type-slug-taken-team",
        "team create",
    );
    assert!(
        page.contains("Checked Meeting"),
        "the form keeps what was typed"
    );
    assert!(
        page.contains(&format!("<option value=\"{team_id}\" selected>")),
        "the team stays selected, so the resubmit creates a team event type"
    );
    assert_eq!(event_type_count(&pool).await, before);
}

/// Renaming a team event type to a slug its creator already uses fails the
/// same way, and leaves every field as it was rather than half-saved.
#[tokio::test]
async fn team_edit_refuses_a_slug_the_creator_already_uses() {
    let (app, pool, session, _) = setup_test_app().await;
    let (team_id, team_et_id) = seed_team_event_type(&pool).await;

    let csrf = "csrf-team-slug-edit";
    let response = app
        .oneshot(post_form(
            &format!("/dashboard/group-event-types/{team_id}/team-sync/edit"),
            &session,
            csrf,
            &et_form_body(csrf, &[("slug", "test-meeting")]),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    assert_form_error(
        &body_string(response).await,
        "form-error-event-type-slug-taken",
        "team edit",
    );
    let (slug, title): (String, String) =
        sqlx::query_as("SELECT slug, title FROM event_types WHERE id = ?")
            .bind(&team_et_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!((slug.as_str(), title.as_str()), ("team-sync", "Team Sync"));
}

/// A personal event type cannot take a slug the user's own team event type
/// already holds under their account.
#[tokio::test]
async fn personal_create_refuses_a_slug_of_the_users_team_event_type() {
    let (app, pool, session, _) = setup_test_app().await;
    seed_team_event_type(&pool).await;
    let before = event_type_count(&pool).await;

    let csrf = "csrf-personal-slug-team";
    let response = app
        .oneshot(post_form(
            "/dashboard/event-types/new",
            &session,
            csrf,
            &et_form_body(csrf, &[("slug", "team-sync")]),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    assert_form_error(
        &body_string(response).await,
        "form-error-event-type-slug-taken",
        "personal create",
    );
    assert_eq!(event_type_count(&pool).await, before);
}

/// The schema itself refuses two event types of one team with the same slug,
/// even when different members created them.
#[tokio::test]
async fn schema_refuses_a_duplicate_slug_within_a_team() {
    let pool = setup_test_db().await;
    seed_test_data(&pool).await;
    let (team_id, _) = seed_team_event_type(&pool).await;
    let other_account = user_with_account(&pool, "other-member@x").await;

    let result = sqlx::query(
        "INSERT INTO event_types (id, account_id, slug, title, duration_min, team_id) VALUES ('dup', ?, 'team-sync', 'Dup', 30, ?)",
    )
    .bind(&other_account)
    .bind(&team_id)
    .execute(&pool)
    .await;
    assert!(result.is_err(), "duplicate team slug must be refused");
}

/// Migration 074 repairs a database that already holds two event types of one
/// team with the same slug: the oldest keeps it, later ones get the start of
/// their id appended, and the unique index can then be created.
#[tokio::test]
async fn migration_074_renames_duplicate_team_slugs_before_indexing() {
    let pool = setup_test_db().await;
    seed_test_data(&pool).await;
    let (team_id, original) = seed_team_event_type(&pool).await;
    sqlx::query("UPDATE event_types SET created_at = '2020-01-01 00:00:00' WHERE id = ?")
        .bind(&original)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::raw_sql("DROP INDEX idx_event_types_team_slug")
        .execute(&pool)
        .await
        .unwrap();

    let mut later = Vec::new();
    for (n, email) in ["dup-a@x", "dup-b@x"].iter().enumerate() {
        let account = user_with_account(&pool, email).await;
        let id = format!("{n}aaaaaaa-dup-{n}");
        sqlx::query(
            "INSERT INTO event_types (id, account_id, slug, title, duration_min, team_id, created_at)
             VALUES (?, ?, 'team-sync', 'Dup', 30, ?, '2021-01-01 00:00:00')",
        )
        .bind(&id)
        .bind(&account)
        .bind(&team_id)
        .execute(&pool)
        .await
        .unwrap();
        later.push(id);
    }

    sqlx::raw_sql(include_str!(
        "../../../migrations/074_team_event_type_slug_unique.sql"
    ))
    .execute(&pool)
    .await
    .unwrap();

    let slug_of = |id: String| {
        let pool = pool.clone();
        async move {
            sqlx::query_scalar::<_, String>("SELECT slug FROM event_types WHERE id = ?")
                .bind(id)
                .fetch_one(&pool)
                .await
                .unwrap()
        }
    };
    assert_eq!(
        slug_of(original).await,
        "team-sync",
        "the oldest keeps its URL"
    );
    for id in later {
        assert_eq!(slug_of(id.clone()).await, format!("team-sync-{}", &id[..8]));
    }
    let index: Option<String> = sqlx::query_scalar(
        "SELECT name FROM sqlite_master WHERE type = 'index' AND name = 'idx_event_types_team_slug'",
    )
    .fetch_optional(&pool)
    .await
    .unwrap();
    assert!(index.is_some());
}

/// A write refused by a slug constraint (a slug taken between the pre-check
/// and the write) maps to the matching form message; anything else does not,
/// so the handler returns a 500 instead of reporting a save.
#[tokio::test]
async fn a_refused_write_maps_to_the_matching_slug_message() {
    let pool = setup_test_db().await;
    seed_test_data(&pool).await;
    let (team_id, _) = seed_team_event_type(&pool).await;
    let other_account = user_with_account(&pool, "racer@x").await;
    let account: String =
        sqlx::query_scalar("SELECT account_id FROM event_types WHERE slug = 'team-sync'")
            .fetch_one(&pool)
            .await
            .unwrap();

    let team_dup = sqlx::query(
        "INSERT INTO event_types (id, account_id, slug, title, duration_min, team_id) VALUES ('r1', ?, 'team-sync', 'R', 30, ?)",
    )
    .bind(&other_account)
    .bind(&team_id)
    .execute(&pool)
    .await
    .unwrap_err();
    assert_eq!(
        crate::web::event_types::slug_taken_key(&team_dup),
        Some("form-error-event-type-slug-taken-team")
    );

    let account_dup = sqlx::query(
        "INSERT INTO event_types (id, account_id, slug, title, duration_min) VALUES ('r2', ?, 'test-meeting', 'R', 30)",
    )
    .bind(&account)
    .execute(&pool)
    .await
    .unwrap_err();
    assert_eq!(
        crate::web::event_types::slug_taken_key(&account_dup),
        Some("form-error-event-type-slug-taken")
    );

    let other = sqlx::query("INSERT INTO event_types (id) VALUES ('r3')")
        .execute(&pool)
        .await
        .unwrap_err();
    assert_eq!(crate::web::event_types::slug_taken_key(&other), None);
}

// --- The error re-render keeps every setting of the form it replaces ---

/// A team edit that fails validation re-renders with the posted scheduling
/// mode, watchers and member card: a resubmit must not turn a collective
/// event type round-robin or clear its watchers.
#[tokio::test]
async fn team_edit_error_keeps_mode_watchers_and_members() {
    let (app, pool, session, _) = setup_test_app().await;
    let (team_id, team_et_id) = seed_team_event_type(&pool).await;
    let watcher = uuid::Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO teams (id, name, slug, visibility) VALUES (?, 'Watchers', 'watchers', 'public')")
        .bind(&watcher)
        .execute(&pool)
        .await
        .unwrap();

    let csrf = "csrf-team-edit-context";
    let response = app
        .oneshot(post_form(
            &format!("/dashboard/group-event-types/{team_id}/team-sync/edit"),
            &session,
            csrf,
            &et_form_body(
                csrf,
                &[
                    ("slug", "renamed-sync"),
                    ("duration_min", "-5"),
                    ("scheduling_mode", "collective"),
                    ("watcher_team_ids", &watcher),
                ],
            ),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let page = body_string(response).await;
    assert_form_error(&page, "form-error-event-type-duration", "team edit");
    assert!(
        page.contains("<option value=\"collective\" selected>"),
        "mode kept"
    );
    assert!(
        page.contains(&format!("id=\"watcher_team_ids\" value=\"{watcher}\"")),
        "watchers kept"
    );
    assert!(page.contains("id=\"priority-card\""), "member card shown");
    // The posted slug was invalid, so nothing was renamed: the member
    // buttons, which save at once, must still target the stored slug.
    assert!(
        page.contains("/team-sync/priority/"),
        "priority URL uses the stored slug"
    );
    assert!(!page.contains("/renamed-sync/priority/"));
    let duration: i32 = sqlx::query_scalar("SELECT duration_min FROM event_types WHERE id = ?")
        .bind(&team_et_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(duration, 30, "nothing saved");
}

/// A personal edit that fails validation keeps the reminder, the guest limit
/// and the calendar selection the user posted.
#[tokio::test]
async fn personal_edit_error_keeps_reminder_guests_and_calendars() {
    let (app, pool, session, _) = setup_test_app().await;
    let user_id: String = sqlx::query_scalar("SELECT id FROM users WHERE username = 'testuser'")
        .fetch_one(&pool)
        .await
        .unwrap();
    let source_id = seed_write_source(&pool, &user_id, "https://dav.invalid").await;
    for cal in ["cal-a", "cal-b"] {
        sqlx::query("INSERT INTO calendars (id, source_id, href, display_name, is_busy) VALUES (?, ?, ?, ?, 1)")
            .bind(cal)
            .bind(&source_id)
            .bind(format!("/{cal}/"))
            .bind(cal)
            .execute(&pool)
            .await
            .unwrap();
    }

    let csrf = "csrf-personal-edit-context";
    let response = app
        .oneshot(post_form(
            "/dashboard/event-types/test-meeting/edit",
            &session,
            csrf,
            &et_form_body(
                csrf,
                &[
                    ("slug", "test-meeting"),
                    ("duration_min", "-5"),
                    ("reminder_minutes", "60"),
                    ("max_additional_guests", "3"),
                    ("calendar_ids", "cal-a,cal-b"),
                ],
            ),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let page = body_string(response).await;
    assert_form_error(&page, "form-error-event-type-duration", "personal edit");
    assert!(
        page.contains("<option value=\"60\" selected>"),
        "reminder kept"
    );
    assert!(
        page.contains("<option value=\"3\" selected>"),
        "guest limit kept"
    );
    assert!(
        page.contains("id=\"calendar_ids\" value=\"cal-a,cal-b\""),
        "calendar selection kept"
    );
}

/// A personal-form create aimed at a team that fails validation keeps the
/// team selected, instead of resubmitting as a personal event type.
#[tokio::test]
async fn create_error_keeps_the_chosen_team() {
    let (app, pool, session, _) = setup_test_app().await;
    let (team_id, _) = seed_team_event_type(&pool).await;
    let member: String = sqlx::query_scalar("SELECT id FROM users WHERE username = 'testuser'")
        .fetch_one(&pool)
        .await
        .unwrap();

    let csrf = "csrf-create-team-context";
    let response = app
        .oneshot(post_form(
            "/dashboard/event-types/new",
            &session,
            csrf,
            &et_form_body(
                csrf,
                &[
                    ("team_id", &team_id),
                    ("duration_min", "-5"),
                    ("member_priorities", &format!("{member}:3")),
                ],
            ),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let page = body_string(response).await;
    assert_form_error(&page, "form-error-event-type-duration", "create");
    // The member card starts from the posted priorities, not the defaults.
    assert!(
        page.contains(&format!("\"{member}:3\"")),
        "posted priorities echoed"
    );
    assert!(page.contains(&format!("<option value=\"{team_id}\" selected>")));
}
