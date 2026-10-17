//! Small first-party OAuth authorization server, independent of MCP's dispatcher.
//! Public clients, S256 PKCE, explicit consent, opaque hashed credentials and
//! rotating refresh tokens. Uses the existing local/SSO login and public URL.

use axum::{
    extract::{DefaultBodyLimit, Form, Query, State},
    http::{HeaderMap, StatusCode},
    response::{Html, IntoResponse, Redirect, Response},
    routing::{get, post},
    Json, Router,
};
use axum_extra::extract::cookie::CookieJar;
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use chrono::Utc;
use rand::RngCore;
use reqwest::Url;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::{FromRow, SqlitePool};
use std::sync::Arc;

use crate::{auth, models::User, web::AppState};

pub const SCOPES: [&str; 3] = ["event-types:read", "calendar:read", "invites:write"];
const CONTINUE_COOKIE: &str = "__Host-calrs_oauth";
const ACCESS_LIFETIME: i64 = 3600;
const GRANT_LIFETIME: i64 = 30 * 86400;

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/.well-known/oauth-authorization-server", get(metadata))
        .route(
            "/.well-known/oauth-protected-resource",
            get(resource_metadata),
        )
        .route(
            "/.well-known/oauth-protected-resource/mcp",
            get(resource_metadata),
        )
        .route("/oauth/register", post(register))
        .route("/oauth/authorize", get(authorize))
        .route("/oauth/consent", get(consent_page).post(consent))
        .route("/oauth/token", post(token))
        .route("/oauth/revoke", post(revoke))
        .layer(DefaultBodyLimit::max(16 * 1024))
}

pub fn issuer() -> String {
    crate::settings::base_url()
        .unwrap_or_else(|| "http://localhost:3000".to_string())
        .trim_end_matches('/')
        .to_string()
}

pub fn resource() -> String {
    format!("{}/mcp", issuer())
}

async fn metadata() -> Json<serde_json::Value> {
    let base = issuer();
    Json(json!({
        "issuer": base,
        "authorization_endpoint": format!("{base}/oauth/authorize"),
        "token_endpoint": format!("{base}/oauth/token"),
        "registration_endpoint": format!("{base}/oauth/register"),
        "revocation_endpoint": format!("{base}/oauth/revoke"),
        "response_types_supported": ["code"],
        "grant_types_supported": ["authorization_code", "refresh_token"],
        "token_endpoint_auth_methods_supported": ["none"],
        "revocation_endpoint_auth_methods_supported": ["none"],
        "code_challenge_methods_supported": ["S256"],
        "scopes_supported": SCOPES,
        "client_id_metadata_document_supported": false
    }))
}

async fn resource_metadata() -> Json<serde_json::Value> {
    Json(
        json!({"resource": resource(), "authorization_servers": [issuer()],
        "scopes_supported": SCOPES, "bearer_methods_supported": ["header"]}),
    )
}

#[derive(Debug)]
pub struct Error(StatusCode, &'static str);

impl IntoResponse for Error {
    fn into_response(self) -> Response {
        (
            self.0,
            [("Cache-Control", "no-store")],
            Json(json!({"error": self.1})),
        )
            .into_response()
    }
}

impl From<sqlx::Error> for Error {
    fn from(error: sqlx::Error) -> Self {
        tracing::error!(%error, "OAuth database operation failed");
        Self(StatusCode::INTERNAL_SERVER_ERROR, "server_error")
    }
}

fn invalid(error: &'static str) -> Error {
    Error(StatusCode::BAD_REQUEST, error)
}

pub(crate) fn random_token() -> String {
    let mut bytes = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

pub(crate) fn hash(token: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(token.as_bytes()))
}

fn cookie(value: &str, max_age: i64) -> String {
    format!("{CONTINUE_COOKIE}={value}; HttpOnly; Secure; SameSite=Lax; Path=/; Max-Age={max_age}")
}

/// A fixed local destination, never a user-supplied redirect. The opaque cookie
/// survives both password and OIDC login; only the consent POST grants access.
pub fn login_destination(jar: &CookieJar) -> &'static str {
    if jar.get(CONTINUE_COOKIE).is_some() {
        "/oauth/consent"
    } else {
        "/dashboard"
    }
}

async fn real_user(pool: &SqlitePool, jar: &CookieJar) -> Option<User> {
    auth::validate_session(pool, jar.get(auth::SESSION_COOKIE)?.value()).await
}

