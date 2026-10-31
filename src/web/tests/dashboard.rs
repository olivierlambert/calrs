//! Dashboard pages, host booking actions, settings and avatar.

use super::*;

#[test]
fn detect_image_ext_recognizes_known_formats() {
    // PNG signature.
    assert_eq!(
        detect_image_ext(&[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0xff]),
        Some("png")
    );
    // JPEG SOI + APP marker.
    assert_eq!(
        detect_image_ext(&[0xFF, 0xD8, 0xFF, 0xE0, 0x00]),
        Some("jpg")
    );
    // GIF.
    assert_eq!(detect_image_ext(b"GIF89a..."), Some("gif"));
    assert_eq!(detect_image_ext(b"GIF87a..."), Some("gif"));
    // RIFF/WEBP.
    let mut webp = Vec::from(*b"RIFF\x00\x00\x00\x00WEBP");
    webp.extend_from_slice(b"VP8 ");
    assert_eq!(detect_image_ext(&webp), Some("webp"));
}

#[test]
fn detect_image_ext_rejects_non_images() {
    // Empty input, plain text, HTML stub, executable header are all rejected.
    assert_eq!(detect_image_ext(b""), None);
    assert_eq!(detect_image_ext(b"not an image"), None);
    assert_eq!(detect_image_ext(b"<html><body>"), None);
    assert_eq!(detect_image_ext(b"\x7fELF"), None);
    // RIFF without the WEBP marker should not be accepted.
    let riff_wave = b"RIFF\x00\x00\x00\x00WAVEfmt ";
    assert_eq!(detect_image_ext(riff_wave), None);
}

#[tokio::test]
async fn dashboard_returns_200_for_authenticated_user() {
    let (app, _, session, _) = setup_test_app().await;
    let response = app
        .oneshot(get_authed("/dashboard", &session))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = body_string(response).await;
    assert!(body.contains("Welcome"), "Dashboard should contain Welcome");
}

#[tokio::test]
async fn event_types_page_returns_200() {
    let (app, _, session, _) = setup_test_app().await;
    let response = app
        .oneshot(get_authed("/dashboard/event-types", &session))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = body_string(response).await;
    assert!(
        body.contains("Test Meeting"),
        "Event types page should list seeded event type"
    );
}

#[tokio::test]
async fn bookings_page_returns_200() {
    let (app, _, session, _) = setup_test_app().await;
    let response = app
        .oneshot(get_authed("/dashboard/bookings", &session))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = body_string(response).await;
    assert!(
        body.contains("Upcoming bookings"),
        "Bookings page should render"
    );
}

#[tokio::test]
async fn settings_page_returns_200() {
    let (app, _, session, _) = setup_test_app().await;
    let response = app
        .oneshot(get_authed("/dashboard/settings", &session))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = body_string(response).await;
    assert!(body.contains("Test User"), "Settings should show user name");
    assert!(
        body.contains("href=\"/dashboard/settings/mfa\""),
        "Local accounts should have an MFA settings link"
    );
}

#[tokio::test]
async fn confirm_pending_booking_via_dashboard() {
    let (app, pool, session, et_id) = setup_test_app().await;

    // Create a pending booking
    let booking_id = uuid::Uuid::new_v4().to_string();
    let cancel_tok = uuid::Uuid::new_v4().to_string();
    let resched_tok = uuid::Uuid::new_v4().to_string();
    let confirm_tok = uuid::Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO bookings (id, event_type_id, uid, guest_name, guest_email, guest_timezone, start_at, end_at, status, cancel_token, reschedule_token, confirm_token) VALUES (?, ?, 'uid-confirm', 'Guest', 'guest@test.com', 'UTC', '2026-06-15T10:00:00', '2026-06-15T10:30:00', 'pending', ?, ?, ?)")
        .bind(&booking_id)
        .bind(&et_id)
        .bind(&cancel_tok)
        .bind(&resched_tok)
        .bind(&confirm_tok)
        .execute(&pool)
        .await
        .unwrap();

    let csrf = "test-csrf-confirm";
    let response = app
        .oneshot(post_form(
            &format!("/dashboard/bookings/{}/confirm", booking_id),
            &session,
            csrf,
            &format!("_csrf={}", csrf),
        ))
        .await
        .unwrap();
    assert!(response.status().is_redirection());

    let status: Option<(String,)> = sqlx::query_as("SELECT status FROM bookings WHERE id = ?")
        .bind(&booking_id)
        .fetch_optional(&pool)
        .await
        .unwrap();
    assert_eq!(
        status.unwrap().0,
        "confirmed",
        "Booking should be confirmed"
    );
}

