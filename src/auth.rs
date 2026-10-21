use anyhow::{Context, Result};
use argon2::password_hash::SaltString;
use argon2::{Argon2, PasswordHash, PasswordHasher, PasswordVerifier};
use axum::extract::{Form, FromRequestParts, State};
use axum::http::request::Parts;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{Html, IntoResponse, Redirect, Response};
use axum_extra::extract::CookieJar;
use base64::Engine;
use chrono::{Duration, Utc};
use rand::rngs::OsRng;
use rand::RngCore;
use sqlx::SqlitePool;
use std::sync::Arc;

use crate::models::{AuthConfig, Session, User};
use crate::web::{csrf_cookie_value, generate_csrf_token, verify_csrf_token, AppState};

// `__Host-` prefix forces browsers to enforce: Secure flag, Path=/, no Domain
// attribute. This prevents cookies from being overwritten by a sibling
// subdomain or downgraded over plaintext HTTP.
const SESSION_COOKIE: &str = "__Host-calrs_session";
const IMPERSONATE_COOKIE: &str = "__Host-calrs_impersonate";
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

// Pre-computed Argon2id hash used as a timing dummy when no user is found,
// preventing user enumeration via response-time differences.
const DUMMY_HASH: &str = "$argon2id$v=19$m=19456,t=2,p=1$AAAAAAAAAAAAAAAAAAAAAA$AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";

// --- Session management ---

fn generate_session_token() -> String {
    // Use OsRng (kernel CSPRNG) directly rather than thread_rng (userspace
    // ChaCha12). 30-day session tokens warrant the strongest entropy source.
    let mut bytes = [0u8; 32];
    OsRng.fill_bytes(&mut bytes);
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

// --- Last-admin guard ---

/// SQL condition on a `users` row: changing it away from "enabled admin"
/// still leaves at least one enabled admin. True for any row that is not an
/// enabled admin. It is part of the `UPDATE`/`DELETE` itself rather than a
/// count read beforehand, so two concurrent demotions cannot both pass:
/// SQLite runs one write at a time, and the second sees the first's result.
const KEEPS_AN_ENABLED_ADMIN: &str = "(users.role != 'admin' OR users.enabled = 0 \
     OR EXISTS (SELECT 1 FROM users other \
                WHERE other.role = 'admin' AND other.enabled = 1 AND other.id != users.id))";

/// Result of a role or enabled-flag change that must keep an enabled admin.
#[derive(Debug, PartialEq, Eq)]
pub enum AdminChange {
    Changed,
    NotFound,
    /// Refused: the user is the last enabled admin.
    LastAdmin,
}

/// Apply `set` (a fixed assignment, never user input) to one user, under the
/// last-admin guard when `guarded`.
async fn guarded_user_update(
    pool: &SqlitePool,
    user_id: &str,
    set: &'static str,
    guarded: bool,
) -> std::result::Result<AdminChange, sqlx::Error> {
    let guard = if guarded {
        format!(" AND {KEEPS_AN_ENABLED_ADMIN}")
    } else {
        String::new()
    };
    let sql = format!("UPDATE users SET {set}, updated_at = datetime('now') WHERE id = ?{guard}");
    let result = sqlx::query(&sql).bind(user_id).execute(pool).await?;
    if result.rows_affected() > 0 {
        return Ok(AdminChange::Changed);
    }
    let exists: Option<i64> = sqlx::query_scalar("SELECT 1 FROM users WHERE id = ?")
        .bind(user_id)
        .fetch_optional(pool)
        .await?;
    Ok(if exists.is_some() {
        AdminChange::LastAdmin
    } else {
        AdminChange::NotFound
    })
}

/// Make a user an admin (`true`) or a regular user (`false`), refusing to
/// demote the last enabled admin.
pub async fn set_user_admin(
    pool: &SqlitePool,
    user_id: &str,
    admin: bool,
) -> std::result::Result<AdminChange, sqlx::Error> {
    if admin {
        guarded_user_update(pool, user_id, "role = 'admin'", false).await
    } else {
        guarded_user_update(pool, user_id, "role = 'user'", true).await
    }
}

/// Enable or disable a user, refusing to disable the last enabled admin.
pub async fn set_user_enabled(
    pool: &SqlitePool,
    user_id: &str,
    enabled: bool,
) -> std::result::Result<AdminChange, sqlx::Error> {
    if enabled {
        guarded_user_update(pool, user_id, "enabled = 1", false).await
    } else {
        guarded_user_update(pool, user_id, "enabled = 0", true).await
    }
}

// --- User deletion ---

/// Reasons `delete_user` can refuse or fail. The web layer and CLI both
/// match on this to render a user-facing message.
#[derive(Debug)]
pub enum DeleteUserError {
    NotFound,
    LastAdmin,
    SelfDelete,
    HasFutureBookings { count: i64 },
    Db(sqlx::Error),
}

impl std::fmt::Display for DeleteUserError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound => write!(f, "user not found"),
            Self::LastAdmin => write!(
                f,
                "cannot delete the last admin (promote another user first)"
            ),
            Self::SelfDelete => {
                write!(f, "admins cannot delete themselves; ask another admin")
            }
            Self::HasFutureBookings { count } => write!(
                f,
                "user has {} upcoming booking(s) (as host or assigned member); cancel them before deletion",
                count
            ),
            Self::Db(e) => write!(f, "database error: {}", e),
        }
    }
}

impl std::error::Error for DeleteUserError {}

impl From<sqlx::Error> for DeleteUserError {
    fn from(e: sqlx::Error) -> Self {
        Self::Db(e)
    }
}

/// Delete a user and all data uniquely owned by them.
///
/// Refuses if: target is the last admin, target is the requester,
/// or the target has upcoming confirmed/pending bookings (as the
/// owning host or as an assigned member of a team event).
///
/// Walks the cascade chain explicitly because `accounts.user_id` is
/// `ON DELETE SET NULL`, not CASCADE — a naive `DELETE FROM users`
/// would orphan the user's accounts (and everything under them) and
/// would also fail the `booking_invites.created_by_user_id` foreign
/// key for any invites this user authored on other users' event types.
///
/// `requester_user_id` should be the id of the admin performing the
/// deletion (used for the self-delete check). Pass `None` to skip
/// that check (e.g. for emergency CLI cleanup or test setup).
///
/// `avatars_dir` is the directory holding avatar files. The user's
/// avatar (if any) is removed best-effort after the DB transaction
/// commits. Pass `None` to skip the filesystem cleanup.
pub async fn delete_user(
    pool: &SqlitePool,
    target_user_id: &str,
    requester_user_id: Option<&str>,
    avatars_dir: Option<&std::path::Path>,
) -> std::result::Result<(), DeleteUserError> {
    let target: Option<(Option<String>, bool)> = sqlx::query_as(&format!(
        "SELECT avatar_path, {KEEPS_AN_ENABLED_ADMIN} FROM users WHERE id = ?"
    ))
    .bind(target_user_id)
    .fetch_optional(pool)
    .await?;
    let (avatar_filename, keeps_an_admin) = match target {
        Some(t) => t,
        None => return Err(DeleteUserError::NotFound),
    };

    if let Some(req) = requester_user_id {
        if req == target_user_id {
            return Err(DeleteUserError::SelfDelete);
        }
    }

    // Checked here for an early answer, and again by the DELETE itself below
    // so a concurrent demotion cannot slip in between.
    if !keeps_an_admin {
        return Err(DeleteUserError::LastAdmin);
    }

    // Future bookings as host (own event_types) or as a round-robin assignee
    // on a team event. Both block deletion to keep guests from showing up to
    // a meeting whose host no longer exists.
    //
    // These counts run outside the deletion transaction, so a booking landing
    // between the count and the commit would be silently destroyed. Acceptable
    // for a manual admin action: the window is sub-second and the action is
    // intentional. If this is ever called from automation, switch to
    // `pool.begin()` with `BEGIN IMMEDIATE` and move the counts inside the
    // transaction so the SELECTs serialize against concurrent booking inserts.
    let host_future: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM bookings b
         JOIN event_types et ON et.id = b.event_type_id
         JOIN accounts a ON a.id = et.account_id
         WHERE a.user_id = ?
           AND b.status IN ('confirmed', 'pending')
           AND b.start_at > datetime('now')",
    )
    .bind(target_user_id)
    .fetch_one(pool)
    .await?;
    let assigned_future: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM bookings
         WHERE assigned_user_id = ?
           AND status IN ('confirmed', 'pending')
           AND start_at > datetime('now')",
    )
    .bind(target_user_id)
    .fetch_one(pool)
    .await?;
    let total_future = host_future.0 + assigned_future.0;
    if total_future > 0 {
        return Err(DeleteUserError::HasFutureBookings {
            count: total_future,
        });
    }

    let mut tx = pool.begin().await?;

    // Bookings for event types under this user's accounts must go first:
    // `bookings.event_type_id` has no ON DELETE action (a pre-existing schema
    // quirk in migration 001), so the cascade from accounts → event_types
    // would otherwise be blocked. Deleting bookings cascades booking_attendees
    // and booking_claim_tokens.
    sqlx::query(
        "DELETE FROM bookings WHERE event_type_id IN (
            SELECT et.id FROM event_types et
            JOIN accounts a ON a.id = et.account_id
            WHERE a.user_id = ?)",
    )
    .bind(target_user_id)
    .execute(&mut *tx)
    .await?;

    // Invites this user authored on event types owned by *other* users. The
    // `booking_invites.created_by_user_id` FK has no ON DELETE action either,
    // so the user delete would fail without this. Invites on the user's *own*
    // event types are mopped up by the cascade from `event_types`.
    sqlx::query("DELETE FROM booking_invites WHERE created_by_user_id = ?")
        .bind(target_user_id)
        .execute(&mut *tx)
        .await?;

    // Accounts cascade-delete their event_types, caldav_sources, calendars,
    // events, smtp_config, and (transitively) any availability rules,
    // overrides, attendees, claim tokens, member weights, frequency limits,
    // and watcher rows attached to those event types.
    sqlx::query("DELETE FROM accounts WHERE user_id = ?")
        .bind(target_user_id)
        .execute(&mut *tx)
        .await?;

    // The user delete cascades sessions, team_members, user_availability_rules,
    // event_type_member_weights, and booking_claim_tokens. teams.created_by
    // and the bookings.assigned/claimed_by_user_id columns are SET NULL.
    let deleted = sqlx::query(&format!(
        "DELETE FROM users WHERE id = ? AND {KEEPS_AN_ENABLED_ADMIN}"
    ))
    .bind(target_user_id)
    .execute(&mut *tx)
    .await?;
    if deleted.rows_affected() == 0 {
        tx.rollback().await?;
        return Err(DeleteUserError::LastAdmin);
    }

    tx.commit().await?;

    if let (Some(dir), Some(filename)) = (avatars_dir, avatar_filename) {
        let path = dir.join(&filename);
        let _ = tokio::fs::remove_file(&path).await;
    }

    Ok(())
}

