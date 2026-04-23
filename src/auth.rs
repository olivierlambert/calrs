use anyhow::{Context, Result};
use argon2::password_hash::rand_core::OsRng;
use argon2::password_hash::SaltString;
use argon2::{Argon2, PasswordHash, PasswordHasher, PasswordVerifier};
use axum::extract::{Form, FromRequestParts, State};
use axum::http::request::Parts;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{Html, IntoResponse, Redirect, Response};
use axum_extra::extract::CookieJar;
use base64::Engine;
use chrono::{Duration, Utc};
use rand::Rng;
use sqlx::SqlitePool;
use std::sync::Arc;

use crate::models::{AuthConfig, Session, User};
use crate::web::{csrf_cookie_value, generate_csrf_token, verify_csrf_token, AppState};

const SESSION_COOKIE: &str = "calrs_session";
const IMPERSONATE_COOKIE: &str = "calrs_impersonate";
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

pub async fn get_user_by_id(pool: &SqlitePool, user_id: &str) -> Option<User> {
    sqlx::query_as::<_, User>("SELECT * FROM users WHERE id = ? AND enabled = 1")
        .bind(user_id)
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
    let config =
        sqlx::query_as::<_, AuthConfig>("SELECT * FROM auth_config WHERE id = 'singleton'")
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
    let base = if base.is_empty() {
        "user".to_string()
    } else {
        base
    };

    let mut candidate = base.clone();
    let mut suffix = 1u32;
    loop {
        let taken: Option<(String,)> = sqlx::query_as("SELECT id FROM users WHERE username = ?")
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

/// Info about an active impersonation session.
#[derive(Clone)]
pub struct ImpersonationInfo {
    pub admin_name: String,
    pub target_name: String,
}

/// Extractor that requires an authenticated user. Redirects to /auth/login if not authenticated.
/// Supports admin impersonation: if the `calrs_impersonate` cookie is set and the real user is
/// an admin, returns the impersonated user instead.
pub struct AuthUser {
    pub user: User,
    pub impersonation: Option<ImpersonationInfo>,
}

impl FromRequestParts<Arc<AppState>> for AuthUser {
    type Rejection = Response;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &Arc<AppState>,
    ) -> Result<Self, Self::Rejection> {
        let jar = CookieJar::from_headers(&parts.headers);
        let token = jar.get(SESSION_COOKIE).map(|c| c.value().to_string());

        let real_user = match token {
            Some(ref token) => validate_session(&state.pool, token).await,
            None => None,
        };

        let real_user = match real_user {
            Some(u) => u,
            None => return Err(Redirect::to("/auth/login").into_response()),
        };

        // Check for impersonation
        if real_user.role == "admin" {
            if let Some(target_id) = jar.get(IMPERSONATE_COOKIE).map(|c| c.value().to_string()) {
                if target_id != real_user.id {
                    if let Some(target_user) = get_user_by_id(&state.pool, &target_id).await {
                        let info = ImpersonationInfo {
                            admin_name: real_user.name.clone(),
                            target_name: target_user.name.clone(),
                        };
                        return Ok(AuthUser {
                            user: target_user,
                            impersonation: Some(info),
                        });
                    }
                }
            }
        }

        Ok(AuthUser {
            user: real_user,
            impersonation: None,
        })
    }
}

/// Extractor that requires an admin user. Returns 403 if not admin.
/// Always uses the real session user, ignoring impersonation.
pub struct AdminUser(pub User);

impl FromRequestParts<Arc<AppState>> for AdminUser {
    type Rejection = Response;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &Arc<AppState>,
    ) -> Result<Self, Self::Rejection> {
        let jar = CookieJar::from_headers(&parts.headers);
        let token = jar.get(SESSION_COOKIE).map(|c| c.value().to_string());

        let real_user = match token {
            Some(ref token) => validate_session(&state.pool, token).await,
            None => None,
        };

        match real_user {
            Some(user) if user.role == "admin" => Ok(AdminUser(user)),
            Some(_) => Err((StatusCode::FORBIDDEN, "Admin access required").into_response()),
            None => Err(Redirect::to("/auth/login").into_response()),
        }
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
        .route("/auth/oidc/login", get(oidc_login))
        .route("/auth/oidc/callback", get(oidc_callback))
}

#[derive(Deserialize)]
struct CsrfForm {
    _csrf: Option<String>,
}

#[derive(Deserialize)]
pub struct LoginForm {
    pub _csrf: Option<String>,
    pub email: String,
    pub password: String,
}

#[derive(Deserialize)]
pub struct RegisterForm {
    pub _csrf: Option<String>,
    pub name: String,
    pub email: String,
    pub password: String,
}

async fn login_page(State(state): State<Arc<AppState>>, jar: CookieJar) -> Response {
    // If already authenticated, redirect to dashboard
    if let Some(token) = jar.get(SESSION_COOKIE).map(|c| c.value().to_string()) {
        if validate_session(&state.pool, &token).await.is_some() {
            return Redirect::to("/dashboard").into_response();
        }
    }

    let auth_config = get_auth_config(&state.pool).await.ok();
    let oidc_enabled = auth_config
        .as_ref()
        .map(|c| c.oidc_enabled)
        .unwrap_or(false);
    let registration_enabled = auth_config
        .as_ref()
        .map(|c| c.registration_enabled)
        .unwrap_or(true);

    let csrf_token = generate_csrf_token();

    let tmpl = match state.templates.get_template("auth/login.html") {
        Ok(t) => t,
        Err(e) => return Html(format!("Template error: {}", e)).into_response(),
    };
    let body = Html(
        tmpl.render(minijinja::context! { error => "", oidc_enabled => oidc_enabled, registration_enabled => registration_enabled, csrf_token => csrf_token })
            .unwrap_or_default(),
    );
    ([("Set-Cookie", csrf_cookie_value(&csrf_token))], body).into_response()
}

async fn login_handler(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    jar: CookieJar,
    Form(form): Form<LoginForm>,
) -> Response {
    if let Err(resp) = verify_csrf_token(&headers, &form._csrf) {
        return resp;
    }
    // Rate limit by IP (X-Forwarded-For from reverse proxy, or fallback to "unknown")
    let client_ip = headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.split(',').next())
        .unwrap_or("unknown")
        .trim()
        .to_string();

    if state.login_limiter.check_limited(&client_ip).await {
        tracing::warn!(ip = %client_ip, "rate limited");
        return render_login_error(&state, "Too many login attempts. Please try again later.");
    }

    let user = sqlx::query_as::<_, User>(
        "SELECT * FROM users WHERE email = ? AND auth_provider = 'local' AND enabled = 1",
    )
    .bind(&form.email)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    let user = match user {
        Some(u) => u,
        None => {
            tracing::warn!(email = %form.email, ip = %client_ip, "login failed");
            return render_login_error(&state, "Invalid email or password");
        }
    };

    let password_hash = match &user.password_hash {
        Some(h) => h,
        None => {
            tracing::warn!(email = %form.email, ip = %client_ip, "login failed");
            return render_login_error(&state, "Invalid email or password");
        }
    };

    if !verify_password(&form.password, password_hash) {
        tracing::warn!(email = %form.email, ip = %client_ip, "login failed");
        return render_login_error(&state, "Invalid email or password");
    }

    let session = match create_session(&state.pool, &user.id).await {
        Ok(s) => s,
        Err(_) => return render_login_error(&state, "Internal error"),
    };

    let cookie = format!(
        "{}={}; HttpOnly; Secure; SameSite=Lax; Path=/; Max-Age={}",
        SESSION_COOKIE,
        session.id,
        SESSION_DURATION_DAYS * 86400
    );

    tracing::info!(email = %form.email, ip = %client_ip, "user login");

    (jar, [("Set-Cookie", cookie)], Redirect::to("/dashboard")).into_response()
}

