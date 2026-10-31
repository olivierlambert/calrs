//! Duplicating an event type from the dashboard (#235).

use super::*;

/// Columns a copy is expected to differ on; every other column must match.
/// `created_by_user_id` and `sms_phone_mode` have tests of their own.
const OWN_COLUMNS: [&str; 7] = [
    "id",
    "slug",
    "title",
    "enabled",
    "created_at",
    "created_by_user_id",
    "sms_phone_mode",
];

const DISABLED_BANNER: &str = "This event type is disabled, so nobody can book it yet.";

async fn enabled(pool: &SqlitePool, et_id: &str) -> i32 {
    sqlx::query_scalar("SELECT enabled FROM event_types WHERE id = ?")
        .bind(et_id)
        .fetch_one(pool)
        .await
        .unwrap()
}

/// Renders the edit page with minijinja's `/` escaping undone, so URLs in
/// attributes can be matched as written.
async fn edit_page(app: Router, uri: &str, session: &str) -> String {
    body_string(app.oneshot(get_authed(uri, session)).await.unwrap())
        .await
        .replace("&#x2f;", "/")
}

async fn post_duplicate(app: Router, uri: &str, session: &str) -> axum::http::Response<Body> {
    let csrf = "test-csrf-duplicate";
    app.oneshot(post_form(uri, session, csrf, &format!("_csrf={}", csrf)))
        .await
        .unwrap()
}

fn location(response: &axum::http::Response<Body>) -> String {
    response
        .headers()
        .get("location")
        .map(|v| v.to_str().unwrap().to_string())
        .unwrap_or_default()
}

async fn et_id_by_slug(pool: &SqlitePool, slug: &str) -> Option<String> {
    sqlx::query_scalar("SELECT id FROM event_types WHERE slug = ?")
        .bind(slug)
        .fetch_optional(pool)
        .await
        .unwrap()
}

async fn count_where(pool: &SqlitePool, table: &str, et_id: &str) -> i64 {
    sqlx::query_scalar(&format!(
        "SELECT COUNT(*) FROM {table} WHERE event_type_id = ?"
    ))
    .bind(et_id)
    .fetch_one(pool)
    .await
    .unwrap()
}

/// Every `event_types` column outside `OWN_COLUMNS` holds the same value on
/// both rows, NULLs included. Reads the column list from the live schema so a
/// column added later is covered without touching this test.
async fn assert_same_settings(pool: &SqlitePool, original: &str, copy: &str) {
    let columns: Vec<(i64, String, String, i64, Option<String>, i64)> =
        sqlx::query_as("PRAGMA table_info(event_types)")
            .fetch_all(pool)
            .await
            .unwrap();
    assert!(
        columns.len() > 20,
        "schema should be the full event_types table"
    );
    for (_, name, ..) in columns {
        if OWN_COLUMNS.contains(&name.as_str()) {
            continue;
        }
        let same: i64 = sqlx::query_scalar(&format!(
            "SELECT (SELECT {name} FROM event_types WHERE id = ?) IS \
                    (SELECT {name} FROM event_types WHERE id = ?)"
        ))
        .bind(original)
        .bind(copy)
        .fetch_one(pool)
        .await
        .unwrap();
        assert_eq!(same, 1, "column {name} should be copied");
    }
}

/// Gives the seeded event type one of every child row a copy must carry.
async fn seed_children(pool: &SqlitePool, user_id: &str, et_id: &str) {
    sqlx::query(
        "UPDATE event_types SET description = 'd', location_type = 'phone', location_value = '+1',
         requires_confirmation = 1, reminder_minutes = 15, visibility = 'internal',
         max_additional_guests = 2, slot_interval_min = 15, first_slot_only = 1,
         timezone = 'Europe/Paris', pending_expiry_hours = 6, booking_horizon_days = 30
         WHERE id = ?",
    )
    .bind(et_id)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO availability_overrides (id, event_type_id, date, start_time, end_time, is_blocked)
         VALUES ('ov1', ?, '2030-01-02', '10:00', '12:00', 0)",
    )
    .bind(et_id)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO booking_frequency_limits (id, event_type_id, max_bookings, period, per_member)
         VALUES ('fl1', ?, 3, 'week', 1)",
    )
    .bind(et_id)
    .execute(pool)
    .await
    .unwrap();
    let source_id = seed_write_source(pool, user_id, "https://dav.invalid").await;
    sqlx::query(
        "INSERT INTO calendars (id, source_id, href, display_name) VALUES ('cal1', ?, '/c/', 'C')",
    )
    .bind(&source_id)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO event_type_calendars (event_type_id, calendar_id) VALUES (?, 'cal1')")
        .bind(et_id)
        .execute(pool)
        .await
        .unwrap();
    let resource_id = insert_resource(pool, "Lab").await;
    attach_resource(pool, et_id, &resource_id).await;
}

