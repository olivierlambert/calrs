//! Local-auth MFA. A challenge is never a session; only completing it grants
//! dashboard access. SQLite transactions serialize code consumption and session
//! creation, including recovery codes and concurrent enrollment attempts.
use std::sync::Arc;

use anyhow::{bail, Context, Result};
use axum::{
    extract::{Form, State},
    http::{HeaderMap, StatusCode},
    response::{AppendHeaders, Html, IntoResponse, Redirect, Response},
    routing::{get, post},
    Router,
};
use axum_extra::extract::CookieJar;
use chrono::Utc;
use rand::{rngs::OsRng, RngCore};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use sqlx::{SqliteConnection, SqlitePool};
use subtle::ConstantTimeEq;
use totp_rs::{Builder, Totp};
use zeroize::Zeroizing;

use crate::{
    auth, crypto,
    models::{Session, User},
    web::{self, AppState},
};

const CHALLENGE_COOKIE: &str = "__Host-calrs_mfa";
const SESSION_COOKIE: &str = "__Host-calrs_session";
const CHALLENGE_SECONDS: i64 = 600;

fn random_hex(bytes: usize) -> String {
    let mut value = Zeroizing::new(vec![0; bytes]);
    OsRng.fill_bytes(&mut value);
    hex::encode(&*value)
}

fn hash(value: &str) -> String {
    hex::encode(Sha256::digest(value.as_bytes()))
}

fn totp(secret: &str, email: &str) -> Result<Totp> {
    Ok(Builder::new()
        .with_secret(hex::decode(secret)?)
        .with_account_name(email.replace(':', "_"))
        .with_issuer(Some("calrs"))
        .build()?)
}

// Return the matched counter so it can be consumed once, even across processes.
fn matching_step(secret: &str, code: &str, now: i64) -> Result<Option<i64>> {
    if code.len() != 6 || !code.bytes().all(|c| c.is_ascii_digit()) || now < 30 {
        return Ok(None);
    }
    let generator = totp(secret, "user")?;
    for step in (now / 30 - 1..=now / 30 + 1).rev() {
        let expected = generator.generate((step * 30) as u64).to_string();
        if bool::from(expected.as_bytes().ct_eq(code.as_bytes())) {
            return Ok(Some(step));
        }
    }
    Ok(None)
}

pub async fn required(pool: &SqlitePool) -> Result<bool> {
    Ok(
        sqlx::query_scalar("SELECT mfa_required FROM auth_config WHERE id = 'singleton'")
            .fetch_one(pool)
            .await?,
    )
}

pub async fn enrolled(pool: &SqlitePool, user_id: &str) -> Result<bool> {
    Ok(
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM user_mfa WHERE user_id = ?)")
            .bind(user_id)
            .fetch_one(pool)
            .await?,
    )
}