async fn rate_limit(state: &AppState, headers: &HeaderMap, operation: &str) -> Result<(), Error> {
    let key = format!(
        "oauth:{operation}:{}",
        crate::web::client_ip_for_rate_limit(headers)
    );
    if state.login_limiter.check_limited(&key).await {
        return Err(Error(
            StatusCode::TOO_MANY_REQUESTS,
            "temporarily_unavailable",
        ));
    }
    Ok(())
}

#[derive(Deserialize)]
struct Registration {
    redirect_uris: Vec<String>,
    client_name: Option<String>,
    token_endpoint_auth_method: Option<String>,
    grant_types: Option<Vec<String>>,
    response_types: Option<Vec<String>>,
}

fn valid_redirect(uri: &str) -> bool {
    let Ok(url) = Url::parse(uri) else {
        return false;
    };
    uri.len() <= 2048
        && url.host_str().is_some()
        && url.username().is_empty()
        && url.password().is_none()
        && url.fragment().is_none()
        && (url.scheme() == "https"
            || (url.scheme() == "http"
                && matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "[::1]"))))
        && !url
            .query_pairs()
            .any(|(k, _)| matches!(k.as_ref(), "code" | "state" | "error" | "iss"))
}

async fn register(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(form): Json<Registration>,
) -> Result<Response, Error> {
    rate_limit(&state, &headers, "register").await?;
    if form.redirect_uris.is_empty()
        || form.redirect_uris.len() > 10
        || !form.redirect_uris.iter().all(|u| valid_redirect(u))
    {
        return Err(invalid("invalid_redirect_uri"));
    }
    if form.token_endpoint_auth_method.as_deref().unwrap_or("none") != "none"
        || form.grant_types.as_ref().is_some_and(|v| {
            v.is_empty()
                || v.iter()
                    .any(|g| g != "authorization_code" && g != "refresh_token")
        })
        || form.response_types.as_ref().is_some_and(|v| v != &["code"])
    {
        return Err(invalid("invalid_client_metadata"));
    }
    let name = form.client_name.as_deref().unwrap_or("MCP client").trim();
    if name.is_empty() || name.len() > 200 || name.chars().any(char::is_control) {
        return Err(invalid("invalid_client_metadata"));
    }
    let id = random_token();
    let now = Utc::now().timestamp();
    let inserted = sqlx::query("INSERT INTO oauth_clients (id, name, redirect_uris, created_at) SELECT ?, ?, ?, ? WHERE (SELECT COUNT(*) FROM oauth_clients) < 10000")
        .bind(&id).bind(name).bind(json!(form.redirect_uris).to_string()).bind(now).execute(&state.pool).await?;
    if inserted.rows_affected() != 1 {
        return Err(Error(
            StatusCode::SERVICE_UNAVAILABLE,
            "temporarily_unavailable",
        ));
    }
    Ok((
        StatusCode::CREATED,
        [("Cache-Control", "no-store")],
        Json(json!({
            "client_id": id, "client_id_issued_at": now, "client_name": name,
            "redirect_uris": form.redirect_uris, "token_endpoint_auth_method": "none",
            "grant_types": ["authorization_code", "refresh_token"], "response_types": ["code"]
        })),
    )
        .into_response())
}

#[derive(Deserialize, Serialize)]
struct Authorization {
    client_id: String,
    redirect_uri: String,
    response_type: String,
    code_challenge: String,
    code_challenge_method: String,
    resource: String,
    scope: Option<String>,
    state: Option<String>,
}

async fn authorize(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(mut params): Query<Authorization>,
) -> Result<Response, Error> {
    rate_limit(&state, &headers, "authorize").await?;
    let uris: Option<String> =
        sqlx::query_scalar("SELECT redirect_uris FROM oauth_clients WHERE id = ?")
            .bind(&params.client_id)
            .fetch_optional(&state.pool)
            .await?;
    let uris: Vec<String> = uris
        .and_then(|v| serde_json::from_str(&v).ok())
        .ok_or(invalid("invalid_client"))?;
    if !uris.contains(&params.redirect_uri) {
        return Err(invalid("invalid_request"));
    }
    if params.response_type != "code" {
        return Err(invalid("unsupported_response_type"));
    }
    if params.resource != resource() {
        return Err(invalid("invalid_target"));
    }
    if params.code_challenge_method != "S256"
        || params.code_challenge.len() != 43
        || URL_SAFE_NO_PAD.decode(&params.code_challenge).is_err()
        || params.state.as_ref().is_some_and(|v| v.len() > 2048)
    {
        return Err(invalid("invalid_request"));
    }
    let requested = params.scope.clone().unwrap_or_else(|| SCOPES.join(" "));
    let scopes: Vec<&str> = requested.split_whitespace().collect();
    if scopes.is_empty() || scopes.iter().any(|s| !SCOPES.contains(s)) {
        return Err(invalid("invalid_scope"));
    }
    params.scope = Some(scopes.join(" "));
    let id = random_token();
    sqlx::query(
        "INSERT INTO oauth_requests (id, client_id, parameters, expires_at) VALUES (?, ?, ?, ?)",
    )
    .bind(hash(&id))
    .bind(&params.client_id)
    .bind(json!(params).to_string())
    .bind(Utc::now().timestamp() + 600)
    .execute(&state.pool)
    .await?;
    Ok((
        [
            ("Set-Cookie", cookie(&id, 600)),
            ("Cache-Control", "no-store".into()),
        ],
        Redirect::to("/oauth/consent"),
    )
        .into_response())
}

