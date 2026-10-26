mod admin;
mod availability;
mod booking;
mod booking_actions;
mod branding;
mod calendar_push;
pub mod captcha;
mod dashboard;
mod event_types;
mod format;
mod hosts;
mod invites;
pub mod meeting;
mod microsoft;
mod notify;
mod pending;
mod public_pages;
mod sources;
mod teams;
pub(crate) mod writeback;

use crate::utils::{convert_event_to_tz, parse_ical_datetime};
use axum::extract::{DefaultBodyLimit, Form, Multipart, Path, Query, State};
use axum::http::HeaderMap;
use axum::response::Redirect;
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{get, post};
use axum::Router;
use chrono::{
    Datelike, Duration, Local, NaiveDate, NaiveDateTime, NaiveTime, Offset, TimeZone, Timelike, Utc,
};
use chrono_tz::{OffsetName, Tz};
use minijinja::{context, Environment};
use serde::Deserialize;
use sqlx::SqlitePool;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use subtle::ConstantTimeEq;
use tokio::sync::Mutex;
use tower_http::set_header::SetResponseHeaderLayer;
use tower_http::trace::TraceLayer;

use admin::*;
pub(crate) use availability::expand_recurring_into_busy;
use availability::*;
use booking::*;
use booking_actions::*;
use branding::*;
pub(crate) use calendar_push::resource_delete_booking;
use calendar_push::*;
pub(crate) use calendar_push::{
    booking_additional_attendees, booking_attendee_languages, booking_guest_input,
    booking_ics_sequence, with_invite_holders,
};
pub(crate) use calendar_push::{caldav_delete_booking, claimant_delete_booking};
use dashboard::*;
use event_types::*;
use format::*;
pub(crate) use hosts::booking_host_identity;
use hosts::*;
use invites::*;
use microsoft::*;
pub use notify::drain_emails;
pub use notify::run_reminder_loop;
pub(crate) use notify::spawn_emails;
use notify::*;
use pending::*;
use public_pages::*;
use sources::*;
use teams::*;

/// Simple per-IP rate limiter for login attempts.
/// Tracks (attempt_count, window_start) per IP.
pub struct RateLimiter {
    state: Mutex<RateLimiterState>,
    max_attempts: u32,
    window: std::time::Duration,
}

struct RateLimiterState {
    attempts: HashMap<String, (u32, std::time::Instant)>,
    last_sweep: std::time::Instant,
}

impl RateLimiter {
    pub fn new(max_attempts: u32, window_secs: u64) -> Self {
        Self {
            state: Mutex::new(RateLimiterState {
                attempts: HashMap::new(),
                last_sweep: std::time::Instant::now(),
            }),
            max_attempts,
            window: std::time::Duration::from_secs(window_secs),
        }
    }

    /// Returns true if the request should be rejected (rate limited).
    pub async fn check_limited(&self, key: &str) -> bool {
        let mut state = self.state.lock().await;
        let now = std::time::Instant::now();

        // Evict entries whose window has elapsed. Bounded by "at most once per
        // window" so the work is amortized; without this the HashMap would
        // grow without bound under traffic from many distinct IPs.
        if now.duration_since(state.last_sweep) >= self.window {
            let window = self.window;
            state
                .attempts
                .retain(|_, (_, start)| now.duration_since(*start) <= window);
            state.last_sweep = now;
        }

        if let Some((count, start)) = state.attempts.get_mut(key) {
            if now.duration_since(*start) > self.window {
                // Window expired, reset
                *count = 1;
                *start = now;
                false
            } else if *count >= self.max_attempts {
                true
            } else {
                *count += 1;
                false
            }
        } else {
            state.attempts.insert(key.to_string(), (1, now));
            false
        }
    }
}

pub struct AppState {
    pub pool: SqlitePool,
    pub templates: Environment<'static>,
    pub login_limiter: RateLimiter,
    pub booking_limiter: RateLimiter,
    pub data_dir: PathBuf,
    pub secret_key: [u8; 32],
    pub theme_css: tokio::sync::RwLock<String>,
    pub company_link: tokio::sync::RwLock<Option<String>>,
    pub captcha_config: tokio::sync::RwLock<Option<captcha::CaptchaConfig>>,
    /// Org-wide meeting provider config (Jitsi + webhook). Cached so guest
    /// slot/booking pages don't hit `auth_config` on every render. Rebuilt
    /// when the admin saves changes.
    pub meeting_config: tokio::sync::RwLock<meeting::MeetingConfig>,
    /// CSP for booking form pages: relaxed with captcha origins when captcha
    /// is enabled, identical to `csp_baseline` otherwise. Rebuilt on admin save.
    pub csp: tokio::sync::RwLock<String>,
    /// Strict baseline CSP served on every non-booking page regardless of
    /// captcha configuration.
    pub csp_baseline: String,
}

// --- CSRF protection (double-submit cookie pattern) ---

/// Generate a random CSRF token using UUID v4.
pub fn generate_csrf_token() -> String {
    uuid::Uuid::new_v4().to_string()
}

/// Cookie name for the CSRF token. The `__Host-` prefix forces the browser to
/// enforce Secure, Path=/, and no Domain, so the token cannot be overwritten
/// by a sibling subdomain.
pub const CSRF_COOKIE_NAME: &str = "__Host-calrs_csrf";

