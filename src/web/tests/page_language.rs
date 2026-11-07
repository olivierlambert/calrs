//! Which language public, token and dashboard pages render in.

use super::*;

/// Pages declare the language they are rendered in, so browsers pick the
/// right spell-check dictionary (#209).
#[tokio::test]
async fn pages_declare_their_language_on_the_html_element() {
    let (app, _, _, _) = setup_test_app().await;
    let french = axum::http::Request::builder()
        .uri("/auth/login")
        .header("accept-language", "fr-FR,fr;q=0.9")
        .body(Body::empty())
        .unwrap();
    let body = body_string(app.clone().oneshot(french).await.unwrap()).await;
    assert!(body.contains("<html lang=\"fr\">"), "{body}");

    let body = body_string(app.oneshot(get("/auth/login")).await.unwrap()).await;
    assert!(body.contains("<html lang=\"en\">"), "{body}");
}

// --- The stored booking language is the language the form was shown in ---

fn book_request(
    uri: &str,
    csrf: &str,
    cookie_extra: &str,
    accept: &str,
) -> axum::http::Request<Body> {
    axum::http::Request::builder()
        .method("POST")
        .uri(uri)
        .header(
            "cookie",
            format!("__Host-calrs_csrf={}{}", csrf, cookie_extra),
        )
        .header("accept-language", accept)
        .header("content-type", "application/x-www-form-urlencoded")
        .body(Body::from(book_body(csrf)))
        .unwrap()
}

