//! A booking whose availability cannot be read is refused: every submit-time
//! re-check (new bookings on every route, guest reschedules, both approval
//! paths) answers the internal error page and changes nothing, while slot
//! grids keep degrading as before. A child of `web::tests`, so it shares that
//! module's helpers.

use super::booking::{app_for_route, booking_count, BookRoute, ALL_BOOK_ROUTES};
use super::*;

/// Make every read of `table` fail, the way a starved pool or a broken
/// database would.
async fn break_table(pool: &SqlitePool, table: &str) {
    sqlx::query(&format!("ALTER TABLE {table} RENAME TO {table}_unreadable"))
        .execute(pool)
        .await
        .unwrap();
}

async fn restore_table(pool: &SqlitePool, table: &str) {
    sqlx::query(&format!("ALTER TABLE {table}_unreadable RENAME TO {table}"))
        .execute(pool)
        .await
        .unwrap();
}

/// POST a booking for 10:00 on a free Monday; returns the response status.
async fn post_booking(app: &Router, uri: &str) -> u16 {
    let csrf = "csrf-fail-closed";
    let response = app
        .clone()
        .oneshot(post_form_unauthed(uri, csrf, &book_body(csrf)))
        .await
        .unwrap();
    response.status().as_u16()
}

/// The route's app, with the team event type switched to collective when
/// `collective` is set.
async fn app_for(route: BookRoute, collective: bool) -> (Router, SqlitePool, &'static str, String) {
    let (app, pool, uri, et_id) = app_for_route(route).await;
    if collective {
        sqlx::query("UPDATE event_types SET scheduling_mode = 'collective' WHERE id = ?")
            .bind(&et_id)
            .execute(&pool)
            .await
            .unwrap();
    }
    (app, pool, uri, et_id)
}

#[tokio::test]
async fn an_unreadable_calendar_refuses_the_booking_on_every_route() {
    let mut cases: Vec<(BookRoute, bool)> = ALL_BOOK_ROUTES.iter().map(|r| (*r, false)).collect();
    cases.push((BookRoute::Team, true));
    for (route, collective) in cases {
        let (app, pool, uri, et_id) = app_for(route, collective).await;
        break_table(&pool, "events").await;

        assert_eq!(
            post_booking(&app, uri).await,
            500,
            "{route:?} (collective: {collective}): an unreadable calendar must refuse"
        );
        assert_eq!(
            booking_count(&pool, &et_id).await,
            0,
            "{route:?} (collective: {collective}): nothing may be created"
        );

        // Control: the same POST once the calendar reads again.
        restore_table(&pool, "events").await;
        assert_eq!(post_booking(&app, uri).await, 200);
        assert_eq!(
            booking_count(&pool, &et_id).await,
            1,
            "{route:?} (collective: {collective}): the slot is bookable once readable"
        );
    }
}

/// Date overrides, attached resources and team members' working hours gate a
/// booking too: an unreadable one refuses it rather than lifting the block.
#[tokio::test]
async fn other_unreadable_availability_refuses_the_booking() {
    for (route, table) in [
        (BookRoute::User, "availability_overrides"),
        (BookRoute::User, "event_type_resources"),
        (BookRoute::Team, "user_availability_rules"),
    ] {
        let (app, pool, uri, et_id) = app_for(route, false).await;
        break_table(&pool, table).await;

        assert_eq!(post_booking(&app, uri).await, 500, "{route:?}, {table}");
        assert_eq!(booking_count(&pool, &et_id).await, 0, "{route:?}, {table}");

        restore_table(&pool, table).await;
        assert_eq!(post_booking(&app, uri).await, 200, "{route:?}, {table}");
        assert_eq!(booking_count(&pool, &et_id).await, 1, "{route:?}, {table}");
    }
}

/// Slot grids stay lenient: the page still renders with the calendar gone.
#[tokio::test]
async fn an_unreadable_calendar_leaves_the_slot_page_up() {
    let (app, pool, _, _) = setup_test_app().await;
    break_table(&pool, "events").await;
    let response = app.oneshot(get("/u/testuser/test-meeting")).await.unwrap();
    assert_eq!(response.status(), 200);
}

#[tokio::test]
async fn an_unreadable_calendar_refuses_a_guest_reschedule() {
    let (app, pool, _, et_id) = setup_test_app().await;
    let booking_id = insert_v1_booking(
        &pool,
        &et_id,
        "guest@test.com",
        Utc::now() + Duration::days(3),
        30,
        "confirmed",
        &[("reschedule_token", Some("resched-fail-closed".to_string()))],
    )
    .await;
    let start_before: String = sqlx::query_scalar("SELECT start_at FROM bookings WHERE id = ?")
        .bind(&booking_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    break_table(&pool, "events").await;

    let csrf = "csrf-reschedule-fail-closed";
    let body = format!(
        "_csrf={csrf}&date={}&time=10%3A00&tz=UTC",
        far_future_monday()
    );
    let reschedule = || {
        app.clone().oneshot(post_form_unauthed(
            "/booking/reschedule/resched-fail-closed",
            csrf,
            &body,
        ))
    };
    let response = reschedule().await.unwrap();
    assert_eq!(response.status(), 500);
    let start_after: String = sqlx::query_scalar("SELECT start_at FROM bookings WHERE id = ?")
        .bind(&booking_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(start_after, start_before, "the booking must not move");

    // Control: the same reschedule goes through once the calendar reads.
    restore_table(&pool, "events").await;
    let response = reschedule().await.unwrap();
    assert_eq!(response.status(), 200);
    let start_after: String = sqlx::query_scalar("SELECT start_at FROM bookings WHERE id = ?")
        .bind(&booking_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_ne!(
        start_after, start_before,
        "the control reschedule must move it"
    );
}

#[tokio::test]
async fn an_unreadable_calendar_refuses_both_approvals() {
    let (app, pool, session, et_id) = setup_test_app().await;
    let pending = insert_v1_booking(
        &pool,
        &et_id,
        "pending@test.com",
        Utc::now() + Duration::days(3),
        30,
        "pending",
        &[],
    )
    .await;
    break_table(&pool, "events").await;

    let csrf = "csrf-approve-fail-closed";
    let dashboard = app
        .clone()
        .oneshot(post_form(
            &format!("/dashboard/bookings/{pending}/confirm"),
            &session,
            csrf,
            &format!("_csrf={csrf}"),
        ))
        .await
        .unwrap();
    assert_eq!(dashboard.status(), 500);
    assert_eq!(booking_status(&pool, &pending).await, "pending");

    let token = app
        .clone()
        .oneshot(post_bare(&format!("/booking/approve/confirm-{pending}")))
        .await
        .unwrap();
    assert_eq!(token.status(), 500);
    assert_eq!(booking_status(&pool, &pending).await, "pending");

    // Control: the approval goes through once the calendar reads.
    restore_table(&pool, "events").await;
    let token = app
        .oneshot(post_bare(&format!("/booking/approve/confirm-{pending}")))
        .await
        .unwrap();
    assert_eq!(token.status(), 200);
    assert_eq!(booking_status(&pool, &pending).await, "confirmed");
}
