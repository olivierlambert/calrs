use anyhow::{Context, Result};
use argon2::password_hash::rand_core::OsRng;
use argon2::password_hash::SaltString;
use argon2::{Argon2, PasswordHash, PasswordHasher, PasswordVerifier};
use axum::extract::{Form, FromRequestParts, State};
use axum::http::request::Parts;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Redirect, Response};
use axum_extra::extract::CookieJar;
use chrono::{Duration, Utc};
use rand::Rng;
use sqlx::SqlitePool;
use std::sync::Arc;

use crate::models::{AuthConfig, Session, User};
use crate::web::AppState;

const SESSION_COOKIE: &str = "calrs_session";
const SESSION_DURATION_DAYS: i64 = 30;

// --- Password hashing ---

pub fn hash_password(password: &str) -> Result<String> {
    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default();
    let hash = argon2
        .hash_password(password.as_bytes(), &salt)
        .map_err(|e| anyhow::anyhow!("Failed to hash password: {}", e))?;
    Ok(hash.to_string())
}

pub fn verify_password(password: &str, hash: &str) -> bool {
    let parsed = match PasswordHash::new(hash) {
        Ok(h) => h,
        Err(_) => return false,
    };
    Argon2::default()
        .verify_password(password.as_bytes(), &parsed)
        .is_ok()
}

// --- Session management ---

fn generate_session_token() -> String {
    let mut rng = rand::thread_rng();
    let bytes: [u8; 32] = rng.gen();
    hex::encode(bytes)
}

pub async fn create_session(pool: &SqlitePool, user_id: &str) -> Result<Session> {
    let id = generate_session_token();
    let expires_at = (Utc::now() + Duration::days(SESSION_DURATION_DAYS))
        .format("%Y-%m-%dT%H:%M:%S")
        .to_string();

    sqlx::query("INSERT INTO sessions (id, user_id, expires_at) VALUES (?, ?, ?)")
        .bind(&id)
        .bind(user_id)
        .bind(&expires_at)
        .execute(pool)
        .await
        .context("Failed to create session")?;

    Ok(Session {
        id,
        user_id: user_id.to_string(),
        expires_at,
        created_at: Utc::now().format("%Y-%m-%dT%H:%M:%S").to_string(),
    })
}

pub async fn validate_session(pool: &SqlitePool, token: &str) -> Option<User> {
    let now = Utc::now().format("%Y-%m-%dT%H:%M:%S").to_string();
    sqlx::query_as::<_, User>(
        "SELECT u.* FROM users u
         JOIN sessions s ON s.user_id = u.id
         WHERE s.id = ? AND s.expires_at > ? AND u.enabled = 1",
    )
    .bind(token)
    .bind(&now)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten()
}

pub async fn delete_session(pool: &SqlitePool, token: &str) -> Result<()> {
    sqlx::query("DELETE FROM sessions WHERE id = ?")
        .bind(token)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn cleanup_expired_sessions(pool: &SqlitePool) -> Result<u64> {
    let now = Utc::now().format("%Y-%m-%dT%H:%M:%S").to_string();
    let result = sqlx::query("DELETE FROM sessions WHERE expires_at <= ?")
        .bind(&now)
        .execute(pool)
        .await?;
    Ok(result.rows_affected())
}

// --- Auth config helpers ---

pub async fn get_auth_config(pool: &SqlitePool) -> Result<AuthConfig> {
    let config = sqlx::query_as::<_, AuthConfig>("SELECT * FROM auth_config WHERE id = 'singleton'")
        .fetch_one(pool)
        .await
        .context("Failed to load auth config")?;
    Ok(config)
}

pub fn is_email_allowed(email: &str, allowed_domains: &Option<String>) -> bool {
    let domains = match allowed_domains {
        Some(d) if !d.trim().is_empty() => d,
        _ => return true, // No restriction
    };

    let email_domain = match email.rsplit_once('@') {
        Some((_, domain)) => domain.to_lowercase(),
        None => return false,
    };

    domains
        .split(',')
        .map(|d| d.trim().to_lowercase())
        .any(|d| d == email_domain)
}

pub async fn has_any_users(pool: &SqlitePool) -> Result<bool> {
    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM users")
        .fetch_one(pool)
        .await?;
    Ok(count.0 > 0)
}

/// Generate a unique username from an email address.
pub async fn generate_username(pool: &SqlitePool, email: &str) -> Result<String> {
    let local_part = email.split('@').next().unwrap_or("user");
    let base: String = local_part
        .to_lowercase()
        .replace('.', "-")
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == '-')
        .collect();
    let base = if base.is_empty() { "user".to_string() } else { base };

    let mut candidate = base.clone();
    let mut suffix = 1u32;
    loop {
        let taken: Option<(String,)> =
            sqlx::query_as("SELECT id FROM users WHERE username = ?")
                .bind(&candidate)
                .fetch_optional(pool)
                .await?;
        if taken.is_none() {
            break;
        }
        candidate = format!("{}-{}", base, suffix);
        suffix += 1;
    }
    Ok(candidate)
}