async fn register_page(State(state): State<Arc<AppState>>, jar: CookieJar) -> Response {
    // If already authenticated, redirect to dashboard
    if let Some(token) = jar.get(SESSION_COOKIE).map(|c| c.value().to_string()) {
        if validate_session(&state.pool, &token).await.is_some() {
            return Redirect::to("/dashboard").into_response();
        }
    }

    let auth_config = get_auth_config(&state.pool).await.unwrap_or(AuthConfig {
        id: "singleton".to_string(),
        registration_enabled: false,
        allowed_email_domains: None,
        created_at: String::new(),
        updated_at: String::new(),
        oidc_enabled: false,
        oidc_issuer_url: None,
        oidc_client_id: None,
        oidc_client_secret: None,
        oidc_auto_register: true,
    });

    if !auth_config.registration_enabled {
        return Html("Registration is disabled.".to_string()).into_response();
    }

    let csrf_token = generate_csrf_token();

    let tmpl = match state.templates.get_template("auth/register.html") {
        Ok(t) => t,
        Err(e) => return Html(format!("Template error: {}", e)).into_response(),
    };
    let body = Html(
        tmpl.render(minijinja::context! {
            error => "",
            allowed_domains => auth_config.allowed_email_domains,
            csrf_token => csrf_token,
        })
        .unwrap_or_default(),
    );
    ([("Set-Cookie", csrf_cookie_value(&csrf_token))], body).into_response()
}

async fn register_handler(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    jar: CookieJar,
    Form(form): Form<RegisterForm>,
) -> Response {
    if let Err(resp) = verify_csrf_token(&headers, &form._csrf) {
        return resp;
    }
    let auth_config = match get_auth_config(&state.pool).await {
        Ok(c) => c,
        Err(_) => return Html("Internal error".to_string()).into_response(),
    };

    if !auth_config.registration_enabled {
        return Html("Registration is disabled.".to_string()).into_response();
    }

    // Validate name
    let name = form.name.trim();
    if name.is_empty() || name.len() > 255 {
        return render_register_error(
            &state,
            "Name must be between 1 and 255 characters",
            &auth_config,
        );
    }

    // Validate email format
    let email = form.email.trim();
    if email.is_empty() || email.len() > 255 {
        return render_register_error(
            &state,
            "Email must be between 1 and 255 characters",
            &auth_config,
        );
    }
    if !email.contains('@')
        || email
            .rsplit('@')
            .next()
            .is_none_or(|domain| !domain.contains('.'))
    {
        return render_register_error(&state, "Please enter a valid email address", &auth_config);
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

    if sqlx::query(
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
    .is_err()
    {
        return render_register_error(&state, "Failed to create account", &auth_config);
    }

    // Link to existing account or create a new one
    let existing_account: Option<(String,)> =
        sqlx::query_as("SELECT id FROM accounts WHERE email = ?")
            .bind(&form.email)
            .fetch_optional(&state.pool)
            .await
            .unwrap_or(None);

    if let Some((account_id,)) = existing_account {
        let _ = sqlx::query("UPDATE accounts SET user_id = ?, name = ? WHERE id = ?")
            .bind(&user_id)
            .bind(&form.name)
            .bind(&account_id)
            .execute(&state.pool)
            .await;
    } else {
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
    }

    // Auto-login
    let session = match create_session(&state.pool, &user_id).await {
        Ok(s) => s,
        Err(_) => return Redirect::to("/auth/login").into_response(),
    };

    let cookie = format!(
        "{}={}; HttpOnly; Secure; SameSite=Lax; Path=/; Max-Age={}",
        SESSION_COOKIE,
        session.id,
        SESSION_DURATION_DAYS * 86400
    );

    tracing::info!(email = %form.email, "user registered");

    (jar, [("Set-Cookie", cookie)], Redirect::to("/dashboard")).into_response()
}

async fn logout_handler(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    jar: CookieJar,
    Form(csrf): Form<CsrfForm>,
) -> Response {
    if let Err(resp) = verify_csrf_token(&headers, &csrf._csrf) {
        return resp;
    }
    if let Some(cookie) = jar.get(SESSION_COOKIE) {
        let _ = delete_session(&state.pool, cookie.value()).await;
    }

    tracing::info!("user logout");

    let clear_cookie = format!(
        "{}=; HttpOnly; Secure; SameSite=Lax; Path=/; Max-Age=0",
        SESSION_COOKIE
    );

    ([("Set-Cookie", clear_cookie)], Redirect::to("/auth/login")).into_response()
}

// --- OIDC ---

const OIDC_STATE_COOKIE: &str = "calrs_oidc_state";
const OIDC_NONCE_COOKIE: &str = "calrs_oidc_nonce";
const OIDC_PKCE_COOKIE: &str = "calrs_oidc_pkce";

use axum::extract::Query;
use openidconnect::core::{CoreClient, CoreProviderMetadata, CoreResponseType};
use openidconnect::{
    AuthenticationFlow, AuthorizationCode, ClientId, ClientSecret, CsrfToken, EndpointMaybeSet,
    EndpointNotSet, EndpointSet, IssuerUrl, Nonce, PkceCodeChallenge, PkceCodeVerifier,
    RedirectUrl, Scope, TokenResponse,
};

fn build_http_client() -> Result<openidconnect::reqwest::Client> {
    let client = openidconnect::reqwest::ClientBuilder::new()
        .redirect(openidconnect::reqwest::redirect::Policy::none())
        .build()
        .map_err(|e| anyhow::anyhow!("Failed to build HTTP client: {}", e))?;
    Ok(client)
}

async fn build_oidc_client_with_redirect(
    auth_config: &AuthConfig,
) -> Result<
    CoreClient<
        EndpointSet,
        EndpointNotSet,
        EndpointNotSet,
        EndpointNotSet,
        EndpointMaybeSet,
        EndpointMaybeSet,
    >,
> {
    let issuer_url = IssuerUrl::new(
        auth_config
            .oidc_issuer_url
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("OIDC issuer URL not configured"))?
            .clone(),
    )
    .map_err(|e| anyhow::anyhow!("Invalid issuer URL: {}", e))?;

    let client_id = ClientId::new(
        auth_config
            .oidc_client_id
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("OIDC client ID not configured"))?
            .clone(),
    );

    let client_secret = auth_config
        .oidc_client_secret
        .as_ref()
        .map(|s| ClientSecret::new(s.clone()));

    let http_client = build_http_client()?;
    let provider_metadata = CoreProviderMetadata::discover_async(issuer_url, &http_client)
        .await
        .map_err(|e| anyhow::anyhow!("OIDC discovery failed: {}", e))?;

    let redirect_url = RedirectUrl::new(format!(
        "{}/auth/oidc/callback",
        std::env::var("CALRS_BASE_URL").unwrap_or_else(|_| "http://localhost:3000".to_string())
    ))
    .map_err(|e| anyhow::anyhow!("Invalid redirect URL: {}", e))?;

    let client = CoreClient::from_provider_metadata(provider_metadata, client_id, client_secret)
        .set_redirect_uri(redirect_url);

    Ok(client)
}

