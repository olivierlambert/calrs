//! Token pages: approve, decline, guest cancel, reschedule, notice windows, ICS.

use super::*;

#[tokio::test]
async fn reschedule_token_lookup_finds_confirmed_booking() {
    let pool = setup_test_db().await;
    let (_, _, et_id) = seed_test_data(&pool).await;

    let (bid, token, _) = insert_test_booking(
        &pool,
        &et_id,
        "2026-03-16T10:00:00",
        "2026-03-16T10:30:00",
        "confirmed",
    )
    .await;

    let found: Option<(String,)> = sqlx::query_as(
        "SELECT id FROM bookings WHERE reschedule_token = ? AND status IN ('confirmed', 'pending')",
    )
    .bind(&token)
    .fetch_optional(&pool)
    .await
    .unwrap();

    assert!(found.is_some(), "Should find booking by reschedule_token");
    assert_eq!(found.unwrap().0, bid);
}

#[tokio::test]
async fn reschedule_token_lookup_finds_pending_booking() {
    let pool = setup_test_db().await;
    let (_, _, et_id) = seed_test_data(&pool).await;

    let (bid, token, _) = insert_test_booking(
        &pool,
        &et_id,
        "2026-03-16T10:00:00",
        "2026-03-16T10:30:00",
        "pending",
    )
    .await;

    let found: Option<(String,)> = sqlx::query_as(
        "SELECT id FROM bookings WHERE reschedule_token = ? AND status IN ('confirmed', 'pending')",
    )
    .bind(&token)
    .fetch_optional(&pool)
    .await
    .unwrap();

    assert!(
        found.is_some(),
        "Should find pending booking by reschedule_token"
    );
    assert_eq!(found.unwrap().0, bid);
}

#[tokio::test]
async fn reschedule_token_lookup_rejects_cancelled_booking() {
    let pool = setup_test_db().await;
    let (_, _, et_id) = seed_test_data(&pool).await;

    let (_, token, _) = insert_test_booking(
        &pool,
        &et_id,
        "2026-03-16T10:00:00",
        "2026-03-16T10:30:00",
        "cancelled",
    )
    .await;

    let found: Option<(String,)> = sqlx::query_as(
        "SELECT id FROM bookings WHERE reschedule_token = ? AND status IN ('confirmed', 'pending')",
    )
    .bind(&token)
    .fetch_optional(&pool)
    .await
    .unwrap();

    assert!(
        found.is_none(),
        "Cancelled booking should not be found by reschedule_token"
    );
}

#[tokio::test]
async fn reschedule_token_lookup_rejects_declined_booking() {
    let pool = setup_test_db().await;
    let (_, _, et_id) = seed_test_data(&pool).await;

    let (_, token, _) = insert_test_booking(
        &pool,
        &et_id,
        "2026-03-16T10:00:00",
        "2026-03-16T10:30:00",
        "declined",
    )
    .await;

    let found: Option<(String,)> = sqlx::query_as(
        "SELECT id FROM bookings WHERE reschedule_token = ? AND status IN ('confirmed', 'pending')",
    )
    .bind(&token)
    .fetch_optional(&pool)
    .await
    .unwrap();

    assert!(
        found.is_none(),
        "Declined booking should not be found by reschedule_token"
    );
}

#[tokio::test]
async fn reschedule_updates_times_and_regenerates_tokens() {
    let pool = setup_test_db().await;
    let (_, _, et_id) = seed_test_data(&pool).await;

    let (bid, old_resched, old_cancel) = insert_test_booking(
        &pool,
        &et_id,
        "2026-03-16T10:00:00",
        "2026-03-16T10:30:00",
        "confirmed",
    )
    .await;

    // Simulate a guest reschedule: update times, regenerate tokens, set pending
    let new_resched = uuid::Uuid::new_v4().to_string();
    let new_cancel = uuid::Uuid::new_v4().to_string();
    let new_confirm = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "UPDATE bookings SET start_at = ?, end_at = ?, status = 'pending',
                    reschedule_token = ?, cancel_token = ?, confirm_token = ?,
                    reminder_sent_at = NULL
             WHERE id = ?",
    )
    .bind("2026-03-17T14:00:00")
    .bind("2026-03-17T14:30:00")
    .bind(&new_resched)
    .bind(&new_cancel)
    .bind(&new_confirm)
    .bind(&bid)
    .execute(&pool)
    .await
    .unwrap();

    // Verify the booking was updated
    let (start_at, end_at, status, resched_tok, cancel_tok, confirm_tok): (String, String, String, String, String, Option<String>) =
        sqlx::query_as("SELECT start_at, end_at, status, reschedule_token, cancel_token, confirm_token FROM bookings WHERE id = ?")
        .bind(&bid)
        .fetch_one(&pool).await.unwrap();

    assert_eq!(start_at, "2026-03-17T14:00:00");
    assert_eq!(end_at, "2026-03-17T14:30:00");
    assert_eq!(status, "pending");
    assert_eq!(resched_tok, new_resched);
    assert_ne!(
        resched_tok, old_resched,
        "Reschedule token should be regenerated"
    );
    assert_eq!(cancel_tok, new_cancel);
    assert_ne!(cancel_tok, old_cancel, "Cancel token should be regenerated");
    assert_eq!(confirm_tok.unwrap(), new_confirm);
}

#[tokio::test]
async fn stale_reschedule_token_returns_no_match() {
    let pool = setup_test_db().await;
    let (_, _, et_id) = seed_test_data(&pool).await;

    let (bid, old_token, _) = insert_test_booking(
        &pool,
        &et_id,
        "2026-03-16T10:00:00",
        "2026-03-16T10:30:00",
        "confirmed",
    )
    .await;

    // Regenerate the token (simulating a prior reschedule)
    let new_token = uuid::Uuid::new_v4().to_string();
    sqlx::query("UPDATE bookings SET reschedule_token = ? WHERE id = ?")
        .bind(&new_token)
        .bind(&bid)
        .execute(&pool)
        .await
        .unwrap();

    // Old token should no longer find anything
    let found: Option<(String,)> = sqlx::query_as(
        "SELECT id FROM bookings WHERE reschedule_token = ? AND status IN ('confirmed', 'pending')",
    )
    .bind(&old_token)
    .fetch_optional(&pool)
    .await
    .unwrap();

    assert!(
        found.is_none(),
        "Stale reschedule token should not find any booking"
    );

    // New token should work
    let found_new: Option<(String,)> = sqlx::query_as(
        "SELECT id FROM bookings WHERE reschedule_token = ? AND status IN ('confirmed', 'pending')",
    )
    .bind(&new_token)
    .fetch_optional(&pool)
    .await
    .unwrap();

    assert!(
        found_new.is_some(),
        "New reschedule token should find booking"
    );
}