/// Build the Set-Cookie header value for a CSRF token.
///
/// `HttpOnly` is intentionally omitted, the double-submit pattern needs the
/// client JS in `base.html` to read the cookie and inject it into the form.
/// `Secure` is set so the token never leaks over plaintext HTTP; modern
/// browsers still honour `Secure` cookies on localhost for dev over HTTP.
///
/// `cross_site=true` switches the cookie to `SameSite=None` so the booking
/// page can read it from inside a third-party iframe (the embed flow). The
/// double-submit token-vs-cookie comparison still defends against CSRF — we
/// never relied on SameSite for that. Browsers blocking all third-party
/// cookies (Safari ITP, Chrome with 3P cookies disabled) still won't deliver
/// it; that's an upstream-platform limitation outside our control.
pub fn csrf_cookie_value(token: &str) -> String {
    csrf_cookie_value_for(token, false)
}

pub fn csrf_cookie_value_for(token: &str, cross_site: bool) -> String {
    let same_site = if cross_site { "None" } else { "Lax" };
    // In the cross-site (embed) case also mark the cookie `Partitioned` (CHIPS).
    // Browsers that block unpartitioned third-party cookies (Safari 18.4+,
    // Firefox 131+, Chrome with 3P cookies disabled) will then still deliver it
    // inside the embed iframe, so the double-submit CSRF check keeps working.
    // `Partitioned` requires `Secure` + `Path=/`, both already set, and is
    // compatible with the `__Host-` prefix.
    let partitioned = if cross_site { "; Partitioned" } else { "" };
    format!(
        "{}={}; Path=/; Secure; SameSite={}; Max-Age=86400{}",
        CSRF_COOKIE_NAME, token, same_site, partitioned
    )
}

/// Extract the CSRF cookie value from request headers.
pub fn csrf_token_from_headers(headers: &HeaderMap) -> Option<String> {
    let prefix = format!("{}=", CSRF_COOKIE_NAME);
    headers
        .get_all(axum::http::header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|s| s.split(';'))
        .find_map(|part| {
            let part = part.trim();
            part.strip_prefix(&prefix).map(|v| v.to_string())
        })
}

/// Verify that the CSRF form field matches the cookie value.
///
/// The two tokens are compared with `subtle::ConstantTimeEq` to avoid leaking
/// any partial-match timing information that could otherwise be used to
/// recover a valid token byte by byte.
#[allow(clippy::result_large_err)]
pub fn verify_csrf_token(
    headers: &HeaderMap,
    form_token: &Option<String>,
) -> Result<(), axum::response::Response> {
    let cookie_token = csrf_token_from_headers(headers);
    let valid = match (cookie_token.as_deref(), form_token.as_deref()) {
        (Some(cookie), Some(form)) if !cookie.is_empty() => {
            bool::from(cookie.as_bytes().ct_eq(form.as_bytes()))
        }
        _ => false,
    };
    if valid {
        Ok(())
    } else {
        Err((
            axum::http::StatusCode::FORBIDDEN,
            Html("403 Forbidden: CSRF token mismatch".to_string()),
        )
            .into_response())
    }
}

// --- Error sanitization helpers ---
//
// Internal errors (template render failures, DB errors, OIDC token-exchange
// errors) used to be `format!`'d straight into the HTTP response body. The
// detail leaks internal layout (template paths, schema hints, IdP URLs and
// occasionally token contents) to anyone who can trigger the path. These
// helpers funnel the detail to the structured log instead and return a
// generic message to the user.

/// Internal-error response (500). Use for any server-side failure on a path
/// where the user cannot fix the cause themselves.
pub(crate) fn internal_error_response<E: std::fmt::Display + ?Sized>(
    context: &str,
    error: &E,
) -> Response {
    tracing::error!(error = %error, context = %context, "internal error");
    (
        axum::http::StatusCode::INTERNAL_SERVER_ERROR,
        Html("An internal error occurred. Please try again.".to_string()),
    )
        .into_response()
}

/// Same as `internal_error_response` but yields `Html<String>` so it can be
/// returned from helper functions whose signature is `Html<String>` (these
/// are then themselves wrapped via `.into_response()` at the call site).
pub(crate) fn internal_error_html<E: std::fmt::Display + ?Sized>(
    context: &str,
    error: &E,
) -> Html<String> {
    tracing::error!(error = %error, context = %context, "internal error");
    Html("An internal error occurred. Please try again.".to_string())
}

/// Same as `internal_error_response` but yields a plain `String` for sites
/// that compose the response inline, e.g. as a fallback template body via
/// `unwrap_or_else`.
pub(crate) fn internal_error_body<E: std::fmt::Display + ?Sized>(
    context: &str,
    error: &E,
) -> String {
    tracing::error!(error = %error, context = %context, "internal error");
    "An internal error occurred. Please try again.".to_string()
}

