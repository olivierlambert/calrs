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
use sqlx::{FromRow, SqliteConnection, SqlitePool};
use std::sync::Arc;

use crate::{auth, models::User, web::AppState};

/// The scopes of the MCP resource (`resource()`).
pub const SCOPES: [&str; 3] = ["event-types:read", "calendar:read", "invites:write"];
/// The scopes of the REST API resource (`api_resource()`, #169). A grant is
/// bound to one resource, so `event-types:read` of an MCP grant never opens
/// the API, nor the other way round.
pub const API_SCOPES: [&str; 4] = [
    "event-types:read",
    "slots:read",
    "bookings:read",
    "bookings:write",
];
const CONTINUE_COOKIE: &str = "__Host-calrs_oauth";
const ACCESS_LIFETIME: i64 = 3600;
const GRANT_LIFETIME: i64 = 30 * 86400;
/// A registered client that never obtained a grant is dropped after 30 days:
/// long enough for a user who denied, or gave up during login or MFA setup,
/// to retry with the same client; abuse is bounded by the per-address cap.
const UNUSED_CLIENT_LIFETIME: i64 = 30 * 86400;
/// A client once granted is kept this long after its last grant, so an MCP
/// client whose grant was revoked or expired can simply authorize again.
const GRANTED_CLIENT_LIFETIME: i64 = 180 * 86400;
/// Clients never granted, registered from one address (`rate_key`) within
/// UNUSED_CLIENT_LIFETIME, beyond which that address cannot register more.
/// Not applied to the "unknown" address (no X-Forwarded-For), which every
/// registrant would share.
const PENDING_CLIENTS_PER_ADDRESS: i64 = 50;
/// The clients cleanup and the cap may remove, as a condition on `c`, binding
/// now, now - UNUSED_CLIENT_LIFETIME and now - GRANTED_CLIENT_LIFETIME: no
/// grant left, no authorization a signed-in user is looking at, and either
/// never granted and over 30 days old, or last granted over 180 days ago.
const RECLAIMABLE_CLIENT: &str = "NOT EXISTS (SELECT 1 FROM oauth_grants g WHERE g.client_id = c.id)
    AND NOT EXISTS (SELECT 1 FROM oauth_requests r WHERE r.client_id = c.id AND r.user_id IS NOT NULL AND r.expires_at > ?)
    AND ((c.last_granted_at IS NULL AND c.created_at < ?) OR c.last_granted_at < ?)";
/// Registered clients kept at most (`register` reclaims unused ones).
const CLIENT_CAP: i64 = 10_000;
/// The local MFA policy of `auth::validate_session`, applied to a grant `g` of
/// user `u`: a token is worth what the session that approved it was worth, and
/// stops working when the admin starts requiring MFA it never passed.
pub(crate) const GRANT_MFA_POLICY: &str = "(u.auth_provider != 'local'
    OR (SELECT mfa_required FROM auth_config WHERE id = 'singleton') = 0
    OR (g.mfa_verified = 1 AND EXISTS (SELECT 1 FROM user_mfa WHERE user_id = u.id)))";

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/.well-known/oauth-authorization-server", get(metadata))
        // RFC 9728 section 3.1: the metadata of resource `<base>/mcp` lives at
        // the path-suffixed URL, which the 401 challenge also names. The root
        // URL would describe the resource `<base>` itself, which calrs does
        // not serve, so it is not answered.
        .route(
            "/.well-known/oauth-protected-resource/mcp",
            get(resource_metadata),
        )
        .route(
            "/.well-known/oauth-protected-resource/api/v1",
            get(api_resource_metadata),
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

/// The REST API resource (#169): `<base>/api/v1`.
pub fn api_resource() -> String {
    format!("{}/api/v1", issuer())
}

/// The scopes a grant for `resource` may carry, or `None` for a resource
/// calrs does not serve.
fn resource_scopes(resource: &str) -> Option<&'static [&'static str]> {
    if resource == self::resource() {
        Some(&SCOPES)
    } else if resource == api_resource() {
        Some(&API_SCOPES)
    } else {
        None
    }
}