async fn oidc_login(State(state): State<Arc<AppState>>) -> Response {
    let auth_config = match get_auth_config(&state.pool).await {
        Ok(c) if c.oidc_enabled => c,
        _ => return Html("OIDC is not enabled.".to_string()).into_response(),
    };

    let client = match build_oidc_client_with_redirect(&auth_config).await {
        Ok(c) => c,
        Err(e) => return Html(format!("OIDC configuration error: {}", e)).into_response(),
    };

    let (pkce_challenge, pkce_verifier) = PkceCodeChallenge::new_random_sha256();

    let (auth_url, csrf_token, nonce) = client
        .authorize_url(
            AuthenticationFlow::<CoreResponseType>::AuthorizationCode,
            CsrfToken::new_random,
            Nonce::new_random,
        )
        .add_scope(Scope::new("openid".to_string()))
        .add_scope(Scope::new("email".to_string()))
        .add_scope(Scope::new("profile".to_string()))
        .set_pkce_challenge(pkce_challenge)
        .url();

    // Store state, nonce, and PKCE verifier in short-lived cookies
    let cookie_opts = "; HttpOnly; Secure; SameSite=Lax; Path=/; Max-Age=600";
    let state_cookie = format!(
        "{}={}{}",
        OIDC_STATE_COOKIE,
        csrf_token.secret(),
        cookie_opts
    );
    let nonce_cookie = format!("{}={}{}", OIDC_NONCE_COOKIE, nonce.secret(), cookie_opts);
    let pkce_cookie = format!(
        "{}={}{}",
        OIDC_PKCE_COOKIE,
        pkce_verifier.secret(),
        cookie_opts
    );

    let mut headers = axum::http::HeaderMap::new();
    headers.append(
        axum::http::header::SET_COOKIE,
        state_cookie.parse().unwrap(),
    );
    headers.append(
        axum::http::header::SET_COOKIE,
        nonce_cookie.parse().unwrap(),
    );
    headers.append(axum::http::header::SET_COOKIE, pkce_cookie.parse().unwrap());

    (headers, Redirect::to(auth_url.as_str())).into_response()
}

#[derive(Deserialize)]
struct OidcCallbackQuery {
    code: String,
    state: String,
}

async fn oidc_callback(
    State(state): State<Arc<AppState>>,
    jar: CookieJar,
    Query(query): Query<OidcCallbackQuery>,
) -> Response {
    let auth_config = match get_auth_config(&state.pool).await {
        Ok(c) if c.oidc_enabled => c,
        _ => return Html("OIDC is not enabled.".to_string()).into_response(),
    };

    // Verify CSRF state
    let stored_state = match jar.get(OIDC_STATE_COOKIE) {
        Some(c) => c.value().to_string(),
        None => return Html("Missing OIDC state. Please try again.".to_string()).into_response(),
    };
    if query.state != stored_state {
        tracing::warn!("OIDC callback failed: state mismatch");
        return Html("Invalid OIDC state. Possible CSRF attack.".to_string()).into_response();
    }

    let stored_nonce = match jar.get(OIDC_NONCE_COOKIE) {
        Some(c) => c.value().to_string(),
        None => return Html("Missing OIDC nonce. Please try again.".to_string()).into_response(),
    };

    let pkce_verifier_secret = match jar.get(OIDC_PKCE_COOKIE) {
        Some(c) => c.value().to_string(),
        None => {
            return Html("Missing PKCE verifier. Please try again.".to_string()).into_response()
        }
    };

    let client = match build_oidc_client_with_redirect(&auth_config).await {
        Ok(c) => c,
        Err(e) => return Html(format!("OIDC error: {}", e)).into_response(),
    };

    let http_client = match build_http_client() {
        Ok(c) => c,
        Err(e) => return Html(format!("HTTP client error: {}", e)).into_response(),
    };

    // Exchange code for tokens
    let token_request = match client.exchange_code(AuthorizationCode::new(query.code)) {
        Ok(r) => r,
        Err(e) => return Html(format!("OIDC configuration error: {}", e)).into_response(),
    };
    let token_response = match token_request
        .set_pkce_verifier(PkceCodeVerifier::new(pkce_verifier_secret))
        .request_async(&http_client)
        .await
    {
        Ok(t) => t,
        Err(e) => return Html(format!("Token exchange failed: {}", e)).into_response(),
    };

    // Verify and extract ID token claims
    let id_token = match token_response.id_token() {
        Some(t) => t,
        None => return Html("No ID token in response.".to_string()).into_response(),
    };

    let verifier = client.id_token_verifier();
    let claims = match id_token.claims(&verifier, &Nonce::new(stored_nonce)) {
        Ok(c) => c,
        Err(e) => return Html(format!("ID token verification failed: {}", e)).into_response(),
    };

    let subject = claims.subject().to_string();
    let email = claims
        .email()
        .map(|e: &openidconnect::EndUserEmail| e.to_string())
        .unwrap_or_default();
    // `email_verified` is optional in the spec. If the IdP doesn't assert
    // verification, we treat it as unverified (conservative).
    let email_verified = claims.email_verified().unwrap_or(false);
    let name = claims
        .name()
        .and_then(
            |n: &openidconnect::LocalizedClaim<openidconnect::EndUserName>| {
                n.get(None)
                    .map(|v: &openidconnect::EndUserName| v.to_string())
            },
        )
        .unwrap_or_else(|| email.split('@').next().unwrap_or("User").to_string());

    if email.is_empty() {
        tracing::warn!("OIDC callback failed: no email in token");
        return Html("OIDC provider did not return an email address.".to_string()).into_response();
    }

    // Check email domain restriction
    if !is_email_allowed(&email, &auth_config.allowed_email_domains) {
        tracing::warn!(email = %email, "OIDC callback failed: email domain not allowed");
        return Html("Your email domain is not allowed.".to_string()).into_response();
    }

    // Extract groups and title from ID token JWT payload
    let claims = extract_claims_from_id_token(id_token.to_string().as_str());

    // Find or create user
    let user_id = match find_or_create_oidc_user(
        &state.pool,
        &subject,
        &email,
        email_verified,
        &name,
        &auth_config,
    )
    .await
    {
        Ok(id) => id,
        Err(e) => {
            tracing::warn!(email = %email, error = %e, "OIDC callback failed: account error");
            return Html(format!("Account error: {}", e)).into_response();
        }
    };

    // Sync title from OIDC token (best-effort)
    if let Some(ref title) = claims.title {
        let _ =
            sqlx::query("UPDATE users SET title = ?, updated_at = datetime('now') WHERE id = ?")
                .bind(title)
                .bind(&user_id)
                .execute(&state.pool)
                .await;
    }

    // Sync groups from OIDC token (best-effort, don't fail login)
    if let Some(groups) = &claims.groups {
        if let Err(e) = sync_user_groups(&state.pool, &user_id, groups).await {
            tracing::warn!(error = %e, "failed to sync OIDC groups");
        }
    }

    // Create session
    let session = match create_session(&state.pool, &user_id).await {
        Ok(s) => s,
        Err(_) => return Html("Failed to create session.".to_string()).into_response(),
    };

    tracing::info!(email = %email, provider = "oidc", "user login via OIDC");

    let session_cookie = format!(
        "{}={}; HttpOnly; Secure; SameSite=Lax; Path=/; Max-Age={}",
        SESSION_COOKIE,
        session.id,
        SESSION_DURATION_DAYS * 86400
    );

    // Clear OIDC transient cookies
    let clear = "; HttpOnly; Secure; SameSite=Lax; Path=/; Max-Age=0";
    let clear_state = format!("{OIDC_STATE_COOKIE}={clear}");
    let clear_nonce = format!("{OIDC_NONCE_COOKIE}={clear}");
    let clear_pkce = format!("{OIDC_PKCE_COOKIE}={clear}");

    let mut headers = axum::http::HeaderMap::new();
    headers.append(
        axum::http::header::SET_COOKIE,
        session_cookie.parse().unwrap(),
    );
    headers.append(axum::http::header::SET_COOKIE, clear_state.parse().unwrap());
    headers.append(axum::http::header::SET_COOKIE, clear_nonce.parse().unwrap());
    headers.append(axum::http::header::SET_COOKIE, clear_pkce.parse().unwrap());

    (headers, Redirect::to("/dashboard")).into_response()
}