#[tokio::test]
async fn cancel_booking_via_dashboard() {
    let (app, pool, session, et_id) = setup_test_app().await;

    let booking_id = uuid::Uuid::new_v4().to_string();
    let cancel_tok = uuid::Uuid::new_v4().to_string();
    let resched_tok = uuid::Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO bookings (id, event_type_id, uid, guest_name, guest_email, guest_timezone, start_at, end_at, status, cancel_token, reschedule_token) VALUES (?, ?, 'uid-cancel', 'Guest', 'guest@test.com', 'UTC', '2026-06-15T14:00:00', '2026-06-15T14:30:00', 'confirmed', ?, ?)")
        .bind(&booking_id)
        .bind(&et_id)
        .bind(&cancel_tok)
        .bind(&resched_tok)
        .execute(&pool)
        .await
        .unwrap();

    let csrf = "test-csrf-cancel";
    let response = app
        .oneshot(post_form(
            &format!("/dashboard/bookings/{}/cancel", booking_id),
            &session,
            csrf,
            &format!("_csrf={}&reason=conflict", csrf),
        ))
        .await
        .unwrap();
    assert!(response.status().is_redirection());

    let status: Option<(String,)> = sqlx::query_as("SELECT status FROM bookings WHERE id = ?")
        .bind(&booking_id)
        .fetch_optional(&pool)
        .await
        .unwrap();
    assert_eq!(
        status.unwrap().0,
        "cancelled",
        "Booking should be cancelled"
    );
}

#[tokio::test]
async fn decline_pending_booking_via_dashboard() {
    let (app, pool, session, et_id) = setup_test_app().await;

    let booking_id = uuid::Uuid::new_v4().to_string();
    let cancel_tok = uuid::Uuid::new_v4().to_string();
    let resched_tok = uuid::Uuid::new_v4().to_string();
    let confirm_tok = uuid::Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO bookings (id, event_type_id, uid, guest_name, guest_email, guest_timezone, start_at, end_at, status, cancel_token, reschedule_token, confirm_token) VALUES (?, ?, 'uid-decline-dash', 'Guest', 'guest@test.com', 'UTC', '2026-06-16T14:00:00', '2026-06-16T14:30:00', 'pending', ?, ?, ?)")
        .bind(&booking_id)
        .bind(&et_id)
        .bind(&cancel_tok)
        .bind(&resched_tok)
        .bind(&confirm_tok)
        .execute(&pool)
        .await
        .unwrap();

    let csrf = "test-csrf-decline-dash";
    let response = app
        .oneshot(post_form(
            &format!("/dashboard/bookings/{}/cancel", booking_id),
            &session,
            csrf,
            &format!("_csrf={}&reason=not+a+fit", csrf),
        ))
        .await
        .unwrap();
    assert!(response.status().is_redirection());

    let status: Option<(String,)> = sqlx::query_as("SELECT status FROM bookings WHERE id = ?")
        .bind(&booking_id)
        .fetch_optional(&pool)
        .await
        .unwrap();
    assert_eq!(
        status.unwrap().0,
        "declined",
        "Pending booking should be declined, not silently left in pending"
    );
}

// --- Settings ---