/// OIDC-flow failure response. Rendered to the user when the auth handshake
/// breaks down (token exchange, ID token verification, configuration). The
/// underlying error from `openidconnect` can include the IdP's response
/// body, token endpoint URL, or token contents, so it never reaches the
/// client.
pub(crate) fn oidc_error_response<E: std::fmt::Display + ?Sized>(
    context: &str,
    error: &E,
) -> Response {
    tracing::error!(error = %error, context = %context, "oidc auth failure");
    oidc_failure_response()
}

/// Same as `oidc_error_response`, for a `std::error::Error`: also logs the
/// `source()` chain as `cause`. `oauth2` errors keep the useful part there
/// (a token response that fails to parse displays only as "Failed to parse
/// server response"), so logging `Display` alone hides the reason (#264).
/// Only for errors whose sources cannot carry token material; the token
/// exchange's parse failure is logged by its own path in `auth.rs`.
pub(crate) fn oidc_error_response_with_source(
    context: &str,
    error: &(dyn std::error::Error + 'static),
) -> Response {
    tracing::error!(
        error = %error,
        cause = %error_source_chain(error),
        context = %context,
        "oidc auth failure"
    );
    oidc_failure_response()
}

/// The `source()` chain of `error`, outermost first, joined with ": ".
/// Empty when the error has no source.
pub(crate) fn error_source_chain(error: &(dyn std::error::Error + 'static)) -> String {
    let mut parts = Vec::new();
    let mut current = error.source();
    while let Some(e) = current {
        parts.push(e.to_string());
        current = e.source();
    }
    parts.join(": ")
}

/// The generic OIDC failure response, for a caller that already logged the
/// detail itself.
pub(crate) fn oidc_failure_response() -> Response {
    (
        axum::http::StatusCode::INTERNAL_SERVER_ERROR,
        Html("Authentication failed. Please try again or contact your administrator.".to_string()),
    )
        .into_response()
}

/// Extract the client IP for rate-limiting from request headers.
///
/// Trust model: assumes calrs runs behind a single reverse proxy (the
/// documented Caddy/Nginx setup) that appends the observed peer address
/// to `X-Forwarded-For`. Each proxy in the chain appends, so the
/// rightmost value is the most-recent trusted hop's view of its peer,
/// and everything to its left is attacker-controlled. Taking the
/// rightmost value defeats the "set X-Forwarded-For: random.ip on every
/// request" rate-limit bypass.
///
/// `X-Real-IP` is intentionally not honoured: neither documented proxy
/// sets it by default, so a client-supplied `X-Real-IP` would pass
/// straight through and be trusted (worse than the bug this fixes).
///
/// Falls back to `"unknown"` if `X-Forwarded-For` is missing, which
/// collapses such requests into a single shared rate-limit bucket. That
/// is intentional: it throttles abuse rather than letting it through.
pub fn client_ip_for_rate_limit(headers: &HeaderMap) -> String {
    headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.rsplit(',').next())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "unknown".to_string())
}

/// Form struct for POST endpoints that only need CSRF validation.
#[derive(Deserialize)]
struct CsrfForm {
    _csrf: Option<String>,
}

/// Query struct for CSRF validation on multipart endpoints.
#[derive(Deserialize)]
struct CsrfQuery {
    _csrf: Option<String>,
}

async fn get_company_link(pool: &SqlitePool) -> Option<String> {
    sqlx::query_scalar::<_, Option<String>>(
        "SELECT company_link FROM auth_config WHERE id = 'singleton'",
    )
    .fetch_optional(pool)
    .await
    .ok()
    .flatten()
    .flatten()
    .filter(|s| !s.is_empty())
    // Defense in depth: even if a bad value reached the DB (legacy data,
    // manual edit), refuse to render anything but http(s). The admin
    // update handler is the primary gate; this is the backstop.
    .filter(|s| is_safe_company_link(s))
}

/// Wrap an HTML render in a response. When `embed=1`, override the global
/// `X-Frame-Options: SAMEORIGIN` so legacy browsers don't refuse the iframe.
/// The matching `frame-ancestors *` CSP override is added by `csp_middleware`
/// — that keeps the CSP composition (captcha allowances + embed framing) in
/// one place. Modern browsers honour CSP's frame-ancestors over the legacy
/// header per CSP Level 2.
fn html_response(body: String, embed: &EmbedParams) -> Response {
    let mut resp = Html(body).into_response();
    if embed.is_embedded() {
        // `ALLOWALL` is non-standard; modern browsers ignore it once CSP
        // frame-ancestors is set, but it prevents the global SAMEORIGIN
        // layer from re-asserting itself for browsers that don't speak CSP.
        resp.headers_mut().insert(
            axum::http::header::X_FRAME_OPTIONS,
            axum::http::HeaderValue::from_static("ALLOWALL"),
        );
    }
    resp
}

/// Middleware that ensures a CSRF cookie is set on every response.
///
/// When the request URL carries `embed=1`, the cookie is issued with
/// `SameSite=None; Secure` so the booking flow inside a third-party iframe
/// can read its own cookie. Outside the embed flow, the strict `SameSite=Lax`
/// default applies.
async fn csrf_cookie_middleware(
    headers: HeaderMap,
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    let cross_site_cookie =
        is_embedded_booking_request(request.uri().path(), request.uri().query());
    let mut response = next.run(request).await;

    // Only set cookie if not already present in the request
    if csrf_token_from_headers(&headers).is_none() {
        let token = generate_csrf_token();
        if let Ok(cookie_val) = csrf_cookie_value_for(&token, cross_site_cookie).parse() {
            response
                .headers_mut()
                .append(axum::http::header::SET_COOKIE, cookie_val);
        }
    }

    response
}