// --- Auth config helpers ---

// Uses SELECT * intentionally: oidc_client_secret is encrypted at rest (AES-256-GCM)
// and encrypted in memory. Accidental serialization is prevented by #[serde(skip)]
// on the AuthConfig.oidc_client_secret field.
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

/// Insert a local-auth user, computing the admin/user role atomically inside
/// the INSERT to avoid the read-then-write race where two concurrent
/// registrations on a fresh DB both observe an empty users table and both
/// claim admin. When `force_admin` is true the user is admin regardless
/// (used by `calrs user create --admin`); otherwise the user is admin only
/// if no other users exist. Returns the role the database assigned.
pub(crate) async fn create_local_user(
    pool: &SqlitePool,
    user_id: &str,
    email: &str,
    name: &str,
    password_hash: &str,
    username: &str,
    force_admin: bool,
) -> Result<String> {
    sqlx::query_scalar(
        "INSERT INTO users (id, email, name, timezone, password_hash, role, auth_provider, username)
         VALUES (?, ?, ?, 'UTC', ?,
                 CASE WHEN ? OR NOT EXISTS (SELECT 1 FROM users) THEN 'admin' ELSE 'user' END,
                 'local', ?)
         RETURNING role",
    )
    .bind(user_id)
    .bind(email)
    .bind(name)
    .bind(password_hash)
    .bind(force_admin)
    .bind(username)
    .fetch_one(pool)
    .await
    .context("failed to insert user")
}

// --- Axum extractors ---

/// Info about an active impersonation session.
#[derive(Clone)]
pub struct ImpersonationInfo {
    pub admin_name: String,
    pub target_name: String,
}

/// Validate the session cookie and resolve admin impersonation in one place,
/// so AuthUser/OptionalAuthUser stay in sync. Returns None when there is no
/// valid session.
async fn resolve_session_user(
    pool: &SqlitePool,
    jar: &CookieJar,
) -> Option<(User, Option<ImpersonationInfo>)> {
    let token = jar.get(SESSION_COOKIE).map(|c| c.value().to_string())?;
    let real_user = validate_session(pool, &token).await?;

    if real_user.role == "admin" {
        if let Some(target_id) = jar.get(IMPERSONATE_COOKIE).map(|c| c.value().to_string()) {
            if target_id != real_user.id {
                if let Some(target_user) = get_user_by_id(pool, &target_id).await {
                    let info = ImpersonationInfo {
                        admin_name: real_user.name.clone(),
                        target_name: target_user.name.clone(),
                    };
                    return Some((target_user, Some(info)));
                }
            }
        }
    }

    Some((real_user, None))
}

/// Extractor that requires an authenticated user. Redirects to /auth/login if not authenticated.
/// Supports admin impersonation: if the `__Host-calrs_impersonate` cookie is set and the real user is
/// an admin, returns the impersonated user instead.
pub struct AuthUser {
    pub user: User,
    pub impersonation: Option<ImpersonationInfo>,
    /// Language for host-facing rendering: the user's saved preference when set
    /// and supported, else `Accept-Language`, else English. Resolved here because
    /// the extractor is the one place that has both the user row and the headers.
    pub lang: &'static str,
}

impl FromRequestParts<Arc<AppState>> for AuthUser {
    type Rejection = Response;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &Arc<AppState>,
    ) -> Result<Self, Self::Rejection> {
        let jar = CookieJar::from_headers(&parts.headers);
        match resolve_session_user(&state.pool, &jar).await {
            Some((user, impersonation)) => {
                let lang = crate::i18n::resolve(user.language.as_deref(), &parts.headers);
                Ok(AuthUser {
                    user,
                    impersonation,
                    lang,
                })
            }
            None => Err(Redirect::to("/auth/login").into_response()),
        }
    }
}

/// Extractor that does NOT require authentication. Returns Some(user) for
/// authenticated requests (mirroring AuthUser's impersonation resolution) and
/// None for anonymous/invalid sessions — never redirects.
///
/// It deliberately carries no language: it is only used on public pages,
/// which follow the visitor's browser, never a logged-in user's saved
/// preference (staff booking for a client must not stamp their own language
/// on the client's booking).
pub struct OptionalAuthUser {
    pub user: Option<User>,
    pub impersonation: Option<ImpersonationInfo>,
}

impl FromRequestParts<Arc<AppState>> for OptionalAuthUser {
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &Arc<AppState>,
    ) -> Result<Self, Self::Rejection> {
        let jar = CookieJar::from_headers(&parts.headers);
        let (user, impersonation) = match resolve_session_user(&state.pool, &jar).await {
            Some((u, info)) => (Some(u), info),
            None => (None, None),
        };
        Ok(OptionalAuthUser {
            user,
            impersonation,
        })
    }
}

/// Extractor that requires an admin user. Returns 403 if not admin.
/// Always uses the real session user, ignoring impersonation.
pub struct AdminUser {
    pub user: User,
    pub lang: &'static str,
}

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
            Some(user) if user.role == "admin" => {
                let lang = crate::i18n::resolve(user.language.as_deref(), &parts.headers);
                Ok(AdminUser { user, lang })
            }
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