// --- Axum extractors ---

/// Extractor that requires an authenticated user. Redirects to /auth/login if not authenticated.
pub struct AuthUser(pub User);

impl FromRequestParts<Arc<AppState>> for AuthUser {
    type Rejection = Response;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &Arc<AppState>,
    ) -> Result<Self, Self::Rejection> {
        let jar = CookieJar::from_headers(&parts.headers);
        let token = jar
            .get(SESSION_COOKIE)
            .map(|c| c.value().to_string());

        if let Some(token) = token {
            if let Some(user) = validate_session(&state.pool, &token).await {
                return Ok(AuthUser(user));
            }
        }

        Err(Redirect::to("/auth/login").into_response())
    }
}

/// Extractor that requires an admin user. Returns 403 if not admin.
pub struct AdminUser(pub User);

impl FromRequestParts<Arc<AppState>> for AdminUser {
    type Rejection = Response;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &Arc<AppState>,
    ) -> Result<Self, Self::Rejection> {
        let AuthUser(user) = AuthUser::from_request_parts(parts, state).await?;

        if user.role != "admin" {
            return Err((StatusCode::FORBIDDEN, "Admin access required").into_response());
        }

        Ok(AdminUser(user))
    }
}

// --- Axum auth handlers ---

use axum::routing::{get, post};
use axum::Router;
use serde::Deserialize;

pub fn auth_router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/auth/login", get(login_page).post(login_handler))
        .route("/auth/register", get(register_page).post(register_handler))
        .route("/auth/logout", post(logout_handler))
}

#[derive(Deserialize)]
pub struct LoginForm {
    pub email: String,
    pub password: String,
}

#[derive(Deserialize)]
pub struct RegisterForm {
    pub name: String,
    pub email: String,
    pub password: String,
}

async fn login_page(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let tmpl = match state.templates.get_template("auth/login.html") {
        Ok(t) => t,
        Err(e) => return Html(format!("Template error: {}", e)),
    };
    Html(tmpl.render(minijinja::context! { error => "" }).unwrap_or_default())
}

async fn login_handler(
    State(state): State<Arc<AppState>>,
    jar: CookieJar,
    Form(form): Form<LoginForm>,
) -> Response {
    let user = sqlx::query_as::<_, User>(
        "SELECT * FROM users WHERE email = ? AND auth_provider = 'local' AND enabled = 1",
    )
    .bind(&form.email)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    let user = match user {
        Some(u) => u,
        None => return render_login_error(&state, "Invalid email or password"),
    };

    let password_hash = match &user.password_hash {
        Some(h) => h,
        None => return render_login_error(&state, "Invalid email or password"),
    };

    if !verify_password(&form.password, password_hash) {
        return render_login_error(&state, "Invalid email or password");
    }

    let session = match create_session(&state.pool, &user.id).await {
        Ok(s) => s,
        Err(_) => return render_login_error(&state, "Internal error"),
    };

    let cookie = format!(
        "{}={}; HttpOnly; SameSite=Lax; Path=/; Max-Age={}",
        SESSION_COOKIE,
        session.id,
        SESSION_DURATION_DAYS * 86400
    );

    (
        jar,
        [("Set-Cookie", cookie)],
        Redirect::to("/dashboard"),
    )
        .into_response()
}

async fn register_page(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let auth_config = get_auth_config(&state.pool).await.unwrap_or(AuthConfig {
        id: "singleton".to_string(),
        registration_enabled: false,
        allowed_email_domains: None,
        created_at: String::new(),
        updated_at: String::new(),
    });

    if !auth_config.registration_enabled {
        return Html("Registration is disabled.".to_string());
    }

    let tmpl = match state.templates.get_template("auth/register.html") {
        Ok(t) => t,
        Err(e) => return Html(format!("Template error: {}", e)),
    };
    Html(
        tmpl.render(minijinja::context! {
            error => "",
            allowed_domains => auth_config.allowed_email_domains,
        })
        .unwrap_or_default(),
    )
}