/// Detects the `embed=1` (or `embed=true`) query param without pulling in a
/// full URL parser. Tolerates extra params and arbitrary parameter order.
fn is_embed_request(query: Option<&str>) -> bool {
    let Some(q) = query else { return false };
    q.split('&').any(|pair| {
        let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
        k == "embed" && (v == "1" || v == "true")
    })
}

/// Public booking surfaces that may legitimately be loaded inside a
/// third-party iframe.
///
/// SECURITY: this gates BOTH the relaxed `frame-ancestors *` CSP swap and the
/// `SameSite=None` CSRF cookie. It must never match authenticated or sensitive
/// surfaces (`/auth/...`, `/dashboard/...`, asset routes), otherwise those
/// pages become frameable by any site — e.g. `/auth/login?embed=1` would be a
/// fully functional login form inside an attacker's iframe (clickjacking /
/// login CSRF). The embed=1 flag alone is NOT sufficient; the path must also
/// be a booking surface.
fn is_embeddable_path(path: &str) -> bool {
    // Modern user/team booking routes (allowlist).
    if path == "/u"
        || path.starts_with("/u/")
        || path == "/team"
        || path.starts_with("/team/")
        || path == "/g"
        || path.starts_with("/g/")
    {
        return true;
    }
    // Legacy single-user booking pages: only `/{slug}` and `/{slug}/book`.
    // Everything is a single top-level segment, so reject any path whose first
    // segment is a reserved (non-booking) route. New reserved top-level routes
    // must be added to RESERVED below to stay out of the embed surface.
    let trimmed = path.trim_start_matches('/');
    if trimmed.is_empty() {
        return false;
    }
    let mut segs = trimmed.split('/');
    let first = segs.next().unwrap_or("");
    let rest: Vec<&str> = segs.collect();
    let shape_ok = rest.is_empty() || (rest.len() == 1 && rest[0] == "book");
    if !shape_ok {
        return false;
    }
    const RESERVED: &[&str] = &[
        "auth",
        "dashboard",
        "avatar",
        "team-avatar",
        "logo",
        "favicon",
        "favicon.ico",
        "accent.css",
        "brand-logo",
        "embed.js",
        "fonts",
        "t",
        "booking",
        "u",
        "team",
        "g",
    ];
    !RESERVED.contains(&first)
}

/// True when the request is both flagged `embed=1` and targets an embeddable
/// booking surface. This is the single gate for embed-mode behavior changes.
fn is_embedded_booking_request(path: &str, query: Option<&str>) -> bool {
    is_embed_request(query) && is_embeddable_path(path)
}

fn build_csp(captcha: &Option<captcha::CaptchaConfig>) -> String {
    let (wasm_eval, worker_src, script_extra, connect_extra) = match captcha.as_ref() {
        Some(c) => {
            let widget_origin = c.widget_script_origin();
            let instance_origin = c.instance_origin();
            // A custom WASM URL is fetched by the widget, so its origin joins
            // connect-src (and only connect-src: it is never a script).
            let wasm_origin = c
                .wasm_origin()
                .filter(|o| *o != widget_origin && *o != instance_origin)
                .map(|o| format!(" {o}"))
                .unwrap_or_default();
            (
                " 'wasm-unsafe-eval'",
                "worker-src blob:; ",
                format!(" {}", widget_origin),
                format!(" {} {}{}", instance_origin, widget_origin, wasm_origin),
            )
        }
        None => ("", "", String::new(), String::new()),
    };
    format!(
        "default-src 'self'; img-src 'self' data:; \
         style-src 'self' 'unsafe-inline'; \
         script-src 'self' 'unsafe-inline'{}{}; \
         {}connect-src 'self'{}; \
         object-src 'none'; base-uri 'self'; frame-ancestors 'self'",
        wasm_eval, script_extra, worker_src, connect_extra
    )
}

/// Booking form pages (`…/book`) are the only pages that embed the captcha
/// widget, so they are the only ones that get the relaxed CSP when captcha is
/// enabled. Every other page keeps the strict baseline policy.
fn is_booking_form_path(path: &str) -> bool {
    path.ends_with("/book")
}

async fn csp_middleware(
    State(state): State<Arc<AppState>>,
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    let booking_page = is_booking_form_path(request.uri().path());
    let embed_mode = is_embedded_booking_request(request.uri().path(), request.uri().query());
    let mut response = next.run(request).await;
    if !response
        .headers()
        .contains_key(axum::http::header::CONTENT_SECURITY_POLICY)
    {
        let strict = if booking_page {
            state.csp.read().await.clone()
        } else {
            state.csp_baseline.clone()
        };
        // In embed mode, swap the default `frame-ancestors 'self'` for `*` so
        // the page can be loaded inside a third-party iframe. The replacement
        // is safe because `build_csp` is the sole producer of the strict CSP
        // string and always uses that exact directive.
        let csp = if embed_mode {
            strict.replace("frame-ancestors 'self'", "frame-ancestors *")
        } else {
            strict
        };
        if let Ok(val) = axum::http::HeaderValue::from_str(&csp) {
            response
                .headers_mut()
                .insert(axum::http::header::CONTENT_SECURITY_POLICY, val);
        }
    }
    response
}