#[tokio::test]
async fn host_initiated_reschedule_stays_confirmed() {
    let pool = setup_test_db().await;
    let (_, _, et_id) = seed_test_data(&pool).await;

    let (bid, _, _) = insert_test_booking(
        &pool,
        &et_id,
        "2026-03-16T10:00:00",
        "2026-03-16T10:30:00",
        "confirmed",
    )
    .await;

    // Step 1: Host initiates reschedule (sets flag, regenerates token)
    let new_resched = uuid::Uuid::new_v4().to_string();
    sqlx::query("UPDATE bookings SET reschedule_token = ?, reschedule_by_host = 1 WHERE id = ?")
        .bind(&new_resched)
        .bind(&bid)
        .execute(&pool)
        .await
        .unwrap();

    let by_host: i32 = sqlx::query_scalar("SELECT reschedule_by_host FROM bookings WHERE id = ?")
        .bind(&bid)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(by_host, 1, "reschedule_by_host should be set");

    // Step 2: Guest picks a new time — should stay confirmed since host initiated
    let new_cancel = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "UPDATE bookings SET start_at = ?, end_at = ?, status = 'confirmed',
                    reschedule_token = ?, cancel_token = ?, confirm_token = NULL,
                    reminder_sent_at = NULL, reschedule_by_host = 0
             WHERE id = ?",
    )
    .bind("2026-03-18T09:00:00")
    .bind("2026-03-18T09:30:00")
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(&new_cancel)
    .bind(&bid)
    .execute(&pool)
    .await
    .unwrap();

    let (status, confirm_tok, by_host2): (String, Option<String>, i32) = sqlx::query_as(
        "SELECT status, confirm_token, reschedule_by_host FROM bookings WHERE id = ?",
    )
    .bind(&bid)
    .fetch_one(&pool)
    .await
    .unwrap();

    assert_eq!(
        status, "confirmed",
        "Host-initiated reschedule should keep confirmed status after guest picks time"
    );
    assert!(
        confirm_tok.is_none(),
        "No confirm_token needed for host-initiated reschedule"
    );
    assert_eq!(
        by_host2, 0,
        "reschedule_by_host should be reset after completion"
    );
}

#[tokio::test]
async fn reschedule_clears_reminder_sent_at() {
    let pool = setup_test_db().await;
    let (_, _, et_id) = seed_test_data(&pool).await;

    let (bid, _, _) = insert_test_booking(
        &pool,
        &et_id,
        "2026-03-16T10:00:00",
        "2026-03-16T10:30:00",
        "confirmed",
    )
    .await;

    // Set reminder_sent_at
    sqlx::query("UPDATE bookings SET reminder_sent_at = '2026-03-15T08:00:00' WHERE id = ?")
        .bind(&bid)
        .execute(&pool)
        .await
        .unwrap();

    // Reschedule
    sqlx::query(
        "UPDATE bookings SET start_at = '2026-03-17T10:00:00', end_at = '2026-03-17T10:30:00',
                    reschedule_token = ?, cancel_token = ?, reminder_sent_at = NULL
             WHERE id = ?",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(&bid)
    .execute(&pool)
    .await
    .unwrap();

    let reminder: Option<Option<String>> =
        sqlx::query_scalar("SELECT reminder_sent_at FROM bookings WHERE id = ?")
            .bind(&bid)
            .fetch_optional(&pool)
            .await
            .unwrap();

    assert_eq!(
        reminder,
        Some(None),
        "reminder_sent_at should be cleared after reschedule"
    );
}

#[tokio::test]
async fn booking_with_invalid_cancel_token_shows_error() {
    let (app, _, _, _) = setup_test_app().await;
    let response = app
        .oneshot(get("/booking/cancel/invalid-token-123"))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = body_string(response).await;
    assert!(
        body.contains("invalid") || body.contains("Invalid") || body.contains("expired"),
        "Invalid cancel token should show error"
    );
}

#[tokio::test]
async fn booking_with_invalid_approve_token_shows_error() {
    let (app, _, _, _) = setup_test_app().await;
    let response = app
        .oneshot(get("/booking/approve/invalid-token-123"))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = body_string(response).await;
    assert!(
        body.contains("invalid") || body.contains("Invalid") || body.contains("expired"),
        "Invalid approve token should show error"
    );
}

#[tokio::test]
async fn booking_with_invalid_reschedule_token_shows_error() {
    let (app, _, _, _) = setup_test_app().await;
    let response = app
        .oneshot(get("/booking/reschedule/invalid-token-123"))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = body_string(response).await;
    assert!(
        body.contains("invalid") || body.contains("Invalid") || body.contains("expired"),
        "Invalid reschedule token should show error"
    );
}

#[tokio::test]
async fn approve_booking_via_email_token() {
    let (app, pool, _, et_id) = setup_test_app().await;

    let booking_id = uuid::Uuid::new_v4().to_string();
    let cancel_tok = uuid::Uuid::new_v4().to_string();
    let resched_tok = uuid::Uuid::new_v4().to_string();
    let confirm_tok = uuid::Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO bookings (id, event_type_id, uid, guest_name, guest_email, guest_timezone, start_at, end_at, status, cancel_token, reschedule_token, confirm_token) VALUES (?, ?, 'uid-approve', 'Guest', 'guest@test.com', 'UTC', '2026-06-20T10:00:00', '2026-06-20T10:30:00', 'pending', ?, ?, ?)")
        .bind(&booking_id)
        .bind(&et_id)
        .bind(&cancel_tok)
        .bind(&resched_tok)
        .bind(&confirm_tok)
        .execute(&pool)
        .await
        .unwrap();

    // GET /booking/approve/{token} — shows confirmation form, does NOT approve
    let app2 = app.clone();
    let response = app2
        .oneshot(get(&format!("/booking/approve/{}", confirm_tok)))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = body_string(response).await;
    assert!(
        body.contains("Approve booking"),
        "GET should show approve form"
    );

    let status: Option<(String,)> = sqlx::query_as("SELECT status FROM bookings WHERE id = ?")
        .bind(&booking_id)
        .fetch_optional(&pool)
        .await
        .unwrap();
    assert_eq!(
        status.unwrap().0,
        "pending",
        "Booking should still be pending after GET"
    );

    // POST /booking/approve/{token} — actually approves
    let response = app
        .oneshot(post_bare(&format!("/booking/approve/{}", confirm_tok)))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);

    let status: Option<(String,)> = sqlx::query_as("SELECT status FROM bookings WHERE id = ?")
        .bind(&booking_id)
        .fetch_optional(&pool)
        .await
        .unwrap();
    assert_eq!(
        status.unwrap().0,
        "confirmed",
        "Booking should be confirmed via POST token"
    );
}