async fn stored_language(pool: &SqlitePool, guest_email: &str) -> Option<String> {
    sqlx::query_scalar("SELECT language FROM bookings WHERE guest_email = ?")
        .bind(guest_email)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn set_public_page_language(pool: &SqlitePool, mode: &str) {
    sqlx::query("UPDATE users SET public_page_language = ? WHERE email = 'test@example.com'")
        .bind(mode)
        .execute(pool)
        .await
        .unwrap();
}

/// With "always my language", a user's booking page is shown in the
/// host's saved language (#76), so that is what the booking stores and
/// what the guest is emailed in, whatever the guest's browser says.
#[tokio::test]
async fn user_page_booking_stores_the_language_the_form_was_shown_in() {
    let _env = crate::test_support::ENV_LOCK.lock().await;
    let (app, pool, _, _) = setup_test_app().await;
    let (port, mails) = spawn_capturing_mta().await;
    use_capturing_mta(&pool, port).await;
    set_user_language(&pool, "test@example.com", "fr").await;
    set_public_page_language(&pool, "host").await;

    let form = app
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .uri(format!(
                    "/u/testuser/test-meeting/book?date={}&time=10:00",
                    far_future_monday()
                ))
                .header("accept-language", "en-US,en;q=0.9")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert!(body_string(form).await.contains("<html lang=\"fr\">"));

    let csrf = "csrf-page-lang-fr";
    let response = app
        .oneshot(book_request(
            "/u/testuser/test-meeting/book",
            csrf,
            "",
            "en-US,en;q=0.9",
        ))
        .await
        .unwrap();
    assert!(response.status() == 200 || response.status().is_redirection());

    assert_eq!(
        stored_language(&pool, "guest@test.com").await.as_deref(),
        Some("fr")
    );
    let guest = mail_to(&mails, "guest@test.com").await;
    assert!(guest.contains("Bonjour Guest,"), "{guest}");
    assert!(guest.contains("<html lang=\"fr\">"), "{guest}");
    let host = mail_to(&mails, "test@example.com").await;
    assert!(host.contains("Nouvelle réservation !"), "{host}");
}

/// A browser language calrs does not ship falls back like the page does:
/// on the page of a host who chose "always my language", French.
#[tokio::test]
async fn unsupported_browser_language_stores_the_page_language() {
    let _env = crate::test_support::ENV_LOCK.lock().await;
    let (app, pool, _, _) = setup_test_app().await;
    set_user_language(&pool, "test@example.com", "fr").await;
    set_public_page_language(&pool, "host").await;

    let csrf = "csrf-page-lang-ja";
    app.oneshot(book_request(
        "/u/testuser/test-meeting/book",
        csrf,
        "",
        "ja-JP,ja;q=0.9",
    ))
    .await
    .unwrap();
    assert_eq!(
        stored_language(&pool, "guest@test.com").await.as_deref(),
        Some("fr")
    );
}

/// Public guest pages store the browser's language, for an anonymous
/// guest and for a logged-in host with a saved preference booking on a
/// client's behalf alike: staff never stamp their own language on a
/// guest's booking.
#[tokio::test]
async fn browser_language_page_stores_the_browser_language() {
    let _env = crate::test_support::ENV_LOCK.lock().await;
    let (app, pool, session, _) = setup_test_app().await;
    set_user_language(&pool, "test@example.com", "fr").await;

    let csrf = "csrf-page-lang-anon";
    app.clone()
        .oneshot(book_request(
            "/test-meeting/book",
            csrf,
            "",
            "en-US,en;q=0.9",
        ))
        .await
        .unwrap();
    assert_eq!(
        stored_language(&pool, "guest@test.com").await.as_deref(),
        Some("en")
    );
    sqlx::query("DELETE FROM bookings")
        .execute(&pool)
        .await
        .unwrap();

    let csrf = "csrf-page-lang-en";
    let response = app
        .oneshot(book_request(
            "/test-meeting/book",
            csrf,
            &format!("; __Host-calrs_session={}", session),
            "en-US,en;q=0.9",
        ))
        .await
        .unwrap();
    assert!(response.status() == 200 || response.status().is_redirection());
    assert_eq!(
        stored_language(&pool, "guest@test.com").await.as_deref(),
        Some("en")
    );
}

/// Team pages follow the browser even for a logged-in visitor with a
/// saved preference: a German-preference user with an English browser
/// sees them in English, and the booking stores 'en'.
#[tokio::test]
async fn team_pages_follow_the_browser_not_a_logged_in_preference() {
    let _env = crate::test_support::ENV_LOCK.lock().await;
    let (app, pool, _, _) = setup_test_app().await;
    seed_team_event_type(&pool).await;
    let visitor = uuid::Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO users (id, email, name, role, auth_provider, username, enabled, language) VALUES (?, 'visitor@test.com', 'Visitor', 'user', 'local', 'visitor', 1, 'de')")
        .bind(&visitor)
        .execute(&pool)
        .await
        .unwrap();
    let session = uuid::Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO sessions (id, user_id, expires_at) VALUES (?, ?, ?)")
        .bind(&session)
        .bind(&visitor)
        .bind(
            (Utc::now() + Duration::days(1))
                .format("%Y-%m-%d %H:%M:%S")
                .to_string(),
        )
        .execute(&pool)
        .await
        .unwrap();
    let cookie = format!("__Host-calrs_session={}", session);

    for uri in [
        "/team/horizon-team".to_string(),
        "/team/horizon-team/team-sync".to_string(),
        format!(
            "/team/horizon-team/team-sync/book?date={}&time=10:00",
            far_future_monday()
        ),
    ] {
        let request = axum::http::Request::builder()
            .uri(&uri)
            .header("cookie", &cookie)
            .header("accept-language", "en-US,en;q=0.9")
            .body(Body::empty())
            .unwrap();
        let body = body_string(app.clone().oneshot(request).await.unwrap()).await;
        assert!(body.contains("<html lang=\"en\">"), "{uri}: {body}");
    }

    let csrf = "csrf-team-visitor-de";
    let response = app
        .oneshot(book_request(
            "/team/horizon-team/team-sync/book",
            csrf,
            &format!("; {}", cookie),
            "en-US,en;q=0.9",
        ))
        .await
        .unwrap();
    assert!(response.status() == 200 || response.status().is_redirection());
    assert_eq!(
        stored_language(&pool, "guest@test.com").await.as_deref(),
        Some("en")
    );
}

/// On a host-language `/u/` page, a rejected booking answers fully in the
/// page's language, not the browser's.
#[tokio::test]
async fn host_mode_booking_error_page_uses_the_page_language() {
    let _env = crate::test_support::ENV_LOCK.lock().await;
    let (app, pool, _, _) = setup_test_app().await;
    set_user_language(&pool, "test@example.com", "fr").await;
    set_public_page_language(&pool, "host").await;

    let csrf = "csrf-host-mode-error";
    let body = format!(
        "_csrf={}&date={}&time=10:00&name=Guest&email=not-an-email&tz=UTC",
        csrf,
        far_future_monday()
    );
    let request = axum::http::Request::builder()
        .method("POST")
        .uri("/u/testuser/test-meeting/book")
        .header("cookie", format!("__Host-calrs_csrf={}", csrf))
        .header("accept-language", "en-US,en;q=0.9")
        .header("content-type", "application/x-www-form-urlencoded")
        .body(Body::from(body))
        .unwrap();
    let page = body_string(app.oneshot(request).await.unwrap()).await;
    assert!(page.contains("<html lang=\"fr\">"), "{page}");
    assert!(page.contains("Détails de réservation invalides"), "{page}");
    assert!(
        page.contains("Veuillez saisir une adresse e-mail valide."),
        "{page}"
    );
    assert!(!page.contains("Invalid booking details"), "{page}");
}

// --- Public booking page language setting ---

/// By default a user's pages follow the visitor: a French-speaking host's
/// pages are English for an English browser, the booking stores 'en',
/// and only the host's own email is French.
#[tokio::test]
async fn user_pages_follow_the_visitor_by_default() {
    let _env = crate::test_support::ENV_LOCK.lock().await;
    let (app, pool, _, _) = setup_test_app().await;
    let (port, mails) = spawn_capturing_mta().await;
    use_capturing_mta(&pool, port).await;
    set_user_language(&pool, "test@example.com", "fr").await;

    for uri in [
        "/u/testuser".to_string(),
        "/u/testuser/test-meeting".to_string(),
        format!(
            "/u/testuser/test-meeting/book?date={}&time=10:00",
            far_future_monday()
        ),
    ] {
        let body = body_string(
            app.clone()
                .oneshot(get_with_language(&uri, "en-US,en;q=0.9"))
                .await
                .unwrap(),
        )
        .await;
        assert!(body.contains("<html lang=\"en\">"), "{uri}: {body}");
    }

    let csrf = "csrf-public-visitor";
    let response = app
        .oneshot(book_request(
            "/u/testuser/test-meeting/book",
            csrf,
            "",
            "en-US,en;q=0.9",
        ))
        .await
        .unwrap();
    assert!(response.status() == 200 || response.status().is_redirection());
    assert_eq!(
        stored_language(&pool, "guest@test.com").await.as_deref(),
        Some("en")
    );
    let guest = mail_to(&mails, "guest@test.com").await;
    assert!(guest.contains("Hi Guest,"), "{guest}");
    let host = mail_to(&mails, "test@example.com").await;
    assert!(host.contains("Nouvelle réservation !"), "{host}");
}

/// "Always my language" with no saved language falls back to the
/// visitor's.
#[tokio::test]
async fn host_mode_without_a_saved_language_follows_the_visitor() {
    let _env = crate::test_support::ENV_LOCK.lock().await;
    let (app, pool, _, _) = setup_test_app().await;
    set_public_page_language(&pool, "host").await;

    let body = body_string(
        app.clone()
            .oneshot(get_with_language(
                "/u/testuser/test-meeting",
                "de-DE,de;q=0.9",
            ))
            .await
            .unwrap(),
    )
    .await;
    assert!(body.contains("<html lang=\"de\">"), "{body}");

    let csrf = "csrf-public-host-none";
    app.oneshot(book_request(
        "/u/testuser/test-meeting/book",
        csrf,
        "",
        "de-DE,de;q=0.9",
    ))
    .await
    .unwrap();
    assert_eq!(
        stored_language(&pool, "guest@test.com").await.as_deref(),
        Some("de")
    );
}

async fn stored_public_page_language(pool: &SqlitePool) -> String {
    sqlx::query_scalar("SELECT public_page_language FROM users WHERE email = 'test@example.com'")
        .fetch_one(pool)
        .await
        .unwrap()
}

#[tokio::test]
async fn settings_save_the_public_page_language_and_reject_unknown_values() {
    let _env = crate::test_support::ENV_LOCK.lock().await;
    let (app, pool, session, _) = setup_test_app().await;
    assert_eq!(stored_public_page_language(&pool).await, "visitor");

    let csrf = "csrf-settings-public-lang";
    let body = format!(
        "_csrf={}&name=Test+User&booking_email=&public_page_language=host",
        csrf
    );
    let response = app
        .clone()
        .oneshot(post_form("/dashboard/settings", &session, csrf, &body))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let page = body_string(response).await;
    assert!(page.contains("<option value=\"host\" selected>"), "{page}");
    assert_eq!(stored_public_page_language(&pool).await, "host");

    let body = format!(
        "_csrf={}&name=Test+User&booking_email=&public_page_language=guest",
        csrf
    );
    let response = app
        .oneshot(post_form("/dashboard/settings", &session, csrf, &body))
        .await
        .unwrap();
    let page = body_string(response).await;
    assert!(
        page.contains("Pick a valid public booking page language."),
        "{page}"
    );
    assert_eq!(stored_public_page_language(&pool).await, "host");
}

/// The host's approve page is in the host's saved language, like the
/// approval email that links to it, whatever the browser says.
#[tokio::test]
async fn approve_page_uses_the_host_language() {
    let _env = crate::test_support::ENV_LOCK.lock().await;
    let (app, pool, _, et_id) = setup_test_app().await;
    set_user_language(&pool, "test@example.com", "fr").await;
    insert_pending_french_booking(&pool, &et_id, "uid-host-page-lang").await;

    for uri in [
        "/booking/approve/confirm-uid-host-page-lang",
        "/booking/decline/confirm-uid-host-page-lang",
    ] {
        let page = body_string(
            app.clone()
                .oneshot(get_with_language(uri, "en-US,en;q=0.9"))
                .await
                .unwrap(),
        )
        .await;
        assert!(page.contains("<html lang=\"fr\">"), "{uri}: {page}");
    }
}

/// A collective booking has several approvers and the shared link cannot
/// tell which one clicked, so its approve page follows the browser, even
/// when the owner has a saved language.
#[tokio::test]
async fn collective_approve_page_follows_the_browser() {
    let _env = crate::test_support::ENV_LOCK.lock().await;
    let (app, pool, _, et_id) = setup_test_app().await;
    set_user_language(&pool, "test@example.com", "fr").await;
    let team_id = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO teams (id, name, slug, visibility) VALUES (?, 'Crew', 'crew', 'public')",
    )
    .bind(&team_id)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("UPDATE event_types SET team_id = ?, scheduling_mode = 'collective' WHERE id = ?")
        .bind(&team_id)
        .bind(&et_id)
        .execute(&pool)
        .await
        .unwrap();
    insert_pending_french_booking(&pool, &et_id, "uid-collective-page-lang").await;

    let page = body_string(
        app.oneshot(get_with_language(
            "/booking/approve/confirm-uid-collective-page-lang",
            "de-DE,de;q=0.9",
        ))
        .await
        .unwrap(),
    )
    .await;
    assert!(page.contains("<html lang=\"de\">"), "{page}");
}