pub async fn create_router(pool: SqlitePool, data_dir: PathBuf, secret_key: [u8; 32]) -> Router {
    let mut env = Environment::new();
    env.set_undefined_behavior(minijinja::UndefinedBehavior::Lenient);
    env.set_loader(minijinja::path_loader("templates"));
    crate::i18n::register(&mut env);

    // Populate the process-global runtime-settings cache (base URL, private-host
    // allowlist) so synchronous readers and email/link builders see DB values.
    crate::settings::load_from_db(&pool).await;

    let initial_theme_css = build_theme_css(&pool).await;
    let initial_company_link = get_company_link(&pool).await;
    let initial_captcha = captcha::load_captcha_config(&pool, &secret_key).await;
    let initial_meeting = meeting::load_config(&pool, &secret_key).await;
    let initial_csp = build_csp(&initial_captcha);

    let state = Arc::new(AppState {
        pool,
        templates: env,
        // 10 login attempts per IP per 15 minutes
        login_limiter: RateLimiter::new(10, 900),
        booking_limiter: RateLimiter::new(10, 300),
        secret_key,
        data_dir,
        theme_css: tokio::sync::RwLock::new(initial_theme_css),
        company_link: tokio::sync::RwLock::new(initial_company_link),
        captcha_config: tokio::sync::RwLock::new(initial_captcha),
        meeting_config: tokio::sync::RwLock::new(initial_meeting),
        csp: tokio::sync::RwLock::new(initial_csp),
        csp_baseline: build_csp(&None),
    });

    Router::new()
        .merge(crate::auth::auth_router())
        .route("/", get(root_redirect))
        .route("/dashboard", get(dashboard))
        .route("/dashboard/event-types", get(dashboard_event_types))
        .route(
            "/dashboard/availability/default",
            get(dashboard_availability_default),
        )
        .route("/dashboard/bookings", get(dashboard_bookings))
        .route("/dashboard/teams", get(dashboard_teams))
        .route(
            "/dashboard/teams/new",
            get(show_team_form).post(create_team),
        )
        .route("/dashboard/sources", get(dashboard_sources))
        .route("/dashboard/invite-links", get(dashboard_organization))
        .route(
            "/dashboard/organization",
            get(|| async { Redirect::permanent("/dashboard/invite-links") }),
        )
        .route("/dashboard/bookings/{id}/cancel", post(cancel_booking))
        .route("/dashboard/bookings/{id}/confirm", post(confirm_booking))
        .route(
            "/dashboard/event-types/new",
            get(new_event_type_form).post(create_event_type),
        )
        .route(
            "/dashboard/event-types/{slug}/edit",
            get(edit_event_type_form).post(update_event_type),
        )
        .route(
            "/dashboard/event-types/{slug}/priority/{user_id}",
            post(update_event_type_member_priority),
        )
        .route(
            "/dashboard/event-types/{slug}/toggle",
            post(toggle_event_type),
        )
        .route(
            "/dashboard/event-types/{slug}/delete",
            post(delete_event_type),
        )
        // Invite management
        .route(
            "/dashboard/invites/{event_type_id}",
            get(invite_management_page),
        )
        .route(
            "/dashboard/invites/{event_type_id}/send",
            post(send_invite_bulk),
        )
        .route("/dashboard/invites/{invite_id}/delete", post(delete_invite))
        .route(
            "/dashboard/invites/{event_type_id}/quick-link",
            post(generate_quick_link),
        )
        // Availability overrides
        .route(
            "/dashboard/event-types/{slug}/overrides",
            get(overrides_page).post(create_override),
        )
        .route(
            "/dashboard/event-types/{slug}/overrides/{override_id}/delete",
            post(delete_override),
        )
        // Embed code generator
        .route("/dashboard/event-types/{slug}/embed", get(embed_page))
        // Calendar source management
        .route(
            "/dashboard/sources/new",
            get(new_source_form).post(create_source),
        )
        .route(
            "/dashboard/sources/{id}/edit",
            get(edit_source_form).post(update_source),
        )
        .route("/dashboard/sources/{id}/remove", post(remove_source))
        .route("/dashboard/sources/{id}/test", post(test_source))
        .route("/dashboard/sources/google/connect", get(google_connect))
        .route("/dashboard/sources/google/callback", get(google_callback))
        .route("/dashboard/sources/microsoft/connect", post(microsoft_connect))
        .route("/dashboard/sources/microsoft/callback", get(microsoft_callback))
        .route("/dashboard/sources/{id}/sync", post(sync_source))
        .route(
            "/dashboard/sources/{id}/force-sync",
            post(force_sync_source),
        )
        .route(
            "/dashboard/sources/{id}/setup-write",
            get(setup_write_calendar),
        )
        .route(
            "/dashboard/sources/{id}/write-calendar",
            post(set_write_calendar),
        )
        // Settings & avatar
        .route(
            "/dashboard/settings",
            get(settings_page).post(settings_save),
        )
        .route("/dashboard/settings/timezone", post(update_timezone))
        .route("/dashboard/settings/avatar", post(upload_avatar))
        .route("/dashboard/settings/avatar/delete", post(delete_avatar))
        .route("/avatar/{user_id}", get(serve_avatar))
        // Team settings & avatar
        .route(
            "/dashboard/teams/{team_id}/settings",
            get(team_settings_page).post(team_settings_save),
        )
        .route(
            "/dashboard/teams/{team_id}/avatar",
            post(upload_team_avatar),
        )
        .route(
            "/dashboard/teams/{team_id}/avatar/delete",
            post(delete_team_avatar),
        )
        .route("/dashboard/teams/{team_id}/delete", post(delete_team))
        .route("/team-avatar/{team_id}", get(serve_team_avatar))
        // Troubleshoot
        .route("/dashboard/troubleshoot", get(troubleshoot))
        // Admin routes
        .route("/dashboard/admin", get(admin_dashboard))
        .route(
            "/dashboard/admin/users/{id}/toggle-role",
            post(admin_toggle_role),
        )
        .route(
            "/dashboard/admin/users/{id}/toggle-enabled",
            post(admin_toggle_enabled),
        )
        .route(
            "/dashboard/admin/users/{id}/delete",
            post(admin_delete_user),
        )
        .route("/dashboard/admin/auth", post(admin_update_auth))
        .route("/dashboard/admin/general", post(admin_update_general))
        .route("/dashboard/admin/accent", post(admin_update_accent))
        .route("/dashboard/admin/oidc", post(admin_update_oidc))
        .route(
            "/dashboard/admin/google-oauth2",
            post(admin_update_google_oauth2),
        )
        .route("/dashboard/admin/microsoft-oauth2", post(admin_update_microsoft_oauth2))
        .route("/dashboard/admin/captcha", post(admin_update_captcha))
        .route("/dashboard/admin/resources", post(admin_create_resource))
        .route(
            "/dashboard/admin/resources/{id}",
            post(admin_update_resource),
        )
        .route(
            "/dashboard/admin/resources/{id}/delete",
            post(admin_delete_resource),
        )
        .route(
            "/dashboard/admin/resources/{id}/sync",
            post(admin_sync_resource),
        )
        .route(
            "/dashboard/admin/resources/{id}/test-write",
            post(admin_test_write_resource),
        )
        .route("/dashboard/admin/smtp", post(admin_update_smtp))
        .route("/dashboard/admin/smtp/test", post(admin_update_smtp_test))
        .route("/dashboard/admin/smtp/clear", post(admin_update_smtp_clear))
        .route("/dashboard/admin/sms", post(admin_update_sms))
        .route("/dashboard/admin/sms/test", post(admin_update_sms_test))
        .route("/dashboard/admin/sms/clear", post(admin_update_sms_clear))
        .route("/dashboard/admin/sms/policy", post(admin_update_sms_policy))
        .route("/dashboard/admin/jitsi", post(admin_update_jitsi))
        .route(
            "/dashboard/admin/meeting-webhook",
            post(admin_update_meeting_webhook),
        )
        // Axum caps request bodies at 2 MB by default, which would refuse a
        // logo just under the 2 MB it accepts once the multipart framing is
        // added. Leave room for that framing on this route only.
        .route(
            "/dashboard/admin/logo",
            post(admin_upload_logo).layer(DefaultBodyLimit::max(LOGO_MAX_BYTES + 64 * 1024)),
        )
        .route("/dashboard/admin/logo/delete", post(admin_delete_logo))
        .route("/dashboard/admin/favicon", post(admin_upload_favicon))
        .route(
            "/dashboard/admin/favicon/delete",
            post(admin_delete_favicon),
        )
        .route(
            "/dashboard/admin/company-link",
            post(admin_update_company_link),
        )
        .route(
            "/dashboard/admin/groups/{group_id}/members/{user_id}/weight",
            post(admin_update_member_weight),
        )
        .route("/dashboard/admin/impersonate/{id}", post(admin_impersonate))
        .route(
            "/dashboard/admin/stop-impersonate",
            post(admin_stop_impersonate),
        )
        // Group event type management
        .route(
            "/dashboard/group-event-types/new",
            get(new_group_event_type_form).post(create_group_event_type),
        )
        // Team event type management. The `team_id` is in the path so the
        // server-side WHERE clause can scope unambiguously: two teams may
        // legitimately share the same slug (e.g. "intro"), and looking up by
        // slug alone returns whichever row the DB picks first.
        .route(
            "/dashboard/group-event-types/{team_id}/{slug}/edit",
            get(edit_group_event_type_form).post(update_group_event_type),
        )
        .route(
            "/dashboard/group-event-types/{team_id}/{slug}/priority/{user_id}",
            post(update_group_event_type_member_priority),
        )
        .route(
            "/dashboard/group-event-types/{team_id}/{slug}/toggle",
            post(toggle_group_event_type),
        )
        .route(
            "/dashboard/group-event-types/{team_id}/{slug}/delete",
            post(delete_group_event_type),
        )
        .route(
            "/dashboard/group-event-types/{team_id}/{slug}/embed",
            get(group_embed_page),
        )
        // Serve logo and fonts
        .route("/logo", get(serve_logo))
        // `/favicon.ico` too: browsers request it on their own for pages
        // without a <link rel="icon">, such as a raw image or a JSON error.
        .route("/favicon", get(serve_favicon))
        .route("/favicon.ico", get(serve_favicon))
        .route("/accent.css", get(serve_accent_css))
        .route("/brand-logo", get(serve_brand_logo))
        .route("/embed.js", get(serve_embed_js))
        .route("/fonts/inter-latin.woff2", get(serve_font_inter_latin))
        .route(
            "/fonts/inter-latin-ext.woff2",
            get(serve_font_inter_latin_ext),
        )
        .route(
            "/static/intl-tel-input/{file}",
            get(serve_intl_tel_input_asset),
        )
        // Group public routes
        .route("/team/{team_slug}", get(team_profile_page))
        .route("/team/{team_slug}/{slug}", get(show_group_slots))
        .route(
            "/team/{team_slug}/{slug}/book",
            get(show_group_book_form).post(handle_group_booking),
        )
        // Legacy /g/ redirects
        .route("/g/{team_slug}", get(redirect_g_to_team))
        .route("/g/{team_slug}/{slug}", get(redirect_g_to_team_slug))
        .route(
            "/g/{team_slug}/{slug}/book",
            get(redirect_g_to_team_slug_book),
        )
        // Legacy team link redirects → unified teams
        .route("/t/{token}", get(redirect_team_link_to_team))
        .route("/t/{token}/book", get(redirect_team_link_to_team))
        // User-scoped public booking routes
        .route(
            "/booking/claim/{booking_id}",
            get(claim_booking_form).post(claim_booking),
        )
        .route(
            "/booking/approve/{token}",
            get(approve_booking_form).post(approve_booking_by_token),
        )
        .route(
            "/booking/decline/{token}",
            get(decline_booking_form).post(decline_booking_by_token),
        )
        .route(
            "/booking/cancel/{token}",
            get(guest_cancel_form).post(guest_cancel_booking),
        )
        .route("/booking/ics/{token}", get(booking_ics))
        .route(
            "/booking/reschedule/{token}",
            get(guest_reschedule_slots).post(guest_reschedule_booking),
        )
        .route(
            "/dashboard/bookings/{id}/reschedule",
            get(host_reschedule_slots).post(host_reschedule_booking),
        )
        .route("/u/{username}", get(user_profile))
        .route("/u/{username}/", get(redirect_user_profile_trailing_slash))
        .route("/u/{username}/{slug}", get(show_slots_for_user))
        .route(
            "/u/{username}/{slug}/book",
            get(show_book_form_for_user).post(handle_booking_for_user),
        )
        // Legacy single-user routes (kept for backward compatibility)
        .route("/{slug}", get(show_slots))
        .route("/{slug}/book", get(show_book_form).post(handle_booking))
        // OAuth callback query strings contain authorization codes/state.
        // Record only paths, never the default full request URI.
        .layer(TraceLayer::new_for_http().make_span_with(
            |request: &axum::http::Request<axum::body::Body>| {
                tracing::info_span!("http_request", method = %request.method(), path = %request.uri().path())
            },
        ))
        .layer(axum::middleware::from_fn(csrf_cookie_middleware))
        // Defensive HTTP headers on every response (incl. error pages).
        //
        // CSP allows `'unsafe-inline'` for scripts and styles because the
        // templates rely on inline `<script>` blocks (CSRF auto-injection,
        // theme toggle, time-zone banner) and inline `style="..."` attributes.
        // Tightening this would require nonces wired through every render
        // site or a sweep across templates to externalize inline blocks.
        //
        // The CSP itself is set by `csp_middleware`: booking form pages
        // (`…/book`) get the captcha-relaxed policy when captcha is enabled,
        // everything else always gets the strict baseline.
        .layer(SetResponseHeaderLayer::if_not_present(
            axum::http::header::X_FRAME_OPTIONS,
            axum::http::HeaderValue::from_static("SAMEORIGIN"),
        ))
        .layer(SetResponseHeaderLayer::if_not_present(
            axum::http::header::X_CONTENT_TYPE_OPTIONS,
            axum::http::HeaderValue::from_static("nosniff"),
        ))
        .layer(SetResponseHeaderLayer::if_not_present(
            axum::http::header::REFERRER_POLICY,
            axum::http::HeaderValue::from_static("strict-origin-when-cross-origin"),
        ))
        .layer(SetResponseHeaderLayer::if_not_present(
            axum::http::HeaderName::from_static("permissions-policy"),
            axum::http::HeaderValue::from_static(
                "geolocation=(), microphone=(), camera=(), payment=(), usb=()",
            ),
        ))
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            csp_middleware,
        ))
        .with_state(state)
}