// Persistent, account-wide budget: starting a new challenge or changing IPs
// cannot reset the allowance. Count management attempts as well as login codes.
async fn limited(pool: &SqlitePool, user_id: &str) -> Result<bool> {
    let now = Utc::now().timestamp();
    let attempts: i64 = sqlx::query_scalar(
        "INSERT INTO mfa_attempts (user_id, window_start, attempts) VALUES (?, ?, 1)
         ON CONFLICT(user_id) DO UPDATE SET
           attempts = CASE WHEN window_start <= excluded.window_start - 900 THEN 1 ELSE MIN(attempts + 1, 11) END,
           window_start = CASE WHEN window_start <= excluded.window_start - 900 THEN excluded.window_start ELSE window_start END
         RETURNING attempts")
        .bind(user_id).bind(now).fetch_one(pool).await?;
    Ok(attempts > 10)
}

pub enum LoginResult {
    Session(Session),
    Challenge(String),
}

/// Called only after password verification (or successful registration).
/// `setup` is an explicit, password-confirmed request to enable optional MFA.
pub async fn begin_login(
    pool: &SqlitePool,
    key: &[u8; 32],
    user: &User,
    setup: bool,
) -> Result<LoginResult> {
    let now = Utc::now().timestamp();
    let mut tx = pool.begin().await?;
    // Acquire the SQLite write lock before checking policy/enrollment.
    sqlx::query("DELETE FROM mfa_challenges WHERE expires_at <= ?")
        .bind(now)
        .execute(&mut *tx)
        .await?;
    let current: User = sqlx::query_as(
        "SELECT * FROM users WHERE id = ? AND enabled = 1 AND auth_provider = 'local'",
    )
    .bind(&user.id)
    .fetch_one(&mut *tx)
    .await?;
    if current.password_hash != user.password_hash {
        bail!("credentials changed");
    }
    let active: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM user_mfa WHERE user_id = ?)")
            .bind(&user.id)
            .fetch_one(&mut *tx)
            .await?;
    let mandatory: bool =
        sqlx::query_scalar("SELECT mfa_required FROM auth_config WHERE id = 'singleton'")
            .fetch_one(&mut *tx)
            .await?;
    if setup && active {
        bail!("MFA already enabled");
    }
    let result = if active || mandatory || setup {
        let token = random_hex(32);
        let secret_enc = if active {
            None
        } else {
            Some(crypto::encrypt_value(key, &Zeroizing::new(random_hex(20)))?)
        };
        // Only the latest password-authenticated challenge survives.
        sqlx::query("DELETE FROM mfa_challenges WHERE user_id = ?")
            .bind(&user.id)
            .execute(&mut *tx)
            .await?;
        sqlx::query("INSERT INTO mfa_challenges (token_hash, user_id, password_hash, purpose, secret_enc, expires_at) VALUES (?, ?, ?, ?, ?, ?)")
            .bind(hash(&token)).bind(&user.id).bind(user.password_hash.as_deref().context("missing password")?)
            .bind(if active { "login" } else { "enroll" }).bind(secret_enc)
            .bind(now + CHALLENGE_SECONDS).execute(&mut *tx).await?;
        LoginResult::Challenge(token)
    } else {
        LoginResult::Session(auth::create_session(&mut *tx, &user.id).await?)
    };
    tx.commit().await?;
    Ok(result)
}

fn session_cookie(session: &Session) -> String {
    format!(
        "{SESSION_COOKIE}={}; HttpOnly; Secure; SameSite=Lax; Path=/; Max-Age=2592000",
        session.id
    )
}

fn challenge_cookie(token: &str, age: i64) -> String {
    format!("{CHALLENGE_COOKIE}={token}; HttpOnly; Secure; SameSite=Strict; Path=/; Max-Age={age}")
}

pub async fn cancel_challenge(pool: &SqlitePool, jar: &CookieJar) -> Result<String> {
    if let Some(cookie) = jar.get(CHALLENGE_COOKIE) {
        sqlx::query("DELETE FROM mfa_challenges WHERE token_hash = ?")
            .bind(hash(cookie.value()))
            .execute(pool)
            .await?;
    }
    Ok(challenge_cookie("", 0))
}

pub fn login_response(result: LoginResult) -> Response {
    match result {
        LoginResult::Session(session) => (
            AppendHeaders([
                ("Set-Cookie", session_cookie(&session)),
                ("Set-Cookie", challenge_cookie("", 0)),
            ]),
            Redirect::to("/dashboard"),
        )
            .into_response(),
        LoginResult::Challenge(token) => (
            [("Set-Cookie", challenge_cookie(&token, CHALLENGE_SECONDS))],
            Redirect::to("/auth/mfa"),
        )
            .into_response(),
    }
}

#[derive(sqlx::FromRow)]
struct Challenge {
    user_id: String,
    purpose: String,
    secret_enc: Option<String>,
    email: String,
}

async fn challenge(conn: &mut SqliteConnection, token: &str) -> Result<Option<Challenge>> {
    Ok(sqlx::query_as(
        "SELECT c.user_id, c.purpose, c.secret_enc, u.email FROM mfa_challenges c
         JOIN users u ON u.id = c.user_id
         WHERE c.token_hash = ? AND c.expires_at > ? AND u.enabled = 1
           AND u.auth_provider = 'local' AND c.password_hash = u.password_hash",
    )
    .bind(hash(token))
    .bind(Utc::now().timestamp())
    .fetch_optional(conn)
    .await?)
}