/// Find an existing user by OIDC subject or email, or create a new one.
///
/// `email_verified` is the `email_verified` claim from the ID token. It gates
/// the two branches that trust the email at face value:
///
/// - **Email-based linking** (step 2) — attaches a new `oidc_subject` to an
///   existing local user matching the email. If `email_verified` is false,
///   an attacker who can register at the IdP with any email they want would
///   silently hijack the matching local account on first OIDC login.
/// - **Auto-register** (step 3) — creates a brand-new user keyed on the
///   email. Same concern: unverified emails could be used to squat on
///   addresses the attacker doesn't actually own.
///
/// Step 1 (match by `oidc_subject`) is always allowed — the subject claim is
/// the IdP-issued stable identifier and doesn't rely on the email being
/// accurate, so a returning user can always sign in once their account
/// exists.
async fn find_or_create_oidc_user(
    pool: &SqlitePool,
    subject: &str,
    email: &str,
    email_verified: bool,
    name: &str,
    auth_config: &AuthConfig,
) -> Result<String> {
    // 1. Try to find by oidc_subject — always allowed, subject is the stable
    //    IdP-issued identity. Email is only a display attribute at this point.
    if let Some((id,)) = sqlx::query_as::<_, (String,)>(
        "SELECT id FROM users WHERE oidc_subject = ? AND auth_provider = 'oidc' AND enabled = 1",
    )
    .bind(subject)
    .fetch_optional(pool)
    .await?
    {
        // Update email in case it changed on the IdP; only update name if currently empty
        sqlx::query(
            "UPDATE users SET name = CASE WHEN name IS NULL OR name = '' THEN ? ELSE name END, email = ?, updated_at = datetime('now') WHERE id = ?",
        )
        .bind(name)
        .bind(email)
        .bind(&id)
        .execute(pool)
        .await?;
        return Ok(id);
    }

    // 2. Link to existing local user by email — only if the IdP asserts the
    //    email is verified. Otherwise an attacker could register at the IdP
    //    with the target's email and silently take over their local account.
    if let Some((id, _existing_provider)) = sqlx::query_as::<_, (String, String)>(
        "SELECT id, auth_provider FROM users WHERE email = ? AND enabled = 1",
    )
    .bind(email)
    .fetch_optional(pool)
    .await?
    {
        if !email_verified {
            tracing::warn!(
                email = %email, subject = %subject,
                "OIDC login refused: email matches existing local user but IdP did not assert email_verified=true"
            );
            anyhow::bail!(
                "An account with this email already exists. The identity provider has not verified that you own this address, so we cannot link them automatically. Contact an administrator."
            );
        }
        sqlx::query(
            "UPDATE users SET oidc_subject = ?, auth_provider = 'oidc', updated_at = datetime('now') WHERE id = ?",
        )
        .bind(subject)
        .bind(&id)
        .execute(pool)
        .await?;
        return Ok(id);
    }

    // 3. Create new user if auto-register is enabled — also gated on
    //    email_verified. Otherwise an attacker could create accounts keyed on
    //    addresses they don't own (squatting) or pollute the directory.
    if !auth_config.oidc_auto_register {
        anyhow::bail!("No account found for this email. Contact an administrator.");
    }
    if !email_verified {
        tracing::warn!(
            email = %email, subject = %subject,
            "OIDC auto-register refused: IdP did not assert email_verified=true"
        );
        anyhow::bail!(
            "The identity provider has not verified your email address. Ask your administrator to verify it at the provider and try again."
        );
    }

    let is_first = !has_any_users(pool).await?;
    let role = if is_first { "admin" } else { "user" };
    let user_id = uuid::Uuid::new_v4().to_string();
    let username = generate_username(pool, email).await?;

    sqlx::query(
        "INSERT INTO users (id, email, name, timezone, role, auth_provider, oidc_subject, username) VALUES (?, ?, ?, 'UTC', ?, 'oidc', ?, ?)",
    )
    .bind(&user_id)
    .bind(email)
    .bind(name)
    .bind(role)
    .bind(subject)
    .bind(&username)
    .execute(pool)
    .await?;

    // Link to existing account or create a new one
    let existing_account: Option<(String,)> =
        sqlx::query_as("SELECT id FROM accounts WHERE email = ?")
            .bind(email)
            .fetch_optional(pool)
            .await?;

    if let Some((account_id,)) = existing_account {
        sqlx::query("UPDATE accounts SET user_id = ?, name = ? WHERE id = ?")
            .bind(&user_id)
            .bind(name)
            .bind(&account_id)
            .execute(pool)
            .await?;
    } else {
        let account_id = uuid::Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO accounts (id, name, email, timezone, user_id) VALUES (?, ?, ?, 'UTC', ?)",
        )
        .bind(&account_id)
        .bind(name)
        .bind(email)
        .bind(&user_id)
        .execute(pool)
        .await?;
    }

    Ok(user_id)
}

// --- Helpers ---

fn render_login_error(state: &AppState, error: &str) -> Response {
    // Best-effort: try to show OIDC button even on error page
    let oidc_enabled = false; // Can't async here easily; login errors are local-auth only anyway
    let csrf_token = generate_csrf_token();
    let tmpl = match state.templates.get_template("auth/login.html") {
        Ok(t) => t,
        Err(_) => return Html(error.to_string()).into_response(),
    };
    let body = Html(
        tmpl.render(minijinja::context! { error => error, oidc_enabled => oidc_enabled, csrf_token => csrf_token })
            .unwrap_or_else(|_| error.to_string()),
    );
    ([("Set-Cookie", csrf_cookie_value(&csrf_token))], body).into_response()
}

fn render_register_error(state: &AppState, error: &str, auth_config: &AuthConfig) -> Response {
    let csrf_token = generate_csrf_token();
    let tmpl = match state.templates.get_template("auth/register.html") {
        Ok(t) => t,
        Err(_) => return Html(error.to_string()).into_response(),
    };
    let body = Html(
        tmpl.render(minijinja::context! {
            error => error,
            allowed_domains => auth_config.allowed_email_domains,
            csrf_token => csrf_token,
        })
        .unwrap_or_else(|_| error.to_string()),
    );
    ([("Set-Cookie", csrf_cookie_value(&csrf_token))], body).into_response()
}

// --- OIDC group sync ---

/// Extract the `groups` claim from a raw JWT ID token.
/// Decodes the payload (middle part) as base64 and parses JSON.
/// Returns None if the token has no groups claim or cannot be parsed.
/// Parsed claims from an OIDC ID token JWT payload.
struct OidcClaims {
    groups: Option<Vec<String>>,
    title: Option<String>,
}

fn extract_claims_from_id_token(raw_token: &str) -> OidcClaims {
    let parts: Vec<&str> = raw_token.split('.').collect();
    if parts.len() != 3 {
        return OidcClaims {
            groups: None,
            title: None,
        };
    }
    let payload_bytes = match base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(parts[1]) {
        Ok(b) => b,
        Err(_) => {
            return OidcClaims {
                groups: None,
                title: None,
            }
        }
    };
    let payload: serde_json::Value = match serde_json::from_slice(&payload_bytes) {
        Ok(v) => v,
        Err(_) => {
            return OidcClaims {
                groups: None,
                title: None,
            }
        }
    };

    let groups = payload.get("groups").and_then(|g| {
        let arr = g.as_array()?;
        let group_strings: Vec<String> = arr
            .iter()
            .filter_map(|v| {
                v.as_str()
                    .map(|s| s.strip_prefix('/').unwrap_or(s).to_string())
            })
            .collect();
        if group_strings.is_empty() {
            None
        } else {
            Some(group_strings)
        }
    });

    let title = payload
        .get("title")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    OidcClaims { groups, title }
}