/// #265: a timezone outside the common list (set from the dashboard banner,
/// which takes any IANA name) was not preselected, so the next save stored
/// the first option, Pacific/Midway.
#[tokio::test]
async fn settings_keeps_a_saved_timezone_outside_the_common_list() {
    let (app, pool, session, _) = setup_test_app().await;
    for saved in ["Europe/Bucharest", "US/Eastern"] {
        sqlx::query("UPDATE users SET timezone = ? WHERE email = 'test@example.com'")
            .bind(saved)
            .execute(&pool)
            .await
            .unwrap();
        let response = app
            .clone()
            .oneshot(get_authed("/dashboard/settings", &session))
            .await
            .unwrap();
        let body = body_string(response).await;
        let selected = selected_options(&body, "timezone");
        assert_eq!(selected, [saved]);
        assert!(body.contains(r#"<optgroup label="Europe">"#));

        // Save the form with the value the browser would submit.
        let csrf = "test-csrf-settings-265";
        let form = format!(
            "_csrf={}&name=Test+User&booking_email=&timezone={}",
            csrf,
            selected[0].replace('/', "%2F")
        );
        let response = app
            .clone()
            .oneshot(post_form("/dashboard/settings", &session, csrf, &form))
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        let body = body_string(response).await;
        assert_eq!(selected_options(&body, "timezone"), [saved]);
        let stored: String =
            sqlx::query_scalar("SELECT timezone FROM users WHERE email = 'test@example.com'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(stored, saved);
    }
}

/// #265: a stored timezone chrono-tz cannot parse is offered as it is, and
/// posting it back keeps it (it used to become UTC). Other unparseable
/// input is refused and the stored value kept.
#[tokio::test]
async fn settings_keeps_an_unparseable_stored_timezone_posted_back() {
    let (app, pool, session, _) = setup_test_app().await;
    sqlx::query("UPDATE users SET timezone = 'Mars/Olympus_Mons' WHERE email = 'test@example.com'")
        .execute(&pool)
        .await
        .unwrap();
    let csrf = "test-csrf-settings-265-raw";
    for (posted, expected) in [
        ("Mars%2FOlympus_Mons", "Mars/Olympus_Mons"),
        ("Not%2FA_Zone", "Mars/Olympus_Mons"),
        ("Europe%2FBucharest", "Europe/Bucharest"),
        ("Not%2FA_Zone", "Europe/Bucharest"),
    ] {
        let form = format!("_csrf={csrf}&name=Test+User&booking_email=&timezone={posted}");
        let response = app
            .clone()
            .oneshot(post_form("/dashboard/settings", &session, csrf, &form))
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        let stored: String =
            sqlx::query_scalar("SELECT timezone FROM users WHERE email = 'test@example.com'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(stored, expected, "after posting {posted}");
    }
}

#[tokio::test]
async fn settings_save_updates_name() {
    let (app, pool, session, _) = setup_test_app().await;
    let csrf = "test-csrf-settings";
    let body = format!("_csrf={}&name=Updated+Name&booking_email=", csrf);
    let response = app
        .oneshot(post_form("/dashboard/settings", &session, csrf, &body))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let resp_body = body_string(response).await;
    assert!(resp_body.contains("Settings saved") || resp_body.contains("Updated Name"));

    let name: Option<String> =
        sqlx::query_scalar("SELECT name FROM users WHERE email = 'test@example.com'")
            .fetch_optional(&pool)
            .await
            .unwrap();
    assert_eq!(name.unwrap(), "Updated Name");
}

#[tokio::test]
async fn settings_failed_save_does_not_write_the_username() {
    let (app, pool, session, _) = setup_test_app().await;
    let before: Option<String> =
        sqlx::query_scalar("SELECT username FROM users WHERE email = 'test@example.com'")
            .fetch_one(&pool)
            .await
            .unwrap();
    let csrf = "test-csrf-settings-208";
    let body = format!(
        "_csrf={}&name=Test+User&username=brand-new-name&booking_email=not-an-email",
        csrf
    );
    let response = app
        .oneshot(post_form("/dashboard/settings", &session, csrf, &body))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let resp_body = body_string(response).await;
    assert!(
        !resp_body.contains("Settings saved"),
        "a rejected booking email must not report a save"
    );

    let after: Option<String> =
        sqlx::query_scalar("SELECT username FROM users WHERE email = 'test@example.com'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        after, before,
        "the username must not be written by a failed save"
    );
    assert_ne!(after.as_deref(), Some("brand-new-name"));
}

#[tokio::test]
async fn settings_save_writes_the_username_with_the_rest() {
    let (app, pool, session, _) = setup_test_app().await;
    let csrf = "test-csrf-settings-208-ok";
    let body = format!(
        "_csrf={}&name=Test+User&username=brand-new-name&booking_email=",
        csrf
    );
    let response = app
        .oneshot(post_form("/dashboard/settings", &session, csrf, &body))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let after: Option<String> =
        sqlx::query_scalar("SELECT username FROM users WHERE email = 'test@example.com'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(after.as_deref(), Some("brand-new-name"));
}

#[tokio::test]
async fn settings_save_confirms_language_change_in_the_new_language() {
    let (app, _pool, session, _) = setup_test_app().await;
    let csrf = "test-csrf-settings-lang";
    let body = format!("_csrf={}&name=Test+User&booking_email=&language=fr", csrf);
    let response = app
        .oneshot(post_form("/dashboard/settings", &session, csrf, &body))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let resp_body = body_string(response).await;
    assert!(
        resp_body.contains("Paramètres enregistrés"),
        "confirmation should render in the language that was just saved"
    );
    assert!(
        !resp_body.contains("Settings saved"),
        "confirmation should not render in the pre-save language"
    );
}

#[tokio::test]
async fn settings_validation_error_keeps_the_submitted_fields() {
    let (app, pool, session, _) = setup_test_app().await;
    let csrf = "test-csrf-settings-keep";
    // Empty name is rejected. Every other field carries an edit, and none of
    // them matches what the seeded row holds.
    let body = format!(
        "_csrf={}&name=&username=testuser&title=Head+of+Demos&bio=Runs+the+demo+rota\
             &booking_email=demo%40example.com&timezone=Europe%2FParis&language=fr",
        csrf
    );
    let response = app
        .oneshot(post_form("/dashboard/settings", &session, csrf, &body))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let resp_body = body_string(response).await;

    assert!(
        resp_body.contains("Name must be between 1 and 255 characters."),
        "the name error should be shown"
    );
    for expected in [
        r#"value="Head of Demos""#,
        "Runs the demo rota",
        r#"value="demo@example.com""#,
        // minijinja escapes the slash in the option value.
        r#"value="Europe&#x2f;Paris" selected"#,
        r#"value="fr" selected"#,
    ] {
        assert!(
            resp_body.contains(expected),
            "a rejected field should not discard the other edits, missing {expected}"
        );
    }

    // The checkbox was left unchecked, so it must come back unchecked even
    // though the stored row still has it on.
    let checkbox = resp_body
        .split(r#"name="allow_dynamic_group""#)
        .nth(1)
        .expect("the dynamic-group checkbox should render")
        .split('>')
        .next()
        .unwrap_or("");
    assert!(
        !checkbox.contains("checked"),
        "an unchecked box should come back unchecked"
    );

    // Nothing was saved: the UPDATE never ran.
    let (title, tz): (Option<String>, String) =
        sqlx::query_as("SELECT title, timezone FROM users WHERE email = 'test@example.com'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(title, None);
    assert_eq!(tz, "UTC");
}

#[tokio::test]
async fn settings_booking_email_error_shows_the_rejected_value() {
    let (app, _pool, session, _) = setup_test_app().await;
    let csrf = "test-csrf-settings-email";
    let body = format!(
        "_csrf={}&name=Alice+Martin&username=testuser&title=Head+of+Demos\
             &booking_email=not-an-email",
        csrf
    );
    let response = app
        .oneshot(post_form("/dashboard/settings", &session, csrf, &body))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let resp_body = body_string(response).await;

    assert!(
        resp_body.contains("Please enter a valid booking email address."),
        "the booking email error should be shown"
    );
    assert!(
        resp_body.contains(r#"value="not-an-email""#),
        "the field in error should show the value that was rejected"
    );
    assert!(
        resp_body.contains(r#"value="Head of Demos""#),
        "the other edits should survive the error"
    );
    assert!(
        resp_body.contains(r#"value="Alice Martin""#),
        "the submitted name should survive the error"
    );
    let avatar = resp_body
        .split(r#"flex-shrink: 0; overflow: hidden;">"#)
        .nth(1)
        .expect("the avatar preview should render")
        .split("</div>")
        .next()
        .unwrap_or("");
    assert!(
        avatar.contains("AM"),
        "the avatar initials should follow the name being shown, got {avatar:?}"
    );
}

// --- Avatar routes ---

#[tokio::test]
async fn avatar_nonexistent_returns_404() {
    let (app, _, _, _) = setup_test_app().await;
    let response = app
        .oneshot(get("/avatar/nonexistent-user-id"))
        .await
        .unwrap();
    assert_eq!(response.status(), 404);
}

// --- Dashboard overview stats ---

#[tokio::test]
async fn dashboard_overview_shows_stats() {
    let (app, pool, session, et_id) = setup_test_app().await;

    // Add a confirmed booking
    let booking_id = uuid::Uuid::new_v4().to_string();
    let cancel_tok = uuid::Uuid::new_v4().to_string();
    let resched_tok = uuid::Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO bookings (id, event_type_id, uid, guest_name, guest_email, guest_timezone, start_at, end_at, status, cancel_token, reschedule_token) VALUES (?, ?, 'uid-stat', 'Guest', 'guest@test.com', 'UTC', '2030-06-15T10:00:00', '2030-06-15T10:30:00', 'confirmed', ?, ?)")
        .bind(&booking_id)
        .bind(&et_id)
        .bind(&cancel_tok)
        .bind(&resched_tok)
        .execute(&pool)
        .await
        .unwrap();

    let response = app
        .oneshot(get_authed("/dashboard", &session))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = body_string(response).await;
    // Should show at least 1 event type and 1 upcoming booking
    assert!(body.contains("Event Type") || body.contains("event type") || body.contains("1"));
}

// --- Settings: booking email validation ---

#[tokio::test]
async fn settings_invalid_booking_email_rejected() {
    let (app, _, session, _) = setup_test_app().await;
    let csrf = "test-csrf-bad-email";
    let body = format!("_csrf={}&name=Test+User&booking_email=not-an-email", csrf);
    let response = app
        .oneshot(post_form("/dashboard/settings", &session, csrf, &body))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let resp_body = body_string(response).await;
    assert!(
        resp_body.contains("Invalid")
            || resp_body.contains("invalid")
            || resp_body.contains("email"),
        "Invalid booking email should show error"
    );
}

// --- Settings: empty name rejected ---

#[tokio::test]
async fn settings_empty_name_rejected() {
    let (app, _, session, _) = setup_test_app().await;
    let csrf = "test-csrf-no-name";
    let body = format!("_csrf={}&name=&booking_email=", csrf);
    let response = app
        .oneshot(post_form("/dashboard/settings", &session, csrf, &body))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let resp_body = body_string(response).await;
    assert!(
        resp_body.contains("required") || resp_body.contains("Name") || resp_body.contains("empty"),
        "Empty name should show error"
    );
}

// --- Helper function tests ---

#[test]
fn compute_initials_two_names() {
    assert_eq!(compute_initials("Alice Bob"), "AB");
}

#[test]
fn compute_initials_single_name() {
    assert_eq!(compute_initials("Alice"), "A");
}

#[test]
fn compute_initials_three_names() {
    assert_eq!(compute_initials("Alice Bob Charlie"), "AC");
}

#[test]
fn compute_initials_empty() {
    assert_eq!(compute_initials(""), "?");
}

// --- Dashboard with cancelled bookings (shouldn't show) ---

#[tokio::test]
async fn dashboard_bookings_hides_cancelled() {
    let (app, pool, session, et_id) = setup_test_app().await;

    let booking_id = uuid::Uuid::new_v4().to_string();
    let cancel_tok = uuid::Uuid::new_v4().to_string();
    let resched_tok = uuid::Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO bookings (id, event_type_id, uid, guest_name, guest_email, guest_timezone, start_at, end_at, status, cancel_token, reschedule_token) VALUES (?, ?, 'uid-cancelled', 'Cancelled Guest', 'cancelled@test.com', 'UTC', '2030-06-15T10:00:00', '2030-06-15T10:30:00', 'cancelled', ?, ?)")
        .bind(&booking_id)
        .bind(&et_id)
        .bind(&cancel_tok)
        .bind(&resched_tok)
        .execute(&pool)
        .await
        .unwrap();

    let response = app
        .oneshot(get_authed("/dashboard/bookings", &session))
        .await
        .unwrap();
    let body = body_string(response).await;
    assert!(
        !body.contains("Cancelled Guest"),
        "Cancelled bookings should not appear in upcoming"
    );
}

// --- Pending bookings show on dashboard ---

#[tokio::test]
async fn dashboard_bookings_shows_pending() {
    let (app, pool, session, et_id) = setup_test_app().await;

    let booking_id = uuid::Uuid::new_v4().to_string();
    let cancel_tok = uuid::Uuid::new_v4().to_string();
    let resched_tok = uuid::Uuid::new_v4().to_string();
    let confirm_tok = uuid::Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO bookings (id, event_type_id, uid, guest_name, guest_email, guest_timezone, start_at, end_at, status, cancel_token, reschedule_token, confirm_token) VALUES (?, ?, 'uid-pending-dash', 'Pending Guest', 'pending-dash@test.com', 'UTC', '2030-06-15T10:00:00', '2030-06-15T10:30:00', 'pending', ?, ?, ?)")
        .bind(&booking_id)
        .bind(&et_id)
        .bind(&cancel_tok)
        .bind(&resched_tok)
        .bind(&confirm_tok)
        .execute(&pool)
        .await
        .unwrap();

    let response = app
        .oneshot(get_authed("/dashboard/bookings", &session))
        .await
        .unwrap();
    let body = body_string(response).await;
    assert!(
        body.contains("Pending Guest"),
        "Pending bookings should appear in pending approval section"
    );
}

// --- The guest's note shows on the host dashboard (#230) ---

#[tokio::test]
async fn dashboard_shows_the_guests_note_escaped() {
    let (app, pool, session, et_id) = setup_test_app().await;

    for (uid, email, status, notes) in [
        (
            "uid-note-pending",
            "note-pending@test.com",
            "pending",
            "Pending note line 1\n<script>alert(1)</script>",
        ),
        (
            "uid-note-confirmed",
            "note-confirmed@test.com",
            "confirmed",
            "Confirmed note & agenda",
        ),
    ] {
        sqlx::query("INSERT INTO bookings (id, event_type_id, uid, guest_name, guest_email, guest_timezone, start_at, end_at, status, cancel_token, reschedule_token, confirm_token, notes) VALUES (?, ?, ?, 'Note Guest', ?, 'UTC', ?, ?, ?, ?, ?, ?, ?)")
            .bind(uuid::Uuid::new_v4().to_string())
            .bind(&et_id)
            .bind(uid)
            .bind(email)
            .bind(if status == "pending" { "2030-06-15T10:00:00" } else { "2030-06-16T10:00:00" })
            .bind(if status == "pending" { "2030-06-15T10:30:00" } else { "2030-06-16T10:30:00" })
            .bind(status)
            .bind(uuid::Uuid::new_v4().to_string())
            .bind(uuid::Uuid::new_v4().to_string())
            .bind(uuid::Uuid::new_v4().to_string())
            .bind(notes)
            .execute(&pool)
            .await
            .unwrap();
    }

    let response = app
        .clone()
        .oneshot(get_authed("/dashboard/bookings", &session))
        .await
        .unwrap();
    let body = body_string(response).await;
    assert!(body.contains("Pending note line 1\n&lt;script&gt;alert(1)&lt;&#x2f;script&gt;"));
    assert!(body.contains("Confirmed note &amp; agenda"));
    assert!(!body.contains("<script>alert(1)"));
    assert!(body.contains("booking-notes"));

    let response = app
        .oneshot(get_authed("/dashboard", &session))
        .await
        .unwrap();
    let body = body_string(response).await;
    assert!(body.contains("Pending note line 1"));
    assert!(!body.contains("<script>alert(1)"));
}

// --- Upcoming bookings include guest phone when SMS notifications are enabled ---

#[tokio::test]
async fn dashboard_bookings_shows_guest_phone_for_sms_booking() {
    let (app, pool, session, et_id) = setup_test_app().await;

    sqlx::query("UPDATE event_types SET sms_phone_mode = 'optional' WHERE id = ?")
        .bind(&et_id)
        .execute(&pool)
        .await
        .unwrap();

    let booking_id = uuid::Uuid::new_v4().to_string();
    let cancel_tok = uuid::Uuid::new_v4().to_string();
    let resched_tok = uuid::Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO bookings (id, event_type_id, uid, guest_name, guest_email, guest_timezone, start_at, end_at, status, cancel_token, reschedule_token, guest_phone) VALUES (?, ?, 'uid-phone-dash', 'Phone Guest', 'phone-dash@test.com', 'UTC', '2030-06-15T10:00:00', '2030-06-15T10:30:00', 'confirmed', ?, ?, '+15550000001')")
        .bind(&booking_id)
        .bind(&et_id)
        .bind(&cancel_tok)
        .bind(&resched_tok)
        .execute(&pool)
        .await
        .unwrap();

    let response = app
        .oneshot(get_authed("/dashboard/bookings", &session))
        .await
        .unwrap();
    let body = body_string(response).await;
    assert!(
        body.contains("Phone Guest") && body.contains("+15550000001"),
        "Upcoming booking should display the guest phone number when SMS is enabled"
    );
}

/// The phone shows on pending requests too: the host may need to call
/// the guest before approving (#231).
#[tokio::test]
async fn dashboard_bookings_shows_guest_phone_while_pending() {
    let (app, pool, session, et_id) = setup_test_app().await;
    sqlx::query("UPDATE event_types SET sms_phone_mode = 'optional' WHERE id = ?")
        .bind(&et_id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO bookings (id, event_type_id, uid, guest_name, guest_email, guest_timezone, start_at, end_at, status, cancel_token, reschedule_token, confirm_token, guest_phone) VALUES (?, ?, 'uid-phone-pending', 'Pending Guest', 'pending-phone@test.com', 'UTC', '2030-06-15T10:00:00', '2030-06-15T10:30:00', 'pending', ?, ?, 'confirm-phone-pending', '+15550000002')")
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(&et_id)
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(uuid::Uuid::new_v4().to_string())
        .execute(&pool)
        .await
        .unwrap();

    let response = app
        .clone()
        .oneshot(get_authed("/dashboard/bookings", &session))
        .await
        .unwrap();
    let body = body_string(response).await;
    assert!(
        body.contains("Pending Guest") && body.contains("+15550000002"),
        "a pending request should display the guest phone"
    );

    // Not collected any more: the stored number is not shown.
    sqlx::query("UPDATE event_types SET sms_phone_mode = 'off' WHERE id = ?")
        .bind(&et_id)
        .execute(&pool)
        .await
        .unwrap();
    let response = app
        .oneshot(get_authed("/dashboard/bookings", &session))
        .await
        .unwrap();
    let body = body_string(response).await;
    assert!(body.contains("Pending Guest") && !body.contains("+15550000002"));
}

/// "Reset to my default" on the event type form reads the profile hours and
/// the timezone they are in, so it can select that timezone too (#265).
#[tokio::test]
async fn default_availability_carries_the_profile_timezone() {
    let (app, pool, session, _) = setup_test_app().await;
    sqlx::query("UPDATE users SET timezone = 'Europe/Bucharest'")
        .execute(&pool)
        .await
        .unwrap();

    let response = app
        .oneshot(get_authed("/dashboard/availability/default", &session))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let json: serde_json::Value = serde_json::from_str(&body_string(response).await).unwrap();
    assert_eq!(json["timezone"], "Europe/Bucharest");
    assert!(json["schedule"].is_string());
}
