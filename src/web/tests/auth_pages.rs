//! Login, registration and session handling through the router.

use super::*;

#[tokio::test]
async fn root_redirects_to_login() {
    let (app, _, _, _) = setup_test_app().await;
    let response = app.oneshot(get("/")).await.unwrap();
    assert_eq!(response.status(), 303);
    assert_eq!(
        response
            .headers()
            .get("location")
            .unwrap()
            .to_str()
            .unwrap(),
        "/auth/login"
    );
}

#[tokio::test]
async fn login_page_returns_200() {
    let (app, _, _, _) = setup_test_app().await;
    let response = app.oneshot(get("/auth/login")).await.unwrap();
    assert_eq!(response.status(), 200);
    let body = body_string(response).await;
    assert!(
        body.contains("Sign in"),
        "Login page should contain Sign in"
    );
}

#[tokio::test]
async fn login_page_hides_register_when_disabled() {
    let (app, pool, _, _) = setup_test_app().await;

    // Disable registration
    sqlx::query("UPDATE auth_config SET registration_enabled = 0 WHERE id = 'singleton'")
        .execute(&pool)
        .await
        .unwrap();

    let response = app.oneshot(get("/auth/login")).await.unwrap();
    let body = body_string(response).await;
    assert!(
        !body.contains("Register"),
        "Register link should be hidden when registration is disabled"
    );
}

#[tokio::test]
async fn dashboard_redirects_unauthenticated() {
    let (app, _, _, _) = setup_test_app().await;
    let response = app.oneshot(get("/dashboard")).await.unwrap();
    assert_eq!(response.status(), 303);
}