#[tokio::test]
async fn duplicate_copies_settings_and_children_and_opens_the_copy() {
    let (app, pool, session, et_id) = setup_test_app().await;
    let user_id: String = sqlx::query_scalar("SELECT user_id FROM accounts LIMIT 1")
        .fetch_one(&pool)
        .await
        .unwrap();
    seed_children(&pool, &user_id, &et_id).await;

    let response = post_duplicate(
        app,
        "/dashboard/event-types/test-meeting/duplicate",
        &session,
    )
    .await;

    assert!(response.status().is_redirection());
    assert_eq!(
        location(&response),
        "/dashboard/event-types/test-meeting-copy/edit"
    );

    let copy_id = et_id_by_slug(&pool, "test-meeting-copy")
        .await
        .expect("copy should exist");
    assert_same_settings(&pool, &et_id, &copy_id).await;

    let title: String = sqlx::query_scalar("SELECT title FROM event_types WHERE id = ?")
        .bind(&copy_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(title, "Test Meeting (copy)");
    assert_eq!(enabled(&pool, &copy_id).await, 0, "copy starts disabled");
    assert_eq!(enabled(&pool, &et_id).await, 1, "original stays enabled");

    for table in [
        "availability_rules",
        "availability_overrides",
        "event_type_calendars",
        "booking_frequency_limits",
        "event_type_resources",
    ] {
        assert_eq!(
            count_where(&pool, table, &copy_id).await,
            count_where(&pool, table, &et_id).await,
            "{table} rows should be copied"
        );
        assert!(
            count_where(&pool, table, &copy_id).await > 0,
            "{table} fixture is empty"
        );
    }

    let rules: Vec<(i64, String, String)> = sqlx::query_as(
        "SELECT day_of_week, start_time, end_time FROM availability_rules
         WHERE event_type_id = ? ORDER BY day_of_week",
    )
    .bind(&copy_id)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(rules[0], (1, "09:00".to_string(), "17:00".to_string()));

    let limit: (i64, String, i64) = sqlx::query_as(
        "SELECT max_bookings, period, per_member FROM booking_frequency_limits WHERE event_type_id = ?",
    )
    .bind(&copy_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(limit, (3, "week".to_string(), 1));

    // The copy is credited to whoever made it.
    let created_by: Option<String> =
        sqlx::query_scalar("SELECT created_by_user_id FROM event_types WHERE id = ?")
            .bind(&copy_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(created_by.as_deref(), Some(user_id.as_str()));

    // Copied child rows get UUID v4 ids like every other primary key.
    for table in [
        "availability_rules",
        "availability_overrides",
        "booking_frequency_limits",
    ] {
        let ids: Vec<String> =
            sqlx::query_scalar(&format!("SELECT id FROM {table} WHERE event_type_id = ?"))
                .bind(&copy_id)
                .fetch_all(&pool)
                .await
                .unwrap();
        for id in ids {
            let parsed = uuid::Uuid::parse_str(&id).expect("child id is a UUID");
            assert_eq!(parsed.get_version_num(), 4, "{table} id {id}");
            assert_eq!(parsed.to_string(), id, "{table} id is lowercase hyphenated");
        }
    }

    // The original keeps its children and its slug.
    assert_eq!(count_where(&pool, "availability_rules", &et_id).await, 5);
    assert!(et_id_by_slug(&pool, "test-meeting").await.is_some());
}

#[tokio::test]
async fn edit_form_of_a_disabled_event_type_offers_to_enable_it() {
    let (app, _pool, session, _) = setup_test_app().await;
    post_duplicate(
        app.clone(),
        "/dashboard/event-types/test-meeting/duplicate",
        &session,
    )
    .await;

    let copy = edit_page(
        app.clone(),
        "/dashboard/event-types/test-meeting-copy/edit",
        &session,
    )
    .await;
    assert!(
        copy.contains(DISABLED_BANNER),
        "banner on the disabled copy"
    );
    assert!(copy.contains(r#"action="/dashboard/event-types/test-meeting-copy/toggle""#));

    let original = edit_page(app, "/dashboard/event-types/test-meeting/edit", &session).await;
    assert!(
        !original.contains(DISABLED_BANNER),
        "no banner on an enabled event type"
    );
}

#[tokio::test]
async fn duplicate_picks_the_next_free_slug() {
    let (app, pool, session, _) = setup_test_app().await;

    let first = post_duplicate(
        app.clone(),
        "/dashboard/event-types/test-meeting/duplicate",
        &session,
    )
    .await;
    assert_eq!(
        location(&first),
        "/dashboard/event-types/test-meeting-copy/edit"
    );

    let second = post_duplicate(
        app,
        "/dashboard/event-types/test-meeting/duplicate",
        &session,
    )
    .await;
    assert_eq!(
        location(&second),
        "/dashboard/event-types/test-meeting-copy-2/edit"
    );

    assert!(et_id_by_slug(&pool, "test-meeting-copy-2").await.is_some());
    assert_eq!(event_type_count_all(&pool).await, 3);
}

#[tokio::test]
async fn duplicate_of_someone_elses_event_type_does_nothing() {
    let (app, pool, session, _) = setup_test_app().await;
    let other_id = insert_role_user(&pool, "other@x", "user").await;
    insert_personal_et(&pool, &other_id, "theirs").await;

    let response = post_duplicate(app, "/dashboard/event-types/theirs/duplicate", &session).await;

    assert!(response.status().is_redirection());
    assert_eq!(location(&response), "/dashboard/event-types");
    assert!(et_id_by_slug(&pool, "theirs-copy").await.is_none());
}

#[tokio::test]
async fn duplicate_team_event_type_needs_team_admin_and_copies_team_rows() {
    let pool = setup_test_db().await;
    let (admin_id, admin_session) =
        insert_test_user_with_session(&pool, "Admin", "ta@x", "ta").await;
    let (member_id, member_session) =
        insert_test_user_with_session(&pool, "Member", "tm@x", "tm").await;
    let (team_id, et_id) = insert_team_with_et(
        &pool,
        "team1",
        "intro",
        &[(&admin_id, "admin"), (&member_id, "member")],
    )
    .await;
    sqlx::query(
        "INSERT INTO event_type_member_weights (event_type_id, user_id, weight) VALUES (?, ?, 3)",
    )
    .bind(&et_id)
    .bind(&member_id)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO event_type_watchers (event_type_id, team_id) VALUES (?, ?)")
        .bind(&et_id)
        .bind(&team_id)
        .execute(&pool)
        .await
        .unwrap();
    let uri = format!("/dashboard/group-event-types/{team_id}/intro/duplicate");

    let app = create_router(pool.clone(), std::env::temp_dir(), [0u8; 32]).await;
    let denied = post_duplicate(app, &uri, &member_session).await;
    assert_eq!(location(&denied), "/dashboard/event-types");
    assert!(et_id_by_slug(&pool, "intro-copy").await.is_none());

    let app = create_router(pool.clone(), std::env::temp_dir(), [0u8; 32]).await;
    let allowed = post_duplicate(app, &uri, &admin_session).await;
    assert_eq!(
        location(&allowed),
        format!("/dashboard/group-event-types/{team_id}/intro-copy/edit")
    );

    let copy_id = et_id_by_slug(&pool, "intro-copy").await.expect("copy");
    assert_same_settings(&pool, &et_id, &copy_id).await;
    assert_eq!(
        count_where(&pool, "event_type_member_weights", &copy_id).await,
        1
    );
    assert_eq!(count_where(&pool, "event_type_watchers", &copy_id).await, 1);
    assert_eq!(enabled(&pool, &copy_id).await, 0);

    let app = create_router(pool.clone(), std::env::temp_dir(), [0u8; 32]).await;
    let page = edit_page(
        app,
        &format!("/dashboard/group-event-types/{team_id}/intro-copy/edit"),
        &admin_session,
    )
    .await;
    assert!(page.contains(DISABLED_BANNER));
    assert!(page.contains(&format!(
        r#"action="/dashboard/group-event-types/{team_id}/intro-copy/toggle""#
    )));
}

async fn event_type_count_all(pool: &SqlitePool) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM event_types")
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn set_role_and_sms(pool: &SqlitePool, et_id: &str, role: &str, sms_mode: &str) {
    sqlx::query("UPDATE users SET role = ? WHERE id = (SELECT user_id FROM accounts LIMIT 1)")
        .bind(role)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("UPDATE event_types SET sms_phone_mode = ? WHERE id = ?")
        .bind(sms_mode)
        .bind(et_id)
        .execute(pool)
        .await
        .unwrap();
}

async fn sms_mode_of(pool: &SqlitePool, slug: &str) -> String {
    sqlx::query_scalar("SELECT sms_phone_mode FROM event_types WHERE slug = ?")
        .bind(slug)
        .fetch_one(pool)
        .await
        .unwrap()
}

#[tokio::test]
async fn duplicate_by_a_user_who_may_not_enable_sms_starts_with_sms_off() {
    let (app, pool, session, et_id) = setup_test_app().await;
    set_role_and_sms(&pool, &et_id, "user", "required").await;
    let resource_id = insert_resource(&pool, "Lab").await;
    attach_resource(&pool, &et_id, &resource_id).await;

    post_duplicate(
        app,
        "/dashboard/event-types/test-meeting/duplicate",
        &session,
    )
    .await;

    assert_eq!(sms_mode_of(&pool, "test-meeting-copy").await, "off");
    assert_eq!(sms_mode_of(&pool, "test-meeting").await, "required");
    // Resources only restrict availability, so the copy keeps them.
    let copy_id = et_id_by_slug(&pool, "test-meeting-copy").await.unwrap();
    assert_eq!(
        count_where(&pool, "event_type_resources", &copy_id).await,
        1
    );
}

#[tokio::test]
async fn duplicate_keeps_sms_for_an_admin_or_when_all_users_may_enable_it() {
    let (app, pool, session, et_id) = setup_test_app().await;
    set_role_and_sms(&pool, &et_id, "admin", "optional").await;
    post_duplicate(
        app.clone(),
        "/dashboard/event-types/test-meeting/duplicate",
        &session,
    )
    .await;
    assert_eq!(sms_mode_of(&pool, "test-meeting-copy").await, "optional");

    set_role_and_sms(&pool, &et_id, "user", "optional").await;
    sqlx::query("UPDATE auth_config SET sms_allow_all_users = 1 WHERE id = 'singleton'")
        .execute(&pool)
        .await
        .unwrap();
    post_duplicate(
        app,
        "/dashboard/event-types/test-meeting/duplicate",
        &session,
    )
    .await;
    assert_eq!(sms_mode_of(&pool, "test-meeting-copy-2").await, "optional");
}

#[tokio::test]
async fn enabling_from_the_banner_returns_to_the_edit_form() {
    let (app, pool, session, _) = setup_test_app().await;
    post_duplicate(
        app.clone(),
        "/dashboard/event-types/test-meeting/duplicate",
        &session,
    )
    .await;
    let copy_id = et_id_by_slug(&pool, "test-meeting-copy").await.unwrap();

    let page = edit_page(
        app.clone(),
        "/dashboard/event-types/test-meeting-copy/edit",
        &session,
    )
    .await;
    assert!(page.contains(r#"name="then" value="edit""#));

    let csrf = "test-csrf-toggle";
    let response = app
        .clone()
        .oneshot(post_form(
            "/dashboard/event-types/test-meeting-copy/toggle",
            &session,
            csrf,
            &format!("_csrf={csrf}&then=edit"),
        ))
        .await
        .unwrap();
    assert_eq!(
        location(&response),
        "/dashboard/event-types/test-meeting-copy/edit"
    );
    assert_eq!(enabled(&pool, &copy_id).await, 1);

    // The list's own toggle, and any other `then`, still land on the list.
    for body in [format!("_csrf={csrf}"), format!("_csrf={csrf}&then=//evil")] {
        let response = app
            .clone()
            .oneshot(post_form(
                "/dashboard/event-types/test-meeting-copy/toggle",
                &session,
                csrf,
                &body,
            ))
            .await
            .unwrap();
        assert_eq!(location(&response), "/dashboard/event-types");
    }
}