async fn new_recovery_codes(conn: &mut SqliteConnection, user_id: &str) -> Result<Vec<String>> {
    sqlx::query("DELETE FROM mfa_recovery_codes WHERE user_id = ?")
        .bind(user_id)
        .execute(&mut *conn)
        .await?;
    let mut codes = Vec::new();
    for _ in 0..10 {
        // 128 random bits: SHA-256 is sufficient for these high-entropy codes.
        let code = random_hex(16);
        sqlx::query("INSERT INTO mfa_recovery_codes (user_id, code_hash) VALUES (?, ?)")
            .bind(user_id)
            .bind(hash(&code))
            .execute(&mut *conn)
            .await?;
        codes.push(code);
    }
    Ok(codes)
}

async fn consume_code(
    conn: &mut SqliteConnection,
    key: &[u8; 32],
    user_id: &str,
    code: &str,
) -> Result<bool> {
    let credential: Option<(String, i64)> =
        sqlx::query_as("SELECT secret_enc, last_step FROM user_mfa WHERE user_id = ?")
            .bind(user_id)
            .fetch_optional(&mut *conn)
            .await?;
    let Some((encrypted, last_step)) = credential else {
        return Ok(false);
    };
    if code.len() == 32 && code.bytes().all(|c| c.is_ascii_hexdigit()) {
        return Ok(sqlx::query(
            "DELETE FROM mfa_recovery_codes WHERE user_id = ? AND code_hash = ?",
        )
        .bind(user_id)
        .bind(hash(&code.to_ascii_lowercase()))
        .execute(conn)
        .await?
        .rows_affected()
            == 1);
    }
    let secret = Zeroizing::new(crypto::decrypt_value(key, &encrypted)?);
    let Some(step) = matching_step(&secret, code, Utc::now().timestamp())? else {
        return Ok(false);
    };
    if step <= last_step {
        return Ok(false);
    }
    Ok(
        sqlx::query("UPDATE user_mfa SET last_step = ? WHERE user_id = ? AND last_step < ?")
            .bind(step)
            .bind(user_id)
            .bind(step)
            .execute(conn)
            .await?
            .rows_affected()
            == 1,
    )
}

async fn verified_session(conn: &mut SqliteConnection, user_id: &str) -> Result<Session> {
    let session = auth::create_session(&mut *conn, user_id).await?;
    sqlx::query("UPDATE sessions SET mfa_verified = 1 WHERE id = ?")
        .bind(&session.id)
        .execute(conn)
        .await?;
    Ok(session)
}

async fn revoke_access(conn: &mut SqliteConnection, user_id: &str) -> Result<()> {
    sqlx::query("DELETE FROM sessions WHERE user_id = ?")
        .bind(user_id)
        .execute(&mut *conn)
        .await?;
    sqlx::query("DELETE FROM mfa_challenges WHERE user_id = ?")
        .bind(user_id)
        .execute(conn)
        .await?;
    Ok(())
}

