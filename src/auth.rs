use anyhow::{Context, Result};
use argon2::password_hash::rand_core::OsRng;
use argon2::password_hash::SaltString;
use argon2::{Argon2, PasswordHash, PasswordHasher, PasswordVerifier};
use axum::extract::{Form, FromRequestParts, State};
use axum::http::request::Parts;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Redirect, Response};
use axum_extra::extract::CookieJar;
use base64::Engine;
use chrono::{Duration, Utc};
use rand::Rng;
use sqlx::SqlitePool;
use std::sync::Arc;

use crate::models::{AuthConfig, Session, User};
use crate::web::AppState;

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

    let tmpl = match state.templates.get_template("auth/login.html") {
        Ok(t) => t,
        Err(e) => return Html(format!("Template error: {}", e)).into_response(),
    };
    Html(
        tmpl.render(minijinja::context! { error => "", oidc_enabled => oidc_enabled })
            .unwrap_or_default(),
    )
    .into_response()
}

async fn login_handler(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    jar: CookieJar,
    Form(form): Form<LoginForm>,
) -> Response {
    // Rate limit by IP (X-Forwarded-For from reverse proxy, or fallback to "unknown")
    let client_ip = headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.split(',').next())
        .unwrap_or("unknown")
        .trim()
        .to_string();

    if state.login_limiter.check_limited(&client_ip).await {
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
        "{}={}; HttpOnly; Secure; SameSite=Lax; Path=/; Max-Age={}",
        SESSION_COOKIE,
        session.id,
        SESSION_DURATION_DAYS * 86400
    );

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

    let tmpl = match state.templates.get_template("auth/register.html") {
        Ok(t) => t,
        Err(e) => return Html(format!("Template error: {}", e)).into_response(),
    };
    Html(
        tmpl.render(minijinja::context! {
            error => "",
            allowed_domains => auth_config.allowed_email_domains,
        })
        .unwrap_or_default(),
    )
    .into_response()
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

    (jar, [("Set-Cookie", cookie)], Redirect::to("/dashboard")).into_response()
}

async fn logout_handler(State(state): State<Arc<AppState>>, jar: CookieJar) -> Response {
    if let Some(cookie) = jar.get(SESSION_COOKIE) {
        let _ = delete_session(&state.pool, cookie.value()).await;
    }

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
        return Html("OIDC provider did not return an email address.".to_string()).into_response();
    }

    // Check email domain restriction
    if !is_email_allowed(&email, &auth_config.allowed_email_domains) {
        return Html("Your email domain is not allowed.".to_string()).into_response();
    }

    // Extract groups from ID token JWT payload
    let oidc_groups = extract_groups_from_id_token(id_token.to_string().as_str());

    // Find or create user
    let user_id =
        match find_or_create_oidc_user(&state.pool, &subject, &email, &name, &auth_config).await {
            Ok(id) => id,
            Err(e) => return Html(format!("Account error: {}", e)).into_response(),
        };

    // Sync groups from OIDC token (best-effort, don't fail login)
    if let Some(groups) = &oidc_groups {
        if let Err(e) = sync_user_groups(&state.pool, &user_id, groups).await {
            eprintln!("[calrs] Warning: failed to sync OIDC groups: {}", e);
        }
    }

    // Create session
    let session = match create_session(&state.pool, &user_id).await {
        Ok(s) => s,
        Err(_) => return Html("Failed to create session.".to_string()).into_response(),
    };

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
async fn find_or_create_oidc_user(
    pool: &SqlitePool,
    subject: &str,
    email: &str,
    name: &str,
    auth_config: &AuthConfig,
) -> Result<String> {
    // 1. Try to find by oidc_subject
    if let Some((id,)) = sqlx::query_as::<_, (String,)>(
        "SELECT id FROM users WHERE oidc_subject = ? AND auth_provider = 'oidc' AND enabled = 1",
    )
    .bind(subject)
    .fetch_optional(pool)
    .await?
    {
        // Update name/email in case they changed on the IdP
        sqlx::query(
            "UPDATE users SET name = ?, email = ?, updated_at = datetime('now') WHERE id = ?",
        )
        .bind(name)
        .bind(email)
        .bind(&id)
        .execute(pool)
        .await?;
        return Ok(id);
    }

    // 2. Try to link to existing user by email
    if let Some((id, _existing_provider)) = sqlx::query_as::<_, (String, String)>(
        "SELECT id, auth_provider FROM users WHERE email = ? AND enabled = 1",
    )
    .bind(email)
    .fetch_optional(pool)
    .await?
    {
        // Link OIDC identity to existing local user
        sqlx::query(
            "UPDATE users SET oidc_subject = ?, auth_provider = 'oidc', updated_at = datetime('now') WHERE id = ?",
        )
        .bind(subject)
        .bind(&id)
        .execute(pool)
        .await?;
        return Ok(id);
    }

    // 3. Create new user if auto-register is enabled
    if !auth_config.oidc_auto_register {
        anyhow::bail!("No account found for this email. Contact an administrator.");
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

use axum::response::Html;

fn render_login_error(state: &AppState, error: &str) -> Response {
    // Best-effort: try to show OIDC button even on error page
    let oidc_enabled = false; // Can't async here easily; login errors are local-auth only anyway
    let tmpl = match state.templates.get_template("auth/login.html") {
        Ok(t) => t,
        Err(_) => return Html(error.to_string()).into_response(),
    };
    Html(
        tmpl.render(minijinja::context! { error => error, oidc_enabled => oidc_enabled })
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

// --- OIDC group sync ---

/// Extract the `groups` claim from a raw JWT ID token.
/// Decodes the payload (middle part) as base64 and parses JSON.
/// Returns None if the token has no groups claim or cannot be parsed.
fn extract_groups_from_id_token(raw_token: &str) -> Option<Vec<String>> {
    let parts: Vec<&str> = raw_token.split('.').collect();
    if parts.len() != 3 {
        return None;
    }
    let payload_bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(parts[1])
        .ok()?;
    let payload: serde_json::Value = serde_json::from_slice(&payload_bytes).ok()?;
    let groups = payload.get("groups")?;
    let groups_array = groups.as_array()?;
    let group_strings: Vec<String> = groups_array
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
    }

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
}