async fn consent_page(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    jar: CookieJar,
) -> Result<Response, Error> {
    let Some(user) = real_user(&state.pool, &jar).await else {
        return Ok(Redirect::to("/auth/login").into_response());
    };
    let id = jar
        .get(CONTINUE_COOKIE)
        .ok_or(invalid("invalid_request"))?
        .value();
    let row: Option<(String, String)> = sqlx::query_as("SELECT r.parameters, c.name FROM oauth_requests r JOIN oauth_clients c ON c.id = r.client_id WHERE r.id = ? AND r.expires_at > ?")
        .bind(hash(id)).bind(Utc::now().timestamp()).fetch_optional(&state.pool).await?;
    let (parameters, name) = row.ok_or(invalid("invalid_request"))?;
    let params: Authorization =
        serde_json::from_str(&parameters).map_err(|_| invalid("invalid_request"))?;
    sqlx::query("UPDATE oauth_requests SET user_id = ? WHERE id = ?")
        .bind(&user.id)
        .bind(hash(id))
        .execute(&state.pool)
        .await?;
    let scopes = params.scope.as_deref().unwrap_or("");
    let csrf = crate::web::generate_csrf_token();
    let lang = crate::i18n::resolve(user.language.as_deref(), &headers);
    let body = state.templates.get_template("auth/consent.html")
        .and_then(|t| t.render(minijinja::context! {
            lang => lang, email => user.email, client_name => name, redirect_uri => params.redirect_uri,
            events => scopes.split_whitespace().any(|s| s == SCOPES[0]),
            blocks => scopes.split_whitespace().any(|s| s == SCOPES[1]),
            invites => scopes.split_whitespace().any(|s| s == SCOPES[2]),
            csrf_token => csrf, request_id => id,
        })).map_err(|e| { tracing::error!(%e, "OAuth consent template"); Error(StatusCode::INTERNAL_SERVER_ERROR, "server_error") })?;
    Ok((
        [
            ("Set-Cookie", crate::web::csrf_cookie_value(&csrf)),
            ("Cache-Control", "no-store".into()),
            ("Referrer-Policy", "no-referrer".into()),
        ],
        Html(body),
    )
        .into_response())
}

#[derive(Deserialize)]
struct Consent {
    _csrf: Option<String>,
    request_id: String,
    decision: String,
}

async fn consent(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    jar: CookieJar,
    Form(form): Form<Consent>,
) -> Result<Response, Error> {
    if let Err(response) = crate::web::verify_csrf_token(&headers, &form._csrf) {
        return Ok(response);
    }
    let user = real_user(&state.pool, &jar)
        .await
        .ok_or(Error(StatusCode::UNAUTHORIZED, "login_required"))?;
    let id = jar
        .get(CONTINUE_COOKIE)
        .ok_or(invalid("invalid_request"))?
        .value();
    if id != form.request_id || !matches!(form.decision.as_str(), "allow" | "deny") {
        return Err(invalid("invalid_request"));
    }
    let mut tx = state.pool.begin().await?;
    let parameters: Option<String> = sqlx::query_scalar("DELETE FROM oauth_requests WHERE id = ? AND user_id = ? AND expires_at > ? RETURNING parameters")
        .bind(hash(id)).bind(&user.id).bind(Utc::now().timestamp()).fetch_optional(&mut *tx).await?;
    let params: Authorization =
        serde_json::from_str(&parameters.ok_or(invalid("invalid_request"))?)
            .map_err(|_| invalid("invalid_request"))?;
    let mut redirect = Url::parse(&params.redirect_uri).map_err(|_| invalid("invalid_request"))?;
    if let Some(s) = &params.state {
        redirect.query_pairs_mut().append_pair("state", s);
    }
    redirect.query_pairs_mut().append_pair("iss", &issuer());
    if form.decision == "deny" {
        redirect
            .query_pairs_mut()
            .append_pair("error", "access_denied");
    } else {
        let grant = random_token();
        let code = random_token();
        let now = Utc::now().timestamp();
        sqlx::query("INSERT INTO oauth_grants (id, client_id, user_id, resource, scope, expires_at) VALUES (?, ?, ?, ?, ?, ?)")
            .bind(&grant).bind(&params.client_id).bind(&user.id).bind(&params.resource).bind(params.scope.as_deref().unwrap_or(""))
            .bind(now + GRANT_LIFETIME).execute(&mut *tx).await?;
        sqlx::query("INSERT INTO oauth_credentials (hash, grant_id, kind, expires_at, redirect_uri, challenge) VALUES (?, ?, 'code', ?, ?, ?)")
            .bind(hash(&code)).bind(&grant).bind(now + 120).bind(&params.redirect_uri).bind(&params.code_challenge).execute(&mut *tx).await?;
        redirect.query_pairs_mut().append_pair("code", &code);
    }
    tx.commit().await?;
    Ok((
        [
            ("Set-Cookie", cookie("", 0)),
            ("Cache-Control", "no-store".into()),
            ("Referrer-Policy", "no-referrer".into()),
        ],
        Redirect::to(redirect.as_str()),
    )
        .into_response())
}