/// The claim page is in the claiming watcher's saved language, like the
/// claim email.
#[tokio::test]
async fn claim_page_uses_the_watcher_language() {
    let _env = crate::test_support::ENV_LOCK.lock().await;
    let (app, pool, _, et_id) = setup_test_app().await;
    let booking_id = insert_pending_french_booking(&pool, &et_id, "uid-claim-page-lang").await;
    let watcher = uuid::Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO users (id, email, name, role, auth_provider, username, enabled, language) VALUES (?, 'watcher@test.com', 'Watcher', 'user', 'local', 'watcher', 1, 'de')")
        .bind(&watcher)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO booking_claim_tokens (id, booking_id, user_id, token, expires_at) VALUES (?, ?, ?, 'claim-tok-de', datetime('now', '+7 days'))")
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(&booking_id)
        .bind(&watcher)
        .execute(&pool)
        .await
        .unwrap();

    let page = body_string(
        app.oneshot(get_with_language(
            &format!("/booking/claim/{}?token=claim-tok-de", booking_id),
            "en-US,en;q=0.9",
        ))
        .await
        .unwrap(),
    )
    .await;
    assert!(page.contains("<html lang=\"de\">"), "{page}");
}

/// The legacy single-user pages honour the owner's public page language
/// exactly like `/u/`.
#[tokio::test]
async fn legacy_pages_honour_the_owner_page_language() {
    let _env = crate::test_support::ENV_LOCK.lock().await;
    let (app, pool, _, _) = setup_test_app().await;
    set_user_language(&pool, "test@example.com", "fr").await;
    set_public_page_language(&pool, "host").await;

    for uri in [
        "/test-meeting".to_string(),
        format!("/test-meeting/book?date={}&time=10:00", far_future_monday()),
    ] {
        let page = body_string(
            app.clone()
                .oneshot(get_with_language(&uri, "en-US,en;q=0.9"))
                .await
                .unwrap(),
        )
        .await;
        assert!(page.contains("<html lang=\"fr\">"), "{uri}: {page}");
    }

    let csrf = "csrf-legacy-host-mode";
    let response = app
        .oneshot(book_request(
            "/test-meeting/book",
            csrf,
            "",
            "en-US,en;q=0.9",
        ))
        .await
        .unwrap();
    assert!(response.status() == 200 || response.status().is_redirection());
    assert_eq!(
        stored_language(&pool, "guest@test.com").await.as_deref(),
        Some("fr")
    );
}