async fn register_handler(
    State(state): State<Arc<AppState>>,
    jar: CookieJar,
    Form(form): Form<RegisterForm>,
) -> Response {
    let auth_config = match get_auth_config(&state.pool).await {
        Ok(c) => c,
        Err(_) => return Html("Internal error".to_string()).into_response(),
    };

    if !auth_config.registration_enabled {
        return Html("Registration is disabled.".to_string()).into_response();
    }

    // Validate email domain
    if !is_email_allowed(&form.email, &auth_config.allowed_email_domains) {
        return render_register_error(&state, "Email domain not allowed", &auth_config);
    }

    // Validate password length
    if form.password.len() < 8 {
        return render_register_error(
            &state,
            "Password must be at least 8 characters",
            &auth_config,
        );
    }

    // Check if email already taken
    let existing: Option<(String,)> = sqlx::query_as("SELECT id FROM users WHERE email = ?")
        .bind(&form.email)
        .fetch_optional(&state.pool)
        .await
        .unwrap_or(None);

    if existing.is_some() {
        return render_register_error(&state, "Email already registered", &auth_config);
    }

    let password_hash = match hash_password(&form.password) {
        Ok(h) => h,
        Err(_) => return render_register_error(&state, "Internal error", &auth_config),
    };

    // First user gets admin role
    let is_first = !has_any_users(&state.pool).await.unwrap_or(true);
    let role = if is_first { "admin" } else { "user" };

    let user_id = uuid::Uuid::new_v4().to_string();
    let username = match generate_username(&state.pool, &form.email).await {
        Ok(u) => u,
        Err(_) => return render_register_error(&state, "Internal error", &auth_config),
    };

    if let Err(_) = sqlx::query(
        "INSERT INTO users (id, email, name, timezone, password_hash, role, auth_provider, username) VALUES (?, ?, ?, 'UTC', ?, ?, 'local', ?)",
    )
    .bind(&user_id)
    .bind(&form.email)
    .bind(&form.name)
    .bind(&password_hash)
    .bind(role)
    .bind(&username)
    .execute(&state.pool)
    .await
    {
        return render_register_error(&state, "Failed to create account", &auth_config);
    }

    // Auto-create a scheduling account linked to this user
    let account_id = uuid::Uuid::new_v4().to_string();
    let _ = sqlx::query(
        "INSERT INTO accounts (id, name, email, timezone, user_id) VALUES (?, ?, ?, 'UTC', ?)",
    )
    .bind(&account_id)
    .bind(&form.name)
    .bind(&form.email)
    .bind(&user_id)
    .execute(&state.pool)
    .await;

    // Auto-login
    let session = match create_session(&state.pool, &user_id).await {
        Ok(s) => s,
        Err(_) => return Redirect::to("/auth/login").into_response(),
    };

    let cookie = format!(
        "{}={}; HttpOnly; SameSite=Lax; Path=/; Max-Age={}",
        SESSION_COOKIE,
        session.id,
        SESSION_DURATION_DAYS * 86400
    );

    (
        jar,
        [("Set-Cookie", cookie)],
        Redirect::to("/dashboard"),
    )
        .into_response()
}

async fn logout_handler(State(state): State<Arc<AppState>>, jar: CookieJar) -> Response {
    if let Some(cookie) = jar.get(SESSION_COOKIE) {
        let _ = delete_session(&state.pool, cookie.value()).await;
    }

    let clear_cookie = format!(
        "{}=; HttpOnly; SameSite=Lax; Path=/; Max-Age=0",
        SESSION_COOKIE
    );

    ([("Set-Cookie", clear_cookie)], Redirect::to("/auth/login")).into_response()
}

// --- Helpers ---

use axum::response::Html;

fn render_login_error(state: &AppState, error: &str) -> Response {
    let tmpl = match state.templates.get_template("auth/login.html") {
        Ok(t) => t,
        Err(_) => return Html(error.to_string()).into_response(),
    };
    Html(
        tmpl.render(minijinja::context! { error => error })
            .unwrap_or_else(|_| error.to_string()),
    )
    .into_response()
}

fn render_register_error(state: &AppState, error: &str, auth_config: &AuthConfig) -> Response {
    let tmpl = match state.templates.get_template("auth/register.html") {
        Ok(t) => t,
        Err(_) => return Html(error.to_string()).into_response(),
    };
    Html(
        tmpl.render(minijinja::context! {
            error => error,
            allowed_domains => auth_config.allowed_email_domains,
        })
        .unwrap_or_else(|_| error.to_string()),
    )
    .into_response()
}
