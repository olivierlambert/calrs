use super::*;
use axum::{body::Body, http::Request};
use http_body_util::BodyExt;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use std::{str::FromStr, sync::OnceLock};
use tower::ServiceExt;

const KEY: [u8; 32] = [42; 32];
const PASSWORD: &str = "correct-horse-battery";

async fn database() -> SqlitePool {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(
            SqliteConnectOptions::from_str("sqlite::memory:")
                .unwrap()
                .foreign_keys(true),
        )
        .await
        .unwrap();
    crate::db::migrate(&pool).await.unwrap();
    pool
}

async fn user(pool: &SqlitePool, email: &str) -> User {
    static PASSWORD_HASH: OnceLock<String> = OnceLock::new();
    let id = uuid::Uuid::new_v4().to_string();
    auth::create_local_user(
        pool,
        &id,
        email,
        "Test",
        PASSWORD_HASH.get_or_init(|| auth::hash_password(PASSWORD).unwrap()),
        &id,
        false,
    )
    .await
    .unwrap();
    // Keep SQL errors visible in fixtures instead of collapsing them into the
    // production lookup's fail-closed Option.
    sqlx::query_as("SELECT * FROM users WHERE id = ?")
        .bind(&id)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn token(pool: &SqlitePool, user: &User, setup: bool) -> String {
    match begin_login(pool, &KEY, user, setup).await.unwrap() {
        LoginResult::Challenge(t) => t,
        _ => panic!("expected MFA challenge"),
    }
}

async fn setup_secret(pool: &SqlitePool, token: &str) -> String {
    let stored: String =
        sqlx::query_scalar("SELECT secret_enc FROM mfa_challenges WHERE token_hash = ?")
            .bind(hash(token))
            .fetch_one(pool)
            .await
            .unwrap();
    assert!(stored.starts_with("enc:v1:"));
    crypto::decrypt_value(&KEY, &stored).unwrap()
}

fn code(secret: &str, offset: i64) -> String {
    totp(secret, "test@example.com")
        .unwrap()
        .generate((Utc::now().timestamp() + offset) as u64)
        .to_string()
}

async fn enroll(pool: &SqlitePool, user: &User) -> (String, Session, Vec<String>) {
    let token = token(pool, user, true).await;
    let secret = setup_secret(pool, &token).await;
    // Consume the previous time step, leaving the current step for the test.
    // If verification crosses the 30-second boundary, that previous step has
    // legitimately expired. Retry only in that case, never hide other failures.
    for _ in 0..2 {
        let now = Utc::now().timestamp();
        let previous = totp(&secret, &user.email)
            .unwrap()
            .generate((now - 30) as u64)
            .to_string();
        if let Some((session, codes)) = finish_challenge(pool, &KEY, &token, &previous)
            .await
            .unwrap()
        {
            return (secret, session, codes);
        }
        assert_ne!(now / 30, Utc::now().timestamp() / 30);
    }
    panic!("enrollment crossed two consecutive TOTP boundaries");
}

async fn count(pool: &SqlitePool, table: &str) -> i64 {
    sqlx::query_scalar(&format!("SELECT COUNT(*) FROM {table}"))
        .fetch_one(pool)
        .await
        .unwrap()
}

#[test]
fn rfc6238_vectors_and_validation_window() {
    let secret = hex::encode(b"12345678901234567890");
    let generator = Builder::new()
        .with_secret(b"12345678901234567890".to_vec())
        .with_digits(8)
        .build()
        .unwrap();
    for (time, expected) in [
        (59, "94287082"),
        (1111111109, "07081804"),
        (1111111111, "14050471"),
        (1234567890, "89005924"),
        (2000000000, "69279037"),
        (20000000000, "65353130"),
    ] {
        assert_eq!(generator.generate(time).to_string(), expected);
        assert_eq!(
            matching_step(&secret, &expected[2..], time as i64).unwrap(),
            Some(time as i64 / 30)
        );
    }
    assert_eq!(matching_step(&secret, "287082", 60).unwrap(), Some(1));
    assert_eq!(matching_step(&secret, "287082", 29).unwrap(), None);
    assert_eq!(matching_step(&secret, "287082", 90).unwrap(), None);
    for invalid in ["", "12345", "1234567", "１２３４５６", "abcdef"] {
        assert_eq!(matching_step(&secret, invalid, 59).unwrap(), None);
    }
}

#[tokio::test]
async fn optional_login_preserves_existing_behavior() {
    let pool = database().await;
    let user = user(&pool, "user@example.com").await;
    assert!(!required(&pool).await.unwrap());
    let LoginResult::Session(session) = begin_login(&pool, &KEY, &user, false).await.unwrap()
    else {
        panic!()
    };
    assert!(auth::validate_session(&pool, &session.id).await.is_some());
    assert_eq!(count(&pool, "mfa_challenges").await, 0);
}

#[tokio::test]
async fn enrollment_requires_proof_and_revokes_old_sessions() {
    let pool = database().await;
    let user = user(&pool, "user@example.com").await;
    let old = auth::create_session(&pool, &user.id).await.unwrap();
    let token = token(&pool, &user, true).await;
    let secret = setup_secret(&pool, &token).await;
    assert!(!enrolled(&pool, &user.id).await.unwrap());
    assert!(auth::validate_session(&pool, &token).await.is_none());
    assert!(finish_challenge(&pool, &KEY, &token, "bad")
        .await
        .unwrap()
        .is_none());
    assert_eq!(count(&pool, "sessions").await, 1);
    let (session, codes) = finish_challenge(&pool, &KEY, &token, &code(&secret, 0))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(codes.len(), 10);
    assert!(auth::validate_session(&pool, &session.id).await.is_some());
    assert!(auth::validate_session(&pool, &old.id).await.is_none());
    let encrypted: String = sqlx::query_scalar("SELECT secret_enc FROM user_mfa")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_ne!(encrypted, secret);
    let stored: Vec<String> = sqlx::query_scalar("SELECT code_hash FROM mfa_recovery_codes")
        .fetch_all(&pool)
        .await
        .unwrap();
    assert!(codes
        .iter()
        .all(|c| !stored.contains(c) && stored.contains(&hash(c))));
    assert!(finish_challenge(&pool, &KEY, &token, &code(&secret, 0))
        .await
        .unwrap()
        .is_none());
    assert!(begin_login(&pool, &KEY, &user, true).await.is_err());
}

#[tokio::test]
async fn login_totp_is_single_use_across_challenges() {
    let pool = database().await;
    let user = user(&pool, "user@example.com").await;
    let (secret, _, _) = enroll(&pool, &user).await;
    let first = token(&pool, &user, false).await;
    let otp = code(&secret, 0);
    assert!(finish_challenge(&pool, &KEY, &first, &otp)
        .await
        .unwrap()
        .is_some());
    let second = token(&pool, &user, false).await;
    assert!(finish_challenge(&pool, &KEY, &second, &otp)
        .await
        .unwrap()
        .is_none());
    assert!(finish_challenge(&pool, &KEY, &second, &code(&secret, -30))
        .await
        .unwrap()
        .is_none());
}

#[tokio::test]
async fn recovery_codes_are_single_use_and_bound_to_user() {
    let pool = database().await;
    let alice = user(&pool, "alice@example.com").await;
    let bob = user(&pool, "bob@example.com").await;
    let (_, _, codes) = enroll(&pool, &alice).await;
    enroll(&pool, &bob).await;
    let bob_token = token(&pool, &bob, false).await;
    assert!(finish_challenge(&pool, &KEY, &bob_token, &codes[0])
        .await
        .unwrap()
        .is_none());
    let first = token(&pool, &alice, false).await;
    assert!(finish_challenge(&pool, &KEY, &first, &codes[0])
        .await
        .unwrap()
        .is_some());
    let second = token(&pool, &alice, false).await;
    assert!(finish_challenge(&pool, &KEY, &second, &codes[0])
        .await
        .unwrap()
        .is_none());
    assert_eq!(count(&pool, "mfa_recovery_codes").await, 19);
}

#[tokio::test]
async fn stale_challenges_fail_after_expiry_reset_disable_or_linking() {
    let pool = database().await;
    for change in ["expires", "password", "disable", "oidc", "reset"] {
        let user = user(&pool, &format!("{change}@example.com")).await;
        let (_, _, codes) = enroll(&pool, &user).await;
        let token = token(&pool, &user, false).await;
        match change {
            "expires" => {
                sqlx::query("UPDATE mfa_challenges SET expires_at = 0 WHERE user_id = ?")
                    .bind(&user.id)
                    .execute(&pool)
                    .await
                    .unwrap();
            }
            "password" => {
                sqlx::query("UPDATE users SET password_hash = 'changed' WHERE id = ?")
                    .bind(&user.id)
                    .execute(&pool)
                    .await
                    .unwrap();
            }
            "disable" => {
                sqlx::query("UPDATE users SET enabled = 0 WHERE id = ?")
                    .bind(&user.id)
                    .execute(&pool)
                    .await
                    .unwrap();
            }
            "oidc" => {
                sqlx::query("UPDATE users SET auth_provider = 'oidc' WHERE id = ?")
                    .bind(&user.id)
                    .execute(&pool)
                    .await
                    .unwrap();
            }
            "reset" => reset(&pool, &user.email).await.unwrap(),
            _ => unreachable!(),
        }
        assert!(
            finish_challenge(&pool, &KEY, &token, &codes[0])
                .await
                .unwrap()
                .is_none(),
            "{change}"
        );
    }
}

#[tokio::test]
async fn account_budget_survives_new_challenges() {
    let pool = database().await;
    let user = user(&pool, "user@example.com").await;
    let (_, _, codes) = enroll(&pool, &user).await;
    for _ in 0..9 {
        let token = token(&pool, &user, false).await;
        assert!(finish_challenge(&pool, &KEY, &token, "bad")
            .await
            .unwrap()
            .is_none());
    }
    let token = token(&pool, &user, false).await;
    assert!(finish_challenge(&pool, &KEY, &token, &codes[0])
        .await
        .unwrap()
        .is_none());
    sqlx::query("UPDATE mfa_attempts SET window_start = window_start - 901")
        .execute(&pool)
        .await
        .unwrap();
    assert!(finish_challenge(&pool, &KEY, &token, &codes[0])
        .await
        .unwrap()
        .is_some());
}

#[tokio::test]
async fn concurrent_completion_creates_only_one_session() {
    // Use multiple SQLite connections to a real WAL database, not just a
    // single-connection in-memory pool, to exercise transaction serialization.
    let dir = std::env::temp_dir().join(format!("calrs-mfa-race-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let pool = crate::db::connect(&dir).await.unwrap();
    crate::db::migrate(&pool).await.unwrap();
    let user = user(&pool, "user@example.com").await;
    let (_, _, codes) = enroll(&pool, &user).await;
    let token = token(&pool, &user, false).await;
    let (a, b) = tokio::join!(
        finish_challenge(&pool, &KEY, &token, &codes[0]),
        finish_challenge(&pool, &KEY, &token, &codes[0])
    );
    assert_eq!(
        usize::from(a.unwrap().is_some()) + usize::from(b.unwrap().is_some()),
        1
    );
    assert_eq!(count(&pool, "sessions").await, 2);
    assert_eq!(count(&pool, "mfa_recovery_codes").await, 9);
    pool.close().await;
    std::fs::remove_dir_all(dir).unwrap();
}

#[tokio::test]
async fn policy_rejects_old_local_sessions_but_preserves_oidc() {
    let pool = database().await;
    let local = user(&pool, "local@example.com").await;
    let oidc = user(&pool, "oidc@example.com").await;
    sqlx::query("UPDATE users SET auth_provider = 'oidc' WHERE id = ?")
        .bind(&oidc.id)
        .execute(&pool)
        .await
        .unwrap();
    let old = auth::create_session(&pool, &local.id).await.unwrap();
    let sso = auth::create_session(&pool, &oidc.id).await.unwrap();
    sqlx::query("UPDATE auth_config SET mfa_required = 1")
        .execute(&pool)
        .await
        .unwrap();
    assert!(auth::validate_session(&pool, &old.id).await.is_none());
    assert!(auth::validate_session(&pool, &sso.id).await.is_some());
    let token = token(&pool, &local, false).await;
    let secret = setup_secret(&pool, &token).await;
    let (session, _) = finish_challenge(&pool, &KEY, &token, &code(&secret, 0))
        .await
        .unwrap()
        .unwrap();
    assert!(auth::validate_session(&pool, &session.id).await.is_some());
    reset(&pool, &local.email).await.unwrap();
    assert!(required(&pool).await.unwrap());
    assert!(auth::validate_session(&pool, &session.id).await.is_none());
    token_for_required_enrollment(&pool, &local).await;
}

async fn token_for_required_enrollment(pool: &SqlitePool, user: &User) {
    let token = token(pool, user, false).await;
    assert!(!setup_secret(pool, &token).await.is_empty());
}

async fn app(pool: &SqlitePool) -> Router {
    web::create_router(pool.clone(), std::env::temp_dir(), KEY).await
}

async fn request(app: &Router, method: &str, path: &str, cookies: &str, fields: &str) -> Response {
    app.clone()
        .oneshot(
            Request::builder()
                .method(method)
                .uri(path)
                .header("cookie", cookies)
                .header("content-type", "application/x-www-form-urlencoded")
                .body(Body::from(fields.to_owned()))
                .unwrap(),
        )
        .await
        .unwrap()
}

fn cookies(session: &str) -> String {
    format!("{SESSION_COOKIE}={session}; __Host-calrs_csrf=test")
}

fn response_cookie(response: &Response, name: &str) -> Option<String> {
    response
        .headers()
        .get_all("set-cookie")
        .iter()
        .filter_map(|v| v.to_str().ok())
        .find_map(|v| {
            v.strip_prefix(&format!("{name}="))
                .map(|v| v.split(';').next().unwrap().to_owned())
        })
}

async fn body(response: Response) -> String {
    String::from_utf8(
        response
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes()
            .to_vec(),
    )
    .unwrap()
}

#[tokio::test]
async fn http_login_requires_mfa_before_issuing_session() {
    let pool = database().await;
    let user = user(&pool, "user@example.com").await;
    let (_, _, codes) = enroll(&pool, &user).await;
    let app = app(&pool).await;
    let r = request(
        &app,
        "POST",
        "/auth/login",
        "__Host-calrs_csrf=test",
        &format!("_csrf=test&email=user%40example.com&password={PASSWORD}"),
    )
    .await;
    assert_eq!(r.headers()["location"], "/auth/mfa");
    assert!(response_cookie(&r, SESSION_COOKIE).is_none());
    let challenge = response_cookie(&r, CHALLENGE_COOKIE).unwrap();
    let cookie = format!("{CHALLENGE_COOKIE}={challenge}; __Host-calrs_csrf=test");
    let r = request(&app, "GET", "/dashboard", &cookie, "").await;
    assert_eq!(r.headers()["location"], "/auth/login");
    let r = request(
        &app,
        "POST",
        "/auth/mfa",
        &cookie,
        &format!("_csrf=test&code={}", codes[0]),
    )
    .await;
    assert_eq!(r.headers()["location"], "/dashboard");
    assert!(
        auth::validate_session(&pool, &response_cookie(&r, SESSION_COOKIE).unwrap())
            .await
            .is_some()
    );
}

#[tokio::test]
async fn http_registration_enforces_enrollment() {
    let pool = database().await;
    sqlx::query("UPDATE auth_config SET mfa_required = 1")
        .execute(&pool)
        .await
        .unwrap();
    let app = app(&pool).await;
    let r = request(
        &app,
        "POST",
        "/auth/register",
        "__Host-calrs_csrf=test",
        &format!("_csrf=test&name=New&email=new%40example.com&password={PASSWORD}"),
    )
    .await;
    assert_eq!(r.headers()["location"], "/auth/mfa");
    assert!(response_cookie(&r, SESSION_COOKIE).is_none());
    assert_eq!(count(&pool, "sessions").await, 0);
    let token = response_cookie(&r, CHALLENGE_COOKIE).unwrap();
    let r = request(
        &app,
        "GET",
        "/auth/mfa",
        &format!("{CHALLENGE_COOKIE}={token}"),
        "",
    )
    .await;
    assert_eq!(r.status(), 200);
    assert_eq!(r.headers()["cache-control"], "no-store");
    let csrf = response_cookie(&r, "__Host-calrs_csrf").unwrap();
    let html = body(r).await;
    assert!(html.contains("data:image/png;base64,"));
    assert!(html.contains(&format!("name=\"_csrf\" value=\"{csrf}\"")));
    let secret = setup_secret(&pool, &token).await;
    let r = request(
        &app,
        "POST",
        "/auth/mfa",
        &format!("{CHALLENGE_COOKIE}={token}; __Host-calrs_csrf={csrf}"),
        &format!("_csrf={csrf}&code={}", code(&secret, 0)),
    )
    .await;
    assert_eq!(r.status(), 200);
    assert!(response_cookie(&r, SESSION_COOKIE).is_some());
    assert_eq!(body(r).await.matches("class=\"recovery-code\"").count(), 10);
}

#[tokio::test]
async fn http_management_requires_password_code_csrf_and_real_user() {
    let pool = database().await;
    let admin = user(&pool, "admin@example.com").await;
    let target = user(&pool, "target@example.com").await;
    let (_, session, codes) = enroll(&pool, &admin).await;
    let app = app(&pool).await;
    let fields = format!(
        "_csrf=test&action=disable&password={PASSWORD}&code={}",
        codes[0]
    );
    let r = request(
        &app,
        "POST",
        "/dashboard/settings/mfa",
        &cookies(&session.id),
        "action=disable",
    )
    .await;
    assert!(r.status().is_client_error());
    let r = request(
        &app,
        "POST",
        "/dashboard/settings/mfa",
        &format!(
            "{}; __Host-calrs_impersonate={}",
            cookies(&session.id),
            target.id
        ),
        &fields,
    )
    .await;
    assert_eq!(r.status(), 403);
    let r = request(
        &app,
        "POST",
        "/dashboard/settings/mfa",
        &cookies(&session.id),
        &fields.replace(PASSWORD, "wrong"),
    )
    .await;
    assert!(response_cookie(&r, SESSION_COOKIE).is_none());
    assert!(enrolled(&pool, &admin.id).await.unwrap());
    let r = request(
        &app,
        "POST",
        "/dashboard/settings/mfa",
        &cookies(&session.id),
        &fields.replace("_csrf=test", "_csrf=wrong"),
    )
    .await;
    assert_eq!(r.status(), 403);
    let r = request(
        &app,
        "POST",
        "/dashboard/settings/mfa",
        &cookies(&session.id),
        &fields,
    )
    .await;
    assert_eq!(r.status(), 303);
    assert!(!enrolled(&pool, &admin.id).await.unwrap());
    assert_eq!(count(&pool, "mfa_recovery_codes").await, 0);
    assert!(auth::validate_session(&pool, &session.id).await.is_none());
}

#[tokio::test]
async fn http_policy_enforcement_and_keep_current_saves() {
    let pool = database().await;
    let admin = user(&pool, "admin@example.com").await;
    let member = user(&pool, "member@example.com").await;
    let app = app(&pool).await;
    let old = auth::create_session(&pool, &admin.id).await.unwrap();
    let r = request(
        &app,
        "POST",
        "/dashboard/admin/mfa",
        &cookies(&old.id),
        &format!("_csrf=test&policy=required&password={PASSWORD}&code=123456"),
    )
    .await;
    assert!(response_cookie(&r, SESSION_COOKIE).is_none());
    assert!(!required(&pool).await.unwrap());
    let (_, session, codes) = enroll(&pool, &admin).await;
    let member_session = auth::create_session(&pool, &member.id).await.unwrap();
    let r = request(
        &app,
        "POST",
        "/dashboard/admin/mfa",
        &cookies(&member_session.id),
        "_csrf=test&policy=required",
    )
    .await;
    assert_eq!(r.status(), 403);
    let r = request(
        &app,
        "POST",
        "/dashboard/admin/mfa",
        &cookies(&session.id),
        &format!(
            "_csrf=test&policy=required&password={PASSWORD}&code={}",
            codes[0]
        ),
    )
    .await;
    assert_eq!(r.status(), 303);
    assert!(required(&pool).await.unwrap());
    assert!(auth::validate_session(&pool, &session.id).await.is_some());
    assert!(auth::validate_session(&pool, &member_session.id)
        .await
        .is_none());
    let r = request(
        &app,
        "POST",
        "/dashboard/settings/mfa",
        &cookies(&session.id),
        &format!(
            "_csrf=test&action=disable&password={PASSWORD}&code={}",
            codes[1]
        ),
    )
    .await;
    assert!(response_cookie(&r, SESSION_COOKIE).is_none());
    assert!(enrolled(&pool, &admin.id).await.unwrap());
    // The ordinary admin save has no MFA fields; it must preserve the policy.
    request(
        &app,
        "POST",
        "/dashboard/admin/auth",
        &cookies(&session.id),
        "_csrf=test&registration_enabled=on",
    )
    .await;
    assert!(required(&pool).await.unwrap());
    assert_eq!(count(&pool, "mfa_recovery_codes").await, 9);
    let r = request(
        &app,
        "POST",
        "/dashboard/admin/mfa",
        &cookies(&session.id),
        &format!(
            "_csrf=test&policy=optional&password={PASSWORD}&code={}",
            codes[1]
        ),
    )
    .await;
    assert_eq!(r.status(), 303);
    assert!(!required(&pool).await.unwrap());
}

#[tokio::test]
async fn http_regeneration_revokes_previous_codes_and_sessions() {
    let pool = database().await;
    let user = user(&pool, "user@example.com").await;
    let (_, session, codes) = enroll(&pool, &user).await;
    let app = app(&pool).await;
    let r = request(
        &app,
        "POST",
        "/dashboard/settings/mfa",
        &cookies(&session.id),
        &format!(
            "_csrf=test&action=regenerate&password={PASSWORD}&code={}",
            codes[0]
        ),
    )
    .await;
    assert_eq!(r.status(), 200);
    let new_session = response_cookie(&r, SESSION_COOKIE).unwrap();
    assert_eq!(body(r).await.matches("class=\"recovery-code\"").count(), 10);
    assert!(auth::validate_session(&pool, &session.id).await.is_none());
    assert!(auth::validate_session(&pool, &new_session).await.is_some());
    let token = token(&pool, &user, false).await;
    assert!(finish_challenge(&pool, &KEY, &token, &codes[1])
        .await
        .unwrap()
        .is_none());
}

#[tokio::test]
async fn http_logout_cancels_pending_challenge() {
    let pool = database().await;
    let user = user(&pool, "user@example.com").await;
    let (_, _, codes) = enroll(&pool, &user).await;
    let token = token(&pool, &user, false).await;
    let app = app(&pool).await;
    let r = request(
        &app,
        "POST",
        "/auth/logout",
        &format!("{CHALLENGE_COOKIE}={token}; __Host-calrs_csrf=test"),
        "_csrf=test",
    )
    .await;
    assert_eq!(r.status(), 303);
    assert_eq!(response_cookie(&r, CHALLENGE_COOKIE).as_deref(), Some(""));
    assert_eq!(response_cookie(&r, SESSION_COOKIE).as_deref(), Some(""));
    assert!(finish_challenge(&pool, &KEY, &token, &codes[0])
        .await
        .unwrap()
        .is_none());
}

#[tokio::test]
async fn disable_enable_does_not_resurrect_a_pending_challenge() {
    let pool = database().await;
    let user = user(&pool, "user@example.com").await;
    let (_, _, codes) = enroll(&pool, &user).await;
    let token = token(&pool, &user, false).await;
    crate::commands::user::run(
        &pool,
        std::path::Path::new("/tmp"),
        crate::commands::user::UserCommands::Disable {
            email: user.email.clone(),
        },
    )
    .await
    .unwrap();
    crate::commands::user::run(
        &pool,
        std::path::Path::new("/tmp"),
        crate::commands::user::UserCommands::Enable {
            email: user.email.clone(),
        },
    )
    .await
    .unwrap();
    assert!(
        finish_challenge(&pool, &KEY, &token, &codes[0])
            .await
            .unwrap()
            .is_none(),
        "re-enabling the user must not revive a bearer challenge issued before account suspension"
    );
}

#[tokio::test]
async fn web_suspension_revokes_sessions_and_pending_enrollment() {
    let pool = database().await;
    let admin = user(&pool, "admin@example.com").await;
    let member = user(&pool, "member@example.com").await;
    let admin_session = auth::create_session(&pool, &admin.id).await.unwrap();
    let old_session = auth::create_session(&pool, &member.id).await.unwrap();
    let challenge = token(&pool, &member, true).await;
    let secret = setup_secret(&pool, &challenge).await;
    let app = app(&pool).await;
    let path = format!("/dashboard/admin/users/{}/toggle-enabled", member.id);
    for _ in 0..2 {
        let r = request(
            &app,
            "POST",
            &path,
            &cookies(&admin_session.id),
            "_csrf=test",
        )
        .await;
        assert_eq!(r.status(), 303);
    }
    assert!(auth::get_user_by_id(&pool, &member.id).await.is_some());
    assert!(auth::validate_session(&pool, &old_session.id)
        .await
        .is_none());
    assert!(finish_challenge(&pool, &KEY, &challenge, &code(&secret, 0))
        .await
        .unwrap()
        .is_none());
}

#[tokio::test]
async fn encrypted_secret_failure_never_creates_a_session() {
    let pool = database().await;
    let user = user(&pool, "user@example.com").await;
    let (secret, _, _) = enroll(&pool, &user).await;
    let token = token(&pool, &user, false).await;
    let before = count(&pool, "sessions").await;
    assert!(finish_challenge(&pool, &[9; 32], &token, &code(&secret, 0))
        .await
        .is_err());
    assert_eq!(count(&pool, "sessions").await, before);
    assert!(finish_challenge(&pool, &KEY, &token, &code(&secret, 0))
        .await
        .unwrap()
        .is_some());
}

#[tokio::test]
async fn replacing_enrollment_challenge_invalidates_old_secret() {
    let pool = database().await;
    let user = user(&pool, "user@example.com").await;
    let first = token(&pool, &user, true).await;
    let first_secret = setup_secret(&pool, &first).await;
    let second = token(&pool, &user, true).await;
    let second_secret = setup_secret(&pool, &second).await;
    assert_ne!(first_secret, second_secret);
    assert!(
        finish_challenge(&pool, &KEY, &first, &code(&first_secret, 0))
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        finish_challenge(&pool, &KEY, &second, &code(&second_secret, 0))
            .await
            .unwrap()
            .is_some()
    );
}

#[tokio::test]
async fn oidc_admin_can_manage_policy_without_local_mfa() {
    let pool = database().await;
    let admin = user(&pool, "admin@example.com").await;
    sqlx::query("UPDATE users SET auth_provider = 'oidc' WHERE id = ?")
        .bind(&admin.id)
        .execute(&pool)
        .await
        .unwrap();
    let session = auth::create_session(&pool, &admin.id).await.unwrap();
    let app = app(&pool).await;
    let r = request(
        &app,
        "POST",
        "/dashboard/admin/mfa",
        &cookies(&session.id),
        "_csrf=test&policy=required",
    )
    .await;
    assert_eq!(r.status(), 303);
    assert!(required(&pool).await.unwrap());
    assert!(auth::validate_session(&pool, &session.id).await.is_some());
    let r = request(
        &app,
        "GET",
        "/dashboard/settings/mfa",
        &cookies(&session.id),
        "",
    )
    .await;
    assert_eq!(r.status(), 403);
}