// --- Formal German ("Deutsch (Sie)") ---

/// A host who saved "Deutsch (Sie)" and chose "Always my language" shows
/// their pages to a German browser in the formal register; the booking
/// stores `de-formal`, so the guest's emails address them as "Sie" too, and
/// the host's own email follows their saved language.
#[tokio::test]
async fn formal_german_host_pages_and_emails_address_the_guest_as_sie() {
    let _env = crate::test_support::ENV_LOCK.lock().await;
    let (app, pool, _, _) = setup_test_app().await;
    let (port, mails) = spawn_capturing_mta().await;
    use_capturing_mta(&pool, port).await;
    set_user_language(&pool, "test@example.com", "de-formal").await;
    set_public_page_language(&pool, "host").await;

    let body = body_string(
        app.clone()
            .oneshot(get_with_language(
                "/u/testuser/test-meeting",
                "de-DE,de;q=0.9",
            ))
            .await
            .unwrap(),
    )
    .await;
    // Still plain German to the browser.
    assert!(body.contains("<html lang=\"de\">"), "{body}");
    assert!(body.contains("Ihre Zeitzone"), "{body}");
    assert!(!body.contains("Deine Zeitzone"), "{body}");

    let csrf = "csrf-formal-german";
    let response = app
        .oneshot(book_request(
            "/u/testuser/test-meeting/book",
            csrf,
            "",
            "de-DE,de;q=0.9",
        ))
        .await
        .unwrap();
    assert!(response.status() == 200 || response.status().is_redirection());
    assert_eq!(
        stored_language(&pool, "guest@test.com").await.as_deref(),
        Some("de-formal")
    );
    let guest = mail_to(&mails, "guest@test.com").await;
    assert!(guest.contains("Guten Tag Guest,"), "{guest}");
    assert!(guest.contains("Ihre Buchung wurde bestätigt!"), "{guest}");
    assert!(guest.contains("<html lang=\"de\">"), "{guest}");
    assert!(!guest.contains("Deine Buchung"), "{guest}");
    let host = mail_to(&mails, "test@example.com").await;
    assert!(
        host.contains("hat einen Termin bei Ihnen gebucht"),
        "{host}"
    );
}