/// Every scope of every resource, once each, for the server metadata.
fn all_scopes() -> Vec<&'static str> {
    let mut all: Vec<&str> = SCOPES.to_vec();
    for scope in API_SCOPES {
        if !all.contains(&scope) {
            all.push(scope);
        }
    }
    all
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
        // RFC 9207: the consent redirect carries `iss`.
        "authorization_response_iss_parameter_supported": true,
        "scopes_supported": all_scopes(),
        "client_id_metadata_document_supported": false
    }))
}

async fn resource_metadata() -> Json<serde_json::Value> {
    Json(
        json!({"resource": resource(), "authorization_servers": [issuer()],
        "scopes_supported": SCOPES, "bearer_methods_supported": ["header"]}),
    )
}

async fn api_resource_metadata() -> Json<serde_json::Value> {
    Json(
        json!({"resource": api_resource(), "authorization_servers": [issuer()],
        "scopes_supported": API_SCOPES, "bearer_methods_supported": ["header"]}),
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

/// The rate-limit identity of a client address: an IPv6 address counts by its
/// /64, which one subscriber usually holds whole, so rotating addresses inside
/// it buys nothing. IPv4 (and IPv4-mapped IPv6) counts per address.
fn rate_key(headers: &HeaderMap) -> String {
    let ip = crate::web::client_ip_for_rate_limit(headers);
    match ip.parse::<std::net::IpAddr>() {
        Ok(std::net::IpAddr::V6(v6)) => match v6.to_ipv4_mapped() {
            Some(v4) => v4.to_string(),
            None => {
                let s = v6.segments();
                format!("{:x}:{:x}:{:x}:{:x}::/64", s[0], s[1], s[2], s[3])
            }
        },
        _ => ip,
    }
}

/// Per client address (`rate_key`). Registration gets 30 per 15 minutes
/// (one per client, but an office behind one NAT registers many);
/// authorization, token exchanges and revocations come from every connected
/// client of that office, so they get 60 per 5 minutes each.
async fn rate_limit(state: &AppState, headers: &HeaderMap, operation: &str) -> Result<(), Error> {
    let key = format!("oauth:{operation}:{}", rate_key(headers));
    let limiter = match operation {
        "register" => &state.oauth_register_limiter,
        _ => &state.oauth_limiter,
    };
    if limiter.check_limited(&key).await {
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
    // ASCII only, no whitespace or control characters anywhere: the URL parser
    // would silently strip surrounding whitespace, tabs and newlines, and
    // turn a look-alike Unicode host into punycode the consent page never shows.
    if !uri.is_ascii()
        || uri
            .bytes()
            .any(|b| b.is_ascii_whitespace() || b.is_ascii_control())
    {
        return false;
    }
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

/// Whether an authorization request's `redirect_uri` is one the client
/// registered. Both are compared as parsed URLs, so `https://app.example`
/// matches `https://app.example/` and `http://localhost:80/cb` matches
/// `http://localhost/cb`, as the browser treats them; the path and query stay
/// exact (no case folding, no percent-decoding). RFC 8252 section 7.3: a
/// loopback callback may use any port. The code goes to the requested string.
fn redirect_matches(registered: &str, requested: &str) -> bool {
    if !valid_redirect(registered) || !valid_redirect(requested) {
        return false;
    }
    let (Ok(mut registered), Ok(mut requested)) = (Url::parse(registered), Url::parse(requested))
    else {
        return false;
    };
    if registered == requested {
        return true;
    }
    let loopback = |u: &Url| {
        u.scheme() == "http" && matches!(u.host_str(), Some("localhost" | "127.0.0.1" | "[::1]"))
    };
    loopback(&registered)
        && loopback(&requested)
        && registered.set_port(None).is_ok()
        && requested.set_port(None).is_ok()
        && registered == requested
}

#[test]
fn loopback_redirect_matching_only_relaxes_the_port() {
    for host in ["127.0.0.1", "[::1]", "localhost"] {
        let registered = format!("http://{host}:4321/callback?app=calrs");
        // Dot segments resolve as in the browser, to the same callback.
        assert!(redirect_matches(
            &registered,
            &format!("http://{host}:52319/a/../callback?app=calrs")
        ));
        for port in [":52319", "", ":80"] {
            assert!(redirect_matches(
                &registered,
                &format!("http://{host}{port}/callback?app=calrs")
            ));
        }
        for requested in [
            format!("https://{host}:52319/callback?app=calrs"),
            format!("http://{host}:52319/other?app=calrs"),
            format!("http://{host}:52319/callback?app=other"),
            format!("http://{host}:52319/callback?app=calrs#fragment"),
            format!("http://user@{host}:52319/callback?app=calrs"),
        ] {
            assert!(!redirect_matches(&registered, &requested), "{requested}");
        }
    }
    for (registered, requested) in [
        (
            "http://127.0.0.1:4321/callback",
            "http://localhost:52319/callback",
        ),
        (
            "http://127.0.0.1:4321/callback",
            "http://127.0.0.2:52319/callback",
        ),
        (
            "https://example.com:4321/callback",
            "https://example.com:52319/callback",
        ),
        (
            "https://127.0.0.1:4321/callback",
            "https://127.0.0.1:52319/callback",
        ),
    ] {
        assert!(!redirect_matches(registered, requested));
        assert!(redirect_matches(registered, registered));
    }
}

#[test]
fn rate_keys_group_ipv6_by_64_and_keep_ipv4_whole() {
    let key = |ip: &str| {
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-for", ip.parse().unwrap());
        rate_key(&headers)
    };
    assert_eq!(key("2001:db8:1:2::1"), key("2001:db8:1:2:ffff:abcd:1:2"));
    assert_eq!(key("2001:db8:1:2::1"), "2001:db8:1:2::/64");
    assert_ne!(key("2001:db8:1:2::1"), key("2001:db8:1:3::1"));
    assert_eq!(key("203.0.113.7"), "203.0.113.7");
    assert_ne!(key("203.0.113.7"), key("203.0.113.8"));
    assert_eq!(key("::ffff:203.0.113.7"), "203.0.113.7");
    assert_eq!(rate_key(&HeaderMap::new()), "unknown");
}

#[test]
fn redirect_matching_keeps_path_case_and_percent_encoding() {
    // Hosts are case blind, as in the browser; nothing else is folded.
    assert!(redirect_matches(
        "https://app.example/callback",
        "https://APP.example/callback"
    ));
    assert!(redirect_matches(
        "http://localhost:4321/callback",
        "http://LOCALHOST:4321/callback"
    ));
    // Forms the parser normalizes, as clients register them.
    assert!(redirect_matches(
        "https://app.example",
        "https://app.example"
    ));
    assert!(redirect_matches(
        "https://app.example",
        "https://app.example/"
    ));
    assert!(redirect_matches(
        "http://localhost:80/cb",
        "http://localhost:80/cb"
    ));
    assert!(redirect_matches(
        "http://localhost:80/cb",
        "http://localhost/cb"
    ));
    assert!(redirect_matches(
        "https://app.example:443/cb",
        "https://app.example/cb"
    ));
    assert!(!redirect_matches(
        "https://app.example:8443/cb",
        "https://app.example/cb"
    ));
    for (registered, requested) in [
        (
            "http://127.0.0.1:4321/callback",
            "http://127.0.0.1:4321/Callback",
        ),
        (
            "http://127.0.0.1:4321/callback",
            "http://127.0.0.1:4321/%63allback",
        ),
        (
            "https://app.example/callback",
            "https://app.example/%63allback",
        ),
        (
            "https://app.example/callback?a=b",
            "https://app.example/callback?a=%62",
        ),
    ] {
        assert!(!redirect_matches(registered, requested), "{requested}");
    }
}

#[test]
fn redirect_validation_refuses_look_alikes_and_stripped_characters() {
    for uri in [
        "https://ex\u{0430}mple.com/cb",
        "https://example.com/c\u{00e9}",
        " https://example.com/cb",
        "https://example.com/cb ",
        "https://example.com/c\tb",
        "https://example.com/cb\n",
        "https://example.com/c\rb",
    ] {
        assert!(!valid_redirect(uri), "{uri:?}");
    }
    assert!(valid_redirect("https://xn--exmple-cua.com/cb"));
}

async fn register(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(form): Json<Registration>,
) -> Result<Response, Error> {
    rate_limit(&state, &headers, "register").await?;
    // Stored as registered; authorize compares parsed URLs (redirect_matches).
    if form.redirect_uris.is_empty()
        || form.redirect_uris.len() > 10
        || !form.redirect_uris.iter().all(|u| valid_redirect(u))
    {
        return Err(invalid("invalid_redirect_uri"));
    }
    // RFC 7591 section 3.2.1: the server may narrow the requested grant and
    // response types. Clients asking for more (VS Code also lists the device
    // flow) get the two supported ones back, as long as they ask for the code
    // flow at all.
    if form.token_endpoint_auth_method.as_deref().unwrap_or("none") != "none"
        || form
            .grant_types
            .as_ref()
            .is_some_and(|v| !v.iter().any(|g| g == "authorization_code"))
        || form
            .response_types
            .as_ref()
            .is_some_and(|v| !v.iter().any(|r| r == "code"))
    {
        return Err(invalid("invalid_client_metadata"));
    }
    let name = form.client_name.as_deref().unwrap_or("MCP client").trim();
    if name.is_empty() || name.len() > 200 || name.chars().any(spoofing_char) {
        return Err(invalid("invalid_client_metadata"));
    }
    let id = random_token();
    let now = Utc::now().timestamp();
    let registered_by = hash(&rate_key(&headers));
    // At the cap, the oldest reclaimable client (RECLAIMABLE_CLIENT) makes
    // room; only a cap full of clients in use refuses. One address cannot
    // hold more than PENDING_CLIENTS_PER_ADDRESS clients nobody approved.
    let mut tx = state.pool.begin().await?;
    let reclaim = format!("DELETE FROM oauth_clients WHERE id IN (SELECT c.id FROM oauth_clients c WHERE (SELECT COUNT(*) FROM oauth_clients) >= ? AND {RECLAIMABLE_CLIENT} ORDER BY COALESCE(c.last_granted_at, c.created_at), c.id LIMIT 1)");
    sqlx::query(&reclaim)
        .bind(CLIENT_CAP)
        .bind(now)
        .bind(now - UNUSED_CLIENT_LIFETIME)
        .bind(now - GRANTED_CLIENT_LIFETIME)
        .execute(&mut *tx)
        .await?;
    let pending: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM oauth_clients WHERE registered_by = ? AND last_granted_at IS NULL AND created_at > ?")
        .bind(&registered_by)
        .bind(now - UNUSED_CLIENT_LIFETIME)
        .fetch_one(&mut *tx)
        .await?;
    if pending >= PENDING_CLIENTS_PER_ADDRESS && rate_key(&headers) != "unknown" {
        return Err(Error(
            StatusCode::TOO_MANY_REQUESTS,
            "temporarily_unavailable",
        ));
    }
    let inserted = sqlx::query("INSERT INTO oauth_clients (id, name, redirect_uris, created_at, registered_by) SELECT ?, ?, ?, ?, ? WHERE (SELECT COUNT(*) FROM oauth_clients) < ?")
        .bind(&id).bind(name).bind(json!(form.redirect_uris).to_string()).bind(now).bind(&registered_by).bind(CLIENT_CAP).execute(&mut *tx).await?;
    tx.commit().await?;
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

/// Characters a client name could use to look like another one on the consent
/// page: controls, bidirectional overrides and invisible separators.
pub(crate) fn spoofing_char(c: char) -> bool {
    c.is_control()
        || matches!(c, '\u{200b}'..='\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2060}'..='\u{2069}' | '\u{feff}' | '\u{061c}' | '\u{00ad}')
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
    if !uris
        .iter()
        .any(|uri| redirect_matches(uri, &params.redirect_uri))
    {
        return Err(invalid("invalid_request"));
    }
    if params.response_type != "code" {
        return Err(invalid("unsupported_response_type"));
    }
    let Some(allowed) = resource_scopes(&params.resource) else {
        return Err(invalid("invalid_target"));
    };
    if params.code_challenge_method != "S256"
        || params.code_challenge.len() != 43
        || URL_SAFE_NO_PAD.decode(&params.code_challenge).is_err()
        || params.state.as_ref().is_some_and(|v| v.len() > 2048)
    {
        return Err(invalid("invalid_request"));
    }
    let requested = params.scope.clone().unwrap_or_else(|| allowed.join(" "));
    let scopes: Vec<&str> = requested.split_whitespace().collect();
    if scopes.is_empty() || scopes.iter().any(|s| !allowed.contains(s)) {
        // RFC 6749 section 4.1.2.1: the client and its callback are valid,
        // so the error goes back to the client rather than to the browser.
        let mut redirect =
            Url::parse(&params.redirect_uri).map_err(|_| invalid("invalid_request"))?;
        if let Some(s) = &params.state {
            redirect.query_pairs_mut().append_pair("state", s);
        }
        redirect
            .query_pairs_mut()
            .append_pair("iss", &issuer())
            .append_pair("error", "invalid_scope");
        return Ok((
            [("Cache-Control", "no-store")],
            Redirect::to(redirect.as_str()),
        )
            .into_response());
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
            // The parsed form, which `consent` redirects to: a backslash, a
            // percent-encoded or numeric host would read differently raw.
            lang => lang, email => user.email, client_name => name,
            redirect_uri => Url::parse(&params.redirect_uri).map(|u| u.as_str().to_string()).unwrap_or_default(),
            redirect_host => Url::parse(&params.redirect_uri).ok().and_then(|u| u.host_str().map(str::to_string)),
            events => scopes.split_whitespace().any(|s| s == SCOPES[0]),
            blocks => scopes.split_whitespace().any(|s| s == SCOPES[1]),
            invites => scopes.split_whitespace().any(|s| s == SCOPES[2]),
            slots => scopes.split_whitespace().any(|s| s == API_SCOPES[1]),
            bookings_read => scopes.split_whitespace().any(|s| s == API_SCOPES[2]),
            bookings_write => scopes.split_whitespace().any(|s| s == API_SCOPES[3]),
            csrf_token => csrf, request_id => id,
        })).map_err(|e| { tracing::error!(%e, "OAuth consent template"); Error(StatusCode::INTERNAL_SERVER_ERROR, "server_error") })?;
    // The approval is one click: never let another page frame it, not even
    // one of ours (the global policy allows same-origin framing).
    let csp = state
        .csp_baseline
        .replace("frame-ancestors 'self'", "frame-ancestors 'none'");
    Ok((
        [
            ("Set-Cookie", crate::web::csrf_cookie_value(&csrf)),
            ("Cache-Control", "no-store".into()),
            ("Referrer-Policy", "no-referrer".into()),
            ("Content-Security-Policy", csp),
            ("X-Frame-Options", "DENY".into()),
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
        let session = jar
            .get(auth::SESSION_COOKIE)
            .map(|c| c.value())
            .unwrap_or("");
        sqlx::query("INSERT INTO oauth_grants (id, client_id, user_id, resource, scope, expires_at, mfa_verified, created_at) VALUES (?, ?, ?, ?, ?, ?, COALESCE((SELECT mfa_verified FROM sessions WHERE id = ? AND user_id = ?), 0), ?)")
            .bind(&grant).bind(&params.client_id).bind(&user.id).bind(&params.resource).bind(params.scope.as_deref().unwrap_or(""))
            .bind(now + GRANT_LIFETIME).bind(session).bind(&user.id).bind(now).execute(&mut *tx).await?;
        sqlx::query("UPDATE oauth_clients SET last_granted_at = ? WHERE id = ?")
            .bind(now)
            .bind(&params.client_id)
            .execute(&mut *tx)
            .await?;
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
    headers: HeaderMap,
    Form(form): Form<TokenRequest>,
) -> Result<Response, Error> {
    rate_limit(&state, &headers, "token").await?;
    let (kind, value) = match form.grant_type.as_str() {
        "authorization_code" => ("code", form.code.as_deref()),
        "refresh_token" => ("refresh", form.refresh_token.as_deref()),
        _ => return Err(invalid("unsupported_grant_type")),
    };
    let value = value.ok_or(invalid("invalid_request"))?;
    // The grant is bound to its resource (the query below matches it).
    if resource_scopes(&form.resource).is_none() {
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
    let query = format!("SELECT c.grant_id, c.used, c.redirect_uri, c.challenge, g.scope, g.expires_at FROM oauth_credentials c JOIN oauth_grants g ON g.id = c.grant_id JOIN users u ON u.id = g.user_id WHERE c.hash = ? AND c.kind = ? AND (c.expires_at > ? OR c.used = 1) AND g.expires_at > ? AND g.revoked = 0 AND g.client_id = ? AND g.resource = ? AND u.enabled = 1 AND {GRANT_MFA_POLICY}");
    let row: Option<Credential> = sqlx::query_as(&query)
        .bind(hash(value))
        .bind(kind)
        .bind(now)
        .bind(now)
        .bind(&form.client_id)
        .bind(&form.resource)
        .fetch_optional(&mut *tx)
        .await?;
    // A spent credential passes the expiry filter above (an expired unused
    // one does not), so replaying it, even after it expired, revokes the
    // grant below.
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
    sqlx::query("UPDATE oauth_grants SET last_used_at = ? WHERE id = ?")
        .bind(now)
        .bind(&row.grant_id)
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
    headers: HeaderMap,
    Form(form): Form<Revocation>,
) -> Result<Response, Error> {
    rate_limit(&state, &headers, "revoke").await?;
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
    Ok(bearer_grant(pool, headers, audience)
        .await?
        .map(|(user, scope, _)| (user, scope)))
}

/// The bearer token of `Authorization` when it has the shape of an access
/// token (43 characters of URL-safe base64), else `None`.
pub(crate) fn bearer_token(headers: &HeaderMap) -> Option<&str> {
    let value = headers.get("authorization")?.to_str().ok()?;
    let (scheme, token) = value.split_once(' ')?;
    scheme.eq_ignore_ascii_case("bearer").then_some(token)
}

/// `bearer_user`, also returning the grant the token belongs to (the REST
/// API counts its rate limits per grant).
pub(crate) async fn bearer_grant(
    pool: &SqlitePool,
    headers: &HeaderMap,
    audience: &str,
) -> Result<Option<(User, String, String)>, Error> {
    let Some(token) = bearer_token(headers) else {
        return Ok(None);
    };
    if token.len() != 43 {
        return Ok(None);
    }
    let query = format!("SELECT g.id, g.user_id, COALESCE(c.scope, g.scope), g.scope FROM oauth_credentials c JOIN oauth_grants g ON g.id = c.grant_id JOIN users u ON u.id = g.user_id WHERE c.hash = ? AND c.kind = 'access' AND c.expires_at > ? AND g.expires_at > ? AND g.revoked = 0 AND g.resource = ? AND {GRANT_MFA_POLICY}");
    let row: Option<(String, String, String, String)> = sqlx::query_as(&query)
        .bind(hash(token))
        .bind(Utc::now().timestamp())
        .bind(Utc::now().timestamp())
        .bind(audience)
        .fetch_optional(pool)
        .await?;
    let Some((grant, id, scope, granted)) = row else {
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
    if user.is_some() {
        let now = Utc::now().timestamp();
        sqlx::query("UPDATE oauth_grants SET last_used_at = ? WHERE id = ? AND (last_used_at IS NULL OR last_used_at < ?)")
            .bind(now).bind(&grant).bind(now - 60).execute(pool).await?;
    }
    Ok(user.map(|u| (u, scope, grant)))
}

/// Deletes every grant a local account approved from a session that had not
/// passed MFA, with its tokens, and the pending authorizations of local
/// accounts without MFA. Run when the admin starts requiring MFA, in the same
/// transaction as the session purge.
pub(crate) async fn delete_unverified_local_grants(
    conn: &mut SqliteConnection,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "DELETE FROM oauth_credentials WHERE grant_id IN
             (SELECT g.id FROM oauth_grants g JOIN users u ON u.id = g.user_id
              WHERE g.mfa_verified = 0 AND u.auth_provider = 'local')",
    )
    .execute(&mut *conn)
    .await?;
    sqlx::query(
        "DELETE FROM oauth_grants WHERE mfa_verified = 0
             AND user_id IN (SELECT id FROM users WHERE auth_provider = 'local')",
    )
    .execute(&mut *conn)
    .await?;
    sqlx::query(
        "DELETE FROM oauth_requests WHERE user_id IN (SELECT id FROM users WHERE auth_provider = 'local')
             AND NOT EXISTS (SELECT 1 FROM user_mfa m WHERE m.user_id = oauth_requests.user_id)",
    )
    .execute(&mut *conn)
    .await?;
    // Personal access tokens created from such a session go the same way.
    sqlx::query(
        "DELETE FROM api_tokens WHERE mfa_verified = 0
             AND user_id IN (SELECT id FROM users WHERE auth_provider = 'local')",
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// Deletes the user's personal access tokens (#169): one (API tokens
/// "Revoke"), or all of them (admin, and `mfa::revoke_access`). Returns how
/// many went.
pub(crate) async fn delete_api_tokens(
    conn: &mut SqliteConnection,
    user_id: &str,
    token_id: Option<&str>,
) -> Result<u64, sqlx::Error> {
    Ok(
        sqlx::query("DELETE FROM api_tokens WHERE user_id = ? AND (? IS NULL OR id = ?)")
            .bind(user_id)
            .bind(token_id)
            .bind(token_id)
            .execute(conn)
            .await?
            .rows_affected(),
    )
}

/// Deletes the user's grants with their tokens: one grant (Connected apps
/// "Revoke"), or all of them (admin, and `mfa::revoke_access`). Returns how
/// many grants went.
pub(crate) async fn delete_grants(
    conn: &mut SqliteConnection,
    user_id: &str,
    grant_id: Option<&str>,
) -> Result<u64, sqlx::Error> {
    sqlx::query(
        "DELETE FROM oauth_credentials WHERE grant_id IN
             (SELECT id FROM oauth_grants WHERE user_id = ? AND (? IS NULL OR id = ?))",
    )
    .bind(user_id)
    .bind(grant_id)
    .bind(grant_id)
    .execute(&mut *conn)
    .await?;
    Ok(
        sqlx::query("DELETE FROM oauth_grants WHERE user_id = ? AND (? IS NULL OR id = ?)")
            .bind(user_id)
            .bind(grant_id)
            .bind(grant_id)
            .execute(conn)
            .await?
            .rows_affected(),
    )
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
    sqlx::query("DELETE FROM api_tokens WHERE expires_at IS NOT NULL AND expires_at <= ?")
        .bind(now)
        .execute(pool)
        .await?;
    sqlx::query("DELETE FROM api_idempotency WHERE created_at <= ?")
        .bind(now - 86400)
        .execute(pool)
        .await?;
    // Retain spent refresh credentials until their grant expires (replay detection).
    let clients = format!(
        "DELETE FROM oauth_clients WHERE id IN (SELECT c.id FROM oauth_clients c WHERE {RECLAIMABLE_CLIENT})"
    );
    sqlx::query(&clients)
        .bind(now)
        .bind(now - UNUSED_CLIENT_LIFETIME)
        .bind(now - GRANTED_CLIENT_LIFETIME)
        .execute(pool)
        .await?;
    Ok(())
}
