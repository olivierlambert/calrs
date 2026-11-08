//! A booking whose availability cannot be read is refused: every submit-time
//! re-check (new bookings on every route, guest reschedules, both approval
//! paths) answers the internal error page and changes nothing, while slot
//! grids keep degrading as before. A child of `web::tests`, so it shares that
//! module's helpers.

use super::booking::{app_for_route, booking_count, BookRoute, ALL_BOOK_ROUTES};
use super::booking_actions::seed_round_robin_reschedule;
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

/// Slot grids stay lenient: the personal, team and reschedule pages still
/// render with the calendar gone.
#[tokio::test]
async fn an_unreadable_calendar_leaves_the_slot_pages_up() {
    let (app, pool, _, _) = setup_test_app().await;
    seed_team_event_type(&pool).await;
    let (_, resched_tok, _) = seed_round_robin_reschedule(&pool).await;
    break_table(&pool, "events").await;
    for uri in [
        "/u/testuser/test-meeting".to_string(),
        "/team/horizon-team/team-sync".to_string(),
        format!("/booking/reschedule/{resched_tok}"),
    ] {
        let response = app.clone().oneshot(get(&uri)).await.unwrap();
        assert_eq!(response.status(), 200, "{uri}");
    }
}

/// A reschedule issues a new token, so the booking is found by its uid.
async fn start_at_of(pool: &SqlitePool, uid: &str) -> String {
    sqlx::query_scalar("SELECT start_at FROM bookings WHERE uid = ?")
        .bind(uid)
        .fetch_one(pool)
        .await
        .unwrap()
}

/// The team reschedule checks the assigned member strictly too.
#[tokio::test]
async fn an_unreadable_calendar_refuses_a_team_reschedule() {
    let (app, pool, _, _) = setup_test_app().await;
    let (_alice, resched_tok, target) = seed_round_robin_reschedule(&pool).await;
    let before = start_at_of(&pool, "uid-r166").await;
    break_table(&pool, "events").await;

    let csrf = "csrf-team-reschedule-fail-closed";
    let body = format!(
        "_csrf={csrf}&date={}&time=11%3A00&tz=UTC",
        target.format("%Y-%m-%d")
    );
    let uri = format!("/booking/reschedule/{resched_tok}");
    let reschedule = || app.clone().oneshot(post_form_unauthed(&uri, csrf, &body));
    assert_eq!(reschedule().await.unwrap().status(), 500);
    assert_eq!(
        start_at_of(&pool, "uid-r166").await,
        before,
        "the booking must not move"
    );

    // Control: 11:00 is free for the assigned member once the calendar reads.
    restore_table(&pool, "events").await;
    assert_eq!(reschedule().await.unwrap().status(), 200);
    let after = start_at_of(&pool, "uid-r166").await;
    assert!(
        after.starts_with(&target.format("%Y-%m-%dT11:00:00").to_string()),
        "the control reschedule must move it, got {after}"
    );
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
    let mut pending = Vec::new();
    for days in [3, 4] {
        pending.push(
            insert_v1_booking(
                &pool,
                &et_id,
                "pending@test.com",
                Utc::now() + Duration::days(days),
                30,
                "pending",
                &[],
            )
            .await,
        );
    }
    let (by_dashboard, by_token) = (&pending[0], &pending[1]);
    let dashboard_approval = || {
        let csrf = "csrf-approve-fail-closed";
        app.clone().oneshot(post_form(
            &format!("/dashboard/bookings/{by_dashboard}/confirm"),
            &session,
            csrf,
            &format!("_csrf={csrf}"),
        ))
    };
    let token_approval = || {
        app.clone()
            .oneshot(post_bare(&format!("/booking/approve/confirm-{by_token}")))
    };
    break_table(&pool, "events").await;

    assert_eq!(dashboard_approval().await.unwrap().status(), 500);
    assert_eq!(booking_status(&pool, by_dashboard).await, "pending");
    assert_eq!(token_approval().await.unwrap().status(), 500);
    assert_eq!(booking_status(&pool, by_token).await, "pending");

    // Control: both approvals go through once the calendar reads.
    restore_table(&pool, "events").await;
    assert!(dashboard_approval()
        .await
        .unwrap()
        .status()
        .is_redirection());
    assert_eq!(booking_status(&pool, by_dashboard).await, "confirmed");
    assert_eq!(token_approval().await.unwrap().status(), 200);
    assert_eq!(booking_status(&pool, by_token).await, "confirmed");
}