/// Helper: create impersonation template context values (active, target_name, admin_name).
fn impersonation_ctx(auth_user: &crate::auth::AuthUser) -> (bool, String, String) {
    match &auth_user.impersonation {
        Some(info) => (true, info.target_name.clone(), info.admin_name.clone()),
        None => (false, String::new(), String::new()),
    }
}

/// Compute two-letter Matrix-style initials from a name (first letter of first + last word).
fn compute_initials(name: &str) -> String {
    let parts: Vec<&str> = name.split_whitespace().collect();
    let mut initials = String::new();
    if let Some(first) = parts.first() {
        if let Some(c) = first.chars().next() {
            initials.extend(c.to_uppercase());
        }
    }
    if parts.len() > 1 {
        if let Some(last) = parts.last() {
            if let Some(c) = last.chars().next() {
                initials.extend(c.to_uppercase());
            }
        }
    }
    if initials.is_empty() {
        "?".to_string()
    } else {
        initials
    }
}

/// Build OIDC groups context with member details for stacked avatars.
async fn build_groups_ctx(
    pool: &sqlx::SqlitePool,
    oidc_groups: &[(String, String, i64)],
    linked_set: &std::collections::HashSet<String>,
) -> Vec<minijinja::Value> {
    let mut out = Vec::new();
    for (id, name, member_count) in oidc_groups {
        let group_members: Vec<(String, String, Option<String>)> = sqlx::query_as(
            "SELECT u.id, u.name, u.avatar_path FROM users u \
             JOIN user_groups ug ON ug.user_id = u.id \
             WHERE ug.group_id = ? AND u.enabled = 1 ORDER BY u.name LIMIT 8",
        )
        .bind(id)
        .fetch_all(pool)
        .await
        .unwrap_or_default();
        let members_ctx: Vec<minijinja::Value> = group_members
            .iter()
            .map(|(uid, uname, ap)| {
                context! {
                    id => uid,
                    name => uname,
                    has_avatar => ap.is_some(),
                    initials => compute_initials(uname),
                }
            })
            .collect();
        out.push(context! {
            id => id,
            name => name,
            member_count => member_count,
            members => members_ctx,
            linked => linked_set.contains(id),
        });
    }
    out
}