async fn login_page(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    jar: CookieJar,
) -> Response {
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
        Err(e) => return crate::web::internal_error_response("template render", &e),
    };
    let body = Html(
        tmpl.render(minijinja::context! {
            lang => crate::i18n::detect_from_headers(&headers),
            error => "",
            oidc_enabled => oidc_enabled,
            registration_enabled => registration_enabled,
            csrf_token => csrf_token,
        })
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
    let lang = crate::i18n::detect_from_headers(&headers);
    let client_ip = crate::web::client_ip_for_rate_limit(&headers);

    if state.login_limiter.check_limited(&client_ip).await {
        tracing::warn!(ip = %client_ip, "rate limited");
        return render_login_error(&state, lang, "auth-error-rate-limited");
    }

    let user_row = sqlx::query_as::<_, User>(
        "SELECT * FROM users WHERE email = ? AND auth_provider = 'local' AND enabled = 1",
    )
    .bind(&form.email)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    // Always run Argon2 regardless of whether the user exists to prevent
    // user enumeration via response-time differences (timing oracle).
    let hash_to_check = user_row
        .as_ref()
        .and_then(|u| u.password_hash.as_deref())
        .unwrap_or(DUMMY_HASH);
    let password_ok = verify_password(&form.password, hash_to_check);

    let user = match user_row {
        Some(u) if password_ok => u,
        _ => {
            tracing::warn!(email = %form.email, ip = %client_ip, "login failed");
            return render_login_error(&state, lang, "auth-error-invalid-credentials");
        }
    };

    let session = match create_session(&state.pool, &user.id).await {
        Ok(s) => s,
        Err(_) => return render_login_error(&state, lang, "auth-error-internal"),
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

async fn register_page(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    jar: CookieJar,
) -> Response {
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
        google_oauth2_client_id: None,
        google_oauth2_client_secret: None,
    });

    if !auth_config.registration_enabled {
        return Html("Registration is disabled.".to_string()).into_response();
    }

    let csrf_token = generate_csrf_token();

    let tmpl = match state.templates.get_template("auth/register.html") {
        Ok(t) => t,
        Err(e) => return crate::web::internal_error_response("template render", &e),
    };
    let body = Html(
        tmpl.render(minijinja::context! {
            lang => crate::i18n::detect_from_headers(&headers),
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
    let lang = crate::i18n::detect_from_headers(&headers);
    let auth_config = match get_auth_config(&state.pool).await {
        Ok(c) => c,
        Err(_) => {
            return Html(crate::i18n::translate(lang, "auth-error-internal", None)).into_response()
        }
    };

    if !auth_config.registration_enabled {
        return Html(crate::i18n::translate(
            lang,
            "auth-error-registration-disabled",
            None,
        ))
        .into_response();
    }

    // Validate name
    let name = form.name.trim();
    if name.is_empty() || name.len() > 255 {
        return render_register_error(&state, lang, "auth-error-name-length", &auth_config);
    }

    // Validate email format
    let email = form.email.trim();
    if email.is_empty() || email.len() > 255 {
        return render_register_error(&state, lang, "auth-error-email-length", &auth_config);
    }
    if !email.contains('@')
        || email
            .rsplit('@')
            .next()
            .is_none_or(|domain| !domain.contains('.'))
    {
        return render_register_error(&state, lang, "auth-error-email-invalid", &auth_config);
    }

    // Validate email domain
    if !is_email_allowed(&form.email, &auth_config.allowed_email_domains) {
        return render_register_error(&state, lang, "auth-error-email-domain", &auth_config);
    }

    // Validate password length
    if form.password.len() < 12 {
        return render_register_error(&state, lang, "auth-error-password-length", &auth_config);
    }

    // Check if email already taken
    let existing: Option<(String,)> = sqlx::query_as("SELECT id FROM users WHERE email = ?")
        .bind(&form.email)
        .fetch_optional(&state.pool)
        .await
        .unwrap_or(None);

    if existing.is_some() {
        return render_register_error(&state, lang, "auth-error-email-taken", &auth_config);
    }

    let password_hash = match hash_password(&form.password) {
        Ok(h) => h,
        Err(_) => return render_register_error(&state, lang, "auth-error-internal", &auth_config),
    };

    let user_id = uuid::Uuid::new_v4().to_string();
    let username = match generate_username(&state.pool, &form.email).await {
        Ok(u) => u,
        Err(_) => return render_register_error(&state, lang, "auth-error-internal", &auth_config),
    };

    if create_local_user(
        &state.pool,
        &user_id,
        &form.email,
        &form.name,
        &password_hash,
        &username,
        false,
    )
    .await
    .is_err()
    {
        return render_register_error(&state, lang, "auth-error-create-failed", &auth_config);
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
use openidconnect::core::{
    CoreClient, CoreClientAuthMethod, CoreProviderMetadata, CoreResponseType,
};
use openidconnect::{
    AsyncHttpClient, AuthType, AuthenticationFlow, AuthorizationCode, ClientId, ClientSecret,
    CsrfToken, EndpointMaybeSet, EndpointNotSet, EndpointSet, IssuerUrl, Nonce, PkceCodeChallenge,
    PkceCodeVerifier, RedirectUrl, Scope, TokenResponse,
};

/// An HTTP client for `oauth2` that remembers the status of the last response.
/// `oauth2` reports any token endpoint reply that is not JSON as a parse
/// failure, whatever its status, and a refusal (401, 400) reads very
/// differently from a 200.
struct StatusRecordingClient {
    inner: openidconnect::reqwest::Client,
    status: std::sync::Mutex<Option<StatusCode>>,
}

impl StatusRecordingClient {
    fn new(inner: openidconnect::reqwest::Client) -> Self {
        Self {
            inner,
            status: std::sync::Mutex::new(None),
        }
    }

    fn last_status(&self) -> Option<StatusCode> {
        self.status.lock().ok().and_then(|s| *s)
    }
}

impl<'c> AsyncHttpClient<'c> for StatusRecordingClient {
    type Error = openidconnect::HttpClientError<openidconnect::reqwest::Error>;
    type Future = std::pin::Pin<
        Box<
            dyn std::future::Future<Output = Result<openidconnect::HttpResponse, Self::Error>>
                + Send
                + 'c,
        >,
    >;

    fn call(&'c self, request: openidconnect::HttpRequest) -> Self::Future {
        Box::pin(async move {
            let response = self.inner.call(request).await?;
            if let Ok(mut status) = self.status.lock() {
                *status = Some(response.status());
            }
            Ok(response)
        })
    }
}

fn build_http_client() -> Result<openidconnect::reqwest::Client> {
    let client = openidconnect::reqwest::ClientBuilder::new()
        .redirect(openidconnect::reqwest::redirect::Policy::none())
        .build()
        .map_err(|e| anyhow::anyhow!("Failed to build HTTP client: {}", e))?;
    Ok(client)
}

async fn build_oidc_client_with_redirect(
    auth_config: &AuthConfig,
    secret_key: &[u8; 32],
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

    // The stored OIDC client secret is encrypted at rest. The startup
    // password migration (db::migrate_passwords) handles converting
    // pre-existing plaintext values, so by the time we reach here the
    // value should always carry the enc:v1: prefix. Surface a decryption
    // failure as an error rather than falling back to an unauthenticated
    // request, so the admin sees a clear OIDC failure (and a tracing log
    // entry from the caller) rather than silently broken sign-ins.
    let client_secret = auth_config
        .oidc_client_secret
        .as_ref()
        .filter(|s| !s.is_empty())
        .map(|s| crate::crypto::decrypt_value(secret_key, s))
        .transpose()
        .map_err(|e| anyhow::anyhow!("OIDC client secret decryption failed: {}", e))?
        .map(ClientSecret::new);

    let http_client = build_http_client()?;
    let provider_metadata = CoreProviderMetadata::discover_async(issuer_url, &http_client)
        .await
        // Kept as a source rather than flattened into the message, so the
        // log shows why discovery failed (provider metadata is public).
        .context("OIDC discovery failed")?;

    let redirect_url = RedirectUrl::new(format!(
        "{}/auth/oidc/callback",
        crate::settings::base_url().unwrap_or_else(|| "http://localhost:3000".to_string())
    ))
    .map_err(|e| anyhow::anyhow!("Invalid redirect URL: {}", e))?;

    let use_post = prefers_client_secret_post(
        provider_metadata
            .token_endpoint_auth_methods_supported()
            .map(|m| m.as_slice()),
    );
    let mut client =
        CoreClient::from_provider_metadata(provider_metadata, client_id, client_secret)
            .set_redirect_uri(redirect_url);
    if use_post {
        client = client.set_auth_type(AuthType::RequestBody);
    }

    Ok(client)
}

async fn oidc_login(State(state): State<Arc<AppState>>) -> Response {
    let auth_config = match get_auth_config(&state.pool).await {
        Ok(c) if c.oidc_enabled => c,
        _ => return Html("OIDC is not enabled.".to_string()).into_response(),
    };

    let client = match build_oidc_client_with_redirect(&auth_config, &state.secret_key).await {
        Ok(c) => c,
        Err(e) => {
            return crate::web::oidc_error_response("oidc client build (login)", &format!("{e:#}"))
        }
    };

    let (pkce_challenge, pkce_verifier) = PkceCodeChallenge::new_random_sha256();

    let (auth_url, csrf_token, nonce) = client
        .authorize_url(
            AuthenticationFlow::<CoreResponseType>::AuthorizationCode,
            CsrfToken::new_random,
            Nonce::new_random,
        )
        // `openid` is added by the library itself; adding it here sent it twice.
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

/// Whether an ID token's `azp` (authorized party) claim is acceptable.
///
/// Because [`oidc_callback`] accepts ID tokens carrying audiences beyond our
/// own `client_id`, this is the check that stops a multi-audience token minted
/// for a *different* client, which happens to also list ours, from logging that
/// client's user in. OIDC Core section 3.1.3.7 step 5 mandates it and
/// `openidconnect` 4 does not implement it: the crate has the code written out
/// in `verification/mod.rs` but commented out, deferred pending a clear use
/// case.
///
/// A token with no `azp` is accepted, since IdPs commonly omit it and the nonce
/// binding plus the mandatory `client_id`-in-`aud` check still apply. An `azp`
/// present with no configured `client_id` to compare against fails closed.
fn azp_matches_client(azp: Option<&str>, client_id: Option<&str>) -> bool {
    match (azp, client_id) {
        (None, _) => true,
        (Some(azp), Some(client_id)) => azp == client_id,
        (Some(_), None) => false,
    }
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

    let client = match build_oidc_client_with_redirect(&auth_config, &state.secret_key).await {
        Ok(c) => c,
        Err(e) => {
            return crate::web::oidc_error_response(
                "oidc client build (callback)",
                &format!("{e:#}"),
            )
        }
    };

    let http_client = match build_http_client() {
        Ok(c) => c,
        Err(e) => return crate::web::oidc_error_response("oidc http client", &format!("{e:#}")),
    };

    // Exchange code for tokens
    let token_request = match client.exchange_code(AuthorizationCode::new(query.code)) {
        Ok(r) => r,
        Err(e) => return crate::web::oidc_error_response_with_source("oidc exchange_code", &e),
    };
    // `oauth2` reports any token endpoint reply that is not JSON as a parse
    // failure, whatever its HTTP status, so record the status on the way
    // through: a refusal (401, 400) reads very differently from a 200.
    let recording_client = StatusRecordingClient::new(http_client);
    let token_response = match token_request
        .set_pkce_verifier(PkceCodeVerifier::new(pkce_verifier_secret))
        .request_async(&recording_client)
        .await
    {
        Ok(t) => t,
        // A body that fails to parse displays only as "Failed to parse server
        // response" and its source can quote JSON values, tokens included, so
        // it gets a dedicated line: where serde stopped, the reason with its
        // quoted values masked, and the body's shape without its values.
        // `path` is "." for any id_token or claim error, because the token
        // response flattens its extra fields; the claim name is in `reason`.
        Err(openidconnect::RequestTokenError::Parse(parse_err, body)) => {
            let status = recording_client.last_status();
            tracing::error!(
                context = "oidc token exchange",
                status = %status.map_or_else(|| "unknown".to_string(), |s| s.to_string()),
                path = %parse_err.path(),
                reason = %redact_reason(&parse_err.inner().to_string()),
                body = %summarize_token_response_body(&body),
                error_text = %token_error_excerpt(status, &body).unwrap_or_default(),
                "oidc auth failure: token response did not parse"
            );
            return crate::web::oidc_failure_response();
        }
        Err(e) => return crate::web::oidc_error_response_with_source("oidc token exchange", &e),
    };

    // Verify and extract ID token claims
    let id_token = match token_response.id_token() {
        Some(t) => t,
        None => return Html("No ID token in response.".to_string()).into_response(),
    };

    // Accept ID tokens whose `aud` carries audiences in addition to our
    // client_id. Several IdPs (e.g. Zitadel, which always adds the project_id;
    // some Keycloak/Azure AD setups) emit multi-audience tokens. This is
    // spec-compliant: OIDC Core §3.1.3.7 allows extra audiences. The default
    // `openidconnect` verifier rejects any audience besides the client_id,
    // which breaks login against those providers ("Invalid audiences: <x> is
    // not a trusted audience"). The verifier still independently requires our
    // own client_id to be present in `aud`, so a token minted for a different
    // client is still rejected.
    let verifier = client
        .id_token_verifier()
        .set_other_audience_verifier_fn(|_aud| true);
    let claims = match id_token.claims(&verifier, &Nonce::new(stored_nonce)) {
        Ok(c) => c,
        Err(e) => return crate::web::oidc_error_response_with_source("oidc id_token verify", &e),
    };

    // Since we now accept additional audiences, restore the `azp` defense that
    // OIDC Core §3.1.3.7 (5) mandates and that this crate does NOT implement
    // (its README lists "verification of the azp claim" as unsupported): if the
    // token carries an `azp` (authorized party) claim, it MUST equal our
    // client_id. This rejects a multi-audience token that was actually issued
    // for a *different* client but happens to also list ours. When `azp` is
    // absent we fall back on the already-verified nonce binding plus the
    // mandatory client_id-in-aud check, so IdPs that omit `azp` still work.
    let azp = claims.authorized_party().map(|azp| azp.to_string());
    if !azp_matches_client(azp.as_deref(), auth_config.oidc_client_id.as_deref()) {
        // Routed through oidc_error_response like every other verification
        // failure in this handler: it logs at error level and answers 500 with
        // a generic body, so the rejection is visible to the operator without
        // telling the caller which check it tripped.
        return crate::web::oidc_error_response(
            "oidc id_token azp",
            &format!(
                "authorized party {:?} does not match the configured client_id {:?}",
                azp, auth_config.oidc_client_id
            ),
        );
    }

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
            return crate::web::oidc_error_response("oidc account link/create", &format!("{e:#}"));
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

    let user_id = uuid::Uuid::new_v4().to_string();
    let username = generate_username(pool, email).await?;

    // Compute the role inside the INSERT so "first user gets admin" is
    // atomic with the row creation; see create_local_user for the rationale.
    sqlx::query(
        "INSERT INTO users (id, email, name, timezone, role, auth_provider, oidc_subject, username)
         VALUES (?, ?, ?, 'UTC',
                 CASE WHEN NOT EXISTS (SELECT 1 FROM users) THEN 'admin' ELSE 'user' END,
                 'oidc', ?, ?)",
    )
    .bind(&user_id)
    .bind(email)
    .bind(name)
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

fn render_login_error(state: &AppState, lang: &str, error_key: &str) -> Response {
    // Best-effort: try to show OIDC button even on error page
    let oidc_enabled = false; // Can't async here easily; login errors are local-auth only anyway
    let csrf_token = generate_csrf_token();
    let error = crate::i18n::translate(lang, error_key, None);
    let tmpl = match state.templates.get_template("auth/login.html") {
        Ok(t) => t,
        Err(_) => return Html(error).into_response(),
    };
    let body = Html(
        tmpl.render(minijinja::context! { lang => lang, error => error, oidc_enabled => oidc_enabled, csrf_token => csrf_token })
            .unwrap_or_else(|_| error.clone()),
    );
    ([("Set-Cookie", csrf_cookie_value(&csrf_token))], body).into_response()
}

fn render_register_error(
    state: &AppState,
    lang: &str,
    error_key: &str,
    auth_config: &AuthConfig,
) -> Response {
    let csrf_token = generate_csrf_token();
    let error = crate::i18n::translate(lang, error_key, None);
    let tmpl = match state.templates.get_template("auth/register.html") {
        Ok(t) => t,
        Err(_) => return Html(error).into_response(),
    };
    let body = Html(
        tmpl.render(minijinja::context! {
            lang => lang,
            error => error,
            allowed_domains => auth_config.allowed_email_domains,
            csrf_token => csrf_token,
        })
        .unwrap_or_else(|_| error.clone()),
    );
    ([("Set-Cookie", csrf_cookie_value(&csrf_token))], body).into_response()
}

// --- OIDC token response diagnostics (#264) ---

/// Longest serde reason logged after masking, in chars.
const REASON_MAX_CHARS: usize = 512;

/// Mask every quoted or backticked literal in a serde message from a failed
/// token response parse, and cap its length. Those literals are the values
/// serde complains about (an access token or, for an id_token claim of the
/// wrong type, an address, email or phone number), so each one becomes
/// `<string N chars>` unless it is a short `[A-Za-z0-9.-]` word with at
/// most six digits, such as `"3600"` or `"true"` (see `keeps_literal`).
/// Unquoted text, the field and type names that say
/// what went wrong, is kept.
fn redact_reason(message: &str) -> String {
    // Bound the scan as well as the output. A literal cut by the bound is
    // unclosed, so it stays masked through the end.
    let chars: Vec<char> = message.chars().take(4 * REASON_MAX_CHARS).collect();
    let mut out = String::with_capacity(message.len());
    mask_literals(&chars, 0, None, &mut out);
    if out.chars().count() > REASON_MAX_CHARS {
        out = out.chars().take(REASON_MAX_CHARS).collect();
        out.push_str("...");
    }
    out
}

/// The delimiter of a literal in a serde message.
#[derive(Clone, Copy, PartialEq)]
enum Delim {
    /// A double quote after this many backslashes: `"` is 0 and `\"` is 1.
    /// openidconnect wraps claim errors in `Debug`, which nests a message
    /// quoting a value inside another quoted string, so the count tells the
    /// levels apart.
    Quote(usize),
    Backtick,
}

/// The delimiter starting at `chars[i]`, if any, and its length in chars.
fn delim_at(chars: &[char], i: usize) -> Option<(Delim, usize)> {
    if chars[i] == '`' {
        return Some((Delim::Backtick, 1));
    }
    let backslashes = chars[i..].iter().take_while(|&&c| c == '\\').count();
    (chars.get(i + backslashes) == Some(&'"'))
        .then_some((Delim::Quote(backslashes), backslashes + 1))
}

/// Copy `chars[i..]` into `out` up to the delimiter `close` (the end of the
/// text when `None`), masking the literals inside. Returns the index after
/// `close`, or `None` when the text ends first. An unclosed literal masks
/// everything from its opening to the end, so a message shaped differently
/// than expected fails closed.
fn mask_literals(
    chars: &[char],
    mut i: usize,
    close: Option<Delim>,
    out: &mut String,
) -> Option<usize> {
    while i < chars.len() {
        let Some((delim, open_len)) = delim_at(chars, i) else {
            out.push(chars[i]);
            i += 1;
            continue;
        };
        if Some(delim) == close {
            return Some(i + open_len);
        }
        let start = i + open_len;
        let mut inner = String::new();
        let end = match delim {
            // Backticks do not nest: everything up to the next one is a value.
            Delim::Backtick => chars[start..]
                .iter()
                .position(|&c| c == '`')
                .map(|p| start + p + 1),
            Delim::Quote(_) => mask_literals(chars, start, Some(delim), &mut inner),
        };
        let Some(end) = end else {
            if close.is_some() {
                return None;
            }
            out.push_str(&format!("<string {} chars>", chars.len() - start));
            return Some(chars.len());
        };
        let raw = &chars[start..end - open_len];
        if raw.iter().any(|&c| c == '"' || c == '`') && wraps_error_message(&chars[..i]) {
            // A `Debug`-wrapped serde message quoting values of its own:
            // keep its text and the masked values inside. Any other quoted
            // text holding quotes is a value with quotes in it (serde writes
            // `string "a \"b\" c"`), so it is masked whole below.
            out.extend(&chars[i..start]);
            out.push_str(&inner);
            out.extend(&chars[end - open_len..end]);
        } else if keeps_literal(raw) {
            out.extend(&chars[i..end]);
        } else {
            out.push_str(&format!("<string {} chars>", raw.len()));
        }
        i = end;
    }
    close.is_none().then_some(i)
}

/// Whether the text before a quoted literal opens a `Debug`-formatted
/// serde_json error, `Error("message", line: .., column: ..)`, which is how
/// openidconnect reports an id_token claim of the wrong type.
fn wraps_error_message(before: &[char]) -> bool {
    before.ends_with(&['E', 'r', 'r', 'o', 'r', '('])
}

/// Whether a literal is harmless enough to log as is: a short
/// `[A-Za-z0-9.-]` word (`"true"`, `"Bearer"`, `"1.0"`) with at most six
/// digits, enough for `"3600"` or an `expires_in` of `"86400"`. Seven
/// digits or more could be a phone number, so it is masked.
fn keeps_literal(raw: &[char]) -> bool {
    raw.len() <= 8
        && raw
            .iter()
            .all(|&c| c.is_ascii_alphanumeric() || c == '.' || c == '-')
        && raw.iter().filter(|c| c.is_ascii_digit()).count() <= 6
}

fn json_type_name(value: &serde_json::Value) -> &'static str {
    match value {
        serde_json::Value::Null => "null",
        serde_json::Value::Bool(_) => "bool",
        serde_json::Value::Number(_) => "number",
        serde_json::Value::String(_) => "string",
        serde_json::Value::Array(_) => "array",
        serde_json::Value::Object(_) => "object",
    }
}

/// `{key: type, ...}` for a JSON object, keys sorted, values never shown.
/// Keys are IdP-chosen names, so they are capped in length and count.
fn json_shape(map: &serde_json::Map<String, serde_json::Value>) -> String {
    let mut fields: Vec<String> = map
        .iter()
        .map(|(k, v)| {
            let key: String = k.chars().take(64).collect();
            format!("{key:?}: {}", json_type_name(v))
        })
        .collect();
    fields.sort();
    let more = fields.len().saturating_sub(50);
    fields.truncate(50);
    if more > 0 {
        fields.push(format!("... {more} more"));
    }
    format!("{{{}}}", fields.join(", "))
}

/// Decode one base64url JWT segment into a JSON object, tolerating padding.
fn decode_jwt_segment(segment: &str) -> Option<serde_json::Map<String, serde_json::Value>> {
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(segment.trim_end_matches('='))
        .ok()?;
    match serde_json::from_slice(&bytes).ok()? {
        serde_json::Value::Object(map) => Some(map),
        _ => None,
    }
}

/// What an `id_token` string looks like: segment count, the JOSE header's
/// `alg`/`typ`/`kid` (no other header field), and the claim names and types
/// of a three-segment token. The signature is never decoded or shown.
fn summarize_id_token(token: &str) -> String {
    let segments: Vec<&str> = token.split('.').collect();
    let mut out = format!("{} segments", segments.len());
    match decode_jwt_segment(segments[0]) {
        Some(header) => {
            let picked: Vec<String> = ["alg", "typ", "kid"]
                .iter()
                .filter_map(|name| {
                    let v = header.get(*name)?;
                    Some(match v.as_str() {
                        Some(s) => format!("{name}: {:?}", s.chars().take(64).collect::<String>()),
                        None => format!("{name}: {}", json_type_name(v)),
                    })
                })
                .collect();
            out.push_str(&format!(", header {{{}}}", picked.join(", ")));
        }
        None => out.push_str(", header not decodable"),
    }
    // Five segments is a JWE: the second one is an encrypted key, not claims.
    if segments.len() == 3 {
        match decode_jwt_segment(segments[1]) {
            Some(claims) => out.push_str(&format!(", claims {}", json_shape(&claims))),
            None => out.push_str(", claims not decodable"),
        }
    }
    out
}

/// Describe a token endpoint body that `oauth2` failed to parse, without
/// any of its values: its length, then for a JSON object each top-level key
/// with the JSON type of its value, and for a string `id_token` the summary
/// from `summarize_id_token`. Tokens are never part of the output, so the
/// line is safe to log at error level.
fn summarize_token_response_body(body: &[u8]) -> String {
    let len = body.len();
    let value: serde_json::Value = match serde_json::from_slice(body) {
        Ok(v) => v,
        // An HTML page here is usually a proxy or IdP error page; say so
        // without echoing it.
        Err(_) if body.trim_ascii_start().starts_with(b"<") => {
            return format!("{len} bytes, not JSON (looks like HTML)")
        }
        Err(_) => return format!("{len} bytes, not JSON"),
    };
    let map = match value {
        serde_json::Value::Object(map) => map,
        other => return format!("{len} bytes, JSON {}", json_type_name(&other)),
    };
    let mut out = format!("{len} bytes, JSON object {}", json_shape(&map));
    if let Some(serde_json::Value::String(token)) = map.get("id_token") {
        out.push_str(&format!(", id_token: {}", summarize_id_token(token)));
    }
    out
}

/// The text of a token endpoint refusal, when it is short plain text: the
/// status is known and not 200, the body is not JSON, at most
/// `TOKEN_ERROR_EXCERPT_MAX` bytes of UTF-8 with no control characters.
/// A refusal carries no token, and a 200 body is never shown, since that is
/// where tokens are. Anything else returns `None` and only the summary from
/// `summarize_token_response_body` is logged.
fn token_error_excerpt(status: Option<StatusCode>, body: &[u8]) -> Option<String> {
    const TOKEN_ERROR_EXCERPT_MAX: usize = 200;
    if status? == StatusCode::OK || body.len() > TOKEN_ERROR_EXCERPT_MAX {
        return None;
    }
    if serde_json::from_slice::<serde_json::Value>(body).is_ok() {
        return None;
    }
    let text = std::str::from_utf8(body).ok()?.trim();
    if text.is_empty() || text.chars().any(char::is_control) {
        return None;
    }
    Some(format!("{text:?}"))
}

/// Whether to send the client credentials in the token request body
/// (`client_secret_post`) rather than an HTTP Basic header. Only when the
/// provider lists its methods, lists `client_secret_post` and not
/// `client_secret_basic`: with no list, OIDC Discovery says the default is
/// `client_secret_basic`.
fn prefers_client_secret_post(methods: Option<&[CoreClientAuthMethod]>) -> bool {
    methods.is_some_and(|m| {
        m.contains(&CoreClientAuthMethod::ClientSecretPost)
            && !m.contains(&CoreClientAuthMethod::ClientSecretBasic)
    })
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

    // --- token response diagnostics (#264) ---

    fn b64(s: &str) -> String {
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(s)
    }

    const ACCESS: &str = "AT-secret-0123456789abcdefghijklmnop";
    const SIG: &str = "c2lnbmF0dXJlLXNlY3JldC1ieXRlcy0wMTIzNDU2Nzg5";

    #[test]
    fn token_body_summary_lists_keys_and_types_only() {
        let id_token = format!(
            "{}.{}.{SIG}",
            b64(r#"{"alg":"RS256","typ":"JWT","kid":"k1","x5c":["MIIsecret"]}"#),
            b64(r#"{"sub":"u-42","aud":["c"],"email_verified":"true","iat":1}"#)
        );
        let body = format!(
            r#"{{"access_token":"{ACCESS}","expires_in":"3600","id_token":"{id_token}","token_type":"Bearer"}}"#
        );
        let s = summarize_token_response_body(body.as_bytes());
        assert_eq!(
            s,
            format!(
                "{} bytes, JSON object {{\"access_token\": string, \"expires_in\": string, \
                 \"id_token\": string, \"token_type\": string}}, id_token: 3 segments, \
                 header {{alg: \"RS256\", typ: \"JWT\", kid: \"k1\"}}, claims {{\"aud\": array, \
                 \"email_verified\": string, \"iat\": number, \"sub\": string}}",
                body.len()
            )
        );
        assert!(!s.contains(ACCESS));
        assert!(!s.contains(SIG));
        assert!(!s.contains("u-42"));
        assert!(!s.contains("MIIsecret"));
    }

    #[test]
    fn token_body_summary_handles_unsigned_and_odd_tokens() {
        let unsigned = format!("{}.{}.", b64(r#"{"alg":"none"}"#), b64(r#"{"sub":"x"}"#));
        let body = format!(r#"{{"id_token":"{unsigned}"}}"#);
        let s = summarize_token_response_body(body.as_bytes());
        assert!(
            s.ends_with(r#"id_token: 3 segments, header {alg: "none"}, claims {"sub": string}"#),
            "{s}"
        );

        let s = summarize_token_response_body(br#"{"id_token":"not-a-jwt"}"#);
        assert!(
            s.ends_with("id_token: 1 segments, header not decodable"),
            "{s}"
        );

        let jwe = format!("{}.a.b.c.d", b64(r#"{"alg":"RSA-OAEP","enc":"A256GCM"}"#));
        let body = format!(r#"{{"id_token":"{jwe}"}}"#);
        let s = summarize_token_response_body(body.as_bytes());
        assert!(
            s.ends_with(r#"id_token: 5 segments, header {alg: "RSA-OAEP"}"#),
            "{s}"
        );

        let s = summarize_token_response_body(br#"{"id_token":null}"#);
        assert!(s.ends_with(r#"{"id_token": null}"#), "{s}");
    }

    #[test]
    fn token_error_excerpt_shows_short_refusals_only() {
        let unauthorized = Some(StatusCode::UNAUTHORIZED);
        assert_eq!(
            token_error_excerpt(unauthorized, b"invalid_client\n").as_deref(),
            Some("\"invalid_client\"")
        );
        // A 200 body is where tokens are, and an unknown status may be one.
        let body = format!("access_token={ACCESS}&token_type=bearer");
        assert_eq!(
            token_error_excerpt(Some(StatusCode::OK), body.as_bytes()),
            None
        );
        assert_eq!(token_error_excerpt(None, b"invalid_client"), None);
        // JSON is summarized by shape, never echoed.
        assert_eq!(token_error_excerpt(unauthorized, br#"{"error":"x"}"#), None);
        assert_eq!(token_error_excerpt(unauthorized, &[b'a'; 201]), None);
        assert_eq!(token_error_excerpt(unauthorized, b"a\x1b[2Jb"), None);
        assert_eq!(token_error_excerpt(unauthorized, b"  "), None);
        assert_eq!(token_error_excerpt(unauthorized, &[0xff, 0xfe]), None);
    }

    #[test]
    fn client_secret_post_only_when_basic_is_not_offered() {
        use CoreClientAuthMethod::{ClientSecretBasic, ClientSecretPost, PrivateKeyJwt};
        assert!(!prefers_client_secret_post(None));
        assert!(!prefers_client_secret_post(Some(&[])));
        assert!(prefers_client_secret_post(Some(&[ClientSecretPost])));
        assert!(prefers_client_secret_post(Some(&[
            PrivateKeyJwt,
            ClientSecretPost
        ])));
        assert!(!prefers_client_secret_post(Some(&[
            ClientSecretPost,
            ClientSecretBasic
        ])));
        assert!(!prefers_client_secret_post(Some(&[PrivateKeyJwt])));
    }

    #[test]
    fn token_body_summary_never_echoes_non_json_or_non_objects() {
        let body = format!("access_token={ACCESS}&token_type=bearer");
        assert_eq!(
            summarize_token_response_body(body.as_bytes()),
            format!("{} bytes, not JSON", body.len())
        );
        assert_eq!(summarize_token_response_body(b""), "0 bytes, not JSON");
        assert_eq!(
            summarize_token_response_body(format!(r#"["{ACCESS}"]"#).as_bytes()),
            format!("{} bytes, JSON array", ACCESS.len() + 4)
        );
        let page = format!("\n  <html><body>502 Bad Gateway {ACCESS}</body></html>");
        let s = summarize_token_response_body(page.as_bytes());
        assert_eq!(
            s,
            format!("{} bytes, not JSON (looks like HTML)", page.len())
        );
    }

    #[test]
    fn redact_reason_keeps_short_words_and_masks_other_literals() {
        let msg = r#"invalid type: string "3600", expected u64 at line 1 column 40"#;
        assert_eq!(redact_reason(msg), msg);
        let msg =
            format!(r#"invalid type: string "{ACCESS}", expected struct StandardTokenResponse"#);
        assert_eq!(
            redact_reason(&msg),
            "invalid type: string <string 36 chars>, expected struct StandardTokenResponse"
        );
        // `~` and `%` are not base64url, which the quoting rule does not need.
        let msg = r#"invalid type: string "rt~very~secret~value~0123", expected u64"#;
        assert_eq!(
            redact_reason(msg),
            "invalid type: string <string 25 chars>, expected u64"
        );
        // Backticked values and unclosed literals are masked too.
        assert_eq!(
            redact_reason("invalid type: integer `33612345678`, expected a boolean"),
            "invalid type: integer <string 11 chars>, expected a boolean"
        );
        assert_eq!(
            redact_reason(r#"phone_number_verified: "+33 6 12"#),
            "phone_number_verified: <string 8 chars>"
        );
    }

    #[test]
    fn redact_reason_masks_short_literals_with_seven_digits_or_more() {
        // An eight-digit local phone number fits the short-word length.
        assert_eq!(
            redact_reason(r#"invalid type: string "06123456", expected a boolean"#),
            "invalid type: string <string 8 chars>, expected a boolean"
        );
        assert_eq!(
            redact_reason("invalid type: integer `1234567`, expected a boolean"),
            "invalid type: integer <string 7 chars>, expected a boolean"
        );
        // Small numbers and words stay readable.
        for msg in [
            r#"invalid type: string "86400", expected u64"#,
            r#"invalid type: string "123456", expected u64"#,
            r#"unknown variant "Bearer2", expected one of"#,
        ] {
            assert_eq!(redact_reason(msg), msg);
        }
    }

    #[test]
    fn redact_reason_caps_its_length() {
        let msg = "é".repeat(2000);
        let out = redact_reason(&msg);
        assert_eq!(out.chars().count(), REASON_MAX_CHARS + 3);
        assert!(out.ends_with("..."));
    }

    #[test]
    fn bad_claim_reason_keeps_the_claim_name_and_masks_its_value() {
        // A wrong-typed claim fails the whole token response parse, inside a
        // serde message that openidconnect wraps in `Debug`.
        type Resp = openidconnect::core::CoreTokenResponse;
        let address = "12 Rue de la Paix, 75002 Paris, Jean Dupont";
        let email = "jean.dupont@example.com";
        for (claim, value) in [("address", address), ("email_verified", email)] {
            let claims = format!(
                r#"{{"iss":"https://idp","sub":"s","aud":"c","exp":1,"iat":1,"{claim}":"{value}"}}"#
            );
            let id_token = format!("{}.{}.{SIG}", b64(r#"{"alg":"RS256"}"#), b64(&claims));
            let body = format!(
                r#"{{"access_token":"{ACCESS}","token_type":"Bearer","id_token":"{id_token}"}}"#
            );
            let err = serde_json::from_slice::<Resp>(body.as_bytes()).unwrap_err();
            let reason = redact_reason(&err.to_string());
            assert!(reason.contains(claim), "{reason}");
            assert!(
                !reason.contains("Paix") && !reason.contains("dupont"),
                "{reason}"
            );
            assert!(
                !reason.contains(ACCESS) && !reason.contains(SIG),
                "{reason}"
            );
        }
    }

    #[test]
    fn a_value_with_quotes_in_it_is_masked_whole() {
        // serde escapes quotes inside a string value, which used to read as a
        // nested message: the text around the inner quotes was logged.
        type Resp = openidconnect::core::CoreTokenResponse;
        let value = r#"Jean \"JD\" Dupont, 12 Rue de la Paix"#;
        let top = format!(r#"{{"access_token":"x","token_type":"Bearer","expires_in":"{value}"}}"#);
        let claims = format!(
            r#"{{"iss":"https://idp","sub":"s","aud":"c","exp":1,"iat":1,"email_verified":"{value}"}}"#
        );
        let id_token = format!("{}.{}.{SIG}", b64(r#"{"alg":"RS256"}"#), b64(&claims));
        let in_claim =
            format!(r#"{{"access_token":"x","token_type":"Bearer","id_token":"{id_token}"}}"#);
        for body in [top, in_claim] {
            let err = serde_json::from_slice::<Resp>(body.as_bytes()).unwrap_err();
            let reason = redact_reason(&err.to_string());
            for part in ["Jean", "JD", "Dupont", "Paix"] {
                assert!(!reason.contains(part), "{reason}");
            }
            assert!(reason.contains("invalid type"), "{reason}");
        }
    }

    #[test]
    fn redact_reason_bounds_its_scan_and_keeps_a_cut_literal_masked() {
        let msg = format!(r#"invalid type: string "{}""#, "secret ".repeat(1000));
        let out = redact_reason(&msg);
        assert!(!out.contains("secret"), "{out}");
        assert!(out.chars().count() <= REASON_MAX_CHARS + 3);
    }

    #[tokio::test]
    async fn discovery_failure_keeps_its_cause_chain() {
        // Serve provider metadata that does not parse, and check the error
        // the callback logs says why rather than only "discovery failed".
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let app = Router::new().route(
            "/.well-known/openid-configuration",
            get(|| async { ([("content-type", "application/json")], r#"{"issuer":5}"#) }),
        );
        tokio::spawn(async move { axum::serve(listener, app).await });
        let cfg = AuthConfig {
            id: "singleton".to_string(),
            registration_enabled: false,
            allowed_email_domains: None,
            created_at: String::new(),
            updated_at: String::new(),
            oidc_enabled: true,
            oidc_issuer_url: Some(format!("http://{addr}")),
            oidc_client_id: Some("c".to_string()),
            oidc_client_secret: None,
            oidc_auto_register: false,
            google_oauth2_client_id: None,
            google_oauth2_client_secret: None,
        };
        let err = build_oidc_client_with_redirect(&cfg, &[0u8; 32])
            .await
            .unwrap_err();
        let logged = format!("{err:#}");
        assert!(logged.starts_with("OIDC discovery failed: "), "{logged}");
        assert!(logged.contains("invalid type"), "{logged}");
    }

    #[test]
    fn real_parse_failure_is_summarized_safely() {
        // Drive openidconnect's own token response type over a body that
        // fails to parse, the way the callback receives it, and check what
        // the diagnostic line would carry.
        type Resp = openidconnect::core::CoreTokenResponse;
        let id_token = format!(
            "{}.{}.{SIG}",
            b64(r#"{"alg":"RS256"}"#),
            b64(r#"{"iss":"https://idp","sub":"s","aud":"c","exp":1,"iat":1}"#)
        );
        let body = format!(
            r#"{{"access_token":"{ACCESS}","token_type":"Bearer","expires_in":"3600","id_token":"{id_token}"}}"#
        );
        let err = serde_json::from_slice::<Resp>(body.as_bytes()).unwrap_err();
        let reason = redact_reason(&err.to_string());
        assert!(reason.contains("3600"), "{reason}");
        assert!(
            !reason.contains(ACCESS) && !reason.contains(SIG),
            "{reason}"
        );
    }

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

    // --- generate_session_token ---

    #[test]
    fn session_tokens_are_64_hex_chars_and_unique() {
        let a = generate_session_token();
        let b = generate_session_token();
        assert_eq!(a.len(), 64, "32 bytes hex-encoded should be 64 chars");
        assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(a, b, "two consecutive tokens must not collide");
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

    // --- azp verification (multi-audience ID tokens) ---

    #[test]
    fn azp_absent_is_accepted() {
        // IdPs commonly omit azp; the nonce binding and the mandatory
        // client_id-in-aud check still apply, so this must not break login.
        assert!(azp_matches_client(None, Some("calrs")));
        assert!(azp_matches_client(None, None));
    }

    #[test]
    fn azp_matching_our_client_is_accepted() {
        assert!(azp_matches_client(Some("calrs"), Some("calrs")));
    }

    #[test]
    fn azp_for_another_client_is_rejected() {
        // The case that matters: a multi-audience token minted for a different
        // client that happens to list ours in aud.
        assert!(!azp_matches_client(Some("other-app"), Some("calrs")));
    }

    #[test]
    fn azp_without_a_configured_client_id_fails_closed() {
        assert!(!azp_matches_client(Some("calrs"), None));
    }

    #[test]
    fn azp_comparison_is_exact() {
        assert!(!azp_matches_client(Some("calrs "), Some("calrs")));
        assert!(!azp_matches_client(Some("CALRS"), Some("calrs")));
        assert!(!azp_matches_client(Some("calrs-staging"), Some("calrs")));
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

    #[tokio::test]
    async fn oidc_first_auto_registered_user_gets_admin_role() {
        // Pins the inline-CASE behavior: when auto-register creates the very
        // first user via OIDC, that user gets the admin role atomically.
        let pool = setup_db().await;
        let cfg = auth_config_with_oidc(&pool, true).await;

        let user_id =
            find_or_create_oidc_user(&pool, "sub-1", "first@company.com", true, "First", &cfg)
                .await
                .unwrap();

        let role: String = sqlx::query_scalar("SELECT role FROM users WHERE id = ?")
            .bind(&user_id)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(role, "admin");
    }

    #[tokio::test]
    async fn oidc_subsequent_auto_registered_user_gets_user_role() {
        // With users already in the DB, OIDC auto-register must produce a
        // plain 'user' (the admin slot is already taken).
        let pool = setup_db().await;
        insert_user(&pool, "existing@company.com", "Existing", "admin").await;
        let cfg = auth_config_with_oidc(&pool, true).await;

        let user_id =
            find_or_create_oidc_user(&pool, "sub-2", "second@company.com", true, "Second", &cfg)
                .await
                .unwrap();

        let role: String = sqlx::query_scalar("SELECT role FROM users WHERE id = ?")
            .bind(&user_id)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(role, "user");
    }

    // --- delete_user ---

    /// Insert a user + linked account and return (user_id, account_id).
    async fn insert_user_with_account(
        pool: &SqlitePool,
        email: &str,
        name: &str,
        role: &str,
    ) -> (String, String) {
        let user_id = insert_user(pool, email, name, role).await;
        let account_id = uuid::Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO accounts (id, name, email, timezone, user_id) VALUES (?, ?, ?, 'UTC', ?)",
        )
        .bind(&account_id)
        .bind(name)
        .bind(email)
        .bind(&user_id)
        .execute(pool)
        .await
        .unwrap();
        (user_id, account_id)
    }

    #[tokio::test]
    async fn delete_user_not_found() {
        let pool = setup_db().await;
        let result = delete_user(&pool, "nonexistent-id", None, None).await;
        assert!(matches!(result, Err(DeleteUserError::NotFound)));
    }

    #[tokio::test]
    async fn delete_user_blocks_self_delete() {
        let pool = setup_db().await;
        let (uid, _) = insert_user_with_account(&pool, "alice@x.com", "Alice", "admin").await;
        // Add a second admin so the LastAdmin check doesn't trigger first.
        insert_user_with_account(&pool, "bob@x.com", "Bob", "admin").await;

        let result = delete_user(&pool, &uid, Some(&uid), None).await;
        assert!(matches!(result, Err(DeleteUserError::SelfDelete)));
    }

    #[tokio::test]
    async fn delete_user_blocks_last_admin() {
        let pool = setup_db().await;
        let (uid, _) = insert_user_with_account(&pool, "alice@x.com", "Alice", "admin").await;
        // No other admin. Calling without a requester so SelfDelete doesn't
        // mask the LastAdmin check.
        let result = delete_user(&pool, &uid, None, None).await;
        assert!(matches!(result, Err(DeleteUserError::LastAdmin)));
    }

    #[tokio::test]
    async fn delete_user_allows_demoted_last_admin_path() {
        // If you have two admins, deleting one is allowed.
        let pool = setup_db().await;
        let (alice_id, _) = insert_user_with_account(&pool, "alice@x.com", "Alice", "admin").await;
        let (bob_id, _) = insert_user_with_account(&pool, "bob@x.com", "Bob", "admin").await;

        delete_user(&pool, &bob_id, Some(&alice_id), None)
            .await
            .expect("two admins, delete one should succeed");

        let remaining: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM users")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(remaining.0, 1);
    }

    #[tokio::test]
    async fn delete_user_blocks_with_future_bookings_as_host() {
        let pool = setup_db().await;
        let (alice_id, alice_account) =
            insert_user_with_account(&pool, "alice@x.com", "Alice", "admin").await;
        let (bob_id, bob_account) =
            insert_user_with_account(&pool, "bob@x.com", "Bob", "user").await;

        // Bob owns an event type with a confirmed future booking.
        let et_id = uuid::Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO event_types (id, account_id, slug, title, duration_min, buffer_before, buffer_after, min_notice_min, enabled, location_type)
             VALUES (?, ?, 'meet', 'Meet', 30, 0, 0, 0, 1, 'link')",
        )
        .bind(&et_id)
        .bind(&bob_account)
        .execute(&pool).await.unwrap();
        let booking_id = uuid::Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO bookings (id, event_type_id, uid, guest_name, guest_email, guest_timezone, cancel_token, reschedule_token, start_at, end_at, status)
             VALUES (?, ?, 'uid-future@x', 'Guest', 'g@x.com', 'UTC', ?, ?, datetime('now', '+7 days'), datetime('now', '+7 days', '+30 minutes'), 'confirmed')",
        )
        .bind(&booking_id)
        .bind(&et_id)
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(uuid::Uuid::new_v4().to_string())
        .execute(&pool).await.unwrap();

        let result = delete_user(&pool, &bob_id, Some(&alice_id), None).await;
        match result {
            Err(DeleteUserError::HasFutureBookings { count }) => assert_eq!(count, 1),
            other => panic!("expected HasFutureBookings(1), got {:?}", other),
        }

        // alice's untouched account
        let _ = alice_account;
    }

    #[tokio::test]
    async fn delete_user_blocks_with_future_bookings_as_assignee() {
        // Bob is the round-robin assignee on a future booking under a team
        // event type that *alice* owns. The host_future query won't match
        // because alice owns the account; the assigned_future query is what
        // catches it. Locks in the second branch of the count.
        let pool = setup_db().await;
        let (alice_id, alice_account) =
            insert_user_with_account(&pool, "alice@x.com", "Alice", "admin").await;
        let (bob_id, _) = insert_user_with_account(&pool, "bob@x.com", "Bob", "user").await;

        // Alice owns the team event type.
        let et_id = uuid::Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO event_types (id, account_id, slug, title, duration_min, buffer_before, buffer_after, min_notice_min, enabled, location_type)
             VALUES (?, ?, 'team-meet', 'Team Meet', 30, 0, 0, 0, 1, 'link')",
        )
        .bind(&et_id)
        .bind(&alice_account)
        .execute(&pool).await.unwrap();
        // Future booking with bob as the assigned member.
        let booking_id = uuid::Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO bookings (id, event_type_id, uid, guest_name, guest_email, guest_timezone, cancel_token, reschedule_token, start_at, end_at, status, assigned_user_id)
             VALUES (?, ?, 'uid-assigned@x', 'Guest', 'g@x.com', 'UTC', ?, ?, datetime('now', '+7 days'), datetime('now', '+7 days', '+30 minutes'), 'confirmed', ?)",
        )
        .bind(&booking_id)
        .bind(&et_id)
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(&bob_id)
        .execute(&pool).await.unwrap();

        let result = delete_user(&pool, &bob_id, Some(&alice_id), None).await;
        match result {
            Err(DeleteUserError::HasFutureBookings { count }) => assert_eq!(count, 1),
            other => panic!("expected HasFutureBookings(1), got {:?}", other),
        }
    }

    #[tokio::test]
    async fn delete_user_ignores_past_bookings() {
        let pool = setup_db().await;
        let (alice_id, _) = insert_user_with_account(&pool, "alice@x.com", "Alice", "admin").await;
        let (bob_id, bob_account) =
            insert_user_with_account(&pool, "bob@x.com", "Bob", "user").await;

        // Bob's event type has only a *past* booking; that should not block.
        let et_id = uuid::Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO event_types (id, account_id, slug, title, duration_min, buffer_before, buffer_after, min_notice_min, enabled, location_type)
             VALUES (?, ?, 'meet', 'Meet', 30, 0, 0, 0, 1, 'link')",
        )
        .bind(&et_id)
        .bind(&bob_account)
        .execute(&pool).await.unwrap();
        let booking_id = uuid::Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO bookings (id, event_type_id, uid, guest_name, guest_email, guest_timezone, cancel_token, reschedule_token, start_at, end_at, status)
             VALUES (?, ?, 'uid-past@x', 'Guest', 'g@x.com', 'UTC', ?, ?, datetime('now', '-7 days'), datetime('now', '-7 days', '+30 minutes'), 'confirmed')",
        )
        .bind(&booking_id)
        .bind(&et_id)
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(uuid::Uuid::new_v4().to_string())
        .execute(&pool).await.unwrap();

        delete_user(&pool, &bob_id, Some(&alice_id), None)
            .await
            .expect("past bookings must not block deletion");
    }

    #[tokio::test]
    async fn delete_user_cascades_account_subtree_and_invites() {
        // The full happy path: a user with an account, an event type, a past
        // booking, an availability rule, AND an invite they authored on
        // *another* user's event type. After deletion: the user, their
        // account subtree, and their cross-owned invite are all gone, while
        // the other user's account and event type are untouched.
        let pool = setup_db().await;
        let (alice_id, alice_account) =
            insert_user_with_account(&pool, "alice@x.com", "Alice", "admin").await;
        let (bob_id, bob_account) =
            insert_user_with_account(&pool, "bob@x.com", "Bob", "user").await;

        // Bob's own event type with a past booking + an availability rule.
        let bob_et = uuid::Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO event_types (id, account_id, slug, title, duration_min, buffer_before, buffer_after, min_notice_min, enabled, location_type)
             VALUES (?, ?, 'bob-meet', 'Bob Meet', 30, 0, 0, 0, 1, 'link')",
        )
        .bind(&bob_et)
        .bind(&bob_account)
        .execute(&pool).await.unwrap();
        sqlx::query(
            "INSERT INTO bookings (id, event_type_id, uid, guest_name, guest_email, guest_timezone, cancel_token, reschedule_token, start_at, end_at, status)
             VALUES (?, ?, 'uid-past@x', 'Guest', 'g@x.com', 'UTC', ?, ?, datetime('now', '-7 days'), datetime('now', '-7 days', '+30 minutes'), 'confirmed')",
        )
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(&bob_et)
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(uuid::Uuid::new_v4().to_string())
        .execute(&pool).await.unwrap();
        sqlx::query(
            "INSERT INTO availability_rules (id, event_type_id, day_of_week, start_time, end_time) VALUES (?, ?, 1, '09:00', '17:00')",
        )
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(&bob_et)
        .execute(&pool).await.unwrap();

        // Alice owns an internal event type that Bob created an invite on.
        let alice_et = uuid::Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO event_types (id, account_id, slug, title, duration_min, buffer_before, buffer_after, min_notice_min, enabled, location_type, visibility)
             VALUES (?, ?, 'alice-meet', 'Alice Meet', 30, 0, 0, 0, 1, 'link', 'internal')",
        )
        .bind(&alice_et)
        .bind(&alice_account)
        .execute(&pool).await.unwrap();
        sqlx::query(
            "INSERT INTO booking_invites (id, event_type_id, token, guest_name, guest_email, max_uses, used_count, created_by_user_id)
             VALUES (?, ?, 'tok-xyz', '', '', 1, 0, ?)",
        )
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(&alice_et)
        .bind(&bob_id)
        .execute(&pool).await.unwrap();

        delete_user(&pool, &bob_id, Some(&alice_id), None)
            .await
            .expect("delete must succeed");

        // Bob and his account/event_types/bookings are gone.
        let bob_count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM users WHERE id = ?")
            .bind(&bob_id)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(bob_count.0, 0, "user row should be deleted");
        let bob_account_count: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM accounts WHERE user_id IS ?")
                .bind(&bob_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(bob_account_count.0, 0, "account should be deleted");
        let bob_et_count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM event_types WHERE id = ?")
            .bind(&bob_et)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(bob_et_count.0, 0, "bob's event type should cascade-delete");

        // Bob's invite on alice's event type is gone, but alice's event type stays.
        let invite_count: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM booking_invites WHERE token = 'tok-xyz'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(invite_count.0, 0, "bob-authored invite should be deleted");
        let alice_et_count: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM event_types WHERE id = ?")
                .bind(&alice_et)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(alice_et_count.0, 1, "alice's event type must be untouched");

        // Alice still exists.
        let alice_count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM users WHERE id = ?")
            .bind(&alice_id)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(alice_count.0, 1);
    }

    // --- Last-admin guard ---

    async fn enabled_admins(pool: &SqlitePool) -> i64 {
        sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE role = 'admin' AND enabled = 1")
            .fetch_one(pool)
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn last_enabled_admin_cannot_be_demoted_or_disabled() {
        let pool = setup_db().await;
        let alice = insert_user(&pool, "alice@x.com", "Alice", "admin").await;

        assert_eq!(
            set_user_admin(&pool, &alice, false).await.unwrap(),
            AdminChange::LastAdmin
        );
        assert_eq!(
            set_user_enabled(&pool, &alice, false).await.unwrap(),
            AdminChange::LastAdmin
        );
        assert_eq!(enabled_admins(&pool).await, 1);

        // A second enabled admin lifts the guard, for either one.
        let bob = insert_user(&pool, "bob@x.com", "Bob", "user").await;
        assert_eq!(
            set_user_admin(&pool, &bob, true).await.unwrap(),
            AdminChange::Changed
        );
        assert_eq!(
            set_user_admin(&pool, &alice, false).await.unwrap(),
            AdminChange::Changed
        );
        assert_eq!(
            set_user_enabled(&pool, &bob, false).await.unwrap(),
            AdminChange::LastAdmin,
            "bob is now the only admin"
        );
        assert_eq!(
            set_user_admin(&pool, "missing", false).await.unwrap(),
            AdminChange::NotFound
        );
    }

    /// A disabled admin cannot sign in, so it does not count as the admin
    /// the instance keeps. Demoting, disabling or deleting a disabled admin
    /// is always fine.
    #[tokio::test]
    async fn a_disabled_admin_does_not_count_as_another_admin() {
        let pool = setup_db().await;
        let (alice, _) = insert_user_with_account(&pool, "alice@x.com", "Alice", "admin").await;
        let (bob, _) = insert_user_with_account(&pool, "bob@x.com", "Bob", "admin").await;
        sqlx::query("UPDATE users SET enabled = 0 WHERE id = ?")
            .bind(&bob)
            .execute(&pool)
            .await
            .unwrap();

        assert_eq!(
            set_user_admin(&pool, &alice, false).await.unwrap(),
            AdminChange::LastAdmin
        );
        assert_eq!(
            set_user_enabled(&pool, &alice, false).await.unwrap(),
            AdminChange::LastAdmin
        );
        assert!(matches!(
            delete_user(&pool, &alice, None, None).await,
            Err(DeleteUserError::LastAdmin)
        ));

        assert_eq!(
            set_user_admin(&pool, &bob, false).await.unwrap(),
            AdminChange::Changed
        );
        delete_user(&pool, &bob, Some(&alice), None)
            .await
            .expect("a disabled former admin can be deleted");
        assert_eq!(enabled_admins(&pool).await, 1);
    }

    /// Two admins demoting (or disabling) each other at the same moment:
    /// each check passes on its own, but the guard is inside the UPDATE, so
    /// at most one of them may go through. Run against a WAL file database,
    /// as in production, several times to give the race a chance.
    #[tokio::test]
    async fn concurrent_demotions_keep_an_enabled_admin() {
        let dir = std::env::temp_dir().join(format!("calrs_last_admin_{}", uuid::Uuid::new_v4()));
        let pool = crate::db::connect(&dir).await.unwrap();
        crate::db::migrate(&pool).await.unwrap();
        let mut ids = Vec::new();
        for name in ["alice", "bob"] {
            let id = uuid::Uuid::new_v4().to_string();
            sqlx::query(
                "INSERT INTO users (id, email, name, role, auth_provider, username, enabled) \
                 VALUES (?, ?, ?, 'admin', 'local', ?, 1)",
            )
            .bind(&id)
            .bind(format!("{name}@x.com"))
            .bind(name)
            .bind(name)
            .execute(&pool)
            .await
            .unwrap();
            ids.push(id);
        }

        for round in 0..20 {
            sqlx::query("UPDATE users SET role = 'admin', enabled = 1")
                .execute(&pool)
                .await
                .unwrap();
            let disable = round % 2 == 1;
            let tasks: Vec<_> = ids
                .iter()
                .map(|id| {
                    let pool = pool.clone();
                    let id = id.clone();
                    tokio::spawn(async move {
                        if disable {
                            set_user_enabled(&pool, &id, false).await
                        } else {
                            set_user_admin(&pool, &id, false).await
                        }
                    })
                })
                .collect();
            let mut changed = 0;
            for task in tasks {
                // A write that loses the race may also come back as a busy
                // error: either way it did not apply.
                if let Ok(AdminChange::Changed) = task.await.unwrap() {
                    changed += 1;
                }
            }
            assert!(changed <= 1, "round {round}: both changes applied");
            assert_eq!(
                enabled_admins(&pool).await,
                2 - changed,
                "round {round}: an enabled admin must remain"
            );
        }
        pool.close().await;
        let _ = std::fs::remove_dir_all(&dir);
    }
}
