//! Invite links and bulk invites.

use super::*;

// --- Private event type + invite flow ---

#[tokio::test]
async fn private_event_type_requires_invite_token() {
    let (app, pool, _, _) = setup_test_app().await;

    // Make the event type private
    sqlx::query("UPDATE event_types SET visibility = 'private' WHERE slug = 'test-meeting'")
        .execute(&pool)
        .await
        .unwrap();

    let now = Utc::now().with_timezone(&Tz::UTC).naive_local();
    let mut next_monday = now.date() + Duration::days(1);
    while next_monday.weekday() != chrono::Weekday::Mon {
        next_monday += Duration::days(1);
    }
    let date_str = next_monday.format("%Y-%m-%d").to_string();

    // Book without invite token
    let csrf = "test-csrf-private";
    let body = format!(
        "_csrf={}&date={}&time=10%3A00&name=Jane&email=jane%40test.com&notes=",
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
    let resp_body = body_string(response).await;
    assert!(
        resp_body.contains("invite") || resp_body.contains("Invite"),
        "Private event type should require invite"
    );

    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM bookings")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count.0, 0, "No booking should be created without invite");
}

#[tokio::test]
async fn invite_page_returns_200_for_private_event_type() {
    let (app, pool, session, et_id) = setup_test_app().await;

    sqlx::query("UPDATE event_types SET visibility = 'private' WHERE id = ?")
        .bind(&et_id)
        .execute(&pool)
        .await
        .unwrap();

    let response = app
        .oneshot(get_authed(
            &format!("/dashboard/invites/{}", et_id),
            &session,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = body_string(response).await;
    assert!(body.contains("Invites") || body.contains("invite"));
}

#[tokio::test]
async fn personalized_invite_identity_overrides_authenticated_profile() {
    let (app, pool, _, et_id) = setup_test_app().await;
    let (alice_id, session) =
        insert_test_user_with_session(&pool, "Alice Example", "alice@example.com", "alice").await;
    sqlx::query("UPDATE event_types SET visibility = 'private' WHERE id = ?")
        .bind(&et_id)
        .execute(&pool)
        .await
        .unwrap();
    let token = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO booking_invites (id, event_type_id, token, guest_name, guest_email, max_uses, created_by_user_id) \
             VALUES (?, ?, ?, 'Bob Guest', 'bob@example.net', 1, ?)",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(&et_id)
    .bind(&token)
    .bind(&alice_id)
    .execute(&pool)
    .await
    .unwrap();

    let response = app
        .oneshot(get_authed(&booking_form_uri(Some(&token)), &session))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = body_string(response).await;
    assert!(body.contains("value=\"Bob Guest\""));
    assert!(body.contains("value=\"bob@example.net\""));
    assert!(!body.contains("value=\"Alice Example\""));
    assert!(!body.contains("value=\"alice@example.com\""));
}

#[tokio::test]
async fn quick_invite_without_identity_uses_authenticated_profile() {
    let (app, pool, _, et_id) = setup_test_app().await;
    let (alice_id, session) =
        insert_test_user_with_session(&pool, "Alice Example", "alice@example.com", "alice").await;
    sqlx::query("UPDATE event_types SET visibility = 'internal' WHERE id = ?")
        .bind(&et_id)
        .execute(&pool)
        .await
        .unwrap();
    let token = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO booking_invites (id, event_type_id, token, guest_name, guest_email, max_uses, created_by_user_id) \
             VALUES (?, ?, ?, '', '', 1, ?)",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(&et_id)
    .bind(&token)
    .bind(&alice_id)
    .execute(&pool)
    .await
    .unwrap();

    let response = app
        .oneshot(get_authed(&booking_form_uri(Some(&token)), &session))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = body_string(response).await;
    assert!(body.contains("value=\"Alice Example\""));
    assert!(body.contains("value=\"alice@example.com\""));
}

// --- Bulk invite parsing tests ---

#[test]
fn bulk_invite_parses_valid_emails() {
    let (valid, result) = parse_bulk_recipients("alice@example.com\nbob@example.org\n", 100);
    assert_eq!(valid.len(), 2);
    assert_eq!(valid[0].0, "alice@example.com");
    assert_eq!(valid[0].1, "Alice");
    assert_eq!(valid[1].0, "bob@example.org");
    assert_eq!(valid[1].1, "Bob");
    assert!(result.invalid.is_empty());
    assert!(result.duplicates.is_empty());
    assert!(!result.over_limit);
}

#[test]
fn bulk_invite_skips_blank_lines_and_trims() {
    let (valid, _) = parse_bulk_recipients("\n  alice@example.com  \n\n", 100);
    assert_eq!(valid.len(), 1);
    assert_eq!(valid[0].0, "alice@example.com");
}

#[test]
fn bulk_invite_rejects_malformed_rows() {
    let (valid, result) = parse_bulk_recipients(
        "alice@example.com\nnot-an-email\n@nope.com\nfoo@\nfoo@bar\nok@x.io",
        100,
    );
    assert_eq!(valid.len(), 2);
    assert_eq!(result.invalid.len(), 4);
}

#[test]
fn bulk_invite_dedupes_case_insensitively() {
    let (valid, result) = parse_bulk_recipients("Alice@Example.com\nalice@example.com\n", 100);
    assert_eq!(valid.len(), 1);
    assert_eq!(result.duplicates, vec!["alice@example.com".to_string()]);
}

#[test]
fn bulk_invite_caps_at_max() {
    let mut input = String::new();
    for i in 0..10 {
        input.push_str(&format!("user{}@example.com\n", i));
    }
    let (valid, result) = parse_bulk_recipients(&input, 3);
    assert_eq!(valid.len(), 3);
    assert!(result.over_limit);
}

#[test]
fn bulk_invite_derives_pretty_names() {
    assert_eq!(derive_name_from_email("john.doe@example.com"), "John Doe");
    assert_eq!(
        derive_name_from_email("mary_smith@example.com"),
        "Mary Smith"
    );
    assert_eq!(derive_name_from_email("alice@example.com"), "Alice");
}

/// The booking form's "Back to times" link keeps the invite token, without
/// which a private event type's slot page refuses the guest (#275).
#[tokio::test]
async fn booking_form_back_link_keeps_the_invite() {
    let (app, pool, _, et_id) = setup_test_app().await;
    sqlx::query("UPDATE event_types SET visibility = 'private' WHERE id = ?")
        .bind(&et_id)
        .execute(&pool)
        .await
        .unwrap();
    let token = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO booking_invites (id, event_type_id, token, guest_name, guest_email, max_uses, created_by_user_id) \
             VALUES (?, ?, ?, '', '', 1, (SELECT id FROM users LIMIT 1))",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(&et_id)
    .bind(&token)
    .execute(&pool)
    .await
    .unwrap();

    let request = axum::http::Request::builder()
        .uri(booking_form_uri(Some(&token)))
        .body(Body::empty())
        .unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), 200);
    let body = body_string(response).await;
    let back = format!("test-meeting?invite={token}\" class=\"back-link\"");
    assert!(body.contains(&back), "back link lost the invite");

    let request = axum::http::Request::builder()
        .uri(format!("{}&embed=1", booking_form_uri(Some(&token))))
        .body(Body::empty())
        .unwrap();
    let body = body_string(app.oneshot(request).await.unwrap()).await;
    let back = format!("test-meeting?embed=1&amp;invite={token}\" class=\"back-link\"");
    assert!(body.contains(&back), "embedded back link lost the invite");
}

// --- Spending an invite's use (#373) ---

/// One guarded step: a use is spent only while one is left and the invite
/// has not expired, and a rolled-back booking gives it back.
#[tokio::test]
async fn consume_invite_spends_a_use_only_while_one_is_left() {
    let pool = setup_test_db().await;
    let (user_id, _, et_id) = seed_test_data(&pool).await;
    for (id, token, max_uses, expires_at) in [
        ("i-two", "two", 2, None),
        ("i-old", "old", 5, Some("2026-01-01 00:00:00")),
    ] {
        sqlx::query(
            "INSERT INTO booking_invites (id, event_type_id, token, guest_name, guest_email, max_uses, expires_at, created_by_user_id)
             VALUES (?, ?, ?, '', '', ?, ?, ?)",
        )
        .bind(id)
        .bind(&et_id)
        .bind(token)
        .bind(max_uses)
        .bind(expires_at)
        .bind(&user_id)
        .execute(&pool)
        .await
        .unwrap();
    }
    let now = "2026-06-01 12:00:00";
    let used = |id: &'static str| {
        let pool = pool.clone();
        async move {
            sqlx::query_scalar::<_, i64>("SELECT used_count FROM booking_invites WHERE id = ?")
                .bind(id)
                .fetch_one(&pool)
                .await
                .unwrap()
        }
    };

    // A rolled-back booking gives its use back.
    let mut tx = pool.begin().await.unwrap();
    assert!(consume_invite(&mut tx, "two", &et_id, now).await.unwrap());
    tx.rollback().await.unwrap();
    assert_eq!(used("i-two").await, 0);

    for expected in [true, true, false] {
        let mut tx = pool.begin().await.unwrap();
        assert_eq!(
            consume_invite(&mut tx, "two", &et_id, now).await.unwrap(),
            expected
        );
        tx.commit().await.unwrap();
    }
    assert_eq!(used("i-two").await, 2, "never past max_uses");

    let mut tx = pool.begin().await.unwrap();
    assert!(!consume_invite(&mut tx, "old", &et_id, now).await.unwrap());
    assert!(
        !consume_invite(&mut tx, "two", "another-event-type", now)
            .await
            .unwrap(),
        "an invite is spent on its own event type only"
    );
    tx.commit().await.unwrap();
    assert_eq!(used("i-old").await, 0, "an expired invite is not spent");
}