/// Build sidebar context for dashboard templates.
fn sidebar_context(auth_user: &crate::auth::AuthUser, active: &str) -> minijinja::Value {
    let user = &auth_user.user;
    let (impersonating, _, _) = impersonation_ctx(auth_user);
    let effective_role = if impersonating {
        "admin".to_string()
    } else {
        user.role.clone()
    };
    context! {
        user_name => user.name,
        user_title => user.title.as_deref().unwrap_or(""),
        user_id => user.id,
        user_role => effective_role,
        user_timezone => user.timezone,
        has_avatar => user.avatar_path.is_some(),
        user_initials => compute_initials(&user.name),
        active => active,
        version => env!("CARGO_PKG_VERSION"),
    }
}

fn admin_sidebar_context(user: &crate::models::User, active: &str) -> minijinja::Value {
    context! {
        user_name => user.name,
        user_title => user.title.as_deref().unwrap_or(""),
        user_id => user.id,
        user_role => "admin",
        user_timezone => user.timezone,
        has_avatar => user.avatar_path.is_some(),
        user_initials => compute_initials(&user.name),
        active => active,
        version => env!("CARGO_PKG_VERSION"),
    }
}

/// Translate a message that takes exactly one string argument.
///
/// Server-rendered labels (the troubleshoot timeline, mostly) are built one at
/// a time and would otherwise each need four lines of FluentArgs plumbing.
fn tr1(lang: &str, key: &str, arg: &str, value: &str) -> String {
    let mut args = fluent_bundle::FluentArgs::new();
    args.set(arg, fluent_bundle::FluentValue::from(value));
    crate::i18n::translate(lang, key, Some(&args))
}