/// Regression test for #101: when the event type's timezone differs from
/// the guest's timezone, approving a pending booking must render (and
/// email) the time in the guest's timezone, not the event-type-local time
/// the row was stored as.
#[tokio::test]
async fn approve_booking_renders_time_in_guest_timezone() {
    let (app, pool, _, et_id) = setup_test_app().await;

    // Event type stored in America/New_York. 10:00 NY in June (EDT, UTC-4)
    // == 14:00 UTC == 16:00 in Europe/Paris (CEST, UTC+2).
    sqlx::query("UPDATE event_types SET timezone = 'America/New_York' WHERE id = ?")
        .bind(&et_id)
        .execute(&pool)
        .await
        .unwrap();

    let booking_id = uuid::Uuid::new_v4().to_string();
    let cancel_tok = uuid::Uuid::new_v4().to_string();
    let resched_tok = uuid::Uuid::new_v4().to_string();
    let confirm_tok = uuid::Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO bookings (id, event_type_id, uid, guest_name, guest_email, guest_timezone, start_at, end_at, status, cancel_token, reschedule_token, confirm_token) VALUES (?, ?, 'uid-approve-tz', 'Guest', 'guest@test.com', 'Europe/Paris', '2026-06-20T10:00:00', '2026-06-20T10:30:00', 'pending', ?, ?, ?)")
        .bind(&booking_id)
        .bind(&et_id)
        .bind(&cancel_tok)
        .bind(&resched_tok)
        .bind(&confirm_tok)
        .execute(&pool)
        .await
        .unwrap();

    let response = app
        .oneshot(post_bare(&format!("/booking/approve/{}", confirm_tok)))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = body_string(response).await;

    assert!(
        body.contains("16:00") && body.contains("16:30"),
        "Approval page should render times in the guest's tz (Europe/Paris: 16:00 – 16:30), got body fragment: {}",
        &body[..body.len().min(2000)]
    );
    assert!(
        !body.contains("10:00") && !body.contains("10:30"),
        "Approval page should not render the event-type-local time (America/New_York: 10:00)"
    );
}

#[tokio::test]
async fn decline_booking_via_email_token() {
    let (app, pool, _, et_id) = setup_test_app().await;

    let booking_id = uuid::Uuid::new_v4().to_string();
    let cancel_tok = uuid::Uuid::new_v4().to_string();
    let resched_tok = uuid::Uuid::new_v4().to_string();
    let confirm_tok = uuid::Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO bookings (id, event_type_id, uid, guest_name, guest_email, guest_timezone, start_at, end_at, status, cancel_token, reschedule_token, confirm_token) VALUES (?, ?, 'uid-decline', 'Guest', 'guest@test.com', 'UTC', '2026-06-21T10:00:00', '2026-06-21T10:30:00', 'pending', ?, ?, ?)")
        .bind(&booking_id)
        .bind(&et_id)
        .bind(&cancel_tok)
        .bind(&resched_tok)
        .bind(&confirm_tok)
        .execute(&pool)
        .await
        .unwrap();

    // GET /booking/decline/{token} shows form
    let response = app
        .oneshot(get(&format!("/booking/decline/{}", confirm_tok)))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = body_string(response).await;
    assert!(
        body.contains("ecline") || body.contains("reason"),
        "Decline form should render"
    );
}

#[tokio::test]
async fn guest_cancel_via_token() {
    let (app, pool, _, et_id) = setup_test_app().await;

    let booking_id = uuid::Uuid::new_v4().to_string();
    let cancel_tok = uuid::Uuid::new_v4().to_string();
    let resched_tok = uuid::Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO bookings (id, event_type_id, uid, guest_name, guest_email, guest_timezone, start_at, end_at, status, cancel_token, reschedule_token) VALUES (?, ?, 'uid-gcancel', 'Guest', 'guest@test.com', 'UTC', '2026-06-22T10:00:00', '2026-06-22T10:30:00', 'confirmed', ?, ?)")
        .bind(&booking_id)
        .bind(&et_id)
        .bind(&cancel_tok)
        .bind(&resched_tok)
        .execute(&pool)
        .await
        .unwrap();

    // GET /booking/cancel/{token} shows cancel form
    let response = app
        .oneshot(get(&format!("/booking/cancel/{}", cancel_tok)))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = body_string(response).await;
    assert!(
        body.contains("cancel") || body.contains("Cancel"),
        "Cancel form should render"
    );
}