/// Backward-compatible wrapper for tests.
fn extract_groups_from_id_token(raw_token: &str) -> Option<Vec<String>> {
    extract_claims_from_id_token(raw_token).groups
}

/// Sync a user's group memberships from OIDC groups claim.
/// Creates any missing groups and replaces the user's memberships.
pub async fn sync_user_groups(pool: &SqlitePool, user_id: &str, groups: &[String]) -> Result<()> {
    // Delete existing OIDC group memberships for this user
    sqlx::query(
        "DELETE FROM user_groups WHERE user_id = ? AND group_id IN (SELECT id FROM groups WHERE source = 'oidc')",
    )
    .bind(user_id)
    .execute(pool)
    .await
    .context("Failed to clear user OIDC groups")?;

    // Collect the group IDs the user currently belongs to (for team sync)
    let mut current_group_ids: Vec<String> = Vec::new();

    for group_path in groups {
        let group_id = uuid::Uuid::new_v4().to_string();
        let slug = generate_group_slug(group_path);

        // Upsert group: insert if not exists (keyed on name + source=oidc)
        sqlx::query(
            "INSERT INTO groups (id, name, source, oidc_id, slug, created_at) \
             VALUES (?, ?, 'oidc', ?, ?, datetime('now')) \
             ON CONFLICT(name) DO UPDATE SET oidc_id = excluded.oidc_id, slug = excluded.slug",
        )
        .bind(&group_id)
        .bind(group_path)
        .bind(group_path)
        .bind(&slug)
        .execute(pool)
        .await
        .context("Failed to upsert group")?;

        // Get the actual group ID (may differ if it already existed)
        let (actual_group_id,): (String,) = sqlx::query_as("SELECT id FROM groups WHERE name = ?")
            .bind(group_path)
            .fetch_one(pool)
            .await
            .context("Failed to fetch group id")?;

        // Insert membership
        sqlx::query("INSERT OR IGNORE INTO user_groups (user_id, group_id) VALUES (?, ?)")
            .bind(user_id)
            .bind(&actual_group_id)
            .execute(pool)
            .await
            .context("Failed to insert user_group")?;

        current_group_ids.push(actual_group_id);
    }

    // --- Sync team memberships via team_groups ---

    // Add user to teams linked to their current OIDC groups
    for group_id in &current_group_ids {
        let team_ids: Vec<(String,)> =
            sqlx::query_as("SELECT team_id FROM team_groups WHERE group_id = ?")
                .bind(group_id)
                .fetch_all(pool)
                .await
                .context("Failed to fetch team_groups")?;

        for (team_id,) in team_ids {
            sqlx::query(
                "INSERT OR IGNORE INTO team_members (team_id, user_id, role, source) VALUES (?, ?, 'member', 'group')",
            )
            .bind(&team_id)
            .bind(user_id)
            .execute(pool)
            .await
            .context("Failed to insert team_member from group")?;
        }
    }

    // Remove user from teams where they were added via group sync but no longer belong
    // to any linked OIDC group. Only remove source='group' rows (not 'direct').
    sqlx::query(
        "DELETE FROM team_members WHERE user_id = ? AND source = 'group' \
         AND team_id NOT IN ( \
             SELECT tg.team_id FROM team_groups tg \
             WHERE tg.group_id IN (SELECT group_id FROM user_groups WHERE user_id = ?) \
         )",
    )
    .bind(user_id)
    .bind(user_id)
    .execute(pool)
    .await
    .context("Failed to clean up stale team memberships")?;

    Ok(())
}