async fn finish_challenge(
    pool: &SqlitePool,
    key: &[u8; 32],
    token: &str,
    code: &str,
) -> Result<Option<(Session, Vec<String>)>> {
    let Some(initial) = challenge(&mut *pool.acquire().await?, token).await? else {
        return Ok(None);
    };
    if limited(pool, &initial.user_id).await? {
        return Ok(None);
    }
    let mut tx = pool.begin().await?;
    // Write first: concurrent consumers cannot both observe the same challenge.
    sqlx::query("UPDATE mfa_challenges SET expires_at = expires_at WHERE token_hash = ?")
        .bind(hash(token))
        .execute(&mut *tx)
        .await?;
    let Some(c) = challenge(&mut tx, token).await? else {
        return Ok(None);
    };
    let mut codes = Vec::new();
    if c.purpose == "enroll" {
        let encrypted = c.secret_enc.context("missing enrollment secret")?;
        let secret = Zeroizing::new(crypto::decrypt_value(key, &encrypted)?);
        let Some(step) = matching_step(&secret, code, Utc::now().timestamp())? else {
            return Ok(None);
        };
        let inserted = sqlx::query(
            "INSERT OR IGNORE INTO user_mfa (user_id, secret_enc, last_step) VALUES (?, ?, ?)",
        )
        .bind(&c.user_id)
        .bind(encrypted)
        .bind(step)
        .execute(&mut *tx)
        .await?;
        if inserted.rows_affected() != 1 {
            return Ok(None);
        }
        codes = new_recovery_codes(&mut tx, &c.user_id).await?;
        revoke_access(&mut tx, &c.user_id).await?;
    } else if !consume_code(&mut tx, key, &c.user_id, code).await? {
        return Ok(None);
    }
    sqlx::query("DELETE FROM mfa_challenges WHERE token_hash = ?")
        .bind(hash(token))
        .execute(&mut *tx)
        .await?;
    let session = verified_session(&mut tx, &c.user_id).await?;
    tx.commit().await?;
    tracing::info!(user_id = %c.user_id, action = %c.purpose, "MFA completed");
    Ok(Some((session, codes)))
}

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/auth/mfa", get(challenge_page).post(challenge_submit))
        .route(
            "/dashboard/settings/mfa",
            get(settings_page).post(settings_action),
        )
        .route("/dashboard/admin/mfa", post(policy_save))
}

fn render(state: &AppState, lang: &str, values: minijinja::Value) -> Response {
    let csrf = web::generate_csrf_token();
    let result = state.templates.get_template("auth/mfa.html").and_then(|t| {
        t.render(minijinja::context! {
            lang => lang, csrf_token => csrf, ..values
        })
    });
    match result {
        Ok(body) => (
            [
                ("Cache-Control", "no-store"),
                ("Referrer-Policy", "no-referrer"),
                ("Set-Cookie", &web::csrf_cookie_value(&csrf)),
            ],
            Html(body),
        )
            .into_response(),
        Err(e) => web::internal_error_response("MFA template", &e),
    }
}

fn error(state: &AppState, lang: &str) -> Response {
    render(state, lang, minijinja::context! { mode => "error" })
}

async fn challenge_view(
    state: &AppState,
    lang: &str,
    token: &str,
    invalid: bool,
) -> Result<Response> {
    let c = challenge(&mut *state.pool.acquire().await?, token).await?;
    let Some(c) = c else {
        return Ok(error(state, lang));
    };
    let (secret, qr) = if let Some(encrypted) = c.secret_enc {
        let secret = Zeroizing::new(crypto::decrypt_value(&state.secret_key, &encrypted)?);
        let generator = totp(&secret, &c.email)?;
        (generator.secret().to_base32(), generator.to_qr_base64()?)
    } else {
        (String::new(), String::new())
    };
    Ok(render(
        state,
        lang,
        minijinja::context! {
            mode => c.purpose, secret => secret, qr => qr, invalid => invalid,
        },
    ))
}

async fn challenge_page(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    jar: CookieJar,
) -> Response {
    let lang = crate::i18n::detect_from_headers(&headers);
    let token = jar.get(CHALLENGE_COOKIE).map(|c| c.value()).unwrap_or("");
    match challenge_view(&state, lang, token, false).await {
        Ok(r) => r,
        Err(e) => web::internal_error_response("MFA challenge", &e),
    }
}

#[derive(Deserialize)]
struct CodeForm {
    _csrf: Option<String>,
    code: String,
}

async fn challenge_submit(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    jar: CookieJar,
    Form(form): Form<CodeForm>,
) -> Response {
    if let Err(r) = web::verify_csrf_token(&headers, &form._csrf) {
        return r;
    }
    let lang = crate::i18n::detect_from_headers(&headers);
    let token = jar.get(CHALLENGE_COOKIE).map(|c| c.value()).unwrap_or("");
    match finish_challenge(&state.pool, &state.secret_key, token, form.code.trim()).await {
        Ok(Some((session, codes))) => completion(&state, lang, session, codes),
        Ok(None) => match challenge_view(&state, lang, token, true).await {
            Ok(r) => r,
            Err(e) => web::internal_error_response("MFA challenge", &e),
        },
        Err(e) => web::internal_error_response("MFA verification", &e),
    }
}