#[derive(Deserialize)]
struct TokenRequest {
    grant_type: String,
    client_id: String,
    code: Option<String>,
    code_verifier: Option<String>,
    redirect_uri: Option<String>,
    refresh_token: Option<String>,
    resource: String,
    scope: Option<String>,
}

#[derive(FromRow)]
struct Credential {
    grant_id: String,
    used: bool,
    redirect_uri: Option<String>,
    challenge: Option<String>,
    scope: String,
    expires_at: i64,
}

async fn token(
    State(state): State<Arc<AppState>>,
    Form(form): Form<TokenRequest>,
) -> Result<Response, Error> {
    let (kind, value) = match form.grant_type.as_str() {
        "authorization_code" => ("code", form.code.as_deref()),
        "refresh_token" => ("refresh", form.refresh_token.as_deref()),
        _ => return Err(invalid("unsupported_grant_type")),
    };
    let value = value.ok_or(invalid("invalid_request"))?;
    if form.resource != resource() {
        return Err(invalid("invalid_target"));
    }
    let now = Utc::now().timestamp();
    let mut tx = state.pool.begin().await?;
    // Write first: serializes redemption across connections/processes before
    // reading used, avoiding deferred SQLite read-to-write upgrade races.
    sqlx::query("UPDATE oauth_credentials SET used = used WHERE hash = ?")
        .bind(hash(value))
        .execute(&mut *tx)
        .await?;
    let row: Option<Credential> = sqlx::query_as("SELECT c.grant_id, c.used, c.redirect_uri, c.challenge, g.scope, g.expires_at FROM oauth_credentials c JOIN oauth_grants g ON g.id = c.grant_id JOIN users u ON u.id = g.user_id WHERE c.hash = ? AND c.kind = ? AND c.expires_at > ? AND g.expires_at > ? AND g.revoked = 0 AND g.client_id = ? AND g.resource = ? AND u.enabled = 1")
        .bind(hash(value)).bind(kind).bind(now).bind(now).bind(&form.client_id).bind(&form.resource).fetch_optional(&mut *tx).await?;
    let row = row.ok_or(invalid("invalid_grant"))?;
    if kind == "code" {
        let verifier = form
            .code_verifier
            .as_deref()
            .ok_or(invalid("invalid_grant"))?;
        if !(43..=128).contains(&verifier.len())
            || !verifier
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || b"-._~".contains(&c))
            || row.challenge.as_deref() != Some(hash(verifier).as_str())
            || row.redirect_uri != form.redirect_uri
        {
            return Err(invalid("invalid_grant"));
        }
    }
    if row.used {
        // Refresh/code replay invalidates the entire family, including access
        // tokens already minted by whoever won the first redemption.
        sqlx::query("UPDATE oauth_grants SET revoked = 1 WHERE id = ?")
            .bind(&row.grant_id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        return Err(invalid("invalid_grant"));
    }
    let scope = match form.scope.as_deref() {
        Some(requested) => {
            let granted: std::collections::BTreeSet<_> = row.scope.split_whitespace().collect();
            let requested: std::collections::BTreeSet<_> = requested.split_whitespace().collect();
            if requested.is_empty() || !requested.is_subset(&granted) {
                return Err(invalid("invalid_scope"));
            }
            requested.into_iter().collect::<Vec<_>>().join(" ")
        }
        None => row.scope.clone(),
    };
    sqlx::query("UPDATE oauth_credentials SET used = 1 WHERE hash = ?")
        .bind(hash(value))
        .execute(&mut *tx)
        .await?;
    let access = random_token();
    let refresh = random_token();
    let access_expiry = (now + ACCESS_LIFETIME).min(row.expires_at);
    for (secret, kind, expiry) in [
        (&access, "access", access_expiry),
        (&refresh, "refresh", row.expires_at),
    ] {
        sqlx::query(
            "INSERT INTO oauth_credentials (hash, grant_id, kind, expires_at, scope) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(hash(secret))
        .bind(&row.grant_id)
        .bind(kind)
        .bind(expiry)
        // A rotated refresh token retains the original grant's scope (RFC
        // 6749 section 6). Only this access token receives the requested subset.
        .bind((kind == "access").then_some(&scope))
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok((
        [("Cache-Control", "no-store"), ("Pragma", "no-cache")],
        Json(json!({
            "access_token": access, "token_type": "Bearer", "expires_in": access_expiry - now,
            "refresh_token": refresh, "scope": scope
        })),
    )
        .into_response())
}

#[derive(Deserialize)]
struct Revocation {
    client_id: String,
    token: String,
}

async fn revoke(
    State(state): State<Arc<AppState>>,
    Form(form): Form<Revocation>,
) -> Result<Response, Error> {
    sqlx::query("UPDATE oauth_grants SET revoked = 1 WHERE client_id = ? AND id IN (SELECT grant_id FROM oauth_credentials WHERE hash = ? AND kind IN ('access', 'refresh'))")
        .bind(&form.client_id).bind(hash(&form.token)).execute(&state.pool).await?;
    Ok((StatusCode::OK, [("Cache-Control", "no-store")]).into_response())
}

/// Revalidates the user on every call, so disabling/deleting an account takes
/// effect immediately. Browser session cookies and upstream tokens never work.
pub async fn bearer_user(
    pool: &SqlitePool,
    headers: &HeaderMap,
    audience: &str,
) -> Result<Option<(User, String)>, Error> {
    let Some(value) = headers.get("authorization").and_then(|h| h.to_str().ok()) else {
        return Ok(None);
    };
    let Some((scheme, token)) = value.split_once(' ') else {
        return Ok(None);
    };
    if !scheme.eq_ignore_ascii_case("bearer") || token.len() != 43 {
        return Ok(None);
    }
    let row: Option<(String, String, String)> = sqlx::query_as("SELECT g.user_id, COALESCE(c.scope, g.scope), g.scope FROM oauth_credentials c JOIN oauth_grants g ON g.id = c.grant_id WHERE c.hash = ? AND c.kind = 'access' AND c.expires_at > ? AND g.expires_at > ? AND g.revoked = 0 AND g.resource = ?")
        .bind(hash(token)).bind(Utc::now().timestamp()).bind(Utc::now().timestamp()).bind(audience).fetch_optional(pool).await?;
    let Some((id, scope, granted)) = row else {
        return Ok(None);
    };
    let scope = scope
        .split_whitespace()
        .filter(|s| granted.split_whitespace().any(|g| g == *s))
        .collect::<Vec<_>>()
        .join(" ");
    let user = sqlx::query_as::<_, User>("SELECT * FROM users WHERE id = ? AND enabled = 1")
        .bind(id)
        .fetch_optional(pool)
        .await?;
    Ok(user.map(|u| (u, scope)))
}

pub async fn cleanup(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    let now = Utc::now().timestamp();
    sqlx::query("DELETE FROM oauth_requests WHERE expires_at <= ?")
        .bind(now)
        .execute(pool)
        .await?;
    sqlx::query("DELETE FROM oauth_grants WHERE expires_at <= ?")
        .bind(now)
        .execute(pool)
        .await?;
    sqlx::query("DELETE FROM oauth_credentials WHERE kind = 'access' AND expires_at <= ?")
        .bind(now)
        .execute(pool)
        .await?;
    // Retain spent refresh credentials until their grant expires (replay detection).
    sqlx::query("DELETE FROM oauth_clients WHERE created_at < ? AND NOT EXISTS (SELECT 1 FROM oauth_grants WHERE client_id = oauth_clients.id) AND NOT EXISTS (SELECT 1 FROM oauth_requests WHERE client_id = oauth_clients.id)")
        .bind(now - GRANT_LIFETIME).execute(pool).await?;
    Ok(())
}