/// The booking error page, from Fluent message ids, in `lang`. The booking
/// POSTs and the token pages answer in the language of the page the visitor
/// came from, which is not always the browser's, so the language is always
/// passed in. The string-taking form stays for `resolve_guest_phone`, which
/// picks its own wording and hands back a pair that is already translated.
fn render_booking_action_error_keys_in(
    state: &AppState,
    lang: &str,
    title_key: &str,
    message_key: &str,
) -> axum::response::Response {
    render_booking_action_error_in(
        state,
        lang,
        &crate::i18n::translate(lang, title_key, None),
        &crate::i18n::translate(lang, message_key, None),
    )
}

/// The booking error page from already-translated strings, in `lang`.
fn render_booking_action_error_in(
    state: &AppState,
    lang: &str,
    title: &str,
    message: &str,
) -> axum::response::Response {
    render_booking_action_error_html_in(state, lang, title, message).into_response()
}

/// Same page as `render_booking_action_error_in`, for handlers whose
/// signature is `Html<String>` rather than `Response`.
fn render_booking_action_error_html_in(
    state: &AppState,
    lang: &str,
    title: &str,
    message: &str,
) -> Html<String> {
    let tmpl = match state.templates.get_template("booking_action_error.html") {
        Ok(t) => t,
        Err(e) => return internal_error_html("internal", &e),
    };
    Html(
        tmpl.render(context! {
            title => title,
            message => message,
            lang => lang,
        })
        .unwrap_or_else(|e| internal_error_body("template render", &e)),
    )
}

#[cfg(test)]
mod tests;