/// Without "Always my language", a German browser keeps the informal
/// German: nothing about a request can ask for the formal register.
#[tokio::test]
async fn a_german_browser_alone_gets_informal_german() {
    let _env = crate::test_support::ENV_LOCK.lock().await;
    let (app, pool, _, _) = setup_test_app().await;
    set_user_language(&pool, "test@example.com", "de-formal").await;

    let body = body_string(
        app.oneshot(get_with_language(
            "/u/testuser/test-meeting",
            "de-DE,de;q=0.9",
        ))
        .await
        .unwrap(),
    )
    .await;
    assert!(body.contains("Deine Zeitzone"), "{body}");
}

/// The admin can make "Sie" the instance's German for visitors: a German
/// browser then gets `de-formal` on every public page, and the booking
/// stores it, so the guest's emails follow. Other browsers, and users with
/// a saved language, are untouched.
#[tokio::test]
async fn admin_setting_addresses_german_visitors_as_sie() {
    let _env = crate::test_support::ENV_LOCK.lock().await;
    let (app, pool, session, _) = setup_test_app().await;
    let (port, mails) = spawn_capturing_mta().await;
    use_capturing_mta(&pool, port).await;

    let csrf = "csrf-german-formal";
    let response = app
        .clone()
        .oneshot(post_form(
            "/dashboard/admin/general",
            &session,
            csrf,
            &format!("_csrf={csrf}&german_formal=sie"),
        ))
        .await
        .unwrap();
    assert!(response.status().is_redirection());
    let stored: i64 = sqlx::query_scalar("SELECT german_formal FROM auth_config")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(stored, 1);
    let admin = body_string(
        app.clone()
            .oneshot(get_authed("/dashboard/admin", &session))
            .await
            .unwrap(),
    )
    .await;
    assert!(
        admin.contains("<option value=\"sie\" selected>"),
        "the admin form shows the saved choice"
    );

    let page = |accept: &'static str| {
        let app = app.clone();
        async move {
            body_string(
                app.oneshot(get_with_language("/u/testuser/test-meeting", accept))
                    .await
                    .unwrap(),
            )
            .await
        }
    };
    let german = page("de-AT,de;q=0.9").await;
    assert!(german.contains("Ihre Zeitzone"), "{german}");
    assert!(german.contains("<html lang=\"de\">"), "{german}");
    let english = page("en-US,en;q=0.9").await;
    assert!(english.contains("Your timezone"), "{english}");

    app.clone()
        .oneshot(book_request(
            "/u/testuser/test-meeting/book",
            "csrf-german-formal-book",
            "",
            "de-DE,de;q=0.9",
        ))
        .await
        .unwrap();
    assert_eq!(
        stored_language(&pool, "guest@test.com").await.as_deref(),
        Some("de-formal")
    );
    let guest = mail_to(&mails, "guest@test.com").await;
    assert!(guest.contains("Ihre Buchung wurde bestätigt!"), "{guest}");

    // Back to "du": takes effect at once, no restart.
    app.clone()
        .oneshot(post_form(
            "/dashboard/admin/general",
            &session,
            csrf,
            &format!("_csrf={csrf}&german_formal=du"),
        ))
        .await
        .unwrap();
    assert!(page("de-DE").await.contains("Deine Zeitzone"));
}

/// The marker the instance setting travels in is internal: a client
/// sending it gets nothing from it when the setting is off.
#[tokio::test]
async fn a_client_cannot_ask_for_formal_german_through_the_internal_header() {
    let _env = crate::test_support::ENV_LOCK.lock().await;
    let (app, _, _, _) = setup_test_app().await;
    let request = axum::http::Request::builder()
        .uri("/u/testuser/test-meeting")
        .header("accept-language", "de-DE")
        .header(crate::i18n::GERMAN_FORMAL_HEADER, "1")
        .body(Body::empty())
        .unwrap();
    let body = body_string(app.oneshot(request).await.unwrap()).await;
    assert!(body.contains("Deine Zeitzone"), "{body}");
}