#[tokio::test]
async fn login_with_wrong_password_shows_error() {
    let (app, pool, _, _) = setup_test_app().await;

    // Set a password on the test user
    let hash = crate::auth::hash_password("correct-password").unwrap();
    sqlx::query("UPDATE users SET password_hash = ? WHERE email = 'test@example.com'")
        .bind(&hash)
        .execute(&pool)
        .await
        .unwrap();

    let csrf = "test-csrf-login";
    let response = app
        .oneshot(post_form_unauthed(
            "/auth/login",
            csrf,
            &format!(
                "_csrf={}&email=test%40example.com&password=wrong-password",
                csrf
            ),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = body_string(response).await;
    assert!(
        body.contains("Invalid") || body.contains("invalid") || body.contains("error"),
        "Wrong password should show error"
    );
}

#[tokio::test]
async fn login_with_correct_password_redirects() {
    let (app, pool, _, _) = setup_test_app().await;

    let hash = crate::auth::hash_password("my-password").unwrap();
    sqlx::query("UPDATE users SET password_hash = ? WHERE email = 'test@example.com'")
        .bind(&hash)
        .execute(&pool)
        .await
        .unwrap();

    let csrf = "test-csrf-login-ok";
    let response = app
        .oneshot(post_form_unauthed(
            "/auth/login",
            csrf,
            &format!(
                "_csrf={}&email=test%40example.com&password=my-password",
                csrf
            ),
        ))
        .await
        .unwrap();
    assert!(
        response.status().is_redirection(),
        "Successful login should redirect, got {}",
        response.status()
    );
    let location = response
        .headers()
        .get("location")
        .unwrap()
        .to_str()
        .unwrap();
    assert!(
        location.contains("dashboard"),
        "Should redirect to dashboard, got {}",
        location
    );
}

#[tokio::test]
async fn register_when_disabled_returns_error() {
    let (app, pool, _, _) = setup_test_app().await;

    sqlx::query("UPDATE auth_config SET registration_enabled = 0 WHERE id = 'singleton'")
        .execute(&pool)
        .await
        .unwrap();

    let csrf = "test-csrf-reg";
    let response = app
        .oneshot(post_form_unauthed(
            "/auth/register",
            csrf,
            &format!(
                "_csrf={}&name=New+User&email=new%40example.com&password=pass1234",
                csrf
            ),
        ))
        .await
        .unwrap();
    let body = body_string(response).await;
    assert!(
        body.contains("disabled") || body.contains("Disabled") || body.contains("not available"),
        "Registration when disabled should show error"
    );

    let count: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM users WHERE email = 'new@example.com'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(count.0, 0, "User should not be created");
}

// --- Expired session ---

#[tokio::test]
async fn expired_session_redirects_to_login() {
    let (app, pool, _, _) = setup_test_app().await;

    let user_id: String = sqlx::query_scalar("SELECT id FROM users LIMIT 1")
        .fetch_one(&pool)
        .await
        .unwrap();
    let expired_session = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO sessions (id, user_id, expires_at) VALUES (?, ?, '2020-01-01 00:00:00')",
    )
    .bind(&expired_session)
    .bind(&user_id)
    .execute(&pool)
    .await
    .unwrap();

    let response = app
        .oneshot(get_authed("/dashboard", &expired_session))
        .await
        .unwrap();
    assert_eq!(
        response.status(),
        303,
        "Expired session should redirect to login"
    );
}

// --- Authenticated user visiting login redirects to dashboard ---

#[tokio::test]
async fn authenticated_user_login_page_redirects() {
    let (app, _, session, _) = setup_test_app().await;
    let response = app
        .oneshot(get_authed("/auth/login", &session))
        .await
        .unwrap();
    assert!(
        response.status().is_redirection(),
        "Authenticated user should be redirected from login"
    );
}

// --- Register page ---

#[tokio::test]
async fn register_page_returns_200() {
    let (app, _, _, _) = setup_test_app().await;
    let response = app.oneshot(get("/auth/register")).await.unwrap();
    assert_eq!(response.status(), 200);
    let body = body_string(response).await;
    assert!(body.contains("Register") || body.contains("register"));
}

#[tokio::test]
async fn register_creates_user() {
    let (app, pool, _, _) = setup_test_app().await;
    let csrf = "test-csrf-register-ok";
    let body = format!(
        "_csrf={}&name=New+User&email=newuser%40example.com&password=strongpassword123",
        csrf
    );
    let response = app
        .oneshot(post_form_unauthed("/auth/register", csrf, &body))
        .await
        .unwrap();
    assert!(
        response.status().is_redirection(),
        "Registration should redirect, got {}",
        response.status()
    );

    let count: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM users WHERE email = 'newuser@example.com'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(count.0, 1, "User should be created");
}

// --- Registration never takes over an account that has a user ---

/// POST a registration form for `email` (already URL-encoded).
async fn register(app: Router, email: &str) -> axum::response::Response {
    let csrf = "test-csrf-register-claim";
    let body = format!(
        "_csrf={}&name=Newcomer&email={}&password=strongpassword123",
        csrf, email
    );
    app.oneshot(post_form_unauthed("/auth/register", csrf, &body))
        .await
        .unwrap()
}

#[tokio::test]
async fn register_with_a_former_address_creates_a_separate_account() {
    let (app, pool, _, et_id) = setup_test_app().await;
    let (victim_id, victim_account): (String, String) = sqlx::query_as(
        "SELECT u.id, a.id FROM users u JOIN accounts a ON a.user_id = u.id \
         WHERE u.email = 'test@example.com'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    // The owner signs in with SSO and their address changed at the identity
    // provider: users.email follows, accounts.email keeps the old one.
    sqlx::query(
        "UPDATE users SET email = 'moved@example.com', auth_provider = 'oidc', \
         oidc_subject = 'victim-sub' WHERE id = ?",
    )
    .bind(&victim_id)
    .execute(&pool)
    .await
    .unwrap();
    let source_id = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO caldav_sources (id, account_id, name, url, username, password_enc) \
         VALUES (?, ?, 'Work', 'https://dav.example.com/', 'u', 'BLOB')",
    )
    .bind(&source_id)
    .bind(&victim_account)
    .execute(&pool)
    .await
    .unwrap();

    let response = register(app, "test%40example.com").await;
    assert!(
        response.status().is_redirection(),
        "registration should succeed, got {}",
        response.status()
    );

    let (new_user, new_account): (String, String) = sqlx::query_as(
        "SELECT u.id, a.id FROM users u JOIN accounts a ON a.user_id = u.id \
         WHERE u.email = 'test@example.com'",
    )
    .fetch_one(&pool)
    .await
    .expect("the newcomer gets an account of their own");
    assert_ne!(new_user, victim_id);
    assert_ne!(new_account, victim_account);

    let (owner, name): (Option<String>, String) =
        sqlx::query_as("SELECT user_id, name FROM accounts WHERE id = ?")
            .bind(&victim_account)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(owner.as_deref(), Some(victim_id.as_str()));
    assert_ne!(name, "Newcomer");
    let victim_email: String = sqlx::query_scalar("SELECT email FROM accounts WHERE id = ?")
        .bind(&victim_account)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        victim_email, "moved@example.com",
        "stale copy follows its owner"
    );
    let et_account: String = sqlx::query_scalar("SELECT account_id FROM event_types WHERE id = ?")
        .bind(&et_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(et_account, victim_account);
    let source_account: String =
        sqlx::query_scalar("SELECT account_id FROM caldav_sources WHERE id = ?")
            .bind(&source_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(source_account, victim_account);
    let newcomer_event_types: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM event_types WHERE account_id = ?")
            .bind(&new_account)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(newcomer_event_types, 0);
}

#[tokio::test]
async fn register_never_hands_over_an_unowned_account() {
    let (app, pool, _, _) = setup_test_app().await;
    // An account left without a user (its user removed outside calrs), with
    // its data still attached.
    let orphan = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO accounts (id, name, email, timezone, user_id) \
         VALUES (?, 'Gone', 'Gone@example.com', 'UTC', NULL)",
    )
    .bind(&orphan)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO caldav_sources (id, account_id, name, url, username, password_enc) \
         VALUES (?, ?, 'Work', 'https://dav.example.com/', 'u', 'BLOB')",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(&orphan)
    .execute(&pool)
    .await
    .unwrap();

    let response = register(app, "+Gone%40example.com+").await;
    assert!(response.status().is_redirection());

    let new_user: String =
        sqlx::query_scalar("SELECT id FROM users WHERE email = 'Gone@example.com'")
            .fetch_one(&pool)
            .await
            .expect("the address is stored trimmed");
    let owner: Option<String> = sqlx::query_scalar("SELECT user_id FROM accounts WHERE id = ?")
        .bind(&orphan)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(owner, None);
    let own: Vec<String> = sqlx::query_scalar("SELECT id FROM accounts WHERE user_id = ?")
        .bind(&new_user)
        .fetch_all(&pool)
        .await
        .unwrap();
    assert_eq!(own.len(), 1);
    assert_ne!(own[0], orphan);
    let own_email: String = sqlx::query_scalar("SELECT email FROM accounts WHERE id = ?")
        .bind(&own[0])
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(own_email, crate::auth::placeholder_account_email(&own[0]));

    // A restart does not hand it over either.
    crate::db::migrate(&pool).await.unwrap();
    let owner: Option<String> = sqlx::query_scalar("SELECT user_id FROM accounts WHERE id = ?")
        .bind(&orphan)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(owner, None);
}