fn completion(state: &AppState, lang: &str, session: Session, codes: Vec<String>) -> Response {
    if codes.is_empty() {
        return login_response(LoginResult::Session(session));
    }
    let mut response = render(
        state,
        lang,
        minijinja::context! { mode => "recovery", codes => codes },
    );
    response
        .headers_mut()
        .append("Set-Cookie", session_cookie(&session).parse().unwrap());
    response
        .headers_mut()
        .append("Set-Cookie", challenge_cookie("", 0).parse().unwrap());
    response
}

async fn settings_page(State(state): State<Arc<AppState>>, user: auth::AuthUser) -> Response {
    if user.impersonation.is_some() || user.user.auth_provider != "local" {
        return StatusCode::FORBIDDEN.into_response();
    }
    let result: Result<_> = async {
        Ok((
            enrolled(&state.pool, &user.user.id).await?,
            required(&state.pool).await?,
        ))
    }
    .await;
    match result {
        Ok((active, mandatory)) => render(
            &state,
            user.lang,
            minijinja::context! {
                mode => "settings", active => active, mandatory => mandatory,
            },
        ),
        Err(e) => web::internal_error_response("MFA settings", &e),
    }
}

#[derive(Deserialize)]
struct ManagementForm {
    _csrf: Option<String>,
    password: String,
    #[serde(default)]
    code: String,
    action: String,
}

// Recheck enabled/provider/password after obtaining the write lock, protecting
// management requests racing account disable, password reset or OIDC linking.
async fn same_local_user(conn: &mut SqliteConnection, user: &User) -> Result<bool> {
    Ok(sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM users WHERE id = ? AND enabled = 1 AND auth_provider = 'local' AND password_hash = ?)")
        .bind(&user.id).bind(&user.password_hash).fetch_one(conn).await?)
}

async fn manage(
    state: &AppState,
    user: &User,
    form: &ManagementForm,
) -> Result<Option<(Session, Vec<String>)>> {
    let mut tx = state.pool.begin().await?;
    sqlx::query("UPDATE user_mfa SET last_step = last_step WHERE user_id = ?")
        .bind(&user.id)
        .execute(&mut *tx)
        .await?;
    if !same_local_user(&mut tx, user).await? {
        return Ok(None);
    }
    let mandatory: bool =
        sqlx::query_scalar("SELECT mfa_required FROM auth_config WHERE id = 'singleton'")
            .fetch_one(&mut *tx)
            .await?;
    if form.action == "disable" && mandatory {
        return Ok(None);
    }
    if !consume_code(&mut tx, &state.secret_key, &user.id, form.code.trim()).await? {
        return Ok(None);
    }
    let codes = match form.action.as_str() {
        "disable" => {
            sqlx::query("DELETE FROM user_mfa WHERE user_id = ?")
                .bind(&user.id)
                .execute(&mut *tx)
                .await?;
            Vec::new()
        }
        "regenerate" => new_recovery_codes(&mut tx, &user.id).await?,
        _ => return Ok(None),
    };
    revoke_access(&mut tx, &user.id).await?;
    let session = if form.action == "disable" {
        auth::create_session(&mut *tx, &user.id).await?
    } else {
        verified_session(&mut tx, &user.id).await?
    };
    tx.commit().await?;
    tracing::info!(user_id = %user.id, action = %form.action, "MFA settings changed");
    Ok(Some((session, codes)))
}