/// Regression test for #101 (cancel surface): when the event type is in a
/// different tz from the guest, the cancellation page (and the cancellation
/// email it mirrors) must render the time in the guest's tz, not the
/// event-type-local stored time.
#[tokio::test]
async fn guest_cancel_renders_time_in_guest_timezone() {
    let (app, pool, _, et_id) = setup_test_app().await;

    sqlx::query("UPDATE event_types SET timezone = 'America/New_York' WHERE id = ?")
        .bind(&et_id)
        .execute(&pool)
        .await
        .unwrap();

    let booking_id = uuid::Uuid::new_v4().to_string();
    let cancel_tok = uuid::Uuid::new_v4().to_string();
    let resched_tok = uuid::Uuid::new_v4().to_string();
    // 10:00 NY in June (EDT, UTC-4) == 16:00 Europe/Paris (CEST, UTC+2).
    sqlx::query("INSERT INTO bookings (id, event_type_id, uid, guest_name, guest_email, guest_timezone, start_at, end_at, status, cancel_token, reschedule_token) VALUES (?, ?, 'uid-gcancel-tz', 'Guest', 'guest@test.com', 'Europe/Paris', '2026-06-22T10:00:00', '2026-06-22T10:30:00', 'confirmed', ?, ?)")
        .bind(&booking_id)
        .bind(&et_id)
        .bind(&cancel_tok)
        .bind(&resched_tok)
        .execute(&pool)
        .await
        .unwrap();

    let csrf = "test-csrf-gcancel-tz";
    let response = app
        .oneshot(post_form_unauthed(
            &format!("/booking/cancel/{}", cancel_tok),
            csrf,
            &format!("_csrf={}", csrf),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = body_string(response).await;

    assert!(
        body.contains("16:00") && body.contains("16:30"),
        "Cancellation page should render times in the guest's tz (Europe/Paris: 16:00 – 16:30)"
    );
    assert!(
        !body.contains("10:00") && !body.contains("10:30"),
        "Cancellation page should not render the event-type-local time (America/New_York: 10:00)"
    );
}

// --- Token-based POST actions ---

#[tokio::test]
async fn decline_booking_via_post() {
    let (app, pool, _, et_id) = setup_test_app().await;

    let booking_id = uuid::Uuid::new_v4().to_string();
    let cancel_tok = uuid::Uuid::new_v4().to_string();
    let resched_tok = uuid::Uuid::new_v4().to_string();
    let confirm_tok = uuid::Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO bookings (id, event_type_id, uid, guest_name, guest_email, guest_timezone, start_at, end_at, status, cancel_token, reschedule_token, confirm_token) VALUES (?, ?, 'uid-dec-post', 'Guest', 'guest@test.com', 'UTC', '2026-07-01T10:00:00', '2026-07-01T10:30:00', 'pending', ?, ?, ?)")
        .bind(&booking_id)
        .bind(&et_id)
        .bind(&cancel_tok)
        .bind(&resched_tok)
        .bind(&confirm_tok)
        .execute(&pool)
        .await
        .unwrap();

    let csrf = "test-csrf-decline-post";
    let response = app
        .oneshot(post_form_unauthed(
            &format!("/booking/decline/{}", confirm_tok),
            csrf,
            &format!("_csrf={}&reason=Not+available", csrf),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);

    let status: String = sqlx::query_scalar("SELECT status FROM bookings WHERE id = ?")
        .bind(&booking_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(status, "declined", "Booking should be declined");
}

#[tokio::test]
async fn guest_cancel_booking_via_post() {
    let (app, pool, _, et_id) = setup_test_app().await;

    let booking_id = uuid::Uuid::new_v4().to_string();
    let cancel_tok = uuid::Uuid::new_v4().to_string();
    let resched_tok = uuid::Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO bookings (id, event_type_id, uid, guest_name, guest_email, guest_timezone, start_at, end_at, status, cancel_token, reschedule_token) VALUES (?, ?, 'uid-gcancel-post', 'Guest', 'guest@test.com', 'UTC', '2026-07-02T10:00:00', '2026-07-02T10:30:00', 'confirmed', ?, ?)")
        .bind(&booking_id)
        .bind(&et_id)
        .bind(&cancel_tok)
        .bind(&resched_tok)
        .execute(&pool)
        .await
        .unwrap();

    let csrf = "test-csrf-gcancel-post";
    let response = app
        .oneshot(post_form_unauthed(
            &format!("/booking/cancel/{}", cancel_tok),
            csrf,
            &format!("_csrf={}&reason=Changed+plans", csrf),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);

    let status: String = sqlx::query_scalar("SELECT status FROM bookings WHERE id = ?")
        .bind(&booking_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(status, "cancelled", "Booking should be cancelled by guest");
}

#[tokio::test]
async fn guest_cancel_no_notice_succeeds() {
    let (app, pool, _, et_id) = setup_test_app().await;
    // No cancel_notice_min set on event type. Booking starts in 30 min.
    let cancel_tok = uuid::Uuid::new_v4().to_string();
    let resched_tok = uuid::Uuid::new_v4().to_string();
    let booking_id = insert_booking_at(
        &pool,
        &et_id,
        &cancel_tok,
        &resched_tok,
        chrono::Duration::minutes(30),
    )
    .await;

    let csrf = "csrf-cancel-no-notice";
    let response = app
        .oneshot(post_form_unauthed(
            &format!("/booking/cancel/{}", cancel_tok),
            csrf,
            &format!("_csrf={}", csrf),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let status: String = sqlx::query_scalar("SELECT status FROM bookings WHERE id = ?")
        .bind(&booking_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        status, "cancelled",
        "Without a notice window the cancel must succeed"
    );
}

#[tokio::test]
async fn guest_cancel_within_notice_window_blocked() {
    let (app, pool, _, et_id) = setup_test_app().await;
    // Notice window of 60 min, booking starts in 30 min: blocked.
    sqlx::query("UPDATE event_types SET cancel_notice_min = 60 WHERE id = ?")
        .bind(&et_id)
        .execute(&pool)
        .await
        .unwrap();
    let cancel_tok = uuid::Uuid::new_v4().to_string();
    let resched_tok = uuid::Uuid::new_v4().to_string();
    let booking_id = insert_booking_at(
        &pool,
        &et_id,
        &cancel_tok,
        &resched_tok,
        chrono::Duration::minutes(30),
    )
    .await;

    // GET should already block.
    let response = app
        .clone()
        .oneshot(get(&format!("/booking/cancel/{}", cancel_tok)))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = body_string(response).await;
    assert!(
        body.contains("notice") || body.contains("60") || body.contains("minutes"),
        "Blocked page should mention the notice (got body length {})",
        body.len()
    );

    // POST must also be blocked, and the booking must not transition.
    let csrf = "csrf-cancel-blocked";
    let response = app
        .oneshot(post_form_unauthed(
            &format!("/booking/cancel/{}", cancel_tok),
            csrf,
            &format!("_csrf={}", csrf),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let status: String = sqlx::query_scalar("SELECT status FROM bookings WHERE id = ?")
        .bind(&booking_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(status, "confirmed", "Blocked POST must not mutate state");
}

#[tokio::test]
async fn guest_cancel_outside_notice_window_succeeds() {
    let (app, pool, _, et_id) = setup_test_app().await;
    // Notice window of 60 min, booking starts in 120 min: allowed.
    sqlx::query("UPDATE event_types SET cancel_notice_min = 60 WHERE id = ?")
        .bind(&et_id)
        .execute(&pool)
        .await
        .unwrap();
    let cancel_tok = uuid::Uuid::new_v4().to_string();
    let resched_tok = uuid::Uuid::new_v4().to_string();
    let booking_id = insert_booking_at(
        &pool,
        &et_id,
        &cancel_tok,
        &resched_tok,
        chrono::Duration::minutes(120),
    )
    .await;

    let csrf = "csrf-cancel-allowed";
    let response = app
        .oneshot(post_form_unauthed(
            &format!("/booking/cancel/{}", cancel_tok),
            csrf,
            &format!("_csrf={}", csrf),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let status: String = sqlx::query_scalar("SELECT status FROM bookings WHERE id = ?")
        .bind(&booking_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(status, "cancelled");
}

#[tokio::test]
async fn guest_reschedule_no_notice_succeeds() {
    let (app, pool, _, et_id) = setup_test_app().await;
    let cancel_tok = uuid::Uuid::new_v4().to_string();
    let resched_tok = uuid::Uuid::new_v4().to_string();
    let _ = insert_booking_at(
        &pool,
        &et_id,
        &cancel_tok,
        &resched_tok,
        chrono::Duration::minutes(30),
    )
    .await;

    // GET the slot picker. With no notice, it should render the slot
    // page (status 200 and contains the event title or slot markers).
    let response = app
        .oneshot(get(&format!("/booking/reschedule/{}", resched_tok)))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = body_string(response).await;
    assert!(
        body.contains("Test Meeting") || body.contains("reschedul"),
        "No notice -> slot page should render, body length {}",
        body.len()
    );
}

#[tokio::test]
async fn guest_reschedule_within_notice_window_blocked() {
    let (app, pool, _, et_id) = setup_test_app().await;
    sqlx::query("UPDATE event_types SET reschedule_notice_min = 60 WHERE id = ?")
        .bind(&et_id)
        .execute(&pool)
        .await
        .unwrap();
    let cancel_tok = uuid::Uuid::new_v4().to_string();
    let resched_tok = uuid::Uuid::new_v4().to_string();
    let booking_id = insert_booking_at(
        &pool,
        &et_id,
        &cancel_tok,
        &resched_tok,
        chrono::Duration::minutes(30),
    )
    .await;

    // GET blocks before the slot picker is rendered.
    let response = app
        .clone()
        .oneshot(get(&format!("/booking/reschedule/{}", resched_tok)))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = body_string(response).await;
    assert!(
        body.contains("notice") || body.contains("60") || body.contains("minutes"),
        "Blocked page should mention the notice"
    );

    // POST also blocked: booking start_at must not change.
    let csrf = "csrf-resched-blocked";
    // Submit a new date 10 days out so we don't hit other guards.
    let new_date = (Local::now().naive_local().date() + chrono::Duration::days(10))
        .format("%Y-%m-%d")
        .to_string();
    let original_start: String = sqlx::query_scalar("SELECT start_at FROM bookings WHERE id = ?")
        .bind(&booking_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    let response = app
        .oneshot(post_form_unauthed(
            &format!("/booking/reschedule/{}", resched_tok),
            csrf,
            &format!("_csrf={}&date={}&time=10%3A00&tz=UTC", csrf, new_date),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let still: String = sqlx::query_scalar("SELECT start_at FROM bookings WHERE id = ?")
        .bind(&booking_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        still, original_start,
        "Blocked POST must not move the booking"
    );
}

#[tokio::test]
async fn guest_reschedule_outside_notice_window_succeeds() {
    let (app, pool, _, et_id) = setup_test_app().await;
    sqlx::query("UPDATE event_types SET reschedule_notice_min = 60 WHERE id = ?")
        .bind(&et_id)
        .execute(&pool)
        .await
        .unwrap();
    let cancel_tok = uuid::Uuid::new_v4().to_string();
    let resched_tok = uuid::Uuid::new_v4().to_string();
    let _ = insert_booking_at(
        &pool,
        &et_id,
        &cancel_tok,
        &resched_tok,
        chrono::Duration::minutes(120),
    )
    .await;

    // GET should render the slot picker (not the blocked page).
    let response = app
        .oneshot(get(&format!("/booking/reschedule/{}", resched_tok)))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = body_string(response).await;
    // Slot picker contains the event title; the blocked page does
    // not. Either presence of "Test Meeting" or absence of the
    // notice-line is fine.
    assert!(
        body.contains("Test Meeting") || body.contains("reschedul"),
        "Outside notice window the slot picker should render"
    );
}

#[tokio::test]
async fn host_cancel_unaffected_by_notice() {
    // The dashboard cancel handler must bypass the guest notice window.
    // Set a 24h cancel_notice and a booking 30 min out: the host should
    // still be able to cancel via /dashboard/bookings/{id}/cancel.
    let (app, pool, session, et_id) = setup_test_app().await;
    sqlx::query("UPDATE event_types SET cancel_notice_min = 1440 WHERE id = ?")
        .bind(&et_id)
        .execute(&pool)
        .await
        .unwrap();
    let cancel_tok = uuid::Uuid::new_v4().to_string();
    let resched_tok = uuid::Uuid::new_v4().to_string();
    let booking_id = insert_booking_at(
        &pool,
        &et_id,
        &cancel_tok,
        &resched_tok,
        chrono::Duration::minutes(30),
    )
    .await;

    let csrf = "csrf-host-cancel";
    let response = app
        .oneshot(post_form(
            &format!("/dashboard/bookings/{}/cancel", booking_id),
            &session,
            csrf,
            &format!("_csrf={}", csrf),
        ))
        .await
        .unwrap();
    assert!(
        response.status().is_redirection(),
        "Host cancel should redirect, got {}",
        response.status()
    );
    let status: String = sqlx::query_scalar("SELECT status FROM bookings WHERE id = ?")
        .bind(&booking_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        status, "cancelled",
        "Host cancel must succeed regardless of guest notice window"
    );
}

// --- Host reschedule ---

#[tokio::test]
async fn host_reschedule_page_returns_200() {
    let (app, pool, session, et_id) = setup_test_app().await;

    let booking_id = uuid::Uuid::new_v4().to_string();
    let cancel_tok = uuid::Uuid::new_v4().to_string();
    let resched_tok = uuid::Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO bookings (id, event_type_id, uid, guest_name, guest_email, guest_timezone, start_at, end_at, status, cancel_token, reschedule_token) VALUES (?, ?, 'uid-hresched', 'Guest', 'guest@test.com', 'UTC', '2026-07-10T10:00:00', '2026-07-10T10:30:00', 'confirmed', ?, ?)")
        .bind(&booking_id)
        .bind(&et_id)
        .bind(&cancel_tok)
        .bind(&resched_tok)
        .execute(&pool)
        .await
        .unwrap();

    let response = app
        .oneshot(get_authed(
            &format!("/dashboard/bookings/{}/reschedule", booking_id),
            &session,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = body_string(response).await;
    assert!(body.contains("Guest") || body.contains("reschedule") || body.contains("Reschedule"));
}

#[tokio::test]
async fn host_reschedule_post_sets_flag() {
    let (app, pool, session, et_id) = setup_test_app().await;

    let booking_id = uuid::Uuid::new_v4().to_string();
    let cancel_tok = uuid::Uuid::new_v4().to_string();
    let resched_tok = uuid::Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO bookings (id, event_type_id, uid, guest_name, guest_email, guest_timezone, start_at, end_at, status, cancel_token, reschedule_token) VALUES (?, ?, 'uid-hresched2', 'Guest', 'guest@test.com', 'UTC', '2026-07-11T10:00:00', '2026-07-11T10:30:00', 'confirmed', ?, ?)")
        .bind(&booking_id)
        .bind(&et_id)
        .bind(&cancel_tok)
        .bind(&resched_tok)
        .execute(&pool)
        .await
        .unwrap();

    let csrf = "test-csrf-hresched";
    let response = app
        .oneshot(post_form(
            &format!("/dashboard/bookings/{}/reschedule", booking_id),
            &session,
            csrf,
            &format!("_csrf={}", csrf),
        ))
        .await
        .unwrap();
    assert!(response.status().is_redirection());

    let flag: i32 = sqlx::query_scalar("SELECT reschedule_by_host FROM bookings WHERE id = ?")
        .bind(&booking_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(flag, 1, "reschedule_by_host should be set");
}

/// With the event type's date range over, the guest would land on a slot
/// page with nothing to pick: the host is told instead, and nothing changes.
#[tokio::test]
async fn host_reschedule_is_refused_when_nothing_is_bookable() {
    let (app, pool, session, et_id) = setup_test_app().await;
    sqlx::query("UPDATE event_types SET bookable_until = '2020-01-31' WHERE id = ?")
        .bind(&et_id)
        .execute(&pool)
        .await
        .unwrap();
    let booking_id = uuid::Uuid::new_v4().to_string();
    let resched_tok = uuid::Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO bookings (id, event_type_id, uid, guest_name, guest_email, guest_timezone, start_at, end_at, status, cancel_token, reschedule_token) VALUES (?, ?, 'uid-hresched3', 'Guest', 'guest@test.com', 'UTC', '2026-07-11T10:00:00', '2026-07-11T10:30:00', 'confirmed', ?, ?)")
        .bind(&booking_id)
        .bind(&et_id)
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(&resched_tok)
        .execute(&pool)
        .await
        .unwrap();

    let csrf = "test-csrf-hresched3";
    let response = app
        .clone()
        .oneshot(post_form(
            &format!("/dashboard/bookings/{}/reschedule", booking_id),
            &session,
            csrf,
            &format!("_csrf={}", csrf),
        ))
        .await
        .unwrap();
    assert_eq!(
        response.headers()["location"],
        "/dashboard/bookings?error=no-bookable-dates"
    );
    let (flag, token): (i32, String) =
        sqlx::query_as("SELECT reschedule_by_host, reschedule_token FROM bookings WHERE id = ?")
            .bind(&booking_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!((flag, token.as_str()), (0, resched_tok.as_str()));

    let page = body_string(
        app.oneshot(get_authed(
            "/dashboard/bookings?error=no-bookable-dates",
            &session,
        ))
        .await
        .unwrap(),
    )
    .await;
    assert!(page.contains("has no bookable dates left"));
}

// --- Guest reschedule flow ---

#[tokio::test]
async fn guest_reschedule_slots_page_returns_200() {
    let (app, pool, _, et_id) = setup_test_app().await;

    let booking_id = uuid::Uuid::new_v4().to_string();
    let cancel_tok = uuid::Uuid::new_v4().to_string();
    let resched_tok = uuid::Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO bookings (id, event_type_id, uid, guest_name, guest_email, guest_timezone, start_at, end_at, status, cancel_token, reschedule_token) VALUES (?, ?, 'uid-gresched', 'Guest', 'guest@test.com', 'UTC', '2026-08-01T10:00:00', '2026-08-01T10:30:00', 'confirmed', ?, ?)")
        .bind(&booking_id)
        .bind(&et_id)
        .bind(&cancel_tok)
        .bind(&resched_tok)
        .execute(&pool)
        .await
        .unwrap();

    let response = app
        .oneshot(get(&format!("/booking/reschedule/{}", resched_tok)))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = body_string(response).await;
    assert!(
        body.contains("Test Meeting") || body.contains("reschedul"),
        "Guest reschedule page should show event info"
    );
}

/// Round-robin team event types on the creator's account — exactly how the
/// dashboard creates them, which is why resolving the host through
/// `accounts.user_id` picked the wrong person (#166). Alice is the assigned
/// member of the booking to reschedule, which sits at 14:00 on the 15th of
/// next month, far enough out that the whole day is inside the slot window
/// whatever today is. She is already booked at 10:00 on a *second* team
/// event type, so the clash is only visible on her calendar: the owner is
/// free all day, and `idx_bookings_no_overlap` (keyed by event type) does
/// not fire either. Returns (assigned member id, reschedule token, day).
async fn seed_round_robin_reschedule(pool: &SqlitePool) -> (String, String, NaiveDate) {
    let (owner, account_id): (String, String) = sqlx::query_as(
        "SELECT u.id, a.id FROM users u JOIN accounts a ON a.user_id = u.id \
             WHERE u.username = 'testuser'",
    )
    .fetch_one(pool)
    .await
    .unwrap();
    let alice = insert_role_user(pool, "alice@resched166.test", "user").await;
    let alice_account = uuid::Uuid::new_v4().to_string();
    let alice_source = uuid::Uuid::new_v4().to_string();
    let alice_calendar = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO accounts (id, name, email, timezone, user_id) \
             VALUES (?, 'Alice', 'alice@resched166.test', 'UTC', ?)",
    )
    .bind(&alice_account)
    .bind(&alice)
    .execute(pool)
    .await
    .unwrap();
    // Keep the source fresh so the handler does not attempt network I/O.
    // The cached event below represents a prior successful write + sync.
    sqlx::query(
        "INSERT INTO caldav_sources (id, account_id, name, url, username, last_synced, \
             write_calendar_href) VALUES (?, ?, 'Alice calendar', \
             'https://calendar.test', 'alice', datetime('now'), '/alice/')",
    )
    .bind(&alice_source)
    .bind(&alice_account)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO calendars (id, source_id, href, display_name, is_busy) \
             VALUES (?, ?, '/alice/', 'Alice calendar', 1)",
    )
    .bind(&alice_calendar)
    .bind(&alice_source)
    .execute(pool)
    .await
    .unwrap();

    let team_id = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO teams (id, name, slug, visibility, created_by) \
             VALUES (?, 'RR Team', 'rr-team', 'public', ?)",
    )
    .bind(&team_id)
    .bind(&owner)
    .execute(pool)
    .await
    .unwrap();
    for member in [&owner, &alice] {
        sqlx::query(
            "INSERT INTO team_members (team_id, user_id, role, source) \
                 VALUES (?, ?, 'member', 'direct')",
        )
        .bind(&team_id)
        .bind(member)
        .execute(pool)
        .await
        .unwrap();
    }
    let et_id = uuid::Uuid::new_v4().to_string();
    let other_et_id = uuid::Uuid::new_v4().to_string();
    for (id, slug) in [(&et_id, "rr-meeting"), (&other_et_id, "rr-other")] {
        sqlx::query(
            "INSERT INTO event_types (id, account_id, slug, title, duration_min, buffer_before, \
                 buffer_after, min_notice_min, enabled, team_id, created_by_user_id, \
                 scheduling_mode, timezone) \
                 VALUES (?, ?, ?, 'RR Meeting', 30, 0, 0, 0, 1, ?, ?, 'round_robin', 'UTC')",
        )
        .bind(id)
        .bind(&account_id)
        .bind(slug)
        .bind(&team_id)
        .bind(&owner)
        .execute(pool)
        .await
        .unwrap();
    }
    // Open every day of the week so the assertions do not depend on which
    // weekday the 15th falls on.
    for day_of_week in 0..7 {
        sqlx::query(
            "INSERT INTO availability_rules (id, event_type_id, day_of_week, start_time, end_time) \
                 VALUES (?, ?, ?, '09:00', '17:00')",
        )
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(&et_id)
        .bind(day_of_week)
        .execute(pool)
        .await
        .unwrap();
    }

    let today = Utc::now().date_naive();
    let (year, month) = if today.month() == 12 {
        (today.year() + 1, 1)
    } else {
        (today.year(), today.month() + 1)
    };
    let target = NaiveDate::from_ymd_opt(year, month, 15).unwrap();

    // The booking being rescheduled has already been pushed to Alice's
    // write calendar and synced back into `events`. This copy must be
    // excluded by UID just like the `bookings` row is excluded by id.
    sqlx::query(
        "INSERT INTO events (id, calendar_id, uid, summary, start_at, end_at, status, transp) \
             VALUES (?, ?, 'uid-r166', 'RR Meeting', ?, ?, 'CONFIRMED', 'OPAQUE')",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(&alice_calendar)
    .bind(target.format("%Y%m%dT140000").to_string())
    .bind(target.format("%Y%m%dT143000").to_string())
    .execute(pool)
    .await
    .unwrap();

    insert_booking(
        pool,
        &other_et_id,
        Some(&alice),
        target.and_hms_opt(10, 0, 0).unwrap(),
        "confirmed",
    )
    .await;
    let resched_tok = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO bookings (id, event_type_id, uid, guest_name, guest_email, \
             guest_timezone, start_at, end_at, status, cancel_token, reschedule_token, \
             assigned_user_id) \
             VALUES (?, ?, 'uid-r166', 'Guest', 'guest@test.com', 'UTC', ?, ?, 'confirmed', ?, ?, ?)",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(&et_id)
    .bind(target.format("%Y-%m-%dT14:00:00").to_string())
    .bind(target.format("%Y-%m-%dT14:30:00").to_string())
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(&resched_tok)
    .bind(&alice)
    .execute(pool)
    .await
    .unwrap();

    (alice, resched_tok, target)
}

/// #265: the reschedule slot picker offers every zone too.
#[tokio::test]
async fn guest_reschedule_picker_offers_every_timezone() {
    let (app, pool, _, _) = setup_test_app().await;
    let (_alice, resched_tok, _) = seed_round_robin_reschedule(&pool).await;
    let response = app
        .oneshot(get(&format!(
            "/booking/reschedule/{}?tz=Europe%2FBucharest",
            resched_tok
        )))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = body_string(response).await;
    assert_eq!(selected_options(&body, "tz-select"), ["Europe/Bucharest"]);
    assert!(body.contains(r#"<optgroup label="Pacific Ocean">"#));
    assert!(body.contains(r#"id="slot-offset-note" hidden"#));
}

/// End-to-end guard for #166: the slot grid a guest is offered when
/// rescheduling a round-robin team booking must come from the assigned
/// member's calendar. Before the fix it came from the event type owner's,
/// so the guest was offered a slot the assigned member was already booked
/// for — and the page named the owner as their host.
#[tokio::test]
async fn guest_reschedule_slot_grid_follows_the_assigned_member() {
    let (app, pool, _, _) = setup_test_app().await;
    let (_alice, resched_tok, target) = seed_round_robin_reschedule(&pool).await;
    let day = target.format("%Y-%m-%d").to_string();

    let response = app
        .oneshot(get(&format!(
            "/booking/reschedule/{}?tz=UTC&month={}",
            resched_tok,
            target.format("%Y-%m")
        )))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = body_string(response).await;

    assert!(
        body.contains(&format!(r#""hostDate":"{}","hostTime":"11:00""#, day)),
        "the grid must be populated for {}",
        day
    );
    assert!(
        !body.contains(&format!(r#""hostDate":"{}","hostTime":"10:00""#, day)),
        "10:00 is taken on the assigned member's calendar and must not be offered"
    );
    assert!(
        body.contains(&format!(r#""hostDate":"{}","hostTime":"14:00""#, day)),
        "the slot the booking currently occupies must stay offered"
    );
    assert!(
        body.contains("alice@resched166.test"),
        "the page must show the assigned member as the host"
    );
}

/// The submit-time re-check has to reject a slot the assigned member is
/// busy for, even though the event type owner is free then (#166).
#[tokio::test]
async fn guest_reschedule_post_rejects_slot_busy_for_the_assigned_member() {
    let (app, pool, _, _) = setup_test_app().await;
    let (_alice, resched_tok, target) = seed_round_robin_reschedule(&pool).await;

    let csrf = "test-csrf-r166";
    let body = format!(
        "_csrf={}&date={}&time=10%3A00&tz=UTC",
        csrf,
        target.format("%Y-%m-%d")
    );
    let response = app
        .oneshot(post_form_unauthed(
            &format!("/booking/reschedule/{}", resched_tok),
            csrf,
            &body,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    assert!(
        body_string(response).await.contains("no longer available"),
        "10:00 is taken on the assigned member's calendar"
    );

    let start_at: String =
        sqlx::query_scalar("SELECT start_at FROM bookings WHERE reschedule_token = ?")
            .bind(&resched_tok)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        start_at,
        target.format("%Y-%m-%dT14:00:00").to_string(),
        "the rejected reschedule must leave the booking where it was"
    );
}

// --- Guests can add the booking to their calendar from the page ---

#[tokio::test]
async fn booking_ics_is_served_for_a_valid_cancel_token() {
    let (app, pool, _, et_id) = setup_test_app().await;
    let booking_id = uuid::Uuid::new_v4().to_string();
    let cancel_tok = uuid::Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO bookings (id, event_type_id, uid, guest_name, guest_email, guest_timezone, start_at, end_at, status, cancel_token, reschedule_token) VALUES (?, ?, 'uid-ics', 'ICS Guest', 'ics@test.com', 'UTC', '2030-06-15T10:00:00', '2030-06-15T10:30:00', 'confirmed', ?, 'resched-ics')")
        .bind(&booking_id)
        .bind(&et_id)
        .bind(&cancel_tok)
        .execute(&pool)
        .await
        .unwrap();

    let response = app
        .clone()
        .oneshot(get(&format!("/booking/ics/{}", cancel_tok)))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    assert!(response
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .starts_with("text/calendar"));
    let body = body_string(response).await;
    assert!(body.contains("BEGIN:VCALENDAR"));
    assert!(body.contains("UID:uid-ics"));

    let response = app.oneshot(get("/booking/ics/not-a-token")).await.unwrap();
    assert_eq!(response.status(), 404);
}

#[tokio::test]
async fn guest_reschedule_post_rejects_beyond_horizon() {
    let (app, pool, _, et_id) = setup_test_app().await;
    set_horizon(&pool, &et_id, 7).await;
    let cancel_tok = uuid::Uuid::new_v4().to_string();
    let resched_tok = uuid::Uuid::new_v4().to_string();
    let booking_id = insert_booking_at(
        &pool,
        &et_id,
        &cancel_tok,
        &resched_tok,
        chrono::Duration::hours(48),
    )
    .await;
    let before: String = sqlx::query_scalar("SELECT start_at FROM bookings WHERE id = ?")
        .bind(&booking_id)
        .fetch_one(&pool)
        .await
        .unwrap();

    let csrf = "csrf-horizon-resched";
    let body = format!(
        "_csrf={}&date={}&time=10:00&tz=UTC",
        csrf,
        far_future_monday()
    );
    let response = app
        .oneshot(post_form_unauthed(
            &format!("/booking/reschedule/{}", resched_tok),
            csrf,
            &body,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let after: String = sqlx::query_scalar("SELECT start_at FROM bookings WHERE id = ?")
        .bind(&booking_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        before, after,
        "a beyond-horizon reschedule must not move the booking"
    );
}

/// A reschedule POST has to name a slot the reschedule page offers: a day
/// blocked by an override, or a time outside its custom hours, is refused;
/// a time inside them moves the booking.
#[tokio::test]
async fn guest_reschedule_post_honours_availability_overrides() {
    let (app, pool, _, et_id) = setup_test_app().await;
    let monday = NaiveDate::parse_from_str(&far_future_monday(), "%Y-%m-%d").unwrap();
    let start = Utc.from_utc_datetime(&monday.and_hms_opt(14, 0, 0).unwrap());
    let id = insert_v1_booking(&pool, &et_id, "g@test.com", start, 30, "confirmed", &[]).await;
    let token: String = sqlx::query_scalar("SELECT reschedule_token FROM bookings WHERE id = ?")
        .bind(&id)
        .fetch_one(&pool)
        .await
        .unwrap();
    let tuesday = (monday + Duration::days(1)).format("%Y-%m-%d").to_string();
    let start_at = || async {
        sqlx::query_scalar::<_, String>("SELECT start_at FROM bookings WHERE id = ?")
            .bind(&id)
            .fetch_one(&pool)
            .await
            .unwrap()
    };
    let original = start_at().await;
    let reschedule = |time: &'static str| {
        let app = app.clone();
        let uri = format!("/booking/reschedule/{token}");
        let body = format!("_csrf=csrf-resched-ovr&date={tuesday}&time={time}&tz=UTC");
        async move {
            app.oneshot(post_form_unauthed(&uri, "csrf-resched-ovr", &body))
                .await
                .unwrap()
        }
    };

    sqlx::query("INSERT INTO availability_overrides (id, event_type_id, date, is_blocked) VALUES (?, ?, ?, 1)")
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(&et_id)
        .bind(&tuesday)
        .execute(&pool)
        .await
        .unwrap();
    let page = body_string(reschedule("10%3A00").await).await;
    assert!(page.contains("no longer available"));
    assert_eq!(
        start_at().await,
        original,
        "blocked day must not take the booking"
    );

    sqlx::query("UPDATE availability_overrides SET is_blocked = 0, start_time = '13:00', end_time = '15:00' WHERE event_type_id = ?")
        .bind(&et_id)
        .execute(&pool)
        .await
        .unwrap();
    reschedule("10%3A00").await;
    assert_eq!(
        start_at().await,
        original,
        "10:00 is outside the override's custom hours"
    );

    reschedule("13%3A00").await;
    assert_eq!(
        start_at().await,
        format!("{tuesday}T13:00:00Z"),
        "13:00 is inside the override's custom hours"
    );
}

/// #267: the guest cancel reads the booking, then cancels it. When an
/// approval or a reschedule lands in between, whether the cancellation ends
/// a reschedule awaiting approval must follow the status the booking
/// actually left, not the one read first.
#[tokio::test]
async fn guest_cancel_reports_the_status_it_actually_left() {
    let (_app, pool, _session, et_id) = setup_test_app().await;
    let start = Utc::now() + Duration::days(20);
    // Read as pending, approved since.
    let approved =
        insert_v1_booking(&pool, &et_id, "a@test.com", start, 30, "confirmed", &[]).await;
    assert_eq!(
        cancel_from_status(&pool, &approved, "pending")
            .await
            .as_deref(),
        Some("confirmed")
    );
    assert_eq!(booking_status(&pool, &approved).await, "cancelled");
    // Read as confirmed, sent back to pending by a reschedule since.
    let moved = insert_v1_booking(
        &pool,
        &et_id,
        "m@test.com",
        start + Duration::hours(2),
        30,
        "pending",
        &[],
    )
    .await;
    assert_eq!(
        cancel_from_status(&pool, &moved, "confirmed")
            .await
            .as_deref(),
        Some("pending")
    );
    // Already over: nothing to cancel, nothing changed.
    assert_eq!(
        cancel_from_status(&pool, &approved, "confirmed").await,
        None
    );
    sqlx::query("UPDATE bookings SET status = 'declined' WHERE id = ?")
        .bind(&moved)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(cancel_from_status(&pool, &moved, "pending").await, None);
    assert_eq!(booking_status(&pool, &moved).await, "declined");
}