/// Generate a URL-friendly slug from a group name.
/// "Demo Team" -> "demo-team", "engineering/backend" -> "engineering-backend"
pub fn generate_group_slug(name: &str) -> String {
    let slug: String = name
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect();
    // Collapse multiple dashes and trim leading/trailing dashes
    let mut result = String::new();
    let mut prev_dash = true; // start true to skip leading dashes
    for c in slug.chars() {
        if c == '-' {
            if !prev_dash {
                result.push('-');
            }
            prev_dash = true;
        } else {
            result.push(c);
            prev_dash = false;
        }
    }
    // Trim trailing dash
    if result.ends_with('-') {
        result.pop();
    }
    if result.is_empty() {
        "group".to_string()
    } else {
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- is_email_allowed ---

    #[test]
    fn email_allowed_no_restriction() {
        assert!(is_email_allowed("alice@anything.com", &None));
        assert!(is_email_allowed(
            "alice@anything.com",
            &Some("".to_string())
        ));
        assert!(is_email_allowed(
            "alice@anything.com",
            &Some("  ".to_string())
        ));
    }

    #[test]
    fn email_allowed_single_domain() {
        let domains = Some("example.com".to_string());
        assert!(is_email_allowed("alice@example.com", &domains));
        assert!(!is_email_allowed("alice@other.com", &domains));
    }

    #[test]
    fn email_allowed_multiple_domains() {
        let domains = Some("example.com, company.org".to_string());
        assert!(is_email_allowed("alice@example.com", &domains));
        assert!(is_email_allowed("bob@company.org", &domains));
        assert!(!is_email_allowed("eve@evil.com", &domains));
    }

    #[test]
    fn email_allowed_case_insensitive() {
        let domains = Some("Example.COM".to_string());
        assert!(is_email_allowed("alice@example.com", &domains));
        assert!(is_email_allowed("alice@EXAMPLE.COM", &domains));
    }

    #[test]
    fn email_allowed_no_at_sign() {
        let domains = Some("example.com".to_string());
        assert!(!is_email_allowed("invalid-email", &domains));
    }

    #[test]
    fn email_allowed_subdomain_not_matched() {
        let domains = Some("example.com".to_string());
        assert!(!is_email_allowed("alice@sub.example.com", &domains));
    }

    // --- generate_group_slug ---

    #[test]
    fn slug_basic() {
        assert_eq!(generate_group_slug("Demo Team"), "demo-team");
    }

    #[test]
    fn slug_with_slashes() {
        assert_eq!(
            generate_group_slug("engineering/backend"),
            "engineering-backend"
        );
    }

    #[test]
    fn slug_collapses_dashes() {
        assert_eq!(generate_group_slug("a - - b"), "a-b");
    }

    #[test]
    fn slug_trims_leading_trailing() {
        assert_eq!(generate_group_slug(" -hello- "), "hello");
    }

    #[test]
    fn slug_special_chars() {
        // Unicode alphanumeric chars are kept, non-alphanumeric become dashes
        assert_eq!(generate_group_slug("café & más"), "café-más");
        assert_eq!(generate_group_slug("test!@#$%"), "test");
    }

    #[test]
    fn slug_empty_returns_group() {
        assert_eq!(generate_group_slug(""), "group");
        assert_eq!(generate_group_slug("---"), "group");
        assert_eq!(generate_group_slug("///"), "group");
    }

    #[test]
    fn slug_numeric() {
        assert_eq!(generate_group_slug("team42"), "team42");
    }

    // --- extract_groups_from_id_token ---

    #[test]
    fn extract_groups_valid_token() {
        let header = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(r#"{"alg":"RS256"}"#);
        let payload = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(r#"{"sub":"user1","groups":["engineering","/admins","devops"]}"#);
        let token = format!("{}.{}.fake-sig", header, payload);

        let groups = extract_groups_from_id_token(&token);
        assert_eq!(
            groups,
            Some(vec![
                "engineering".to_string(),
                "admins".to_string(),
                "devops".to_string(),
            ])
        );
    }

    #[test]
    fn extract_groups_no_groups_claim() {
        let header = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(r#"{"alg":"RS256"}"#);
        let payload = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(r#"{"sub":"user1","email":"alice@test.com"}"#);
        let token = format!("{}.{}.fake-sig", header, payload);

        assert_eq!(extract_groups_from_id_token(&token), None);
    }

    #[test]
    fn extract_groups_empty_groups() {
        let header = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(r#"{"alg":"RS256"}"#);
        let payload = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(r#"{"sub":"user1","groups":[]}"#);
        let token = format!("{}.{}.fake-sig", header, payload);

        assert_eq!(extract_groups_from_id_token(&token), None);
    }

    #[test]
    fn extract_groups_invalid_token() {
        assert_eq!(extract_groups_from_id_token("not-a-jwt"), None);
        assert_eq!(extract_groups_from_id_token("a.b"), None);
        assert_eq!(extract_groups_from_id_token(""), None);
    }

    // --- hash_password / verify_password ---

    #[test]
    fn password_hash_roundtrip() {
        let password = "SecureP@ss123";
        let hash = hash_password(password).unwrap();
        assert!(verify_password(password, &hash));
        assert!(!verify_password("wrong-password", &hash));
    }

    #[test]
    fn verify_password_invalid_hash() {
        assert!(!verify_password("anything", "not-a-valid-hash"));
        assert!(!verify_password("anything", ""));
    }

    #[test]
    fn password_hashes_are_unique() {
        let h1 = hash_password("same-password").unwrap();
        let h2 = hash_password("same-password").unwrap();
        assert_ne!(h1, h2); // different salts
        assert!(verify_password("same-password", &h1));
        assert!(verify_password("same-password", &h2));
    }

    #[test]
    fn hash_password_empty_string() {
        let hash = hash_password("").unwrap();
        assert!(verify_password("", &hash));
        assert!(!verify_password(" ", &hash));
    }

    #[test]
    fn hash_password_unicode() {
        let password = "pässwörd-日本語-🔑";
        let hash = hash_password(password).unwrap();
        assert!(verify_password(password, &hash));
        assert!(!verify_password("pässwörd-日本語", &hash));
    }

    #[test]
    fn hash_password_long_input() {
        let password = "a".repeat(1000);
        let hash = hash_password(&password).unwrap();
        assert!(verify_password(&password, &hash));
    }

    // --- extract_claims_from_id_token (title) ---

    #[test]
    fn extract_claims_with_title() {
        let header = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(r#"{"alg":"RS256"}"#);
        let payload = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(r#"{"sub":"user1","title":"Senior Engineer","groups":["team-a"]}"#);
        let token = format!("{}.{}.fake-sig", header, payload);

        let claims = extract_claims_from_id_token(&token);
        assert_eq!(claims.title, Some("Senior Engineer".to_string()));
        assert_eq!(claims.groups, Some(vec!["team-a".to_string()]));
    }

    #[test]
    fn extract_claims_no_title() {
        let header = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(r#"{"alg":"RS256"}"#);
        let payload = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(r#"{"sub":"user1","groups":["eng"]}"#);
        let token = format!("{}.{}.fake-sig", header, payload);

        let claims = extract_claims_from_id_token(&token);
        assert_eq!(claims.title, None);
        assert!(claims.groups.is_some());
    }

    #[test]
    fn extract_claims_title_only() {
        let header = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(r#"{"alg":"RS256"}"#);
        let payload = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(r#"{"sub":"user1","title":"CTO"}"#);
        let token = format!("{}.{}.fake-sig", header, payload);

        let claims = extract_claims_from_id_token(&token);
        assert_eq!(claims.title, Some("CTO".to_string()));
        assert_eq!(claims.groups, None);
    }

    // ===== DB-backed integration tests =====

    async fn setup_db() -> SqlitePool {
        use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
        use std::str::FromStr;
        let pool = SqlitePoolOptions::new()
            .max_connections(2)
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

    /// Helper: insert a user directly and return their id.
    async fn insert_user(pool: &SqlitePool, email: &str, name: &str, role: &str) -> String {
        let id = uuid::Uuid::new_v4().to_string();
        let hash = hash_password("testpass123").unwrap();
        let username = generate_username(pool, email).await.unwrap();
        sqlx::query(
            "INSERT INTO users (id, email, name, timezone, password_hash, role, auth_provider, username)
             VALUES (?, ?, ?, 'UTC', ?, ?, 'local', ?)",
        )
        .bind(&id)
        .bind(email)
        .bind(name)
        .bind(&hash)
        .bind(role)
        .bind(&username)
        .execute(pool)
        .await
        .unwrap();
        id
    }

    // --- generate_username ---

    #[tokio::test]
    async fn generate_username_from_email() {
        let pool = setup_db().await;
        let username = generate_username(&pool, "alice@example.com").await.unwrap();
        assert_eq!(username, "alice");
    }

    #[tokio::test]
    async fn generate_username_dots_become_dashes() {
        let pool = setup_db().await;
        let username = generate_username(&pool, "alice.smith@example.com")
            .await
            .unwrap();
        assert_eq!(username, "alice-smith");
    }

    #[tokio::test]
    async fn generate_username_strips_special_chars() {
        let pool = setup_db().await;
        let username = generate_username(&pool, "al!ce+tag@example.com")
            .await
            .unwrap();
        assert_eq!(username, "alcetag");
    }

    #[tokio::test]
    async fn generate_username_uppercase_lowered() {
        let pool = setup_db().await;
        let username = generate_username(&pool, "Alice.BOB@example.com")
            .await
            .unwrap();
        assert_eq!(username, "alice-bob");
    }

    #[tokio::test]
    async fn generate_username_empty_local_part_fallback() {
        let pool = setup_db().await;
        let username = generate_username(&pool, "@example.com").await.unwrap();
        assert_eq!(username, "user");
    }

    #[tokio::test]
    async fn generate_username_no_at_sign() {
        let pool = setup_db().await;
        let username = generate_username(&pool, "justname").await.unwrap();
        assert_eq!(username, "justname");
    }

    #[tokio::test]
    async fn generate_username_collision_appends_suffix() {
        let pool = setup_db().await;
        // First user takes "alice"
        insert_user(&pool, "alice@one.com", "Alice One", "user").await;
        // Second user with same local part should get "alice-1"
        let username = generate_username(&pool, "alice@two.com").await.unwrap();
        assert_eq!(username, "alice-1");
    }

    #[tokio::test]
    async fn generate_username_multiple_collisions() {
        let pool = setup_db().await;
        // Take "bob", "bob-1", "bob-2"
        insert_user(&pool, "bob@one.com", "Bob One", "user").await;
        // Manually insert bob-1 and bob-2
        let id2 = uuid::Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO users (id, email, name, timezone, role, auth_provider, username)
             VALUES (?, 'bob2@x.com', 'Bob2', 'UTC', 'user', 'local', 'bob-1')",
        )
        .bind(&id2)
        .execute(&pool)
        .await
        .unwrap();
        let id3 = uuid::Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO users (id, email, name, timezone, role, auth_provider, username)
             VALUES (?, 'bob3@x.com', 'Bob3', 'UTC', 'user', 'local', 'bob-2')",
        )
        .bind(&id3)
        .execute(&pool)
        .await
        .unwrap();

        let username = generate_username(&pool, "bob@new.com").await.unwrap();
        assert_eq!(username, "bob-3");
    }

    // --- create_session / validate_session ---

    #[tokio::test]
    async fn create_and_validate_session() {
        let pool = setup_db().await;
        let user_id = insert_user(&pool, "sess@test.com", "Session User", "user").await;

        let session = create_session(&pool, &user_id).await.unwrap();
        assert!(!session.id.is_empty());
        assert_eq!(session.user_id, user_id);

        // Validate returns the user
        let user = validate_session(&pool, &session.id).await;
        assert!(user.is_some());
        let user = user.unwrap();
        assert_eq!(user.id, user_id);
        assert_eq!(user.email, "sess@test.com");
    }

    #[tokio::test]
    async fn validate_session_invalid_token() {
        let pool = setup_db().await;
        let user = validate_session(&pool, "nonexistent-token").await;
        assert!(user.is_none());
    }

    #[tokio::test]
    async fn validate_session_expired() {
        let pool = setup_db().await;
        let user_id = insert_user(&pool, "expired@test.com", "Expired User", "user").await;

        // Manually insert an expired session
        let token = "expired-session-token";
        sqlx::query("INSERT INTO sessions (id, user_id, expires_at) VALUES (?, ?, ?)")
            .bind(token)
            .bind(&user_id)
            .bind("2020-01-01T00:00:00") // far in the past
            .execute(&pool)
            .await
            .unwrap();

        let user = validate_session(&pool, token).await;
        assert!(user.is_none());
    }

    #[tokio::test]
    async fn validate_session_disabled_user() {
        let pool = setup_db().await;
        let user_id = insert_user(&pool, "disabled@test.com", "Disabled User", "user").await;

        let session = create_session(&pool, &user_id).await.unwrap();

        // Disable the user
        sqlx::query("UPDATE users SET enabled = 0 WHERE id = ?")
            .bind(&user_id)
            .execute(&pool)
            .await
            .unwrap();

        let user = validate_session(&pool, &session.id).await;
        assert!(user.is_none());
    }

    #[tokio::test]
    async fn create_multiple_sessions_same_user() {
        let pool = setup_db().await;
        let user_id = insert_user(&pool, "multi@test.com", "Multi User", "user").await;

        let s1 = create_session(&pool, &user_id).await.unwrap();
        let s2 = create_session(&pool, &user_id).await.unwrap();
        assert_ne!(s1.id, s2.id);

        // Both sessions are valid
        assert!(validate_session(&pool, &s1.id).await.is_some());
        assert!(validate_session(&pool, &s2.id).await.is_some());
    }

    // --- delete_session ---

    #[tokio::test]
    async fn delete_session_invalidates_it() {
        let pool = setup_db().await;
        let user_id = insert_user(&pool, "del@test.com", "Del User", "user").await;

        let session = create_session(&pool, &user_id).await.unwrap();
        assert!(validate_session(&pool, &session.id).await.is_some());

        delete_session(&pool, &session.id).await.unwrap();
        assert!(validate_session(&pool, &session.id).await.is_none());
    }

    #[tokio::test]
    async fn delete_session_nonexistent_is_ok() {
        let pool = setup_db().await;
        // Should not error
        delete_session(&pool, "does-not-exist").await.unwrap();
    }

    // --- cleanup_expired_sessions ---

    #[tokio::test]
    async fn cleanup_expired_sessions_removes_old() {
        let pool = setup_db().await;
        let user_id = insert_user(&pool, "cleanup@test.com", "Cleanup User", "user").await;

        // Insert one expired and one valid session
        sqlx::query("INSERT INTO sessions (id, user_id, expires_at) VALUES (?, ?, ?)")
            .bind("expired-1")
            .bind(&user_id)
            .bind("2020-01-01T00:00:00")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO sessions (id, user_id, expires_at) VALUES (?, ?, ?)")
            .bind("expired-2")
            .bind(&user_id)
            .bind("2021-06-15T12:00:00")
            .execute(&pool)
            .await
            .unwrap();

        let valid_session = create_session(&pool, &user_id).await.unwrap();

        let removed = cleanup_expired_sessions(&pool).await.unwrap();
        assert_eq!(removed, 2);

        // Valid session still works
        assert!(validate_session(&pool, &valid_session.id).await.is_some());
        // Expired ones are gone
        assert!(validate_session(&pool, "expired-1").await.is_none());
        assert!(validate_session(&pool, "expired-2").await.is_none());
    }

    #[tokio::test]
    async fn cleanup_expired_sessions_none_expired() {
        let pool = setup_db().await;
        let user_id = insert_user(&pool, "fresh@test.com", "Fresh User", "user").await;
        let _session = create_session(&pool, &user_id).await.unwrap();

        let removed = cleanup_expired_sessions(&pool).await.unwrap();
        assert_eq!(removed, 0);
    }

    #[tokio::test]
    async fn cleanup_expired_sessions_empty_table() {
        let pool = setup_db().await;
        let removed = cleanup_expired_sessions(&pool).await.unwrap();
        assert_eq!(removed, 0);
    }

    // --- get_user_by_id ---

    #[tokio::test]
    async fn get_user_by_id_found() {
        let pool = setup_db().await;
        let user_id = insert_user(&pool, "lookup@test.com", "Lookup User", "user").await;

        let user = get_user_by_id(&pool, &user_id).await;
        assert!(user.is_some());
        let user = user.unwrap();
        assert_eq!(user.email, "lookup@test.com");
        assert_eq!(user.name, "Lookup User");
        assert_eq!(user.role, "user");
    }

    #[tokio::test]
    async fn get_user_by_id_not_found() {
        let pool = setup_db().await;
        let user = get_user_by_id(&pool, "nonexistent-id").await;
        assert!(user.is_none());
    }

    #[tokio::test]
    async fn get_user_by_id_disabled_returns_none() {
        let pool = setup_db().await;
        let user_id = insert_user(&pool, "dis@test.com", "Dis User", "user").await;

        // Disable the user
        sqlx::query("UPDATE users SET enabled = 0 WHERE id = ?")
            .bind(&user_id)
            .execute(&pool)
            .await
            .unwrap();

        let user = get_user_by_id(&pool, &user_id).await;
        assert!(user.is_none());
    }

    // --- get_auth_config ---

    #[tokio::test]
    async fn get_auth_config_returns_singleton() {
        let pool = setup_db().await;
        let config = get_auth_config(&pool).await.unwrap();
        assert_eq!(config.id, "singleton");
        // Default values from migration
        assert!(config.registration_enabled);
        assert!(!config.oidc_enabled);
        assert!(config.allowed_email_domains.is_none());
    }

    #[tokio::test]
    async fn get_auth_config_after_update() {
        let pool = setup_db().await;

        sqlx::query(
            "UPDATE auth_config SET registration_enabled = 0, allowed_email_domains = 'test.com' WHERE id = 'singleton'",
        )
        .execute(&pool)
        .await
        .unwrap();

        let config = get_auth_config(&pool).await.unwrap();
        assert!(!config.registration_enabled);
        assert_eq!(config.allowed_email_domains, Some("test.com".to_string()));
    }

    // --- has_any_users ---

    #[tokio::test]
    async fn has_any_users_empty() {
        let pool = setup_db().await;
        assert!(!has_any_users(&pool).await.unwrap());
    }

    #[tokio::test]
    async fn has_any_users_with_user() {
        let pool = setup_db().await;
        insert_user(&pool, "exists@test.com", "Exists", "user").await;
        assert!(has_any_users(&pool).await.unwrap());
    }

    // --- sync_user_groups ---

    #[tokio::test]
    async fn sync_user_groups_creates_groups() {
        let pool = setup_db().await;
        let user_id = insert_user(&pool, "grp@test.com", "Group User", "user").await;

        sync_user_groups(
            &pool,
            &user_id,
            &["Engineering".to_string(), "DevOps".to_string()],
        )
        .await
        .unwrap();

        // Verify groups were created
        let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM groups WHERE source = 'oidc'")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count.0, 2);

        // Verify memberships
        let memberships: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM user_groups WHERE user_id = ?")
                .bind(&user_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(memberships.0, 2);
    }

    #[tokio::test]
    async fn sync_user_groups_replaces_memberships() {
        let pool = setup_db().await;
        let user_id = insert_user(&pool, "grp2@test.com", "Group User 2", "user").await;

        // Initial sync with two groups
        sync_user_groups(&pool, &user_id, &["TeamA".to_string(), "TeamB".to_string()])
            .await
            .unwrap();

        // Re-sync with different groups
        sync_user_groups(&pool, &user_id, &["TeamC".to_string()])
            .await
            .unwrap();

        // Should have only one membership now
        let memberships: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM user_groups WHERE user_id = ?")
                .bind(&user_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(memberships.0, 1);

        // All three groups should still exist (groups are not deleted)
        let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM groups WHERE source = 'oidc'")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count.0, 3);
    }

    #[tokio::test]
    async fn sync_user_groups_empty_clears_memberships() {
        let pool = setup_db().await;
        let user_id = insert_user(&pool, "grp3@test.com", "Group User 3", "user").await;

        sync_user_groups(&pool, &user_id, &["TeamX".to_string()])
            .await
            .unwrap();

        // Sync with empty groups
        sync_user_groups(&pool, &user_id, &[]).await.unwrap();

        let memberships: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM user_groups WHERE user_id = ?")
                .bind(&user_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(memberships.0, 0);
    }

    // --- find_or_create_oidc_user: email_verified gate (OIDC takeover defense) ---

    async fn auth_config_with_oidc(pool: &SqlitePool, auto_register: bool) -> AuthConfig {
        let mut c = get_auth_config(pool).await.unwrap();
        c.oidc_enabled = true;
        c.oidc_auto_register = auto_register;
        c
    }

    #[tokio::test]
    async fn oidc_link_requires_email_verified() {
        // Takeover scenario: a local user "admin@company.com" exists. An
        // attacker registers at the IdP with the same email but the IdP
        // doesn't assert email_verified. Before the fix, calrs would link the
        // attacker's oidc_subject to the admin account. With the fix, the
        // link is refused.
        let pool = setup_db().await;
        let victim_id = insert_user(&pool, "admin@company.com", "Admin", "admin").await;
        let cfg = auth_config_with_oidc(&pool, false).await;

        let err = find_or_create_oidc_user(
            &pool,
            "attacker-sub-123",
            "admin@company.com",
            false, // email_verified=false — the attack vector
            "Attacker",
            &cfg,
        )
        .await
        .expect_err("must refuse to link when email is unverified");
        assert!(
            err.to_string().contains("verified"),
            "error should mention verification, got: {}",
            err
        );

        // The victim's user row must be unchanged: no oidc_subject written,
        // auth_provider still 'local'.
        let (provider, oidc_subject): (String, Option<String>) =
            sqlx::query_as("SELECT auth_provider, oidc_subject FROM users WHERE id = ?")
                .bind(&victim_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(provider, "local", "auth_provider must not change");
        assert!(oidc_subject.is_none(), "oidc_subject must remain NULL");
    }

    #[tokio::test]
    async fn oidc_link_succeeds_when_email_verified() {
        // Legitimate flow: same email match, but the IdP asserts
        // email_verified=true, so we trust it and link.
        let pool = setup_db().await;
        let user_id = insert_user(&pool, "alice@company.com", "Alice", "user").await;
        let cfg = auth_config_with_oidc(&pool, false).await;

        let result = find_or_create_oidc_user(
            &pool,
            "alice-sub-123",
            "alice@company.com",
            true,
            "Alice",
            &cfg,
        )
        .await
        .expect("verified email must link");
        assert_eq!(result, user_id);

        let (provider, oidc_subject): (String, Option<String>) =
            sqlx::query_as("SELECT auth_provider, oidc_subject FROM users WHERE id = ?")
                .bind(&user_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(provider, "oidc");
        assert_eq!(oidc_subject.as_deref(), Some("alice-sub-123"));
    }

    #[tokio::test]
    async fn oidc_auto_register_requires_email_verified() {
        // Squatting scenario: auto-register is on. Attacker signs in with a
        // fresh subject and an email they don't own. Before the fix, calrs
        // would create a new local account keyed on that email, polluting the
        // directory and potentially letting the attacker intercept anything
        // addressed to that email inside calrs. With the fix, refused.
        let pool = setup_db().await;
        let cfg = auth_config_with_oidc(&pool, true).await;

        let err = find_or_create_oidc_user(
            &pool,
            "attacker-sub",
            "victim@company.com",
            false,
            "Attacker",
            &cfg,
        )
        .await
        .expect_err("auto-register must refuse unverified email");
        assert!(err.to_string().to_lowercase().contains("verified"));

        // No user row created.
        let (count,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM users")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn oidc_auto_register_creates_user_when_email_verified() {
        let pool = setup_db().await;
        let cfg = auth_config_with_oidc(&pool, true).await;

        let user_id = find_or_create_oidc_user(
            &pool,
            "fresh-sub-001",
            "newuser@company.com",
            true,
            "New User",
            &cfg,
        )
        .await
        .expect("verified + auto_register must create");

        let (provider, oidc_subject, email): (String, Option<String>, String) =
            sqlx::query_as("SELECT auth_provider, oidc_subject, email FROM users WHERE id = ?")
                .bind(&user_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(provider, "oidc");
        assert_eq!(oidc_subject.as_deref(), Some("fresh-sub-001"));
        assert_eq!(email, "newuser@company.com");
    }

    #[tokio::test]
    async fn oidc_returning_user_bypasses_email_verified_gate() {
        // Once a user is linked (identified by oidc_subject), subsequent
        // logins always work — the subject claim is the IdP's stable
        // identifier. Whether the IdP re-asserts email_verified on each
        // login is not our concern at that point; we've already trusted
        // the subject.
        let pool = setup_db().await;
        // Seed: verified first login, auto_register=true so the account exists.
        let cfg = auth_config_with_oidc(&pool, true).await;
        let id_first = find_or_create_oidc_user(
            &pool,
            "stable-sub-001",
            "user@company.com",
            true,
            "User",
            &cfg,
        )
        .await
        .expect("initial verified sign-in succeeds");

        // Second login: same subject, but IdP has stopped asserting
        // email_verified. Must still succeed via step 1 (oidc_subject match).
        let id_second = find_or_create_oidc_user(
            &pool,
            "stable-sub-001",
            "user@company.com",
            false,
            "User",
            &cfg,
        )
        .await
        .expect("returning user with existing oidc_subject must always work");
        assert_eq!(id_first, id_second);
    }

    #[tokio::test]
    async fn oidc_no_auto_register_without_match_fails() {
        // Baseline: no existing user, auto_register=false. Unrelated to
        // verification — the pre-fix message path. Confirming we didn't
        // accidentally change this.
        let pool = setup_db().await;
        let cfg = auth_config_with_oidc(&pool, false).await;

        let err = find_or_create_oidc_user(&pool, "sub", "nope@company.com", true, "Nobody", &cfg)
            .await
            .expect_err("no user + auto_register=false must error");
        assert!(
            err.to_string().to_lowercase().contains("no account"),
            "expected 'no account' in: {}",
            err
        );
    }
}