async fn settings_action(
    State(state): State<Arc<AppState>>,
    user: auth::AuthUser,
    headers: HeaderMap,
    Form(form): Form<ManagementForm>,
) -> Response {
    if let Err(r) = web::verify_csrf_token(&headers, &form._csrf) {
        return r;
    }
    if user.impersonation.is_some() || user.user.auth_provider != "local" {
        return StatusCode::FORBIDDEN.into_response();
    }
    let result: Result<Response> = async {
        if limited(&state.pool, &user.user.id).await?
            || !auth::verify_password(
                &form.password,
                user.user.password_hash.as_deref().unwrap_or(""),
            )
        {
            return Ok(error(&state, user.lang));
        }
        if form.action == "enable" {
            return Ok(login_response(
                begin_login(&state.pool, &state.secret_key, &user.user, true).await?,
            ));
        }
        Ok(match manage(&state, &user.user, &form).await? {
            Some((session, codes)) => completion(&state, user.lang, session, codes),
            None => error(&state, user.lang),
        })
    }
    .await;
    result.unwrap_or_else(|e| web::internal_error_response("MFA management", &e))
}

#[derive(Deserialize)]
struct PolicyForm {
    _csrf: Option<String>,
    policy: String,
    #[serde(default)]
    password: String,
    #[serde(default)]
    code: String,
}

async fn policy_save(
    State(state): State<Arc<AppState>>,
    admin: auth::AdminUser,
    headers: HeaderMap,
    Form(form): Form<PolicyForm>,
) -> Response {
    if let Err(r) = web::verify_csrf_token(&headers, &form._csrf) {
        return r;
    }
    let mandatory = match form.policy.as_str() {
        "required" => true,
        "optional" => false,
        _ => return StatusCode::BAD_REQUEST.into_response(),
    };
    let result: Result<bool> = async {
        if admin.user.auth_provider == "local" && (limited(&state.pool, &admin.user.id).await?
            || !auth::verify_password(&form.password, admin.user.password_hash.as_deref().unwrap_or(""))) { return Ok(false); }
        let mut tx = state.pool.begin().await?;
        sqlx::query("UPDATE auth_config SET mfa_required = mfa_required WHERE id = 'singleton'").execute(&mut *tx).await?;
        let still_admin: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM users WHERE id = ? AND enabled = 1 AND role = 'admin' AND auth_provider = ?)")
            .bind(&admin.user.id).bind(&admin.user.auth_provider).fetch_one(&mut *tx).await?;
        if !still_admin { return Ok(false); }
        if admin.user.auth_provider == "local" && (!same_local_user(&mut tx, &admin.user).await?
            || !consume_code(&mut tx, &state.secret_key, &admin.user.id, form.code.trim()).await?) { return Ok(false); }
        sqlx::query("UPDATE auth_config SET mfa_required = ?, updated_at = datetime('now') WHERE id = 'singleton'")
            .bind(mandatory).execute(&mut *tx).await?;
        if mandatory {
            sqlx::query("DELETE FROM sessions WHERE mfa_verified = 0 AND user_id IN (SELECT id FROM users WHERE auth_provider = 'local')")
                .execute(&mut *tx).await?;
        }
        tx.commit().await?;
        tracing::info!(admin = %admin.user.id, required = mandatory, "MFA policy changed");
        Ok(true)
    }.await;
    match result {
        Ok(true) => Redirect::to("/dashboard/admin").into_response(),
        Ok(false) => error(&state, admin.lang),
        Err(e) => web::internal_error_response("MFA policy", &e),
    }
}

/// Server-operator recovery. The policy remains in force, so the next local
/// login must enroll again if MFA is required. Never reset MFA on password save.
pub async fn reset(pool: &SqlitePool, email: &str) -> Result<()> {
    let mut tx = pool.begin().await?;
    sqlx::query("DELETE FROM mfa_challenges WHERE expires_at <= ?")
        .bind(Utc::now().timestamp())
        .execute(&mut *tx)
        .await?;
    let id: String =
        sqlx::query_scalar("SELECT id FROM users WHERE email = ? AND auth_provider = 'local'")
            .bind(email)
            .fetch_one(&mut *tx)
            .await
            .context("local user not found")?;
    sqlx::query("DELETE FROM user_mfa WHERE user_id = ?")
        .bind(&id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM mfa_attempts WHERE user_id = ?")
        .bind(&id)
        .execute(&mut *tx)
        .await?;
    revoke_access(&mut tx, &id).await?;
    tx.commit().await?;
    tracing::info!(user_id = %id, "MFA reset by server operator");
    Ok(())
}

#[cfg(test)]
mod tests;
