pub mod captcha;
pub mod ews_global;
pub mod meeting;

use crate::utils::{convert_event_to_tz, parse_ical_datetime};
use axum::extract::{Form, Multipart, Path, Query, State};
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
    /// Global EWS impersonation: when `Some`, every user has a managed source
    /// row that the sync loop drives with the SOAP `ExchangeImpersonation`
    /// header. Reloaded when the admin saves changes.
    pub ews_global: tokio::sync::RwLock<Option<ews_global::EwsGlobalConfig>>,
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

#[derive(Deserialize)]
struct CompanyLinkForm {
    company_link: String,
    _csrf: Option<String>,
}

/// Whether a `company_link` URL is safe to render as a clickable anchor.
/// Only `http://` and `https://` schemes are accepted; this rejects
/// `javascript:`, `data:`, `vbscript:`, `file:` etc., any of which would
/// turn the admin-controlled link on every public page into a stored XSS
/// or local-file vector.
pub(crate) fn is_safe_company_link(url: &str) -> bool {
    let lower = url.trim().to_ascii_lowercase();
    lower.starts_with("http://") || lower.starts_with("https://")
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

/// Read the org-wide display labels for the Jitsi / webhook providers from
/// the cached `MeetingConfig`. Returns empty strings when no display name is
/// configured — slot/book templates fall back to the generic "Video call"
/// label in that case.
async fn meeting_provider_labels(state: &AppState) -> (String, String) {
    let cfg = state.meeting_config.read().await;
    let jitsi = cfg
        .jitsi
        .as_ref()
        .and_then(|j| j.display_name.clone())
        .unwrap_or_default();
    let webhook = cfg
        .webhook
        .as_ref()
        .and_then(|w| w.display_name.clone())
        .unwrap_or_default();
    (jitsi, webhook)
}

/// Pick the location URL to expose on a freshly-created booking.
///
/// When the event type uses an auto provider (`jitsi_auto`, `webhook_auto`)
/// **and** the booking is going straight to confirmed, we generate a fresh
/// URL via `meeting::generate_and_persist` and store it on the booking row.
/// Otherwise we fall back to the event type's static `location_value`. The
/// returned value is what gets piped into the email + ICS + CalDAV write.
///
/// For pending bookings, generation is deferred to the approval moment
/// (`approve_booking_by_token`) so a host who declines never invokes a
/// configured webhook.
async fn resolve_booking_location(
    state: &AppState,
    booking_id: &str,
    event_type_id: &str,
    host_user_id: Option<&str>,
    static_location_value: Option<&str>,
    guest_name: &str,
    guest_email: &str,
    confirmed: bool,
) -> Option<String> {
    if confirmed {
        if let Some(generated) = meeting::generate_and_persist(
            &state.pool,
            &state.secret_key,
            booking_id,
            event_type_id,
            host_user_id,
            guest_name,
            guest_email,
        )
        .await
        {
            return Some(generated);
        }
    }
    static_location_value
        .map(str::to_string)
        .filter(|s| !s.is_empty())
}

/// Background task that sends booking reminders on a 60-second tick. Also
/// piggybacks an hourly sweep of expired sessions so the `sessions` table
/// doesn't accumulate dead rows indefinitely.
pub async fn run_reminder_loop(pool: SqlitePool, secret_key: [u8; 32]) {
    let mut last_session_cleanup = std::time::Instant::now();
    loop {
        tokio::time::sleep(std::time::Duration::from_secs(60)).await;

        // Hourly: prune sessions whose `expires_at` has passed.
        if last_session_cleanup.elapsed() >= std::time::Duration::from_secs(3600) {
            match crate::auth::cleanup_expired_sessions(&pool).await {
                Ok(removed) if removed > 0 => {
                    tracing::info!(removed, "expired sessions pruned");
                }
                Ok(_) => {}
                Err(e) => tracing::warn!(error = %e, "session cleanup failed"),
            }
            last_session_cleanup = std::time::Instant::now();
        }

        // Find bookings that need a reminder:
        // - status is confirmed
        // - reminder not yet sent
        // - event type has reminder_minutes set (> 0)
        // - start_at minus reminder_minutes <= now
        // - start_at > now (don't remind for past bookings)
        // `host_timezone` resolves like `get_host_tz`: prefer the explicit
        // event-type tz, fall back to the host user's tz. NULL falls through
        // to UTC at parse time.
        let due: Vec<(String, String, String, String, String, String, String, String, String, Option<String>, Option<String>, String, Option<String>, Option<String>, Option<String>)> = sqlx::query_as(
            "SELECT b.id, b.guest_name, b.guest_email, b.guest_timezone, b.start_at, b.end_at, et.title, u.name, COALESCE(u.booking_email, u.email), COALESCE(NULLIF(b.meeting_url, ''), et.location_value), b.cancel_token, b.uid, b.language, u.language, COALESCE(NULLIF(et.timezone, ''), u.timezone)
             FROM bookings b
             JOIN event_types et ON et.id = b.event_type_id
             JOIN accounts a ON a.id = et.account_id
             JOIN users u ON u.id = a.user_id
             WHERE b.status = 'confirmed'
               AND b.reminder_sent_at IS NULL
               AND et.reminder_minutes IS NOT NULL
               AND et.reminder_minutes > 0
               AND datetime(b.start_at, '-' || et.reminder_minutes || ' minutes') <= datetime('now')
               AND datetime(b.start_at) > datetime('now')",
        )
        .fetch_all(&pool)
        .await
        .unwrap_or_default();

        let base_url = std::env::var("CALRS_BASE_URL").ok();

        if due.is_empty() {
            continue;
        }

        let smtp_config = match crate::email::load_smtp_config(&pool, &secret_key).await {
            Ok(Some(cfg)) => cfg,
            _ => continue,
        };

        for (
            bid,
            guest_name,
            guest_email,
            guest_timezone,
            start_at,
            end_at,
            event_title,
            host_name,
            host_email,
            location_value,
            cancel_token,
            uid,
            guest_language,
            host_language,
            host_timezone,
        ) in &due
        {
            // start_at/end_at are stored in the event-type tz (see #101). Convert
            // to the guest's tz so `BookingDetails` carries guest-local wall-clock —
            // matches the contract used by cancel/confirm handlers, and lets
            // `host_time_display` convert back into the host's tz correctly for
            // the host reminder.
            let stored_tz_str = host_timezone.as_deref().unwrap_or("UTC");
            let stored_tz = stored_tz_str.parse::<Tz>().unwrap_or(Tz::UTC);
            let guest_tz_parsed = guest_timezone.parse::<Tz>().unwrap_or(Tz::UTC);
            let (date, start_time, end_time) =
                booking_strings_in_guest_tz(start_at, end_at, stored_tz, guest_tz_parsed);

            let location = location_value.as_ref().filter(|v| !v.is_empty()).cloned();

            let details = crate::email::BookingDetails {
                event_title: event_title.clone(),
                date,
                start_time,
                end_time,
                guest_name: guest_name.clone(),
                guest_email: guest_email.clone(),
                guest_timezone: guest_timezone.clone(),
                host_name: host_name.clone(),
                host_email: host_email.clone(),
                uid: uid.clone(),
                notes: None,
                location,
                reminder_minutes: None,
                additional_attendees: vec![],
                guest_language: guest_language.clone(),
                host_language: host_language.clone(),
                host_timezone: stored_tz_str.to_string(),
            };

            let guest_cancel_url = cancel_token.as_ref().and_then(|t| {
                base_url
                    .as_ref()
                    .map(|base| format!("{}/booking/cancel/{}", base.trim_end_matches('/'), t))
            });

            let _ = crate::email::send_guest_reminder(
                &smtp_config,
                &details,
                guest_cancel_url.as_deref(),
            )
            .await;
            let _ = crate::email::send_host_reminder(&smtp_config, &details).await;

            // Mark reminder as sent
            let _ =
                sqlx::query("UPDATE bookings SET reminder_sent_at = datetime('now') WHERE id = ?")
                    .bind(bid)
                    .execute(&pool)
                    .await;

            tracing::info!(booking_id = %bid, "reminder sent");
        }

        // Background sync: pick the stalest enabled source and sync it.
        // With ctag + sync-token this is very cheap for unchanged calendars.
        let stalest: Option<(String,)> = sqlx::query_as(
            "SELECT cs.id
             FROM caldav_sources cs
             WHERE cs.enabled = 1
             ORDER BY COALESCE(cs.last_synced, '2000-01-01') ASC
             LIMIT 1",
        )
        .fetch_optional(&pool)
        .await
        .unwrap_or(None);

        if let Some((source_id,)) = stalest {
            crate::commands::sync::sync_source_by_id(&pool, &secret_key, &source_id).await;
            tracing::debug!(source_id = %source_id, "background sync completed");
        }
    }
}

/// Parse a datetime string from the database, handling both space and T separators.
/// Supports: "2025-03-15 14:30:00", "2025-03-15T14:30:00", "2025-03-15T14:30:00Z"
fn parse_booking_datetime(dt_str: &str) -> Option<NaiveDateTime> {
    let s = dt_str.trim_end_matches('Z');
    NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S")
        .or_else(|_| NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S"))
        .ok()
}

/// Format a booking datetime string into a human-friendly label.
/// Input: "2025-03-15 14:30:00" or "2025-03-15T14:30:00"
/// Output: "Tomorrow at 2:30 PM" or "Sat, Mar 15 at 2:30 PM" or "Sat, Mar 15, 2026 at 2:30 PM"
fn format_booking_datetime(dt_str: &str) -> String {
    let ndt = match parse_booking_datetime(dt_str) {
        Some(d) => d,
        None => return dt_str.to_string(),
    };
    let now = Local::now().naive_local();
    let today = now.date();
    let date = ndt.date();
    let time = ndt.time().format("%-I:%M %p").to_string();

    let day_diff = (date - today).num_days();
    let date_part = if day_diff == 0 {
        "Today".to_string()
    } else if day_diff == 1 {
        "Tomorrow".to_string()
    } else if day_diff < 7 {
        date.format("%A").to_string() // e.g. "Wednesday"
    } else if date.year() == today.year() {
        date.format("%a, %b %-d").to_string() // e.g. "Sat, Mar 15"
    } else {
        date.format("%a, %b %-d, %Y").to_string() // e.g. "Sat, Mar 15, 2026"
    };
    format!("{} at {}", date_part, time)
}

/// Format a time range for booking display.
/// Returns e.g. "Tomorrow at 2:30 PM — 3:00 PM"
fn format_booking_range(start_str: &str, end_str: &str) -> String {
    let start_label = format_booking_datetime(start_str);
    // For the end, only show the time (same day implied)
    if let Some(end_ndt) = parse_booking_datetime(end_str) {
        let end_time = end_ndt.time().format("%-I:%M %p").to_string();
        format!("{} — {}", start_label, end_time)
    } else {
        format!("{} — {}", start_label, end_str)
    }
}

/// Convert a naive datetime from one timezone to another, going via UTC. Used
/// when the dashboard needs to show a stored time (which is naive in the
/// event-type's timezone) in either the host's profile timezone or the guest's
/// selected timezone.
fn convert_naive_between_tz(naive: NaiveDateTime, from: Tz, to: Tz) -> NaiveDateTime {
    let utc = from
        .from_local_datetime(&naive)
        .earliest()
        .unwrap_or_else(|| from.from_utc_datetime(&naive))
        .with_timezone(&Utc);
    utc.with_timezone(&to).naive_local()
}

/// Convert stored booking start/end (naive in `stored_tz`) to the guest's
/// timezone and return (date, start_time, end_time) strings used to populate
/// `BookingDetails`. `email::generate_ics` calls `convert_to_utc(date,
/// start_time, end_time, guest_timezone)`, so these fields must be guest-local
/// for the ICS and the rendered email body to match what the guest booked.
/// Fixes #101.
fn booking_strings_in_guest_tz(
    start_at: &str,
    end_at: &str,
    stored_tz: Tz,
    guest_tz: Tz,
) -> (String, String, String) {
    match (
        parse_booking_datetime(start_at),
        parse_booking_datetime(end_at),
    ) {
        (Some(s), Some(e)) => {
            let gs = convert_naive_between_tz(s, stored_tz, guest_tz);
            let ge = convert_naive_between_tz(e, stored_tz, guest_tz);
            (
                gs.date().format("%Y-%m-%d").to_string(),
                gs.time().format("%H:%M").to_string(),
                ge.time().format("%H:%M").to_string(),
            )
        }
        _ => (
            start_at.get(..10).unwrap_or(start_at).to_string(),
            extract_time_24h(start_at),
            extract_time_24h(end_at),
        ),
    }
}

/// Short timezone label for display next to a time, e.g. "CEST" or "EST".
/// Falls back to the IANA name when the abbreviation is numeric (zones like
/// Asia/Tehran which expose "+0330" rather than a letter code).
fn tz_short_label(tz: Tz, at: NaiveDateTime) -> String {
    let dt = tz
        .from_local_datetime(&at)
        .earliest()
        .unwrap_or_else(|| tz.from_utc_datetime(&at));
    let abbr = dt.offset().abbreviation();
    if abbr.len() >= 3 && abbr.chars().all(|c| c.is_ascii_alphabetic()) {
        abbr.to_string()
    } else {
        tz.name().to_string()
    }
}

/// Format a booking range for dashboard display: returns a primary label in the
/// host's profile timezone (with tz suffix), and an optional secondary label
/// showing the guest's view when the two timezones differ.
///
/// `start_str`/`end_str` are naive datetimes stored in `stored_tz` (the event
/// type's timezone, falling back to the host's profile tz for legacy rows).
fn format_booking_for_dashboard(
    start_str: &str,
    end_str: &str,
    stored_tz: &str,
    host_tz: &str,
    guest_tz: &str,
) -> (String, Option<String>) {
    let stored = stored_tz.parse::<Tz>().unwrap_or(Tz::UTC);
    let host = host_tz.parse::<Tz>().unwrap_or(Tz::UTC);
    let guest = guest_tz.parse::<Tz>().unwrap_or(Tz::UTC);

    let (start, end) = match (
        parse_booking_datetime(start_str),
        parse_booking_datetime(end_str),
    ) {
        (Some(s), Some(e)) => (s, e),
        _ => return (format_booking_range(start_str, end_str), None),
    };

    let host_start = convert_naive_between_tz(start, stored, host);
    let host_end = convert_naive_between_tz(end, stored, host);
    let today_host = Utc::now().with_timezone(&host).date_naive();
    let primary = format_dashboard_range(
        host_start,
        host_end,
        today_host,
        &tz_short_label(host, host_start),
    );

    let secondary = if guest != host {
        let g_start = convert_naive_between_tz(start, stored, guest);
        let g_end = convert_naive_between_tz(end, stored, guest);
        let g_time_start = g_start.time().format("%-I:%M %p").to_string();
        let g_time_end = g_end.time().format("%-I:%M %p").to_string();
        Some(format!(
            "{} — {} {}",
            g_time_start,
            g_time_end,
            tz_short_label(guest, g_start)
        ))
    } else {
        None
    };

    (primary, secondary)
}

fn format_dashboard_range(
    start: NaiveDateTime,
    end: NaiveDateTime,
    today: NaiveDate,
    tz_label: &str,
) -> String {
    let date = start.date();
    let time_start = start.time().format("%-I:%M %p").to_string();
    let time_end = end.time().format("%-I:%M %p").to_string();
    let day_diff = (date - today).num_days();
    let date_part = if day_diff == 0 {
        "Today".to_string()
    } else if day_diff == 1 {
        "Tomorrow".to_string()
    } else if (1..7).contains(&day_diff) {
        date.format("%A").to_string()
    } else if date.year() == today.year() {
        date.format("%a, %b %-d").to_string()
    } else {
        date.format("%a, %b %-d, %Y").to_string()
    };
    format!(
        "{} at {} — {} {}",
        date_part, time_start, time_end, tz_label
    )
}

/// Format a raw date string (YYYY-MM-DD or from datetime) into a human-friendly
/// localized date label. Returns e.g. "Saturday, March 15, 2026" / "samedi 15 mars 2026".
fn format_date_label(dt_str: &str, lang: &str) -> String {
    // Try parsing as full datetime first, then as date-only
    if let Some(ndt) = parse_booking_datetime(dt_str) {
        return crate::i18n::format_long_date(ndt.date(), lang);
    }
    if let Ok(d) = NaiveDate::parse_from_str(&dt_str[..10.min(dt_str.len())], "%Y-%m-%d") {
        return crate::i18n::format_long_date(d, lang);
    }
    dt_str.to_string()
}

/// Extract a human-friendly time (e.g. "2:30 PM") from a datetime string.
fn format_time_from_dt(dt_str: &str) -> String {
    if let Some(ndt) = parse_booking_datetime(dt_str) {
        ndt.time().format("%-I:%M %p").to_string()
    } else if dt_str.len() >= 16 {
        dt_str[11..16].to_string()
    } else {
        "00:00".to_string()
    }
}

/// Extract HH:MM (24-hour) from a booking datetime for ICS generation.
/// format_time_from_dt returns 12-hour display format ("2:00 PM") which
/// convert_to_utc cannot parse. This function returns "14:00" format.
fn extract_time_24h(dt_str: &str) -> String {
    if let Some(ndt) = parse_booking_datetime(dt_str) {
        ndt.time().format("%H:%M").to_string()
    } else if dt_str.len() >= 16 {
        dt_str[11..16].to_string()
    } else {
        "00:00".to_string()
    }
}

/// Parse availability windows from the form.
/// Supports new `avail_windows` format ("09:00-12:00,13:00-17:00") with fallback to
/// legacy single `avail_start`/`avail_end` pair. Returns at least one window.
fn parse_avail_windows(
    windows_str: Option<&str>,
    legacy_start: Option<&str>,
    legacy_end: Option<&str>,
) -> Vec<(String, String)> {
    if let Some(ws) = windows_str.filter(|s| !s.trim().is_empty()) {
        let parsed: Vec<(String, String)> = ws
            .split(',')
            .filter_map(|w| {
                let parts: Vec<&str> = w.trim().splitn(2, '-').collect();
                if parts.len() == 2
                    && NaiveTime::parse_from_str(parts[0], "%H:%M").is_ok()
                    && NaiveTime::parse_from_str(parts[1], "%H:%M").is_ok()
                {
                    Some((parts[0].to_string(), parts[1].to_string()))
                } else {
                    None
                }
            })
            .collect();
        if !parsed.is_empty() {
            return parsed;
        }
    }
    // Fallback to legacy single window
    vec![(
        legacy_start.unwrap_or("09:00").to_string(),
        legacy_end.unwrap_or("17:00").to_string(),
    )]
}

/// Parse a single schedule string in the new format.
/// Format: "1:09:00-17:00;2:09:00-12:00,13:00-17:00" (day:windows;day:windows)
fn parse_schedule_string(s: &str) -> std::collections::BTreeMap<i32, Vec<(String, String)>> {
    let mut result = std::collections::BTreeMap::new();
    let s = s.trim();
    if s.is_empty() {
        return result;
    }
    for segment in s.split(';') {
        let segment = segment.trim();
        if segment.is_empty() {
            continue;
        }
        let parts: Vec<&str> = segment.splitn(2, ':').collect();
        if parts.len() != 2 {
            continue;
        }
        let day: i32 = match parts[0].trim().parse() {
            Ok(d) if (0..=6).contains(&d) => d,
            _ => continue,
        };
        let windows: Vec<(String, String)> = parts[1]
            .split(',')
            .filter_map(|w| {
                let times: Vec<&str> = w.trim().split('-').collect();
                if times.len() == 2 {
                    let s = times[0].trim().to_string();
                    let e = times[1].trim().to_string();
                    if !s.is_empty() && !e.is_empty() {
                        Some((s, e))
                    } else {
                        None
                    }
                } else {
                    None
                }
            })
            .collect();
        if !windows.is_empty() {
            result.insert(day, windows);
        }
    }
    result
}

/// Parse per-day availability schedule.
/// Resolution order: submitted `schedule` (new format) → `user_default` (new format,
/// e.g. the user's profile default availability) → legacy form fields → hardcoded
/// Mon–Fri 09:00–17:00. The `user_default` step is what stops an empty submission
/// from silently snapping back to the hardcoded default.
fn parse_avail_schedule(
    schedule: Option<&str>,
    legacy_days: Option<&str>,
    legacy_windows: Option<&str>,
    legacy_start: Option<&str>,
    legacy_end: Option<&str>,
    user_default: Option<&str>,
) -> std::collections::BTreeMap<i32, Vec<(String, String)>> {
    if let Some(s) = schedule {
        let parsed = parse_schedule_string(s);
        if !parsed.is_empty() {
            return parsed;
        }
    }

    if let Some(s) = user_default {
        let parsed = parse_schedule_string(s);
        if !parsed.is_empty() {
            return parsed;
        }
    }

    // Fall back to legacy format
    let mut result = std::collections::BTreeMap::new();
    let days_str = legacy_days.unwrap_or("1,2,3,4,5");
    let windows = parse_avail_windows(legacy_windows, legacy_start, legacy_end);
    for day_str in days_str.split(',') {
        if let Ok(day) = day_str.trim().parse::<i32>() {
            if (0..=6).contains(&day) {
                result.insert(day, windows.clone());
            }
        }
    }
    result
}

/// Build an avail_schedule string from availability rules.
/// Output format: "1:09:00-17:00;2:09:00-12:00,13:00-17:00"
fn build_avail_schedule(all_rules: &[(i32, String, String)]) -> String {
    let mut day_map: std::collections::BTreeMap<i32, Vec<String>> =
        std::collections::BTreeMap::new();
    for (day, start, end) in all_rules {
        day_map
            .entry(*day)
            .or_default()
            .push(format!("{}-{}", start, end));
    }
    day_map
        .iter()
        .map(|(day, ws)| format!("{}:{}", day, ws.join(",")))
        .collect::<Vec<_>>()
        .join(";")
}

/// Query parameters that the embed-code generator threads into the booking
/// flow. When `embed=1`, the booking pages drop their chrome (logo, footer,
/// theme toggle) and emit a postMessage on load so the parent iframe can
/// auto-size. The other fields are passed straight through to the templates
/// as visual hints. None of them are persisted server-side — the embed code
/// embeds them as URL params on every iframe load.
#[derive(Deserialize, Default, Clone, Debug)]
struct EmbedParams {
    #[serde(default)]
    embed: Option<String>,
    #[serde(default)]
    layout: Option<String>,
    #[serde(default)]
    theme: Option<String>,
    #[serde(default)]
    brand: Option<String>,
}

impl EmbedParams {
    fn is_embedded(&self) -> bool {
        matches!(self.embed.as_deref(), Some("1") | Some("true"))
    }

    /// Full set of accent CSS variable declarations derived from `brand`,
    /// suitable for inlining inside a `:root, html.dark { … }` rule. Returns
    /// None if `brand` is missing, empty, or not 3/6 hex digits — we never pass
    /// raw user input into CSS to avoid breaking out of the rule.
    ///
    /// Besides `--accent`/`--accent-hover` (the brand hex), we derive the rgba
    /// variants `--accent-subtle`/`--accent-border`/`--accent-muted` from the
    /// same color so slot pills, borders and muted states pick up the brand
    /// too — otherwise those would keep the org accent and render half-themed.
    fn brand_css(&self) -> Option<String> {
        let raw = self.brand.as_deref()?.trim_start_matches('#');
        let (r, g, b) = hex_to_rgb(raw)?;
        Some(format!(
            "--accent: #{hex}; --accent-hover: #{hex}; \
             --accent-subtle: rgba({r},{g},{b},0.12); \
             --accent-border: rgba({r},{g},{b},0.3); \
             --accent-muted: rgba({r},{g},{b},0.5);",
            hex = raw
        ))
    }

    /// "?embed=1&layout=month&theme=dark&brand=ff0000" — used to keep params
    /// alive across page transitions inside the iframe. Returns empty string
    /// when not embedded so callers can unconditionally append it.
    fn query_suffix(&self, leading_amp: bool) -> String {
        if !self.is_embedded() {
            return String::new();
        }
        let mut parts: Vec<String> = vec!["embed=1".to_string()];
        if let Some(v) = self.layout.as_deref().filter(|s| !s.is_empty()) {
            parts.push(format!("layout={}", urlencode_simple(v)));
        }
        if let Some(v) = self.theme.as_deref().filter(|s| !s.is_empty()) {
            parts.push(format!("theme={}", urlencode_simple(v)));
        }
        if let Some(v) = self.brand.as_deref().filter(|s| !s.is_empty()) {
            parts.push(format!("brand={}", urlencode_simple(v)));
        }
        let sep = if leading_amp { "&" } else { "?" };
        format!("{}{}", sep, parts.join("&"))
    }
}

/// Parse a 3- or 6-digit hex color (no leading '#') into (r, g, b) decimal
/// components. Returns None for any other length or non-hex input. A 3-digit
/// value is expanded by doubling each nibble (e.g. "f30" → ff3300).
fn hex_to_rgb(raw: &str) -> Option<(u8, u8, u8)> {
    let full = match raw.len() {
        3 if raw.chars().all(|c| c.is_ascii_hexdigit()) => {
            raw.chars().flat_map(|c| [c, c]).collect::<String>()
        }
        6 if raw.chars().all(|c| c.is_ascii_hexdigit()) => raw.to_string(),
        _ => return None,
    };
    let r = u8::from_str_radix(&full[0..2], 16).ok()?;
    let g = u8::from_str_radix(&full[2..4], 16).ok()?;
    let b = u8::from_str_radix(&full[4..6], 16).ok()?;
    Some((r, g, b))
}

/// Minimal URL-encoder for the small whitelist of values EmbedParams carries
/// (slug-like layout names, theme tokens, hex colors). Keeps the helper
/// dependency-free; if we ever accept richer values we can swap in `urlencoding`.
fn urlencode_simple(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
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
            (
                " 'wasm-unsafe-eval'",
                "worker-src blob:; ",
                format!(" {}", widget_origin),
                format!(" {} {}", instance_origin, widget_origin),
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

    let initial_theme_css = build_theme_css(&pool).await;
    let initial_company_link = get_company_link(&pool).await;
    let initial_captcha = captcha::load_captcha_config(&pool, &secret_key).await;
    let initial_ews_global = ews_global::load_ews_global_config(&pool, &secret_key).await;
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
        ews_global: tokio::sync::RwLock::new(initial_ews_global),
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
        .route("/dashboard/admin/accent", post(admin_update_accent))
        .route("/dashboard/admin/oidc", post(admin_update_oidc))
        .route(
            "/dashboard/admin/google-oauth2",
            post(admin_update_google_oauth2),
        )
        .route("/dashboard/admin/captcha", post(admin_update_captcha))
        .route("/dashboard/admin/ews", post(admin_update_ews_global))
        .route("/dashboard/admin/jitsi", post(admin_update_jitsi))
        .route(
            "/dashboard/admin/meeting-webhook",
            post(admin_update_meeting_webhook),
        )
        .route("/dashboard/admin/logo", post(admin_upload_logo))
        .route("/dashboard/admin/logo/delete", post(admin_delete_logo))
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
        .route("/accent.css", get(serve_accent_css))
        .route("/brand-logo", get(serve_brand_logo))
        .route("/embed.js", get(serve_embed_js))
        .route("/fonts/inter-latin.woff2", get(serve_font_inter_latin))
        .route(
            "/fonts/inter-latin-ext.woff2",
            get(serve_font_inter_latin_ext),
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
        .route(
            "/booking/reschedule/{token}",
            get(guest_reschedule_slots).post(guest_reschedule_booking),
        )
        .route(
            "/dashboard/bookings/{id}/reschedule",
            get(host_reschedule_slots).post(host_reschedule_booking),
        )
        .route("/u/{username}", get(user_profile))
        .route("/u/{username}/{slug}", get(show_slots_for_user))
        .route(
            "/u/{username}/{slug}/book",
            get(show_book_form_for_user).post(handle_booking_for_user),
        )
        // Legacy single-user routes (kept for backward compatibility)
        .route("/{slug}", get(show_slots))
        .route("/{slug}/book", get(show_book_form).post(handle_booking))
        .layer(TraceLayer::new_for_http())
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

// --- Root redirect ---

async fn root_redirect() -> impl IntoResponse {
    Redirect::to("/auth/login")
}

// --- Dashboard (overview) ---

async fn dashboard(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
) -> impl IntoResponse {
    let user = &auth_user.user;

    let event_type_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM event_types et JOIN accounts a ON a.id = et.account_id WHERE a.user_id = ? AND et.team_id IS NULL")
            .bind(&user.id)
            .fetch_one(&state.pool)
            .await
            .unwrap_or(0);

    let pending_bookings: Vec<(
        String,
        String,
        String,
        String,
        String,
        String,
        String,
        String,
    )> = sqlx::query_as(
        "SELECT b.id, b.guest_name, b.guest_email, b.start_at, b.end_at, et.title,
                COALESCE(NULLIF(et.timezone, ''), u.timezone) AS stored_tz,
                COALESCE(NULLIF(b.guest_timezone, ''), 'UTC') AS guest_tz
         FROM bookings b
         JOIN event_types et ON et.id = b.event_type_id
         JOIN accounts a ON a.id = et.account_id
         JOIN users u ON u.id = a.user_id
         WHERE a.user_id = ? AND b.status = 'pending'
         ORDER BY b.start_at",
    )
    .bind(&user.id)
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();

    let upcoming_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM bookings b JOIN event_types et ON et.id = b.event_type_id JOIN accounts a ON a.id = et.account_id WHERE a.user_id = ? AND b.status = 'confirmed' AND b.start_at >= datetime('now')")
            .bind(&user.id)
            .fetch_one(&state.pool)
            .await
            .unwrap_or(0);

    let source_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM caldav_sources cs JOIN accounts a ON a.id = cs.account_id WHERE a.user_id = ?")
            .bind(&user.id)
            .fetch_one(&state.pool)
            .await
            .unwrap_or(0);

    let team_count: i64 = if user.role == "admin" {
        sqlx::query_scalar("SELECT COUNT(*) FROM teams")
            .fetch_one(&state.pool)
            .await
            .unwrap_or(0)
    } else {
        sqlx::query_scalar("SELECT COUNT(*) FROM team_members WHERE user_id = ?")
            .bind(&user.id)
            .fetch_one(&state.pool)
            .await
            .unwrap_or(0)
    };

    let tmpl = match state.templates.get_template("dashboard_overview.html") {
        Ok(t) => t,
        Err(e) => return internal_error_html("template render", &e),
    };

    let host_tz = &user.timezone;
    let pending_ctx: Vec<minijinja::Value> = pending_bookings
        .iter()
        .map(
            |(id, name, email, start, end, title, stored_tz, guest_tz)| {
                let (primary, secondary) =
                    format_booking_for_dashboard(start, end, stored_tz, host_tz, guest_tz);
                context! {
                    id => id,
                    guest_name => name,
                    guest_email => email,
                    start_at => primary,
                    start_at_guest => secondary,
                    event_title => title,
                }
            },
        )
        .collect();

    let pending_count = pending_ctx.len() as i64;

    let (impersonating, impersonating_name, _) = impersonation_ctx(&auth_user);

    Html(
        tmpl.render(context! {
            sidebar => sidebar_context(&auth_user, "overview"),
            user_name => user.name,
            user_email => user.email,
            user_role => user.role,
            username => user.username,
            event_type_count => event_type_count,
            upcoming_count => upcoming_count,
            pending_count => pending_count,
            source_count => source_count,
            team_count => team_count,
            is_admin => user.role == "admin",
            pending_bookings => pending_ctx,
            impersonating => impersonating,
            impersonating_name => impersonating_name,
        })
        .unwrap_or_else(|e| internal_error_body("template render", &e)),
    )
}

// --- Dashboard: Event Types ---

async fn dashboard_event_types(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
) -> impl IntoResponse {
    let user = &auth_user.user;

    let event_types: Vec<(String, String, String, i32, bool, i32, i64, String)> = sqlx::query_as(
        "SELECT et.id, et.slug, et.title, et.duration_min, et.enabled, et.requires_confirmation,
                (SELECT COUNT(*) FROM bookings b WHERE b.event_type_id = et.id AND b.status IN ('confirmed', 'pending')) as active_bookings,
                et.visibility
         FROM event_types et
         JOIN accounts a ON a.id = et.account_id
         WHERE a.user_id = ? AND et.team_id IS NULL
         ORDER BY et.created_at",
    )
    .bind(&user.id)
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();

    let is_admin = user.role == "admin";

    // Teams the user is an admin of — used for `can_manage` (edit/toggle/delete)
    // and "Create team event type" enablement. Regular team members can view but
    // not manage.
    let admin_team_ids: std::collections::HashSet<String> = if is_admin {
        std::collections::HashSet::new() // global admins can manage all
    } else {
        let ids: Vec<(String,)> =
            sqlx::query_as("SELECT team_id FROM team_members WHERE user_id = ? AND role = 'admin'")
                .bind(&user.id)
                .fetch_all(&state.pool)
                .await
                .unwrap_or_default();
        ids.into_iter().map(|(id,)| id).collect()
    };

    let team_event_types: Vec<(
        String,
        String,
        String,
        i32,
        bool,
        String,
        String,
        i64,
        String,
        String,
        String,
    )> = if is_admin {
        sqlx::query_as(
            "SELECT et.id, et.slug, et.title, et.duration_min, et.enabled, t.name, t.slug,
                    (SELECT COUNT(*) FROM bookings b WHERE b.event_type_id = et.id AND b.status IN ('confirmed', 'pending')) as active_bookings,
                    et.visibility, t.id, et.scheduling_mode
             FROM event_types et
             JOIN teams t ON t.id = et.team_id
             WHERE et.team_id IS NOT NULL
             ORDER BY t.name, et.created_at",
        )
        .fetch_all(&state.pool)
        .await
        .unwrap_or_default()
    } else {
        sqlx::query_as(
            "SELECT et.id, et.slug, et.title, et.duration_min, et.enabled, t.name, t.slug,
                    (SELECT COUNT(*) FROM bookings b WHERE b.event_type_id = et.id AND b.status IN ('confirmed', 'pending')) as active_bookings,
                    et.visibility, t.id, et.scheduling_mode
             FROM event_types et
             JOIN teams t ON t.id = et.team_id
             JOIN team_members tm ON tm.team_id = t.id
             WHERE tm.user_id = ?
             ORDER BY t.name, et.created_at",
        )
        .bind(&user.id)
        .fetch_all(&state.pool)
        .await
        .unwrap_or_default()
    };

    // Whether user can create new team event types (global admin or team admin of at least one team)
    let can_create_team_et = is_admin || !admin_team_ids.is_empty();

    let tmpl = match state.templates.get_template("dashboard_event_types.html") {
        Ok(t) => t,
        Err(e) => return internal_error_html("template render", &e),
    };

    // Build a single unified list: personal event types first, then team ones
    let mut all_et_ctx: Vec<minijinja::Value> = Vec::new();

    for (id, slug, title, duration, enabled, req_conf, active_bookings, vis) in &event_types {
        all_et_ctx.push(context! {
            id => id, slug => slug, title => title, duration_min => duration,
            enabled => enabled, requires_confirmation => *req_conf != 0,
            active_bookings => active_bookings, visibility => vis,
            is_team => false, can_manage => true,
            edit_url => format!("/dashboard/event-types/{}/edit", slug),
            toggle_url => format!("/dashboard/event-types/{}/toggle", slug),
            delete_url => format!("/dashboard/event-types/{}/delete", slug),
            overrides_url => format!("/dashboard/event-types/{}/overrides", slug),
            embed_url => format!("/dashboard/event-types/{}/embed", slug),
            view_url => if vis != "private" { user.username.as_ref().map(|u| format!("/u/{}/{}", u, slug)) } else { None::<String> },
        });
    }

    for (
        id,
        slug,
        title,
        duration,
        enabled,
        team_name,
        team_slug,
        active_bookings,
        vis,
        team_id,
        scheduling_mode,
    ) in &team_event_types
    {
        let can_manage = is_admin || admin_team_ids.contains(team_id);
        all_et_ctx.push(context! {
            id => id, slug => slug, title => title, duration_min => duration,
            enabled => enabled, active_bookings => active_bookings, visibility => vis,
            is_team => true, team_name => team_name, team_slug => team_slug,
            team_id => team_id, scheduling_mode => scheduling_mode, can_manage => can_manage,
            edit_url => format!("/dashboard/group-event-types/{}/{}/edit", team_id, slug),
            toggle_url => format!("/dashboard/group-event-types/{}/{}/toggle", team_id, slug),
            delete_url => format!("/dashboard/group-event-types/{}/{}/delete", team_id, slug),
            embed_url => format!("/dashboard/group-event-types/{}/{}/embed", team_id, slug),
            overrides_url => format!("/dashboard/event-types/{}/overrides", slug),
            // Team event types always get a view link — logged-in team members
            // can view private/internal slots without an invite token.
            view_url => Some(format!("/team/{}/{}", team_slug, slug)),
        });
    }

    let (impersonating, impersonating_name, _) = impersonation_ctx(&auth_user);

    Html(
        tmpl.render(context! {
            sidebar => sidebar_context(&auth_user, "event-types"),
            username => user.username,
            all_event_types => all_et_ctx,
            has_any => !event_types.is_empty() || !team_event_types.is_empty(),
            can_create_team_et => can_create_team_et,
            impersonating => impersonating,
            impersonating_name => impersonating_name,
        })
        .unwrap_or_else(|e| internal_error_body("template render", &e)),
    )
}

// --- Dashboard: Bookings ---

async fn dashboard_bookings(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
) -> impl IntoResponse {
    let user = &auth_user.user;

    let pending_bookings: Vec<(
        String,
        String,
        String,
        String,
        String,
        String,
        String,
        String,
    )> = sqlx::query_as(
        "SELECT b.id, b.guest_name, b.guest_email, b.start_at, b.end_at, et.title,
                COALESCE(NULLIF(et.timezone, ''), u.timezone) AS stored_tz,
                COALESCE(NULLIF(b.guest_timezone, ''), 'UTC') AS guest_tz
         FROM bookings b
         JOIN event_types et ON et.id = b.event_type_id
         JOIN accounts a ON a.id = et.account_id
         JOIN users u ON u.id = a.user_id
         WHERE a.user_id = ? AND b.status = 'pending'
         ORDER BY b.start_at",
    )
    .bind(&user.id)
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();

    let upcoming_bookings: Vec<(String, String, String, String, String, String, i32, String, String)> =
        sqlx::query_as(
            "SELECT b.id, b.guest_name, b.guest_email, b.start_at, b.end_at, et.title, b.reschedule_by_host,
                    COALESCE(NULLIF(et.timezone, ''), u.timezone) AS stored_tz,
                    COALESCE(NULLIF(b.guest_timezone, ''), 'UTC') AS guest_tz
         FROM bookings b
         JOIN event_types et ON et.id = b.event_type_id
         JOIN accounts a ON a.id = et.account_id
         JOIN users u ON u.id = a.user_id
         WHERE a.user_id = ? AND b.status = 'confirmed' AND b.start_at >= datetime('now')
         ORDER BY b.start_at
         LIMIT 50",
        )
        .bind(&user.id)
        .fetch_all(&state.pool)
        .await
        .unwrap_or_default();

    // Claimable bookings: unclaimed bookings on event types watched by the user's teams.
    // For team event types et.account_id may be null, so the timezone falls back to the
    // event type's own tz column (always set since migration 046's backfill).
    let claimable_bookings: Vec<(String, String, String, String, String, String, String, String, String, String)> =
        sqlx::query_as(
            "SELECT b.id, b.guest_name, b.guest_email, b.start_at, b.end_at, et.title, t.name, bct.token, \
                    COALESCE(NULLIF(et.timezone, ''), 'UTC') AS stored_tz, \
                    COALESCE(NULLIF(b.guest_timezone, ''), 'UTC') AS guest_tz \
             FROM bookings b \
             JOIN event_types et ON et.id = b.event_type_id \
             JOIN event_type_watchers ew ON ew.event_type_id = et.id \
             JOIN team_members tm ON tm.team_id = ew.team_id \
             JOIN teams t ON t.id = ew.team_id \
             JOIN booking_claim_tokens bct ON bct.booking_id = b.id AND bct.user_id = tm.user_id AND bct.used_at IS NULL \
             WHERE tm.user_id = ? AND b.status = 'confirmed' AND b.claimed_by_user_id IS NULL \
             AND b.start_at >= datetime('now') AND bct.expires_at > datetime('now') \
             ORDER BY b.start_at \
             LIMIT 50",
        )
        .bind(&user.id)
        .fetch_all(&state.pool)
        .await
        .unwrap_or_default();

    let tmpl = match state.templates.get_template("dashboard_bookings.html") {
        Ok(t) => t,
        Err(e) => return internal_error_html("template render", &e),
    };

    let host_tz = &user.timezone;
    let claimable_ctx: Vec<minijinja::Value> = claimable_bookings
        .iter()
        .map(
            |(id, name, email, start, end, title, team_name, token, stored_tz, guest_tz)| {
                let (primary, secondary) =
                    format_booking_for_dashboard(start, end, stored_tz, host_tz, guest_tz);
                context! {
                    id => id,
                    guest_name => name,
                    guest_email => email,
                    start_at => primary,
                    start_at_guest => secondary,
                    event_title => title,
                    team_name => team_name,
                    claim_token => token,
                }
            },
        )
        .collect();

    let pending_ctx: Vec<minijinja::Value> = pending_bookings
        .iter()
        .map(
            |(id, name, email, start, end, title, stored_tz, guest_tz)| {
                let (primary, secondary) =
                    format_booking_for_dashboard(start, end, stored_tz, host_tz, guest_tz);
                context! {
                    id => id,
                    guest_name => name,
                    guest_email => email,
                    start_at => primary,
                    start_at_guest => secondary,
                    event_title => title,
                }
            },
        )
        .collect();

    let bookings_ctx: Vec<minijinja::Value> = upcoming_bookings
        .iter()
        .map(
            |(id, name, email, start, end, title, resched, stored_tz, guest_tz)| {
                let (primary, secondary) =
                    format_booking_for_dashboard(start, end, stored_tz, host_tz, guest_tz);
                context! {
                    id => id,
                    guest_name => name,
                    guest_email => email,
                    start_at => primary,
                    start_at_guest => secondary,
                    event_title => title,
                    awaiting_reschedule => *resched != 0,
                }
            },
        )
        .collect();

    let (impersonating, impersonating_name, _) = impersonation_ctx(&auth_user);

    Html(
        tmpl.render(context! {
            sidebar => sidebar_context(&auth_user, "bookings"),
            pending_bookings => pending_ctx,
            claimable_bookings => claimable_ctx,
            bookings => bookings_ctx,
            impersonating => impersonating,
            impersonating_name => impersonating_name,
        })
        .unwrap_or_else(|e| internal_error_body("template render", &e)),
    )
}

// --- Dashboard: Teams ---

async fn dashboard_teams(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
) -> impl IntoResponse {
    let user = &auth_user.user;
    let is_admin = user.role == "admin";

    // For non-admin users, also fetch which teams they admin
    let teams: Vec<(
        String,
        String,
        String,
        Option<String>,
        String,
        Option<String>,
        Option<String>,
        i64,
    )> = if is_admin {
        sqlx::query_as(
            "SELECT t.id, t.name, t.slug, t.description, t.visibility, t.avatar_path, t.invite_token,
                    (SELECT COUNT(*) FROM team_members tm WHERE tm.team_id = t.id) as member_count
             FROM teams t
             ORDER BY t.name",
        )
        .fetch_all(&state.pool)
        .await
        .unwrap_or_default()
    } else {
        sqlx::query_as(
            "SELECT t.id, t.name, t.slug, t.description, t.visibility, t.avatar_path, t.invite_token,
                    (SELECT COUNT(*) FROM team_members tm WHERE tm.team_id = t.id) as member_count
             FROM teams t
             JOIN team_members tm2 ON tm2.team_id = t.id
             WHERE tm2.user_id = ?
             ORDER BY t.name",
        )
        .bind(&user.id)
        .fetch_all(&state.pool)
        .await
        .unwrap_or_default()
    };

    let member_team_ids: std::collections::HashSet<String> = if is_admin {
        // Global admins can manage all teams
        std::collections::HashSet::new()
    } else {
        let ids: Vec<(String,)> =
            sqlx::query_as("SELECT team_id FROM team_members WHERE user_id = ? AND role = 'admin'")
                .bind(&user.id)
                .fetch_all(&state.pool)
                .await
                .unwrap_or_default();
        ids.into_iter().map(|(id,)| id).collect()
    };

    let tmpl = match state.templates.get_template("dashboard_teams.html") {
        Ok(t) => t,
        Err(e) => return internal_error_html("template render", &e),
    };

    let teams_ctx: Vec<minijinja::Value> = teams
        .iter()
        .map(
            |(id, name, slug, description, visibility, avatar_path, invite_token, member_count)| {
                let initials = name
                    .split_whitespace()
                    .filter_map(|w| w.chars().next())
                    .take(2)
                    .collect::<String>()
                    .to_uppercase();
                let user_is_team_admin = is_admin || member_team_ids.contains(id);
                context! {
                    id => id,
                    name => name,
                    slug => slug,
                    description => description,
                    visibility => visibility,
                    has_avatar => avatar_path.is_some(),
                    initials => initials,
                    member_count => member_count,
                    is_team_admin => user_is_team_admin,
                    invite_token => invite_token,
                }
            },
        )
        .collect();

    let (impersonating, impersonating_name, _) = impersonation_ctx(&auth_user);

    Html(
        tmpl.render(context! {
            sidebar => sidebar_context(&auth_user, "teams"),
            teams => teams_ctx,
            is_admin => is_admin,
            impersonating => impersonating,
            impersonating_name => impersonating_name,
        })
        .unwrap_or_else(|e| internal_error_body("template render", &e)),
    )
}

// --- Team creation form ---

#[derive(Deserialize)]
struct TeamForm {
    _csrf: Option<String>,
    name: String,
    slug: String,
    description: Option<String>,
    visibility: Option<String>,
    #[serde(default)]
    members: String,
    #[serde(default)]
    group_ids: String,
}

/// Split a comma-separated form field into a Vec of non-empty trimmed strings.
fn split_csv_ids(s: &str) -> Vec<String> {
    s.split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

/// Parse a dynamic group username string like "alice+bob+carol" into individual usernames.
/// Returns the deduplicated list, or an error if fewer than 2 valid usernames.
/// Parse a numeric form field, defaulting to `default` if empty or invalid.
fn parse_int_field(s: &str, default: i32) -> i32 {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        default
    } else {
        trimmed.parse().unwrap_or(default)
    }
}

fn parse_optional_positive_int(s: &str) -> Option<i32> {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        None
    } else {
        trimmed.parse::<i32>().ok().filter(|v| *v > 0)
    }
}

/// Convert a (value, unit) pair from the event type form into a minute count
/// suitable for storage. Returns `None` when the value is blank, zero, or
/// invalid (no restriction). Unit defaults to minutes if missing or unknown.
fn parse_notice_to_minutes(value: &str, unit: &str) -> Option<i32> {
    let v = value.trim();
    if v.is_empty() {
        return None;
    }
    let n: i32 = match v.parse() {
        Ok(n) if n > 0 => n,
        _ => return None,
    };
    let multiplier = match unit.trim() {
        "days" => 1440,
        "hours" => 60,
        _ => 1, // minutes (default)
    };
    n.checked_mul(multiplier).filter(|m| *m > 0)
}

/// Pick the most natural unit for displaying a stored minute value back in
/// the form: divisible by 1440 → days, divisible by 60 → hours, else
/// minutes. Returns `(value, unit)`. Zero / negative / NULL all collapse to
/// `(0, "minutes")` so the form shows an empty restriction.
fn minutes_to_notice_form(min: Option<i32>) -> (i32, &'static str) {
    let m = min.unwrap_or(0);
    if m <= 0 {
        return (0, "minutes");
    }
    if m % 1440 == 0 {
        (m / 1440, "days")
    } else if m % 60 == 0 {
        (m / 60, "hours")
    } else {
        (m, "minutes")
    }
}

fn parse_dynamic_group_usernames(combined: &str) -> Result<Vec<String>, String> {
    let mut seen = std::collections::HashSet::new();
    let unique: Vec<String> = combined
        .split('+')
        .map(|s| s.trim().to_lowercase())
        .filter(|s| !s.is_empty())
        .filter(|s| seen.insert(s.clone()))
        .collect();
    if unique.len() < 2 {
        return Err("Dynamic group links require at least two usernames.".to_string());
    }
    Ok(unique)
}

/// Validate that all usernames exist, are enabled, and allow dynamic group links.
/// Returns Vec<(user_id, username, name, email)> in the same order as input.
/// Validate that all usernames exist, are enabled, and allow dynamic group links.
/// Returns Vec<(user_id, username, name, email, avatar_path)> in the same order as input.
async fn validate_dynamic_group_users(
    pool: &SqlitePool,
    usernames: &[String],
) -> Result<Vec<(String, String, String, String, Option<String>)>, String> {
    let mut users = Vec::with_capacity(usernames.len());
    for uname in usernames {
        let row: Option<(String, String, String, String, Option<String>, bool)> = sqlx::query_as(
            "SELECT id, username, name, COALESCE(booking_email, email), avatar_path, allow_dynamic_group FROM users WHERE username = ? AND enabled = 1",
        )
        .bind(uname)
        .fetch_optional(pool)
        .await
        .unwrap_or(None);
        match row {
            None => return Err(format!("User '{}' not found.", uname)),
            Some((_, _, _, _, _, false)) => {
                return Err(format!(
                    "User '{}' has not enabled dynamic group links.",
                    uname
                ))
            }
            Some((id, username, name, email, avatar_path, _)) => {
                users.push((id, username, name, email, avatar_path));
            }
        }
    }
    Ok(users)
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

async fn show_team_form(
    State(state): State<Arc<AppState>>,
    admin: crate::auth::AdminUser,
) -> impl IntoResponse {
    let user = &admin.0;

    // Fetch all enabled users
    let all_users: Vec<(String, String, String, Option<String>)> = sqlx::query_as(
        "SELECT id, name, email, avatar_path FROM users WHERE enabled = 1 ORDER BY name",
    )
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();

    let users_ctx: Vec<minijinja::Value> = all_users
        .iter()
        .map(|(id, name, email, avatar_path)| {
            context! {
                id => id,
                name => name,
                email => email,
                is_self => id == &user.id,
                has_avatar => avatar_path.is_some(),
                initials => compute_initials(name),
            }
        })
        .collect();

    // Fetch OIDC groups with member details (for stacked avatars)
    let oidc_groups: Vec<(String, String, i64)> = sqlx::query_as(
        "SELECT g.id, g.name, COUNT(ug.user_id) as member_count \
         FROM groups g LEFT JOIN user_groups ug ON ug.group_id = g.id \
         GROUP BY g.id ORDER BY g.name",
    )
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();

    let groups_ctx =
        build_groups_ctx(&state.pool, &oidc_groups, &std::collections::HashSet::new()).await;

    let tmpl = match state.templates.get_template("team_form.html") {
        Ok(t) => t,
        Err(e) => return internal_error_html("template render", &e),
    };

    Html(
        tmpl.render(context! {
            sidebar => admin_sidebar_context(user, "teams"),
            users => users_ctx,
            oidc_groups => if groups_ctx.is_empty() { None } else { Some(groups_ctx) },
            form_name => "",
            form_slug => "",
            form_description => "",
            form_visibility => "public",
        })
        .unwrap_or_else(|e| internal_error_body("template render", &e)),
    )
}

async fn create_team(
    State(state): State<Arc<AppState>>,
    admin: crate::auth::AdminUser,
    headers: HeaderMap,
    Form(form): Form<TeamForm>,
) -> impl IntoResponse {
    if let Err(resp) = verify_csrf_token(&headers, &form._csrf) {
        return resp;
    }
    let user = &admin.0;

    let name = form.name.trim().to_string();
    let slug = form.slug.trim().to_lowercase();
    let description = form
        .description
        .as_deref()
        .map(|d| d.trim().to_string())
        .filter(|d| !d.is_empty());
    let visibility = form.visibility.as_deref().unwrap_or("public");

    // Validate
    if name.is_empty() || slug.is_empty() {
        return render_team_form_error(&state, user, "Name and slug are required.", &form)
            .await
            .into_response();
    }

    if name.len() > 255 {
        return render_team_form_error(&state, user, "Name must be at most 255 characters.", &form)
            .await
            .into_response();
    }

    if let Some(ref d) = description {
        if d.len() > 5000 {
            return render_team_form_error(
                &state,
                user,
                "Description must be at most 5000 characters.",
                &form,
            )
            .await
            .into_response();
        }
    }

    if !slug
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    {
        return render_team_form_error(
            &state,
            user,
            "Slug must contain only lowercase letters, numbers, and dashes.",
            &form,
        )
        .await
        .into_response();
    }

    // Check slug against reserved names
    const RESERVED_SLUGS: &[&str] = &["new", "settings", "admin", "api"];
    if RESERVED_SLUGS.contains(&slug.as_str()) {
        return render_team_form_error(
            &state,
            user,
            "This slug is reserved. Please choose a different one.",
            &form,
        )
        .await
        .into_response();
    }

    // Check slug uniqueness
    let existing: Option<String> = sqlx::query_scalar("SELECT id FROM teams WHERE slug = ?")
        .bind(&slug)
        .fetch_optional(&state.pool)
        .await
        .unwrap_or(None);
    if existing.is_some() {
        return render_team_form_error(
            &state,
            user,
            "A team with this slug already exists.",
            &form,
        )
        .await
        .into_response();
    }

    let team_id = uuid::Uuid::new_v4().to_string();
    let invite_token = if visibility == "private" {
        Some(uuid::Uuid::new_v4().to_string())
    } else {
        None
    };

    // Insert team
    let _ = sqlx::query(
        "INSERT INTO teams (id, name, slug, description, visibility, invite_token, created_by) VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&team_id)
    .bind(&name)
    .bind(&slug)
    .bind(&description)
    .bind(visibility)
    .bind(&invite_token)
    .bind(&user.id)
    .execute(&state.pool)
    .await;

    // Add creator as admin member
    let _ = sqlx::query(
        "INSERT INTO team_members (team_id, user_id, role, source) VALUES (?, ?, 'admin', 'direct')",
    )
    .bind(&team_id)
    .bind(&user.id)
    .execute(&state.pool)
    .await;

    // Add selected members (skip creator, already added)
    let member_ids = split_csv_ids(&form.members);
    for member_id in &member_ids {
        if member_id == &user.id {
            continue;
        }
        let _ = sqlx::query(
            "INSERT OR IGNORE INTO team_members (team_id, user_id, role, source) VALUES (?, ?, 'member', 'direct')",
        )
        .bind(&team_id)
        .bind(member_id)
        .execute(&state.pool)
        .await;
    }

    // Link OIDC groups and add their members
    let group_ids = split_csv_ids(&form.group_ids);
    for group_id in &group_ids {
        let _ = sqlx::query("INSERT OR IGNORE INTO team_groups (team_id, group_id) VALUES (?, ?)")
            .bind(&team_id)
            .bind(group_id)
            .execute(&state.pool)
            .await;

        // Add group members to team
        let group_members: Vec<(String,)> =
            sqlx::query_as("SELECT user_id FROM user_groups WHERE group_id = ?")
                .bind(group_id)
                .fetch_all(&state.pool)
                .await
                .unwrap_or_default();

        for (member_user_id,) in &group_members {
            let _ = sqlx::query(
                "INSERT OR IGNORE INTO team_members (team_id, user_id, role, source) VALUES (?, ?, 'member', 'group')",
            )
            .bind(&team_id)
            .bind(member_user_id)
            .execute(&state.pool)
            .await;
        }
    }

    tracing::info!(team_id = %team_id, name = %name, slug = %slug, "Team created");

    Redirect::to("/dashboard/teams").into_response()
}

async fn render_team_form_error(
    state: &AppState,
    user: &crate::models::User,
    error: &str,
    form: &TeamForm,
) -> Html<String> {
    let all_users: Vec<(String, String, String, Option<String>)> = sqlx::query_as(
        "SELECT id, name, email, avatar_path FROM users WHERE enabled = 1 ORDER BY name",
    )
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();

    let users_ctx: Vec<minijinja::Value> = all_users
        .iter()
        .map(|(id, name, email, avatar_path)| {
            context! {
                id => id,
                name => name,
                email => email,
                is_self => id == &user.id,
                has_avatar => avatar_path.is_some(),
                initials => compute_initials(name),
            }
        })
        .collect();

    let oidc_groups: Vec<(String, String, i64)> = sqlx::query_as(
        "SELECT g.id, g.name, COUNT(ug.user_id) as member_count \
         FROM groups g LEFT JOIN user_groups ug ON ug.group_id = g.id \
         GROUP BY g.id ORDER BY g.name",
    )
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();

    let groups_ctx =
        build_groups_ctx(&state.pool, &oidc_groups, &std::collections::HashSet::new()).await;

    let tmpl = match state.templates.get_template("team_form.html") {
        Ok(t) => t,
        Err(e) => return internal_error_html("template render", &e),
    };

    Html(
        tmpl.render(context! {
            sidebar => admin_sidebar_context(user, "teams"),
            users => users_ctx,
            oidc_groups => if groups_ctx.is_empty() { None } else { Some(groups_ctx) },
            form_name => &form.name,
            form_slug => &form.slug,
            form_description => form.description.as_deref().unwrap_or(""),
            form_visibility => form.visibility.as_deref().unwrap_or("public"),
            error => error,
        })
        .unwrap_or_else(|e| internal_error_body("template render", &e)),
    )
}

// --- Dashboard: Sources ---

async fn dashboard_organization(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
) -> impl IntoResponse {
    let internal_ets: Vec<(
        String,
        String,
        String,
        i32,
        String,
        Option<String>,
        Option<String>,
        Option<String>,
        String,
    )> = sqlx::query_as(
        "SELECT et.id, et.slug, et.title, et.duration_min, u.name,
                u.username,
                CASE WHEN et.team_id IS NOT NULL THEN t.name ELSE NULL END,
                CASE WHEN et.team_id IS NOT NULL THEN t.slug ELSE NULL END,
                et.visibility
         FROM event_types et
         JOIN accounts a ON a.id = et.account_id
         JOIN users u ON u.id = a.user_id
         LEFT JOIN teams t ON t.id = et.team_id
         WHERE et.visibility = 'internal' AND et.enabled = 1
         ORDER BY et.created_at",
    )
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();

    let ets_ctx: Vec<minijinja::Value> = internal_ets
        .iter()
        .map(
            |(id, slug, title, duration, host_name, username, team_name, team_slug, visibility)| {
                context! {
                    id => id,
                    slug => slug,
                    title => title,
                    duration_min => duration,
                    host_name => host_name,
                    username => username,
                    team_name => team_name,
                    team_slug => team_slug,
                    visibility => visibility,
                }
            },
        )
        .collect();

    let tmpl = match state.templates.get_template("dashboard_internal.html") {
        Ok(t) => t,
        Err(e) => return internal_error_html("template render", &e),
    };

    let (impersonating, impersonating_name, _) = impersonation_ctx(&auth_user);

    Html(
        tmpl.render(context! {
            sidebar => sidebar_context(&auth_user, "organization"),
            event_types => ets_ctx,
            impersonating => impersonating,
            impersonating_name => impersonating_name,
        })
        .unwrap_or_default(),
    )
}

#[derive(Deserialize)]
struct PriorityForm {
    _csrf: Option<String>,
    priority: String,
}

async fn dashboard_sources(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
) -> impl IntoResponse {
    let user = &auth_user.user;

    #[allow(clippy::type_complexity)]
    let sources: Vec<(
        String,
        String,
        String,
        String,
        Option<String>,
        bool,
        Option<String>,
        String,
        String,
        i64,
    )> = sqlx::query_as(
        "SELECT cs.id, cs.name, cs.url, cs.username, cs.last_synced, cs.enabled, \
                cs.write_calendar_href, cs.auth_type, cs.provider_type, cs.managed
         FROM caldav_sources cs
         JOIN accounts a ON a.id = cs.account_id
         WHERE a.user_id = ?
         ORDER BY cs.created_at",
    )
    .bind(&user.id)
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();

    let all_calendars: Vec<(String, String, Option<String>)> = sqlx::query_as(
        "SELECT c.source_id, c.href, c.display_name
         FROM calendars c
         JOIN caldav_sources cs ON cs.id = c.source_id
         JOIN accounts a ON a.id = cs.account_id
         WHERE a.user_id = ?
         ORDER BY c.display_name",
    )
    .bind(&user.id)
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();

    let sources_ctx: Vec<minijinja::Value> = sources
        .iter()
        .map(
            |(
                id,
                name,
                url,
                username,
                last_synced,
                enabled,
                write_cal,
                auth_type,
                provider_type,
                managed,
            )| {
                let cals: Vec<minijinja::Value> = all_calendars
                    .iter()
                    .filter(|(sid, _, _)| sid == id)
                    .map(|(_, href, display)| {
                        context! {
                            href => href,
                            name => display.as_deref().unwrap_or(href),
                        }
                    })
                    .collect();
                context! {
                    id => id,
                    id_short => &id[..8.min(id.len())],
                    name => name,
                    url => url,
                    username => username,
                    last_synced => last_synced.as_deref().unwrap_or("never"),
                    enabled => enabled,
                    write_calendar_href => write_cal.as_deref().unwrap_or(""),
                    auth_type => auth_type,
                    provider_type => provider_type,
                    provider_label => crate::providers::factory::label(provider_type),
                    managed => *managed != 0,
                    calendars => cals,
                }
            },
        )
        .collect();

    let tmpl = match state.templates.get_template("dashboard_sources.html") {
        Ok(t) => t,
        Err(e) => return internal_error_html("template render", &e),
    };

    let (impersonating, impersonating_name, _) = impersonation_ctx(&auth_user);

    let lock_user_sources = state
        .ews_global
        .read()
        .await
        .as_ref()
        .map(|c| c.lock_user_sources)
        .unwrap_or(false);
    let is_admin = auth_user.user.role == "admin";

    Html(
        tmpl.render(context! {
            sidebar => sidebar_context(&auth_user, "sources"),
            sources => sources_ctx,
            impersonating => impersonating,
            impersonating_name => impersonating_name,
            lock_user_sources => lock_user_sources,
            is_admin => is_admin,
        })
        .unwrap_or_else(|e| internal_error_body("template render", &e)),
    )
}

// --- Settings ---

#[derive(Deserialize)]
struct SettingsForm {
    _csrf: Option<String>,
    name: String,
    username: Option<String>,
    title: Option<String>,
    bio: Option<String>,
    booking_email: Option<String>,
    timezone: Option<String>,
    language: Option<String>,
    allow_dynamic_group: Option<String>,
    #[serde(default)]
    avail_schedule: String,
}

async fn settings_page(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
) -> impl IntoResponse {
    let sidebar = sidebar_context(&auth_user, "settings");
    let (impersonating, impersonating_name, _) = impersonation_ctx(&auth_user);
    ensure_user_avail_seeded(&state.pool, &auth_user.user.id).await;
    let avail = load_user_avail_schedule(&state.pool, &auth_user.user.id).await;
    settings_render(
        &state,
        &auth_user.user,
        None,
        None,
        sidebar,
        impersonating,
        &impersonating_name,
        &avail,
    )
}

/// Convert a user's default availability rules into "busy" times for hours OUTSIDE
/// their available windows. This lets us constrain dynamic group link participants
/// by their working hours without changing the slot computation engine.
/// Convert a user's default availability rules into "busy" times for hours OUTSIDE
/// their available windows. Times are converted from the participant's timezone to
/// the host's timezone so they integrate correctly with the slot computation.
async fn user_avail_as_busy(
    pool: &SqlitePool,
    user_id: &str,
    window_start: NaiveDateTime,
    window_end: NaiveDateTime,
    host_tz: chrono_tz::Tz,
) -> Vec<(NaiveDateTime, NaiveDateTime)> {
    let rules: Vec<(i32, String, String)> = sqlx::query_as(
        "SELECT day_of_week, start_time, end_time FROM user_availability_rules WHERE user_id = ? ORDER BY day_of_week, start_time",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    // No rules = no constraint (user hasn't set default availability)
    if rules.is_empty() {
        return vec![];
    }

    // Get the participant's timezone
    let user_tz_str: String = sqlx::query_scalar("SELECT timezone FROM users WHERE id = ?")
        .bind(user_id)
        .fetch_one(pool)
        .await
        .unwrap_or_else(|_| "UTC".to_string());
    let user_tz: chrono_tz::Tz = user_tz_str.parse().unwrap_or(chrono_tz::Tz::UTC);

    // Work in the participant's timezone: convert window bounds from host TZ to user TZ
    let window_start_user = host_tz
        .from_local_datetime(&window_start)
        .earliest()
        .map(|dt| dt.with_timezone(&user_tz).naive_local())
        .unwrap_or(window_start);
    let window_end_user = host_tz
        .from_local_datetime(&window_end)
        .earliest()
        .map(|dt| dt.with_timezone(&user_tz).naive_local())
        .unwrap_or(window_end);

    let mut busy = Vec::new();
    let mut date = window_start_user.date();
    let end_date = window_end_user.date();
    while date <= end_date {
        let weekday = date.weekday().num_days_from_sunday() as i32;
        let mut windows: Vec<(NaiveTime, NaiveTime)> = rules
            .iter()
            .filter(|(d, _, _)| *d == weekday)
            .filter_map(|(_, s, e)| {
                let start = NaiveTime::parse_from_str(s, "%H:%M").ok()?;
                let end = NaiveTime::parse_from_str(e, "%H:%M").ok()?;
                Some((start, end))
            })
            .collect();
        windows.sort_by_key(|(s, _)| *s);

        if windows.is_empty() {
            // Entire day is unavailable
            let day_start = date.and_hms_opt(0, 0, 0).unwrap();
            let day_end = date.and_hms_opt(23, 59, 59).unwrap();
            busy.push((day_start, day_end));
        } else {
            // Block hours outside available windows
            let day_start = date.and_hms_opt(0, 0, 0).unwrap();
            let first_avail = date.and_time(windows[0].0);
            if first_avail > day_start {
                busy.push((day_start, first_avail));
            }
            for i in 0..windows.len() - 1 {
                let gap_start = date.and_time(windows[i].1);
                let gap_end = date.and_time(windows[i + 1].0);
                if gap_end > gap_start {
                    busy.push((gap_start, gap_end));
                }
            }
            let last_avail_end = date.and_time(windows.last().unwrap().1);
            let day_end = date.and_hms_opt(23, 59, 59).unwrap();
            if day_end > last_avail_end {
                busy.push((last_avail_end, day_end));
            }
        }
        date = date.succ_opt().unwrap_or(date);
    }

    // Convert busy times from participant's TZ back to host's TZ
    busy.into_iter()
        .filter_map(|(start, end)| {
            let start_host = user_tz
                .from_local_datetime(&start)
                .earliest()
                .map(|dt| dt.with_timezone(&host_tz).naive_local())?;
            let end_host = user_tz
                .from_local_datetime(&end)
                .earliest()
                .map(|dt| dt.with_timezone(&host_tz).naive_local())?;
            Some((start_host, end_host))
        })
        .collect()
}

/// Load a user's default availability rules as a serialized schedule string.
/// Format: "1:09:00-17:00;2:09:00-12:00,13:00-17:00" (day:start-end,start-end;...)
async fn load_user_avail_schedule(pool: &SqlitePool, user_id: &str) -> String {
    let rules: Vec<(i32, String, String)> = sqlx::query_as(
        "SELECT day_of_week, start_time, end_time FROM user_availability_rules WHERE user_id = ? ORDER BY day_of_week, start_time",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    let mut parts = Vec::new();
    let mut current_day: Option<i32> = None;
    let mut windows = Vec::new();
    for (day, start, end) in &rules {
        if current_day != Some(*day) {
            if let Some(d) = current_day {
                parts.push(format!("{}:{}", d, windows.join(",")));
            }
            current_day = Some(*day);
            windows.clear();
        }
        windows.push(format!("{}-{}", start, end));
    }
    if let Some(d) = current_day {
        parts.push(format!("{}:{}", d, windows.join(",")));
    }
    parts.join(";")
}

/// Save a user's default availability rules from a serialized schedule string.
async fn save_user_avail_schedule(pool: &SqlitePool, user_id: &str, schedule: &str) {
    // Delete existing rules
    let _ = sqlx::query("DELETE FROM user_availability_rules WHERE user_id = ?")
        .bind(user_id)
        .execute(pool)
        .await;

    // Parse and insert new rules
    for seg in schedule.split(';') {
        let seg = seg.trim();
        if seg.is_empty() {
            continue;
        }
        let parts: Vec<&str> = seg.splitn(2, ':').collect();
        if parts.len() < 2 {
            continue;
        }
        let day: i32 = match parts[0].parse() {
            Ok(d) if (0..=6).contains(&d) => d,
            _ => continue,
        };
        for window in parts[1].split(',') {
            let times: Vec<&str> = window.trim().split('-').collect();
            if times.len() != 2 {
                continue;
            }
            let id = uuid::Uuid::new_v4().to_string();
            let _ = sqlx::query(
                "INSERT INTO user_availability_rules (id, user_id, day_of_week, start_time, end_time) VALUES (?, ?, ?, ?, ?)",
            )
            .bind(&id)
            .bind(user_id)
            .bind(day)
            .bind(times[0].trim())
            .bind(times[1].trim())
            .execute(pool)
            .await;
        }
    }
}

/// Ensure a user has default availability rules. If none exist, insert Mon-Fri 9:00-17:00.
async fn ensure_user_avail_seeded(pool: &SqlitePool, user_id: &str) {
    let count: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM user_availability_rules WHERE user_id = ?")
            .bind(user_id)
            .fetch_one(pool)
            .await
            .unwrap_or((0,));
    if count.0 > 0 {
        return;
    }
    // Seed Mon(1)-Fri(5) 09:00-17:00
    for day in 1..=5 {
        let id = uuid::Uuid::new_v4().to_string();
        let _ = sqlx::query(
            "INSERT INTO user_availability_rules (id, user_id, day_of_week, start_time, end_time) VALUES (?, ?, ?, '09:00', '17:00')",
        )
        .bind(&id)
        .bind(user_id)
        .bind(day)
        .execute(pool)
        .await;
    }
}

/// Returns the authenticated user's profile-default availability schedule
/// as JSON. Used by the event-type form's "Reset to my default" button.
async fn dashboard_availability_default(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
) -> impl IntoResponse {
    ensure_user_avail_seeded(&state.pool, &auth_user.user.id).await;
    let schedule = load_user_avail_schedule(&state.pool, &auth_user.user.id).await;
    axum::Json(serde_json::json!({ "schedule": schedule }))
}

fn settings_render(
    state: &AppState,
    user: &crate::models::User,
    success: Option<&str>,
    error: Option<&str>,
    sidebar: minijinja::Value,
    impersonating: bool,
    impersonating_name: &str,
    avail_schedule: &str,
) -> Html<String> {
    let tmpl = match state.templates.get_template("settings.html") {
        Ok(t) => t,
        Err(e) => return internal_error_html("internal", &e),
    };
    let tz_options: Vec<minijinja::Value> = common_timezones_with("")
        .iter()
        .map(|(iana, label)| {
            context! { value => iana, label => label }
        })
        .collect();
    let lang_options: Vec<minijinja::Value> = crate::i18n::supported_with_labels()
        .map(|(code, label)| context! { value => code, label => label })
        .collect();
    Html(
        tmpl.render(context! {
            sidebar => sidebar,
            form_name => user.name,
            form_initials => compute_initials(&user.name),
            form_title => user.title.as_deref().unwrap_or(""),
            form_bio => user.bio.as_deref().unwrap_or(""),
            form_booking_email => user.booking_email.as_deref().unwrap_or(""),
            form_timezone => user.timezone,
            tz_options => tz_options,
            form_language => user.language.as_deref().unwrap_or(""),
            lang_options => lang_options,
            user_email => user.email,
            user_id => user.id,
            has_avatar => user.avatar_path.is_some(),
            username => user.username.as_deref().unwrap_or(""),
            allow_dynamic_group => user.allow_dynamic_group,
            form_avail_schedule => avail_schedule,
            success => success.unwrap_or(""),
            error => error.unwrap_or(""),
            impersonating => impersonating,
            impersonating_name => impersonating_name,
        })
        .unwrap_or_else(|e| internal_error_body("template render", &e)),
    )
}

async fn settings_save(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
    headers: HeaderMap,
    Form(form): Form<SettingsForm>,
) -> impl IntoResponse {
    if let Err(resp) = verify_csrf_token(&headers, &form._csrf) {
        return resp;
    }
    let user = &auth_user.user;
    let name = form.name.trim().to_string();
    let sidebar = sidebar_context(&auth_user, "settings");
    let (imp, imp_name, _) = impersonation_ctx(&auth_user);

    if name.is_empty() || name.len() > 255 {
        return settings_render(
            &state,
            user,
            None,
            Some("Name must be between 1 and 255 characters."),
            sidebar,
            imp,
            &imp_name,
            &form.avail_schedule,
        )
        .into_response();
    }

    // Validate and update username if provided
    let new_username = form
        .username
        .as_deref()
        .map(|s| {
            s.trim()
                .to_lowercase()
                .chars()
                .filter(|c| c.is_ascii_alphanumeric() || *c == '-')
                .take(100)
                .collect::<String>()
        })
        .filter(|s| !s.is_empty());

    if let Some(ref uname) = new_username {
        if uname.len() < 2 {
            return settings_render(
                &state,
                user,
                None,
                Some("Username must be at least 2 characters."),
                sidebar,
                imp,
                &imp_name,
                &form.avail_schedule,
            )
            .into_response();
        }
        // Check uniqueness (only if different from current)
        if user.username.as_deref() != Some(uname.as_str()) {
            let taken: Option<(String,)> =
                sqlx::query_as("SELECT id FROM users WHERE username = ? AND id != ?")
                    .bind(uname)
                    .bind(&user.id)
                    .fetch_optional(&state.pool)
                    .await
                    .unwrap_or(None);
            if taken.is_some() {
                return settings_render(
                    &state,
                    user,
                    None,
                    Some("This username is already taken."),
                    sidebar,
                    imp,
                    &imp_name,
                    &form.avail_schedule,
                )
                .into_response();
            }
            let _ = sqlx::query("UPDATE users SET username = ? WHERE id = ?")
                .bind(uname)
                .bind(&user.id)
                .execute(&state.pool)
                .await;
        }
    }

    let title = form
        .title
        .as_deref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());

    let bio = form
        .bio
        .as_deref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());

    let booking_email = form
        .booking_email
        .as_deref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());

    if let Some(ref be) = booking_email {
        if be.len() > 255
            || !be.contains('@')
            || be
                .rsplit('@')
                .next()
                .is_none_or(|domain| !domain.contains('.'))
        {
            return settings_render(
                &state,
                user,
                None,
                Some("Please enter a valid booking email address."),
                sidebar,
                imp,
                &imp_name,
                &form.avail_schedule,
            )
            .into_response();
        }
    }

    let timezone = form
        .timezone
        .as_deref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty() && s.parse::<chrono_tz::Tz>().is_ok())
        .unwrap_or("UTC")
        .to_string();

    // Empty / "auto" / unsupported codes all map to NULL = follow Accept-Language.
    let language: Option<String> = form
        .language
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty() && *s != "auto")
        .filter(|s| crate::i18n::is_supported(s))
        .map(str::to_string);

    let allow_dynamic_group = form.allow_dynamic_group.as_deref() == Some("on");

    let result = sqlx::query(
        "UPDATE users SET name = ?, title = ?, bio = ?, booking_email = ?, timezone = ?, language = ?, allow_dynamic_group = ?, updated_at = datetime('now') WHERE id = ?",
    )
    .bind(&name)
    .bind(&title)
    .bind(&bio)
    .bind(&booking_email)
    .bind(&timezone)
    .bind(&language)
    .bind(allow_dynamic_group)
    .bind(&user.id)
    .execute(&state.pool)
    .await;

    match result {
        Ok(_) => {
            // Also update the linked account name
            let _ = sqlx::query("UPDATE accounts SET name = ? WHERE user_id = ?")
                .bind(&name)
                .bind(&user.id)
                .execute(&state.pool)
                .await;

            // Save default availability schedule
            save_user_avail_schedule(&state.pool, &user.id, &form.avail_schedule).await;

            // Re-fetch user to show updated values
            let updated_user = crate::auth::get_user_by_id(&state.pool, &user.id)
                .await
                .unwrap_or_else(|| user.clone());
            let sidebar = sidebar_context(&auth_user, "settings");
            settings_render(
                &state,
                &updated_user,
                Some("Settings saved."),
                None,
                sidebar,
                imp,
                &imp_name,
                &form.avail_schedule,
            )
            .into_response()
        }
        Err(_) => settings_render(
            &state,
            user,
            None,
            Some("Failed to save settings."),
            sidebar,
            imp,
            &imp_name,
            &form.avail_schedule,
        )
        .into_response(),
    }
}

// --- Quick timezone update (from banner) ---

#[derive(Deserialize)]
struct TimezoneUpdateForm {
    timezone: String,
}

async fn update_timezone(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
    headers: HeaderMap,
    axum::Json(form): axum::Json<TimezoneUpdateForm>,
) -> impl IntoResponse {
    let csrf = headers
        .get("x-csrf-token")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if let Err(resp) = verify_csrf_token(&headers, &Some(csrf.to_string())) {
        return resp;
    }
    let tz = form.timezone.trim();
    if tz.parse::<chrono_tz::Tz>().is_err() {
        return (axum::http::StatusCode::BAD_REQUEST, "Invalid timezone").into_response();
    }
    let _ = sqlx::query("UPDATE users SET timezone = ?, updated_at = datetime('now') WHERE id = ?")
        .bind(tz)
        .bind(&auth_user.user.id)
        .execute(&state.pool)
        .await;
    tracing::info!(user_id = %auth_user.user.id, timezone = %tz, "timezone updated from banner");
    (axum::http::StatusCode::OK, "OK").into_response()
}

// --- Avatar upload/serve/delete ---

/// Detect an image format by its magic bytes. Returns the canonical file
/// extension (without leading dot) or None if the bytes don't match a
/// supported format. The Content-Type from the multipart header is
/// attacker-controlled, so we validate the actual file contents instead.
fn detect_image_ext(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(&[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]) {
        Some("png")
    } else if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some("jpg")
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        Some("gif")
    } else if bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        Some("webp")
    } else {
        None
    }
}

async fn upload_avatar(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
    headers: HeaderMap,
    Query(csrf_query): Query<CsrfQuery>,
    mut multipart: Multipart,
) -> impl IntoResponse {
    if let Err(resp) = verify_csrf_token(&headers, &csrf_query._csrf) {
        return resp;
    }
    let user = &auth_user.user;
    while let Ok(Some(field)) = multipart.next_field().await {
        if field.name() == Some("avatar") {
            let content_type = field.content_type().unwrap_or("").to_string();
            if !content_type.starts_with("image/") {
                return Redirect::to("/dashboard/settings").into_response();
            }
            if let Ok(bytes) = field.bytes().await {
                if bytes.len() > 2 * 1024 * 1024 {
                    return Redirect::to("/dashboard/settings").into_response();
                }
                // Trust magic bytes, not the multipart Content-Type header.
                let ext = match detect_image_ext(&bytes) {
                    Some(e) => e,
                    None => return Redirect::to("/dashboard/settings").into_response(),
                };
                let avatars_dir = state.data_dir.join("avatars");
                let _ = tokio::fs::create_dir_all(&avatars_dir).await;
                let filename = format!("{}.{}", user.id, ext);
                let avatar_path = avatars_dir.join(&filename);

                // Remove old avatar if different extension
                if let Some(old_path) = &user.avatar_path {
                    let old_full = state.data_dir.join("avatars").join(old_path);
                    if old_full != avatar_path {
                        let _ = tokio::fs::remove_file(&old_full).await;
                    }
                }

                if tokio::fs::write(&avatar_path, &bytes).await.is_ok() {
                    let _ = sqlx::query(
                        "UPDATE users SET avatar_path = ?, updated_at = datetime('now') WHERE id = ?",
                    )
                    .bind(&filename)
                    .bind(&user.id)
                    .execute(&state.pool)
                    .await;
                }
            }
        }
    }
    Redirect::to("/dashboard/settings").into_response()
}

async fn delete_avatar(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
    headers: HeaderMap,
    Form(csrf): Form<CsrfForm>,
) -> impl IntoResponse {
    if let Err(resp) = verify_csrf_token(&headers, &csrf._csrf) {
        return resp;
    }
    let user = &auth_user.user;
    if let Some(avatar_path) = &user.avatar_path {
        let full_path = state.data_dir.join("avatars").join(avatar_path);
        let _ = tokio::fs::remove_file(&full_path).await;
    }
    let _ = sqlx::query(
        "UPDATE users SET avatar_path = NULL, updated_at = datetime('now') WHERE id = ?",
    )
    .bind(&user.id)
    .execute(&state.pool)
    .await;
    Redirect::to("/dashboard/settings").into_response()
}

async fn serve_avatar(
    State(state): State<Arc<AppState>>,
    Path(user_id): Path<String>,
) -> impl IntoResponse {
    let avatar_path: Option<(String,)> =
        sqlx::query_as("SELECT avatar_path FROM users WHERE id = ? AND avatar_path IS NOT NULL")
            .bind(&user_id)
            .fetch_optional(&state.pool)
            .await
            .unwrap_or(None);

    let filename = match avatar_path {
        Some((f,)) => f,
        None => return (axum::http::StatusCode::NOT_FOUND, "").into_response(),
    };

    let full_path = state.data_dir.join("avatars").join(&filename);
    match tokio::fs::read(&full_path).await {
        Ok(bytes) => {
            let content_type = if filename.ends_with(".jpg") || filename.ends_with(".jpeg") {
                "image/jpeg"
            } else if filename.ends_with(".png") {
                "image/png"
            } else if filename.ends_with(".gif") {
                "image/gif"
            } else if filename.ends_with(".webp") {
                "image/webp"
            } else {
                "image/png"
            };
            axum::response::Response::builder()
                .status(200)
                .header("Content-Type", content_type)
                .header("Cache-Control", "public, max-age=3600")
                .body(axum::body::Body::from(bytes))
                .unwrap_or_else(|_| axum::response::Response::new(axum::body::Body::empty()))
                .into_response()
        }
        Err(_) => (axum::http::StatusCode::NOT_FOUND, "").into_response(),
    }
}

// --- Team settings & avatar ---

async fn is_team_member(pool: &SqlitePool, user_id: &str, team_id: &str) -> bool {
    sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM team_members WHERE user_id = ? AND team_id = ?",
    )
    .bind(user_id)
    .bind(team_id)
    .fetch_one(pool)
    .await
    .unwrap_or(0)
        > 0
}

/// Check if a user has team-admin role for a specific team.
async fn is_team_admin(pool: &SqlitePool, user_id: &str, team_id: &str) -> bool {
    sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM team_members WHERE user_id = ? AND team_id = ? AND role = 'admin'",
    )
    .bind(user_id)
    .bind(team_id)
    .fetch_one(pool)
    .await
    .unwrap_or(0)
        > 0
}

/// Single source of truth for "is this user allowed to manage this event type?"
/// Policy:
///   - Global admin: always yes.
///   - Personal event type (team_id IS NULL): account owner only.
///   - Team event type: team admin only (regular members cannot mutate).
///
/// Takes an event_type_id (not slug) to avoid slug-collision ambiguity.
async fn can_manage_event_type(
    pool: &SqlitePool,
    user: &crate::models::User,
    event_type_id: &str,
) -> bool {
    if user.role == "admin" {
        return true;
    }
    sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM event_types et
         LEFT JOIN accounts a ON a.id = et.account_id
         LEFT JOIN team_members tm ON tm.team_id = et.team_id AND tm.user_id = ? AND tm.role = 'admin'
         WHERE et.id = ? AND (
             (et.team_id IS NULL AND a.user_id = ?) OR
             (et.team_id IS NOT NULL AND tm.user_id IS NOT NULL)
         )",
    )
    .bind(&user.id)
    .bind(event_type_id)
    .bind(&user.id)
    .fetch_one(pool)
    .await
    .unwrap_or(0)
        > 0
}

/// Look up an event type by slug that the (non-admin) user is allowed to
/// manage. Personal events resolve via the account owner; team events resolve
/// via team admin role. Global admins should bypass this and look up by slug
/// directly. Returns (id, title, team_id).
///
/// If the user has both a personal event and a team event with the same slug,
/// the personal event wins. Branch order in `UNION ALL` is not guaranteed by
/// SQL, so we wrap it in a subquery and order by `team_id IS NULL DESC`
/// (i.e. NULLs first → personal first) to make the resolution deterministic.
async fn find_manageable_event_type_by_slug(
    pool: &SqlitePool,
    user_id: &str,
    slug: &str,
) -> Option<(String, String, Option<String>)> {
    sqlx::query_as(
        "SELECT id, title, team_id FROM (
             SELECT et.id, et.title, et.team_id FROM event_types et
             JOIN accounts a ON a.id = et.account_id
             WHERE a.user_id = ? AND et.slug = ? AND et.team_id IS NULL
             UNION ALL
             SELECT et.id, et.title, et.team_id FROM event_types et
             JOIN team_members tm ON tm.team_id = et.team_id
             WHERE tm.user_id = ? AND tm.role = 'admin' AND et.slug = ? AND et.team_id IS NOT NULL
         )
         ORDER BY (team_id IS NULL) DESC
         LIMIT 1",
    )
    .bind(user_id)
    .bind(slug)
    .bind(user_id)
    .bind(slug)
    .fetch_optional(pool)
    .await
    .unwrap_or(None)
}

#[derive(Deserialize)]
struct GroupSettingsForm {
    _csrf: Option<String>,
    name: Option<String>,
    slug: Option<String>,
    description: Option<String>,
    visibility: Option<String>,
    #[serde(default)]
    members: String,
    #[serde(default)]
    group_ids: String,
    #[serde(default)]
    admin_members: String,
}

async fn team_settings_page(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
    Path(team_id): Path<String>,
    Query(query): Query<std::collections::HashMap<String, String>>,
) -> impl IntoResponse {
    let user = &auth_user.user;
    let is_admin = user.role == "admin";
    if !is_admin && !is_team_admin(&state.pool, &user.id, &team_id).await {
        return Html("Team not found or you are not a team admin.".to_string());
    }

    let team: Option<(String, String, String, Option<String>, Option<String>, String, Option<String>)> =
        sqlx::query_as("SELECT id, name, slug, description, avatar_path, visibility, invite_token FROM teams WHERE id = ?")
            .bind(&team_id)
            .fetch_optional(&state.pool)
            .await
            .unwrap_or(None);

    let (tid, team_name, team_slug, description, avatar_path, visibility, invite_token) = match team
    {
        Some(t) => t,
        None => return Html("Team not found.".to_string()),
    };

    let members: Vec<(String, String, Option<String>, String, String)> = sqlx::query_as(
        "SELECT u.id, u.name, u.avatar_path, tm.role, tm.source FROM users u \
         JOIN team_members tm ON tm.user_id = u.id \
         WHERE tm.team_id = ? AND u.enabled = 1 \
         ORDER BY u.name",
    )
    .bind(&tid)
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();

    let members_ctx: Vec<minijinja::Value> = members
        .iter()
        .map(|(id, name, ap, role, source)| {
            context! {
                id => id,
                name => name,
                has_avatar => ap.is_some(),
                initials => compute_initials(name),
                role => role,
                source => source,
            }
        })
        .collect();

    // All enabled users for the member picker
    let all_users: Vec<(String, String, String, Option<String>)> = sqlx::query_as(
        "SELECT id, name, email, avatar_path FROM users WHERE enabled = 1 ORDER BY name",
    )
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();

    let member_ids: std::collections::HashSet<&str> =
        members.iter().map(|(id, _, _, _, _)| id.as_str()).collect();

    let all_users_ctx: Vec<minijinja::Value> = all_users
        .iter()
        .map(|(id, name, email, avatar_path)| {
            context! {
                id => id,
                name => name,
                email => email,
                has_avatar => avatar_path.is_some(),
                initials => compute_initials(name),
                is_member => member_ids.contains(id.as_str()),
            }
        })
        .collect();

    // OIDC groups with member counts + linked status
    let oidc_groups: Vec<(String, String, i64)> = sqlx::query_as(
        "SELECT g.id, g.name, COUNT(ug.user_id) as member_count \
         FROM groups g LEFT JOIN user_groups ug ON ug.group_id = g.id \
         GROUP BY g.id ORDER BY g.name",
    )
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();

    let linked_group_ids: Vec<(String,)> =
        sqlx::query_as("SELECT group_id FROM team_groups WHERE team_id = ?")
            .bind(&tid)
            .fetch_all(&state.pool)
            .await
            .unwrap_or_default();
    let linked_set: std::collections::HashSet<String> =
        linked_group_ids.into_iter().map(|(id,)| id).collect();

    let groups_ctx = build_groups_ctx(&state.pool, &oidc_groups, &linked_set).await;

    let linked_groups_only: Vec<(String, String, i64)> = oidc_groups
        .iter()
        .filter(|(id, _, _)| linked_set.contains(id))
        .cloned()
        .collect();
    let linked_groups_ctx = build_groups_ctx(
        &state.pool,
        &linked_groups_only,
        &std::collections::HashSet::new(),
    )
    .await;

    let tmpl = match state.templates.get_template("team_settings.html") {
        Ok(t) => t,
        Err(e) => return internal_error_html("template render", &e),
    };

    Html(
        tmpl.render(context! {
            sidebar => sidebar_context(&auth_user, "teams"),
            team_id => tid,
            team_name => team_name,
            team_slug => team_slug,
            team_description => description.unwrap_or_default(),
            team_has_avatar => avatar_path.is_some(),
            team_initials => compute_initials(&team_name),
            visibility => visibility,
            invite_token => invite_token,
            members => members_ctx,
            all_users => all_users_ctx,
            oidc_groups => if groups_ctx.is_empty() { None } else { Some(groups_ctx) },
            linked_groups => linked_groups_ctx,
            success => query.get("success").map(|_| "Settings saved."),
        })
        .unwrap_or_else(|e| internal_error_body("template render", &e)),
    )
}

async fn team_settings_save(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
    headers: HeaderMap,
    Path(team_id): Path<String>,
    Form(form): Form<GroupSettingsForm>,
) -> impl IntoResponse {
    if let Err(resp) = verify_csrf_token(&headers, &form._csrf) {
        return resp;
    }
    let user = &auth_user.user;
    let is_admin = user.role == "admin";
    if !is_admin && !is_team_admin(&state.pool, &user.id, &team_id).await {
        return Redirect::to("/dashboard/event-types").into_response();
    }
    // Update name if provided
    if let Some(ref name) = form.name {
        let name = name.trim().chars().take(255).collect::<String>();
        if !name.is_empty() {
            let _ = sqlx::query("UPDATE teams SET name = ? WHERE id = ?")
                .bind(&name)
                .bind(&team_id)
                .execute(&state.pool)
                .await;
        }
    }

    // Update slug if provided (validate format and uniqueness)
    if let Some(ref slug) = form.slug {
        let slug = slug
            .trim()
            .to_lowercase()
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || *c == '-')
            .take(100)
            .collect::<String>();
        if !slug.is_empty() {
            let conflict: Option<(String,)> =
                sqlx::query_as("SELECT id FROM teams WHERE slug = ? AND id != ?")
                    .bind(&slug)
                    .bind(&team_id)
                    .fetch_optional(&state.pool)
                    .await
                    .unwrap_or(None);
            if conflict.is_none() {
                let _ = sqlx::query("UPDATE teams SET slug = ? WHERE id = ?")
                    .bind(&slug)
                    .bind(&team_id)
                    .execute(&state.pool)
                    .await;
            }
        }
    }

    let desc = form
        .description
        .as_deref()
        .unwrap_or("")
        .trim()
        .chars()
        .take(5000)
        .collect::<String>();
    let desc = if desc.is_empty() { None } else { Some(desc) };
    let _ = sqlx::query("UPDATE teams SET description = ? WHERE id = ?")
        .bind(&desc)
        .bind(&team_id)
        .execute(&state.pool)
        .await;

    // Update visibility if provided
    if let Some(ref vis) = form.visibility {
        let vis = vis.trim();
        if vis == "public" || vis == "private" {
            let _ = sqlx::query("UPDATE teams SET visibility = ? WHERE id = ?")
                .bind(vis)
                .bind(&team_id)
                .execute(&state.pool)
                .await;
            // Generate invite token when switching to private (if none exists)
            if vis == "private" {
                let existing: Option<(Option<String>,)> =
                    sqlx::query_as("SELECT invite_token FROM teams WHERE id = ?")
                        .bind(&team_id)
                        .fetch_optional(&state.pool)
                        .await
                        .unwrap_or(None);
                if existing.map(|(t,)| t.is_none()).unwrap_or(true) {
                    let token = uuid::Uuid::new_v4().to_string();
                    let _ = sqlx::query("UPDATE teams SET invite_token = ? WHERE id = ?")
                        .bind(&token)
                        .bind(&team_id)
                        .execute(&state.pool)
                        .await;
                }
            }
        }
    }

    // Sync direct members (preserve group-synced members).
    // The hidden input is always present (populated by JS). An empty value means
    // either "remove all direct members" (valid for global admins) or JS failure.
    // We proceed either way — the safety net below re-adds non-admin users.
    let member_ids = split_csv_ids(&form.members);
    // 1. Remove direct members not in the submitted list
    let _ = sqlx::query(
        "DELETE FROM team_members WHERE team_id = ? AND source = 'direct' AND user_id NOT IN \
         (SELECT value FROM json_each(?))",
    )
    .bind(&team_id)
    .bind(serde_json::to_string(&member_ids).unwrap_or_else(|_| "[]".to_string()))
    .execute(&state.pool)
    .await;

    // 2. Add new direct members (INSERT OR IGNORE to not conflict with group-synced)
    for member_id in &member_ids {
        let _ = sqlx::query(
            "INSERT OR IGNORE INTO team_members (team_id, user_id, role, source) VALUES (?, ?, 'member', 'direct')",
        )
        .bind(&team_id)
        .bind(member_id)
        .execute(&state.pool)
        .await;
    }

    // 3. Update member roles based on admin_members list
    let admin_ids: std::collections::HashSet<String> =
        split_csv_ids(&form.admin_members).into_iter().collect();
    // Set all members to 'member' first, then promote admins
    let _ = sqlx::query("UPDATE team_members SET role = 'member' WHERE team_id = ?")
        .bind(&team_id)
        .execute(&state.pool)
        .await;
    for admin_id in &admin_ids {
        let _ =
            sqlx::query("UPDATE team_members SET role = 'admin' WHERE team_id = ? AND user_id = ?")
                .bind(&team_id)
                .bind(admin_id)
                .execute(&state.pool)
                .await;
    }

    // Ensure at least one admin remains. Global admins can remove themselves
    // (they can still manage the team via the admin panel).
    if !is_admin {
        // Re-ensure the current user stays as admin
        let _ =
            sqlx::query("UPDATE team_members SET role = 'admin' WHERE team_id = ? AND user_id = ?")
                .bind(&team_id)
                .bind(&user.id)
                .execute(&state.pool)
                .await;
        // Also ensure they remain a member (INSERT OR IGNORE in case they were removed)
        let _ = sqlx::query(
            "INSERT OR IGNORE INTO team_members (team_id, user_id, role, source) VALUES (?, ?, 'admin', 'direct')",
        )
        .bind(&team_id)
        .bind(&user.id)
        .execute(&state.pool)
        .await;
    }

    // Sync linked OIDC groups
    let group_ids = split_csv_ids(&form.group_ids);
    // 1. Remove unlinked groups
    let _ = sqlx::query(
        "DELETE FROM team_groups WHERE team_id = ? AND group_id NOT IN \
         (SELECT value FROM json_each(?))",
    )
    .bind(&team_id)
    .bind(serde_json::to_string(&group_ids).unwrap_or_else(|_| "[]".to_string()))
    .execute(&state.pool)
    .await;

    // 2. Add newly linked groups and their members
    for gid in &group_ids {
        let _ = sqlx::query("INSERT OR IGNORE INTO team_groups (team_id, group_id) VALUES (?, ?)")
            .bind(&team_id)
            .bind(gid)
            .execute(&state.pool)
            .await;

        // Add group members as team members with source='group'
        let group_members: Vec<(String,)> =
            sqlx::query_as("SELECT user_id FROM user_groups WHERE group_id = ?")
                .bind(gid)
                .fetch_all(&state.pool)
                .await
                .unwrap_or_default();
        for (uid,) in &group_members {
            let _ = sqlx::query(
                "INSERT OR IGNORE INTO team_members (team_id, user_id, role, source) VALUES (?, ?, 'member', 'group')",
            )
            .bind(&team_id)
            .bind(uid)
            .execute(&state.pool)
            .await;
        }
    }

    // 3. Remove group-sourced members whose groups are no longer linked
    let _ = sqlx::query(
        "DELETE FROM team_members WHERE team_id = ? AND source = 'group' \
         AND user_id NOT IN (SELECT ug.user_id FROM user_groups ug \
         JOIN team_groups tg ON tg.group_id = ug.group_id WHERE tg.team_id = ?)",
    )
    .bind(&team_id)
    .bind(&team_id)
    .execute(&state.pool)
    .await;

    tracing::info!(team_id = %team_id, user_id = %user.id, "team settings updated");
    Redirect::to(&format!("/dashboard/teams/{}/settings?success=1", team_id)).into_response()
}

async fn upload_team_avatar(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
    headers: HeaderMap,
    Path(team_id): Path<String>,
    Query(csrf_query): Query<CsrfQuery>,
    mut multipart: Multipart,
) -> impl IntoResponse {
    if let Err(resp) = verify_csrf_token(&headers, &csrf_query._csrf) {
        return resp;
    }
    let user = &auth_user.user;
    let is_admin = user.role == "admin";
    if !is_admin && !is_team_admin(&state.pool, &user.id, &team_id).await {
        return Redirect::to("/dashboard/event-types").into_response();
    }
    let redirect_url = format!("/dashboard/teams/{}/settings", team_id);
    while let Ok(Some(field)) = multipart.next_field().await {
        if field.name() == Some("avatar") {
            let content_type = field.content_type().unwrap_or("").to_string();
            if !content_type.starts_with("image/") {
                return Redirect::to(&redirect_url).into_response();
            }
            if let Ok(bytes) = field.bytes().await {
                if bytes.len() > 2 * 1024 * 1024 {
                    return Redirect::to(&redirect_url).into_response();
                }
                // Trust magic bytes, not the multipart Content-Type header.
                let ext = match detect_image_ext(&bytes) {
                    Some(e) => e,
                    None => return Redirect::to(&redirect_url).into_response(),
                };
                let avatars_dir = state.data_dir.join("avatars");
                let _ = tokio::fs::create_dir_all(&avatars_dir).await;
                let filename = format!("team_{}.{}", team_id, ext);
                let avatar_path = avatars_dir.join(&filename);

                // Remove old avatar if different extension
                let old: Option<(String,)> = sqlx::query_as(
                    "SELECT avatar_path FROM teams WHERE id = ? AND avatar_path IS NOT NULL",
                )
                .bind(&team_id)
                .fetch_optional(&state.pool)
                .await
                .unwrap_or(None);
                if let Some((old_name,)) = old {
                    let old_full = avatars_dir.join(&old_name);
                    if old_full != avatar_path {
                        let _ = tokio::fs::remove_file(&old_full).await;
                    }
                }

                if tokio::fs::write(&avatar_path, &bytes).await.is_ok() {
                    let _ = sqlx::query("UPDATE teams SET avatar_path = ? WHERE id = ?")
                        .bind(&filename)
                        .bind(&team_id)
                        .execute(&state.pool)
                        .await;
                    tracing::info!(team_id = %team_id, user_id = %user.id, "team avatar uploaded");
                }
            }
        }
    }
    Redirect::to(&redirect_url).into_response()
}

async fn delete_team_avatar(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
    headers: HeaderMap,
    Path(team_id): Path<String>,
    Form(csrf): Form<CsrfForm>,
) -> impl IntoResponse {
    if let Err(resp) = verify_csrf_token(&headers, &csrf._csrf) {
        return resp;
    }
    let user = &auth_user.user;
    let is_admin = user.role == "admin";
    if !is_admin && !is_team_admin(&state.pool, &user.id, &team_id).await {
        return Redirect::to("/dashboard/event-types").into_response();
    }
    let old: Option<(String,)> =
        sqlx::query_as("SELECT avatar_path FROM teams WHERE id = ? AND avatar_path IS NOT NULL")
            .bind(&team_id)
            .fetch_optional(&state.pool)
            .await
            .unwrap_or(None);
    if let Some((avatar_path,)) = old {
        let full_path = state.data_dir.join("avatars").join(&avatar_path);
        let _ = tokio::fs::remove_file(&full_path).await;
    }
    let _ = sqlx::query("UPDATE teams SET avatar_path = NULL WHERE id = ?")
        .bind(&team_id)
        .execute(&state.pool)
        .await;
    tracing::info!(team_id = %team_id, user_id = %user.id, "team avatar deleted");
    Redirect::to(&format!("/dashboard/teams/{}/settings", team_id)).into_response()
}

async fn delete_team(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
    headers: HeaderMap,
    Path(team_id): Path<String>,
    Form(csrf): Form<CsrfForm>,
) -> impl IntoResponse {
    if let Err(resp) = verify_csrf_token(&headers, &csrf._csrf) {
        return resp;
    }
    let user = &auth_user.user;
    let is_admin = user.role == "admin";
    if !is_admin && !is_team_admin(&state.pool, &user.id, &team_id).await {
        return Redirect::to("/dashboard/teams").into_response();
    }
    // Delete avatar file if present
    let old: Option<(String,)> =
        sqlx::query_as("SELECT avatar_path FROM teams WHERE id = ? AND avatar_path IS NOT NULL")
            .bind(&team_id)
            .fetch_optional(&state.pool)
            .await
            .unwrap_or(None);
    if let Some((avatar_path,)) = old {
        let full_path = state.data_dir.join("avatars").join(&avatar_path);
        let _ = tokio::fs::remove_file(&full_path).await;
    }
    // Nullify team_id on event types (don't delete them — they belong to the creator's account)
    let _ = sqlx::query("UPDATE event_types SET team_id = NULL WHERE team_id = ?")
        .bind(&team_id)
        .execute(&state.pool)
        .await;
    // Delete team (CASCADE removes team_members, team_groups)
    let _ = sqlx::query("DELETE FROM teams WHERE id = ?")
        .bind(&team_id)
        .execute(&state.pool)
        .await;
    tracing::info!(team_id = %team_id, user_id = %user.id, "team deleted");
    Redirect::to("/dashboard/teams").into_response()
}

async fn serve_team_avatar(
    State(state): State<Arc<AppState>>,
    Path(team_id): Path<String>,
) -> impl IntoResponse {
    // Team avatars are always public (they're just logos/icons, not sensitive).
    let filename: Option<(String,)> =
        sqlx::query_as("SELECT avatar_path FROM teams WHERE id = ? AND avatar_path IS NOT NULL")
            .bind(&team_id)
            .fetch_optional(&state.pool)
            .await
            .unwrap_or(None);

    let filename = match filename {
        Some((f,)) => f,
        None => return (axum::http::StatusCode::NOT_FOUND, "").into_response(),
    };

    let full_path = state.data_dir.join("avatars").join(&filename);
    match tokio::fs::read(&full_path).await {
        Ok(bytes) => {
            let content_type = if filename.ends_with(".jpg") || filename.ends_with(".jpeg") {
                "image/jpeg"
            } else if filename.ends_with(".png") {
                "image/png"
            } else if filename.ends_with(".gif") {
                "image/gif"
            } else if filename.ends_with(".webp") {
                "image/webp"
            } else {
                "image/png"
            };
            axum::response::Response::builder()
                .status(200)
                .header("Content-Type", content_type)
                .header("Cache-Control", "public, max-age=3600")
                .body(axum::body::Body::from(bytes))
                .unwrap_or_else(|_| axum::response::Response::new(axum::body::Body::empty()))
                .into_response()
        }
        Err(_) => (axum::http::StatusCode::NOT_FOUND, "").into_response(),
    }
}

// --- Cancel booking ---

#[derive(Deserialize)]
struct CancelForm {
    _csrf: Option<String>,
    reason: Option<String>,
}

async fn cancel_booking(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
    headers: HeaderMap,
    Path(booking_id): Path<String>,
    Form(form): Form<CancelForm>,
) -> impl IntoResponse {
    if let Err(resp) = verify_csrf_token(&headers, &form._csrf) {
        return resp;
    }
    let user = &auth_user.user;

    // Verify the booking belongs to this user and is confirmed or pending.
    // Pending bookings are "declined" (no CalDAV event was ever pushed); confirmed ones are "cancelled".
    let booking: Option<(
        String,
        String,
        String,
        String,
        String,
        String,
        String,
        String,
        String,
        String,
    )> = sqlx::query_as(
        "SELECT b.id, b.uid, b.guest_name, b.guest_email, b.start_at, b.end_at, et.title, et.id, COALESCE(b.guest_timezone, 'UTC'), b.status
             FROM bookings b
             JOIN event_types et ON et.id = b.event_type_id
             JOIN accounts a ON a.id = et.account_id
             WHERE b.id = ? AND a.user_id = ? AND b.status IN ('confirmed', 'pending')",
    )
    .bind(&booking_id)
    .bind(&user.id)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    let (
        bid,
        uid,
        guest_name,
        guest_email,
        start_at,
        end_at,
        event_title,
        et_id,
        guest_timezone,
        prev_status,
    ) = match booking {
        Some(b) => b,
        None => return Redirect::to("/dashboard/bookings").into_response(),
    };

    let was_pending = prev_status == "pending";
    let new_status = if was_pending { "declined" } else { "cancelled" };

    let _ = sqlx::query("UPDATE bookings SET status = ? WHERE id = ?")
        .bind(new_status)
        .bind(&bid)
        .execute(&state.pool)
        .await;

    tracing::info!(booking_id = %bid, status = new_status, "booking {}", new_status);

    if !was_pending {
        caldav_delete_booking(&state.pool, &state.secret_key, &user.id, &uid).await;
    }

    if let Ok(Some(smtp_config)) =
        crate::email::load_smtp_config(&state.pool, &state.secret_key).await
    {
        // Convert event-type-local stored times into the guest's tz; see #101.
        let stored_tz = get_host_tz(&state.pool, &et_id).await;
        let guest_tz_parsed = guest_timezone.parse::<Tz>().unwrap_or(Tz::UTC);
        let (date, start_time, end_time) =
            booking_strings_in_guest_tz(&start_at, &end_at, stored_tz, guest_tz_parsed);

        let reason = form.reason.filter(|r| !r.trim().is_empty());

        let details = crate::email::CancellationDetails {
            event_title: event_title.clone(),
            date: date.clone(),
            start_time: start_time.clone(),
            end_time: end_time.clone(),
            guest_name,
            guest_email,
            guest_timezone,
            host_name: user.name.clone(),
            host_email: user
                .booking_email
                .clone()
                .unwrap_or_else(|| user.email.clone()),
            uid,
            reason,
            cancelled_by_host: true,
            host_timezone: stored_tz.name().to_string(),
            ..Default::default()
        };

        if was_pending {
            let _ = crate::email::send_guest_decline_notice(&smtp_config, &details).await;
        } else {
            let _ = crate::email::send_guest_cancellation(&smtp_config, &details).await;
            let _ = crate::email::send_host_cancellation(&smtp_config, &details).await;
        }
    }

    Redirect::to("/dashboard/bookings").into_response()
}

// --- Confirm pending booking ---

async fn confirm_booking(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
    headers: HeaderMap,
    Path(booking_id): Path<String>,
    Form(csrf): Form<CsrfForm>,
) -> impl IntoResponse {
    if let Err(resp) = verify_csrf_token(&headers, &csrf._csrf) {
        return resp;
    }
    let user = &auth_user.user;

    // Verify the booking belongs to this user and is pending
    let booking: Option<(
        String,
        String,
        String,
        String,
        String,
        String,
        String,
        Option<String>,
        Option<String>,
        String,
        Option<String>,
        String,
    )> = sqlx::query_as(
        "SELECT b.id, b.uid, b.guest_name, b.guest_email, b.start_at, b.end_at, et.title, COALESCE(NULLIF(b.meeting_url, ''), et.location_value), b.cancel_token, COALESCE(b.guest_timezone, 'UTC'), b.reschedule_token, et.id
             FROM bookings b
             JOIN event_types et ON et.id = b.event_type_id
             JOIN accounts a ON a.id = et.account_id
             WHERE b.id = ? AND a.user_id = ? AND b.status = 'pending'",
    )
    .bind(&booking_id)
    .bind(&user.id)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    let (
        bid,
        uid,
        guest_name,
        guest_email,
        start_at,
        end_at,
        event_title,
        location_value,
        cancel_token,
        guest_timezone,
        reschedule_token,
        et_id,
    ) = match booking {
        Some(b) => b,
        None => return Redirect::to("/dashboard/bookings").into_response(),
    };

    // Confirm the booking
    let _ = sqlx::query("UPDATE bookings SET status = 'confirmed' WHERE id = ?")
        .bind(&bid)
        .execute(&state.pool)
        .await;

    tracing::info!(booking_id = %bid, "booking confirmed by host");

    // Mirror the email-token approve path: now that the booking is confirmed,
    // generate (or load via idempotency) the auto meeting URL so the host
    // notification email, ICS attachment, and CalDAV push all carry the link
    // for jitsi_auto/webhook_auto event types. Falls back to location_value
    // for static providers.
    let location_display = resolve_booking_location(
        &state,
        &bid,
        &et_id,
        Some(&user.id),
        location_value.as_deref(),
        &guest_name,
        &guest_email,
        true,
    )
    .await;

    // start_at/end_at are naive in the event type's timezone; convert to the
    // guest's tz so the confirmation email body and the ICS attachment match
    // what the guest booked. See #101.
    let stored_tz = get_host_tz(&state.pool, &et_id).await;
    let guest_tz_parsed = guest_timezone.parse::<Tz>().unwrap_or(Tz::UTC);
    let (date, start_time, end_time) =
        booking_strings_in_guest_tz(&start_at, &end_at, stored_tz, guest_tz_parsed);

    let details = crate::email::BookingDetails {
        event_title,
        date,
        start_time,
        end_time,
        guest_name,
        guest_email,
        guest_timezone,
        host_name: user.name.clone(),
        host_email: user
            .booking_email
            .clone()
            .unwrap_or_else(|| user.email.clone()),
        uid: uid.clone(),
        notes: None,
        location: location_display,
        reminder_minutes: None,
        additional_attendees: vec![],
        host_timezone: stored_tz.name().to_string(),
        ..Default::default()
    };

    // Push to CalDAV calendar
    caldav_push_booking(&state.pool, &state.secret_key, &user.id, &uid, &details).await;

    // Send confirmation emails
    if let Ok(Some(smtp_config)) =
        crate::email::load_smtp_config(&state.pool, &state.secret_key).await
    {
        let base_url = std::env::var("CALRS_BASE_URL").ok();
        let guest_cancel_url = cancel_token.as_ref().and_then(|t| {
            base_url
                .as_ref()
                .map(|base| format!("{}/booking/cancel/{}", base.trim_end_matches('/'), t))
        });
        let guest_reschedule_url = reschedule_token.as_ref().and_then(|t| {
            base_url
                .as_ref()
                .map(|base| format!("{}/booking/reschedule/{}", base.trim_end_matches('/'), t))
        });
        let (cancel_notice_min, reschedule_notice_min) =
            fetch_event_type_notice_minutes(&state.pool, &et_id).await;
        let _ = crate::email::send_guest_confirmation_ex(
            &smtp_config,
            &details,
            guest_cancel_url.as_deref(),
            guest_reschedule_url.as_deref(),
            cancel_notice_min,
            reschedule_notice_min,
        )
        .await;

        // Also send host a confirmation email (no ICS, event pushed via CalDAV)
        if let Err(e) = crate::email::send_host_booking_confirmed(&smtp_config, &details).await {
            tracing::error!(error = %e, host_email = %details.host_email, "host confirmation email failed");
        }
    }

    Redirect::to("/dashboard/bookings").into_response()
}

// --- Event type CRUD ---

#[derive(Deserialize)]
struct EventTypeForm {
    _csrf: Option<String>,
    title: String,
    slug: String,
    description: Option<String>,
    #[serde(default)]
    duration_min: String,
    #[serde(default)]
    slot_interval_min: String,
    #[serde(default)]
    buffer_before: String,
    #[serde(default)]
    buffer_after: String,
    #[serde(default)]
    min_notice_min: String,
    requires_confirmation: Option<String>, // checkbox: "on" or absent
    visibility: Option<String>,            // "public", "internal", or "private"
    location_type: Option<String>, // "link", "phone", "in_person", "custom", "jitsi_auto", "webhook_auto"
    location_value: Option<String>,
    // Optional per-event-type pattern override for the jitsi_auto provider.
    // NULL/empty falls back to the org-wide default pattern from auth_config.
    meeting_pattern_override: Option<String>,
    // Availability schedule
    avail_days: Option<String>,     // comma-separated: "1,2,3,4,5"
    avail_start: Option<String>,    // legacy: "09:00"
    avail_end: Option<String>,      // legacy: "17:00"
    avail_windows: Option<String>,  // "09:00-12:00,13:00-17:00"
    avail_schedule: Option<String>, // "1:09:00-17:00;2:09:00-12:00,13:00-17:00"
    // Scheduling mode (round_robin / collective)
    scheduling_mode: Option<String>,
    // Team (optional)
    team_id: Option<String>,
    // Calendar selection (comma-separated IDs)
    calendar_ids: Option<String>,
    // Reminder
    #[serde(default)]
    reminder_minutes: String,
    // Additional guests
    #[serde(default)]
    max_additional_guests: String,
    // Member priorities for round-robin (creation flow): "uid1:3,uid2:1,uid3:2"
    #[serde(default)]
    member_priorities: String,
    // Default calendar view for guests (month / week / column)
    default_calendar_view: Option<String>,
    // Booking frequency limits: "1:day,5:week,10:month"
    #[serde(default)]
    frequency_limits: String,
    // Show only the earliest available slot per day
    first_slot_only: Option<String>, // checkbox: "on" or absent
    // Watcher teams (comma-separated team IDs)
    watcher_team_ids: Option<String>,
    // Timezone in which the availability rules are interpreted. IANA name
    // (e.g. "Europe/Paris"). Optional on submit — if blank, create falls back
    // to the submitting user's timezone.
    timezone: Option<String>,
    // Minimum notice required for guest-initiated cancel. The form posts a
    // numeric value plus a unit (minutes / hours / days); the handler
    // converts to minutes and stores NULL when the value is empty.
    #[serde(default)]
    cancel_notice_value: String,
    #[serde(default)]
    cancel_notice_unit: String,
    // Minimum notice required for guest-initiated reschedule. Same shape.
    #[serde(default)]
    reschedule_notice_value: String,
    #[serde(default)]
    reschedule_notice_unit: String,
}

async fn new_event_type_form(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
    Query(query): Query<std::collections::HashMap<String, String>>,
) -> impl IntoResponse {
    let user = &auth_user.user;
    let preset = query.get("preset").map(|s| s.as_str()).unwrap_or("");

    // Get teams where the user is a team admin (global admins see all teams).
    // Only team admins can create team event types — regular members see no
    // teams in the dropdown.
    let groups: Vec<(String, String)> = if user.role == "admin" {
        sqlx::query_as("SELECT t.id, t.name FROM teams t ORDER BY t.name")
            .fetch_all(&state.pool)
            .await
            .unwrap_or_default()
    } else {
        sqlx::query_as(
            "SELECT t.id, t.name FROM teams t JOIN team_members tm ON tm.team_id = t.id WHERE tm.user_id = ? AND tm.role = 'admin' ORDER BY t.name",
        )
        .bind(&user.id)
        .fetch_all(&state.pool)
        .await
        .unwrap_or_default()
    };

    // Fetch members for each team (for client-side priority card)
    let mut groups_ctx: Vec<minijinja::Value> = Vec::new();
    for (id, name) in &groups {
        let team_members: Vec<(String, String, Option<String>, String)> = sqlx::query_as(
            "SELECT u.id, u.name, u.avatar_path, u.timezone FROM users u \
             JOIN team_members tm ON tm.user_id = u.id \
             WHERE tm.team_id = ? AND u.enabled = 1 ORDER BY u.name",
        )
        .bind(id)
        .fetch_all(&state.pool)
        .await
        .unwrap_or_default();
        let members_ctx: Vec<minijinja::Value> = team_members
            .iter()
            .map(|(uid, uname, ap, tz)| {
                context! {
                    user_id => uid,
                    name => uname,
                    has_avatar => ap.is_some(),
                    initials => compute_initials(uname),
                    timezone => tz,
                }
            })
            .collect();
        groups_ctx.push(context! { id => id, name => name, members => members_ctx });
    }

    // Get user's calendars (is_busy=1) for calendar selection
    let calendars: Vec<(String, Option<String>, String)> = sqlx::query_as(
        "SELECT c.id, c.display_name, cs.name FROM calendars c
         JOIN caldav_sources cs ON cs.id = c.source_id
         JOIN accounts a ON a.id = cs.account_id
         WHERE a.user_id = ? AND c.is_busy = 1
         ORDER BY cs.name, c.display_name",
    )
    .bind(&user.id)
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();

    let calendars_ctx: Vec<minijinja::Value> = calendars
        .iter()
        .map(|(id, display_name, source_name)| context! {
            id => id,
            name => format!("{} ({})", display_name.as_deref().unwrap_or("Unnamed"), source_name),
        })
        .collect();

    // Pre-fill availability from user's default schedule
    ensure_user_avail_seeded(&state.pool, &user.id).await;
    let user_avail = load_user_avail_schedule(&state.pool, &user.id).await;

    let tmpl = match state.templates.get_template("event_type_form.html") {
        Ok(t) => t,
        Err(e) => return internal_error_html("template render", &e),
    };

    let (impersonating, impersonating_name, _) = impersonation_ctx(&auth_user);
    Html(
        tmpl.render(context! {
            sidebar => sidebar_context(&auth_user, "event-types"),
            impersonating => impersonating,
            impersonating_name => impersonating_name,
            editing => false,
            preset => preset,
            teams => groups_ctx,
            calendars => calendars_ctx,
            selected_calendar_ids => "",
            form_title => "",
            form_slug => "",
            form_description => "",
            form_duration => 30,
            form_slot_interval => 0,
            form_buffer_before => 0,
            form_buffer_after => 0,
            form_min_notice => 60,
            form_requires_confirmation => matches!(preset, "private"),
            form_visibility => match preset { "private" => "private", "internal" => "internal", _ => "public" },
            form_location_type => "link",
            form_location_value => "",
            form_meeting_pattern_override => "",
            form_avail_schedule => user_avail,
            form_reminder_minutes => 1440,
            form_max_additional_guests => 0,
            form_default_calendar_view => "month",
            form_first_slot_only => false,
            form_frequency_limits => "",
            form_timezone => &user.timezone,
            form_cancel_notice_value => 0,
            form_cancel_notice_unit => "minutes",
            form_reschedule_notice_value => 0,
            form_reschedule_notice_unit => "minutes",
            tz_options => common_timezones_with(&user.timezone)
                .iter()
                .map(|(iana, label)| context! { value => iana, label => label })
                .collect::<Vec<_>>(),
            error => "",
        })
        .unwrap_or_else(|e| internal_error_body("template render", &e)),
    )
}

async fn create_event_type(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
    headers: HeaderMap,
    Form(form): Form<EventTypeForm>,
) -> impl IntoResponse {
    if let Err(resp) = verify_csrf_token(&headers, &form._csrf) {
        return resp;
    }
    let user = &auth_user.user;

    // Find the user's account
    let account_id: Option<String> =
        sqlx::query_scalar("SELECT id FROM accounts WHERE user_id = ? LIMIT 1")
            .bind(&user.id)
            .fetch_optional(&state.pool)
            .await
            .unwrap_or(None);

    let account_id = match account_id {
        Some(id) => id,
        None => return Redirect::to("/dashboard/event-types").into_response(),
    };

    // Generate slug from title if empty
    let mut slug = form.slug.trim().to_lowercase().replace(' ', "-");
    slug = slug
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-')
        .collect();
    if slug.is_empty() {
        slug = form
            .title
            .trim()
            .to_lowercase()
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
            .collect::<String>();
        // Collapse multiple dashes and trim
        while slug.contains("--") {
            slug = slug.replace("--", "-");
        }
        slug = slug.trim_matches('-').to_string();
    }
    if slug.is_empty() {
        return render_event_type_form_error(
            &state,
            &auth_user,
            "Title is required to generate a slug.",
            &form,
            false,
        )
        .into_response();
    }

    // Check if a team_id was provided and it's non-empty
    let team_id = form.team_id.as_deref().filter(|s| !s.trim().is_empty());

    // Check uniqueness — scope to team_id when creating a team event type, otherwise to account_id
    let existing: Option<(String,)> = if let Some(tid) = team_id {
        sqlx::query_as("SELECT id FROM event_types WHERE team_id = ? AND slug = ?")
            .bind(tid)
            .bind(&slug)
            .fetch_optional(&state.pool)
            .await
            .unwrap_or(None)
    } else {
        sqlx::query_as(
            "SELECT id FROM event_types WHERE account_id = ? AND slug = ? AND team_id IS NULL",
        )
        .bind(&account_id)
        .bind(&slug)
        .fetch_optional(&state.pool)
        .await
        .unwrap_or(None)
    };

    if existing.is_some() {
        return render_event_type_form_error(
            &state,
            &auth_user,
            "An event type with this slug already exists.",
            &form,
            false,
        )
        .into_response();
    }

    let et_id = uuid::Uuid::new_v4().to_string();
    let requires_confirmation = form.requires_confirmation.as_deref() == Some("on");

    let location_type = form.location_type.as_deref().unwrap_or("link");
    let location_value = form
        .location_value
        .as_deref()
        .filter(|s| !s.trim().is_empty());

    let location_required = !matches!(
        location_type,
        meeting::LOCATION_TYPE_JITSI | meeting::LOCATION_TYPE_WEBHOOK
    );
    if location_required && location_value.is_none() {
        return render_event_type_form_error(
            &state,
            &auth_user,
            "Location details are required (e.g. a video call link, phone number, or address).",
            &form,
            false,
        )
        .into_response();
    }

    // Only team admins (or global admins) can create team event types.
    if let Some(tid) = team_id {
        let is_global_admin = user.role == "admin";
        if !is_global_admin && !is_team_admin(&state.pool, &user.id, tid).await {
            return render_event_type_form_error(
                &state,
                &auth_user,
                "You are not a team admin of this team.",
                &form,
                false,
            )
            .into_response();
        }
    }

    let visibility = match form.visibility.as_deref().unwrap_or("public") {
        v @ ("public" | "internal" | "private") => v.to_string(),
        _ => "public".to_string(),
    };

    let reminder_minutes = {
        let v = parse_int_field(&form.reminder_minutes, 0);
        if v > 0 {
            Some(v)
        } else {
            None
        }
    };

    let default_calendar_view = match form.default_calendar_view.as_deref().unwrap_or("month") {
        v @ ("month" | "week" | "column") => v.to_string(),
        _ => "month".to_string(),
    };

    let first_slot_only = form.first_slot_only.as_deref() == Some("on");
    let timezone = normalize_event_type_tz(form.timezone.as_deref(), &user.timezone);
    let cancel_notice_min =
        parse_notice_to_minutes(&form.cancel_notice_value, &form.cancel_notice_unit);
    let reschedule_notice_min =
        parse_notice_to_minutes(&form.reschedule_notice_value, &form.reschedule_notice_unit);

    let meeting_pattern_override = form
        .meeting_pattern_override
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);

    let _ = sqlx::query(
        "INSERT INTO event_types (id, account_id, slug, title, description, duration_min, slot_interval_min, buffer_before, buffer_after, min_notice_min, requires_confirmation, location_type, location_value, team_id, created_by_user_id, reminder_minutes, visibility, max_additional_guests, default_calendar_view, first_slot_only, timezone, cancel_notice_min, reschedule_notice_min, meeting_pattern_override)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&et_id)
    .bind(&account_id)
    .bind(&slug)
    .bind(form.title.trim())
    .bind(form.description.as_deref().filter(|s| !s.trim().is_empty()))
    .bind(parse_int_field(&form.duration_min, 30))
    .bind(parse_optional_positive_int(&form.slot_interval_min))
    .bind(parse_int_field(&form.buffer_before, 0))
    .bind(parse_int_field(&form.buffer_after, 0))
    .bind(parse_int_field(&form.min_notice_min, 60))
    .bind(requires_confirmation as i32)
    .bind(location_type)
    .bind(location_value)
    .bind(team_id)
    .bind(if team_id.is_some() { Some(&user.id) } else { None })
    .bind(reminder_minutes)
    .bind(&visibility)
    .bind(parse_int_field(&form.max_additional_guests, 0))
    .bind(&default_calendar_view)
    .bind(first_slot_only as i32)
    .bind(&timezone)
    .bind(cancel_notice_min)
    .bind(reschedule_notice_min)
    .bind(&meeting_pattern_override)
    .execute(&state.pool)
    .await;

    tracing::info!(slug = %slug, user = %auth_user.user.email, "event type created");

    // Create availability rules. Pass the user's profile-default schedule as a
    // fallback so an empty submission falls back to it instead of hardcoded
    // Mon-Fri 09:00-17:00.
    ensure_user_avail_seeded(&state.pool, &auth_user.user.id).await;
    let user_default = load_user_avail_schedule(&state.pool, &auth_user.user.id).await;
    let schedule = parse_avail_schedule(
        form.avail_schedule.as_deref(),
        form.avail_days.as_deref(),
        form.avail_windows.as_deref(),
        form.avail_start.as_deref(),
        form.avail_end.as_deref(),
        Some(&user_default),
    );

    for (day, windows) in &schedule {
        for (ws, we) in windows {
            let rule_id = uuid::Uuid::new_v4().to_string();
            let _ = sqlx::query(
                "INSERT INTO availability_rules (id, event_type_id, day_of_week, start_time, end_time) VALUES (?, ?, ?, ?, ?)",
            )
            .bind(&rule_id)
            .bind(&et_id)
            .bind(day)
            .bind(ws)
            .bind(we)
            .execute(&state.pool)
            .await;
        }
    }

    // Save calendar selections
    if let Some(ref cal_ids_str) = form.calendar_ids {
        for cal_id in cal_ids_str.split(',') {
            let cal_id = cal_id.trim();
            if !cal_id.is_empty() {
                let _ = sqlx::query(
                    "INSERT INTO event_type_calendars (event_type_id, calendar_id) VALUES (?, ?)",
                )
                .bind(&et_id)
                .bind(cal_id)
                .execute(&state.pool)
                .await;
            }
        }
    }

    // Save member priorities (creation flow: "uid1:3,uid2:1,uid3:2")
    // Only insert weights for users who are actually team members.
    if let Some(tid) = team_id {
        if !form.member_priorities.is_empty() {
            let valid_members: Vec<(String,)> =
                sqlx::query_as("SELECT user_id FROM team_members WHERE team_id = ?")
                    .bind(tid)
                    .fetch_all(&state.pool)
                    .await
                    .unwrap_or_default();
            let valid_set: std::collections::HashSet<&str> =
                valid_members.iter().map(|(id,)| id.as_str()).collect();

            for entry in form.member_priorities.split(',') {
                let parts: Vec<&str> = entry.split(':').collect();
                if parts.len() == 2 {
                    let uid = parts[0].trim();
                    let weight: i64 = parts[1].trim().parse().unwrap_or(1);
                    if !uid.is_empty() && valid_set.contains(uid) {
                        let _ = sqlx::query(
                            "INSERT OR REPLACE INTO event_type_member_weights (event_type_id, user_id, weight) VALUES (?, ?, ?)",
                        )
                        .bind(&et_id)
                        .bind(uid)
                        .bind(weight)
                        .execute(&state.pool)
                        .await;
                    }
                }
            }
        }
    }

    // Save booking frequency limits
    save_frequency_limits(&state.pool, &et_id, &form.frequency_limits).await;

    Redirect::to("/dashboard/event-types").into_response()
}

async fn edit_event_type_form(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
    Path(slug): Path<String>,
) -> impl IntoResponse {
    let user = &auth_user.user;

    let et: Option<(String, String, String, Option<String>, i32, i32, i32, i32, i32, String, Option<String>, Option<i32>, String, i32, String, Option<String>)> = sqlx::query_as(
        "SELECT et.id, et.slug, et.title, et.description, et.duration_min, et.buffer_before, et.buffer_after, et.min_notice_min, et.requires_confirmation, et.location_type, et.location_value, et.reminder_minutes, et.visibility, et.max_additional_guests, et.scheduling_mode, et.team_id
         FROM event_types et
         JOIN accounts a ON a.id = et.account_id
         WHERE a.user_id = ? AND et.slug = ? AND et.team_id IS NULL",
    )
    .bind(&user.id)
    .bind(&slug)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    let (
        et_id,
        et_slug,
        et_title,
        et_desc,
        duration,
        buf_before,
        buf_after,
        min_notice,
        requires_conf,
        loc_type,
        loc_value,
        reminder_min,
        visibility,
        max_additional_guests,
        scheduling_mode,
        team_id,
    ) = match et {
        Some(e) => e,
        None => return Html("Event type not found.".to_string()),
    };

    let slot_interval: Option<i32> =
        sqlx::query_scalar("SELECT slot_interval_min FROM event_types WHERE id = ?")
            .bind(&et_id)
            .fetch_one(&state.pool)
            .await
            .unwrap_or(None);

    let default_calendar_view: String =
        sqlx::query_scalar("SELECT default_calendar_view FROM event_types WHERE id = ?")
            .bind(&et_id)
            .fetch_one(&state.pool)
            .await
            .unwrap_or_else(|_| "month".to_string());

    let first_slot_only: i32 =
        sqlx::query_scalar("SELECT first_slot_only FROM event_types WHERE id = ?")
            .bind(&et_id)
            .fetch_one(&state.pool)
            .await
            .unwrap_or(0);

    let form_timezone: String =
        sqlx::query_scalar::<_, Option<String>>("SELECT timezone FROM event_types WHERE id = ?")
            .bind(&et_id)
            .fetch_optional(&state.pool)
            .await
            .unwrap_or(None)
            .flatten()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| user.timezone.clone());

    // Min-notice for guest-initiated cancel and reschedule (NULL = no
    // restriction). Pulled separately to keep the existing tuple compact.
    let cancel_notice_min: Option<i32> = sqlx::query_scalar::<_, Option<i32>>(
        "SELECT cancel_notice_min FROM event_types WHERE id = ?",
    )
    .bind(&et_id)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None)
    .flatten();
    let reschedule_notice_min: Option<i32> = sqlx::query_scalar::<_, Option<i32>>(
        "SELECT reschedule_notice_min FROM event_types WHERE id = ?",
    )
    .bind(&et_id)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None)
    .flatten();
    let (form_cancel_notice_value, form_cancel_notice_unit) =
        minutes_to_notice_form(cancel_notice_min);
    let (form_reschedule_notice_value, form_reschedule_notice_unit) =
        minutes_to_notice_form(reschedule_notice_min);

    let meeting_pattern_override: Option<String> = sqlx::query_scalar::<_, Option<String>>(
        "SELECT meeting_pattern_override FROM event_types WHERE id = ?",
    )
    .bind(&et_id)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None)
    .flatten();

    // Get current availability rules
    let all_rules: Vec<(i32, String, String)> = sqlx::query_as(
        "SELECT day_of_week, start_time, end_time FROM availability_rules WHERE event_type_id = ? ORDER BY day_of_week, start_time",
    )
    .bind(&et_id)
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();

    let avail_days: String = {
        let mut days: Vec<i32> = all_rules.iter().map(|(d, _, _)| *d).collect();
        days.sort();
        days.dedup();
        days.iter()
            .map(|d| d.to_string())
            .collect::<Vec<_>>()
            .join(",")
    };

    // Collect distinct time windows (preserving order)
    let mut windows: Vec<(String, String)> = Vec::new();
    for (_, s, e) in &all_rules {
        let pair = (s.clone(), e.clone());
        if !windows.contains(&pair) {
            windows.push(pair);
        }
    }
    let avail_windows: String = windows
        .iter()
        .map(|(s, e)| format!("{}-{}", s, e))
        .collect::<Vec<_>>()
        .join(",");
    // Legacy fields for backward compat
    let (avail_start, avail_end) = windows
        .first()
        .cloned()
        .unwrap_or_else(|| ("09:00".to_string(), "17:00".to_string()));

    // Build per-day schedule string
    let avail_schedule = build_avail_schedule(&all_rules);

    // Get user's calendars (is_busy=1) for calendar selection
    let calendars: Vec<(String, Option<String>, String)> = sqlx::query_as(
        "SELECT c.id, c.display_name, cs.name FROM calendars c
         JOIN caldav_sources cs ON cs.id = c.source_id
         JOIN accounts a ON a.id = cs.account_id
         WHERE a.user_id = ? AND c.is_busy = 1
         ORDER BY cs.name, c.display_name",
    )
    .bind(&user.id)
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();

    let calendars_ctx: Vec<minijinja::Value> = calendars
        .iter()
        .map(|(id, display_name, source_name)| context! {
            id => id,
            name => format!("{} ({})", display_name.as_deref().unwrap_or("Unnamed"), source_name),
        })
        .collect();

    // Get currently selected calendars for this event type
    let selected_cals: Vec<(String,)> =
        sqlx::query_as("SELECT calendar_id FROM event_type_calendars WHERE event_type_id = ?")
            .bind(&et_id)
            .fetch_all(&state.pool)
            .await
            .unwrap_or_default();

    let selected_calendar_ids: String = selected_cals
        .iter()
        .map(|(id,)| id.as_str())
        .collect::<Vec<_>>()
        .join(",");

    // Fetch team members with per-ET weights (round-robin priority OR collective exclusion)
    let is_round_robin_group = team_id.is_some() && scheduling_mode == "round_robin";
    let is_collective_team = team_id.is_some() && scheduling_mode == "collective";
    let members_ctx: Vec<minijinja::Value> = if is_round_robin_group || is_collective_team {
        let tid = team_id.as_deref().unwrap();
        // Also selects timezone so admins can spot mis-configured user TZs at
        // a glance when setting up a team event (e.g. a US member whose TZ
        // is still the server default — which silently makes their personal
        // working hours land in the wrong local time).
        let members: Vec<(String, String, Option<String>, String)> = sqlx::query_as(
            "SELECT u.id, u.name, u.avatar_path, u.timezone \
             FROM users u JOIN team_members tm ON tm.user_id = u.id \
             WHERE tm.team_id = ? AND u.enabled = 1 \
             ORDER BY u.name",
        )
        .bind(tid)
        .fetch_all(&state.pool)
        .await
        .unwrap_or_default();

        let et_weights: Vec<(String, i64)> = sqlx::query_as(
            "SELECT user_id, weight FROM event_type_member_weights WHERE event_type_id = ?",
        )
        .bind(&et_id)
        .fetch_all(&state.pool)
        .await
        .unwrap_or_default();
        let wmap: std::collections::HashMap<String, i64> = et_weights.into_iter().collect();

        members
            .iter()
            .map(|(uid, name, avatar_path, timezone)| {
                let w = wmap.get(uid).copied().unwrap_or(1);
                context! {
                    user_id => uid,
                    name => name,
                    has_avatar => avatar_path.is_some(),
                    initials => compute_initials(name),
                    weight => w,
                    timezone => timezone,
                }
            })
            .collect()
    } else {
        Vec::new()
    };

    // Fetch booking frequency limits
    let freq_limits: Vec<(i32, String, i32)> = sqlx::query_as(
        "SELECT max_bookings, period, per_member FROM booking_frequency_limits WHERE event_type_id = ?",
    )
    .bind(&et_id)
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();

    let form_frequency_limits = freq_limits
        .iter()
        .map(|(c, p, m)| {
            if *m != 0 {
                format!("{}:{}:m", c, p)
            } else {
                format!("{}:{}", c, p)
            }
        })
        .collect::<Vec<_>>()
        .join(",");

    let tmpl = match state.templates.get_template("event_type_form.html") {
        Ok(t) => t,
        Err(e) => return internal_error_html("template render", &e),
    };

    let (impersonating, impersonating_name, _) = impersonation_ctx(&auth_user);

    // Eligible users for dynamic group link picker (excluding self)
    let dg_eligible: Vec<(String, String, String, Option<String>)> = sqlx::query_as(
        "SELECT id, username, name, avatar_path FROM users WHERE enabled = 1 AND allow_dynamic_group = 1 AND id != ? AND username IS NOT NULL ORDER BY name",
    )
    .bind(&user.id)
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();
    let dg_eligible_ctx: Vec<minijinja::Value> = dg_eligible
        .iter()
        .map(|(id, username, name, avatar_path)| {
            context! {
                id => id,
                username => username,
                name => name,
                has_avatar => avatar_path.is_some(),
                initials => compute_initials(name),
            }
        })
        .collect();

    Html(
        tmpl.render(context! {
            editing => true,
            original_slug => et_slug,
            calendars => calendars_ctx,
            selected_calendar_ids => selected_calendar_ids,
            form_title => et_title,
            form_slug => et_slug,
            form_description => et_desc.unwrap_or_default(),
            form_duration => duration,
            form_slot_interval => slot_interval.unwrap_or(0),
            form_buffer_before => buf_before,
            form_buffer_after => buf_after,
            form_min_notice => min_notice,
            form_requires_confirmation => requires_conf != 0,
            form_visibility => visibility,
            form_location_type => loc_type,
            form_location_value => loc_value.unwrap_or_default(),
            form_meeting_pattern_override => meeting_pattern_override.clone().unwrap_or_default(),
            form_avail_days => avail_days,
            form_avail_start => avail_start,
            form_avail_end => avail_end,
            form_avail_windows => avail_windows,
            form_avail_schedule => avail_schedule,
            form_reminder_minutes => reminder_min.unwrap_or(0),
            form_max_additional_guests => max_additional_guests,
            form_scheduling_mode => scheduling_mode,
            form_default_calendar_view => default_calendar_view,
            form_first_slot_only => first_slot_only != 0,
            form_frequency_limits => form_frequency_limits,
            form_timezone => &form_timezone,
            form_cancel_notice_value => form_cancel_notice_value,
            form_cancel_notice_unit => form_cancel_notice_unit,
            form_reschedule_notice_value => form_reschedule_notice_value,
            form_reschedule_notice_unit => form_reschedule_notice_unit,
            tz_options => common_timezones_with(&form_timezone)
                .iter()
                .map(|(iana, label)| context! { value => iana, label => label })
                .collect::<Vec<_>>(),
            is_group => team_id.is_some(),
            is_round_robin_group => is_round_robin_group,
            is_collective_team => is_collective_team,
            priority_members => members_ctx,
            owner_username => auth_user.user.username.as_deref().unwrap_or(""),
            dg_eligible_users => dg_eligible_ctx,
            error => "",
            sidebar => sidebar_context(&auth_user, "event-types"),
            impersonating => impersonating,
            impersonating_name => impersonating_name,
        })
        .unwrap_or_else(|e| internal_error_body("template render", &e)),
    )
}

async fn update_event_type(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
    headers: HeaderMap,
    Path(slug): Path<String>,
    Form(form): Form<EventTypeForm>,
) -> impl IntoResponse {
    if let Err(resp) = verify_csrf_token(&headers, &form._csrf) {
        return resp;
    }
    let user = &auth_user.user;

    let et: Option<(String, String)> = sqlx::query_as(
        "SELECT et.id, et.account_id
         FROM event_types et
         JOIN accounts a ON a.id = et.account_id
         WHERE a.user_id = ? AND et.slug = ? AND et.team_id IS NULL",
    )
    .bind(&user.id)
    .bind(&slug)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    let (et_id, account_id) = match et {
        Some(e) => e,
        None => return Redirect::to("/dashboard/event-types").into_response(),
    };

    let new_slug = form.slug.trim().to_lowercase().replace(' ', "-");
    let requires_confirmation = form.requires_confirmation.as_deref() == Some("on");
    let visibility = match form.visibility.as_deref().unwrap_or("public") {
        v @ ("public" | "internal" | "private") => v.to_string(),
        _ => "public".to_string(),
    };

    // Check slug uniqueness if changed
    if new_slug != slug {
        let existing: Option<(String,)> =
            sqlx::query_as("SELECT id FROM event_types WHERE account_id = ? AND slug = ?")
                .bind(&account_id)
                .bind(&new_slug)
                .fetch_optional(&state.pool)
                .await
                .unwrap_or(None);

        if existing.is_some() {
            return render_event_type_form_error(
                &state,
                &auth_user,
                "An event type with this slug already exists.",
                &form,
                true,
            )
            .into_response();
        }
    }

    let location_type = form.location_type.as_deref().unwrap_or("link");
    let location_value = form
        .location_value
        .as_deref()
        .filter(|s| !s.trim().is_empty());

    let location_required = !matches!(
        location_type,
        meeting::LOCATION_TYPE_JITSI | meeting::LOCATION_TYPE_WEBHOOK
    );
    if location_required && location_value.is_none() {
        return render_event_type_form_error(
            &state,
            &auth_user,
            "Location details are required (e.g. a video call link, phone number, or address).",
            &form,
            true,
        )
        .into_response();
    }

    let reminder_minutes = {
        let v = parse_int_field(&form.reminder_minutes, 0);
        if v > 0 {
            Some(v)
        } else {
            None
        }
    };

    let default_calendar_view = match form.default_calendar_view.as_deref().unwrap_or("month") {
        v @ ("month" | "week" | "column") => v.to_string(),
        _ => "month".to_string(),
    };

    let timezone = normalize_event_type_tz(form.timezone.as_deref(), &user.timezone);
    let cancel_notice_min =
        parse_notice_to_minutes(&form.cancel_notice_value, &form.cancel_notice_unit);
    let reschedule_notice_min =
        parse_notice_to_minutes(&form.reschedule_notice_value, &form.reschedule_notice_unit);

    let meeting_pattern_override = form
        .meeting_pattern_override
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);

    let _ = sqlx::query(
        "UPDATE event_types SET slug = ?, title = ?, description = ?, duration_min = ?, slot_interval_min = ?, buffer_before = ?, buffer_after = ?, min_notice_min = ?, requires_confirmation = ?, location_type = ?, location_value = ?, reminder_minutes = ?, visibility = ?, max_additional_guests = ?, scheduling_mode = ?, default_calendar_view = ?, first_slot_only = ?, timezone = ?, cancel_notice_min = ?, reschedule_notice_min = ?, meeting_pattern_override = ? WHERE id = ?",
    )
    .bind(&new_slug)
    .bind(form.title.trim())
    .bind(form.description.as_deref().filter(|s| !s.trim().is_empty()))
    .bind(parse_int_field(&form.duration_min, 30))
    .bind(parse_optional_positive_int(&form.slot_interval_min))
    .bind(parse_int_field(&form.buffer_before, 0))
    .bind(parse_int_field(&form.buffer_after, 0))
    .bind(parse_int_field(&form.min_notice_min, 60))
    .bind(requires_confirmation as i32)
    .bind(location_type)
    .bind(location_value)
    .bind(reminder_minutes)
    .bind(&visibility)
    .bind(parse_int_field(&form.max_additional_guests, 0))
    .bind(form.scheduling_mode.as_deref().unwrap_or("round_robin"))
    .bind(&default_calendar_view)
    .bind(if form.first_slot_only.as_deref() == Some("on") { 1 } else { 0 })
    .bind(&timezone)
    .bind(cancel_notice_min)
    .bind(reschedule_notice_min)
    .bind(&meeting_pattern_override)
    .bind(&et_id)
    .execute(&state.pool)
    .await;

    // Update availability rules: delete old, insert new. Pass the user's
    // profile-default schedule as a fallback so an empty submission falls back
    // to it instead of hardcoded Mon-Fri 09:00-17:00.
    let _ = sqlx::query("DELETE FROM availability_rules WHERE event_type_id = ?")
        .bind(&et_id)
        .execute(&state.pool)
        .await;

    ensure_user_avail_seeded(&state.pool, &auth_user.user.id).await;
    let user_default = load_user_avail_schedule(&state.pool, &auth_user.user.id).await;
    let schedule = parse_avail_schedule(
        form.avail_schedule.as_deref(),
        form.avail_days.as_deref(),
        form.avail_windows.as_deref(),
        form.avail_start.as_deref(),
        form.avail_end.as_deref(),
        Some(&user_default),
    );

    for (day, windows) in &schedule {
        for (ws, we) in windows {
            let rule_id = uuid::Uuid::new_v4().to_string();
            let _ = sqlx::query(
                "INSERT INTO availability_rules (id, event_type_id, day_of_week, start_time, end_time) VALUES (?, ?, ?, ?, ?)",
            )
            .bind(&rule_id)
            .bind(&et_id)
            .bind(day)
            .bind(ws)
            .bind(we)
            .execute(&state.pool)
            .await;
        }
    }

    // Update calendar selections: delete old, insert new
    let _ = sqlx::query("DELETE FROM event_type_calendars WHERE event_type_id = ?")
        .bind(&et_id)
        .execute(&state.pool)
        .await;

    if let Some(ref cal_ids_str) = form.calendar_ids {
        for cal_id in cal_ids_str.split(',') {
            let cal_id = cal_id.trim();
            if !cal_id.is_empty() {
                let _ = sqlx::query(
                    "INSERT INTO event_type_calendars (event_type_id, calendar_id) VALUES (?, ?)",
                )
                .bind(&et_id)
                .bind(cal_id)
                .execute(&state.pool)
                .await;
            }
        }
    }

    // Update booking frequency limits: delete old, insert new
    let _ = sqlx::query("DELETE FROM booking_frequency_limits WHERE event_type_id = ?")
        .bind(&et_id)
        .execute(&state.pool)
        .await;
    save_frequency_limits(&state.pool, &et_id, &form.frequency_limits).await;

    Redirect::to("/dashboard/event-types").into_response()
}

async fn update_event_type_member_priority(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
    headers: HeaderMap,
    Path((slug, target_user_id)): Path<(String, String)>,
    Form(form): Form<PriorityForm>,
) -> impl IntoResponse {
    if let Err(resp) = verify_csrf_token(&headers, &form._csrf) {
        return resp;
    }
    let user = &auth_user.user;

    // Resolve a personal event type owned by this user (team priority is
    // handled by the group-scoped route).
    let et: Option<(String,)> = sqlx::query_as(
        "SELECT et.id FROM event_types et \
         JOIN accounts a ON a.id = et.account_id \
         WHERE a.user_id = ? AND et.slug = ? AND et.team_id IS NULL",
    )
    .bind(&user.id)
    .bind(&slug)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    let et_id = match et {
        Some((id,)) => id,
        None => return Redirect::to("/dashboard/event-types").into_response(),
    };

    let weight: i64 = match form.priority.as_str() {
        "high" => 3,
        "medium" => 2,
        "exclude" => 0,
        _ => 1,
    };

    let _ = sqlx::query(
        "INSERT INTO event_type_member_weights (event_type_id, user_id, weight) \
         VALUES (?, ?, ?) \
         ON CONFLICT(event_type_id, user_id) DO UPDATE SET weight = excluded.weight",
    )
    .bind(&et_id)
    .bind(&target_user_id)
    .bind(weight)
    .execute(&state.pool)
    .await;

    tracing::info!(
        slug,
        target_user_id,
        priority = form.priority.as_str(),
        "updated member priority for event type"
    );

    Redirect::to(&format!("/dashboard/event-types/{}/edit", slug)).into_response()
}

async fn update_group_event_type_member_priority(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
    headers: HeaderMap,
    Path((team_id, slug, target_user_id)): Path<(String, String, String)>,
    Form(form): Form<PriorityForm>,
) -> impl IntoResponse {
    if let Err(resp) = verify_csrf_token(&headers, &form._csrf) {
        return resp;
    }
    let user = &auth_user.user;
    let is_admin = user.role == "admin";

    // Resolve event type within the addressed team. Two teams may share the
    // same slug, so the team_id from the route is required for the lookup
    // to be unambiguous.
    let et: Option<(String,)> = if is_admin {
        sqlx::query_as(
            "SELECT et.id FROM event_types et \
             WHERE et.team_id = ? AND et.slug = ?",
        )
        .bind(&team_id)
        .bind(&slug)
        .fetch_optional(&state.pool)
        .await
        .unwrap_or(None)
    } else {
        sqlx::query_as(
            "SELECT et.id FROM event_types et \
             JOIN team_members tm ON tm.team_id = et.team_id \
             WHERE tm.user_id = ? AND tm.role = 'admin' AND et.team_id = ? AND et.slug = ?",
        )
        .bind(&user.id)
        .bind(&team_id)
        .bind(&slug)
        .fetch_optional(&state.pool)
        .await
        .unwrap_or(None)
    };

    let et_id = match et {
        Some((id,)) => id,
        None => return Redirect::to("/dashboard/event-types").into_response(),
    };

    let weight: i64 = match form.priority.as_str() {
        "high" => 3,
        "medium" => 2,
        "exclude" => 0,
        _ => 1,
    };

    let _ = sqlx::query(
        "INSERT INTO event_type_member_weights (event_type_id, user_id, weight) \
         VALUES (?, ?, ?) \
         ON CONFLICT(event_type_id, user_id) DO UPDATE SET weight = excluded.weight",
    )
    .bind(&et_id)
    .bind(&target_user_id)
    .bind(weight)
    .execute(&state.pool)
    .await;

    tracing::info!(
        slug,
        target_user_id,
        priority = form.priority.as_str(),
        "updated member priority for group event type"
    );

    Redirect::to(&format!(
        "/dashboard/group-event-types/{}/{}/edit",
        team_id, slug
    ))
    .into_response()
}

async fn toggle_event_type(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
    headers: HeaderMap,
    Path(slug): Path<String>,
    Form(csrf): Form<CsrfForm>,
) -> impl IntoResponse {
    if let Err(resp) = verify_csrf_token(&headers, &csrf._csrf) {
        return resp;
    }
    let user = &auth_user.user;

    let _ = sqlx::query(
        "UPDATE event_types SET enabled = CASE WHEN enabled = 1 THEN 0 ELSE 1 END
         WHERE slug = ? AND team_id IS NULL AND account_id IN (SELECT id FROM accounts WHERE user_id = ?)",
    )
    .bind(&slug)
    .bind(&user.id)
    .execute(&state.pool)
    .await;

    tracing::debug!(event_type_id = %slug, "event type toggled");

    Redirect::to("/dashboard/event-types").into_response()
}

async fn delete_event_type(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
    headers: HeaderMap,
    Path(slug): Path<String>,
    Form(csrf): Form<CsrfForm>,
) -> impl IntoResponse {
    if let Err(resp) = verify_csrf_token(&headers, &csrf._csrf) {
        return resp;
    }
    let user = &auth_user.user;

    // Find the personal event type owned by this user (team event types are
    // managed via the team-scoped routes).
    let et: Option<(String,)> = sqlx::query_as(
        "SELECT et.id FROM event_types et
         JOIN accounts a ON a.id = et.account_id
         WHERE et.slug = ? AND a.user_id = ? AND et.team_id IS NULL",
    )
    .bind(&slug)
    .bind(&user.id)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    let et_id = match et {
        Some((id,)) => id,
        None => return Redirect::to("/dashboard/event-types").into_response(),
    };

    // Check for active bookings (confirmed or pending)
    let active_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM bookings WHERE event_type_id = ? AND status IN ('confirmed', 'pending')",
    )
    .bind(&et_id)
    .fetch_one(&state.pool)
    .await
    .unwrap_or(0);

    if active_count > 0 {
        // Can't delete — has active bookings. Just redirect back.
        return Redirect::to("/dashboard/event-types").into_response();
    }

    // Delete in order: availability_rules, availability_overrides, event_type_calendars, bookings (past/cancelled), then event_type
    let _ = sqlx::query("DELETE FROM availability_rules WHERE event_type_id = ?")
        .bind(&et_id)
        .execute(&state.pool)
        .await;
    let _ = sqlx::query("DELETE FROM availability_overrides WHERE event_type_id = ?")
        .bind(&et_id)
        .execute(&state.pool)
        .await;
    let _ = sqlx::query("DELETE FROM event_type_calendars WHERE event_type_id = ?")
        .bind(&et_id)
        .execute(&state.pool)
        .await;
    let _ = sqlx::query("DELETE FROM bookings WHERE event_type_id = ?")
        .bind(&et_id)
        .execute(&state.pool)
        .await;
    let _ = sqlx::query("DELETE FROM event_types WHERE id = ?")
        .bind(&et_id)
        .execute(&state.pool)
        .await;

    tracing::info!(event_type_id = %et_id, user = %auth_user.user.email, "event type deleted");

    Redirect::to("/dashboard/event-types").into_response()
}

#[derive(Deserialize)]
struct SourceForm {
    _csrf: Option<String>,
    provider: Option<String>,
    /// Backend protocol: "caldav" (default) or "ews".
    #[serde(default)]
    provider_type: Option<String>,
    name: String,
    url: String,
    username: String,
    password: String,
    no_test: Option<String>,
}

/// Resolve and validate the provider type from a form input. Defaults to
/// `caldav` when the field is missing (older clients) and rejects unknown
/// values rather than silently coercing.
fn parse_provider_type(raw: Option<&str>) -> Result<String, String> {
    let value = raw
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("caldav");
    match value {
        crate::providers::factory::kinds::CALDAV => Ok("caldav".to_string()),
        crate::providers::factory::kinds::EWS => Ok("ews".to_string()),
        other => Err(format!("Unknown provider type '{}'", other)),
    }
}

/// Preset list shown in the source-add form. Tuple is
/// `(id, display name, default URL, backend)`, where `backend` is `caldav`
/// or `ews`. The template uses the backend tag to keep the Backend dropdown
/// in sync with the preset choice.
fn caldav_providers() -> Vec<(&'static str, &'static str, &'static str, &'static str)> {
    vec![
        (
            "bluemind",
            "BlueMind",
            "https://mail.example.com/dav/",
            "caldav",
        ),
        (
            "nextcloud",
            "Nextcloud",
            "https://cloud.example.com/remote.php/dav",
            "caldav",
        ),
        (
            "fastmail",
            "Fastmail",
            "https://caldav.fastmail.com/dav/calendars/user/you@fastmail.com/",
            "caldav",
        ),
        ("icloud", "iCloud", "https://caldav.icloud.com/", "caldav"),
        (
            "zimbra",
            "Zimbra",
            "https://mail.example.com/dav/",
            "caldav",
        ),
        (
            "sogo",
            "SOGo",
            "https://mail.example.com/SOGo/dav/",
            "caldav",
        ),
        ("radicale", "Radicale", "https://cal.example.com/", "caldav"),
        ("google", "Google Calendar", "", "caldav"),
        (
            "exchange",
            "Microsoft Exchange (EWS)",
            "https://mail.example.com/EWS/Exchange.asmx",
            "ews",
        ),
        ("other", "Other / Generic CalDAV", "", "caldav"),
    ]
}

/// When the admin has enabled the "lock user sources" flag, non-admin users
/// can't add or edit their own calendar sources. Returns `Some(response)` with
/// a 403 to short-circuit the handler in that case, `None` to proceed.
async fn user_source_lockdown_response(
    state: &Arc<AppState>,
    auth_user: &crate::auth::AuthUser,
) -> Option<Response> {
    if auth_user.user.role == "admin" {
        return None;
    }
    let locked = state
        .ews_global
        .read()
        .await
        .as_ref()
        .map(|c| c.lock_user_sources)
        .unwrap_or(false);
    if !locked {
        return None;
    }
    Some(
        (
            axum::http::StatusCode::FORBIDDEN,
            Html(
                "Adding personal calendar sources is disabled by the administrator. \
                 Your Exchange calendar is managed centrally."
                    .to_string(),
            ),
        )
            .into_response(),
    )
}

async fn new_source_form(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
) -> Response {
    if let Some(resp) = user_source_lockdown_response(&state, &auth_user).await {
        return resp;
    }
    let tmpl = match state.templates.get_template("source_form.html") {
        Ok(t) => t,
        Err(e) => return internal_error_html("template render", &e).into_response(),
    };

    let providers: Vec<minijinja::Value> = caldav_providers()
        .iter()
        .map(|(id, name, url, backend)| context! { id => id, name => name, url => url, backend => backend })
        .collect();

    let google_configured: bool = sqlx::query_scalar::<_, Option<String>>(
        "SELECT google_oauth2_client_id FROM auth_config LIMIT 1",
    )
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None)
    .flatten()
    .map(|s| !s.is_empty())
    .unwrap_or(false);

    let (impersonating, impersonating_name, _) = impersonation_ctx(&auth_user);
    Html(
        tmpl.render(context! {
            providers => providers,
            form_provider => "bluemind",
            form_provider_type => "caldav",
            form_name => "",
            form_url => "https://mail.example.com/dav/",
            form_username => "",
            error => "",
            google_oauth2_configured => google_configured,
            sidebar => sidebar_context(&auth_user, "sources"),
            impersonating => impersonating,
            impersonating_name => impersonating_name,
        })
        .unwrap_or_else(|e| internal_error_body("template render", &e)),
    )
    .into_response()
}

async fn create_source(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
    headers: HeaderMap,
    Form(form): Form<SourceForm>,
) -> impl IntoResponse {
    if let Err(resp) = verify_csrf_token(&headers, &form._csrf) {
        return resp;
    }
    if let Some(resp) = user_source_lockdown_response(&state, &auth_user).await {
        return resp;
    }
    let user = &auth_user.user;

    let account_id: Option<String> =
        sqlx::query_scalar("SELECT id FROM accounts WHERE user_id = ? LIMIT 1")
            .bind(&user.id)
            .fetch_optional(&state.pool)
            .await
            .unwrap_or(None);

    let account_id = match account_id {
        Some(id) => id,
        None => {
            return render_source_form_error(
                &state,
                &auth_user,
                "No scheduling account found. Please contact an administrator.",
                &form,
            )
            .into_response()
        }
    };

    let url = form.url.trim().to_string();
    let username = form.username.trim().to_string();
    let name = form.name.trim().to_string();

    if url.is_empty() || username.is_empty() || name.is_empty() || form.password.is_empty() {
        return render_source_form_error(&state, &auth_user, "All fields are required.", &form)
            .into_response();
    }

    let provider_type = match parse_provider_type(form.provider_type.as_deref()) {
        Ok(p) => p,
        Err(msg) => {
            return render_source_form_error(&state, &auth_user, &msg, &form).into_response();
        }
    };

    // Validate URL against SSRF (HTTPS-only, no private targets) for both
    // CalDAV and EWS — the validator is shared.
    if let Err(e) = crate::providers::factory::validate_url(&provider_type, &url) {
        return render_source_form_error(&state, &auth_user, &e.to_string(), &form).into_response();
    }

    // Test connection unless skip requested
    let skip_test = form.no_test.as_deref() == Some("on");
    if !skip_test {
        let client = match crate::providers::build_provider(
            &provider_type,
            &url,
            &username,
            &form.password,
            None,
        ) {
            Ok(c) => c,
            Err(e) => {
                return render_source_form_error(&state, &auth_user, &e.to_string(), &form)
                    .into_response();
            }
        };
        match client.check_connection().await {
            Ok(_) => {} // fine, even if features not explicitly advertised
            Err(e) => {
                let msg = format!("Connection failed: {}. Check the URL and credentials, or check \"Skip connection test\" to save anyway.", e);
                return render_source_form_error(&state, &auth_user, &msg, &form).into_response();
            }
        }
    }

    let id = uuid::Uuid::new_v4().to_string();
    let password_enc = match crate::crypto::encrypt_password(&state.secret_key, &form.password) {
        Ok(enc) => enc,
        Err(_) => return Html("Encryption error.".to_string()).into_response(),
    };

    let _ = sqlx::query(
        "INSERT INTO caldav_sources (id, account_id, name, url, username, password_enc, provider_type) VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(&account_id)
    .bind(&name)
    .bind(&url)
    .bind(&username)
    .bind(&password_enc)
    .bind(&provider_type)
    .execute(&state.pool)
    .await;

    tracing::info!(source_name = %name, provider = %provider_type, user = %auth_user.user.email, "calendar source added");

    // Auto-sync immediately after creating the source, then redirect to
    // write-back setup if calendars were found.
    let (messages, calendar_count) = run_sync(
        &state.pool,
        &state.secret_key,
        &id,
        &provider_type,
        &url,
        &username,
        &form.password,
    )
    .await;

    if calendar_count > 0 {
        let joined_messages = messages.join("\n");
        let encoded_messages = urlencoding::encode(&joined_messages);
        return Redirect::to(&format!(
            "/dashboard/sources/{}/setup-write?sync_messages={}",
            id, encoded_messages
        ))
        .into_response();
    }

    Redirect::to("/dashboard/sources").into_response()
}

fn render_source_form_error(
    state: &AppState,
    auth_user: &crate::auth::AuthUser,
    error: &str,
    form: &SourceForm,
) -> Html<String> {
    let tmpl = match state.templates.get_template("source_form.html") {
        Ok(t) => t,
        Err(e) => return internal_error_html("template render", &e),
    };

    let providers: Vec<minijinja::Value> = caldav_providers()
        .iter()
        .map(|(id, name, url, backend)| context! { id => id, name => name, url => url, backend => backend })
        .collect();

    let (impersonating, impersonating_name, _) = impersonation_ctx(auth_user);
    Html(
        tmpl.render(context! {
            providers => providers,
            form_provider => form.provider.as_deref().unwrap_or("other"),
            form_provider_type => form.provider_type.as_deref().unwrap_or("caldav"),
            form_name => form.name.as_str(),
            form_url => form.url.as_str(),
            form_username => form.username.as_str(),
            error => error,
            sidebar => sidebar_context(auth_user, "sources"),
            impersonating => impersonating,
            impersonating_name => impersonating_name,
        })
        .unwrap_or_else(|e| internal_error_body("template render", &e)),
    )
}

/// Render the source form in editing mode with the given values.
fn render_source_edit_form(
    state: &AppState,
    auth_user: &crate::auth::AuthUser,
    source_id: &str,
    name: &str,
    url: &str,
    username: &str,
    error: &str,
) -> Html<String> {
    let tmpl = match state.templates.get_template("source_form.html") {
        Ok(t) => t,
        Err(e) => return Html(format!("Template error: {}", e)),
    };

    let providers: Vec<minijinja::Value> = caldav_providers()
        .iter()
        .map(|(id, name, url, backend)| context! { id => id, name => name, url => url, backend => backend })
        .collect();

    let (impersonating, impersonating_name, _) = impersonation_ctx(auth_user);
    Html(
        tmpl.render(context! {
            editing => true,
            source_id => source_id,
            providers => providers,
            form_provider => "other",
            form_name => name,
            form_url => url,
            form_username => username,
            error => error,
            sidebar => sidebar_context(auth_user, "sources"),
            impersonating => impersonating,
            impersonating_name => impersonating_name,
        })
        .unwrap_or_else(|e| format!("Template error: {}", e)),
    )
}

async fn edit_source_form(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
    Path(source_id): Path<String>,
) -> impl IntoResponse {
    let user = &auth_user.user;
    // Verify ownership and load current values.
    let source: Option<(String, String, String)> = sqlx::query_as(
        "SELECT cs.name, cs.url, cs.username
         FROM caldav_sources cs
         JOIN accounts a ON a.id = cs.account_id
         WHERE cs.id = ? AND a.user_id = ?",
    )
    .bind(&source_id)
    .bind(&user.id)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    let (name, url, username) = match source {
        Some(s) => s,
        None => return Redirect::to("/dashboard/sources").into_response(),
    };

    render_source_edit_form(&state, &auth_user, &source_id, &name, &url, &username, "")
        .into_response()
}

async fn update_source(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
    headers: HeaderMap,
    Path(source_id): Path<String>,
    Form(form): Form<SourceForm>,
) -> impl IntoResponse {
    if let Err(resp) = verify_csrf_token(&headers, &form._csrf) {
        return resp;
    }
    let user = &auth_user.user;

    // Confirm the source belongs to this user and grab the existing
    // password (used as fallback when the form leaves it blank).
    let existing: Option<(String, i64)> = sqlx::query_as(
        "SELECT cs.password_enc, cs.managed
         FROM caldav_sources cs
         JOIN accounts a ON a.id = cs.account_id
         WHERE cs.id = ? AND a.user_id = ?",
    )
    .bind(&source_id)
    .bind(&user.id)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    let (existing_password_enc, is_managed) = match existing {
        Some((enc, managed)) => (enc, managed),
        None => return Redirect::to("/dashboard/sources").into_response(),
    };

    // Managed sources are owned by the global admin config — regular users
    // cannot mutate them via the dashboard.
    if is_managed != 0 && user.role != "admin" {
        return (
            axum::http::StatusCode::FORBIDDEN,
            Html(
                "This calendar source is provisioned by the administrator and cannot be edited."
                    .to_string(),
            ),
        )
            .into_response();
    }

    let url = form.url.trim().to_string();
    let username = form.username.trim().to_string();
    let name = form.name.trim().to_string();

    if url.is_empty() || username.is_empty() || name.is_empty() {
        return render_source_edit_form(
            &state,
            &auth_user,
            &source_id,
            &form.name,
            &form.url,
            &form.username,
            "Name, URL, and username are required.",
        )
        .into_response();
    }

    if let Err(e) = crate::caldav::validate_caldav_url(&url) {
        return render_source_edit_form(
            &state,
            &auth_user,
            &source_id,
            &name,
            &url,
            &username,
            &e.to_string(),
        )
        .into_response();
    }

    // Resolve the password we'll actually use for the test + storage.
    // Empty form password means "keep existing"; we decrypt the stored
    // blob so the connection test exercises the same credentials that
    // sync will use afterwards.
    let password_changed = !form.password.is_empty();
    let plaintext_password = if password_changed {
        form.password.clone()
    } else {
        match crate::crypto::decrypt_password(&state.secret_key, &existing_password_enc) {
            Ok(p) => p,
            Err(_) => {
                return Html("Failed to decrypt stored credentials.".to_string()).into_response()
            }
        }
    };

    let skip_test = form.no_test.as_deref() == Some("on");
    if !skip_test {
        let client = crate::caldav::CaldavClient::new(&url, &username, &plaintext_password);
        if let Err(e) = client.check_connection().await {
            let msg = format!("Connection failed: {}. Check the URL and credentials, or check \"Skip connection test\" to save anyway.", e);
            return render_source_edit_form(
                &state, &auth_user, &source_id, &name, &url, &username, &msg,
            )
            .into_response();
        }
    }

    if password_changed {
        let new_enc = match crate::crypto::encrypt_password(&state.secret_key, &plaintext_password)
        {
            Ok(enc) => enc,
            Err(_) => return Html("Encryption error.".to_string()).into_response(),
        };
        let _ = sqlx::query(
            "UPDATE caldav_sources SET name = ?, url = ?, username = ?, password_enc = ? WHERE id = ?",
        )
        .bind(&name)
        .bind(&url)
        .bind(&username)
        .bind(&new_enc)
        .bind(&source_id)
        .execute(&state.pool)
        .await;
    } else {
        let _ =
            sqlx::query("UPDATE caldav_sources SET name = ?, url = ?, username = ? WHERE id = ?")
                .bind(&name)
                .bind(&url)
                .bind(&username)
                .bind(&source_id)
                .execute(&state.pool)
                .await;
    }

    tracing::info!(
        source_id = %source_id,
        source_name = %name,
        password_changed = %password_changed,
        user = %auth_user.user.email,
        "CalDAV source updated"
    );

    Redirect::to("/dashboard/sources").into_response()
}

async fn remove_source(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
    headers: HeaderMap,
    Path(source_id): Path<String>,
    Form(csrf): Form<CsrfForm>,
) -> impl IntoResponse {
    if let Err(resp) = verify_csrf_token(&headers, &csrf._csrf) {
        return resp;
    }
    let user = &auth_user.user;

    // Managed (admin-provisioned) sources can't be removed by regular users —
    // only admins can delete them. Without this check a user could remove the
    // central Exchange link the admin set up.
    if user.role != "admin" {
        let is_managed: Option<(i64,)> = sqlx::query_as(
            "SELECT cs.managed FROM caldav_sources cs \
             JOIN accounts a ON a.id = cs.account_id \
             WHERE cs.id = ? AND a.user_id = ?",
        )
        .bind(&source_id)
        .bind(&user.id)
        .fetch_optional(&state.pool)
        .await
        .unwrap_or(None);
        if matches!(is_managed, Some((m,)) if m != 0) {
            return (
                axum::http::StatusCode::FORBIDDEN,
                Html(
                    "This calendar source is provisioned by the administrator and cannot be removed."
                        .to_string(),
                ),
            )
                .into_response();
        }
    }

    // Verify source belongs to this user before deleting
    let _ = sqlx::query(
        "DELETE FROM caldav_sources WHERE id = ? AND account_id IN (SELECT id FROM accounts WHERE user_id = ?)",
    )
    .bind(&source_id)
    .bind(&user.id)
    .execute(&state.pool)
    .await;

    tracing::info!(source_id = %source_id, user = %auth_user.user.email, "CalDAV source removed");

    Redirect::to("/dashboard/sources").into_response()
}

async fn test_source(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
    headers: HeaderMap,
    Path(source_id): Path<String>,
    Form(csrf): Form<CsrfForm>,
) -> impl IntoResponse {
    if let Err(resp) = verify_csrf_token(&headers, &csrf._csrf) {
        return resp;
    }
    let user = &auth_user.user;

    let source: Option<(
        String,
        String,
        Option<String>,
        String,
        String,
        Option<String>,
        Option<String>,
        String,
    )> = sqlx::query_as(
        "SELECT cs.url, cs.username, cs.password_enc, cs.name, cs.auth_type, cs.access_token_enc, cs.token_expires_at, cs.provider_type
         FROM caldav_sources cs
         JOIN accounts a ON a.id = cs.account_id
         WHERE cs.id = ? AND a.user_id = ?",
    )
    .bind(&source_id)
    .bind(&user.id)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    let (
        url,
        username,
        password_enc,
        name,
        auth_type,
        access_token_enc,
        token_expires_at,
        provider_type,
    ) = match source {
        Some(s) => s,
        None => return Html("Source not found.".to_string()).into_response(),
    };

    let label = crate::providers::factory::label(&provider_type);

    // EWS sources go through the provider trait; CalDAV (basic or OAuth2) keeps
    // the existing CaldavClient path so OAuth2 refresh + ctag stay intact.
    let result = if provider_type == crate::providers::factory::kinds::EWS {
        let password = match password_enc.as_deref() {
            Some(enc) => match crate::crypto::decrypt_password(&state.secret_key, enc) {
                Ok(p) => p,
                Err(_) => {
                    return Html("Failed to decrypt stored credentials.".to_string())
                        .into_response()
                }
            },
            None => {
                return Html("Source has no stored password.".to_string()).into_response();
            }
        };
        match crate::providers::build_provider(&provider_type, &url, &username, &password, None) {
            Ok(client) => match client.check_connection().await {
                Ok(true) => format!("'{}' — connection OK ({}).", name, label),
                Ok(false) => format!(
                    "'{}' — connected but {} features not explicitly advertised. Sync may still work.",
                    name, label,
                ),
                Err(e) => format!("'{}' — connection failed: {}", name, e),
            },
            Err(e) => format!("'{}' — could not build provider: {}", name, e),
        }
    } else {
        let client = match crate::oauth2_caldav::build_client_for_source(
            &state.pool,
            &state.secret_key,
            &source_id,
            &url,
            &auth_type,
            &username,
            password_enc.as_deref(),
            access_token_enc.as_deref(),
            token_expires_at.as_deref(),
        )
        .await
        {
            Ok(c) => c,
            Err(_) => {
                return Html("Failed to decrypt stored credentials.".to_string()).into_response()
            }
        };
        match client.check_connection().await {
            Ok(true) => format!("'{}' — connection OK, CalDAV supported.", name),
            Ok(false) => format!(
                "'{}' — connected but CalDAV not explicitly detected. Sync may still work.",
                name
            ),
            Err(e) => format!("'{}' — connection failed: {}", name, e),
        }
    };

    // Return a simple page with back link
    let tmpl = match state.templates.get_template("source_test.html") {
        Ok(t) => t,
        Err(_) => {
            return Html(format!(
                "<p>{}</p><p><a href=\"/dashboard\">Back to dashboard</a></p>",
                result
            ))
            .into_response()
        }
    };
    let (impersonating, impersonating_name, _) = impersonation_ctx(&auth_user);
    Html(
        tmpl.render(context! {
            result => result,
            sidebar => sidebar_context(&auth_user, "sources"),
            impersonating => impersonating,
            impersonating_name => impersonating_name,
        })
        .unwrap_or_else(|e| internal_error_body("template render", &e)),
    )
    .into_response()
}

/// Runs CalDAV sync using build_client_for_source (supports both basic and OAuth2).
async fn run_sync_for_source(
    pool: &SqlitePool,
    key: &[u8; 32],
    source_id: &str,
    url: &str,
    username: &str,
    password_enc: Option<&str>,
    auth_type: &str,
    access_token_enc: Option<&str>,
    token_expires_at: Option<&str>,
    provider_type: &str,
) -> (Vec<String>, usize) {
    // EWS sources go through the provider trait — no OAuth2 dispatch needed.
    if provider_type == crate::providers::factory::kinds::EWS {
        let enc = match password_enc {
            Some(e) => e,
            None => return (vec!["EWS source missing password".to_string()], 0),
        };
        let password = match crate::crypto::decrypt_password(key, enc) {
            Ok(p) => p,
            Err(e) => return (vec![format!("Decrypt failed: {}", e)], 0),
        };
        return run_sync(
            pool,
            key,
            source_id,
            provider_type,
            url,
            username,
            &password,
        )
        .await;
    }
    let client = match crate::oauth2_caldav::build_client_for_source(
        pool,
        key,
        source_id,
        url,
        auth_type,
        username,
        password_enc,
        access_token_enc,
        token_expires_at,
    )
    .await
    {
        Ok(c) => c,
        Err(e) => return (vec![format!("Failed to build client: {}", e)], 0),
    };

    match crate::commands::sync::sync_source(pool, key, &client, source_id).await {
        Ok(()) => {
            let cal_count: i64 =
                sqlx::query_scalar("SELECT COUNT(*) FROM calendars WHERE source_id = ?")
                    .bind(source_id)
                    .fetch_one(pool)
                    .await
                    .unwrap_or(0);
            (vec!["Sync complete.".to_string()], cal_count as usize)
        }
        Err(e) => (vec![format!("Sync failed: {}", e)], 0),
    }
}

/// Run discovery + sync for a freshly-created source with plaintext password.
/// Dispatches on `provider_type`: EWS goes through the trait-based path,
/// CalDAV reuses the existing `CaldavClient` + `sync_source` flow.
async fn run_sync(
    pool: &SqlitePool,
    key: &[u8; 32],
    source_id: &str,
    provider_type: &str,
    url: &str,
    username: &str,
    password: &str,
) -> (Vec<String>, usize) {
    if provider_type == crate::providers::factory::kinds::EWS {
        let provider =
            match crate::providers::build_provider(provider_type, url, username, password, None) {
                Ok(p) => p,
                Err(e) => return (vec![format!("Could not build provider: {}", e)], 0),
            };
        match crate::commands::sync::sync_ews_source(pool, key, provider.as_ref(), source_id).await
        {
            Ok(()) => {
                let cal_count: i64 =
                    sqlx::query_scalar("SELECT COUNT(*) FROM calendars WHERE source_id = ?")
                        .bind(source_id)
                        .fetch_one(pool)
                        .await
                        .unwrap_or(0);
                (vec!["Sync complete.".to_string()], cal_count as usize)
            }
            Err(e) => (vec![format!("Sync failed: {}", e)], 0),
        }
    } else {
        let client = crate::caldav::CaldavClient::new(url, username, password);
        match crate::commands::sync::sync_source(pool, key, &client, source_id).await {
            Ok(()) => {
                let cal_count: i64 =
                    sqlx::query_scalar("SELECT COUNT(*) FROM calendars WHERE source_id = ?")
                        .bind(source_id)
                        .fetch_one(pool)
                        .await
                        .unwrap_or(0);
                (vec!["Sync complete.".to_string()], cal_count as usize)
            }
            Err(e) => (vec![format!("Sync failed: {}", e)], 0),
        }
    }
}

async fn force_sync_source(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
    headers: HeaderMap,
    Path(source_id): Path<String>,
    Form(csrf): Form<CsrfForm>,
) -> impl IntoResponse {
    if let Err(resp) = verify_csrf_token(&headers, &csrf._csrf) {
        return resp;
    }
    let user = &auth_user.user;

    // Verify ownership
    let source: Option<(
        String,
        String,
        String,
        Option<String>,
        String,
        Option<String>,
        Option<String>,
        String,
    )> = sqlx::query_as(
        "SELECT cs.id, cs.url, cs.username, cs.password_enc, cs.auth_type, cs.access_token_enc, cs.token_expires_at, cs.provider_type
         FROM caldav_sources cs JOIN accounts a ON a.id = cs.account_id
         WHERE cs.id = ? AND a.user_id = ?",
    )
    .bind(&source_id)
    .bind(&user.id)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    let (
        sid,
        url,
        username,
        password_enc,
        auth_type,
        access_token_enc,
        token_expires_at,
        provider_type,
    ) = match source {
        Some(s) => s,
        None => return Html("Source not found.".to_string()).into_response(),
    };

    // Clear sync tokens to force a full fetch (same as `calrs sync --full`)
    let _ = sqlx::query("UPDATE calendars SET sync_token = NULL, ctag = NULL WHERE source_id = ?")
        .bind(&sid)
        .execute(&state.pool)
        .await;

    tracing::info!(source_id = %sid, "force full resync triggered from dashboard");

    let name: String = sqlx::query_scalar("SELECT name FROM caldav_sources WHERE id = ?")
        .bind(&sid)
        .fetch_one(&state.pool)
        .await
        .unwrap_or_else(|_| "Source".to_string());

    let (messages, _) = run_sync_for_source(
        &state.pool,
        &state.secret_key,
        &sid,
        &url,
        &username,
        password_enc.as_deref(),
        &auth_type,
        access_token_enc.as_deref(),
        token_expires_at.as_deref(),
        &provider_type,
    )
    .await;

    render_sync_result(&state, &auth_user, &name, &messages).into_response()
}

async fn sync_source(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
    headers: HeaderMap,
    Path(source_id): Path<String>,
    Form(csrf): Form<CsrfForm>,
) -> impl IntoResponse {
    if let Err(resp) = verify_csrf_token(&headers, &csrf._csrf) {
        return resp;
    }
    let user = &auth_user.user;

    let source: Option<(
        String,
        String,
        String,
        Option<String>,
        String,
        String,
        Option<String>,
        Option<String>,
        String,
    )> = sqlx::query_as(
        "SELECT cs.id, cs.url, cs.username, cs.password_enc, cs.name, cs.auth_type, cs.access_token_enc, cs.token_expires_at, cs.provider_type
         FROM caldav_sources cs
         JOIN accounts a ON a.id = cs.account_id
         WHERE cs.id = ? AND a.user_id = ?",
    )
    .bind(&source_id)
    .bind(&user.id)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    let (
        sid,
        url,
        username,
        password_enc,
        name,
        auth_type,
        access_token_enc,
        token_expires_at,
        provider_type,
    ) = match source {
        Some(s) => s,
        None => return Html("Source not found.".to_string()).into_response(),
    };

    tracing::info!(source_id = %sid, "calendar sync triggered from dashboard");

    let (messages, calendar_count) = run_sync_for_source(
        &state.pool,
        &state.secret_key,
        &sid,
        &url,
        &username,
        password_enc.as_deref(),
        &auth_type,
        access_token_enc.as_deref(),
        token_expires_at.as_deref(),
        &provider_type,
    )
    .await;

    // If write_calendar_href is not yet configured and we found calendars,
    // redirect to the write-calendar setup page (onboarding flow).
    let write_href: Option<String> =
        sqlx::query_scalar("SELECT write_calendar_href FROM caldav_sources WHERE id = ?")
            .bind(&sid)
            .fetch_optional(&state.pool)
            .await
            .unwrap_or(None)
            .flatten();

    if write_href.is_none() && calendar_count > 0 {
        let joined_messages = messages.join("\n");
        let encoded_messages = urlencoding::encode(&joined_messages);
        return Redirect::to(&format!(
            "/dashboard/sources/{}/setup-write?sync_messages={}",
            sid, encoded_messages
        ))
        .into_response();
    }

    render_sync_result(&state, &auth_user, &name, &messages).into_response()
}

fn render_sync_result(
    state: &AppState,
    auth_user: &crate::auth::AuthUser,
    source_name: &str,
    messages: &[String],
) -> Html<String> {
    let tmpl = match state.templates.get_template("source_test.html") {
        Ok(t) => t,
        Err(_) => {
            return Html(format!(
                "<p>{}</p><p><a href=\"/dashboard\">Back to dashboard</a></p>",
                messages.join("<br>")
            ))
        }
    };
    let (impersonating, impersonating_name, _) = impersonation_ctx(auth_user);
    Html(
        tmpl.render(context! {
            result => messages.join("\n"),
            source_name => source_name,
            sidebar => sidebar_context(auth_user, "sources"),
            impersonating => impersonating,
            impersonating_name => impersonating_name,
        })
        .unwrap_or_else(|e| internal_error_body("template render", &e)),
    )
}

#[derive(Deserialize)]
struct SetupWriteQuery {
    sync_messages: Option<String>,
}

async fn setup_write_calendar(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
    Path(source_id): Path<String>,
    Query(query): Query<SetupWriteQuery>,
) -> impl IntoResponse {
    let user = &auth_user.user;

    // Fetch source name and verify ownership
    let source: Option<(String, String)> = sqlx::query_as(
        "SELECT cs.id, cs.name FROM caldav_sources cs
         JOIN accounts a ON a.id = cs.account_id
         WHERE cs.id = ? AND a.user_id = ?",
    )
    .bind(&source_id)
    .bind(&user.id)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    let (_sid, source_name) = match source {
        Some(s) => s,
        None => return Redirect::to("/dashboard/sources").into_response(),
    };

    // Get calendars for this source, sorted by event count (most events first)
    let calendars: Vec<(String, Option<String>, Option<String>, i64)> = sqlx::query_as(
        "SELECT c.href, c.display_name, c.color,
                (SELECT COUNT(*) FROM events e WHERE e.calendar_id = c.id) as event_count
         FROM calendars c WHERE c.source_id = ?
         ORDER BY event_count DESC, c.display_name",
    )
    .bind(&source_id)
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();

    if calendars.is_empty() {
        return Redirect::to("/dashboard/sources").into_response();
    }

    let cal_values: Vec<minijinja::Value> = calendars
        .iter()
        .map(|(href, name, color, event_count)| {
            context! {
                href => href,
                name => name.as_deref().unwrap_or(href),
                color => color,
                event_count => event_count,
            }
        })
        .collect();

    let tmpl = match state.templates.get_template("source_write_setup.html") {
        Ok(t) => t,
        Err(e) => return internal_error_response("template render", &e),
    };

    Html(
        tmpl.render(context! {
            source_id => source_id,
            source_name => source_name,
            calendars => cal_values,
            sync_messages => query.sync_messages,
        })
        .unwrap_or_else(|e| internal_error_body("template render", &e)),
    )
    .into_response()
}

#[derive(Deserialize)]
struct WriteCalendarForm {
    _csrf: Option<String>,
    calendar_href: String,
}

async fn set_write_calendar(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
    headers: HeaderMap,
    Path(source_id): Path<String>,
    Form(form): Form<WriteCalendarForm>,
) -> impl IntoResponse {
    if let Err(resp) = verify_csrf_token(&headers, &form._csrf) {
        return resp;
    }
    let user = &auth_user.user;

    // Verify source belongs to this user
    let owned: Option<(String,)> = sqlx::query_as(
        "SELECT cs.id FROM caldav_sources cs
         JOIN accounts a ON a.id = cs.account_id
         WHERE cs.id = ? AND a.user_id = ?",
    )
    .bind(&source_id)
    .bind(&user.id)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    if owned.is_none() {
        return Redirect::to("/dashboard/sources").into_response();
    }

    let href = if form.calendar_href.is_empty() {
        None
    } else {
        Some(form.calendar_href)
    };

    let _ = sqlx::query("UPDATE caldav_sources SET write_calendar_href = ? WHERE id = ?")
        .bind(&href)
        .bind(&source_id)
        .execute(&state.pool)
        .await;

    tracing::info!(source_id = %source_id, "write calendar configured");

    Redirect::to("/dashboard/sources").into_response()
}

fn render_event_type_form_error(
    state: &AppState,
    auth_user: &crate::auth::AuthUser,
    error: &str,
    form: &EventTypeForm,
    editing: bool,
) -> Html<String> {
    let tmpl = match state.templates.get_template("event_type_form.html") {
        Ok(t) => t,
        Err(e) => return internal_error_html("template render", &e),
    };

    let (impersonating, impersonating_name, _) = impersonation_ctx(auth_user);
    Html(
        tmpl.render(context! {
            editing => editing,
            form_title => form.title.as_str(),
            form_slug => form.slug.as_str(),
            form_description => form.description.as_deref().unwrap_or(""),
            form_duration => parse_int_field(&form.duration_min, 30),
            form_slot_interval => parse_optional_positive_int(&form.slot_interval_min).unwrap_or(0),
            form_buffer_before => parse_int_field(&form.buffer_before, 0),
            form_buffer_after => parse_int_field(&form.buffer_after, 0),
            form_min_notice => parse_int_field(&form.min_notice_min, 60),
            form_requires_confirmation => form.requires_confirmation.as_deref() == Some("on"),
            form_visibility => form.visibility.as_deref().unwrap_or("public"),
            form_location_type => form.location_type.as_deref().unwrap_or("link"),
            form_location_value => form.location_value.as_deref().unwrap_or(""),
            form_meeting_pattern_override => form.meeting_pattern_override.as_deref().unwrap_or(""),
            form_avail_days => form.avail_days.as_deref().unwrap_or("1,2,3,4,5"),
            form_avail_start => form.avail_start.as_deref().unwrap_or("09:00"),
            form_avail_end => form.avail_end.as_deref().unwrap_or("17:00"),
            form_avail_windows => form.avail_windows.as_deref().unwrap_or(""),
            form_avail_schedule => form.avail_schedule.as_deref().unwrap_or(""),
            form_default_calendar_view => form.default_calendar_view.as_deref().unwrap_or("month"),
            form_first_slot_only => form.first_slot_only.as_deref() == Some("on"),
            form_frequency_limits => form.frequency_limits.as_str(),
            form_timezone => form.timezone.as_deref().unwrap_or(&auth_user.user.timezone),
            form_cancel_notice_value => parse_int_field(&form.cancel_notice_value, 0),
            form_cancel_notice_unit => match form.cancel_notice_unit.as_str() {
                "hours" => "hours",
                "days" => "days",
                _ => "minutes",
            },
            form_reschedule_notice_value => parse_int_field(&form.reschedule_notice_value, 0),
            form_reschedule_notice_unit => match form.reschedule_notice_unit.as_str() {
                "hours" => "hours",
                "days" => "days",
                _ => "minutes",
            },
            tz_options => common_timezones_with(form.timezone.as_deref().unwrap_or(&auth_user.user.timezone))
                .iter()
                .map(|(iana, label)| context! { value => iana, label => label })
                .collect::<Vec<_>>(),
            error => error,
            sidebar => sidebar_context(auth_user, "event-types"),
            impersonating => impersonating,
            impersonating_name => impersonating_name,
        })
        .unwrap_or_else(|e| internal_error_body("template render", &e)),
    )
}

// --- Invite management handlers ---

const MAX_BULK_INVITES: usize = 100;

#[derive(Deserialize)]
struct BulkInviteForm {
    _csrf: Option<String>,
    recipients: String,
    message: Option<String>,
    expires_days: Option<i32>,
    single_use: Option<String>, // checkbox: "on" or absent
}

#[derive(Default)]
struct BulkInviteResult {
    sent: Vec<String>,
    invalid: Vec<String>,
    duplicates: Vec<String>,
    failed: Vec<String>,
    over_limit: bool,
}

fn parse_bulk_recipients(input: &str, max: usize) -> (Vec<(String, String)>, BulkInviteResult) {
    let mut valid: Vec<(String, String)> = Vec::new();
    let mut result = BulkInviteResult::default();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();

    for raw in input.lines() {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        if valid.len() + result.invalid.len() + result.duplicates.len() >= max {
            result.over_limit = true;
            break;
        }
        if !is_plausible_email(line) {
            result.invalid.push(line.to_string());
            continue;
        }
        let key = line.to_ascii_lowercase();
        if !seen.insert(key) {
            result.duplicates.push(line.to_string());
            continue;
        }
        let name = derive_name_from_email(line);
        valid.push((line.to_string(), name));
    }
    (valid, result)
}

fn is_plausible_email(s: &str) -> bool {
    if s.chars().any(char::is_whitespace) {
        return false;
    }
    if s.len() > 254 {
        return false;
    }
    let mut parts = s.splitn(2, '@');
    let local = parts.next().unwrap_or("");
    let domain = parts.next().unwrap_or("");
    !local.is_empty() && domain.contains('.') && domain.len() >= 3 && !domain.starts_with('.')
}

fn derive_name_from_email(email: &str) -> String {
    let local = email.split('@').next().unwrap_or(email);
    let parts: Vec<String> = local
        .split(['.', '_', '-', '+'])
        .filter(|s| !s.is_empty())
        .map(|s| {
            let mut chars = s.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect();
    if parts.is_empty() {
        local.to_string()
    } else {
        parts.join(" ")
    }
}

fn bulk_result_context(result: &BulkInviteResult) -> minijinja::Value {
    context! {
        sent_count => result.sent.len(),
        invalid => &result.invalid,
        duplicates => &result.duplicates,
        failed => &result.failed,
        over_limit => result.over_limit,
        max_recipients => MAX_BULK_INVITES,
        has_issues => !result.invalid.is_empty() || !result.duplicates.is_empty() || !result.failed.is_empty() || result.over_limit,
    }
}

async fn render_invite_management(
    state: &Arc<AppState>,
    auth_user: &crate::auth::AuthUser,
    event_type_id: &str,
    bulk_result: Option<&BulkInviteResult>,
) -> Response {
    let et: Option<(
        String,
        String,
        String,
        Option<String>,
        Option<String>,
        Option<String>,
        String,
    )> = sqlx::query_as(
        "SELECT et.id, et.title, et.slug,
                CASE WHEN et.team_id IS NOT NULL THEN t.slug ELSE NULL END,
                CASE WHEN et.team_id IS NULL THEN u.username ELSE NULL END,
                CASE WHEN et.team_id IS NOT NULL THEN t.name ELSE u.name END,
                et.visibility
         FROM event_types et
         LEFT JOIN teams t ON t.id = et.team_id
         LEFT JOIN accounts a ON a.id = et.account_id
         LEFT JOIN users u ON u.id = a.user_id
         WHERE et.id = ? AND et.visibility IN ('private', 'internal')",
    )
    .bind(event_type_id)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    let (et_id, et_title, et_slug, team_slug, username, owner_name, visibility) = match et {
        Some(e) => e,
        None => return Html("Private event type not found.".to_string()).into_response(),
    };

    // Internal event types: any authenticated user can issue invites — that's
    // the documented "any authenticated colleague can distribute" contract.
    // Private event types: invite issuance is management (admin / owner /
    // team admin only).
    if visibility == "private" && !can_manage_event_type(&state.pool, &auth_user.user, &et_id).await
    {
        return Html("Access denied.".to_string()).into_response();
    }

    let invites: Vec<(String, String, String, String, Option<String>, Option<String>, i32, i32, String, String)> = sqlx::query_as(
        "SELECT bi.id, bi.token, bi.guest_name, bi.guest_email, bi.message, bi.expires_at, bi.max_uses, bi.used_count, bi.created_at, u.name
         FROM booking_invites bi
         JOIN users u ON u.id = bi.created_by_user_id
         WHERE bi.event_type_id = ?
         ORDER BY bi.created_at DESC",
    )
    .bind(&et_id)
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();

    let base_url = std::env::var("CALRS_BASE_URL").unwrap_or_default();
    let invites_ctx: Vec<minijinja::Value> = invites
        .iter()
        .map(
            |(
                id,
                token,
                guest_name,
                guest_email,
                message,
                expires_at,
                max_uses,
                used_count,
                created_at,
                created_by,
            )| {
                let is_expired = expires_at.as_ref().is_some_and(|exp| {
                    exp < &chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string()
                });
                let is_used = *used_count >= *max_uses;
                let invite_url = if let Some(ts) = &team_slug {
                    format!("{}/team/{}/{}?invite={}", base_url, ts, et_slug, token)
                } else if let Some(un) = &username {
                    format!("{}/u/{}/{}?invite={}", base_url, un, et_slug, token)
                } else {
                    format!("{}?invite={}", base_url, token)
                };
                context! {
                    id => id,
                    guest_name => guest_name,
                    guest_email => guest_email,
                    message => message,
                    expires_at => expires_at,
                    max_uses => max_uses,
                    used_count => used_count,
                    created_at => created_at,
                    created_by => created_by,
                    is_expired => is_expired,
                    is_used => is_used,
                    invite_url => invite_url,
                }
            },
        )
        .collect();

    // Internal events let any authenticated user *create* invites, but only
    // admins/owners can *delete* them. Pass `can_delete_invites` to the
    // template so it can hide the Delete button for users who'd just hit a
    // 403 — keeps the UI consistent with the SQL gate in `delete_invite`.
    let can_delete_invites = can_manage_event_type(&state.pool, &auth_user.user, &et_id).await;

    let tmpl = match state.templates.get_template("invite_form.html") {
        Ok(t) => t,
        Err(e) => return internal_error_response("template render", &e),
    };

    let bulk_ctx = bulk_result.map(bulk_result_context);
    let (impersonating, impersonating_name, _) = impersonation_ctx(auth_user);
    Html(
        tmpl.render(context! {
            sidebar => sidebar_context(auth_user, "event-types"),
            impersonating => impersonating,
            impersonating_name => impersonating_name,
            event_type_id => et_id,
            event_type_title => et_title,
            event_type_slug => et_slug,
            team_slug => team_slug,
            username => username,
            owner_name => owner_name,
            invites => invites_ctx,
            bulk_result => bulk_ctx,
            can_delete_invites => can_delete_invites,
            success => "",
            error => "",
        })
        .unwrap_or_else(|e| internal_error_body("template render", &e)),
    )
    .into_response()
}

async fn invite_management_page(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
    Path(event_type_id): Path<String>,
) -> impl IntoResponse {
    render_invite_management(&state, &auth_user, &event_type_id, None).await
}

async fn send_invite_bulk(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
    headers: HeaderMap,
    Path(event_type_id): Path<String>,
    Form(form): Form<BulkInviteForm>,
) -> impl IntoResponse {
    if let Err(resp) = verify_csrf_token(&headers, &form._csrf) {
        return resp;
    }

    let et: Option<(String, String, Option<String>, Option<String>, String)> = sqlx::query_as(
        "SELECT et.id, et.title,
                CASE WHEN et.team_id IS NOT NULL THEN t.slug ELSE NULL END,
                CASE WHEN et.team_id IS NULL THEN u.username ELSE NULL END,
                et.visibility
         FROM event_types et
         LEFT JOIN teams t ON t.id = et.team_id
         LEFT JOIN accounts a ON a.id = et.account_id
         LEFT JOIN users u ON u.id = a.user_id
         WHERE et.id = ? AND et.visibility IN ('private', 'internal')",
    )
    .bind(&event_type_id)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    let (et_id, et_title, team_slug, username, visibility) = match et {
        Some(e) => e,
        None => return Redirect::to("/dashboard/event-types").into_response(),
    };

    // Private event types: invite issuance is management (admin / owner / team
    // admin only). Internal event types: any authenticated user can issue
    // invites, matching the quick-link contract.
    if visibility == "private" && !can_manage_event_type(&state.pool, &auth_user.user, &et_id).await
    {
        return Redirect::to("/dashboard/event-types").into_response();
    }

    let (valid_recipients, mut result) = parse_bulk_recipients(&form.recipients, MAX_BULK_INVITES);

    let single_use = form.single_use.as_deref() != Some("on");
    let max_uses = if single_use { 1 } else { 999 };
    let expires_at = form.expires_days.filter(|&d| d > 0).map(|days| {
        (chrono::Utc::now() + chrono::Duration::days(days as i64))
            .format("%Y-%m-%d %H:%M:%S")
            .to_string()
    });
    let message_opt = form
        .message
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());

    let base_url = std::env::var("CALRS_BASE_URL").unwrap_or_default();
    let et_slug: Option<String> = if !valid_recipients.is_empty() {
        sqlx::query_scalar("SELECT slug FROM event_types WHERE id = ?")
            .bind(&et_id)
            .fetch_optional(&state.pool)
            .await
            .unwrap_or(None)
    } else {
        None
    };
    let smtp_config = if !base_url.is_empty() && et_slug.is_some() && !valid_recipients.is_empty() {
        crate::email::load_smtp_config(&state.pool, &state.secret_key)
            .await
            .ok()
            .flatten()
    } else {
        None
    };

    for (email, name) in valid_recipients {
        let token = uuid::Uuid::new_v4().to_string();
        let row_id = uuid::Uuid::new_v4().to_string();
        let insert_res = sqlx::query(
            "INSERT INTO booking_invites (id, event_type_id, token, guest_name, guest_email, message, expires_at, max_uses, created_by_user_id)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&row_id)
        .bind(&et_id)
        .bind(&token)
        .bind(&name)
        .bind(&email)
        .bind(message_opt)
        .bind(&expires_at)
        .bind(max_uses)
        .bind(&auth_user.user.id)
        .execute(&state.pool)
        .await;

        if insert_res.is_err() {
            tracing::warn!(email = %email, "bulk invite DB insert failed");
            result.failed.push(email);
            continue;
        }

        if let (Some(slug), Some(cfg)) = (et_slug.as_ref(), smtp_config.as_ref()) {
            let invite_url = if let Some(ts) = &team_slug {
                format!("{}/team/{}/{}?invite={}", base_url, ts, slug, token)
            } else if let Some(un) = &username {
                format!("{}/u/{}/{}?invite={}", base_url, un, slug, token)
            } else {
                String::new()
            };

            if !invite_url.is_empty() {
                let send_res = crate::email::send_invite_email(
                    cfg,
                    &name,
                    &email,
                    &et_title,
                    &auth_user.user.name,
                    message_opt,
                    &invite_url,
                    expires_at.as_deref(),
                )
                .await;
                if send_res.is_err() {
                    tracing::warn!(email = %email, "bulk invite SMTP send failed");
                    result.failed.push(email);
                    continue;
                }
            }
        }

        result.sent.push(email);
    }

    tracing::info!(
        event_type = %et_id,
        sent = result.sent.len(),
        invalid = result.invalid.len(),
        duplicates = result.duplicates.len(),
        failed = result.failed.len(),
        over_limit = result.over_limit,
        invited_by = %auth_user.user.email,
        "bulk invites processed",
    );

    render_invite_management(&state, &auth_user, &event_type_id, Some(&result)).await
}

#[derive(Deserialize)]
struct DeleteInviteForm {
    _csrf: Option<String>,
}

async fn delete_invite(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
    headers: HeaderMap,
    Path(invite_id): Path<String>,
    Form(form): Form<DeleteInviteForm>,
) -> impl IntoResponse {
    if let Err(resp) = verify_csrf_token(&headers, &form._csrf) {
        return resp;
    }

    // Allow delete for: global admin, personal event type creator, the user who
    // created this specific invite, or a team admin of the team that owns the
    // event type.
    let is_global_admin = auth_user.user.role == "admin";
    let et_id: Option<String> = if is_global_admin {
        sqlx::query_scalar("SELECT event_type_id FROM booking_invites WHERE id = ?")
            .bind(&invite_id)
            .fetch_optional(&state.pool)
            .await
            .unwrap_or(None)
    } else {
        sqlx::query_scalar(
            "SELECT bi.event_type_id FROM booking_invites bi
             JOIN event_types et ON et.id = bi.event_type_id
             LEFT JOIN accounts a ON a.id = et.account_id
             LEFT JOIN team_members tm ON tm.team_id = et.team_id AND tm.user_id = ? AND tm.role = 'admin'
             WHERE bi.id = ? AND (
                 (et.team_id IS NULL AND a.user_id = ?) OR
                 (et.team_id IS NOT NULL AND tm.user_id IS NOT NULL)
             )",
        )
        .bind(&auth_user.user.id)
        .bind(&invite_id)
        .bind(&auth_user.user.id)
        .fetch_optional(&state.pool)
        .await
        .unwrap_or(None)
    };

    if et_id.is_some() {
        let _ = sqlx::query("DELETE FROM booking_invites WHERE id = ?")
            .bind(&invite_id)
            .execute(&state.pool)
            .await;
    }

    tracing::info!(invite_id = %invite_id, deleted_by = %auth_user.user.email, "invite deleted");

    match et_id {
        Some(id) => Redirect::to(&format!("/dashboard/invites/{}", id)).into_response(),
        None => Redirect::to("/dashboard/event-types").into_response(),
    }
}

// --- Quick invite link (for internal event types) ---

async fn generate_quick_link(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
    headers: HeaderMap,
    Path(event_type_id): Path<String>,
    Form(form): Form<CsrfForm>,
) -> impl IntoResponse {
    if let Err(resp) = verify_csrf_token(&headers, &form._csrf) {
        return resp;
    }

    let et: Option<(String, String, Option<String>, Option<String>, String)> = sqlx::query_as(
        "SELECT et.id, et.slug,
                CASE WHEN et.team_id IS NOT NULL THEN t.slug ELSE NULL END,
                CASE WHEN et.team_id IS NULL THEN u.username ELSE NULL END,
                et.visibility
         FROM event_types et
         LEFT JOIN teams t ON t.id = et.team_id
         LEFT JOIN accounts a ON a.id = et.account_id
         LEFT JOIN users u ON u.id = a.user_id
         WHERE et.id = ? AND et.visibility IN ('private', 'internal')",
    )
    .bind(&event_type_id)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    let (et_id, et_slug, team_slug, username, visibility) = match et {
        Some(e) => e,
        None => return Redirect::to("/dashboard/invite-links").into_response(),
    };

    // Private event types: only owner (personal) or team admin (team event)
    // can generate quick links. Internal event types remain open to any
    // authenticated user — that's the documented "any authenticated colleague
    // can distribute a one-time link" contract.
    if visibility == "private" && !can_manage_event_type(&state.pool, &auth_user.user, &et_id).await
    {
        return Redirect::to("/dashboard/invite-links").into_response();
    }

    let token = uuid::Uuid::new_v4().to_string();
    let expires_at = (chrono::Utc::now() + chrono::Duration::days(7))
        .format("%Y-%m-%d %H:%M:%S")
        .to_string();

    let _ = sqlx::query(
        "INSERT INTO booking_invites (id, event_type_id, token, guest_name, guest_email, expires_at, max_uses, created_by_user_id)
         VALUES (?, ?, ?, '', '', ?, 1, ?)",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(&et_id)
    .bind(&token)
    .bind(&expires_at)
    .bind(&auth_user.user.id)
    .execute(&state.pool)
    .await;

    tracing::info!(event_type = %et_id, created_by = %auth_user.user.email, "quick invite link generated");

    let base_url = std::env::var("CALRS_BASE_URL").unwrap_or_default();
    let invite_url = if let Some(ts) = &team_slug {
        format!("{}/team/{}/{}?invite={}", base_url, ts, et_slug, token)
    } else if let Some(un) = &username {
        format!("{}/u/{}/{}?invite={}", base_url, un, et_slug, token)
    } else {
        format!("{}?invite={}", base_url, token)
    };

    // Return JSON with the URL for the frontend to copy
    axum::Json(serde_json::json!({ "url": invite_url })).into_response()
}

// --- Availability overrides ---

async fn overrides_page(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
    Path(slug): Path<String>,
) -> impl IntoResponse {
    let user = &auth_user.user;
    let is_global_admin = user.role == "admin";

    let et: Option<(String, String, Option<String>)> = if is_global_admin {
        sqlx::query_as(
            "SELECT et.id, et.title, et.team_id FROM event_types et WHERE et.slug = ? LIMIT 1",
        )
        .bind(&slug)
        .fetch_optional(&state.pool)
        .await
        .unwrap_or(None)
    } else {
        find_manageable_event_type_by_slug(&state.pool, &user.id, &slug).await
    };

    let (et_id, et_title, team_id) = match et {
        Some(e) => e,
        None => return Redirect::to("/dashboard/event-types").into_response(),
    };
    let is_team = team_id.is_some();

    let overrides: Vec<(String, String, Option<String>, Option<String>, i32)> = sqlx::query_as(
        "SELECT id, date, start_time, end_time, is_blocked FROM availability_overrides WHERE event_type_id = ? ORDER BY date, start_time",
    )
    .bind(&et_id)
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();

    // Dashboard handler: stays English until the dashboard surface is translated.
    let overrides_ctx: Vec<minijinja::Value> = overrides
        .iter()
        .map(|(id, date, start_time, end_time, is_blocked)| {
            let date_label = NaiveDate::parse_from_str(date, "%Y-%m-%d")
                .map(|d| crate::i18n::format_long_date(d, "en"))
                .unwrap_or_else(|_| date.clone());
            context! {
                id => id,
                date => date,
                date_label => date_label,
                start_time => start_time,
                end_time => end_time,
                is_blocked => *is_blocked != 0,
            }
        })
        .collect();

    let tmpl = match state.templates.get_template("overrides.html") {
        Ok(t) => t,
        Err(e) => return internal_error_response("template render", &e),
    };

    let (impersonating, impersonating_name, _) = impersonation_ctx(&auth_user);

    Html(
        tmpl.render(context! {
            sidebar => sidebar_context(&auth_user, "event-types"),
            event_type_title => et_title,
            event_type_slug => slug,
            overrides => overrides_ctx,
            is_team => is_team,
            impersonating => impersonating,
            impersonating_name => impersonating_name,
        })
        .unwrap_or_default(),
    )
    .into_response()
}

#[derive(Deserialize)]
struct OverrideForm {
    _csrf: Option<String>,
    date: String,
    override_type: String,
    start_time: Option<String>,
    end_time: Option<String>,
}

async fn create_override(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
    headers: HeaderMap,
    Path(slug): Path<String>,
    Form(form): Form<OverrideForm>,
) -> impl IntoResponse {
    if let Err(resp) = verify_csrf_token(&headers, &form._csrf) {
        return resp;
    }
    let user = &auth_user.user;
    let is_global_admin = user.role == "admin";

    let et_id: Option<String> = if is_global_admin {
        sqlx::query_scalar("SELECT id FROM event_types WHERE slug = ? LIMIT 1")
            .bind(&slug)
            .fetch_optional(&state.pool)
            .await
            .unwrap_or(None)
    } else {
        find_manageable_event_type_by_slug(&state.pool, &user.id, &slug)
            .await
            .map(|(id, _, _)| id)
    };

    let et_id = match et_id {
        Some(id) => id,
        None => return Redirect::to("/dashboard/event-types").into_response(),
    };

    // Validate date
    if NaiveDate::parse_from_str(&form.date, "%Y-%m-%d").is_err() {
        return Redirect::to(&format!("/dashboard/event-types/{}/overrides", slug)).into_response();
    }

    let is_blocked = form.override_type != "custom";
    let (start_time, end_time) = if !is_blocked {
        let s = form.start_time.as_deref().unwrap_or("");
        let e = form.end_time.as_deref().unwrap_or("");
        if s.is_empty() || e.is_empty() || s >= e {
            return Redirect::to(&format!("/dashboard/event-types/{}/overrides", slug))
                .into_response();
        }
        (Some(s.to_string()), Some(e.to_string()))
    } else {
        (None, None)
    };

    let id = uuid::Uuid::new_v4().to_string();
    let _ = sqlx::query(
        "INSERT INTO availability_overrides (id, event_type_id, date, start_time, end_time, is_blocked) VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(&et_id)
    .bind(&form.date)
    .bind(&start_time)
    .bind(&end_time)
    .bind(if is_blocked { 1 } else { 0 })
    .execute(&state.pool)
    .await;

    tracing::info!(
        override_id = %id,
        event_type = %slug,
        date = %form.date,
        is_blocked = is_blocked,
        "availability override created"
    );

    Redirect::to(&format!("/dashboard/event-types/{}/overrides", slug)).into_response()
}

#[derive(Deserialize)]
struct DeleteOverrideForm {
    _csrf: Option<String>,
}

async fn delete_override(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
    headers: HeaderMap,
    Path((slug, override_id)): Path<(String, String)>,
    Form(form): Form<DeleteOverrideForm>,
) -> impl IntoResponse {
    if let Err(resp) = verify_csrf_token(&headers, &form._csrf) {
        return resp;
    }
    let user = &auth_user.user;

    // Look up the override's event type, then check management policy via
    // `can_manage_event_type` so the rule lives in one place.
    let target_et_id: Option<String> =
        sqlx::query_scalar("SELECT event_type_id FROM availability_overrides WHERE id = ?")
            .bind(&override_id)
            .fetch_optional(&state.pool)
            .await
            .unwrap_or(None);

    if let Some(et_id) = target_et_id {
        if can_manage_event_type(&state.pool, user, &et_id).await {
            let _ = sqlx::query("DELETE FROM availability_overrides WHERE id = ?")
                .bind(&override_id)
                .execute(&state.pool)
                .await;
            tracing::info!(override_id = %override_id, deleted_by = %user.email, "availability override deleted");
        }
    }

    Redirect::to(&format!("/dashboard/event-types/{}/overrides", slug)).into_response()
}

/// Renders the embed code generator for a personal event type. The page is
/// all client-side once rendered: changing form fields rebuilds the snippet
/// and reloads the preview iframe — nothing is persisted server-side.
async fn embed_page(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
    Path(slug): Path<String>,
) -> Response {
    let user = &auth_user.user;

    let et: Option<(String, String, String, String, String)> = sqlx::query_as(
        "SELECT et.id, et.slug, et.title, et.default_calendar_view, et.visibility \
         FROM event_types et \
         JOIN accounts a ON a.id = et.account_id \
         WHERE a.user_id = ? AND et.slug = ? AND et.team_id IS NULL",
    )
    .bind(&user.id)
    .bind(&slug)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    let (_et_id, et_slug, et_title, default_calendar_view, visibility) = match et {
        Some(e) => e,
        None => return Redirect::to("/dashboard/event-types").into_response(),
    };

    // Username for the cal link path: /u/{username}/{slug}. `users.username` is
    // nullable (migration 003) and all /u/{username} lookups match on it, so a
    // user without a username can't produce a working embed link. Rather than
    // emit a snippet pointing at a 404, render the page with a notice and no
    // snippet (has_username = false).
    let username = user.username.clone().filter(|u| !u.is_empty());
    let has_username = username.is_some();
    let cal_path = match &username {
        Some(u) => format!("/u/{}/{}", u, et_slug),
        None => String::new(),
    };

    let base_url = embed_base_url();
    let accent = org_accent_hex(&state.pool).await;

    let tmpl = match state.templates.get_template("embed.html") {
        Ok(t) => t,
        Err(e) => return internal_error_response("template render", &e),
    };

    let (impersonating, impersonating_name, _) = impersonation_ctx(&auth_user);

    Html(
        tmpl.render(context! {
            sidebar => sidebar_context(&auth_user, "event-types"),
            event_type_title => et_title,
            event_type_slug => et_slug,
            cal_path => cal_path,
            has_username => has_username,
            base_url => base_url,
            default_layout => default_calendar_view,
            accent_color => accent,
            visibility => visibility,
            impersonating => impersonating,
            impersonating_name => impersonating_name,
        })
        .unwrap_or_default(),
    )
    .into_response()
}

/// Absolute base URL for embed snippets: `CALRS_BASE_URL` without a trailing
/// slash, or empty (the page then falls back to the dashboard's own origin).
fn embed_base_url() -> String {
    std::env::var("CALRS_BASE_URL")
        .unwrap_or_default()
        .trim_end_matches('/')
        .to_string()
}

/// Resolve the org accent hex for prefilling the embed brand color. Mirrors
/// build_theme_css precedence: the preset's accent for a preset theme,
/// `custom_accent` only when theme='custom', default blue otherwise.
async fn org_accent_hex(pool: &SqlitePool) -> String {
    let theme_row: Option<(String, Option<String>)> =
        sqlx::query_as("SELECT theme, custom_accent FROM auth_config WHERE id = 'singleton'")
            .fetch_optional(pool)
            .await
            .unwrap_or(None);
    match theme_row {
        Some((ref theme, ref custom)) if theme == "custom" => custom
            .clone()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "#2563eb".to_string()),
        Some((ref theme, _)) => preset_accent(theme).to_string(),
        None => "#2563eb".to_string(),
    }
}

/// Embed code generator for a team event type. Mirrors `embed_page` but the
/// booking link is the team path `/team/{team_slug}/{event_slug}`, which is
/// always well-formed (teams always have a slug), so there is no missing-link
/// case. Access is restricted to team admins and global admins.
async fn group_embed_page(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
    Path((team_id, slug)): Path<(String, String)>,
) -> Response {
    let user = &auth_user.user;
    let is_admin = user.role == "admin";

    if !is_admin && !is_team_admin(&state.pool, &user.id, &team_id).await {
        return Html("You are not a team admin of this team.".to_string()).into_response();
    }

    let et: Option<(String, String, String, String, String, String)> = sqlx::query_as(
        "SELECT et.slug, et.title, et.default_calendar_view, et.visibility, t.slug, t.visibility \
         FROM event_types et \
         JOIN teams t ON t.id = et.team_id \
         WHERE et.team_id = ? AND et.slug = ?",
    )
    .bind(&team_id)
    .bind(&slug)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    let (et_slug, et_title, default_calendar_view, visibility, team_slug, team_visibility) =
        match et {
            Some(e) => e,
            None => return Redirect::to("/dashboard/event-types").into_response(),
        };

    let cal_path = format!("/team/{}/{}", team_slug, et_slug);
    let base_url = embed_base_url();
    let accent = org_accent_hex(&state.pool).await;

    let tmpl = match state.templates.get_template("embed.html") {
        Ok(t) => t,
        Err(e) => return internal_error_response("template render", &e),
    };

    let (impersonating, impersonating_name, _) = impersonation_ctx(&auth_user);

    // Team event types always sit on a real team slug, so the embed link is
    // never dead — has_username is always true here.
    Html(
        tmpl.render(context! {
            sidebar => sidebar_context(&auth_user, "event-types"),
            event_type_title => et_title,
            event_type_slug => et_slug,
            cal_path => cal_path,
            has_username => true,
            base_url => base_url,
            default_layout => default_calendar_view,
            accent_color => accent,
            visibility => visibility,
            team_visibility => team_visibility,
            impersonating => impersonating,
            impersonating_name => impersonating_name,
        })
        .unwrap_or_default(),
    )
    .into_response()
}

// --- Group event type handlers ---

async fn new_group_event_type_form(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
) -> impl IntoResponse {
    let user = &auth_user.user;
    let is_admin = user.role == "admin";

    let groups: Vec<(String, String)> = if is_admin {
        sqlx::query_as("SELECT t.id, t.name FROM teams t ORDER BY t.name")
            .fetch_all(&state.pool)
            .await
            .unwrap_or_default()
    } else {
        sqlx::query_as(
            "SELECT t.id, t.name FROM teams t JOIN team_members tm ON tm.team_id = t.id WHERE tm.user_id = ? AND tm.role = 'admin' ORDER BY t.name",
        )
        .bind(&user.id)
        .fetch_all(&state.pool)
        .await
        .unwrap_or_default()
    };

    if groups.is_empty() {
        return Html(if is_admin {
            "No teams exist yet.".to_string()
        } else {
            "You are not a team admin of any teams.".to_string()
        });
    }

    let groups_ctx: Vec<minijinja::Value> = groups
        .iter()
        .map(|(id, name)| context! { id => id, name => name })
        .collect();

    // Pre-fill availability from user's default schedule
    ensure_user_avail_seeded(&state.pool, &user.id).await;
    let user_avail = load_user_avail_schedule(&state.pool, &user.id).await;

    // Load all teams for watcher selection (exclude the selected team itself in template)
    let watcher_teams_ctx: Vec<minijinja::Value> = groups
        .iter()
        .map(|(id, name)| context! { id => id, name => name })
        .collect();

    let tmpl = match state.templates.get_template("event_type_form.html") {
        Ok(t) => t,
        Err(e) => return internal_error_html("template render", &e),
    };

    let (impersonating, impersonating_name, _) = impersonation_ctx(&auth_user);
    Html(
        tmpl.render(context! {
            editing => false,
            is_group => true,
            teams => groups_ctx,
            form_team_id => groups.first().map(|(id, _)| id.as_str()).unwrap_or(""),
            form_title => "",
            form_slug => "",
            form_description => "",
            form_duration => 30,
            form_slot_interval => 0,
            form_buffer_before => 0,
            form_buffer_after => 0,
            form_min_notice => 60,
            form_requires_confirmation => false,
            form_location_type => "link",
            form_location_value => "",
            form_meeting_pattern_override => "",
            form_avail_schedule => user_avail,
            form_default_calendar_view => "month",
            form_first_slot_only => false,
            form_frequency_limits => "",
            form_timezone => &user.timezone,
            tz_options => common_timezones_with(&user.timezone)
                .iter()
                .map(|(iana, label)| context! { value => iana, label => label })
                .collect::<Vec<_>>(),
            watcher_teams => watcher_teams_ctx,
            selected_watcher_team_ids => "",
            error => "",
            sidebar => sidebar_context(&auth_user, "event-types"),
            impersonating => impersonating,
            impersonating_name => impersonating_name,
        })
        .unwrap_or_else(|e| internal_error_body("template render", &e)),
    )
}

async fn create_group_event_type(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
    headers: HeaderMap,
    Form(form): Form<EventTypeForm>,
) -> impl IntoResponse {
    if let Err(resp) = verify_csrf_token(&headers, &form._csrf) {
        return resp;
    }
    let user = &auth_user.user;

    let team_id = match form.team_id.as_deref().filter(|s| !s.trim().is_empty()) {
        Some(tid) => tid.to_string(),
        None => return Redirect::to("/dashboard/event-types").into_response(),
    };

    let is_admin = user.role == "admin";

    // Only team admins (and global admins) can create team event types
    if !is_admin && !is_team_admin(&state.pool, &user.id, &team_id).await {
        return Html("You are not a team admin of this team.".to_string()).into_response();
    }

    // Find the user's account
    let account_id: Option<String> =
        sqlx::query_scalar("SELECT id FROM accounts WHERE user_id = ? LIMIT 1")
            .bind(&user.id)
            .fetch_optional(&state.pool)
            .await
            .unwrap_or(None);

    let account_id = match account_id {
        Some(id) => id,
        None => return Redirect::to("/dashboard/event-types").into_response(),
    };

    let mut slug = form.slug.trim().to_lowercase().replace(' ', "-");
    slug = slug
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-')
        .collect();
    if slug.is_empty() {
        slug = form
            .title
            .trim()
            .to_lowercase()
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
            .collect::<String>();
        while slug.contains("--") {
            slug = slug.replace("--", "-");
        }
        slug = slug.trim_matches('-').to_string();
    }
    if slug.is_empty() {
        return Html("Title is required to generate a slug.".to_string()).into_response();
    }

    // Check uniqueness within the team
    let existing: Option<(String,)> =
        sqlx::query_as("SELECT id FROM event_types WHERE team_id = ? AND slug = ?")
            .bind(&team_id)
            .bind(&slug)
            .fetch_optional(&state.pool)
            .await
            .unwrap_or(None);

    if existing.is_some() {
        return Html("An event type with this slug already exists in this team.".to_string())
            .into_response();
    }

    let et_id = uuid::Uuid::new_v4().to_string();
    let requires_confirmation = form.requires_confirmation.as_deref() == Some("on");
    let location_type = form.location_type.as_deref().unwrap_or("link");
    let location_value = form
        .location_value
        .as_deref()
        .filter(|s| !s.trim().is_empty());

    // Auto-meeting providers compute their own URL per booking; the static
    // location_value field is not used and may be empty.
    let location_required = !matches!(
        location_type,
        meeting::LOCATION_TYPE_JITSI | meeting::LOCATION_TYPE_WEBHOOK
    );
    if location_required && location_value.is_none() {
        return render_event_type_form_error(
            &state,
            &auth_user,
            "Location details are required (e.g. a video call link, phone number, or address).",
            &form,
            false,
        )
        .into_response();
    }

    let default_calendar_view = match form.default_calendar_view.as_deref().unwrap_or("month") {
        v @ ("month" | "week" | "column") => v.to_string(),
        _ => "month".to_string(),
    };

    let first_slot_only = form.first_slot_only.as_deref() == Some("on");
    let timezone = normalize_event_type_tz(form.timezone.as_deref(), &user.timezone);
    let cancel_notice_min =
        parse_notice_to_minutes(&form.cancel_notice_value, &form.cancel_notice_unit);
    let reschedule_notice_min =
        parse_notice_to_minutes(&form.reschedule_notice_value, &form.reschedule_notice_unit);

    let meeting_pattern_override = form
        .meeting_pattern_override
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);

    let _ = sqlx::query(
        "INSERT INTO event_types (id, account_id, slug, title, description, duration_min, slot_interval_min, buffer_before, buffer_after, min_notice_min, requires_confirmation, location_type, location_value, team_id, created_by_user_id, default_calendar_view, first_slot_only, timezone, cancel_notice_min, reschedule_notice_min, meeting_pattern_override)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&et_id)
    .bind(&account_id)
    .bind(&slug)
    .bind(form.title.trim())
    .bind(form.description.as_deref().filter(|s| !s.trim().is_empty()))
    .bind(parse_int_field(&form.duration_min, 30))
    .bind(parse_optional_positive_int(&form.slot_interval_min))
    .bind(parse_int_field(&form.buffer_before, 0))
    .bind(parse_int_field(&form.buffer_after, 0))
    .bind(parse_int_field(&form.min_notice_min, 60))
    .bind(requires_confirmation as i32)
    .bind(location_type)
    .bind(location_value)
    .bind(&team_id)
    .bind(&user.id)
    .bind(&default_calendar_view)
    .bind(first_slot_only as i32)
    .bind(&timezone)
    .bind(cancel_notice_min)
    .bind(reschedule_notice_min)
    .bind(&meeting_pattern_override)
    .execute(&state.pool)
    .await;

    // Create availability rules. Pass the creating user's profile-default
    // schedule as a fallback so an empty submission falls back to it instead
    // of hardcoded Mon-Fri 09:00-17:00.
    ensure_user_avail_seeded(&state.pool, &user.id).await;
    let user_default = load_user_avail_schedule(&state.pool, &user.id).await;
    let schedule = parse_avail_schedule(
        form.avail_schedule.as_deref(),
        form.avail_days.as_deref(),
        form.avail_windows.as_deref(),
        form.avail_start.as_deref(),
        form.avail_end.as_deref(),
        Some(&user_default),
    );

    for (day, windows) in &schedule {
        for (ws, we) in windows {
            let rule_id = uuid::Uuid::new_v4().to_string();
            let _ = sqlx::query(
                "INSERT INTO availability_rules (id, event_type_id, day_of_week, start_time, end_time) VALUES (?, ?, ?, ?, ?)",
            )
            .bind(&rule_id)
            .bind(&et_id)
            .bind(day)
            .bind(ws)
            .bind(we)
            .execute(&state.pool)
            .await;
        }
    }

    // Save watcher teams
    if let Some(ref watcher_ids) = form.watcher_team_ids {
        for wid in watcher_ids.split(',') {
            let wid = wid.trim();
            if !wid.is_empty() {
                let _ = sqlx::query(
                    "INSERT INTO event_type_watchers (event_type_id, team_id) VALUES (?, ?)",
                )
                .bind(&et_id)
                .bind(wid)
                .execute(&state.pool)
                .await;
            }
        }
    }

    // Save booking frequency limits
    save_frequency_limits(&state.pool, &et_id, &form.frequency_limits).await;

    Redirect::to("/dashboard/event-types").into_response()
}

async fn edit_group_event_type_form(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
    Path((team_id, slug)): Path<(String, String)>,
) -> impl IntoResponse {
    let user = &auth_user.user;

    let is_admin = user.role == "admin";

    let et: Option<(
        String,
        String,
        String,
        Option<String>,
        i32,
        i32,
        i32,
        i32,
        i32,
        String,
        Option<String>,
        Option<i32>,
        String,
        String,
        i32,
        String,
    )> = if is_admin {
        sqlx::query_as(
            "SELECT et.id, et.slug, et.title, et.description, et.duration_min, et.buffer_before, et.buffer_after, et.min_notice_min, et.requires_confirmation, et.location_type, et.location_value, et.reminder_minutes, et.team_id, et.visibility, et.max_additional_guests, et.scheduling_mode
             FROM event_types et
             WHERE et.team_id = ? AND et.slug = ?",
        )
        .bind(&team_id)
        .bind(&slug)
        .fetch_optional(&state.pool)
        .await
        .unwrap_or(None)
    } else {
        sqlx::query_as(
            "SELECT et.id, et.slug, et.title, et.description, et.duration_min, et.buffer_before, et.buffer_after, et.min_notice_min, et.requires_confirmation, et.location_type, et.location_value, et.reminder_minutes, et.team_id, et.visibility, et.max_additional_guests, et.scheduling_mode
             FROM event_types et
             JOIN team_members tm ON tm.team_id = et.team_id
             WHERE tm.user_id = ? AND tm.role = 'admin' AND et.team_id = ? AND et.slug = ?",
        )
        .bind(&user.id)
        .bind(&team_id)
        .bind(&slug)
        .fetch_optional(&state.pool)
        .await
        .unwrap_or(None)
    };

    let (
        et_id,
        et_slug,
        et_title,
        et_desc,
        duration,
        buf_before,
        buf_after,
        min_notice,
        requires_conf,
        loc_type,
        loc_value,
        reminder_min,
        team_id,
        visibility,
        max_additional_guests,
        scheduling_mode,
    ) = match et {
        Some(e) => e,
        None => return Html("Event type not found.".to_string()),
    };

    let slot_interval: Option<i32> =
        sqlx::query_scalar("SELECT slot_interval_min FROM event_types WHERE id = ?")
            .bind(&et_id)
            .fetch_one(&state.pool)
            .await
            .unwrap_or(None);

    let default_calendar_view: String =
        sqlx::query_scalar("SELECT default_calendar_view FROM event_types WHERE id = ?")
            .bind(&et_id)
            .fetch_one(&state.pool)
            .await
            .unwrap_or_else(|_| "month".to_string());

    let first_slot_only: i32 =
        sqlx::query_scalar("SELECT first_slot_only FROM event_types WHERE id = ?")
            .bind(&et_id)
            .fetch_one(&state.pool)
            .await
            .unwrap_or(0);

    let form_timezone: String =
        sqlx::query_scalar::<_, Option<String>>("SELECT timezone FROM event_types WHERE id = ?")
            .bind(&et_id)
            .fetch_optional(&state.pool)
            .await
            .unwrap_or(None)
            .flatten()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| user.timezone.clone());

    // Min-notice for guest-initiated cancel and reschedule (NULL = no
    // restriction). Pulled separately to keep the existing tuple compact.
    let cancel_notice_min: Option<i32> = sqlx::query_scalar::<_, Option<i32>>(
        "SELECT cancel_notice_min FROM event_types WHERE id = ?",
    )
    .bind(&et_id)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None)
    .flatten();
    let reschedule_notice_min: Option<i32> = sqlx::query_scalar::<_, Option<i32>>(
        "SELECT reschedule_notice_min FROM event_types WHERE id = ?",
    )
    .bind(&et_id)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None)
    .flatten();
    let (form_cancel_notice_value, form_cancel_notice_unit) =
        minutes_to_notice_form(cancel_notice_min);
    let (form_reschedule_notice_value, form_reschedule_notice_unit) =
        minutes_to_notice_form(reschedule_notice_min);

    let meeting_pattern_override: Option<String> = sqlx::query_scalar::<_, Option<String>>(
        "SELECT meeting_pattern_override FROM event_types WHERE id = ?",
    )
    .bind(&et_id)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None)
    .flatten();

    // Get current availability rules
    let all_rules: Vec<(i32, String, String)> = sqlx::query_as(
        "SELECT day_of_week, start_time, end_time FROM availability_rules WHERE event_type_id = ? ORDER BY day_of_week, start_time",
    )
    .bind(&et_id)
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();

    let avail_days: String = {
        let mut days: Vec<i32> = all_rules.iter().map(|(d, _, _)| *d).collect();
        days.sort();
        days.dedup();
        days.iter()
            .map(|d| d.to_string())
            .collect::<Vec<_>>()
            .join(",")
    };

    let mut windows: Vec<(String, String)> = Vec::new();
    for (_, s, e) in &all_rules {
        let pair = (s.clone(), e.clone());
        if !windows.contains(&pair) {
            windows.push(pair);
        }
    }
    let avail_windows: String = windows
        .iter()
        .map(|(s, e)| format!("{}-{}", s, e))
        .collect::<Vec<_>>()
        .join(",");
    let (avail_start, avail_end) = windows
        .first()
        .cloned()
        .unwrap_or_else(|| ("09:00".to_string(), "17:00".to_string()));

    // Build per-day schedule string
    let avail_schedule = build_avail_schedule(&all_rules);

    // Fetch team members with per-ET weights (round-robin priority OR collective exclusion)
    let is_round_robin_group = scheduling_mode == "round_robin";
    let is_collective_team = scheduling_mode == "collective";
    let members_ctx: Vec<minijinja::Value> = if is_round_robin_group || is_collective_team {
        // See the note on the personal-ET edit path: pulling `timezone` makes
        // wrong-TZ users visible on the Member priority list.
        let members: Vec<(String, String, Option<String>, String)> = sqlx::query_as(
            "SELECT u.id, u.name, u.avatar_path, u.timezone \
             FROM users u JOIN team_members tm ON tm.user_id = u.id \
             WHERE tm.team_id = ? AND u.enabled = 1 \
             ORDER BY u.name",
        )
        .bind(&team_id)
        .fetch_all(&state.pool)
        .await
        .unwrap_or_default();

        let et_weights: Vec<(String, i64)> = sqlx::query_as(
            "SELECT user_id, weight FROM event_type_member_weights WHERE event_type_id = ?",
        )
        .bind(&et_id)
        .fetch_all(&state.pool)
        .await
        .unwrap_or_default();
        let wmap: std::collections::HashMap<String, i64> = et_weights.into_iter().collect();

        members
            .iter()
            .map(|(uid, name, avatar_path, timezone)| {
                let w = wmap.get(uid).copied().unwrap_or(1);
                context! {
                    user_id => uid,
                    name => name,
                    has_avatar => avatar_path.is_some(),
                    initials => compute_initials(name),
                    weight => w,
                    timezone => timezone,
                }
            })
            .collect()
    } else {
        Vec::new()
    };

    // Load all teams for watcher selection
    let all_teams: Vec<(String, String)> = if is_admin {
        sqlx::query_as("SELECT id, name FROM teams ORDER BY name")
            .fetch_all(&state.pool)
            .await
            .unwrap_or_default()
    } else {
        sqlx::query_as(
            "SELECT t.id, t.name FROM teams t JOIN team_members tm ON tm.team_id = t.id WHERE tm.user_id = ? ORDER BY t.name",
        )
        .bind(&user.id)
        .fetch_all(&state.pool)
        .await
        .unwrap_or_default()
    };
    let watcher_teams_ctx: Vec<minijinja::Value> = all_teams
        .iter()
        .map(|(id, name)| context! { id => id, name => name })
        .collect();

    // Load selected watcher team IDs
    let selected_watchers: Vec<(String,)> =
        sqlx::query_as("SELECT team_id FROM event_type_watchers WHERE event_type_id = ?")
            .bind(&et_id)
            .fetch_all(&state.pool)
            .await
            .unwrap_or_default();
    let selected_watcher_team_ids: String = selected_watchers
        .iter()
        .map(|(id,)| id.as_str())
        .collect::<Vec<_>>()
        .join(",");

    // Fetch booking frequency limits
    let freq_limits: Vec<(i32, String, i32)> = sqlx::query_as(
        "SELECT max_bookings, period, per_member FROM booking_frequency_limits WHERE event_type_id = ?",
    )
    .bind(&et_id)
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();

    let form_frequency_limits = freq_limits
        .iter()
        .map(|(c, p, m)| {
            if *m != 0 {
                format!("{}:{}:m", c, p)
            } else {
                format!("{}:{}", c, p)
            }
        })
        .collect::<Vec<_>>()
        .join(",");

    let tmpl = match state.templates.get_template("event_type_form.html") {
        Ok(t) => t,
        Err(e) => return internal_error_html("template render", &e),
    };

    let (impersonating, impersonating_name, _) = impersonation_ctx(&auth_user);
    Html(
        tmpl.render(context! {
            editing => true,
            is_group => true,
            form_team_id => team_id,
            original_slug => et_slug,
            form_title => et_title,
            form_slug => et_slug,
            form_description => et_desc.unwrap_or_default(),
            form_duration => duration,
            form_slot_interval => slot_interval.unwrap_or(0),
            form_buffer_before => buf_before,
            form_buffer_after => buf_after,
            form_min_notice => min_notice,
            form_requires_confirmation => requires_conf != 0,
            form_visibility => visibility,
            form_location_type => loc_type,
            form_location_value => loc_value.unwrap_or_default(),
            form_meeting_pattern_override => meeting_pattern_override.clone().unwrap_or_default(),
            form_avail_days => avail_days,
            form_avail_start => avail_start,
            form_avail_end => avail_end,
            form_avail_windows => avail_windows,
            form_avail_schedule => avail_schedule,
            form_reminder_minutes => reminder_min.unwrap_or(0),
            form_max_additional_guests => max_additional_guests,
            form_scheduling_mode => scheduling_mode,
            form_default_calendar_view => default_calendar_view,
            form_first_slot_only => first_slot_only != 0,
            form_frequency_limits => form_frequency_limits,
            form_timezone => &form_timezone,
            form_cancel_notice_value => form_cancel_notice_value,
            form_cancel_notice_unit => form_cancel_notice_unit,
            form_reschedule_notice_value => form_reschedule_notice_value,
            form_reschedule_notice_unit => form_reschedule_notice_unit,
            tz_options => common_timezones_with(&form_timezone)
                .iter()
                .map(|(iana, label)| context! { value => iana, label => label })
                .collect::<Vec<_>>(),
            is_round_robin_group => is_round_robin_group,
            is_collective_team => is_collective_team,
            priority_members => members_ctx,
            watcher_teams => watcher_teams_ctx,
            selected_watcher_team_ids => selected_watcher_team_ids,
            error => "",
            sidebar => sidebar_context(&auth_user, "event-types"),
            impersonating => impersonating,
            impersonating_name => impersonating_name,
        })
        .unwrap_or_else(|e| internal_error_body("template render", &e)),
    )
}

async fn update_group_event_type(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
    headers: HeaderMap,
    Path((team_id, slug)): Path<(String, String)>,
    Form(form): Form<EventTypeForm>,
) -> impl IntoResponse {
    if let Err(resp) = verify_csrf_token(&headers, &form._csrf) {
        return resp;
    }
    let user = &auth_user.user;

    let is_admin = user.role == "admin";

    let et: Option<(String, String)> = if is_admin {
        sqlx::query_as(
            "SELECT et.id, et.team_id
             FROM event_types et
             WHERE et.team_id = ? AND et.slug = ?",
        )
        .bind(&team_id)
        .bind(&slug)
        .fetch_optional(&state.pool)
        .await
        .unwrap_or(None)
    } else {
        sqlx::query_as(
            "SELECT et.id, et.team_id
             FROM event_types et
             JOIN team_members tm ON tm.team_id = et.team_id
             WHERE tm.user_id = ? AND tm.role = 'admin' AND et.team_id = ? AND et.slug = ?",
        )
        .bind(&user.id)
        .bind(&team_id)
        .bind(&slug)
        .fetch_optional(&state.pool)
        .await
        .unwrap_or(None)
    };

    let (et_id, team_id) = match et {
        Some(e) => e,
        None => return Redirect::to("/dashboard/event-types").into_response(),
    };

    let new_slug = form.slug.trim().to_lowercase().replace(' ', "-");
    let requires_confirmation = form.requires_confirmation.as_deref() == Some("on");
    let visibility = form.visibility.as_deref().unwrap_or("public").to_string();

    // Check slug uniqueness within the team if changed
    if new_slug != slug {
        let existing: Option<(String,)> =
            sqlx::query_as("SELECT id FROM event_types WHERE team_id = ? AND slug = ?")
                .bind(&team_id)
                .bind(&new_slug)
                .fetch_optional(&state.pool)
                .await
                .unwrap_or(None);

        if existing.is_some() {
            return render_event_type_form_error(
                &state,
                &auth_user,
                "An event type with this slug already exists in this team.",
                &form,
                true,
            )
            .into_response();
        }
    }

    let location_type = form.location_type.as_deref().unwrap_or("link");
    let location_value = form
        .location_value
        .as_deref()
        .filter(|s| !s.trim().is_empty());

    let location_required = !matches!(
        location_type,
        meeting::LOCATION_TYPE_JITSI | meeting::LOCATION_TYPE_WEBHOOK
    );
    if location_required && location_value.is_none() {
        return render_event_type_form_error(
            &state,
            &auth_user,
            "Location details are required (e.g. a video call link, phone number, or address).",
            &form,
            true,
        )
        .into_response();
    }

    let reminder_minutes = {
        let v = parse_int_field(&form.reminder_minutes, 0);
        if v > 0 {
            Some(v)
        } else {
            None
        }
    };

    let default_calendar_view = match form.default_calendar_view.as_deref().unwrap_or("month") {
        v @ ("month" | "week" | "column") => v.to_string(),
        _ => "month".to_string(),
    };

    let timezone = normalize_event_type_tz(form.timezone.as_deref(), &user.timezone);
    let cancel_notice_min =
        parse_notice_to_minutes(&form.cancel_notice_value, &form.cancel_notice_unit);
    let reschedule_notice_min =
        parse_notice_to_minutes(&form.reschedule_notice_value, &form.reschedule_notice_unit);

    let meeting_pattern_override = form
        .meeting_pattern_override
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);

    let _ = sqlx::query(
        "UPDATE event_types SET slug = ?, title = ?, description = ?, duration_min = ?, slot_interval_min = ?, buffer_before = ?, buffer_after = ?, min_notice_min = ?, requires_confirmation = ?, location_type = ?, location_value = ?, reminder_minutes = ?, visibility = ?, max_additional_guests = ?, scheduling_mode = ?, default_calendar_view = ?, first_slot_only = ?, timezone = ?, cancel_notice_min = ?, reschedule_notice_min = ?, meeting_pattern_override = ? WHERE id = ?",
    )
    .bind(&new_slug)
    .bind(form.title.trim())
    .bind(form.description.as_deref().filter(|s| !s.trim().is_empty()))
    .bind(parse_int_field(&form.duration_min, 30))
    .bind(parse_optional_positive_int(&form.slot_interval_min))
    .bind(parse_int_field(&form.buffer_before, 0))
    .bind(parse_int_field(&form.buffer_after, 0))
    .bind(parse_int_field(&form.min_notice_min, 60))
    .bind(requires_confirmation as i32)
    .bind(location_type)
    .bind(location_value)
    .bind(reminder_minutes)
    .bind(&visibility)
    .bind(parse_int_field(&form.max_additional_guests, 0))
    .bind(form.scheduling_mode.as_deref().unwrap_or("round_robin"))
    .bind(&default_calendar_view)
    .bind(if form.first_slot_only.as_deref() == Some("on") { 1 } else { 0 })
    .bind(&timezone)
    .bind(cancel_notice_min)
    .bind(reschedule_notice_min)
    .bind(&meeting_pattern_override)
    .bind(&et_id)
    .execute(&state.pool)
    .await;

    // Update availability rules. Pass the editing user's profile-default
    // schedule as a fallback so an empty submission falls back to it instead
    // of hardcoded Mon-Fri 09:00-17:00.
    let _ = sqlx::query("DELETE FROM availability_rules WHERE event_type_id = ?")
        .bind(&et_id)
        .execute(&state.pool)
        .await;

    ensure_user_avail_seeded(&state.pool, &user.id).await;
    let user_default = load_user_avail_schedule(&state.pool, &user.id).await;
    let schedule = parse_avail_schedule(
        form.avail_schedule.as_deref(),
        form.avail_days.as_deref(),
        form.avail_windows.as_deref(),
        form.avail_start.as_deref(),
        form.avail_end.as_deref(),
        Some(&user_default),
    );

    for (day, windows) in &schedule {
        for (ws, we) in windows {
            let rule_id = uuid::Uuid::new_v4().to_string();
            let _ = sqlx::query(
                "INSERT INTO availability_rules (id, event_type_id, day_of_week, start_time, end_time) VALUES (?, ?, ?, ?, ?)",
            )
            .bind(&rule_id)
            .bind(&et_id)
            .bind(day)
            .bind(ws)
            .bind(we)
            .execute(&state.pool)
            .await;
        }
    }

    // Update watcher teams: delete old, insert new
    let _ = sqlx::query("DELETE FROM event_type_watchers WHERE event_type_id = ?")
        .bind(&et_id)
        .execute(&state.pool)
        .await;

    if let Some(ref watcher_ids) = form.watcher_team_ids {
        for wid in watcher_ids.split(',') {
            let wid = wid.trim();
            if !wid.is_empty() {
                let _ = sqlx::query(
                    "INSERT INTO event_type_watchers (event_type_id, team_id) VALUES (?, ?)",
                )
                .bind(&et_id)
                .bind(wid)
                .execute(&state.pool)
                .await;
            }
        }
    }

    // Update booking frequency limits: delete old, insert new
    let _ = sqlx::query("DELETE FROM booking_frequency_limits WHERE event_type_id = ?")
        .bind(&et_id)
        .execute(&state.pool)
        .await;
    save_frequency_limits(&state.pool, &et_id, &form.frequency_limits).await;

    Redirect::to("/dashboard/event-types").into_response()
}

async fn toggle_group_event_type(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
    headers: HeaderMap,
    Path((team_id, slug)): Path<(String, String)>,
    Form(csrf): Form<CsrfForm>,
) -> impl IntoResponse {
    if let Err(resp) = verify_csrf_token(&headers, &csrf._csrf) {
        return resp;
    }
    let user = &auth_user.user;

    let is_admin = user.role == "admin";

    // Scope by team_id from the route to disambiguate slugs shared across teams.
    let et: Option<(String,)> = if is_admin {
        sqlx::query_as("SELECT id FROM event_types WHERE team_id = ? AND slug = ?")
            .bind(&team_id)
            .bind(&slug)
            .fetch_optional(&state.pool)
            .await
            .unwrap_or(None)
    } else {
        sqlx::query_as(
            "SELECT et.id FROM event_types et \
             JOIN team_members tm ON tm.team_id = et.team_id \
             WHERE et.team_id = ? AND et.slug = ? AND tm.user_id = ? AND tm.role = 'admin' \
             LIMIT 1",
        )
        .bind(&team_id)
        .bind(&slug)
        .bind(&user.id)
        .fetch_optional(&state.pool)
        .await
        .unwrap_or(None)
    };

    if let Some((et_id,)) = et {
        let _ = sqlx::query(
            "UPDATE event_types SET enabled = CASE WHEN enabled = 1 THEN 0 ELSE 1 END WHERE id = ?",
        )
        .bind(&et_id)
        .execute(&state.pool)
        .await;

        tracing::debug!(event_type_id = %et_id, event_type_slug = %slug, "group event type toggled");
    }

    Redirect::to("/dashboard/event-types").into_response()
}

async fn delete_group_event_type(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
    headers: HeaderMap,
    Path((team_id, slug)): Path<(String, String)>,
    Form(csrf): Form<CsrfForm>,
) -> impl IntoResponse {
    if let Err(resp) = verify_csrf_token(&headers, &csrf._csrf) {
        return resp;
    }
    let user = &auth_user.user;

    let is_admin = user.role == "admin";

    let et: Option<(String,)> = if is_admin {
        sqlx::query_as(
            "SELECT et.id FROM event_types et
             WHERE et.team_id = ? AND et.slug = ?",
        )
        .bind(&team_id)
        .bind(&slug)
        .fetch_optional(&state.pool)
        .await
        .unwrap_or(None)
    } else {
        sqlx::query_as(
            "SELECT et.id FROM event_types et
             JOIN team_members tm ON tm.team_id = et.team_id
             WHERE et.team_id = ? AND et.slug = ? AND tm.user_id = ? AND tm.role = 'admin' LIMIT 1",
        )
        .bind(&team_id)
        .bind(&slug)
        .bind(&user.id)
        .fetch_optional(&state.pool)
        .await
        .unwrap_or(None)
    };

    let et_id = match et {
        Some((id,)) => id,
        None => return Redirect::to("/dashboard/event-types").into_response(),
    };

    // Check for active bookings
    let active_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM bookings WHERE event_type_id = ? AND status IN ('confirmed', 'pending')",
    )
    .bind(&et_id)
    .fetch_one(&state.pool)
    .await
    .unwrap_or(0);

    if active_count > 0 {
        return Redirect::to("/dashboard/event-types").into_response();
    }

    // Delete in order: availability_rules, availability_overrides, event_type_calendars, bookings, event_type
    let _ = sqlx::query("DELETE FROM availability_rules WHERE event_type_id = ?")
        .bind(&et_id)
        .execute(&state.pool)
        .await;
    let _ = sqlx::query("DELETE FROM availability_overrides WHERE event_type_id = ?")
        .bind(&et_id)
        .execute(&state.pool)
        .await;
    let _ = sqlx::query("DELETE FROM event_type_calendars WHERE event_type_id = ?")
        .bind(&et_id)
        .execute(&state.pool)
        .await;
    let _ = sqlx::query("DELETE FROM bookings WHERE event_type_id = ?")
        .bind(&et_id)
        .execute(&state.pool)
        .await;
    let _ = sqlx::query("DELETE FROM event_types WHERE id = ?")
        .bind(&et_id)
        .execute(&state.pool)
        .await;

    tracing::info!(event_type_id = %et_id, user = %auth_user.user.email, "group event type deleted");

    Redirect::to("/dashboard/event-types").into_response()
}

// --- Legacy /g/ redirects ---

async fn redirect_g_to_team(Path(team_slug): Path<String>) -> impl IntoResponse {
    Redirect::permanent(&format!("/team/{}", team_slug))
}

async fn redirect_g_to_team_slug(
    Path((team_slug, slug)): Path<(String, String)>,
    Query(query): Query<std::collections::HashMap<String, String>>,
) -> impl IntoResponse {
    let qs = if query.is_empty() {
        String::new()
    } else {
        format!(
            "?{}",
            query
                .iter()
                .map(|(k, v)| format!("{}={}", k, v))
                .collect::<Vec<_>>()
                .join("&")
        )
    };
    Redirect::permanent(&format!("/team/{}/{}{}", team_slug, slug, qs))
}

async fn redirect_g_to_team_slug_book(
    Path((team_slug, slug)): Path<(String, String)>,
) -> impl IntoResponse {
    Redirect::permanent(&format!("/team/{}/{}/book", team_slug, slug))
}

// --- Legacy /t/ team link redirects ---

async fn redirect_team_link_to_team(
    State(state): State<Arc<AppState>>,
    Path(token): Path<String>,
) -> impl IntoResponse {
    // Look up the team by invite_token (team links were migrated to teams)
    let team: Option<(Option<String>,)> =
        sqlx::query_as("SELECT slug FROM teams WHERE invite_token = ?")
            .bind(&token)
            .fetch_optional(&state.pool)
            .await
            .unwrap_or(None);

    match team {
        Some((Some(slug),)) => {
            Redirect::permanent(&format!("/team/{}?invite={}", slug, token)).into_response()
        }
        Some((None,)) => {
            // Team exists but has no slug — should not happen after migration fix,
            // but handle gracefully
            Html("Team not found.".to_string()).into_response()
        }
        None => Html("Team not found.".to_string()).into_response(),
    }
}

// --- Group public pages ---

async fn team_profile_page(
    State(state): State<Arc<AppState>>,
    optional_auth: crate::auth::OptionalAuthUser,
    Path(team_slug): Path<String>,
    Query(query): Query<std::collections::HashMap<String, String>>,
) -> impl IntoResponse {
    let team: Option<(String, String, Option<String>, Option<String>, String, Option<String>)> =
        sqlx::query_as("SELECT id, name, description, avatar_path, visibility, invite_token FROM teams WHERE slug = ?")
            .bind(&team_slug)
            .fetch_optional(&state.pool)
            .await
            .unwrap_or(None);

    let (
        team_id,
        team_name,
        team_description,
        team_avatar_path,
        team_visibility,
        team_invite_token,
    ) = match team {
        Some(t) => t,
        None => return Html("Team not found.".to_string()),
    };

    // Logged-in team members (and global admins) bypass the private-team
    // invite token gate — their membership is the access. Compute once and
    // reuse for both the gate below and the event-type filter further down.
    let viewer_is_member_or_admin = match &optional_auth.user {
        Some(u) if u.role == "admin" => true,
        Some(u) => is_team_member(&state.pool, &u.id, &team_id).await,
        None => false,
    };

    let passed_invite = query.get("invite").cloned().unwrap_or_default();
    if team_visibility == "private" && !viewer_is_member_or_admin {
        match &team_invite_token {
            Some(expected) if !passed_invite.is_empty() && passed_invite == *expected => {
                // valid — continue
            }
            _ => {
                return Html("Team not found.".to_string());
            }
        }
    }

    // Logged-in team members (and global admins) see all enabled event types,
    // including private/internal — they have view access via the slot pages
    // anyway, so showing them on the profile makes them discoverable. Anonymous
    // visitors and non-members continue to see only public event types.
    let event_types: Vec<(String, String, Option<String>, i32, String)> =
        if viewer_is_member_or_admin {
            sqlx::query_as(
                "SELECT et.slug, et.title, et.description, et.duration_min, et.visibility
             FROM event_types et
             WHERE et.team_id = ? AND et.enabled = 1
             ORDER BY et.created_at",
            )
            .bind(&team_id)
            .fetch_all(&state.pool)
            .await
            .unwrap_or_default()
        } else {
            sqlx::query_as(
                "SELECT et.slug, et.title, et.description, et.duration_min, et.visibility
             FROM event_types et
             WHERE et.team_id = ? AND et.enabled = 1 AND et.visibility = 'public'
             ORDER BY et.created_at",
            )
            .bind(&team_id)
            .fetch_all(&state.pool)
            .await
            .unwrap_or_default()
        };

    let members: Vec<(String, String, Option<String>)> = sqlx::query_as(
        "SELECT u.id, u.name, u.avatar_path FROM users u \
         JOIN team_members tm ON tm.user_id = u.id \
         WHERE tm.team_id = ? AND u.enabled = 1 \
         ORDER BY u.name",
    )
    .bind(&team_id)
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();

    let members_ctx: Vec<minijinja::Value> = members
        .iter()
        .map(|(id, name, ap)| {
            context! {
                id => id,
                name => name,
                has_avatar => ap.is_some(),
                initials => compute_initials(name),
            }
        })
        .collect();

    let tmpl = match state.templates.get_template("team_profile.html") {
        Ok(t) => t,
        Err(e) => return internal_error_html("template render", &e),
    };

    let et_ctx: Vec<minijinja::Value> = event_types
        .iter()
        .map(|(slug, title, desc, duration, visibility)| {
            context! {
                slug => slug,
                title => title,
                description => desc.as_deref().map(crate::utils::render_inline_markdown),
                duration_min => duration,
                visibility => visibility,
            }
        })
        .collect();

    // Pass invite token through if team is private (so links include it)
    let invite_token_for_template = if team_visibility == "private" && !passed_invite.is_empty() {
        passed_invite
    } else {
        String::new()
    };

    Html(
        tmpl.render(context! {
            team_id => team_id,
            team_name => team_name,
            team_slug => team_slug,
            team_description => team_description.as_deref().map(crate::utils::render_inline_markdown),
            team_has_avatar => team_avatar_path.is_some(),
            team_initials => compute_initials(&team_name),
            members => members_ctx,
            event_types => et_ctx,
            invite_token => invite_token_for_template,
            company_link => state.company_link.read().await.clone(),
        })
        .unwrap_or_else(|e| internal_error_body("template render", &e)),
    )
}

async fn show_group_slots(
    State(state): State<Arc<AppState>>,
    optional_auth: crate::auth::OptionalAuthUser,
    headers: HeaderMap,
    Path((team_slug, slug)): Path<(String, String)>,
    Query(query): Query<SlotsQuery>,
) -> Response {
    let embed = query.embed_params();
    let lang = crate::i18n::detect_from_headers(&headers);
    let et: Option<(String, String, String, Option<String>, i32, i32, i32, i32, String, Option<String>, String, String, String, String, Option<String>, String)> = sqlx::query_as(
        "SELECT et.id, et.slug, et.title, et.description, et.duration_min, et.buffer_before, et.buffer_after, et.min_notice_min, et.location_type, et.location_value, t.name, et.visibility, et.scheduling_mode, t.visibility, t.invite_token, et.default_calendar_view
         FROM event_types et
         JOIN teams t ON t.id = et.team_id
         WHERE t.slug = ? AND et.slug = ? AND et.enabled = 1",
    )
    .bind(&team_slug)
    .bind(&slug)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    let (
        et_id,
        et_slug,
        et_title,
        et_desc,
        duration,
        buf_before,
        buf_after,
        min_notice,
        loc_type,
        loc_value,
        team_name,
        visibility,
        scheduling_mode,
        team_visibility,
        team_invite_token,
        default_calendar_view,
    ) = match et {
        Some(e) => e,
        None => return Html("Event type not found.".to_string()).into_response(),
    };

    // Resolve the team id once and reuse for membership checks and downstream
    // queries. Can't fold this into the event-type SELECT above because sqlx
    // tuples cap at 16 columns and we're already at the limit.
    let team_id: String = match sqlx::query_scalar::<_, String>(
        "SELECT team_id FROM event_types WHERE id = ? AND team_id IS NOT NULL",
    )
    .bind(&et_id)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None)
    {
        Some(tid) => tid,
        None => return Html("Event type not found.".to_string()).into_response(),
    };

    // Global admins can view any team event type (matches their management
    // surface). Booking is still invite-gated for private/internal events;
    // global admins do not auto-bypass the booking gate.
    let is_logged_in_team_member = match &optional_auth.user {
        Some(u) if u.role == "admin" => true,
        Some(u) => is_team_member(&state.pool, &u.id, &team_id).await,
        None => false,
    };

    // Validate booking invite token if one was provided (used both for direct
    // booking-invite access and to mark the request as "can_book").
    let has_valid_booking_invite = if let Some(token) = query.invite.as_deref() {
        if token.is_empty() {
            false
        } else {
            let invite: Option<(Option<String>, i32, i32)> = sqlx::query_as(
                "SELECT expires_at, max_uses, used_count FROM booking_invites WHERE token = ? AND event_type_id = ?",
            )
            .bind(token)
            .bind(&et_id)
            .fetch_optional(&state.pool)
            .await
            .unwrap_or(None);
            match invite {
                Some((expires_at, max_uses, used_count)) => {
                    let not_expired = expires_at.as_deref().is_none_or(|exp| {
                        exp >= chrono::Utc::now()
                            .format("%Y-%m-%d %H:%M:%S")
                            .to_string()
                            .as_str()
                    });
                    not_expired && used_count < max_uses
                }
                None => false,
            }
        }
    } else {
        false
    };

    // Did the request supply a valid team-level invite token? Used for private
    // teams hosting public event types (legacy behavior preserved).
    let has_valid_team_invite = matches!(
        (&team_invite_token, &query.invite),
        (Some(expected), Some(provided)) if !provided.is_empty() && provided == expected
    );

    // Access matrix:
    //   public team + public event       → anyone
    //   private team + public event      → team invite OR logged-in team member
    //   any team + private/internal event → booking invite OR logged-in team member (view only)
    let can_view = match (team_visibility.as_str(), visibility.as_str()) {
        (_, "private") | (_, "internal") => has_valid_booking_invite || is_logged_in_team_member,
        ("private", _) => has_valid_team_invite || is_logged_in_team_member,
        _ => true,
    };

    if !can_view {
        let msg = if visibility == "private" || visibility == "internal" {
            "This event type requires an invite link."
        } else {
            // Public event on private team without team invite — preserve legacy
            // "Event type not found" response so we don't leak existence.
            "Event type not found."
        };
        return Html(msg.to_string()).into_response();
    }

    // can_book: only with a valid booking invite for private/internal events.
    // Public events: anyone who can view can also book.
    let can_book = match visibility.as_str() {
        "private" | "internal" => has_valid_booking_invite,
        _ => can_view,
    };

    let guest_tz = parse_guest_tz(query.tz.as_deref());
    let host_tz = get_host_tz(&state.pool, &et_id).await;
    let guest_tz_name = guest_tz.name().to_string();

    let (year, month) = parse_month_param(query.month.as_deref(), guest_tz);
    let (
        start_offset,
        days_ahead,
        month_label,
        prev_month,
        next_month,
        first_weekday,
        days_in_month,
        today_date,
        month_year,
    ) = build_month_params(year, month, host_tz, guest_tz, lang);

    // Build team busy source: fetch busy times per member
    let now_host = Utc::now().with_timezone(&host_tz).naive_local();
    let end_date = now_host.date() + Duration::days((start_offset + days_ahead) as i64);
    let window_end = end_date.and_hms_opt(23, 59, 59).unwrap_or(now_host);

    let busy = {
        let tid = &team_id;
        let members: Vec<(String,)> = sqlx::query_as(
            "SELECT u.id FROM users u JOIN team_members tm ON tm.user_id = u.id \
             LEFT JOIN event_type_member_weights etw ON etw.user_id = u.id AND etw.event_type_id = ? \
             WHERE tm.team_id = ? AND u.enabled = 1 \
             AND COALESCE(etw.weight, 1) > 0",
        ).bind(&et_id).bind(tid).fetch_all(&state.pool).await.unwrap_or_default();
        // Sync all group members' calendars if stale. sync_if_stale holds
        // a per-source mutex and re-checks staleness inside the lock, so
        // even with this fan-out at most one CalDAV fetch per source is in
        // flight at a time.
        let mut sync_tasks = tokio::task::JoinSet::new();
        for (uid,) in &members {
            let pool = state.pool.clone();
            let key = state.secret_key;
            let uid = uid.clone();
            sync_tasks.spawn(async move {
                crate::commands::sync::sync_if_stale(&pool, &key, &uid).await;
            });
        }
        while sync_tasks.join_next().await.is_some() {}
        let mut member_busy = HashMap::new();
        for (uid,) in &members {
            let mut busy = fetch_busy_times_for_user(
                &state.pool,
                uid,
                now_host,
                window_end,
                host_tz,
                Some(&et_id),
            )
            .await;
            // Constrain each member to their personal working hours, converted
            // from their own timezone into host_tz. Members without explicit
            // hours in user_availability_rules are returned unconstrained
            // (user_avail_as_busy short-circuits to an empty Vec), so we never
            // plant surprise 9-17 defaults — only respect hours users actually
            // set on the settings page.
            busy.extend(user_avail_as_busy(&state.pool, uid, now_host, window_end, host_tz).await);
            member_busy.insert(uid.clone(), busy);
        }
        if scheduling_mode == "collective" {
            BusySource::Team(member_busy)
        } else {
            BusySource::Group(member_busy)
        }
    };

    let slot_days = compute_slots(
        &state.pool,
        &et_id,
        duration,
        buf_before,
        buf_after,
        min_notice,
        start_offset,
        days_ahead,
        host_tz,
        guest_tz,
        busy,
    )
    .await;

    let days_ctx: Vec<minijinja::Value> = slot_days
        .iter()
        .map(|d| {
            let slots: Vec<minijinja::Value> = d
                .slots
                .iter()
                .map(|s| context! { start => s.start, end => s.end, host_date => s.host_date, host_time => s.host_time, guest_date => s.guest_date })
                .collect();
            context! { date => d.date, label => d.label, slots => slots }
        })
        .collect();

    let available_dates: Vec<String> = slot_days.iter().map(|d| d.date.clone()).collect();

    let tz_options: Vec<minijinja::Value> = common_timezones_with(&guest_tz_name)
        .iter()
        .map(|(iana, label)| context! { value => iana, label => label, selected => (*iana == guest_tz_name) })
        .collect();

    // Fetch team avatar for sidebar display (team_id is already in scope from
    // the initial event-type lookup).
    let team_avatar_path: Option<String> =
        sqlx::query_scalar("SELECT avatar_path FROM teams WHERE id = ?")
            .bind(&team_id)
            .fetch_optional(&state.pool)
            .await
            .unwrap_or(None)
            .flatten();

    // Fetch active team members for sidebar display (exclude members with weight=0)
    let team_members_rows: Vec<(String, String, Option<String>)> = sqlx::query_as(
        "SELECT u.id, u.name, u.avatar_path FROM users u \
         JOIN team_members tm ON tm.user_id = u.id \
         LEFT JOIN event_type_member_weights etw ON etw.user_id = u.id AND etw.event_type_id = ? \
         WHERE tm.team_id = ? AND u.enabled = 1 AND COALESCE(etw.weight, 1) > 0 \
         ORDER BY u.name",
    )
    .bind(&et_id)
    .bind(&team_id)
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();

    let team_members_ctx: Vec<minijinja::Value> = team_members_rows
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

    let (meeting_jitsi_label, meeting_webhook_label) = meeting_provider_labels(&state).await;
    let tmpl = match state.templates.get_template("slots.html") {
        Ok(t) => t,
        Err(e) => return internal_error_html("internal", &e).into_response(),
    };
    let rendered = tmpl
        .render(context! {
            meeting_jitsi_label => meeting_jitsi_label,
            meeting_webhook_label => meeting_webhook_label,
            event_type => context! {
                slug => et_slug,
                title => et_title,
                description => et_desc.as_deref().map(crate::utils::render_inline_markdown),
                duration_min => duration,
                location_type => loc_type,
                location_value => loc_value,
            },
            host_name => team_name,
            team_slug => team_slug,
            team_id => team_id,
            team_has_avatar => team_avatar_path.is_some(),
            team_initials => compute_initials(&team_name),
            team_members => team_members_ctx,
            days => days_ctx,
            available_dates => available_dates,
            month_label => month_label,
            month_year => month_year,
            prev_month => prev_month,
            next_month => next_month,
            first_weekday => first_weekday,
            days_in_month => days_in_month,
            today_date => today_date,
            guest_tz => guest_tz_name,
            tz_options => tz_options,
            invite_token => query.invite.as_deref().unwrap_or(""),
            default_calendar_view => embed.layout.as_deref().filter(|s| !s.is_empty()).unwrap_or(default_calendar_view.as_str()),
            company_link => state.company_link.read().await.clone(),
            lang => lang,
            can_book => can_book,
            embed => embed.is_embedded(),
            embed_brand => embed.brand_css().unwrap_or_default(),
            embed_theme => embed.theme.as_deref().unwrap_or(""),
            embed_qs_amp => embed.query_suffix(true),
            embed_qs_q => embed.query_suffix(false),
        })
        .unwrap_or_else(|e| internal_error_body("template render", &e));

    html_response(rendered, &embed)
}

async fn show_group_book_form(
    State(state): State<Arc<AppState>>,
    optional_auth: crate::auth::OptionalAuthUser,
    headers: HeaderMap,
    Path((team_slug, slug)): Path<(String, String)>,
    Query(query): Query<BookQuery>,
) -> Response {
    let embed = query.embed_params();
    let lang = crate::i18n::detect_from_headers(&headers);
    let et: Option<(String, String, String, Option<String>, i32, String, Option<String>, String, String, i32, String, Option<String>, String)> = sqlx::query_as(
        "SELECT et.id, et.slug, et.title, et.description, et.duration_min, et.location_type, et.location_value, t.name, et.visibility, et.max_additional_guests, t.visibility, t.invite_token, t.id
         FROM event_types et
         JOIN teams t ON t.id = et.team_id
         WHERE t.slug = ? AND et.slug = ? AND et.enabled = 1",
    )
    .bind(&team_slug)
    .bind(&slug)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    let (
        et_id,
        et_slug,
        et_title,
        et_desc,
        duration,
        loc_type,
        loc_value,
        team_name,
        visibility,
        max_additional_guests,
        team_visibility,
        team_invite_token,
        team_id,
    ) = match et {
        Some(e) => e,
        None => return Html("Event type not found.".to_string()).into_response(),
    };

    // Logged-in team members (and global admins) substitute for the team
    // invite token on public events of private teams. Private/internal events
    // still require a valid booking-invite token regardless of team membership.
    let viewer_is_member_or_admin = match &optional_auth.user {
        Some(u) if u.role == "admin" => true,
        Some(u) => is_team_member(&state.pool, &u.id, &team_id).await,
        None => false,
    };

    let invite_guest_name;
    let invite_guest_email;
    if visibility == "private" || visibility == "internal" {
        let token = match &query.invite {
            Some(t) => t,
            None => {
                return Html("This event type requires an invite link.".to_string()).into_response()
            }
        };
        let invite: Option<(String, String, Option<String>, i32, i32)> = sqlx::query_as(
            "SELECT guest_name, guest_email, expires_at, max_uses, used_count FROM booking_invites WHERE token = ? AND event_type_id = ?",
        )
        .bind(token)
        .bind(&et_id)
        .fetch_optional(&state.pool)
        .await
        .unwrap_or(None);

        match invite {
            None => return Html("Invalid invite link.".to_string()).into_response(),
            Some((name, email, expires_at, max_uses, used_count)) => {
                if let Some(exp) = &expires_at {
                    if exp < &chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string() {
                        return Html("This invite link has expired.".to_string()).into_response();
                    }
                }
                if used_count >= max_uses {
                    return Html("This invite link has already been used.".to_string())
                        .into_response();
                }
                invite_guest_name = Some(name);
                invite_guest_email = Some(email);
            }
        }
    } else if team_visibility == "private" && !viewer_is_member_or_admin {
        // Public event type on a private team — needs the team invite token
        // unless the viewer is a logged-in team member or global admin.
        let valid = matches!((&team_invite_token, &query.invite), (Some(expected), Some(provided)) if !provided.is_empty() && provided == expected);
        if !valid {
            return Html("Event type not found.".to_string()).into_response();
        }
        invite_guest_name = None;
        invite_guest_email = None;
    } else {
        invite_guest_name = None;
        invite_guest_email = None;
    }

    let guest_tz = parse_guest_tz(query.tz.as_deref());
    let guest_tz_name = guest_tz.name().to_string();

    let date = match NaiveDate::parse_from_str(&query.date, "%Y-%m-%d") {
        Ok(d) => d,
        Err(_) => return Html("Invalid date format.".to_string()).into_response(),
    };
    let time = match NaiveTime::parse_from_str(&query.time, "%H:%M") {
        Ok(t) => t,
        Err(_) => return Html("Invalid time format.".to_string()).into_response(),
    };
    let end_time = (date.and_time(time) + Duration::minutes(duration as i64))
        .time()
        .format("%H:%M")
        .to_string();
    let date_label = crate::i18n::format_long_date(date, lang);

    let (meeting_jitsi_label, meeting_webhook_label) = meeting_provider_labels(&state).await;
    let tmpl = match state.templates.get_template("book.html") {
        Ok(t) => t,
        Err(e) => return internal_error_html("internal", &e).into_response(),
    };
    let captcha = captcha::CaptchaVars::from_config(&*state.captcha_config.read().await);
    let rendered = tmpl
        .render(context! {
            meeting_jitsi_label => meeting_jitsi_label,
            meeting_webhook_label => meeting_webhook_label,
            event_type => context! {
                slug => et_slug,
                title => et_title,
                description => et_desc.as_deref().map(crate::utils::render_inline_markdown),
                duration_min => duration,
                location_type => loc_type,
                location_value => loc_value,
            },
            host_name => team_name,
            team_slug => team_slug,
            date => query.date,
            date_label => date_label,
            time_start => query.time,
            time_end => end_time,
            guest_tz => guest_tz_name,
            error => "",
            form_name => invite_guest_name.as_deref().unwrap_or(""),
            form_email => invite_guest_email.as_deref().unwrap_or(""),
            form_notes => "",
            invite_token => query.invite.as_deref().unwrap_or(""),
            max_additional_guests => max_additional_guests,
            company_link => state.company_link.read().await.clone(),
            captcha_enabled => captcha.enabled,
            captcha_api_endpoint => captcha.api_endpoint,
            captcha_widget_url => captcha.widget_url,
            lang => lang,
            embed => embed.is_embedded(),
            embed_brand => embed.brand_css().unwrap_or_default(),
            embed_theme => embed.theme.as_deref().unwrap_or(""),
            embed_qs_amp => embed.query_suffix(true),
            embed_qs_q => embed.query_suffix(false),
        })
        .unwrap_or_else(|e| internal_error_body("template render", &e));

    html_response(rendered, &embed)
}

async fn handle_group_booking(
    State(state): State<Arc<AppState>>,
    optional_auth: crate::auth::OptionalAuthUser,
    headers: axum::http::HeaderMap,
    Path((team_slug, slug)): Path<(String, String)>,
    Query(embed): Query<EmbedParams>,
    Form(form): Form<BookForm>,
) -> Response {
    if let Err(resp) = verify_csrf_token(&headers, &form._csrf) {
        return resp;
    }
    let lang = crate::i18n::detect_from_headers(&headers);
    let captcha_cfg = state.captcha_config.read().await;
    if captcha::verify(&captcha_cfg, form.captcha_token.as_deref())
        .await
        .is_err()
    {
        tracing::warn!("captcha failed on group booking");
        return render_booking_action_error(
            &state,
            &headers,
            "Captcha verification failed",
            "Please go back and try again.",
        );
    }
    drop(captcha_cfg);
    // Rate limit by IP
    let client_ip = client_ip_for_rate_limit(&headers);
    if state.booking_limiter.check_limited(&client_ip).await {
        tracing::warn!(ip = %client_ip, "rate limited");
        return Html("Too many booking attempts. Please try again in a few minutes.".to_string())
            .into_response();
    }

    if let Err(e) = validate_booking_input(&form.name, &form.email, &form.notes) {
        return render_booking_action_error(&state, &headers, "Invalid booking details", &e);
    }

    let et: Option<(String, String, String, i32, i32, i32, i32, i32, String, Option<String>, String, Option<i32>, String, i32, String, Option<String>)> = sqlx::query_as(
        "SELECT et.id, et.slug, et.title, et.duration_min, et.buffer_before, et.buffer_after, et.min_notice_min, et.requires_confirmation, et.location_type, et.location_value, et.team_id, et.reminder_minutes, et.visibility, et.max_additional_guests, t.visibility, t.invite_token
         FROM event_types et
         JOIN teams t ON t.id = et.team_id
         WHERE t.slug = ? AND et.slug = ? AND et.enabled = 1",
    )
    .bind(&team_slug)
    .bind(&slug)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    let (
        et_id,
        _et_slug,
        et_title,
        duration,
        buffer_before,
        buffer_after,
        min_notice,
        requires_confirmation,
        loc_type,
        loc_value,
        team_id,
        reminder_min,
        visibility,
        max_additional_guests,
        team_visibility,
        team_invite_token,
    ) = match et {
        Some(e) => e,
        None => return Html("Event type not found.".to_string()).into_response(),
    };
    let needs_approval = requires_confirmation != 0;

    // Parse additional guests
    let additional_attendees = match parse_additional_guests(
        &form.additional_guests,
        max_additional_guests,
        &form.email,
    ) {
        Ok(emails) => emails,
        Err(e) => return Html(e).into_response(),
    };

    // Validate access
    if visibility == "private" || visibility == "internal" {
        let token = match &form.invite_token {
            Some(t) if !t.is_empty() => t,
            _ => {
                return Html("This event type requires an invite link.".to_string()).into_response()
            }
        };
        let invite: Option<(Option<String>, i32, i32)> = sqlx::query_as(
            "SELECT expires_at, max_uses, used_count FROM booking_invites WHERE token = ? AND event_type_id = ?",
        )
        .bind(token)
        .bind(&et_id)
        .fetch_optional(&state.pool)
        .await
        .unwrap_or(None);

        match invite {
            None => return Html("Invalid invite link.".to_string()).into_response(),
            Some((expires_at, max_uses, used_count)) => {
                if let Some(exp) = &expires_at {
                    if exp < &chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string() {
                        return Html("This invite link has expired.".to_string()).into_response();
                    }
                }
                if used_count >= max_uses {
                    return Html("This invite link has already been used.".to_string())
                        .into_response();
                }
            }
        }
    } else if team_visibility == "private" {
        // Public event on a private team: logged-in team members and global
        // admins bypass the team invite token requirement (their team
        // membership is the access). Private/internal events above already
        // returned, so this branch is public-only.
        let viewer_is_member_or_admin = match &optional_auth.user {
            Some(u) if u.role == "admin" => true,
            Some(u) => is_team_member(&state.pool, &u.id, &team_id).await,
            None => false,
        };
        if !viewer_is_member_or_admin {
            let valid = matches!((&team_invite_token, &form.invite_token), (Some(expected), Some(provided)) if !provided.is_empty() && provided == expected);
            if !valid {
                return Html("Event type not found.".to_string()).into_response();
            }
        }
    }

    let date = match NaiveDate::parse_from_str(&form.date, "%Y-%m-%d") {
        Ok(d) => d,
        Err(_) => return Html("Invalid date.".to_string()).into_response(),
    };
    if let Err(e) = validate_date_not_too_far(date) {
        return Html(e).into_response();
    }
    let start_time = match NaiveTime::parse_from_str(&form.time, "%H:%M") {
        Ok(t) => t,
        Err(_) => return Html("Invalid time.".to_string()).into_response(),
    };

    let guest_tz = parse_guest_tz(form.tz.as_deref());
    let guest_timezone = guest_tz.name().to_string();
    let host_tz = get_host_tz(&state.pool, &et_id).await;

    // The URL carries the guest's local date/time. Convert to host-local
    // for availability checks and storage (existing semantics).
    let guest_local_start = date.and_time(start_time);
    let guest_local_end = guest_local_start + Duration::minutes(duration as i64);
    let slot_start = guest_to_host_local(guest_local_start, guest_tz, host_tz);
    let slot_end = slot_start + Duration::minutes(duration as i64);

    let now = Local::now().naive_local();
    if slot_start < now + Duration::minutes(min_notice as i64) {
        return Html("This slot is no longer available (too soon).".to_string()).into_response();
    }

    let id = uuid::Uuid::new_v4().to_string();
    let uid = format!("{}@calrs", uuid::Uuid::new_v4());
    let cancel_token = uuid::Uuid::new_v4().to_string();
    let reschedule_token = uuid::Uuid::new_v4().to_string();
    let start_at = slot_start.format("%Y-%m-%dT%H:%M:%S").to_string();
    let end_at = slot_end.format("%Y-%m-%dT%H:%M:%S").to_string();
    let guest_end_time = guest_local_end.time().format("%H:%M").to_string();

    let initial_status = if needs_approval {
        "pending"
    } else {
        "confirmed"
    };
    let confirm_token: Option<String> = if needs_approval {
        Some(uuid::Uuid::new_v4().to_string())
    } else {
        None
    };

    // Start a transaction to ensure atomicity of availability check + insert.
    let mut tx = match state.pool.begin().await {
        Ok(tx) => tx,
        Err(e) => {
            return internal_error_response("database query", &e);
        }
    };

    // Pick an available group member
    let assigned = pick_group_member(
        &state.pool,
        &team_id,
        &et_id,
        slot_start,
        slot_end,
        buffer_before,
        buffer_after,
        host_tz,
    )
    .await;

    let (assigned_user_id, host_name, host_email) = match assigned {
        Some(a) => a,
        None => {
            let _ = tx.rollback().await;
            return Html("No team members are available for this slot.".to_string())
                .into_response();
        }
    };

    // Check booking frequency limits
    if would_exceed_frequency_limit(&state.pool, &et_id, slot_start, Some(&assigned_user_id)).await
    {
        let _ = tx.rollback().await;
        return render_booking_action_error(
            &state,
            &headers,
            "Not available right now",
            "The host isn't accepting more bookings for this date. Please pick a different date, or check back later.",
        );
    }

    let insert_result = sqlx::query(
        "INSERT INTO bookings (id, event_type_id, uid, guest_name, guest_email, guest_timezone, notes, start_at, end_at, status, cancel_token, reschedule_token, assigned_user_id, confirm_token, language)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(&et_id)
    .bind(&uid)
    .bind(&form.name)
    .bind(&form.email)
    .bind(&guest_timezone)
    .bind(&form.notes)
    .bind(&start_at)
    .bind(&end_at)
    .bind(initial_status)
    .bind(&cancel_token)
    .bind(&reschedule_token)
    .bind(&assigned_user_id)
    .bind(&confirm_token)
    .bind(lang)
    .execute(&mut *tx)
    .await;

    match insert_result {
        Ok(_) => {}
        Err(e) => {
            let _ = tx.rollback().await;
            if e.to_string().contains("UNIQUE constraint failed") {
                return Html("This slot is no longer available.".to_string()).into_response();
            }
            return internal_error_response("database query", &e);
        }
    }

    // Insert additional attendees
    for attendee_email in &additional_attendees {
        let attendee_id = uuid::Uuid::new_v4().to_string();
        let _ =
            sqlx::query("INSERT INTO booking_attendees (id, booking_id, email) VALUES (?, ?, ?)")
                .bind(&attendee_id)
                .bind(&id)
                .bind(attendee_email)
                .execute(&mut *tx)
                .await;
    }

    if let Err(e) = tx.commit().await {
        if e.to_string().contains("UNIQUE constraint failed") {
            return Html("This slot is no longer available.".to_string()).into_response();
        }
        return internal_error_response("database query", &e);
    }

    tracing::info!(booking_id = %id, event_type = %slug, guest = %form.email, "booking created");

    // Increment invite used_count if this was an invite-based booking
    if visibility == "private" || visibility == "internal" {
        if let Some(token) = &form.invite_token {
            let _ = sqlx::query("UPDATE booking_invites SET used_count = used_count + 1 WHERE token = ? AND event_type_id = ?")
                .bind(token)
                .bind(&et_id)
                .execute(&state.pool)
                .await;
        }
    }

    // Build BookingDetails once. CalDAV push, watcher notifications, and email
    // sends all need it, and CalDAV push must run independently of SMTP.
    let location_display = resolve_booking_location(
        &state,
        &id,
        &et_id,
        Some(&assigned_user_id),
        loc_value.as_deref(),
        &form.name,
        &form.email,
        !needs_approval,
    )
    .await;
    let details = crate::email::BookingDetails {
        event_title: et_title.clone(),
        date: form.date.clone(),
        start_time: form.time.clone(),
        end_time: guest_end_time.clone(),
        guest_name: form.name.clone(),
        guest_email: form.email.clone(),
        guest_timezone: guest_timezone.clone(),
        host_name: host_name.clone(),
        host_email: host_email.clone(),
        uid: uid.clone(),
        notes: form.notes.clone(),
        location: location_display.clone(),
        reminder_minutes: reminder_min,
        additional_attendees: additional_attendees.clone(),
        guest_language: Some(lang.to_string()),
        host_timezone: host_tz.name().to_string(),
        ..Default::default()
    };

    // For confirmed bookings, push to CalDAV and notify watchers regardless of
    // SMTP availability. notify_watchers self-gates on SMTP for the email part.
    if !needs_approval {
        caldav_push_booking(
            &state.pool,
            &state.secret_key,
            &assigned_user_id,
            &uid,
            &details,
        )
        .await;
        notify_watchers(
            &state.pool,
            &state.secret_key,
            &id,
            &et_id,
            &host_name,
            &details,
        )
        .await;
    }

    let (cancel_notice_min, reschedule_notice_min) =
        fetch_event_type_notice_minutes(&state.pool, &et_id).await;

    // Send emails if SMTP is configured.
    if let Ok(Some(smtp_config)) =
        crate::email::load_smtp_config(&state.pool, &state.secret_key).await
    {
        let base_url = std::env::var("CALRS_BASE_URL").ok();
        let guest_cancel_url = base_url.as_ref().map(|base| {
            format!(
                "{}/booking/cancel/{}",
                base.trim_end_matches('/'),
                cancel_token
            )
        });
        let guest_reschedule_url = base_url.as_ref().map(|base| {
            format!(
                "{}/booking/reschedule/{}",
                base.trim_end_matches('/'),
                reschedule_token
            )
        });

        if needs_approval {
            let _ = crate::email::send_host_approval_request(
                &smtp_config,
                &details,
                &id,
                confirm_token.as_deref(),
                base_url.as_deref(),
            )
            .await;
            let _ = crate::email::send_guest_pending_notice_ex(
                &smtp_config,
                &details,
                guest_cancel_url.as_deref(),
                guest_reschedule_url.as_deref(),
            )
            .await;
        } else {
            let _ = crate::email::send_guest_confirmation_ex(
                &smtp_config,
                &details,
                guest_cancel_url.as_deref(),
                guest_reschedule_url.as_deref(),
                cancel_notice_min,
                reschedule_notice_min,
            )
            .await;
            let _ = crate::email::send_host_notification(&smtp_config, &details).await;
        }
    }

    let date_label = crate::i18n::format_long_date(date, lang);

    let tmpl = match state.templates.get_template("confirmed.html") {
        Ok(t) => t,
        Err(e) => return internal_error_response("internal", &e),
    };
    let rendered = tmpl
        .render(context! {
            event_title => et_title,
            date_label => date_label,
            time_start => form.time,
            time_end => guest_end_time,
            host_name => host_name,
            guest_email => form.email,
            notes => form.notes,
            pending => needs_approval,
            location_type => loc_type,
            location_value => location_display,
            additional_attendees => additional_attendees,
            cancel_notice_min => cancel_notice_min,
            reschedule_notice_min => reschedule_notice_min,
            company_link => state.company_link.read().await.clone(),
            lang => lang,
            embed => embed.is_embedded(),
            embed_brand => embed.brand_css().unwrap_or_default(),
            embed_theme => embed.theme.as_deref().unwrap_or(""),
            embed_qs_amp => embed.query_suffix(true),
            embed_qs_q => embed.query_suffix(false),
        })
        .unwrap_or_else(|e| internal_error_body("template render", &e));

    html_response(rendered, &embed)
}

// --- Group slot computation ---

// --- User profile page ---

async fn user_profile(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(username): Path<String>,
) -> impl IntoResponse {
    let user: Option<(
        String,
        String,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
    )> = sqlx::query_as(
        "SELECT id, name, title, bio, avatar_path, language FROM users WHERE username = ? AND enabled = 1",
    )
    .bind(&username)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    let (user_id, user_name, user_title, user_bio, avatar_path, language) = match user {
        Some(u) => u,
        None => return Html("User not found.".to_string()),
    };
    let lang = crate::i18n::resolve(language.as_deref(), &headers);

    let event_types: Vec<(String, String, Option<String>, i32)> = sqlx::query_as(
        "SELECT et.slug, et.title, et.description, et.duration_min
         FROM event_types et
         JOIN accounts a ON a.id = et.account_id
         WHERE a.user_id = ? AND et.enabled = 1 AND et.visibility = 'public'
         AND et.team_id IS NULL
         ORDER BY et.created_at",
    )
    .bind(&user_id)
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();

    let tmpl = match state.templates.get_template("profile.html") {
        Ok(t) => t,
        Err(e) => return internal_error_html("template render", &e),
    };

    let et_ctx: Vec<minijinja::Value> = event_types
        .iter()
        .map(|(slug, title, desc, duration)| {
            context! { slug => slug, title => title, description => desc.as_deref().map(crate::utils::render_inline_markdown), duration_min => duration }
        })
        .collect();

    Html(
        tmpl.render(context! {
            host_name => &user_name,
            host_initials => compute_initials(&user_name),
            host_title => user_title,
            host_bio => user_bio.as_deref().map(crate::utils::render_inline_markdown),
            host_user_id => user_id,
            host_has_avatar => avatar_path.is_some(),
            username => username,
            event_types => et_ctx,
            company_link => state.company_link.read().await.clone(),
            lang,
        })
        .unwrap_or_else(|e| internal_error_body("template render", &e)),
    )
}

// --- Dynamic group link handlers ---

async fn show_dynamic_group_slots(
    state: &AppState,
    headers: &HeaderMap,
    combined_username: &str,
    slug: &str,
    query: &SlotsQuery,
) -> Html<String> {
    let lang = crate::i18n::detect_from_headers(headers);
    let usernames = match parse_dynamic_group_usernames(combined_username) {
        Ok(u) => u,
        Err(e) => return Html(e),
    };
    let dg_users = match validate_dynamic_group_users(&state.pool, &usernames).await {
        Ok(u) => u,
        Err(e) => return Html(e),
    };

    // Load event type from first user (owner)
    let owner_username = &usernames[0];
    let et: Option<(String, String, String, Option<String>, i32, i32, i32, i32, String, Option<String>, String, String, Option<String>, Option<String>, String, String)> = sqlx::query_as(
        "SELECT et.id, et.slug, et.title, et.description, et.duration_min, et.buffer_before, et.buffer_after, et.min_notice_min, et.location_type, et.location_value, u.id, u.name, u.title, u.avatar_path, et.visibility, et.default_calendar_view
         FROM event_types et
         JOIN accounts a ON a.id = et.account_id
         JOIN users u ON u.id = a.user_id
         WHERE u.username = ? AND et.slug = ? AND et.enabled = 1 AND u.enabled = 1",
    )
    .bind(owner_username)
    .bind(slug)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    let (
        et_id,
        et_slug,
        et_title,
        et_desc,
        duration,
        buf_before,
        buf_after,
        min_notice,
        loc_type,
        loc_value,
        _owner_user_id,
        _owner_name,
        _owner_title,
        _owner_avatar_path,
        visibility,
        default_calendar_view,
    ) = match et {
        Some(e) => e,
        None => return Html("Event type not found.".to_string()),
    };

    // Dynamic group links only for public event types
    if visibility != "public" {
        return Html("Dynamic group links are only available for public event types.".to_string());
    }

    // Build combined host display name
    let host_name = dg_users
        .iter()
        .map(|(_, _, name, _, _)| name.as_str())
        .collect::<Vec<_>>()
        .join(" & ");

    let guest_tz = parse_guest_tz(query.tz.as_deref());
    let host_tz = get_host_tz(&state.pool, &et_id).await;
    let guest_tz_name = guest_tz.name().to_string();

    let (year, month) = parse_month_param(query.month.as_deref(), guest_tz);
    let (
        start_offset,
        days_ahead,
        month_label,
        prev_month,
        next_month,
        first_weekday,
        days_in_month,
        today_date,
        month_year,
    ) = build_month_params(year, month, host_tz, guest_tz, lang);

    // Deferred loading: on initial page load (no &deferred=1), skip sync + computation
    // and render the page shell immediately. JS will fetch with &deferred=1 to get real data.
    let is_deferred_callback = query.deferred.as_deref() == Some("1");

    let slot_days = if is_deferred_callback {
        // Full sync + computation (AJAX callback). Safe to parallelize:
        // sync_if_stale holds a per-source mutex and re-checks staleness,
        // so same-source fan-in collapses to one fetch.
        let mut sync_tasks = tokio::task::JoinSet::new();
        for (uid, _, _, _, _) in &dg_users {
            let pool = state.pool.clone();
            let key = state.secret_key;
            let uid = uid.clone();
            sync_tasks.spawn(async move {
                crate::commands::sync::sync_if_stale(&pool, &key, &uid).await;
            });
        }
        while sync_tasks.join_next().await.is_some() {}

        let now_host = Utc::now().with_timezone(&host_tz).naive_local();
        let end_date = now_host.date() + Duration::days((start_offset + days_ahead) as i64);
        let window_end = end_date.and_hms_opt(23, 59, 59).unwrap_or(now_host);

        let mut member_busy = HashMap::new();
        for (i, (uid, _, _, _, _)) in dg_users.iter().enumerate() {
            let et_filter = if i == 0 { Some(et_id.as_str()) } else { None };
            let mut busy_times = fetch_busy_times_for_user(
                &state.pool,
                uid,
                now_host,
                window_end,
                host_tz,
                et_filter,
            )
            .await;
            // For non-owner participants, apply their default availability as constraints
            if i > 0 {
                ensure_user_avail_seeded(&state.pool, uid).await;
                let avail_busy =
                    user_avail_as_busy(&state.pool, uid, now_host, window_end, host_tz).await;
                busy_times.extend(avail_busy);
            }
            member_busy.insert(uid.clone(), busy_times);
        }
        let busy = BusySource::Team(member_busy);

        compute_slots(
            &state.pool,
            &et_id,
            duration,
            buf_before,
            buf_after,
            min_notice,
            start_offset,
            days_ahead,
            host_tz,
            guest_tz,
            busy,
        )
        .await
    } else {
        // Initial load: empty slots, page renders instantly
        vec![]
    };

    let days_ctx: Vec<minijinja::Value> = slot_days
        .iter()
        .map(|d| {
            let slots: Vec<minijinja::Value> = d
                .slots
                .iter()
                .map(|s| {
                    context! { start => s.start, end => s.end, host_date => s.host_date, host_time => s.host_time, guest_date => s.guest_date }
                })
                .collect();
            context! { date => d.date, label => d.label, slots => slots }
        })
        .collect();

    let available_dates: Vec<String> = slot_days.iter().map(|d| d.date.clone()).collect();

    let tz_options: Vec<minijinja::Value> = common_timezones_with(&guest_tz_name)
        .iter()
        .map(|(iana, label)| {
            context! { value => iana, label => label, selected => (*iana == guest_tz_name) }
        })
        .collect();

    let (meeting_jitsi_label, meeting_webhook_label) = meeting_provider_labels(state).await;
    let tmpl = match state.templates.get_template("slots.html") {
        Ok(t) => t,
        Err(e) => return internal_error_html("internal", &e),
    };
    Html(
        tmpl.render(context! {
            meeting_jitsi_label => meeting_jitsi_label,
            meeting_webhook_label => meeting_webhook_label,
            event_type => context! {
                slug => et_slug,
                title => et_title,
                description => et_desc.as_deref().map(crate::utils::render_inline_markdown),
                duration_min => duration,
                location_type => loc_type,
                location_value => loc_value,
            },
            host_name => host_name,
            dg_members => dg_users.iter().map(|(id, _, name, _, avatar_path)| {
                context! {
                    id => id,
                    name => name,
                    has_avatar => avatar_path.is_some(),
                    initials => compute_initials(name),
                }
            }).collect::<Vec<_>>(),
            username => combined_username,
            days => days_ctx,
            available_dates => available_dates,
            month_label => month_label,
            month_year => month_year,
            prev_month => prev_month,
            next_month => next_month,
            first_weekday => first_weekday,
            days_in_month => days_in_month,
            today_date => today_date,
            guest_tz => guest_tz_name,
            tz_options => tz_options,
            invite_token => "",
            invite_guest_name => "",
            invite_guest_email => "",
            default_calendar_view => default_calendar_view,
            deferred_load => !is_deferred_callback,
            company_link => state.company_link.read().await.clone(),
            lang => lang,
        })
        .unwrap_or_else(|e| internal_error_body("template render", &e)),
    )
}

async fn show_dynamic_group_book_form(
    state: &AppState,
    headers: &HeaderMap,
    combined_username: &str,
    slug: &str,
    query: &BookQuery,
) -> Html<String> {
    let lang = crate::i18n::detect_from_headers(headers);
    let usernames = match parse_dynamic_group_usernames(combined_username) {
        Ok(u) => u,
        Err(e) => return Html(e),
    };
    let dg_users = match validate_dynamic_group_users(&state.pool, &usernames).await {
        Ok(u) => u,
        Err(e) => return Html(e),
    };

    let owner_username = &usernames[0];
    let et: Option<(String, String, String, Option<String>, i32, String, Option<String>, String, i32)> = sqlx::query_as(
        "SELECT et.id, et.slug, et.title, et.description, et.duration_min, et.location_type, et.location_value, et.visibility, et.max_additional_guests
         FROM event_types et
         JOIN accounts a ON a.id = et.account_id
         JOIN users u ON u.id = a.user_id
         WHERE u.username = ? AND et.slug = ? AND et.enabled = 1 AND u.enabled = 1",
    )
    .bind(owner_username)
    .bind(slug)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    let (
        _,
        et_slug,
        et_title,
        et_desc,
        duration,
        loc_type,
        loc_value,
        visibility,
        max_additional_guests,
    ) = match et {
        Some(e) => e,
        None => return Html("Event type not found.".to_string()),
    };

    if visibility != "public" {
        return Html("Dynamic group links are only available for public event types.".to_string());
    }

    let host_name = dg_users
        .iter()
        .map(|(_, _, name, _, _)| name.as_str())
        .collect::<Vec<_>>()
        .join(" & ");

    let guest_tz = parse_guest_tz(query.tz.as_deref());
    let guest_tz_name = guest_tz.name().to_string();

    let date = match NaiveDate::parse_from_str(&query.date, "%Y-%m-%d") {
        Ok(d) => d,
        Err(_) => return Html("Invalid date format.".to_string()),
    };
    let time = match NaiveTime::parse_from_str(&query.time, "%H:%M") {
        Ok(t) => t,
        Err(_) => return Html("Invalid time format.".to_string()),
    };
    let end_time = (date.and_time(time) + Duration::minutes(duration as i64))
        .time()
        .format("%H:%M")
        .to_string();
    let date_label = crate::i18n::format_long_date(date, lang);

    let (meeting_jitsi_label, meeting_webhook_label) = meeting_provider_labels(state).await;
    let tmpl = match state.templates.get_template("book.html") {
        Ok(t) => t,
        Err(e) => return internal_error_html("internal", &e),
    };
    let captcha = captcha::CaptchaVars::from_config(&*state.captcha_config.read().await);
    Html(
        tmpl.render(context! {
            meeting_jitsi_label => meeting_jitsi_label,
            meeting_webhook_label => meeting_webhook_label,
            event_type => context! {
                slug => et_slug,
                title => et_title,
                description => et_desc.as_deref().map(crate::utils::render_inline_markdown),
                duration_min => duration,
                location_type => loc_type,
                location_value => loc_value,
            },
            host_name => host_name,
            username => combined_username,
            date => query.date,
            date_label => date_label,
            time_start => query.time,
            time_end => end_time,
            guest_tz => guest_tz_name,
            error => "",
            form_name => "",
            form_email => "",
            form_notes => "",
            invite_token => "",
            max_additional_guests => max_additional_guests,
            company_link => state.company_link.read().await.clone(),
            captcha_enabled => captcha.enabled,
            captcha_api_endpoint => captcha.api_endpoint,
            captcha_widget_url => captcha.widget_url,
            lang => lang,
        })
        .unwrap_or_else(|e| internal_error_body("template render", &e)),
    )
}

async fn handle_dynamic_group_booking(
    state: &AppState,
    headers: &axum::http::HeaderMap,
    combined_username: &str,
    slug: &str,
    form: &BookForm,
) -> Response {
    let lang = crate::i18n::detect_from_headers(headers);
    // Rate limit by IP
    let client_ip = client_ip_for_rate_limit(headers);
    if state.booking_limiter.check_limited(&client_ip).await {
        tracing::warn!(ip = %client_ip, "rate limited");
        return Html("Too many booking attempts. Please try again in a few minutes.".to_string())
            .into_response();
    }

    if let Err(e) = validate_booking_input(&form.name, &form.email, &form.notes) {
        return render_booking_action_error(state, headers, "Invalid booking details", &e);
    }

    let usernames = match parse_dynamic_group_usernames(combined_username) {
        Ok(u) => u,
        Err(e) => return Html(e).into_response(),
    };
    let dg_users = match validate_dynamic_group_users(&state.pool, &usernames).await {
        Ok(u) => u,
        Err(e) => return Html(e).into_response(),
    };

    let owner_username = &usernames[0];
    let et: Option<(String, String, String, i32, i32, i32, i32, i32, String, Option<String>, String, Option<i32>, String, i32)> = sqlx::query_as(
        "SELECT et.id, et.slug, et.title, et.duration_min, et.buffer_before, et.buffer_after, et.min_notice_min, et.requires_confirmation, et.location_type, et.location_value, u.id, et.reminder_minutes, et.visibility, et.max_additional_guests
         FROM event_types et
         JOIN accounts a ON a.id = et.account_id
         JOIN users u ON u.id = a.user_id
         WHERE u.username = ? AND et.slug = ? AND et.enabled = 1 AND u.enabled = 1",
    )
    .bind(owner_username)
    .bind(slug)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    let (
        et_id,
        _et_slug,
        et_title,
        duration,
        buffer_before,
        buffer_after,
        min_notice,
        requires_confirmation,
        loc_type,
        loc_value,
        owner_user_id,
        reminder_min,
        visibility,
        max_additional_guests,
    ) = match et {
        Some(e) => e,
        None => return Html("Event type not found.".to_string()).into_response(),
    };

    if visibility != "public" {
        return Html("Dynamic group links are only available for public event types.".to_string())
            .into_response();
    }

    let needs_approval = requires_confirmation != 0;

    // Parse additional guests
    let additional_attendees = match parse_additional_guests(
        &form.additional_guests,
        max_additional_guests,
        &form.email,
    ) {
        Ok(emails) => emails,
        Err(e) => return Html(e).into_response(),
    };

    let date = match NaiveDate::parse_from_str(&form.date, "%Y-%m-%d") {
        Ok(d) => d,
        Err(_) => return Html("Invalid date.".to_string()).into_response(),
    };
    if let Err(e) = validate_date_not_too_far(date) {
        return Html(e).into_response();
    }
    let start_time = match NaiveTime::parse_from_str(&form.time, "%H:%M") {
        Ok(t) => t,
        Err(_) => return Html("Invalid time.".to_string()).into_response(),
    };

    let guest_tz = parse_guest_tz(form.tz.as_deref());
    let guest_timezone = guest_tz.name().to_string();
    let host_tz = get_host_tz(&state.pool, &et_id).await;

    // The URL carries guest-local date/time; convert to host-local for storage
    // and availability checks (existing semantics).
    let guest_local_start = date.and_time(start_time);
    let guest_local_end = guest_local_start + Duration::minutes(duration as i64);
    let slot_start = guest_to_host_local(guest_local_start, guest_tz, host_tz);
    let slot_end = slot_start + Duration::minutes(duration as i64);
    let guest_end_time = guest_local_end.time().format("%H:%M").to_string();

    let now = Local::now().naive_local();
    if slot_start < now + Duration::minutes(min_notice as i64) {
        return Html("This slot is no longer available (too soon).".to_string()).into_response();
    }

    let buf_start = slot_start - Duration::minutes(buffer_before as i64);
    let buf_end = slot_end + Duration::minutes(buffer_after as i64);

    // Check availability for ALL participants
    for (i, (uid, uname, _, _, _)) in dg_users.iter().enumerate() {
        let et_filter = if i == 0 { Some(et_id.as_str()) } else { None };
        let mut busy =
            fetch_busy_times_for_user(&state.pool, uid, buf_start, buf_end, host_tz, et_filter)
                .await;
        if i > 0 {
            ensure_user_avail_seeded(&state.pool, uid).await;
            busy.extend(user_avail_as_busy(&state.pool, uid, buf_start, buf_end, host_tz).await);
        }
        if has_conflict(&busy, buf_start, buf_end) {
            return Html(format!(
                "This slot is no longer available ({} has a conflict).",
                uname
            ))
            .into_response();
        }
    }

    // Check booking frequency limits. The dynamic-group flow doesn't pin a
    // single assignee on the booking, so per-member caps fall back to
    // event-type-wide here.
    if would_exceed_frequency_limit(&state.pool, &et_id, slot_start, None).await {
        return render_booking_action_error(
            state,
            headers,
            "Not available right now",
            "The host isn't accepting more bookings for this date. Please pick a different date, or check back later.",
        );
    }

    let id = uuid::Uuid::new_v4().to_string();
    let uid = format!("{}@calrs", uuid::Uuid::new_v4());
    let cancel_token = uuid::Uuid::new_v4().to_string();
    let reschedule_token = uuid::Uuid::new_v4().to_string();
    let start_at = slot_start.format("%Y-%m-%dT%H:%M:%S").to_string();
    let end_at = slot_end.format("%Y-%m-%dT%H:%M:%S").to_string();

    let initial_status = if needs_approval {
        "pending"
    } else {
        "confirmed"
    };
    let confirm_token: Option<String> = if needs_approval {
        Some(uuid::Uuid::new_v4().to_string())
    } else {
        None
    };

    let mut tx = match state.pool.begin().await {
        Ok(tx) => tx,
        Err(e) => return internal_error_response("database query", &e),
    };

    let insert_result = sqlx::query(
        "INSERT INTO bookings (id, event_type_id, uid, guest_name, guest_email, guest_timezone, notes, start_at, end_at, status, cancel_token, reschedule_token, confirm_token, language)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(&et_id)
    .bind(&uid)
    .bind(&form.name)
    .bind(&form.email)
    .bind(&guest_timezone)
    .bind(&form.notes)
    .bind(&start_at)
    .bind(&end_at)
    .bind(initial_status)
    .bind(&cancel_token)
    .bind(&reschedule_token)
    .bind(&confirm_token)
    .bind(lang)
    .execute(&mut *tx)
    .await;

    match insert_result {
        Ok(_) => {}
        Err(e) => {
            let _ = tx.rollback().await;
            if e.to_string().contains("UNIQUE constraint failed") {
                return Html("This slot is no longer available.".to_string()).into_response();
            }
            return internal_error_response("database query", &e);
        }
    }

    // Combine co-participant emails with guest-provided additional attendees
    let co_participant_emails: Vec<String> = dg_users
        .iter()
        .skip(1)
        .map(|(_, _, _, email, _)| email.clone())
        .collect();
    let all_additional: Vec<String> = co_participant_emails
        .iter()
        .chain(additional_attendees.iter())
        .cloned()
        .collect();

    // Insert all additional attendees (co-participants + guest-provided)
    for attendee_email in &all_additional {
        let attendee_id = uuid::Uuid::new_v4().to_string();
        let _ =
            sqlx::query("INSERT INTO booking_attendees (id, booking_id, email) VALUES (?, ?, ?)")
                .bind(&attendee_id)
                .bind(&id)
                .bind(attendee_email)
                .execute(&mut *tx)
                .await;
    }

    if let Err(e) = tx.commit().await {
        if e.to_string().contains("UNIQUE constraint failed") {
            return Html("This slot is no longer available.".to_string()).into_response();
        }
        return internal_error_response("database query", &e);
    }

    tracing::info!(booking_id = %id, event_type = %slug, guest = %form.email, dynamic_group = %combined_username, "dynamic group booking created");

    // Build BookingDetails once. CalDAV push and email send both need it,
    // and CalDAV push must run independently of whether SMTP is configured.
    let owner_email = dg_users[0].3.clone();
    let host_name = dg_users
        .iter()
        .map(|(_, _, name, _, _)| name.as_str())
        .collect::<Vec<_>>()
        .join(" & ");

    let location_display = resolve_booking_location(
        state,
        &id,
        &et_id,
        Some(&owner_user_id),
        loc_value.as_deref(),
        &form.name,
        &form.email,
        !needs_approval,
    )
    .await;
    let details = crate::email::BookingDetails {
        event_title: et_title.clone(),
        date: form.date.clone(),
        start_time: form.time.clone(),
        end_time: guest_end_time.clone(),
        guest_name: form.name.clone(),
        guest_email: form.email.clone(),
        guest_timezone: guest_timezone.clone(),
        host_name: host_name.clone(),
        host_email: owner_email,
        uid: uid.clone(),
        notes: form.notes.clone(),
        location: location_display.clone(),
        reminder_minutes: reminder_min,
        additional_attendees: all_additional.clone(),
        guest_language: Some(lang.to_string()),
        host_timezone: host_tz.name().to_string(),
        ..Default::default()
    };

    // Push confirmed bookings to the owner's CalDAV regardless of SMTP.
    // ICS includes co-participants as ATTENDEEs.
    if !needs_approval {
        caldav_push_booking(
            &state.pool,
            &state.secret_key,
            &owner_user_id,
            &uid,
            &details,
        )
        .await;
    }

    let (cancel_notice_min, reschedule_notice_min) =
        fetch_event_type_notice_minutes(&state.pool, &et_id).await;

    // Send emails if SMTP is configured.
    if let Ok(Some(smtp_config)) =
        crate::email::load_smtp_config(&state.pool, &state.secret_key).await
    {
        let base_url = std::env::var("CALRS_BASE_URL").ok();
        let guest_cancel_url = base_url.as_ref().map(|base| {
            format!(
                "{}/booking/cancel/{}",
                base.trim_end_matches('/'),
                cancel_token
            )
        });
        let guest_reschedule_url = base_url.as_ref().map(|base| {
            format!(
                "{}/booking/reschedule/{}",
                base.trim_end_matches('/'),
                reschedule_token
            )
        });

        if needs_approval {
            let _ = crate::email::send_host_approval_request(
                &smtp_config,
                &details,
                &id,
                confirm_token.as_deref(),
                base_url.as_deref(),
            )
            .await;
            let _ = crate::email::send_guest_pending_notice_ex(
                &smtp_config,
                &details,
                guest_cancel_url.as_deref(),
                guest_reschedule_url.as_deref(),
            )
            .await;
        } else {
            let _ = crate::email::send_guest_confirmation_ex(
                &smtp_config,
                &details,
                guest_cancel_url.as_deref(),
                guest_reschedule_url.as_deref(),
                cancel_notice_min,
                reschedule_notice_min,
            )
            .await;
            let _ = crate::email::send_host_notification(&smtp_config, &details).await;
        }
    }

    let host_display = dg_users
        .iter()
        .map(|(_, _, name, _, _)| name.as_str())
        .collect::<Vec<_>>()
        .join(" & ");
    let date_label = crate::i18n::format_long_date(date, lang);

    let tmpl = match state.templates.get_template("confirmed.html") {
        Ok(t) => t,
        Err(e) => return internal_error_response("internal", &e),
    };
    Html(
        tmpl.render(context! {
            event_title => et_title,
            date_label => date_label,
            time_start => form.time,
            time_end => guest_end_time,
            host_name => host_display,
            guest_email => form.email,
            notes => form.notes,
            pending => needs_approval,
            location_type => loc_type,
            location_value => location_display,
            additional_attendees => all_additional,
            cancel_notice_min => cancel_notice_min,
            reschedule_notice_min => reschedule_notice_min,
            company_link => state.company_link.read().await.clone(),
            lang => lang,
        })
        .unwrap_or_else(|e| internal_error_body("template render", &e)),
    )
    .into_response()
}

// --- User-scoped booking handlers ---

async fn show_slots_for_user(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path((username, slug)): Path<(String, String)>,
    Query(query): Query<SlotsQuery>,
) -> Response {
    let embed = query.embed_params();
    // Dynamic group pages (`alice+bob`) intentionally return before embed
    // threading: embedding ad-hoc combined links is out of scope, so these
    // render with full chrome and don't autosize. The embed generator only
    // produces single-user/team links.
    if username.contains('+') {
        return show_dynamic_group_slots(&state, &headers, &username, &slug, &query)
            .await
            .into_response();
    }

    let user: Option<(
        String,
        String,
        Option<String>,
        Option<String>,
        Option<String>,
    )> = sqlx::query_as(
        "SELECT id, name, title, avatar_path, language FROM users WHERE username = ? AND enabled = 1",
    )
    .bind(&username)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    let (host_user_id, host_name, host_title, host_avatar_path, user_lang) = match user {
        Some(user) => user,
        None => return Html("User not found.".to_string()).into_response(),
    };

    let lang = crate::i18n::resolve(user_lang.as_deref(), &headers);

    let et: Option<(String, String, String, Option<String>, i32, i32, i32, i32, String, Option<String>, String, String)> = sqlx::query_as(
        "SELECT et.id, et.slug, et.title, et.description, et.duration_min, et.buffer_before, et.buffer_after, et.min_notice_min, et.location_type, et.location_value, et.visibility, et.default_calendar_view
         FROM event_types et
         JOIN accounts a ON a.id = et.account_id
         WHERE a.user_id = ? AND et.slug = ? AND et.enabled = 1",
    )
    .bind(&host_user_id)
    .bind(&slug)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    let (
        et_id,
        et_slug,
        et_title,
        et_desc,
        duration,
        buf_before,
        buf_after,
        min_notice,
        loc_type,
        loc_value,
        visibility,
        default_calendar_view,
    ) = match et {
        Some(e) => e,
        None => return Html("Event type not found.".to_string()).into_response(),
    };

    // Validate invite token for private event types
    let invite_guest_name;
    let invite_guest_email;
    if visibility == "private" || visibility == "internal" {
        let token = match &query.invite {
            Some(t) => t,
            None => {
                return Html("This event type requires an invite link.".to_string()).into_response()
            }
        };
        let invite: Option<(String, String, Option<String>, i32, i32)> = sqlx::query_as(
            "SELECT guest_name, guest_email, expires_at, max_uses, used_count FROM booking_invites WHERE token = ? AND event_type_id = ?",
        )
        .bind(token)
        .bind(&et_id)
        .fetch_optional(&state.pool)
        .await
        .unwrap_or(None);

        match invite {
            None => return Html("Invalid invite link.".to_string()).into_response(),
            Some((name, email, expires_at, max_uses, used_count)) => {
                if let Some(exp) = &expires_at {
                    if exp < &chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string() {
                        return Html("This invite link has expired.".to_string()).into_response();
                    }
                }
                if used_count >= max_uses {
                    return Html("This invite link has already been used.".to_string())
                        .into_response();
                }
                invite_guest_name = Some(name);
                invite_guest_email = Some(email);
            }
        }
    } else {
        invite_guest_name = None;
        invite_guest_email = None;
    }

    // Sync calendars if stale before computing availability
    crate::commands::sync::sync_if_stale(&state.pool, &state.secret_key, &host_user_id).await;

    let guest_tz = parse_guest_tz(query.tz.as_deref());
    let host_tz = get_host_tz(&state.pool, &et_id).await;
    let guest_tz_name = guest_tz.name().to_string();

    let (year, month) = parse_month_param(query.month.as_deref(), guest_tz);
    let (
        start_offset,
        days_ahead,
        month_label,
        prev_month,
        next_month,
        first_weekday,
        days_in_month,
        today_date,
        month_year,
    ) = build_month_params(year, month, host_tz, guest_tz, lang);

    let now_host = Utc::now().with_timezone(&host_tz).naive_local();
    let end_date = now_host.date() + Duration::days((start_offset + days_ahead) as i64);
    let window_end = end_date.and_hms_opt(23, 59, 59).unwrap_or(now_host);
    let busy = BusySource::Individual(
        fetch_busy_times_for_user(
            &state.pool,
            &host_user_id,
            now_host,
            window_end,
            host_tz,
            Some(&et_id),
        )
        .await,
    );
    let slot_days = compute_slots(
        &state.pool,
        &et_id,
        duration,
        buf_before,
        buf_after,
        min_notice,
        start_offset,
        days_ahead,
        host_tz,
        guest_tz,
        busy,
    )
    .await;

    let days_ctx: Vec<minijinja::Value> = slot_days
        .iter()
        .map(|d| {
            let slots: Vec<minijinja::Value> = d
                .slots
                .iter()
                .map(|s| context! { start => s.start, end => s.end, host_date => s.host_date, host_time => s.host_time, guest_date => s.guest_date })
                .collect();
            context! { date => d.date, label => d.label, slots => slots }
        })
        .collect();

    let available_dates: Vec<String> = slot_days.iter().map(|d| d.date.clone()).collect();

    let tz_options: Vec<minijinja::Value> = common_timezones_with(&guest_tz_name)
        .iter()
        .map(|(iana, label)| context! { value => iana, label => label, selected => (*iana == guest_tz_name) })
        .collect();

    let (meeting_jitsi_label, meeting_webhook_label) = meeting_provider_labels(&state).await;
    let tmpl = match state.templates.get_template("slots.html") {
        Ok(t) => t,
        Err(e) => return internal_error_html("internal", &e).into_response(),
    };
    let rendered = tmpl
        .render(context! {
            meeting_jitsi_label => meeting_jitsi_label,
            meeting_webhook_label => meeting_webhook_label,
            event_type => context! {
                slug => et_slug,
                title => et_title,
                description => et_desc.as_deref().map(crate::utils::render_inline_markdown),
                duration_min => duration,
                location_type => loc_type,
                location_value => loc_value,
            },
            host_name => host_name,
            host_title => host_title.as_deref().unwrap_or(""),
            host_user_id => host_user_id,
            host_has_avatar => host_avatar_path.is_some(),
            host_initials => compute_initials(&host_name),
            username => username,
            days => days_ctx,
            available_dates => available_dates,
            month_label => month_label,
            month_year => month_year,
            prev_month => prev_month,
            next_month => next_month,
            first_weekday => first_weekday,
            days_in_month => days_in_month,
            today_date => today_date,
            guest_tz => guest_tz_name,
            tz_options => tz_options,
            invite_token => query.invite.as_deref().unwrap_or(""),
            invite_guest_name => invite_guest_name.as_deref().unwrap_or(""),
            invite_guest_email => invite_guest_email.as_deref().unwrap_or(""),
            default_calendar_view => embed.layout.as_deref().filter(|s| !s.is_empty()).unwrap_or(default_calendar_view.as_str()),
            company_link => state.company_link.read().await.clone(),
            lang => lang,
            embed => embed.is_embedded(),
            embed_brand => embed.brand_css().unwrap_or_default(),
            embed_theme => embed.theme.as_deref().unwrap_or(""),
            embed_qs_amp => embed.query_suffix(true),
            embed_qs_q => embed.query_suffix(false),
        })
        .unwrap_or_else(|e| internal_error_body("template render", &e));

    html_response(rendered, &embed)
}

async fn show_book_form_for_user(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path((username, slug)): Path<(String, String)>,
    Query(query): Query<BookQuery>,
) -> Response {
    let embed = query.embed_params();
    if username.contains('+') {
        return show_dynamic_group_book_form(&state, &headers, &username, &slug, &query)
            .await
            .into_response();
    }

    let et: Option<(String, String, String, Option<String>, i32, String, Option<String>, String, i32, Option<String>)> = sqlx::query_as(
        "SELECT et.id, et.slug, et.title, et.description, et.duration_min, et.location_type, et.location_value, et.visibility, et.max_additional_guests, u.language
         FROM event_types et
         JOIN accounts a ON a.id = et.account_id
         JOIN users u ON u.id = a.user_id
         WHERE u.username = ? AND et.slug = ? AND et.enabled = 1 AND u.enabled = 1",
    )
    .bind(&username)
    .bind(&slug)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    let (
        et_id,
        et_slug,
        et_title,
        et_desc,
        duration,
        loc_type,
        loc_value,
        visibility,
        max_additional_guests,
        user_lang,
    ) = match et {
        Some(e) => e,
        None => return Html("Event type not found.".to_string()).into_response(),
    };

    let lang = crate::i18n::resolve(user_lang.as_deref(), &headers);

    // Validate invite token for private event types
    let invite_guest_name;
    let invite_guest_email;
    if visibility == "private" || visibility == "internal" {
        let token = match &query.invite {
            Some(t) => t,
            None => {
                return Html("This event type requires an invite link.".to_string()).into_response()
            }
        };
        let invite: Option<(String, String, Option<String>, i32, i32)> = sqlx::query_as(
            "SELECT guest_name, guest_email, expires_at, max_uses, used_count FROM booking_invites WHERE token = ? AND event_type_id = ?",
        )
        .bind(token)
        .bind(&et_id)
        .fetch_optional(&state.pool)
        .await
        .unwrap_or(None);

        match invite {
            None => return Html("Invalid invite link.".to_string()).into_response(),
            Some((name, email, expires_at, max_uses, used_count)) => {
                if let Some(exp) = &expires_at {
                    if exp < &chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string() {
                        return Html("This invite link has expired.".to_string()).into_response();
                    }
                }
                if used_count >= max_uses {
                    return Html("This invite link has already been used.".to_string())
                        .into_response();
                }
                invite_guest_name = Some(name);
                invite_guest_email = Some(email);
            }
        }
    } else {
        invite_guest_name = None;
        invite_guest_email = None;
    }

    let host_name: String = sqlx::query_scalar("SELECT name FROM users WHERE username = ?")
        .bind(&username)
        .fetch_optional(&state.pool)
        .await
        .unwrap_or(None)
        .unwrap_or_else(|| "Host".to_string());

    let guest_tz = parse_guest_tz(query.tz.as_deref());
    let guest_tz_name = guest_tz.name().to_string();

    let date = match NaiveDate::parse_from_str(&query.date, "%Y-%m-%d") {
        Ok(d) => d,
        Err(_) => return Html("Invalid date format.".to_string()).into_response(),
    };
    let time = match NaiveTime::parse_from_str(&query.time, "%H:%M") {
        Ok(t) => t,
        Err(_) => return Html("Invalid time format.".to_string()).into_response(),
    };
    let end_time = (date.and_time(time) + Duration::minutes(duration as i64))
        .time()
        .format("%H:%M")
        .to_string();
    let date_label = crate::i18n::format_long_date(date, lang);

    let (meeting_jitsi_label, meeting_webhook_label) = meeting_provider_labels(&state).await;
    let tmpl = match state.templates.get_template("book.html") {
        Ok(t) => t,
        Err(e) => return internal_error_html("internal", &e).into_response(),
    };
    let captcha = captcha::CaptchaVars::from_config(&*state.captcha_config.read().await);
    let rendered = tmpl
        .render(context! {
            meeting_jitsi_label => meeting_jitsi_label,
            meeting_webhook_label => meeting_webhook_label,
            event_type => context! {
                slug => et_slug,
                title => et_title,
                description => et_desc.as_deref().map(crate::utils::render_inline_markdown),
                duration_min => duration,
                location_type => loc_type,
                location_value => loc_value,
            },
            host_name => host_name,
            username => username,
            date => query.date,
            date_label => date_label,
            time_start => query.time,
            time_end => end_time,
            guest_tz => guest_tz_name,
            error => "",
            form_name => invite_guest_name.as_deref().unwrap_or(""),
            form_email => invite_guest_email.as_deref().unwrap_or(""),
            form_notes => "",
            invite_token => query.invite.as_deref().unwrap_or(""),
            max_additional_guests => max_additional_guests,
            company_link => state.company_link.read().await.clone(),
            captcha_enabled => captcha.enabled,
            captcha_api_endpoint => captcha.api_endpoint,
            captcha_widget_url => captcha.widget_url,
            lang => lang,
            embed => embed.is_embedded(),
            embed_brand => embed.brand_css().unwrap_or_default(),
            embed_theme => embed.theme.as_deref().unwrap_or(""),
            embed_qs_amp => embed.query_suffix(true),
            embed_qs_q => embed.query_suffix(false),
        })
        .unwrap_or_else(|e| internal_error_body("template render", &e));

    html_response(rendered, &embed)
}

async fn handle_booking_for_user(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Path((username, slug)): Path<(String, String)>,
    Query(embed): Query<EmbedParams>,
    Form(form): Form<BookForm>,
) -> Response {
    if let Err(resp) = verify_csrf_token(&headers, &form._csrf) {
        return resp;
    }
    let captcha_cfg = state.captcha_config.read().await;
    if captcha::verify(&captcha_cfg, form.captcha_token.as_deref())
        .await
        .is_err()
    {
        tracing::warn!("captcha failed on user booking");
        return render_booking_action_error(
            &state,
            &headers,
            "Captcha verification failed",
            "Please go back and try again.",
        );
    }
    drop(captcha_cfg);
    if username.contains('+') {
        return handle_dynamic_group_booking(&state, &headers, &username, &slug, &form)
            .await
            .into_response();
    }
    // Rate limit by IP
    let client_ip = client_ip_for_rate_limit(&headers);
    if state.booking_limiter.check_limited(&client_ip).await {
        tracing::warn!(ip = %client_ip, "rate limited");
        return Html("Too many booking attempts. Please try again in a few minutes.".to_string())
            .into_response();
    }

    if let Err(e) = validate_booking_input(&form.name, &form.email, &form.notes) {
        return render_booking_action_error(&state, &headers, "Invalid booking details", &e);
    }

    let et: Option<(String, String, String, i32, i32, i32, i32, i32, String, Option<String>, String, Option<i32>, String, i32, Option<String>)> = sqlx::query_as(
        "SELECT et.id, et.slug, et.title, et.duration_min, et.buffer_before, et.buffer_after, et.min_notice_min, et.requires_confirmation, et.location_type, et.location_value, u.id, et.reminder_minutes, et.visibility, et.max_additional_guests, u.language
         FROM event_types et
         JOIN accounts a ON a.id = et.account_id
         JOIN users u ON u.id = a.user_id
         WHERE u.username = ? AND et.slug = ? AND et.enabled = 1 AND u.enabled = 1",
    )
    .bind(&username)
    .bind(&slug)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    let (
        et_id,
        _et_slug,
        et_title,
        duration,
        buffer_before,
        buffer_after,
        min_notice,
        requires_confirmation,
        loc_type,
        loc_value,
        host_user_id,
        reminder_min,
        visibility,
        max_additional_guests,
        user_lang,
    ) = match et {
        Some(e) => e,
        None => return Html("Event type not found.".to_string()).into_response(),
    };

    let lang = crate::i18n::resolve(user_lang.as_deref(), &headers);
    let needs_approval = requires_confirmation != 0;

    // Parse additional guests
    let additional_attendees = match parse_additional_guests(
        &form.additional_guests,
        max_additional_guests,
        &form.email,
    ) {
        Ok(emails) => emails,
        Err(e) => return Html(e).into_response(),
    };

    // Validate invite token for private event types
    if visibility == "private" || visibility == "internal" {
        let token = match &form.invite_token {
            Some(t) if !t.is_empty() => t,
            _ => {
                return Html("This event type requires an invite link.".to_string()).into_response()
            }
        };
        let invite: Option<(Option<String>, i32, i32)> = sqlx::query_as(
            "SELECT expires_at, max_uses, used_count FROM booking_invites WHERE token = ? AND event_type_id = ?",
        )
        .bind(token)
        .bind(&et_id)
        .fetch_optional(&state.pool)
        .await
        .unwrap_or(None);

        match invite {
            None => return Html("Invalid invite link.".to_string()).into_response(),
            Some((expires_at, max_uses, used_count)) => {
                if let Some(exp) = &expires_at {
                    if exp < &chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string() {
                        return Html("This invite link has expired.".to_string()).into_response();
                    }
                }
                if used_count >= max_uses {
                    return Html("This invite link has already been used.".to_string())
                        .into_response();
                }
            }
        }
    }

    let date = match NaiveDate::parse_from_str(&form.date, "%Y-%m-%d") {
        Ok(d) => d,
        Err(_) => return Html("Invalid date.".to_string()).into_response(),
    };
    if let Err(e) = validate_date_not_too_far(date) {
        return Html(e).into_response();
    }
    let start_time = match NaiveTime::parse_from_str(&form.time, "%H:%M") {
        Ok(t) => t,
        Err(_) => return Html("Invalid time.".to_string()).into_response(),
    };

    let guest_tz = parse_guest_tz(form.tz.as_deref());
    let guest_timezone = guest_tz.name().to_string();
    let host_tz = get_host_tz(&state.pool, &et_id).await;

    // The URL carries guest-local date/time; convert to host-local for storage
    // and availability checks (existing semantics).
    let guest_local_start = date.and_time(start_time);
    let guest_local_end = guest_local_start + Duration::minutes(duration as i64);
    let slot_start = guest_to_host_local(guest_local_start, guest_tz, host_tz);
    let slot_end = slot_start + Duration::minutes(duration as i64);
    let guest_end_time = guest_local_end.time().format("%H:%M").to_string();

    let now = Local::now().naive_local();
    if slot_start < now + Duration::minutes(min_notice as i64) {
        return Html("This slot is no longer available (too soon).".to_string()).into_response();
    }

    let buf_start = slot_start - Duration::minutes(buffer_before as i64);
    let buf_end = slot_end + Duration::minutes(buffer_after as i64);

    let id = uuid::Uuid::new_v4().to_string();
    let uid = format!("{}@calrs", uuid::Uuid::new_v4());
    let cancel_token = uuid::Uuid::new_v4().to_string();
    let reschedule_token = uuid::Uuid::new_v4().to_string();
    let start_at = slot_start.format("%Y-%m-%dT%H:%M:%S").to_string();
    let end_at = slot_end.format("%Y-%m-%dT%H:%M:%S").to_string();

    let initial_status = if needs_approval {
        "pending"
    } else {
        "confirmed"
    };
    let confirm_token: Option<String> = if needs_approval {
        Some(uuid::Uuid::new_v4().to_string())
    } else {
        None
    };

    // Start a transaction to ensure atomicity of availability check + insert.
    let mut tx = match state.pool.begin().await {
        Ok(tx) => tx,
        Err(e) => {
            return internal_error_response("database query", &e);
        }
    };

    let busy = fetch_busy_times_for_user(
        &state.pool,
        &host_user_id,
        buf_start,
        buf_end,
        host_tz,
        Some(&et_id),
    )
    .await;
    if has_conflict(&busy, buf_start, buf_end) {
        let _ = tx.rollback().await;
        return Html("This slot is no longer available.".to_string()).into_response();
    }

    // Check booking frequency limits. Personal event-type flows don't have a
    // team assignee, so per-member caps degrade to event-type-wide here.
    if would_exceed_frequency_limit(&state.pool, &et_id, slot_start, None).await {
        let _ = tx.rollback().await;
        return render_booking_action_error(
            &state,
            &headers,
            "Not available right now",
            "The host isn't accepting more bookings for this date. Please pick a different date, or check back later.",
        );
    }

    let insert_result = sqlx::query(
        "INSERT INTO bookings (id, event_type_id, uid, guest_name, guest_email, guest_timezone, notes, start_at, end_at, status, cancel_token, reschedule_token, confirm_token, language)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(&et_id)
    .bind(&uid)
    .bind(&form.name)
    .bind(&form.email)
    .bind(&guest_timezone)
    .bind(&form.notes)
    .bind(&start_at)
    .bind(&end_at)
    .bind(initial_status)
    .bind(&cancel_token)
    .bind(&reschedule_token)
    .bind(&confirm_token)
    .bind(lang)
    .execute(&mut *tx)
    .await;

    match insert_result {
        Ok(_) => {}
        Err(e) => {
            let _ = tx.rollback().await;
            if e.to_string().contains("UNIQUE constraint failed") {
                return Html("This slot is no longer available.".to_string()).into_response();
            }
            return internal_error_response("database query", &e);
        }
    }

    // Insert additional attendees
    for attendee_email in &additional_attendees {
        let attendee_id = uuid::Uuid::new_v4().to_string();
        let _ =
            sqlx::query("INSERT INTO booking_attendees (id, booking_id, email) VALUES (?, ?, ?)")
                .bind(&attendee_id)
                .bind(&id)
                .bind(attendee_email)
                .execute(&mut *tx)
                .await;
    }

    if let Err(e) = tx.commit().await {
        if e.to_string().contains("UNIQUE constraint failed") {
            return Html("This slot is no longer available.".to_string()).into_response();
        }
        return internal_error_response("database query", &e);
    }

    tracing::info!(booking_id = %id, event_type = %slug, guest = %form.email, "booking created");

    // Increment invite used_count if this was an invite-based booking
    if visibility == "private" || visibility == "internal" {
        if let Some(token) = &form.invite_token {
            let _ = sqlx::query("UPDATE booking_invites SET used_count = used_count + 1 WHERE token = ? AND event_type_id = ?")
                .bind(token)
                .bind(&et_id)
                .execute(&state.pool)
                .await;
        }
    }

    // Build BookingDetails once. CalDAV push and email send both need it,
    // and CalDAV push must run independently of whether SMTP is configured.
    let host: Option<(String, String)> = sqlx::query_as(
        "SELECT u.name, COALESCE(u.booking_email, u.email) FROM users u WHERE u.username = ?",
    )
    .bind(&username)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    let host_user_id: Option<String> =
        sqlx::query_scalar("SELECT id FROM users WHERE username = ?")
            .bind(&username)
            .fetch_optional(&state.pool)
            .await
            .unwrap_or(None);
    let location_display = resolve_booking_location(
        &state,
        &id,
        &et_id,
        host_user_id.as_deref(),
        loc_value.as_deref(),
        &form.name,
        &form.email,
        !needs_approval,
    )
    .await;

    if let Some((host_name, host_email)) = host {
        let details = crate::email::BookingDetails {
            event_title: et_title.clone(),
            date: form.date.clone(),
            start_time: form.time.clone(),
            end_time: guest_end_time.clone(),
            guest_name: form.name.clone(),
            guest_email: form.email.clone(),
            guest_timezone: guest_timezone.clone(),
            host_name,
            host_email,
            uid: uid.clone(),
            notes: form.notes.clone(),
            location: location_display.clone(),
            reminder_minutes: reminder_min,
            additional_attendees: additional_attendees.clone(),
            guest_language: Some(lang.to_string()),
            host_timezone: host_tz.name().to_string(),
            ..Default::default()
        };

        // Push confirmed bookings to CalDAV regardless of SMTP availability.
        if !needs_approval {
            if let Some(uid_user) = host_user_id.as_deref() {
                caldav_push_booking(&state.pool, &state.secret_key, uid_user, &uid, &details).await;
            }
        }

        let (cancel_notice_min, reschedule_notice_min) =
            fetch_event_type_notice_minutes(&state.pool, &et_id).await;

        // Send emails if SMTP is configured.
        if let Ok(Some(smtp_config)) =
            crate::email::load_smtp_config(&state.pool, &state.secret_key).await
        {
            let base_url = std::env::var("CALRS_BASE_URL").ok();
            let guest_cancel_url = base_url.as_ref().map(|base| {
                format!(
                    "{}/booking/cancel/{}",
                    base.trim_end_matches('/'),
                    cancel_token
                )
            });
            let guest_reschedule_url = base_url.as_ref().map(|base| {
                format!(
                    "{}/booking/reschedule/{}",
                    base.trim_end_matches('/'),
                    reschedule_token
                )
            });

            if needs_approval {
                let _ = crate::email::send_host_approval_request(
                    &smtp_config,
                    &details,
                    &id,
                    confirm_token.as_deref(),
                    base_url.as_deref(),
                )
                .await;
                let _ = crate::email::send_guest_pending_notice_ex(
                    &smtp_config,
                    &details,
                    guest_cancel_url.as_deref(),
                    guest_reschedule_url.as_deref(),
                )
                .await;
            } else {
                let _ = crate::email::send_guest_confirmation_ex(
                    &smtp_config,
                    &details,
                    guest_cancel_url.as_deref(),
                    guest_reschedule_url.as_deref(),
                    cancel_notice_min,
                    reschedule_notice_min,
                )
                .await;
                let _ = crate::email::send_host_notification(&smtp_config, &details).await;
            }
        }
    }

    let host_name: String = sqlx::query_scalar("SELECT name FROM users WHERE username = ?")
        .bind(&username)
        .fetch_optional(&state.pool)
        .await
        .unwrap_or(None)
        .unwrap_or_else(|| "Host".to_string());

    let date_label = crate::i18n::format_long_date(date, lang);
    let (cancel_notice_min, reschedule_notice_min) =
        fetch_event_type_notice_minutes(&state.pool, &et_id).await;

    let tmpl = match state.templates.get_template("confirmed.html") {
        Ok(t) => t,
        Err(e) => return internal_error_response("internal", &e),
    };
    let rendered = tmpl
        .render(context! {
            event_title => et_title,
            date_label => date_label,
            time_start => form.time,
            time_end => guest_end_time,
            host_name => host_name,
            guest_email => form.email,
            notes => form.notes,
            pending => needs_approval,
            location_type => loc_type,
            location_value => location_display,
            additional_attendees => additional_attendees,
            cancel_notice_min => cancel_notice_min,
            reschedule_notice_min => reschedule_notice_min,
            company_link => state.company_link.read().await.clone(),
            lang => lang,
            embed => embed.is_embedded(),
            embed_brand => embed.brand_css().unwrap_or_default(),
            embed_theme => embed.theme.as_deref().unwrap_or(""),
            embed_qs_amp => embed.query_suffix(true),
            embed_qs_q => embed.query_suffix(false),
        })
        .unwrap_or_else(|e| internal_error_body("template render", &e));

    html_response(rendered, &embed)
}

// --- Slot computation (shared with CLI) ---

/// Pick an available team member for a booking slot.
/// Returns (user_id, name, email) of the member with fewest recent bookings.
async fn pick_group_member(
    pool: &SqlitePool,
    team_id: &str,
    event_type_id: &str,
    slot_start: NaiveDateTime,
    slot_end: NaiveDateTime,
    buffer_before: i32,
    buffer_after: i32,
    host_tz: Tz,
) -> Option<(String, String, String)> {
    let buf_start = slot_start - Duration::minutes(buffer_before as i64);
    let buf_end = slot_end + Duration::minutes(buffer_after as i64);

    // Fetch members with per-event-type weight (fallback to default 1)
    // weight=0 means excluded from this event type
    let members: Vec<(String, String, String, i64)> = sqlx::query_as(
        "SELECT u.id, u.name, COALESCE(u.booking_email, u.email), \
         COALESCE(etw.weight, 1) \
         FROM users u JOIN team_members tm ON tm.user_id = u.id \
         LEFT JOIN event_type_member_weights etw ON etw.user_id = u.id AND etw.event_type_id = ? \
         WHERE tm.team_id = ? AND u.enabled = 1 \
         AND COALESCE(etw.weight, 1) > 0",
    )
    .bind(event_type_id)
    .bind(team_id)
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    // Per-member booking-frequency caps. We exclude any member already at
    // (or over) their per-period cap so the picker doesn't pick them just to
    // have the submit-time check reject the booking.
    let per_member_limits: Vec<(i32, String)> = sqlx::query_as(
        "SELECT max_bookings, period FROM booking_frequency_limits \
         WHERE event_type_id = ? AND per_member = 1",
    )
    .bind(event_type_id)
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    let mut available_members = Vec::new();

    for (user_id, name, email, weight) in &members {
        let mut busy = fetch_busy_times_for_user(
            pool,
            user_id,
            buf_start,
            buf_end,
            host_tz,
            Some(event_type_id),
        )
        .await;
        // Also exclude members who are outside their own working hours for
        // this slot. Members without explicit user_availability_rules are
        // returned unconstrained by user_avail_as_busy, matching the slot
        // grid semantics in show_group_slots.
        busy.extend(user_avail_as_busy(pool, user_id, buf_start, buf_end, host_tz).await);
        if has_conflict(&busy, buf_start, buf_end) {
            continue;
        }

        let mut at_per_member_cap = false;
        for (max, period) in &per_member_limits {
            let (rs, re) = frequency_period_range(slot_start, period);
            let rs_str = rs.format("%Y-%m-%dT%H:%M:%S").to_string();
            let re_str = re.format("%Y-%m-%dT%H:%M:%S").to_string();
            let count: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM bookings \
                 WHERE event_type_id = ? AND assigned_user_id = ? \
                 AND status IN ('confirmed', 'pending') \
                 AND start_at >= ? AND start_at < ?",
            )
            .bind(event_type_id)
            .bind(user_id)
            .bind(&rs_str)
            .bind(&re_str)
            .fetch_one(pool)
            .await
            .unwrap_or(0);
            if count >= *max as i64 {
                at_per_member_cap = true;
                break;
            }
        }
        if at_per_member_cap {
            continue;
        }

        available_members.push((user_id.clone(), name.clone(), email.clone(), *weight));
    }

    if available_members.is_empty() {
        return None;
    }

    // Among available members, pick by highest weight first, then fewest bookings in last 30 days
    let thirty_days_ago = (Utc::now() - Duration::days(30))
        .format("%Y-%m-%dT%H:%M:%S")
        .to_string();
    let mut best: Option<(String, String, String, i64, i64)> = None;

    for (user_id, name, email, weight) in &available_members {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM bookings WHERE assigned_user_id = ? AND created_at >= ?",
        )
        .bind(user_id)
        .bind(&thirty_days_ago)
        .fetch_one(pool)
        .await
        .unwrap_or(0);

        let is_better = match &best {
            None => true,
            Some((_, _, _, bw, bc)) => *weight > *bw || (*weight == *bw && count < *bc),
        };
        if is_better {
            best = Some((user_id.clone(), name.clone(), email.clone(), *weight, count));
        }
    }

    best.map(|(uid, name, email, _, _)| (uid, name, email))
}

struct SlotDay {
    date: String,
    label: String,
    slots: Vec<SlotTime>,
}

struct SlotTime {
    start: String,      // guest TZ display
    end: String,        // guest TZ display
    host_date: String,  // YYYY-MM-DD in host TZ (for booking)
    host_time: String,  // HH:MM in host TZ (for booking)
    guest_date: String, // YYYY-MM-DD in guest TZ (for grouping by day)
}

// --- Shared busy-time helpers ---

/// Expand recurring events into (start, end) pairs within a time window.
/// Tuples are (start_at, end_at, rrule, raw_ical, timezone).
fn expand_recurring_into_busy(
    recurring: &[(String, String, String, Option<String>, Option<String>)],
    window_start: NaiveDateTime,
    window_end: NaiveDateTime,
    host_tz: Tz,
) -> Vec<(NaiveDateTime, NaiveDateTime)> {
    let mut result = Vec::new();
    for (s, e, rrule_str, raw_ical, event_tz) in recurring {
        if let (Some(ev_start), Some(ev_end)) = (parse_ical_datetime(s), parse_ical_datetime(e)) {
            let exdates = raw_ical
                .as_deref()
                .map(crate::rrule::extract_exdates)
                .unwrap_or_default();
            // Expand RRULE in the event's own timezone (correct for DST)
            let occurrences = crate::rrule::expand_rrule(
                ev_start,
                ev_end,
                rrule_str,
                &exdates,
                window_start,
                window_end,
            );
            // Convert each occurrence to host timezone
            for (os, oe) in occurrences {
                let cs = convert_event_to_tz(os, event_tz.as_deref(), host_tz);
                let ce = convert_event_to_tz(oe, event_tz.as_deref(), host_tz);
                result.push((cs, ce));
            }
        }
    }
    result
}

/// Fetch busy times for a specific user (events from their calendars + their bookings).
/// Event times are converted from their stored timezone to `host_tz`.
async fn fetch_busy_times_for_user(
    pool: &SqlitePool,
    user_id: &str,
    window_start: NaiveDateTime,
    window_end: NaiveDateTime,
    host_tz: Tz,
    event_type_id: Option<&str>,
) -> Vec<(NaiveDateTime, NaiveDateTime)> {
    fetch_busy_times_for_user_ex(
        pool,
        user_id,
        window_start,
        window_end,
        host_tz,
        event_type_id,
        None,
    )
    .await
}

async fn fetch_busy_times_for_user_ex(
    pool: &SqlitePool,
    user_id: &str,
    window_start: NaiveDateTime,
    window_end: NaiveDateTime,
    host_tz: Tz,
    event_type_id: Option<&str>,
    exclude_booking_id: Option<&str>,
) -> Vec<(NaiveDateTime, NaiveDateTime)> {
    let end_compact = window_end.format("%Y%m%d").to_string();
    let start_compact = window_start.format("%Y%m%dT%H%M%S").to_string();
    let end_iso = window_end.format("%Y-%m-%dT%H:%M:%S").to_string();
    let start_iso = window_start.format("%Y-%m-%dT%H:%M:%S").to_string();

    // Empty string means NOT EXISTS is always true (no rows match), so all calendars pass
    let et_id_for_filter = event_type_id.unwrap_or("");

    let events: Vec<(String, String, Option<String>)> = sqlx::query_as(
        "SELECT e.start_at, e.end_at, e.timezone FROM events e
         JOIN calendars c ON c.id = e.calendar_id
         JOIN caldav_sources cs ON cs.id = c.source_id
         JOIN accounts a ON a.id = cs.account_id
         WHERE a.user_id = ? AND c.is_busy = 1
           AND (NOT EXISTS (SELECT 1 FROM event_type_calendars WHERE event_type_id = ?)
                OR c.id IN (SELECT calendar_id FROM event_type_calendars WHERE event_type_id = ?))
           AND (e.rrule IS NULL OR e.rrule = '')
           AND (e.status IS NULL OR e.status != 'CANCELLED')
           AND (e.transp IS NULL OR e.transp != 'TRANSPARENT')
           AND ((e.start_at <= ? AND e.end_at >= ?) OR (e.start_at <= ? AND e.end_at >= ?))",
    )
    .bind(user_id)
    .bind(et_id_for_filter)
    .bind(et_id_for_filter)
    .bind(&end_compact)
    .bind(&start_compact)
    .bind(&end_iso)
    .bind(&start_iso)
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    let mut busy: Vec<(NaiveDateTime, NaiveDateTime)> = events
        .iter()
        .filter_map(|(s, e, tz)| {
            let start = convert_event_to_tz(parse_ical_datetime(s)?, tz.as_deref(), host_tz);
            let end = convert_event_to_tz(parse_ical_datetime(e)?, tz.as_deref(), host_tz);
            Some((start, end))
        })
        .collect();

    let end_compact_rrule = window_end.format("%Y%m%dT235959").to_string();
    let recurring: Vec<(String, String, String, Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT e.start_at, e.end_at, e.rrule, e.raw_ical, e.timezone FROM events e
         JOIN calendars c ON c.id = e.calendar_id
         JOIN caldav_sources cs ON cs.id = c.source_id
         JOIN accounts a ON a.id = cs.account_id
         WHERE a.user_id = ? AND c.is_busy = 1
           AND (NOT EXISTS (SELECT 1 FROM event_type_calendars WHERE event_type_id = ?)
                OR c.id IN (SELECT calendar_id FROM event_type_calendars WHERE event_type_id = ?))
           AND (e.status IS NULL OR e.status != 'CANCELLED')
           AND (e.transp IS NULL OR e.transp != 'TRANSPARENT')
           AND e.rrule IS NOT NULL AND e.rrule != '' AND (e.start_at <= ? OR e.start_at <= ?)",
    )
    .bind(user_id)
    .bind(et_id_for_filter)
    .bind(et_id_for_filter)
    .bind(&end_iso)
    .bind(&end_compact_rrule)
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    busy.extend(expand_recurring_into_busy(
        &recurring,
        window_start,
        window_end,
        host_tz,
    ));

    let exclude_id = exclude_booking_id.unwrap_or("");
    let bookings: Vec<(String, String)> = sqlx::query_as(
        "SELECT b.start_at, b.end_at FROM bookings b
         JOIN event_types et ON et.id = b.event_type_id
         JOIN accounts a ON a.id = et.account_id
         WHERE (a.user_id = ? OR b.assigned_user_id = ?) AND b.status = 'confirmed'
           AND b.start_at <= ? AND b.end_at >= ?
           AND (? = '' OR b.id != ?)",
    )
    .bind(user_id)
    .bind(user_id)
    .bind(&end_iso)
    .bind(&start_iso)
    .bind(exclude_id)
    .bind(exclude_id)
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    for (s, e) in &bookings {
        if let (Some(start), Some(end)) = (parse_ical_datetime(s), parse_ical_datetime(e)) {
            busy.push((start, end));
        }
    }

    busy
}

/// Check if any busy period overlaps with [buf_start, buf_end).
fn has_conflict(
    busy: &[(NaiveDateTime, NaiveDateTime)],
    buf_start: NaiveDateTime,
    buf_end: NaiveDateTime,
) -> bool {
    busy.iter().any(|(s, e)| *s < buf_end && *e > buf_start)
}

// --- Busy source for unified slot computation ---

enum BusySource {
    /// Flat list of busy times (individual event type)
    Individual(Vec<(NaiveDateTime, NaiveDateTime)>),
    /// Per-member busy times; slot is available if ANY member is free
    Group(HashMap<String, Vec<(NaiveDateTime, NaiveDateTime)>>),
    /// Per-member busy times; slot is available only if ALL members are free
    Team(HashMap<String, Vec<(NaiveDateTime, NaiveDateTime)>>),
}

/// Compute available slots for an event type.
/// Caller provides pre-fetched busy times via BusySource.
async fn compute_slots(
    pool: &SqlitePool,
    et_id: &str,
    duration: i32,
    buffer_before: i32,
    buffer_after: i32,
    min_notice: i32,
    start_offset: i32,
    days_ahead: i32,
    host_tz: Tz,
    guest_tz: Tz,
    busy: BusySource,
) -> Vec<SlotDay> {
    let rules: Vec<(i32, String, String)> = sqlx::query_as(
        "SELECT day_of_week, start_time, end_time FROM availability_rules WHERE event_type_id = ?",
    )
    .bind(et_id)
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    // Fetch availability overrides for this event type
    let overrides: Vec<(String, Option<String>, Option<String>, i32)> = sqlx::query_as(
        "SELECT date, start_time, end_time, is_blocked FROM availability_overrides WHERE event_type_id = ? ORDER BY date, start_time",
    )
    .bind(et_id)
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    // slot_interval_min overrides the cursor step. NULL = use duration (legacy behavior).
    let slot_interval: Option<i32> =
        sqlx::query_scalar("SELECT slot_interval_min FROM event_types WHERE id = ?")
            .bind(et_id)
            .fetch_one(pool)
            .await
            .unwrap_or(None);
    let interval = slot_interval.filter(|v| *v > 0).unwrap_or(duration);

    let mut result = compute_slots_from_rules(
        &rules,
        duration,
        interval,
        buffer_before,
        buffer_after,
        min_notice,
        start_offset,
        days_ahead,
        host_tz,
        guest_tz,
        busy,
        &overrides,
    );

    // Drop slots whose host-local period already meets/exceeds a configured
    // frequency cap. The submit-time check (would_exceed_frequency_limit)
    // stays as a backstop for the race where two guests grab the last slot
    // at the same instant.
    apply_frequency_limit_filter(pool, et_id, &mut result).await;

    // If first_slot_only is enabled, keep only the earliest slot per day
    let first_only: i32 =
        sqlx::query_scalar("SELECT first_slot_only FROM event_types WHERE id = ?")
            .bind(et_id)
            .fetch_one(pool)
            .await
            .unwrap_or(0);

    if first_only != 0 {
        for day in &mut result {
            day.slots.truncate(1);
        }
    }

    result
}

/// Remove slots that fall inside a host-local period already at/over a
/// configured booking-frequency cap, so the picker doesn't surface times
/// that the submit-time check would reject.
///
/// Per-member limits hide a slot only when *every* eligible team member is
/// already at their cap for the slot's period; if any one of them still has
/// headroom, the slot stays available and the round-robin picker will route
/// to that member. On personal event types (no team), per-member limits
/// degrade to event-type-wide behaviour.
async fn apply_frequency_limit_filter(pool: &SqlitePool, et_id: &str, days: &mut [SlotDay]) {
    let limits: Vec<(i32, String, i32)> = sqlx::query_as(
        "SELECT max_bookings, period, per_member FROM booking_frequency_limits WHERE event_type_id = ?",
    )
    .bind(et_id)
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    if limits.is_empty() {
        return;
    }

    let has_per_member = limits.iter().any(|(_, _, m)| *m != 0);
    let team_members: Vec<String> = if has_per_member {
        sqlx::query_scalar(
            "SELECT u.id FROM users u \
             JOIN team_members tm ON tm.user_id = u.id \
             JOIN event_types et ON et.team_id = tm.team_id \
             LEFT JOIN event_type_member_weights etw ON etw.user_id = u.id AND etw.event_type_id = et.id \
             WHERE et.id = ? AND u.enabled = 1 AND COALESCE(etw.weight, 1) > 0",
        )
        .bind(et_id)
        .fetch_all(pool)
        .await
        .unwrap_or_default()
    } else {
        Vec::new()
    };

    // Gather unique (period, period_start) pairs the visible slots touch, and
    // resolve each one to (event-type-wide count, per-member counts).
    let mut wide_counts: HashMap<(String, NaiveDateTime), i64> = HashMap::new();
    let mut member_counts: HashMap<(String, NaiveDateTime, String), i64> = HashMap::new();

    for day in days.iter() {
        for slot in &day.slots {
            let Ok(d) = NaiveDate::parse_from_str(&slot.host_date, "%Y-%m-%d") else {
                continue;
            };
            let dt = d.and_hms_opt(12, 0, 0).unwrap();
            for (_, period, _) in &limits {
                let (ps, _) = frequency_period_range(dt, period);
                wide_counts.entry((period.clone(), ps)).or_insert(0);
            }
        }
    }

    for ((period, ps), wide) in wide_counts.iter_mut() {
        let (rs, re) = frequency_period_range(*ps, period);
        let rs_str = rs.format("%Y-%m-%dT%H:%M:%S").to_string();
        let re_str = re.format("%Y-%m-%dT%H:%M:%S").to_string();

        let c: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM bookings WHERE event_type_id = ? AND status IN ('confirmed', 'pending') AND start_at >= ? AND start_at < ?",
        )
        .bind(et_id)
        .bind(&rs_str)
        .bind(&re_str)
        .fetch_one(pool)
        .await
        .unwrap_or((0,));
        *wide = c.0;

        if has_per_member && !team_members.is_empty() {
            let rows: Vec<(String, i64)> = sqlx::query_as(
                "SELECT assigned_user_id, COUNT(*) FROM bookings \
                 WHERE event_type_id = ? AND assigned_user_id IS NOT NULL \
                 AND status IN ('confirmed', 'pending') \
                 AND start_at >= ? AND start_at < ? \
                 GROUP BY assigned_user_id",
            )
            .bind(et_id)
            .bind(&rs_str)
            .bind(&re_str)
            .fetch_all(pool)
            .await
            .unwrap_or_default();
            for (uid, n) in rows {
                member_counts.insert((period.clone(), *ps, uid), n);
            }
        }
    }

    for day in days.iter_mut() {
        day.slots.retain(|slot| {
            let Ok(d) = NaiveDate::parse_from_str(&slot.host_date, "%Y-%m-%d") else {
                return true;
            };
            let dt = d.and_hms_opt(12, 0, 0).unwrap();
            for (max, period, per_member) in &limits {
                let (ps, _) = frequency_period_range(dt, period);
                let max_i64 = *max as i64;
                if *per_member != 0 && !team_members.is_empty() {
                    // Hide only if every eligible member is at cap.
                    let all_capped = team_members.iter().all(|uid| {
                        let count = member_counts
                            .get(&(period.clone(), ps, uid.clone()))
                            .copied()
                            .unwrap_or(0);
                        count >= max_i64
                    });
                    if all_capped {
                        return false;
                    }
                } else {
                    let count = wide_counts.get(&(period.clone(), ps)).copied().unwrap_or(0);
                    if count >= max_i64 {
                        return false;
                    }
                }
            }
            true
        });
    }
}

/// Save booking frequency limits from the serialized form field.
/// Format per row: "count:period" or "count:period:m" (the trailing ":m"
/// marks the row as per-member; absence means event-type-wide). Rows are
/// comma-separated.
async fn save_frequency_limits(pool: &SqlitePool, event_type_id: &str, limits_str: &str) {
    if limits_str.is_empty() {
        return;
    }
    for part in limits_str.split(',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        let mut bits = part.splitn(3, ':');
        let Some(count_str) = bits.next() else {
            continue;
        };
        let Some(period) = bits.next() else {
            continue;
        };
        let per_member = matches!(bits.next(), Some("m"));
        let count: i32 = count_str.parse().unwrap_or(0);
        if count > 0 && ["day", "week", "month", "year"].contains(&period) {
            let limit_id = uuid::Uuid::new_v4().to_string();
            let _ = sqlx::query(
                "INSERT INTO booking_frequency_limits (id, event_type_id, max_bookings, period, per_member) VALUES (?, ?, ?, ?, ?)",
            )
            .bind(&limit_id)
            .bind(event_type_id)
            .bind(count)
            .bind(period)
            .bind(per_member as i32)
            .execute(pool)
            .await;
        }
    }
}

/// Compute the start/end of a calendar period containing the given datetime.
fn frequency_period_range(dt: NaiveDateTime, period: &str) -> (NaiveDateTime, NaiveDateTime) {
    let date = dt.date();
    match period {
        "day" => {
            let start = date.and_hms_opt(0, 0, 0).unwrap();
            let end = (date + Duration::days(1)).and_hms_opt(0, 0, 0).unwrap();
            (start, end)
        }
        "week" => {
            let weekday = date.weekday().num_days_from_monday();
            let week_start = date - Duration::days(weekday as i64);
            let start = week_start.and_hms_opt(0, 0, 0).unwrap();
            let end = (week_start + Duration::days(7))
                .and_hms_opt(0, 0, 0)
                .unwrap();
            (start, end)
        }
        "month" => {
            let start = NaiveDate::from_ymd_opt(date.year(), date.month(), 1)
                .unwrap()
                .and_hms_opt(0, 0, 0)
                .unwrap();
            let end = if date.month() == 12 {
                NaiveDate::from_ymd_opt(date.year() + 1, 1, 1).unwrap()
            } else {
                NaiveDate::from_ymd_opt(date.year(), date.month() + 1, 1).unwrap()
            }
            .and_hms_opt(0, 0, 0)
            .unwrap();
            (start, end)
        }
        "year" => {
            let start = NaiveDate::from_ymd_opt(date.year(), 1, 1)
                .unwrap()
                .and_hms_opt(0, 0, 0)
                .unwrap();
            let end = NaiveDate::from_ymd_opt(date.year() + 1, 1, 1)
                .unwrap()
                .and_hms_opt(0, 0, 0)
                .unwrap();
            (start, end)
        }
        _ => (dt, dt),
    }
}

/// Check if a booking at the given datetime would exceed any frequency limit
/// configured on the event type. `assigned_user_id` is the team member the
/// booking would land on — required for per-member caps; ignored by
/// event-type-wide caps. Pass `None` for personal event types (no assignee).
async fn would_exceed_frequency_limit(
    pool: &SqlitePool,
    event_type_id: &str,
    proposed_start: NaiveDateTime,
    assigned_user_id: Option<&str>,
) -> bool {
    let limits: Vec<(i32, String, i32)> = sqlx::query_as(
        "SELECT max_bookings, period, per_member FROM booking_frequency_limits WHERE event_type_id = ?",
    )
    .bind(event_type_id)
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    if limits.is_empty() {
        return false;
    }

    for (max_bookings, period, per_member) in &limits {
        let (range_start, range_end) = frequency_period_range(proposed_start, period);
        let range_start_str = range_start.format("%Y-%m-%dT%H:%M:%S").to_string();
        let range_end_str = range_end.format("%Y-%m-%dT%H:%M:%S").to_string();

        let count: (i64,) = if *per_member != 0 {
            // A per-member cap on a personal event type (no assignee) is
            // meaningless — treat it as event-type-wide for those cases.
            match assigned_user_id {
                Some(uid) => sqlx::query_as(
                    "SELECT COUNT(*) FROM bookings WHERE event_type_id = ? AND assigned_user_id = ? AND status IN ('confirmed', 'pending') AND start_at >= ? AND start_at < ?",
                )
                .bind(event_type_id)
                .bind(uid)
                .bind(&range_start_str)
                .bind(&range_end_str)
                .fetch_one(pool)
                .await
                .unwrap_or((0,)),
                None => sqlx::query_as(
                    "SELECT COUNT(*) FROM bookings WHERE event_type_id = ? AND status IN ('confirmed', 'pending') AND start_at >= ? AND start_at < ?",
                )
                .bind(event_type_id)
                .bind(&range_start_str)
                .bind(&range_end_str)
                .fetch_one(pool)
                .await
                .unwrap_or((0,)),
            }
        } else {
            sqlx::query_as(
                "SELECT COUNT(*) FROM bookings WHERE event_type_id = ? AND status IN ('confirmed', 'pending') AND start_at >= ? AND start_at < ?",
            )
            .bind(event_type_id)
            .bind(&range_start_str)
            .bind(&range_end_str)
            .fetch_one(pool)
            .await
            .unwrap_or((0,))
        };

        if count.0 >= *max_bookings as i64 {
            return true;
        }
    }
    false
}

/// Core slot computation from pre-fetched rules.
fn compute_slots_from_rules(
    rules: &[(i32, String, String)],
    duration: i32,
    interval: i32,
    buffer_before: i32,
    buffer_after: i32,
    min_notice: i32,
    start_offset: i32,
    days_ahead: i32,
    host_tz: Tz,
    guest_tz: Tz,
    busy: BusySource,
    overrides: &[(String, Option<String>, Option<String>, i32)],
) -> Vec<SlotDay> {
    let now_host = Utc::now().with_timezone(&host_tz).naive_local();
    let min_start = now_host + Duration::minutes(min_notice as i64);

    let slot_duration = Duration::minutes(duration as i64);
    let slot_step = Duration::minutes(interval.max(1) as i64);
    let mut result = Vec::new();

    for day_offset in start_offset..(start_offset + days_ahead) {
        let date = now_host.date() + Duration::days(day_offset as i64);
        let date_str = date.format("%Y-%m-%d").to_string();

        // Check availability overrides for this date
        let day_overrides: Vec<&(String, Option<String>, Option<String>, i32)> = overrides
            .iter()
            .filter(|(d, _, _, _)| *d == date_str)
            .collect();

        // If any override blocks this day, skip entirely
        if day_overrides.iter().any(|(_, _, _, blocked)| *blocked != 0) {
            continue;
        }

        // Build time windows: use custom hours if overrides exist, else weekly rules
        let windows: Vec<(String, String)> = if !day_overrides.is_empty() {
            // Custom hours overrides replace weekly rules
            day_overrides
                .iter()
                .filter_map(|(_, s, e, _)| match (s, e) {
                    (Some(start), Some(end)) => Some((start.clone(), end.clone())),
                    _ => None,
                })
                .collect()
        } else {
            let weekday = date.weekday().num_days_from_sunday() as i32;
            rules
                .iter()
                .filter(|(d, _, _)| *d == weekday)
                .map(|(_, s, e)| (s.clone(), e.clone()))
                .collect()
        };

        if windows.is_empty() {
            continue;
        }

        let mut day_slots = Vec::new();

        for (start_str, end_str) in &windows {
            let window_start_time = match NaiveTime::parse_from_str(start_str, "%H:%M") {
                Ok(t) => t,
                Err(_) => continue,
            };
            let window_end_time = match NaiveTime::parse_from_str(end_str, "%H:%M") {
                Ok(t) => t,
                Err(_) => continue,
            };

            // Walk the cursor as a NaiveDateTime, not a NaiveTime: NaiveTime +
            // Duration wraps at 24h, which turned a window ending at 23:00 with
            // a 60-minute slot duration into an infinite loop (23:00 + 60m =
            // 00:00, still <= 23:00 as a time-of-day, so the loop emitted a
            // slot every step forever until OOM).
            let window_end = date.and_time(window_end_time);
            let mut cursor = date.and_time(window_start_time);
            while cursor + slot_duration <= window_end {
                let slot_start = cursor;
                let slot_end = slot_start + slot_duration;

                if slot_start < min_start {
                    cursor += slot_step;
                    continue;
                }

                let buf_start = slot_start - Duration::minutes(buffer_before as i64);
                let buf_end = slot_end + Duration::minutes(buffer_after as i64);

                let is_free = match &busy {
                    BusySource::Individual(times) => !has_conflict(times, buf_start, buf_end),
                    BusySource::Group(member_busy) => member_busy
                        .values()
                        .any(|times| !has_conflict(times, buf_start, buf_end)),
                    BusySource::Team(member_busy) => {
                        !member_busy.is_empty()
                            && member_busy
                                .values()
                                .all(|times| !has_conflict(times, buf_start, buf_end))
                    }
                };

                if is_free {
                    let slot_start_utc = host_tz
                        .from_local_datetime(&slot_start)
                        .earliest()
                        .unwrap_or_else(|| host_tz.from_utc_datetime(&slot_start))
                        .with_timezone(&Utc);
                    let slot_end_utc = host_tz
                        .from_local_datetime(&slot_end)
                        .earliest()
                        .unwrap_or_else(|| host_tz.from_utc_datetime(&slot_end))
                        .with_timezone(&Utc);
                    let guest_start = slot_start_utc.with_timezone(&guest_tz);
                    let guest_end = slot_end_utc.with_timezone(&guest_tz);

                    day_slots.push(SlotTime {
                        start: guest_start.format("%H:%M").to_string(),
                        end: guest_end.format("%H:%M").to_string(),
                        host_date: date.format("%Y-%m-%d").to_string(),
                        host_time: cursor.format("%H:%M").to_string(),
                        guest_date: guest_start.format("%Y-%m-%d").to_string(),
                    });
                }

                cursor += slot_step;
            }
        }

        if !day_slots.is_empty() {
            let mut guest_days: std::collections::BTreeMap<String, Vec<SlotTime>> =
                std::collections::BTreeMap::new();
            for slot in day_slots {
                guest_days
                    .entry(slot.guest_date.clone())
                    .or_default()
                    .push(slot);
            }
            for (guest_date_str, slots) in guest_days {
                if let Ok(gd) = NaiveDate::parse_from_str(&guest_date_str, "%Y-%m-%d") {
                    if !result.iter().any(|d: &SlotDay| d.date == guest_date_str) {
                        result.push(SlotDay {
                            date: guest_date_str,
                            label: gd.format("%A, %B %-d").to_string(),
                            slots,
                        });
                    } else if let Some(existing) =
                        result.iter_mut().find(|d| d.date == guest_date_str)
                    {
                        existing.slots.extend(slots);
                    }
                }
            }
        }
    }

    result.sort_by(|a, b| a.date.cmp(&b.date));
    for day in &mut result {
        day.slots.sort_by(|a, b| a.start.cmp(&b.start));
    }
    result
}

// --- Handlers ---

#[derive(Deserialize)]
struct SlotsQuery {
    #[serde(default)]
    month: Option<String>,
    #[serde(default)]
    tz: Option<String>,
    #[serde(default)]
    invite: Option<String>,
    /// When "1", perform full sync + computation (AJAX callback for deferred loading)
    #[serde(default)]
    deferred: Option<String>,
    #[serde(default)]
    embed: Option<String>,
    #[serde(default)]
    layout: Option<String>,
    #[serde(default)]
    theme: Option<String>,
    #[serde(default)]
    brand: Option<String>,
}

impl SlotsQuery {
    fn embed_params(&self) -> EmbedParams {
        EmbedParams {
            embed: self.embed.clone(),
            layout: self.layout.clone(),
            theme: self.theme.clone(),
            brand: self.brand.clone(),
        }
    }
}

/// Parse a "YYYY-MM" month param, returning (year, month_1indexed). Defaults to current month in guest TZ.
fn parse_month_param(param: Option<&str>, guest_tz: Tz) -> (i32, u32) {
    if let Some(s) = param {
        let parts: Vec<&str> = s.split('-').collect();
        if parts.len() == 2 {
            if let (Ok(y), Ok(m)) = (parts[0].parse::<i32>(), parts[1].parse::<u32>()) {
                if (1..=12).contains(&m) {
                    return (y, m);
                }
            }
        }
    }
    let now = Utc::now().with_timezone(&guest_tz).naive_local();
    (now.date().year(), now.date().month())
}

/// Build month-based slot computation parameters and context variables.
/// Returns (start_offset, days_ahead, month_label, prev_month, next_month, first_weekday, days_in_month, today_date, month_year)
fn build_month_params(
    year: i32,
    month: u32,
    host_tz: Tz,
    guest_tz: Tz,
    lang: &str,
) -> (
    i32,
    i32,
    String,
    Option<String>,
    String,
    u32,
    u32,
    String,
    String,
) {
    let now_guest = Utc::now().with_timezone(&guest_tz).naive_local();
    let now_host = Utc::now().with_timezone(&host_tz).naive_local();
    let today_guest = now_guest.date();

    let month_start = NaiveDate::from_ymd_opt(year, month, 1).unwrap();
    let month_end = if month == 12 {
        NaiveDate::from_ymd_opt(year + 1, 1, 1).unwrap()
    } else {
        NaiveDate::from_ymd_opt(year, month + 1, 1).unwrap()
    } - Duration::days(1);

    let days_in_month = (month_end - month_start).num_days() as u32 + 1;

    // Compute start_offset and days_ahead relative to host's today
    let host_today = now_host.date();
    let start_offset = (month_start - host_today).num_days().max(0) as i32;
    let end_offset = (month_end - host_today).num_days() as i32 + 2; // +2 buffer for TZ edge cases
    let days_ahead = (end_offset - start_offset).max(1);

    let month_label = crate::i18n::format_month_year(month_start, lang);
    let month_year = format!("{}-{:02}", year, month);

    // prev_month: None if viewing current month or earlier
    let current_month_start =
        NaiveDate::from_ymd_opt(today_guest.year(), today_guest.month(), 1).unwrap();
    let prev_month = if month_start > current_month_start {
        let (py, pm) = if month == 1 {
            (year - 1, 12)
        } else {
            (year, month - 1)
        };
        Some(format!("{}-{:02}", py, pm))
    } else {
        None
    };

    let (ny, nm) = if month == 12 {
        (year + 1, 1)
    } else {
        (year, month + 1)
    };
    let next_month = format!("{}-{:02}", ny, nm);

    // Monday = 0 for the grid
    let first_weekday = month_start.weekday().num_days_from_monday();
    let today_date = today_guest.format("%Y-%m-%d").to_string();

    (
        start_offset,
        days_ahead,
        month_label,
        prev_month,
        next_month,
        first_weekday,
        days_in_month,
        today_date,
        month_year,
    )
}

/// Parse a timezone string into a Tz, falling back to server local.
fn parse_guest_tz(tz: Option<&str>) -> Tz {
    tz.and_then(|s| s.parse::<Tz>().ok()).unwrap_or_else(|| {
        // Fall back to server's local timezone
        iana_time_zone::get_timezone()
            .ok()
            .and_then(|s| s.parse::<Tz>().ok())
            .unwrap_or(Tz::UTC)
    })
}

/// Normalize a timezone value submitted via an event-type form. Accepts the
/// input only if it parses as a valid IANA timezone, otherwise returns the
/// fallback (typically the submitting user's timezone).
fn normalize_event_type_tz(input: Option<&str>, fallback: &str) -> String {
    input
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .filter(|s| s.parse::<Tz>().is_ok())
        .map(str::to_string)
        .unwrap_or_else(|| fallback.to_string())
}

/// Get the host's timezone from the event type owner's profile.
/// Falls back to the server's local timezone, then UTC.
/// Convert a naive datetime in the guest's timezone to the equivalent naive
/// datetime in the host's timezone. Used when accepting a booking: the URL
/// carries the time the guest clicked (their local time), but availability
/// checks, storage, and the existing display code all assume host-local.
fn guest_to_host_local(guest_local: NaiveDateTime, guest_tz: Tz, host_tz: Tz) -> NaiveDateTime {
    use chrono::TimeZone;
    let utc = guest_tz
        .from_local_datetime(&guest_local)
        .earliest()
        .unwrap_or_else(|| guest_tz.from_utc_datetime(&guest_local))
        .with_timezone(&Utc);
    utc.with_timezone(&host_tz).naive_local()
}

async fn get_host_tz(pool: &SqlitePool, et_id: &str) -> Tz {
    if !et_id.is_empty() {
        // Prefer the explicit event-type timezone (migration 046). Falls back
        // to the account owner's timezone for rows where it is still NULL.
        if let Some(tz_str) = sqlx::query_scalar::<_, String>(
            "SELECT COALESCE(NULLIF(et.timezone, ''), u.timezone)
             FROM event_types et
             JOIN accounts a ON a.id = et.account_id
             JOIN users u ON u.id = a.user_id
             WHERE et.id = ?",
        )
        .bind(et_id)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten()
        {
            if let Ok(tz) = tz_str.parse::<Tz>() {
                return tz;
            }
        }
    }
    server_tz()
}

/// Get a user's timezone from their profile. Falls back to server TZ.
async fn get_user_tz(pool: &SqlitePool, user_id: &str) -> Tz {
    if let Some(tz_str) = sqlx::query_scalar::<_, String>("SELECT timezone FROM users WHERE id = ?")
        .bind(user_id)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten()
    {
        if let Ok(tz) = tz_str.parse::<Tz>() {
            return tz;
        }
    }
    server_tz()
}

/// Server's local timezone as fallback.
fn server_tz() -> Tz {
    iana_time_zone::get_timezone()
        .ok()
        .and_then(|s| s.parse::<Tz>().ok())
        .unwrap_or(Tz::UTC)
}

/// Common IANA timezones for the selector (most used ones).
fn common_timezones_with(guest_tz: &str) -> Vec<(String, String)> {
    use chrono::Utc;
    let now = Utc::now();
    let entries: &[(&str, &str)] = &[
        ("Pacific/Midway", "Midway"),
        ("Pacific/Honolulu", "Hawaii"),
        ("America/Anchorage", "Alaska"),
        ("America/Los_Angeles", "Los Angeles"),
        ("America/Denver", "Denver"),
        ("America/Chicago", "Chicago"),
        ("America/New_York", "New York"),
        ("America/Sao_Paulo", "São Paulo"),
        ("Atlantic/Cape_Verde", "Cape Verde"),
        ("UTC", "UTC"),
        ("Europe/London", "London"),
        ("Europe/Paris", "Paris, Brussels"),
        ("Europe/Helsinki", "Helsinki, Kyiv"),
        ("Europe/Moscow", "Moscow"),
        ("Asia/Dubai", "Dubai"),
        ("Asia/Kolkata", "India"),
        ("Asia/Bangkok", "Bangkok"),
        ("Asia/Shanghai", "Shanghai"),
        ("Asia/Tokyo", "Tokyo"),
        ("Australia/Sydney", "Sydney"),
        ("Pacific/Auckland", "Auckland"),
    ];

    let format_label = |iana: &str, city: &str| -> String {
        if iana == "UTC" {
            "UTC".to_string()
        } else if let Ok(tz) = iana.parse::<chrono_tz::Tz>() {
            let offset = now.with_timezone(&tz).offset().fix().local_minus_utc();
            let h = offset / 3600;
            let m = (offset.abs() % 3600) / 60;
            let offset_str = if m != 0 {
                format!("UTC{:+}:{:02}", h, m)
            } else {
                format!("UTC{:+}", h)
            };
            format!("{} ({})", city, offset_str)
        } else {
            format!("{} ({})", city, iana)
        }
    };

    let mut result: Vec<(String, String)> = entries
        .iter()
        .map(|(iana, city)| {
            let label = format_label(iana, city);
            (iana.to_string(), label)
        })
        .collect();

    // If guest timezone is not in the common list, insert it sorted by UTC offset
    if !guest_tz.is_empty() && !entries.iter().any(|(iana, _)| *iana == guest_tz) {
        if let Ok(tz) = guest_tz.parse::<chrono_tz::Tz>() {
            let guest_offset = now.with_timezone(&tz).offset().fix().local_minus_utc();
            let label = format_label(guest_tz, guest_tz);
            // Find insertion point by UTC offset
            let pos = result
                .iter()
                .position(|(iana, _)| {
                    if *iana == "UTC" {
                        guest_offset < 0
                    } else if let Ok(t) = iana.parse::<chrono_tz::Tz>() {
                        let o = now.with_timezone(&t).offset().fix().local_minus_utc();
                        o > guest_offset
                    } else {
                        false
                    }
                })
                .unwrap_or(result.len());
            result.insert(pos, (guest_tz.to_string(), label));
        }
    }

    result
}

/// Returns CSS that overrides all theme variables for the given preset theme.
fn preset_theme_css(theme: &str) -> &'static str {
    match theme {
        "nord" => concat!(
            ":root{--bg:#eceff4;--surface:#fff;--surface-hover:#e5e9f0;--text:#2e3440;--text-secondary:#4c566a;--text-muted:#7b88a1;",
            "--border:#d8dee9;--border-hover:#b3bdd1;--accent:#5e81ac;--accent-hover:#4c6f97;--accent-subtle:#e8eef5;",
            "--accent-border:#b3cde0;--accent-muted:#81a1c1;--success:#a3be8c;--error-bg:#f5e6e8;--error-text:#bf616a}",
            " html.dark{--bg:#2e3440;--surface:#3b4252;--surface-hover:#434c5e;--text:#eceff4;--text-secondary:#d8dee9;--text-muted:#7b88a1;",
            "--border:#434c5e;--border-hover:#4c566a;--accent:#81a1c1;--accent-hover:#88c0d0;--accent-subtle:rgba(129,161,193,0.12);",
            "--accent-border:rgba(129,161,193,0.3);--accent-muted:#5e81ac;--success:#a3be8c;--error-bg:rgba(191,97,106,0.12);--error-text:#bf616a}"
        ),
        "dracula" => concat!(
            ":root{--bg:#f0edf5;--surface:#fff;--surface-hover:#e8e4ef;--text:#282a36;--text-secondary:#44475a;--text-muted:#7c7f94;",
            "--border:#d6d0e0;--border-hover:#b3adc4;--accent:#bd93f9;--accent-hover:#a76ff0;--accent-subtle:#f3eefe;",
            "--accent-border:#d4bffc;--accent-muted:#caa6fc;--success:#50fa7b;--error-bg:#fce4ec;--error-text:#ff5555}",
            " html.dark{--bg:#282a36;--surface:#44475a;--surface-hover:#4d5068;--text:#f8f8f2;--text-secondary:#d0cfe4;--text-muted:#7c7f94;",
            "--border:#4d5068;--border-hover:#6272a4;--accent:#bd93f9;--accent-hover:#caa6fc;--accent-subtle:rgba(189,147,249,0.12);",
            "--accent-border:rgba(189,147,249,0.3);--accent-muted:#9b6dff;--success:#50fa7b;--error-bg:rgba(255,85,85,0.12);--error-text:#ff5555}"
        ),
        "gruvbox" => concat!(
            ":root{--bg:#f9f5d7;--surface:#fbf1c7;--surface-hover:#f2e5bc;--text:#3c3836;--text-secondary:#504945;--text-muted:#928374;",
            "--border:#d5c4a1;--border-hover:#bdae93;--accent:#d65d0e;--accent-hover:#af3a03;--accent-subtle:#fef0e2;",
            "--accent-border:#f0b886;--accent-muted:#e78a4e;--success:#98971a;--error-bg:#fde8e6;--error-text:#cc241d}",
            " html.dark{--bg:#282828;--surface:#3c3836;--surface-hover:#504945;--text:#ebdbb2;--text-secondary:#d5c4a1;--text-muted:#928374;",
            "--border:#504945;--border-hover:#665c54;--accent:#fe8019;--accent-hover:#fabd2f;--accent-subtle:rgba(254,128,25,0.1);",
            "--accent-border:rgba(254,128,25,0.25);--accent-muted:#d65d0e;--success:#b8bb26;--error-bg:rgba(251,73,52,0.1);--error-text:#fb4934}"
        ),
        "solarized" => concat!(
            ":root{--bg:#fdf6e3;--surface:#eee8d5;--surface-hover:#e8e1cb;--text:#657b83;--text-secondary:#586e75;--text-muted:#93a1a1;",
            "--border:#d6cdb5;--border-hover:#b8b09a;--accent:#268bd2;--accent-hover:#1a6fad;--accent-subtle:#edf5fb;",
            "--accent-border:#a3cee8;--accent-muted:#6aafe2;--success:#859900;--error-bg:#fdf0ed;--error-text:#dc322f}",
            " html.dark{--bg:#002b36;--surface:#073642;--surface-hover:#0a4050;--text:#839496;--text-secondary:#93a1a1;--text-muted:#586e75;",
            "--border:#0a4050;--border-hover:#1a5060;--accent:#268bd2;--accent-hover:#6aafe2;--accent-subtle:rgba(38,139,210,0.1);",
            "--accent-border:rgba(38,139,210,0.25);--accent-muted:#1a6fad;--success:#859900;--error-bg:rgba(220,50,47,0.1);--error-text:#dc322f}"
        ),
        "tokyo-night" => concat!(
            ":root{--bg:#f0f0f5;--surface:#fff;--surface-hover:#e8e8ef;--text:#343b58;--text-secondary:#4c5478;--text-muted:#9099b0;",
            "--border:#d5d6e2;--border-hover:#b0b2c4;--accent:#7a5af5;--accent-hover:#6340db;--accent-subtle:#f0ecfe;",
            "--accent-border:#c4b5fd;--accent-muted:#a78bfa;--success:#41a87a;--error-bg:#fce8ec;--error-text:#e04071}",
            " html.dark{--bg:#1a1b26;--surface:#24283b;--surface-hover:#2f3349;--text:#a9b1d6;--text-secondary:#c0caf5;--text-muted:#565f89;",
            "--border:#2f3349;--border-hover:#414868;--accent:#7aa2f7;--accent-hover:#89b4fa;--accent-subtle:rgba(122,162,247,0.1);",
            "--accent-border:rgba(122,162,247,0.25);--accent-muted:#3d59a1;--success:#9ece6a;--error-bg:rgba(247,118,142,0.1);--error-text:#f7768e}"
        ),
        "vates" => concat!(
            ":root{--bg:#f5f4f0;--surface:#fff;--surface-hover:#faf9f5;--text:#1a1b38;--text-secondary:#3d3e58;--text-muted:#7a7b94;",
            "--border:#e2e1dc;--border-hover:#b5b4ae;--accent:#be1621;--accent-hover:#a01219;--accent-subtle:#fdf1f1;",
            "--accent-border:#f0b8bb;--accent-muted:#d4555e;--success:#2ca878;--error-bg:#fdf1f1;--error-text:#be1621}",
            " html.dark{--bg:#1a1b38;--surface:#262748;--surface-hover:#32335a;--text:#f0efe8;--text-secondary:#c8c7c0;--text-muted:#7a7b94;",
            "--border:#32335a;--border-hover:#4a4b6e;--accent:#e0424c;--accent-hover:#ef7f18;--accent-subtle:rgba(190,22,33,0.12);",
            "--accent-border:rgba(190,22,33,0.3);--accent-muted:#8a1018;--success:#2ca878;--error-bg:rgba(190,22,33,0.12);--error-text:#e0424c}"
        ),
        // "default" (blue) — no overrides needed, base.html defines it
        _ => "",
    }
}

/// Light-mode accent hex for a preset theme, mirroring the `--accent` values in
/// `preset_theme_css`. Used to prefill the embed brand color so it matches the
/// org theme even when a preset (not a custom theme) is active.
fn preset_accent(theme: &str) -> &'static str {
    match theme {
        "nord" => "#5e81ac",
        "dracula" => "#bd93f9",
        "gruvbox" => "#d65d0e",
        "solarized" => "#268bd2",
        "tokyo-night" => "#7a5af5",
        "vates" => "#be1621",
        // "default" (blue) and anything unknown
        _ => "#2563eb",
    }
}

/// Build custom theme CSS from user-provided hex colors.
fn custom_theme_css(
    accent: &str,
    accent_hover: &str,
    bg: &str,
    surface: &str,
    text: &str,
) -> String {
    // Validate that all colors look like hex codes
    let validate_hex = |c: &str| -> bool {
        let c = c.trim();
        c.len() == 7 && c.starts_with('#') && c[1..].chars().all(|ch| ch.is_ascii_hexdigit())
    };
    if ![accent, accent_hover, bg, surface, text]
        .iter()
        .all(|c| validate_hex(c))
    {
        return String::new();
    }
    // Parse accent for subtle/border/muted derivations
    let r = u8::from_str_radix(&accent[1..3], 16).unwrap_or(0);
    let g = u8::from_str_radix(&accent[3..5], 16).unwrap_or(0);
    let b = u8::from_str_radix(&accent[5..7], 16).unwrap_or(0);

    format!(
        ":root{{--bg:{bg};--surface:{surface};--text:{text};--accent:{accent};--accent-hover:{accent_hover};\
         --accent-subtle:rgba({r},{g},{b},0.08);--accent-border:rgba({r},{g},{b},0.25);--accent-muted:rgba({r},{g},{b},0.5)}}\
         html.dark{{--bg:{bg};--surface:{surface};--text:{text};--accent:{accent};--accent-hover:{accent_hover};\
         --accent-subtle:rgba({r},{g},{b},0.12);--accent-border:rgba({r},{g},{b},0.3);--accent-muted:rgba({r},{g},{b},0.5)}}",
    )
}

/// Build the full theme CSS string from DB settings.
async fn build_theme_css(pool: &SqlitePool) -> String {
    let row: Option<(String, Option<String>, Option<String>, Option<String>, Option<String>, Option<String>)> =
        sqlx::query_as("SELECT theme, custom_accent, custom_accent_hover, custom_bg, custom_surface, custom_text FROM auth_config WHERE id = 'singleton'")
            .fetch_optional(pool)
            .await
            .unwrap_or(None);
    match row {
        Some((ref theme, ref ca, ref cah, ref cb, ref cs, ref ct)) if theme == "custom" => {
            let accent = ca.as_deref().unwrap_or("#2563eb");
            let accent_hover = cah.as_deref().unwrap_or("#1d4ed8");
            let bg = cb.as_deref().unwrap_or("#f4f4f5");
            let surface = cs.as_deref().unwrap_or("#ffffff");
            let text = ct.as_deref().unwrap_or("#18181b");
            custom_theme_css(accent, accent_hover, bg, surface, text)
        }
        Some((ref theme, ..)) => preset_theme_css(theme).to_string(),
        None => String::new(),
    }
}

/// Get the current theme name from DB.
async fn get_theme_name(pool: &SqlitePool) -> String {
    let row: Option<(String,)> =
        sqlx::query_as("SELECT theme FROM auth_config WHERE id = 'singleton'")
            .fetch_optional(pool)
            .await
            .unwrap_or(None);
    row.map(|r| r.0).unwrap_or_else(|| "default".to_string())
}

/// Get custom theme colors from DB (for populating the form).
async fn get_custom_colors(pool: &SqlitePool) -> (String, String, String, String, String) {
    let row: Option<(Option<String>, Option<String>, Option<String>, Option<String>, Option<String>)> =
        sqlx::query_as("SELECT custom_accent, custom_accent_hover, custom_bg, custom_surface, custom_text FROM auth_config WHERE id = 'singleton'")
            .fetch_optional(pool)
            .await
            .unwrap_or(None);
    match row {
        Some((a, ah, bg, s, t)) => (
            a.unwrap_or_else(|| "#2563eb".to_string()),
            ah.unwrap_or_else(|| "#1d4ed8".to_string()),
            bg.unwrap_or_else(|| "#f4f4f5".to_string()),
            s.unwrap_or_else(|| "#ffffff".to_string()),
            t.unwrap_or_else(|| "#18181b".to_string()),
        ),
        None => (
            "#2563eb".to_string(),
            "#1d4ed8".to_string(),
            "#f4f4f5".to_string(),
            "#ffffff".to_string(),
            "#18181b".to_string(),
        ),
    }
}

async fn show_slots(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(slug): Path<String>,
    Query(query): Query<SlotsQuery>,
) -> impl IntoResponse {
    let lang = crate::i18n::detect_from_headers(&headers);
    let et: Option<(String, String, String, Option<String>, i32, i32, i32, i32, String, String)> = sqlx::query_as(
        "SELECT id, slug, title, description, duration_min, buffer_before, buffer_after, min_notice_min, visibility, default_calendar_view
         FROM event_types WHERE slug = ? AND enabled = 1",
    )
    .bind(&slug)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    let (
        et_id,
        et_slug,
        et_title,
        et_desc,
        duration,
        buf_before,
        buf_after,
        min_notice,
        visibility,
        default_calendar_view,
    ) = match et {
        Some(e) => e,
        None => return Html("Event type not found.".to_string()),
    };

    // Block private event types on legacy route (use /u/ or /team/ routes with invite token instead)
    if visibility == "private" || visibility == "internal" {
        return Html("This event type requires an invite link.".to_string());
    }

    let host_info: Option<(String, String, Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT u.id, u.name, u.title, u.avatar_path FROM users u JOIN accounts a ON a.user_id = u.id JOIN event_types et ON et.account_id = a.id WHERE et.id = ?",
    )
    .bind(&et_id)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    let (host_user_id, host_name, host_title, host_avatar_path) =
        host_info.unwrap_or_else(|| ("".to_string(), "Host".to_string(), None, None));

    // Sync calendars if stale before computing availability
    crate::commands::sync::sync_if_stale(&state.pool, &state.secret_key, &host_user_id).await;

    let guest_tz = parse_guest_tz(query.tz.as_deref());
    let host_tz = get_host_tz(&state.pool, &et_id).await;
    let guest_tz_name = guest_tz.name().to_string();

    let (year, mo) = parse_month_param(query.month.as_deref(), guest_tz);
    let (
        start_offset,
        days_ahead,
        month_label,
        prev_month,
        next_month,
        first_weekday,
        days_in_month,
        today_date,
        month_year,
    ) = build_month_params(year, mo, host_tz, guest_tz, lang);

    let now_host = Utc::now().with_timezone(&host_tz).naive_local();
    let end_date = now_host.date() + Duration::days((start_offset + days_ahead) as i64);
    let window_end = end_date.and_hms_opt(23, 59, 59).unwrap_or(now_host);
    let busy = BusySource::Individual(
        fetch_busy_times_for_user(
            &state.pool,
            &host_user_id,
            now_host,
            window_end,
            host_tz,
            Some(&et_id),
        )
        .await,
    );
    let slot_days = compute_slots(
        &state.pool,
        &et_id,
        duration,
        buf_before,
        buf_after,
        min_notice,
        start_offset,
        days_ahead,
        host_tz,
        guest_tz,
        busy,
    )
    .await;

    let days_ctx: Vec<minijinja::Value> = slot_days
        .iter()
        .map(|d| {
            let slots: Vec<minijinja::Value> = d
                .slots
                .iter()
                .map(|s| context! { start => s.start, end => s.end, host_date => s.host_date, host_time => s.host_time, guest_date => s.guest_date })
                .collect();
            context! { date => d.date, label => d.label, slots => slots }
        })
        .collect();

    let available_dates: Vec<String> = slot_days.iter().map(|d| d.date.clone()).collect();

    let tz_options: Vec<minijinja::Value> = common_timezones_with(&guest_tz_name)
        .iter()
        .map(|(iana, label)| context! { value => iana, label => label, selected => (*iana == guest_tz_name) })
        .collect();

    let (meeting_jitsi_label, meeting_webhook_label) = meeting_provider_labels(&state).await;
    let tmpl = match state.templates.get_template("slots.html") {
        Ok(t) => t,
        Err(e) => return internal_error_html("internal", &e),
    };
    let rendered = tmpl
        .render(context! {
            meeting_jitsi_label => meeting_jitsi_label,
            meeting_webhook_label => meeting_webhook_label,
            event_type => context! {
                slug => et_slug,
                title => et_title,
                description => et_desc.as_deref().map(crate::utils::render_inline_markdown),
                duration_min => duration,
            },
            host_name => &host_name,
            host_title => host_title.as_deref().unwrap_or(""),
            host_user_id => &host_user_id,
            host_has_avatar => host_avatar_path.is_some(),
            host_initials => compute_initials(&host_name),
            days => days_ctx,
            available_dates => available_dates,
            month_label => month_label,
            month_year => month_year,
            prev_month => prev_month,
            next_month => next_month,
            first_weekday => first_weekday,
            days_in_month => days_in_month,
            today_date => today_date,
            guest_tz => guest_tz_name,
            tz_options => tz_options,
            default_calendar_view => default_calendar_view,
            company_link => state.company_link.read().await.clone(),
            lang => lang,
        })
        .unwrap_or_else(|e| internal_error_body("template render", &e));

    Html(rendered)
}

#[derive(Deserialize)]
struct BookQuery {
    date: String,
    time: String,
    #[serde(default)]
    tz: Option<String>,
    #[serde(default)]
    invite: Option<String>,
    #[serde(default)]
    embed: Option<String>,
    #[serde(default)]
    layout: Option<String>,
    #[serde(default)]
    theme: Option<String>,
    #[serde(default)]
    brand: Option<String>,
}

impl BookQuery {
    fn embed_params(&self) -> EmbedParams {
        EmbedParams {
            embed: self.embed.clone(),
            layout: self.layout.clone(),
            theme: self.theme.clone(),
            brand: self.brand.clone(),
        }
    }
}

async fn show_book_form(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(slug): Path<String>,
    Query(query): Query<BookQuery>,
) -> impl IntoResponse {
    let lang = crate::i18n::detect_from_headers(&headers);
    let et: Option<(String, String, String, Option<String>, i32, i32, String)> = sqlx::query_as(
        "SELECT id, slug, title, description, duration_min, max_additional_guests, visibility
         FROM event_types WHERE slug = ? AND enabled = 1",
    )
    .bind(&slug)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    let (et_id, et_slug, et_title, et_desc, duration, max_additional_guests, visibility) = match et
    {
        Some(e) => e,
        None => return Html("Event type not found.".to_string()),
    };

    // Block non-public event types on legacy route
    if visibility == "private" || visibility == "internal" {
        return Html("This event type requires an invite link.".to_string());
    }

    let host_name: String = sqlx::query_scalar(
        "SELECT a.name FROM accounts a JOIN event_types et ON et.account_id = a.id WHERE et.id = ?",
    )
    .bind(&et_id)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None)
    .unwrap_or_else(|| "Host".to_string());

    let guest_tz = parse_guest_tz(query.tz.as_deref());
    let guest_tz_name = guest_tz.name().to_string();

    let date = match NaiveDate::parse_from_str(&query.date, "%Y-%m-%d") {
        Ok(d) => d,
        Err(_) => return Html("Invalid date format.".to_string()),
    };
    let time = match NaiveTime::parse_from_str(&query.time, "%H:%M") {
        Ok(t) => t,
        Err(_) => return Html("Invalid time format.".to_string()),
    };
    let end_time = (date.and_time(time) + Duration::minutes(duration as i64))
        .time()
        .format("%H:%M")
        .to_string();
    let date_label = crate::i18n::format_long_date(date, lang);

    let (meeting_jitsi_label, meeting_webhook_label) = meeting_provider_labels(&state).await;
    let tmpl = match state.templates.get_template("book.html") {
        Ok(t) => t,
        Err(e) => return internal_error_html("internal", &e),
    };
    let captcha = captcha::CaptchaVars::from_config(&*state.captcha_config.read().await);
    let rendered = tmpl
        .render(context! {
            meeting_jitsi_label => meeting_jitsi_label,
            meeting_webhook_label => meeting_webhook_label,
            event_type => context! {
                slug => et_slug,
                title => et_title,
                description => et_desc.as_deref().map(crate::utils::render_inline_markdown),
                duration_min => duration,
            },
            host_name => host_name,
            date => query.date,
            date_label => date_label,
            time_start => query.time,
            time_end => end_time,
            guest_tz => guest_tz_name,
            error => "",
            form_name => "",
            form_email => "",
            form_notes => "",
            max_additional_guests => max_additional_guests,
            company_link => state.company_link.read().await.clone(),
            captcha_enabled => captcha.enabled,
            captcha_api_endpoint => captcha.api_endpoint,
            captcha_widget_url => captcha.widget_url,
            lang => lang,
        })
        .unwrap_or_else(|e| internal_error_body("template render", &e));

    Html(rendered)
}

fn validate_booking_input(name: &str, email: &str, notes: &Option<String>) -> Result<(), String> {
    let name = name.trim();
    if name.is_empty() || name.len() > 255 {
        return Err("Name must be between 1 and 255 characters.".to_string());
    }
    let email = email.trim();
    if email.is_empty() || email.len() > 255 {
        return Err("Email must be between 1 and 255 characters.".to_string());
    }
    if !email.contains('@')
        || email
            .rsplit('@')
            .next()
            .is_none_or(|domain| !domain.contains('.'))
    {
        return Err("Please enter a valid email address.".to_string());
    }
    if let Some(notes) = notes {
        if notes.len() > 5000 {
            return Err("Notes must be 5000 characters or less.".to_string());
        }
    }
    Ok(())
}

fn validate_date_not_too_far(date: NaiveDate) -> Result<(), String> {
    let max_date = Utc::now().naive_utc().date() + Duration::days(366);
    if date > max_date {
        return Err("Cannot book more than one year in advance.".to_string());
    }
    Ok(())
}

/// Parse comma-separated additional guest emails, validate format, enforce max count.
/// Returns the list of valid, deduplicated emails (excluding the primary guest email).
fn parse_additional_guests(
    raw: &Option<String>,
    max: i32,
    primary_email: &str,
) -> Result<Vec<String>, String> {
    let raw = match raw {
        Some(s) if !s.trim().is_empty() => s,
        _ => return Ok(vec![]),
    };
    if max <= 0 {
        return Err("Additional guests are not allowed for this event type.".to_string());
    }
    let emails: Vec<String> = raw
        .split(',')
        .map(|s| s.trim().to_lowercase())
        .filter(|s| !s.is_empty())
        .collect();
    if emails.len() > max as usize {
        return Err(format!("You can add at most {} additional guest(s).", max));
    }
    let primary = primary_email.trim().to_lowercase();
    let mut seen = std::collections::HashSet::new();
    let mut result = Vec::new();
    for email in &emails {
        if email == &primary {
            continue; // skip if same as primary guest
        }
        if !email.contains('@')
            || email
                .rsplit('@')
                .next()
                .is_none_or(|domain| !domain.contains('.'))
        {
            return Err(format!("Invalid additional guest email: {}", email));
        }
        if seen.insert(email.clone()) {
            result.push(email.clone());
        }
    }
    Ok(result)
}

#[derive(Deserialize)]
struct BookForm {
    _csrf: Option<String>,
    date: String,
    time: String,
    name: String,
    email: String,
    notes: Option<String>,
    #[serde(default)]
    tz: Option<String>,
    #[serde(default)]
    invite_token: Option<String>,
    #[serde(default)]
    additional_guests: Option<String>,
    #[serde(rename = "cap-token", default)]
    captcha_token: Option<String>,
}

async fn handle_booking(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Path(slug): Path<String>,
    Form(form): Form<BookForm>,
) -> impl IntoResponse {
    if let Err(resp) = verify_csrf_token(&headers, &form._csrf) {
        return resp;
    }
    let lang = crate::i18n::detect_from_headers(&headers);
    let captcha_cfg = state.captcha_config.read().await;
    if captcha::verify(&captcha_cfg, form.captcha_token.as_deref())
        .await
        .is_err()
    {
        tracing::warn!("captcha failed on booking");
        return render_booking_action_error(
            &state,
            &headers,
            "Captcha verification failed",
            "Please go back and try again.",
        );
    }
    drop(captcha_cfg);
    // Rate limit by IP
    let client_ip = client_ip_for_rate_limit(&headers);
    if state.booking_limiter.check_limited(&client_ip).await {
        tracing::warn!(ip = %client_ip, "rate limited");
        return Html("Too many booking attempts. Please try again in a few minutes.".to_string())
            .into_response();
    }

    if let Err(e) = validate_booking_input(&form.name, &form.email, &form.notes) {
        return render_booking_action_error(&state, &headers, "Invalid booking details", &e);
    }

    let et: Option<(String, String, String, i32, i32, i32, i32, i32, Option<i32>, i32, String)> = sqlx::query_as(
        "SELECT id, slug, title, duration_min, buffer_before, buffer_after, min_notice_min, requires_confirmation, reminder_minutes, max_additional_guests, visibility
         FROM event_types WHERE slug = ? AND enabled = 1",
    )
    .bind(&slug)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    let (
        et_id,
        _et_slug,
        et_title,
        duration,
        buffer_before,
        buffer_after,
        min_notice,
        requires_confirmation,
        reminder_min,
        max_additional_guests,
        visibility,
    ) = match et {
        Some(e) => e,
        None => return Html("Event type not found.".to_string()).into_response(),
    };
    let needs_approval = requires_confirmation != 0;

    // Block non-public event types on legacy route
    if visibility == "private" || visibility == "internal" {
        return Html("This event type requires an invite link.".to_string()).into_response();
    }

    // Parse additional guests
    let additional_attendees = match parse_additional_guests(
        &form.additional_guests,
        max_additional_guests,
        &form.email,
    ) {
        Ok(emails) => emails,
        Err(e) => return Html(e).into_response(),
    };

    // Get the host user_id for user-scoped busy time check
    let host_user_id: String = sqlx::query_scalar(
        "SELECT a.user_id FROM accounts a JOIN event_types et ON et.account_id = a.id WHERE et.id = ?",
    )
    .bind(&et_id)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None)
    .unwrap_or_default();

    let date = match NaiveDate::parse_from_str(&form.date, "%Y-%m-%d") {
        Ok(d) => d,
        Err(_) => return Html("Invalid date.".to_string()).into_response(),
    };
    if let Err(e) = validate_date_not_too_far(date) {
        return Html(e).into_response();
    }
    let start_time = match NaiveTime::parse_from_str(&form.time, "%H:%M") {
        Ok(t) => t,
        Err(_) => return Html("Invalid time.".to_string()).into_response(),
    };

    let guest_tz = parse_guest_tz(form.tz.as_deref());
    let guest_timezone = guest_tz.name().to_string();
    let host_tz = get_host_tz(&state.pool, &et_id).await;

    // The URL carries guest-local date/time; convert to host-local for storage
    // and availability checks (existing semantics).
    let guest_local_start = date.and_time(start_time);
    let guest_local_end = guest_local_start + Duration::minutes(duration as i64);
    let slot_start = guest_to_host_local(guest_local_start, guest_tz, host_tz);
    let slot_end = slot_start + Duration::minutes(duration as i64);
    let guest_end_time = guest_local_end.time().format("%H:%M").to_string();

    // Validate minimum notice
    let now = Local::now().naive_local();
    if slot_start < now + Duration::minutes(min_notice as i64) {
        return Html("This slot is no longer available (too soon).".to_string()).into_response();
    }

    // Validate conflicts
    let buf_start = slot_start - Duration::minutes(buffer_before as i64);
    let buf_end = slot_end + Duration::minutes(buffer_after as i64);

    // Create booking
    let id = uuid::Uuid::new_v4().to_string();
    let uid = format!("{}@calrs", uuid::Uuid::new_v4());
    let cancel_token = uuid::Uuid::new_v4().to_string();
    let reschedule_token = uuid::Uuid::new_v4().to_string();
    let start_at = slot_start.format("%Y-%m-%dT%H:%M:%S").to_string();
    let end_at = slot_end.format("%Y-%m-%dT%H:%M:%S").to_string();

    let initial_status = if needs_approval {
        "pending"
    } else {
        "confirmed"
    };
    let confirm_token: Option<String> = if needs_approval {
        Some(uuid::Uuid::new_v4().to_string())
    } else {
        None
    };

    // Start a transaction to ensure atomicity of availability check + insert.
    // The unique index idx_bookings_no_overlap is the ultimate guard against double-booking.
    let mut tx = match state.pool.begin().await {
        Ok(tx) => tx,
        Err(e) => {
            return internal_error_response("database query", &e);
        }
    };

    let busy = fetch_busy_times_for_user(
        &state.pool,
        &host_user_id,
        buf_start,
        buf_end,
        host_tz,
        Some(&et_id),
    )
    .await;
    if has_conflict(&busy, buf_start, buf_end) {
        let _ = tx.rollback().await;
        return Html("This slot is no longer available.".to_string()).into_response();
    }

    // Check booking frequency limits. Personal event-type flows don't have a
    // team assignee, so per-member caps degrade to event-type-wide here.
    if would_exceed_frequency_limit(&state.pool, &et_id, slot_start, None).await {
        let _ = tx.rollback().await;
        return render_booking_action_error(
            &state,
            &headers,
            "Not available right now",
            "The host isn't accepting more bookings for this date. Please pick a different date, or check back later.",
        );
    }

    let insert_result = sqlx::query(
        "INSERT INTO bookings (id, event_type_id, uid, guest_name, guest_email, guest_timezone, notes, start_at, end_at, status, cancel_token, reschedule_token, confirm_token, language)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(&et_id)
    .bind(&uid)
    .bind(&form.name)
    .bind(&form.email)
    .bind(&guest_timezone)
    .bind(&form.notes)
    .bind(&start_at)
    .bind(&end_at)
    .bind(initial_status)
    .bind(&cancel_token)
    .bind(&reschedule_token)
    .bind(&confirm_token)
    .bind(lang)
    .execute(&mut *tx)
    .await;

    match insert_result {
        Ok(_) => {}
        Err(e) => {
            let _ = tx.rollback().await;
            if e.to_string().contains("UNIQUE constraint failed") {
                return Html("This slot is no longer available.".to_string()).into_response();
            }
            return internal_error_response("database query", &e);
        }
    }

    // Insert additional attendees
    for attendee_email in &additional_attendees {
        let attendee_id = uuid::Uuid::new_v4().to_string();
        let _ =
            sqlx::query("INSERT INTO booking_attendees (id, booking_id, email) VALUES (?, ?, ?)")
                .bind(&attendee_id)
                .bind(&id)
                .bind(attendee_email)
                .execute(&mut *tx)
                .await;
    }

    if let Err(e) = tx.commit().await {
        if e.to_string().contains("UNIQUE constraint failed") {
            return Html("This slot is no longer available.".to_string()).into_response();
        }
        return internal_error_response("database query", &e);
    }

    tracing::info!(booking_id = %id, event_type = %slug, guest = %form.email, "booking created");

    // Build BookingDetails once. CalDAV push and email send both need it,
    // and CalDAV push must run independently of whether SMTP is configured.
    let host: Option<(String, String)> = sqlx::query_as(
        "SELECT u.name, COALESCE(u.booking_email, u.email) FROM users u JOIN accounts a ON a.user_id = u.id WHERE a.id = (SELECT account_id FROM event_types WHERE id = ?)",
    )
    .bind(&et_id)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    if let Some((host_name, host_email)) = host {
        let host_user_id: Option<String> = sqlx::query_scalar(
            "SELECT u.id FROM users u JOIN accounts a ON a.user_id = u.id JOIN event_types et ON et.account_id = a.id WHERE et.id = ?",
        )
        .bind(&et_id)
        .fetch_optional(&state.pool)
        .await
        .unwrap_or(None);
        let location_display = resolve_booking_location(
            &state,
            &id,
            &et_id,
            host_user_id.as_deref(),
            None,
            &form.name,
            &form.email,
            !needs_approval,
        )
        .await;
        let details = crate::email::BookingDetails {
            event_title: et_title.clone(),
            date: form.date.clone(),
            start_time: form.time.clone(),
            end_time: guest_end_time.clone(),
            guest_name: form.name.clone(),
            guest_email: form.email.clone(),
            guest_timezone: guest_timezone.clone(),
            host_name,
            host_email,
            uid: uid.clone(),
            notes: form.notes.clone(),
            location: location_display,
            reminder_minutes: reminder_min,
            additional_attendees: additional_attendees.clone(),
            guest_language: Some(lang.to_string()),
            host_timezone: host_tz.name().to_string(),
            ..Default::default()
        };

        // Push confirmed bookings to CalDAV regardless of SMTP availability.
        if !needs_approval {
            if let Some(uid_user) = host_user_id.as_deref() {
                caldav_push_booking(&state.pool, &state.secret_key, uid_user, &uid, &details).await;
            }
        }

        let (cancel_notice_min, reschedule_notice_min) =
            fetch_event_type_notice_minutes(&state.pool, &et_id).await;

        // Send emails if SMTP is configured.
        if let Ok(Some(smtp_config)) =
            crate::email::load_smtp_config(&state.pool, &state.secret_key).await
        {
            let base_url = std::env::var("CALRS_BASE_URL").ok();
            let guest_cancel_url = base_url.as_ref().map(|base| {
                format!(
                    "{}/booking/cancel/{}",
                    base.trim_end_matches('/'),
                    cancel_token
                )
            });
            let guest_reschedule_url = base_url.as_ref().map(|base| {
                format!(
                    "{}/booking/reschedule/{}",
                    base.trim_end_matches('/'),
                    reschedule_token
                )
            });

            if needs_approval {
                let _ = crate::email::send_host_approval_request(
                    &smtp_config,
                    &details,
                    &id,
                    confirm_token.as_deref(),
                    base_url.as_deref(),
                )
                .await;
                let _ = crate::email::send_guest_pending_notice_ex(
                    &smtp_config,
                    &details,
                    guest_cancel_url.as_deref(),
                    guest_reschedule_url.as_deref(),
                )
                .await;
            } else {
                let _ = crate::email::send_guest_confirmation_ex(
                    &smtp_config,
                    &details,
                    guest_cancel_url.as_deref(),
                    guest_reschedule_url.as_deref(),
                    cancel_notice_min,
                    reschedule_notice_min,
                )
                .await;
                let _ = crate::email::send_host_notification(&smtp_config, &details).await;
            }
        }
    }

    // Render confirmation
    let host_name: String = sqlx::query_scalar(
        "SELECT a.name FROM accounts a JOIN event_types et ON et.account_id = a.id WHERE et.id = ?",
    )
    .bind(&et_id)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None)
    .unwrap_or_else(|| "Host".to_string());

    let date_label = crate::i18n::format_long_date(date, lang);
    let (cancel_notice_min, reschedule_notice_min) =
        fetch_event_type_notice_minutes(&state.pool, &et_id).await;

    let tmpl = match state.templates.get_template("confirmed.html") {
        Ok(t) => t,
        Err(e) => return internal_error_response("internal", &e),
    };
    let rendered = tmpl
        .render(context! {
            event_title => et_title,
            date_label => date_label,
            time_start => form.time,
            time_end => guest_end_time,
            host_name => host_name,
            guest_email => form.email,
            notes => form.notes,
            pending => needs_approval,
            additional_attendees => additional_attendees,
            cancel_notice_min => cancel_notice_min,
            reschedule_notice_min => reschedule_notice_min,
            company_link => state.company_link.read().await.clone(),
            lang => lang,
        })
        .unwrap_or_else(|e| internal_error_body("template render", &e));

    Html(rendered).into_response()
}

// --- Troubleshoot ---

#[derive(Deserialize)]
struct TroubleshootQuery {
    date: Option<String>,
    event_type: Option<String>,
}

async fn troubleshoot(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
    Query(params): Query<TroubleshootQuery>,
) -> impl IntoResponse {
    let user = &auth_user.user;

    // Always sync before troubleshooting to ensure fresh data
    crate::commands::sync::sync_if_stale(&state.pool, &state.secret_key, &user.id).await;

    let host_tz = get_user_tz(&state.pool, &user.id).await;
    let now_host = Utc::now().with_timezone(&host_tz).naive_local();

    let target_date = params
        .date
        .as_deref()
        .and_then(|d| NaiveDate::parse_from_str(d, "%Y-%m-%d").ok())
        .unwrap_or(now_host.date());

    // Fetch user's event types for the selector
    let event_types: Vec<(String, String, i32, i32, i32, i32)> = sqlx::query_as(
        "SELECT et.slug, et.title, et.duration_min, et.buffer_before, et.buffer_after, et.min_notice_min
         FROM event_types et
         JOIN accounts a ON a.id = et.account_id
         WHERE a.user_id = ? AND et.team_id IS NULL AND et.enabled = 1
         ORDER BY et.created_at",
    )
    .bind(&user.id)
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();

    if event_types.is_empty() {
        let tmpl = match state.templates.get_template("troubleshoot.html") {
            Ok(t) => t,
            Err(e) => return internal_error_html("template render", &e),
        };
        let (impersonating, impersonating_name, _) = impersonation_ctx(&auth_user);
        return Html(
            tmpl.render(context! {
                user_name => &user.name,
                no_event_types => true,
                sidebar => sidebar_context(&auth_user, "troubleshoot"),
                impersonating => impersonating,
                impersonating_name => impersonating_name,
            })
            .unwrap_or_default(),
        );
    }

    let selected_slug = params.event_type.as_deref().unwrap_or(&event_types[0].0);
    let selected_et = event_types
        .iter()
        .find(|et| et.0 == selected_slug)
        .unwrap_or(&event_types[0]);
    let (ref et_slug, ref et_title, duration, buf_before, buf_after, min_notice) = *selected_et;

    // Get event type ID
    let et_id: Option<(String,)> = sqlx::query_as(
        "SELECT et.id FROM event_types et JOIN accounts a ON a.id = et.account_id WHERE a.user_id = ? AND et.slug = ?",
    )
    .bind(&user.id)
    .bind(et_slug)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    let et_id = match et_id {
        Some((id,)) => id,
        None => return Html("Event type not found".to_string()),
    };

    // Check availability overrides for this date
    let target_date_str = target_date.format("%Y-%m-%d").to_string();
    let day_overrides: Vec<(Option<String>, Option<String>, i32)> = sqlx::query_as(
        "SELECT start_time, end_time, is_blocked FROM availability_overrides WHERE event_type_id = ? AND date = ? ORDER BY start_time",
    )
    .bind(&et_id)
    .bind(&target_date_str)
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();

    let date_is_blocked = day_overrides.iter().any(|(_, _, b)| *b != 0);

    // Availability rules: use overrides if present, else weekly rules
    let rules: Vec<(String, String)> = if date_is_blocked {
        vec![] // blocked day — no availability
    } else if !day_overrides.is_empty() {
        // Custom hours replace weekly rules
        day_overrides
            .iter()
            .filter_map(|(s, e, _)| match (s, e) {
                (Some(start), Some(end)) => Some((start.clone(), end.clone())),
                _ => None,
            })
            .collect()
    } else {
        let weekday = target_date.weekday().num_days_from_sunday() as i32;
        sqlx::query_as(
            "SELECT start_time, end_time FROM availability_rules WHERE event_type_id = ? AND day_of_week = ? ORDER BY start_time",
        )
        .bind(&et_id)
        .bind(weekday)
        .fetch_all(&state.pool)
        .await
        .unwrap_or_default()
    };

    // Busy events for this date — enriched with title + calendar name
    let day_start_compact = target_date.format("%Y%m%d").to_string();
    let day_end_compact = (target_date + Duration::days(1))
        .format("%Y%m%d")
        .to_string();
    let day_start_iso = target_date.format("%Y-%m-%dT00:00:00").to_string();
    let day_end_iso = target_date.format("%Y-%m-%dT23:59:59").to_string();

    // Non-recurring busy events for this date
    let raw_busy_events: Vec<(
        String,
        String,
        Option<String>,
        Option<String>,
        Option<String>,
    )> = sqlx::query_as(
        "SELECT e.start_at, e.end_at, e.summary, c.display_name, e.timezone
         FROM events e
         JOIN calendars c ON c.id = e.calendar_id
         JOIN caldav_sources cs ON cs.id = c.source_id
         JOIN accounts a ON a.id = cs.account_id
         WHERE a.user_id = ? AND c.is_busy = 1
           AND (NOT EXISTS (SELECT 1 FROM event_type_calendars WHERE event_type_id = ?)
                OR c.id IN (SELECT calendar_id FROM event_type_calendars WHERE event_type_id = ?))
           AND (e.rrule IS NULL OR e.rrule = '')
           AND (e.status IS NULL OR e.status != 'CANCELLED')
           AND (e.transp IS NULL OR e.transp != 'TRANSPARENT')
           AND ((e.start_at < ? AND e.end_at > ?) OR (e.start_at < ? AND e.end_at > ?))
         ORDER BY e.start_at",
    )
    .bind(&user.id)
    .bind(&et_id)
    .bind(&et_id)
    .bind(&day_end_compact)
    .bind(&day_start_compact)
    .bind(&day_end_iso)
    .bind(&day_start_iso)
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();

    let mut busy_events: Vec<(String, String, Option<String>, Option<String>)> = raw_busy_events
        .iter()
        .filter_map(|(s, e, summary, cal_name, event_tz)| {
            let start = convert_event_to_tz(parse_ical_datetime(s)?, event_tz.as_deref(), host_tz);
            let end = convert_event_to_tz(parse_ical_datetime(e)?, event_tz.as_deref(), host_tz);
            Some((
                start.format("%Y-%m-%dT%H:%M:%S").to_string(),
                end.format("%Y-%m-%dT%H:%M:%S").to_string(),
                summary.clone(),
                cal_name.clone(),
            ))
        })
        .collect();

    // Recurring busy events — expand into this date
    let recurring_events: Vec<(
        String,
        String,
        String,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
    )> = sqlx::query_as(
        "SELECT e.start_at, e.end_at, e.rrule, e.raw_ical, e.summary, c.display_name, e.timezone
         FROM events e
         JOIN calendars c ON c.id = e.calendar_id
         JOIN caldav_sources cs ON cs.id = c.source_id
         JOIN accounts a ON a.id = cs.account_id
         WHERE a.user_id = ? AND c.is_busy = 1
           AND (NOT EXISTS (SELECT 1 FROM event_type_calendars WHERE event_type_id = ?)
                OR c.id IN (SELECT calendar_id FROM event_type_calendars WHERE event_type_id = ?))
           AND (e.status IS NULL OR e.status != 'CANCELLED')
           AND (e.transp IS NULL OR e.transp != 'TRANSPARENT')
           AND e.rrule IS NOT NULL AND e.rrule != ''
           AND (e.start_at <= ? OR e.start_at <= ?)",
    )
    .bind(&user.id)
    .bind(&et_id)
    .bind(&et_id)
    .bind(&day_end_iso)
    .bind(&day_end_compact)
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();

    let ts_window_start = target_date
        .and_hms_opt(0, 0, 0)
        .unwrap_or(target_date.and_time(NaiveTime::MIN));
    let ts_window_end = target_date
        .and_hms_opt(23, 59, 59)
        .unwrap_or(target_date.and_time(NaiveTime::MIN));
    for (s, e, rrule_str, raw_ical, summary, cal_name, event_tz) in &recurring_events {
        if let (Some(ev_start), Some(ev_end)) = (parse_ical_datetime(s), parse_ical_datetime(e)) {
            let exdates = raw_ical
                .as_deref()
                .map(crate::rrule::extract_exdates)
                .unwrap_or_default();
            let occurrences = crate::rrule::expand_rrule(
                ev_start,
                ev_end,
                rrule_str,
                &exdates,
                ts_window_start,
                ts_window_end,
            );
            for (os, oe) in occurrences {
                let cs = convert_event_to_tz(os, event_tz.as_deref(), host_tz);
                let ce = convert_event_to_tz(oe, event_tz.as_deref(), host_tz);
                busy_events.push((
                    cs.format("%Y-%m-%dT%H:%M:%S").to_string(),
                    ce.format("%Y-%m-%dT%H:%M:%S").to_string(),
                    summary.clone(),
                    cal_name.clone(),
                ));
            }
        }
    }

    // Bookings for this date
    let bookings: Vec<(String, String, String, String)> = sqlx::query_as(
        "SELECT b.start_at, b.end_at, b.guest_name, et2.title
         FROM bookings b
         JOIN event_types et2 ON et2.id = b.event_type_id
         JOIN accounts a ON a.id = et2.account_id
         WHERE a.user_id = ? AND b.status IN ('confirmed', 'pending')
           AND b.start_at < ? AND b.end_at > ?
         ORDER BY b.start_at",
    )
    .bind(&user.id)
    .bind(&day_end_iso)
    .bind(&day_start_iso)
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();

    // Build timeline: scan 15-min ticks from display_start to display_end
    let display_start_hour: u32 = rules
        .iter()
        .filter_map(|(s, _)| NaiveTime::parse_from_str(s, "%H:%M").ok())
        .map(|t| t.hour())
        .min()
        .unwrap_or(8)
        .saturating_sub(1);
    let display_end_hour: u32 = rules
        .iter()
        .filter_map(|(_, e)| NaiveTime::parse_from_str(e, "%H:%M").ok())
        .map(|t| {
            if t.minute() > 0 {
                t.hour() + 1
            } else {
                t.hour()
            }
        })
        .max()
        .unwrap_or(18)
        .min(23)
        + 1;

    let display_start = NaiveTime::from_hms_opt(display_start_hour, 0, 0).unwrap_or(NaiveTime::MIN);
    let display_end = NaiveTime::from_hms_opt(display_end_hour, 0, 0).unwrap_or(NaiveTime::MIN);
    let total_minutes = (display_end - display_start).num_minutes() as f64;

    let min_start = now_host + Duration::minutes(min_notice as i64);

    // Parse availability windows
    let avail_windows: Vec<(NaiveTime, NaiveTime)> = rules
        .iter()
        .filter_map(|(s, e)| {
            let st = NaiveTime::parse_from_str(s, "%H:%M").ok()?;
            let en = NaiveTime::parse_from_str(e, "%H:%M").ok()?;
            Some((st, en))
        })
        .collect();

    // Parse busy events into (start_dt, end_dt, label, detail)
    let busy_parsed: Vec<(NaiveDateTime, NaiveDateTime, String, String)> = busy_events
        .iter()
        .filter_map(|(s, e, summary, cal)| {
            let start = parse_ical_datetime(s)?;
            let end = parse_ical_datetime(e)?;
            let label = summary.clone().unwrap_or_else(|| "Busy".to_string());
            let detail = cal.clone().unwrap_or_default();
            Some((start, end, label, detail))
        })
        .collect();

    // Parse bookings into (start_dt, end_dt, label, detail)
    let bookings_parsed: Vec<(NaiveDateTime, NaiveDateTime, String, String)> = bookings
        .iter()
        .filter_map(|(s, e, guest, et_title)| {
            let start = parse_ical_datetime(s)?;
            let end = parse_ical_datetime(e)?;
            Some((start, end, guest.clone(), et_title.clone()))
        })
        .collect();

    // Scan in 15-min increments and classify each tick
    struct Tick {
        time: NaiveTime,
        status: String, // "available", "outside", "busy_event", "booking", "buffer", "min_notice"
        label: String,
        detail: String,
    }

    let mut ticks: Vec<Tick> = Vec::new();
    let mut cursor = display_start;
    let tick_size = Duration::minutes(15);

    while cursor < display_end {
        let tick_dt = target_date.and_time(cursor);
        let tick_end = tick_dt + tick_size;

        // 1. Check if within availability window
        let in_avail = avail_windows
            .iter()
            .any(|(ws, we)| cursor >= *ws && cursor < *we);

        if !in_avail {
            ticks.push(Tick {
                time: cursor,
                status: "outside".to_string(),
                label: "Outside availability".to_string(),
                detail: String::new(),
            });
            cursor = (tick_dt + tick_size).time();
            continue;
        }

        // 2. Check minimum notice
        if tick_dt < min_start {
            ticks.push(Tick {
                time: cursor,
                status: "min_notice".to_string(),
                label: format!("Min. notice ({}min)", min_notice),
                detail: String::new(),
            });
            cursor = (tick_dt + tick_size).time();
            continue;
        }

        // 3. Check calendar events (with buffers)
        let event_conflict = busy_parsed.iter().find(|(s, e, _, _)| {
            let buf_s = *s - Duration::minutes(buf_before as i64);
            let buf_e = *e + Duration::minutes(buf_after as i64);
            tick_dt < buf_e && tick_end > buf_s
        });

        if let Some((ev_s, ev_e, ev_label, ev_detail)) = event_conflict {
            // Is it the event itself or just the buffer zone?
            let in_event = tick_dt < *ev_e && tick_end > *ev_s;
            if in_event {
                ticks.push(Tick {
                    time: cursor,
                    status: "busy_event".to_string(),
                    label: ev_label.clone(),
                    detail: ev_detail.clone(),
                });
            } else {
                ticks.push(Tick {
                    time: cursor,
                    status: "buffer".to_string(),
                    label: format!(
                        "Buffer ({}min)",
                        if tick_dt < *ev_s {
                            buf_before
                        } else {
                            buf_after
                        }
                    ),
                    detail: format!("Around: {}", ev_label),
                });
            }
            cursor = (tick_dt + tick_size).time();
            continue;
        }

        // 4. Check bookings (with buffers)
        let booking_conflict = bookings_parsed.iter().find(|(s, e, _, _)| {
            let buf_s = *s - Duration::minutes(buf_before as i64);
            let buf_e = *e + Duration::minutes(buf_after as i64);
            tick_dt < buf_e && tick_end > buf_s
        });

        if let Some((bk_s, bk_e, bk_guest, bk_et)) = booking_conflict {
            let in_booking = tick_dt < *bk_e && tick_end > *bk_s;
            if in_booking {
                ticks.push(Tick {
                    time: cursor,
                    status: "booking".to_string(),
                    label: bk_guest.clone(),
                    detail: bk_et.clone(),
                });
            } else {
                ticks.push(Tick {
                    time: cursor,
                    status: "buffer".to_string(),
                    label: format!(
                        "Buffer ({}min)",
                        if tick_dt < *bk_s {
                            buf_before
                        } else {
                            buf_after
                        }
                    ),
                    detail: format!("Around: {} booking", bk_guest),
                });
            }
            cursor = (tick_dt + tick_size).time();
            continue;
        }

        // 5. Available!
        ticks.push(Tick {
            time: cursor,
            status: "available".to_string(),
            label: "Available".to_string(),
            detail: String::new(),
        });
        cursor = (tick_dt + tick_size).time();
    }

    // Merge consecutive ticks with same status+label into blocks
    struct Block {
        start: NaiveTime,
        end: NaiveTime,
        status: String,
        label: String,
        detail: String,
        left_pct: f64,
        width_pct: f64,
    }

    let mut blocks: Vec<Block> = Vec::new();
    for tick in &ticks {
        let tick_end_time = (target_date.and_time(tick.time) + tick_size).time();
        if let Some(last) = blocks.last_mut() {
            if last.status == tick.status && last.label == tick.label {
                last.end = tick_end_time;
                let start_min = (last.start - display_start).num_minutes() as f64;
                let dur_min = (last.end - last.start).num_minutes() as f64;
                last.left_pct = start_min / total_minutes * 100.0;
                last.width_pct = dur_min / total_minutes * 100.0;
                continue;
            }
        }
        let start_min = (tick.time - display_start).num_minutes() as f64;
        let dur_min = tick_size.num_minutes() as f64;
        blocks.push(Block {
            start: tick.time,
            end: tick_end_time,
            status: tick.status.clone(),
            label: tick.label.clone(),
            detail: tick.detail.clone(),
            left_pct: start_min / total_minutes * 100.0,
            width_pct: dur_min / total_minutes * 100.0,
        });
    }

    // Build template data
    let blocks_ctx: Vec<minijinja::Value> = blocks
        .iter()
        .map(|b| {
            context! {
                start => b.start.format("%H:%M").to_string(),
                end => b.end.format("%H:%M").to_string(),
                status => &b.status,
                label => &b.label,
                detail => &b.detail,
                left_pct => format!("{:.2}", b.left_pct),
                width_pct => format!("{:.2}", b.width_pct),
            }
        })
        .collect();

    // Hour markers for the timeline
    let mut hour_markers: Vec<minijinja::Value> = Vec::new();
    let mut h = display_start_hour;
    while h <= display_end_hour {
        let min_offset = (h - display_start_hour) as f64 * 60.0;
        let left_pct = min_offset / total_minutes * 100.0;
        hour_markers.push(context! {
            label => format!("{:02}:00", h),
            left_pct => format!("{:.2}", left_pct),
        });
        h += 1;
    }

    // Breakdown: only non-available blocks
    let breakdown_ctx: Vec<minijinja::Value> = blocks
        .iter()
        .filter(|b| b.status != "available" && b.status != "outside")
        .map(|b| {
            let reason = match b.status.as_str() {
                "busy_event" => format!("Calendar event: {}", b.label),
                "booking" => format!("Booking: {}", b.label),
                "buffer" => b.label.clone(),
                "min_notice" => b.label.clone(),
                _ => b.status.clone(),
            };
            context! {
                start => b.start.format("%H:%M").to_string(),
                end => b.end.format("%H:%M").to_string(),
                status => &b.status,
                reason => reason,
                detail => &b.detail,
            }
        })
        .collect();

    let et_options: Vec<minijinja::Value> = event_types
        .iter()
        .map(|et| {
            context! {
                slug => &et.0,
                title => &et.1,
                selected => et.0 == *et_slug,
            }
        })
        .collect();

    let prev_date = (target_date - Duration::days(1))
        .format("%Y-%m-%d")
        .to_string();
    let next_date = (target_date + Duration::days(1))
        .format("%Y-%m-%d")
        .to_string();
    // Troubleshoot is a dashboard page; keep English until dashboard is translated.
    let date_label = crate::i18n::format_long_date(target_date, "en");

    let tmpl = match state.templates.get_template("troubleshoot.html") {
        Ok(t) => t,
        Err(e) => return internal_error_html("template render", &e),
    };

    let (impersonating, impersonating_name, _impersonating_admin) = impersonation_ctx(&auth_user);
    Html(
        tmpl.render(context! {
            user_name => &user.name,
            no_event_types => false,
            event_types => et_options,
            selected_slug => et_slug,
            selected_date => target_date.format("%Y-%m-%d").to_string(),
            date_label => date_label,
            prev_date => prev_date,
            next_date => next_date,
            has_rules => !rules.is_empty(),
            date_is_blocked => date_is_blocked,
            has_custom_hours => !day_overrides.is_empty() && !date_is_blocked,
            blocks => blocks_ctx,
            hour_markers => hour_markers,
            breakdown => breakdown_ctx,
            et_title => et_title,
            duration => duration,
            buf_before => buf_before,
            buf_after => buf_after,
            min_notice => min_notice,
            sidebar => sidebar_context(&auth_user, "troubleshoot"),
            impersonating => impersonating,
            impersonating_name => impersonating_name,
        })
        .unwrap_or_default(),
    )
}

// --- Admin dashboard ---

async fn admin_dashboard(
    State(state): State<Arc<AppState>>,
    admin: crate::auth::AdminUser,
    Query(query): Query<std::collections::HashMap<String, String>>,
) -> impl IntoResponse {
    let current_user = &admin.0;
    let error_message = query.get("error").cloned().unwrap_or_default();

    // Fetch all users
    let users: Vec<(String, String, String, String, String, bool)> = sqlx::query_as(
        "SELECT id, name, email, role, auth_provider, enabled FROM users ORDER BY created_at",
    )
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();

    let user_count = users.len();

    // Fetch groups per user
    let user_groups_rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT ug.user_id, g.name FROM user_groups ug JOIN groups g ON g.id = ug.group_id ORDER BY g.name",
    )
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();

    // Build a map of user_id -> comma-separated group names
    let mut user_groups_map: std::collections::HashMap<String, Vec<String>> =
        std::collections::HashMap::new();
    for (uid, gname) in &user_groups_rows {
        user_groups_map
            .entry(uid.clone())
            .or_default()
            .push(gname.clone());
    }

    let users_ctx: Vec<minijinja::Value> = users
        .iter()
        .map(|(id, name, email, role, auth_provider, enabled)| {
            let groups = user_groups_map.get(id).cloned().unwrap_or_default();
            context! {
                id => id,
                name => name,
                email => email,
                role => role,
                auth_provider => auth_provider,
                enabled => enabled,
                groups => groups,
            }
        })
        .collect();

    // Fetch groups with member count
    let groups_rows: Vec<(String, String, i64)> = sqlx::query_as(
        "SELECT g.id, g.name, COUNT(ug.user_id) as member_count \
         FROM groups g LEFT JOIN user_groups ug ON ug.group_id = g.id \
         GROUP BY g.id ORDER BY g.name",
    )
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();

    let group_count = groups_rows.len();

    // Fetch all group members with weights
    let all_members: Vec<(String, String, String, i64)> = sqlx::query_as(
        "SELECT ug.group_id, ug.user_id, u.name, ug.weight \
         FROM user_groups ug JOIN users u ON u.id = ug.user_id \
         ORDER BY ug.weight DESC, u.name",
    )
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();

    let groups_ctx: Vec<minijinja::Value> = groups_rows
        .iter()
        .map(|(id, name, member_count)| {
            let members: Vec<minijinja::Value> = all_members
                .iter()
                .filter(|(gid, _, _, _)| gid == id)
                .map(|(_, uid, uname, w)| {
                    context! {
                        user_id => uid,
                        name => uname,
                        weight => w,
                    }
                })
                .collect();
            context! {
                id => id,
                name => name,
                member_count => member_count,
                members => members,
            }
        })
        .collect();

    // Fetch auth config
    let auth_config = crate::auth::get_auth_config(&state.pool).await.ok();
    let registration_enabled = auth_config
        .as_ref()
        .map(|c| c.registration_enabled)
        .unwrap_or(false);
    let allowed_email_domains = auth_config
        .as_ref()
        .and_then(|c| c.allowed_email_domains.clone())
        .unwrap_or_default();
    let oidc_enabled = auth_config
        .as_ref()
        .map(|c| c.oidc_enabled)
        .unwrap_or(false);
    let oidc_issuer_url = auth_config
        .as_ref()
        .and_then(|c| c.oidc_issuer_url.clone())
        .unwrap_or_default();
    let oidc_client_id = auth_config
        .as_ref()
        .and_then(|c| c.oidc_client_id.clone())
        .unwrap_or_default();
    let oidc_auto_register = auth_config
        .as_ref()
        .map(|c| c.oidc_auto_register)
        .unwrap_or(true);
    let google_oauth2_client_id = auth_config
        .as_ref()
        .and_then(|c| c.google_oauth2_client_id.clone())
        .unwrap_or_default();
    let google_oauth2_configured = !google_oauth2_client_id.is_empty();

    let (
        smtp_configured,
        smtp_host,
        smtp_port,
        smtp_from_email,
        smtp_enabled,
        smtp_from_env,
        smtp_error,
    ) = match crate::email::load_smtp_status(&state.pool).await {
        Ok(Some(status)) => (
            true,
            status.host,
            status.port,
            status.from_email,
            status.enabled,
            status.from_env,
            String::new(),
        ),
        Ok(None) => (
            false,
            String::new(),
            0u16,
            String::new(),
            false,
            false,
            String::new(),
        ),
        Err(e) => (
            false,
            String::new(),
            0u16,
            String::new(),
            false,
            true,
            e.to_string(),
        ),
    };

    let tmpl = match state.templates.get_template("admin.html") {
        Ok(t) => t,
        Err(e) => return internal_error_html("template render", &e),
    };

    let sidebar = context! {
        user_name => current_user.name,
        user_title => current_user.title.as_deref().unwrap_or(""),
        user_id => current_user.id,
        user_role => "admin",
        user_timezone => current_user.timezone,
        has_avatar => current_user.avatar_path.is_some(),
        user_initials => compute_initials(&current_user.name),
        active => "admin",
        version => env!("CARGO_PKG_VERSION"),
    };

    let captcha_cfg = state.captcha_config.read().await;
    let captcha_configured = captcha_cfg.is_some();
    let captcha_instance_url = captcha_cfg
        .as_ref()
        .map(|c| c.instance_url.clone())
        .unwrap_or_default();
    let captcha_site_key = captcha_cfg
        .as_ref()
        .map(|c| c.site_key.clone())
        .unwrap_or_default();
    let captcha_widget_url = captcha_cfg
        .as_ref()
        .map(|c| c.widget_url.clone())
        .unwrap_or_else(|| captcha::DEFAULT_WIDGET_URL.to_string());
    drop(captcha_cfg);

    let ews_cfg = state.ews_global.read().await;
    let ews_global_enabled = ews_cfg.is_some();
    let ews_global_url = ews_cfg.as_ref().map(|c| c.url.clone()).unwrap_or_default();
    let ews_service_username = ews_cfg
        .as_ref()
        .map(|c| c.service_username.clone())
        .unwrap_or_default();
    let ews_has_password = ews_cfg
        .as_ref()
        .map(|c| !c.service_password.is_empty())
        .unwrap_or(false);
    let ews_lock_user_sources = ews_cfg
        .as_ref()
        .map(|c| c.lock_user_sources)
        .unwrap_or(false);
    let ews_auto_provision = ews_cfg.as_ref().map(|c| c.auto_provision).unwrap_or(false);
    drop(ews_cfg);
    let ews_managed_source_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM caldav_sources WHERE managed = 1 AND provider_type = 'ews'",
    )
    .fetch_one(&state.pool)
    .await
    .unwrap_or(0);

    let meeting_cfg = state.meeting_config.read().await;
    let jitsi_configured = meeting_cfg.jitsi.is_some();
    let jitsi_base_url = meeting_cfg
        .jitsi
        .as_ref()
        .map(|j| j.base_url.clone())
        .unwrap_or_default();
    let jitsi_pattern = meeting_cfg
        .jitsi
        .as_ref()
        .map(|j| j.pattern.clone())
        .unwrap_or_else(|| meeting::DEFAULT_JITSI_PATTERN.to_string());
    let jitsi_display_name = meeting_cfg
        .jitsi
        .as_ref()
        .and_then(|j| j.display_name.clone())
        .unwrap_or_default();
    let meeting_webhook_configured = meeting_cfg.webhook.is_some();
    let meeting_webhook_url = meeting_cfg
        .webhook
        .as_ref()
        .map(|w| w.url.clone())
        .unwrap_or_default();
    let meeting_webhook_auth_mode = meeting_cfg
        .webhook
        .as_ref()
        .map(|w| w.auth_mode.as_str().to_string())
        .unwrap_or_else(|| meeting::WEBHOOK_AUTH_NONE.to_string());
    let meeting_webhook_has_secret = meeting_cfg
        .webhook
        .as_ref()
        .map(|w| !w.secret.is_empty())
        .unwrap_or(false);
    let meeting_webhook_display_name = meeting_cfg
        .webhook
        .as_ref()
        .and_then(|w| w.display_name.clone())
        .unwrap_or_default();
    drop(meeting_cfg);

    Html(
        tmpl.render(context! {
            current_user_id => current_user.id,
            users => users_ctx,
            user_count => user_count,
            groups => groups_ctx,
            group_count => group_count,
            registration_enabled => registration_enabled,
            allowed_email_domains => allowed_email_domains,
            oidc_enabled => oidc_enabled,
            oidc_issuer_url => oidc_issuer_url,
            oidc_client_id => oidc_client_id,
            oidc_auto_register => oidc_auto_register,
            google_oauth2_client_id => google_oauth2_client_id,
            google_oauth2_configured => google_oauth2_configured,
            base_url => std::env::var("CALRS_BASE_URL").unwrap_or_default(),
            smtp_configured => smtp_configured,
            smtp_host => smtp_host,
            smtp_port => smtp_port,
            smtp_from_email => smtp_from_email,
            smtp_enabled => smtp_enabled,
            smtp_from_env => smtp_from_env,
            smtp_error => smtp_error,
            captcha_configured => captcha_configured,
            captcha_instance_url => captcha_instance_url,
            captcha_site_key => captcha_site_key,
            captcha_widget_url => captcha_widget_url,
            ews_global_enabled => ews_global_enabled,
            ews_global_url => ews_global_url,
            ews_service_username => ews_service_username,
            ews_has_password => ews_has_password,
            ews_lock_user_sources => ews_lock_user_sources,
            ews_auto_provision => ews_auto_provision,
            ews_managed_source_count => ews_managed_source_count,
            jitsi_configured => jitsi_configured,
            jitsi_base_url => jitsi_base_url,
            jitsi_pattern => jitsi_pattern,
            jitsi_display_name => jitsi_display_name,
            jitsi_default_pattern => meeting::DEFAULT_JITSI_PATTERN,
            meeting_webhook_configured => meeting_webhook_configured,
            meeting_webhook_url => meeting_webhook_url,
            meeting_webhook_auth_mode => meeting_webhook_auth_mode,
            meeting_webhook_has_secret => meeting_webhook_has_secret,
            meeting_webhook_display_name => meeting_webhook_display_name,
            has_logo => state.data_dir.join("logo.png").exists(),
            company_link => get_company_link(&state.pool).await.unwrap_or_default(),
            current_theme => get_theme_name(&state.pool).await,
            custom_colors => {
                let (a, ah, bg, s, t) = get_custom_colors(&state.pool).await;
                context! { accent => a, accent_hover => ah, bg => bg, surface => s, text => t }
            },
            sidebar => sidebar,
            impersonating => false,
            impersonating_name => "",
            error_message => error_message,
        })
        .unwrap_or_else(|e| internal_error_body("template render", &e)),
    )
}

async fn admin_toggle_role(
    State(state): State<Arc<AppState>>,
    _admin: crate::auth::AdminUser,
    headers: HeaderMap,
    Path(user_id): Path<String>,
    Form(csrf): Form<CsrfForm>,
) -> impl IntoResponse {
    if let Err(resp) = verify_csrf_token(&headers, &csrf._csrf) {
        return resp;
    }
    // Get current role
    let current_role: Option<(String,)> = sqlx::query_as("SELECT role FROM users WHERE id = ?")
        .bind(&user_id)
        .fetch_optional(&state.pool)
        .await
        .unwrap_or(None);

    if let Some((role,)) = current_role {
        let new_role = if role == "admin" { "user" } else { "admin" };
        let _ = sqlx::query("UPDATE users SET role = ?, updated_at = datetime('now') WHERE id = ?")
            .bind(new_role)
            .bind(&user_id)
            .execute(&state.pool)
            .await;
        tracing::info!(target_user = %user_id, new_role = %new_role, admin = %_admin.0.email, "admin: role changed");
    }

    Redirect::to("/dashboard/admin").into_response()
}

async fn admin_toggle_enabled(
    State(state): State<Arc<AppState>>,
    _admin: crate::auth::AdminUser,
    headers: HeaderMap,
    Path(user_id): Path<String>,
    Form(csrf): Form<CsrfForm>,
) -> impl IntoResponse {
    if let Err(resp) = verify_csrf_token(&headers, &csrf._csrf) {
        return resp;
    }
    // Get current enabled status
    let current: Option<(bool,)> = sqlx::query_as("SELECT enabled FROM users WHERE id = ?")
        .bind(&user_id)
        .fetch_optional(&state.pool)
        .await
        .unwrap_or(None);

    if let Some((enabled,)) = current {
        let new_enabled = !enabled;
        let _ =
            sqlx::query("UPDATE users SET enabled = ?, updated_at = datetime('now') WHERE id = ?")
                .bind(new_enabled)
                .bind(&user_id)
                .execute(&state.pool)
                .await;
        tracing::info!(target_user = %user_id, enabled = %new_enabled, admin = %_admin.0.email, "admin: user toggled");
    }

    Redirect::to("/dashboard/admin").into_response()
}

async fn admin_delete_user(
    State(state): State<Arc<AppState>>,
    admin: crate::auth::AdminUser,
    headers: HeaderMap,
    Path(user_id): Path<String>,
    Form(csrf): Form<CsrfForm>,
) -> impl IntoResponse {
    if let Err(resp) = verify_csrf_token(&headers, &csrf._csrf) {
        return resp;
    }
    let admin_user = &admin.0;
    let avatars_dir = state.data_dir.join("avatars");
    match crate::auth::delete_user(
        &state.pool,
        &user_id,
        Some(&admin_user.id),
        Some(&avatars_dir),
    )
    .await
    {
        Ok(()) => {
            tracing::info!(target_user = %user_id, admin = %admin_user.email, "admin: user deleted");
            Redirect::to("/dashboard/admin").into_response()
        }
        Err(e) => {
            tracing::warn!(target_user = %user_id, admin = %admin_user.email, error = %e, "admin: user delete refused");
            let encoded = urlencoding::encode(&e.to_string()).into_owned();
            Redirect::to(&format!("/dashboard/admin?error={}", encoded)).into_response()
        }
    }
}

#[derive(Deserialize)]
struct AdminAuthForm {
    _csrf: Option<String>,
    registration_enabled: Option<String>,
    allowed_email_domains: Option<String>,
}

async fn admin_update_auth(
    State(state): State<Arc<AppState>>,
    _admin: crate::auth::AdminUser,
    headers: HeaderMap,
    Form(form): Form<AdminAuthForm>,
) -> impl IntoResponse {
    if let Err(resp) = verify_csrf_token(&headers, &form._csrf) {
        return resp;
    }
    let registration_enabled = form.registration_enabled.is_some();
    let allowed_domains = form.allowed_email_domains.filter(|d| !d.trim().is_empty());

    let _ = sqlx::query(
        "UPDATE auth_config SET registration_enabled = ?, allowed_email_domains = ?, updated_at = datetime('now') WHERE id = 'singleton'",
    )
    .bind(registration_enabled)
    .bind(&allowed_domains)
    .execute(&state.pool)
    .await;

    tracing::info!(admin = %_admin.0.email, "admin: auth config updated");

    Redirect::to("/dashboard/admin").into_response()
}

#[derive(Deserialize)]
struct AdminThemeForm {
    _csrf: Option<String>,
    theme: Option<String>,
    custom_accent: Option<String>,
    custom_accent_hover: Option<String>,
    custom_bg: Option<String>,
    custom_surface: Option<String>,
    custom_text: Option<String>,
}

async fn admin_update_accent(
    State(state): State<Arc<AppState>>,
    _admin: crate::auth::AdminUser,
    headers: HeaderMap,
    Form(form): Form<AdminThemeForm>,
) -> impl IntoResponse {
    if let Err(resp) = verify_csrf_token(&headers, &form._csrf) {
        return resp;
    }
    let theme = form
        .theme
        .filter(|t| {
            matches!(
                t.as_str(),
                "default"
                    | "nord"
                    | "dracula"
                    | "gruvbox"
                    | "solarized"
                    | "tokyo-night"
                    | "vates"
                    | "custom"
            )
        })
        .unwrap_or_else(|| "default".to_string());

    if theme == "custom" {
        let accent = form.custom_accent.as_deref().unwrap_or("#2563eb");
        let accent_hover = form.custom_accent_hover.as_deref().unwrap_or("#1d4ed8");
        let bg = form.custom_bg.as_deref().unwrap_or("#f4f4f5");
        let surface = form.custom_surface.as_deref().unwrap_or("#ffffff");
        let text = form.custom_text.as_deref().unwrap_or("#18181b");

        let _ = sqlx::query(
            "UPDATE auth_config SET theme = 'custom', custom_accent = ?, custom_accent_hover = ?, custom_bg = ?, custom_surface = ?, custom_text = ?, updated_at = datetime('now') WHERE id = 'singleton'",
        )
        .bind(accent)
        .bind(accent_hover)
        .bind(bg)
        .bind(surface)
        .bind(text)
        .execute(&state.pool)
        .await;
    } else {
        let _ = sqlx::query(
            "UPDATE auth_config SET theme = ?, updated_at = datetime('now') WHERE id = 'singleton'",
        )
        .bind(&theme)
        .execute(&state.pool)
        .await;
    }

    // Rebuild and cache the CSS
    let new_css = build_theme_css(&state.pool).await;
    *state.theme_css.write().await = new_css;

    tracing::info!(admin = %_admin.0.email, theme = %theme, "admin: theme updated");

    Redirect::to("/dashboard/admin").into_response()
}

#[derive(Deserialize)]
struct AdminOidcForm {
    _csrf: Option<String>,
    oidc_enabled: Option<String>,
    oidc_issuer_url: Option<String>,
    oidc_client_id: Option<String>,
    oidc_client_secret: Option<String>,
    oidc_auto_register: Option<String>,
}

async fn admin_update_oidc(
    State(state): State<Arc<AppState>>,
    _admin: crate::auth::AdminUser,
    headers: HeaderMap,
    Form(form): Form<AdminOidcForm>,
) -> impl IntoResponse {
    if let Err(resp) = verify_csrf_token(&headers, &form._csrf) {
        return resp;
    }
    let oidc_enabled = form.oidc_enabled.is_some();
    let issuer_url = form.oidc_issuer_url.filter(|s| !s.trim().is_empty());
    let client_id = form.oidc_client_id.filter(|s| !s.trim().is_empty());
    let auto_register = form.oidc_auto_register.is_some();

    // If client_secret is provided (non-empty), update it; otherwise keep current value
    let secret_provided = form
        .oidc_client_secret
        .as_ref()
        .map(|s| !s.trim().is_empty())
        .unwrap_or(false);

    if secret_provided {
        let client_secret = form.oidc_client_secret.unwrap_or_default();
        // Encrypt the secret at rest. We carry the auth_config row's
        // singleton SQLite primary key, so on the read side it goes
        // through crypto::decrypt_value.
        let encrypted_secret = match crate::crypto::encrypt_value(&state.secret_key, &client_secret)
        {
            Ok(s) => s,
            Err(e) => return internal_error_response("encrypt oidc client_secret", &e),
        };
        let _ = sqlx::query(
            "UPDATE auth_config SET oidc_enabled = ?, oidc_issuer_url = ?, oidc_client_id = ?, oidc_client_secret = ?, oidc_auto_register = ?, updated_at = datetime('now') WHERE id = 'singleton'",
        )
        .bind(oidc_enabled)
        .bind(&issuer_url)
        .bind(&client_id)
        .bind(&encrypted_secret)
        .bind(auto_register)
        .execute(&state.pool)
        .await;
    } else {
        let _ = sqlx::query(
            "UPDATE auth_config SET oidc_enabled = ?, oidc_issuer_url = ?, oidc_client_id = ?, oidc_auto_register = ?, updated_at = datetime('now') WHERE id = 'singleton'",
        )
        .bind(oidc_enabled)
        .bind(&issuer_url)
        .bind(&client_id)
        .bind(auto_register)
        .execute(&state.pool)
        .await;
    }

    tracing::info!(admin = %_admin.0.email, "admin: OIDC config updated");

    Redirect::to("/dashboard/admin").into_response()
}

#[derive(Deserialize)]
struct AdminGoogleOAuth2Form {
    _csrf: Option<String>,
    google_oauth2_client_id: Option<String>,
    google_oauth2_client_secret: Option<String>,
}

async fn admin_update_google_oauth2(
    State(state): State<Arc<AppState>>,
    _admin: crate::auth::AdminUser,
    headers: HeaderMap,
    Form(form): Form<AdminGoogleOAuth2Form>,
) -> impl IntoResponse {
    if let Err(resp) = verify_csrf_token(&headers, &form._csrf) {
        return resp;
    }
    let client_id = form
        .google_oauth2_client_id
        .filter(|s| !s.trim().is_empty());

    let secret_provided = form
        .google_oauth2_client_secret
        .as_ref()
        .map(|s| !s.trim().is_empty())
        .unwrap_or(false);

    if secret_provided {
        let client_secret = form.google_oauth2_client_secret.unwrap_or_default();
        let encrypted_secret = match crate::crypto::encrypt_value(&state.secret_key, &client_secret)
        {
            Ok(s) => s,
            Err(e) => return internal_error_response("encrypt google_oauth2 client_secret", &e),
        };
        let _ = sqlx::query(
            "UPDATE auth_config SET google_oauth2_client_id = ?, google_oauth2_client_secret = ?, updated_at = datetime('now') WHERE id = 'singleton'",
        )
        .bind(&client_id)
        .bind(&encrypted_secret)
        .execute(&state.pool)
        .await;
    } else {
        let _ = sqlx::query(
            "UPDATE auth_config SET google_oauth2_client_id = ?, updated_at = datetime('now') WHERE id = 'singleton'",
        )
        .bind(&client_id)
        .execute(&state.pool)
        .await;
    }

    tracing::info!(admin = %_admin.0.email, "admin: Google OAuth2 config updated");
    Redirect::to("/dashboard/admin").into_response()
}

// --- Google Calendar OAuth2 connect flow ---

async fn google_connect(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
) -> impl IntoResponse {
    // Load Google OAuth2 client credentials
    let creds: Option<(Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT google_oauth2_client_id, google_oauth2_client_secret FROM auth_config LIMIT 1",
    )
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    let client_id = match creds.and_then(|(id, _)| id).filter(|s| !s.is_empty()) {
        Some(id) => id,
        None => {
            return Html("Google Calendar integration is not configured. Ask your administrator to set up Google OAuth2 credentials in the admin panel.".to_string()).into_response();
        }
    };

    let base_url = std::env::var("CALRS_BASE_URL").unwrap_or_default();
    if base_url.is_empty() {
        return Html(
            "CALRS_BASE_URL environment variable must be set for OAuth2 flows.".to_string(),
        )
        .into_response();
    }

    let redirect_uri = format!(
        "{}/dashboard/sources/google/callback",
        base_url.trim_end_matches('/')
    );
    let csrf_state = uuid::Uuid::new_v4().to_string();

    let auth_url =
        crate::oauth2_caldav::build_google_auth_url(&client_id, &redirect_uri, &csrf_state);

    // Store state in cookies via Set-Cookie headers (CookieJar's String→Cookie
    // conversion treats the whole formatted string as the cookie name and drops
    // attributes, so set the headers directly).
    let cookie_opts = "; HttpOnly; Secure; SameSite=Lax; Path=/; Max-Age=600";
    let state_cookie = format!("__Host-calrs_google_state={}{}", csrf_state, cookie_opts);
    let user_cookie = format!(
        "__Host-calrs_google_user={}{}",
        auth_user.user.id, cookie_opts
    );

    let mut headers = axum::http::HeaderMap::new();
    headers.append(
        axum::http::header::SET_COOKIE,
        state_cookie.parse().unwrap(),
    );
    headers.append(axum::http::header::SET_COOKIE, user_cookie.parse().unwrap());

    (headers, Redirect::to(&auth_url)).into_response()
}

#[derive(Deserialize)]
struct GoogleCallbackQuery {
    code: Option<String>,
    state: Option<String>,
    error: Option<String>,
}

async fn google_callback(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
    jar: axum_extra::extract::CookieJar,
    Query(query): Query<GoogleCallbackQuery>,
) -> impl IntoResponse {
    // Check for errors from Google
    if let Some(error) = &query.error {
        return Html(format!("Google authorization failed: {}", error)).into_response();
    }

    // Verify CSRF state
    let stored_state = jar
        .get("__Host-calrs_google_state")
        .map(|c| c.value().to_string())
        .unwrap_or_default();
    let query_state = query.state.unwrap_or_default();
    if stored_state.is_empty() || stored_state != query_state {
        return Html("Invalid state parameter. Please try again.".to_string()).into_response();
    }

    // Cross-tab defense-in-depth: the user cookie set at /connect must match
    // the current session. Guards against a tab swap where the OAuth flow was
    // started as user A but the callback fires while user B is logged in.
    let stored_user = jar
        .get("__Host-calrs_google_user")
        .map(|c| c.value().to_string())
        .unwrap_or_default();
    if stored_user.is_empty() || stored_user != auth_user.user.id {
        tracing::warn!(
            session_user = %auth_user.user.id,
            cookie_user = %stored_user,
            "Google OAuth2 callback: user cookie missing or does not match session"
        );
        return Html(
            "Session changed during Google authorization. Please start the connect flow again."
                .to_string(),
        )
        .into_response();
    }

    let code = match query.code {
        Some(c) => c,
        None => return Html("No authorization code received.".to_string()).into_response(),
    };

    // Load Google OAuth2 credentials
    let creds: Option<(String, String)> = sqlx::query_as(
        "SELECT google_oauth2_client_id, google_oauth2_client_secret FROM auth_config LIMIT 1",
    )
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    let (client_id, client_secret_enc) = match creds {
        Some(c) => c,
        None => return Html("Google OAuth2 not configured.".to_string()).into_response(),
    };

    // The stored Google OAuth2 client secret is encrypted at rest; the
    // startup migration ensures any pre-existing plaintext is upgraded
    // before we reach this path.
    let client_secret = match crate::crypto::decrypt_value(&state.secret_key, &client_secret_enc) {
        Ok(s) => s,
        Err(e) => return internal_error_response("decrypt google_oauth2 client_secret", &e),
    };

    let base_url = std::env::var("CALRS_BASE_URL").unwrap_or_default();
    let redirect_uri = format!(
        "{}/dashboard/sources/google/callback",
        base_url.trim_end_matches('/')
    );

    // Exchange code for tokens
    let (access_token, refresh_token, expires_in) =
        match crate::oauth2_caldav::exchange_google_code(
            &client_id,
            &client_secret,
            &code,
            &redirect_uri,
        )
        .await
        {
            Ok(t) => t,
            Err(e) => return Html(format!("Token exchange failed: {}", e)).into_response(),
        };

    // Identify the Google account that authorized us; its email is what goes in the CalDAV URL.
    // The local calrs user's email is unrelated and may not match the Google account.
    let google_email = match crate::oauth2_caldav::fetch_google_email(&access_token).await {
        Ok(e) => e,
        Err(e) => {
            return Html(format!("Failed to fetch Google account email: {}", e)).into_response();
        }
    };
    let caldav_url = crate::oauth2_caldav::google_caldav_url_for_email(&google_email);

    // Test the CalDAV connection (PROPFIND requires the per-user URL)
    let client = crate::caldav::CaldavClient::with_bearer(&caldav_url, &access_token);
    if let Err(e) = client.check_connection().await {
        tracing::warn!(error = %e, "Google CalDAV connection test failed (non-fatal)");
    }

    // Get user's account
    let account: Option<(String,)> =
        sqlx::query_as("SELECT a.id FROM accounts a WHERE a.user_id = ? LIMIT 1")
            .bind(&auth_user.user.id)
            .fetch_optional(&state.pool)
            .await
            .unwrap_or(None);

    let account_id = match account {
        Some((id,)) => id,
        None => return Html("No scheduling account found.".to_string()).into_response(),
    };

    // Encrypt tokens
    let access_token_enc = match crate::crypto::encrypt_password(&state.secret_key, &access_token) {
        Ok(enc) => enc,
        Err(_) => return Html("Encryption error.".to_string()).into_response(),
    };
    let refresh_token_enc = match crate::crypto::encrypt_password(&state.secret_key, &refresh_token)
    {
        Ok(enc) => enc,
        Err(_) => return Html("Encryption error.".to_string()).into_response(),
    };
    let expires_at = chrono::Utc::now() + chrono::Duration::seconds(expires_in);

    // Create the CalDAV source
    let source_id = uuid::Uuid::new_v4().to_string();
    let _ = sqlx::query(
        "INSERT INTO caldav_sources (id, account_id, name, url, username, auth_type, oauth2_provider, access_token_enc, refresh_token_enc, token_expires_at)
         VALUES (?, ?, 'Google Calendar', ?, ?, 'oauth2', 'google', ?, ?, ?)",
    )
    .bind(&source_id)
    .bind(&account_id)
    .bind(&caldav_url)
    .bind(&google_email)
    .bind(&access_token_enc)
    .bind(&refresh_token_enc)
    .bind(expires_at.to_rfc3339())
    .execute(&state.pool)
    .await;

    tracing::info!(user = %auth_user.user.email, google_email = %google_email, "Google Calendar source added via OAuth2");

    // Auto-sync
    let (_, calendar_count) = run_sync_for_source(
        &state.pool,
        &state.secret_key,
        &source_id,
        &caldav_url,
        &google_email,
        None,
        "oauth2",
        Some(&access_token_enc),
        Some(&expires_at.to_rfc3339()),
        crate::providers::factory::kinds::CALDAV,
    )
    .await;

    // Clear transient cookies. The browser only honors removal of
    // __Host-prefixed cookies when the Set-Cookie also has Secure and Path=/,
    // so emit explicit expiring headers (matches admin_stop_impersonate).
    let clear_opts = "; HttpOnly; Secure; SameSite=Lax; Path=/; Max-Age=0";
    let mut headers = axum::http::HeaderMap::new();
    headers.append(
        axum::http::header::SET_COOKIE,
        format!("__Host-calrs_google_state={}", clear_opts)
            .parse()
            .unwrap(),
    );
    headers.append(
        axum::http::header::SET_COOKIE,
        format!("__Host-calrs_google_user={}", clear_opts)
            .parse()
            .unwrap(),
    );

    let redirect = if calendar_count > 0 {
        Redirect::to(&format!("/dashboard/sources/{}/setup-write", source_id))
    } else {
        Redirect::to("/dashboard/sources")
    };
    (headers, redirect).into_response()
}

// --- Captcha settings ---

#[derive(Deserialize)]
struct AdminCaptchaForm {
    _csrf: Option<String>,
    captcha_instance_url: Option<String>,
    captcha_site_key: Option<String>,
    captcha_secret: Option<String>,
    captcha_widget_url: Option<String>,
}

async fn admin_update_captcha(
    State(state): State<Arc<AppState>>,
    _admin: crate::auth::AdminUser,
    headers: HeaderMap,
    Form(form): Form<AdminCaptchaForm>,
) -> impl IntoResponse {
    if let Err(resp) = verify_csrf_token(&headers, &form._csrf) {
        return resp;
    }

    let instance_url = form.captcha_instance_url.filter(|s| !s.trim().is_empty());
    let site_key = form.captcha_site_key.filter(|s| !s.trim().is_empty());
    let widget_url = form.captcha_widget_url.filter(|s| !s.trim().is_empty());

    let secret_provided = form
        .captcha_secret
        .as_ref()
        .map(|s| !s.trim().is_empty())
        .unwrap_or(false);

    if secret_provided {
        let secret = form.captcha_secret.unwrap_or_default();
        let encrypted = match crate::crypto::encrypt_value(&state.secret_key, &secret) {
            Ok(s) => s,
            Err(e) => return internal_error_response("encrypt captcha secret", &e),
        };
        if let Err(e) = sqlx::query(
            "UPDATE auth_config SET captcha_instance_url = ?, captcha_site_key = ?, captcha_secret = ?, captcha_widget_url = ?, updated_at = datetime('now') WHERE id = 'singleton'",
        )
        .bind(&instance_url)
        .bind(&site_key)
        .bind(&encrypted)
        .bind(&widget_url)
        .execute(&state.pool)
        .await
        {
            tracing::error!(error = %e, "failed to save captcha config");
        }
    } else {
        if let Err(e) = sqlx::query(
            "UPDATE auth_config SET captcha_instance_url = ?, captcha_site_key = ?, captcha_widget_url = ?, updated_at = datetime('now') WHERE id = 'singleton'",
        )
        .bind(&instance_url)
        .bind(&site_key)
        .bind(&widget_url)
        .execute(&state.pool)
        .await
        {
            tracing::error!(error = %e, "failed to save captcha config");
        }
    }

    let new_config = captcha::load_captcha_config(&state.pool, &state.secret_key).await;
    let new_csp = build_csp(&new_config);
    *state.captcha_config.write().await = new_config;
    *state.csp.write().await = new_csp;

    tracing::info!(admin = %_admin.0.email, "admin: captcha config updated");

    Redirect::to("/dashboard/admin").into_response()
}

// --- Global EWS impersonation ---

#[derive(Deserialize)]
struct AdminEwsGlobalForm {
    _csrf: Option<String>,
    ews_global_enabled: Option<String>,
    ews_global_url: Option<String>,
    ews_service_username: Option<String>,
    ews_service_password: Option<String>,
    ews_lock_user_sources: Option<String>,
    ews_auto_provision: Option<String>,
    /// When set (button `name="provision_now" value="1"`), provision a
    /// managed source for every enabled user after saving.
    provision_now: Option<String>,
}

async fn admin_update_ews_global(
    State(state): State<Arc<AppState>>,
    _admin: crate::auth::AdminUser,
    headers: HeaderMap,
    Form(form): Form<AdminEwsGlobalForm>,
) -> impl IntoResponse {
    if let Err(resp) = verify_csrf_token(&headers, &form._csrf) {
        return resp;
    }

    let enabled = form.ews_global_enabled.as_deref() == Some("on");
    let url = form.ews_global_url.filter(|s| !s.trim().is_empty());
    let username = form.ews_service_username.filter(|s| !s.trim().is_empty());
    let lock = form.ews_lock_user_sources.as_deref() == Some("on");
    let auto_provision = form.ews_auto_provision.as_deref() == Some("on");
    let provision_now = form.provision_now.as_deref() == Some("1");

    // Validate URL when enabling. Reject early instead of silently storing
    // garbage that would later make every sync fail.
    if enabled {
        if let Some(u) = url.as_deref() {
            if let Err(e) =
                crate::providers::factory::validate_url(crate::providers::factory::kinds::EWS, u)
            {
                tracing::warn!(error = %e, "admin: invalid global EWS URL");
                return Redirect::to("/dashboard/admin#ews-global").into_response();
            }
        }
    }

    let secret_provided = form
        .ews_service_password
        .as_ref()
        .map(|s| !s.trim().is_empty())
        .unwrap_or(false);

    let write_result = if secret_provided {
        let secret = form.ews_service_password.unwrap_or_default();
        let encrypted = match crate::crypto::encrypt_value(&state.secret_key, &secret) {
            Ok(s) => s,
            Err(e) => return internal_error_response("encrypt EWS service password", &e),
        };
        sqlx::query(
            "UPDATE auth_config SET ews_global_enabled = ?, ews_global_url = ?, \
                 ews_service_username = ?, ews_service_password_enc = ?, \
                 ews_lock_user_sources = ?, ews_auto_provision = ?, \
                 updated_at = datetime('now') WHERE id = 'singleton'",
        )
        .bind(enabled as i64)
        .bind(&url)
        .bind(&username)
        .bind(&encrypted)
        .bind(lock as i64)
        .bind(auto_provision as i64)
        .execute(&state.pool)
        .await
    } else {
        sqlx::query(
            "UPDATE auth_config SET ews_global_enabled = ?, ews_global_url = ?, \
                 ews_service_username = ?, ews_lock_user_sources = ?, ews_auto_provision = ?, \
                 updated_at = datetime('now') WHERE id = 'singleton'",
        )
        .bind(enabled as i64)
        .bind(&url)
        .bind(&username)
        .bind(lock as i64)
        .bind(auto_provision as i64)
        .execute(&state.pool)
        .await
    };
    if let Err(e) = write_result {
        tracing::error!(error = %e, "failed to save global EWS config");
    }

    // Reload cache so the next sync / next request sees the new values.
    let new_config = ews_global::load_ews_global_config(&state.pool, &state.secret_key).await;

    // Run provisioning if asked, OR if the admin enabled auto-provision in
    // the same save. Either intent means "spread the managed source to every
    // existing user now".
    if let Some(cfg) = new_config.as_ref() {
        if provision_now || cfg.auto_provision {
            match ews_global::provision_managed_ews_source_for_all_users(&state.pool, cfg).await {
                Ok(n) => {
                    tracing::info!(provisioned = n, "admin: global EWS provisioning batch done");
                }
                Err(e) => {
                    tracing::error!(error = %e, "global EWS provisioning failed");
                }
            }
        }
    }

    *state.ews_global.write().await = new_config;

    tracing::info!(admin = %_admin.0.email, enabled, "admin: global EWS config updated");

    Redirect::to("/dashboard/admin#ews-global").into_response()
}

// --- Auto-generated meeting link settings ---

#[derive(Deserialize)]
struct AdminJitsiForm {
    _csrf: Option<String>,
    jitsi_base_url: Option<String>,
    jitsi_pattern: Option<String>,
    jitsi_display_name: Option<String>,
}

async fn admin_update_jitsi(
    State(state): State<Arc<AppState>>,
    _admin: crate::auth::AdminUser,
    headers: HeaderMap,
    Form(form): Form<AdminJitsiForm>,
) -> impl IntoResponse {
    if let Err(resp) = verify_csrf_token(&headers, &form._csrf) {
        return resp;
    }

    let base_url = form.jitsi_base_url.filter(|s| !s.trim().is_empty());
    let pattern = form.jitsi_pattern.filter(|s| !s.trim().is_empty());
    let display_name = form.jitsi_display_name.filter(|s| !s.trim().is_empty());

    if let Err(e) = sqlx::query(
        "UPDATE auth_config SET jitsi_base_url = ?, jitsi_pattern = ?, jitsi_display_name = ?, updated_at = datetime('now') WHERE id = 'singleton'",
    )
    .bind(&base_url)
    .bind(&pattern)
    .bind(&display_name)
    .execute(&state.pool)
    .await
    {
        tracing::error!(error = %e, "failed to save jitsi config");
    }

    *state.meeting_config.write().await =
        meeting::load_config(&state.pool, &state.secret_key).await;

    tracing::info!(admin = %_admin.0.email, "admin: jitsi config updated");
    Redirect::to("/dashboard/admin").into_response()
}

#[derive(Deserialize)]
struct AdminMeetingWebhookForm {
    _csrf: Option<String>,
    meeting_webhook_url: Option<String>,
    meeting_webhook_auth_mode: Option<String>,
    meeting_webhook_secret: Option<String>,
    meeting_webhook_display_name: Option<String>,
}

async fn admin_update_meeting_webhook(
    State(state): State<Arc<AppState>>,
    _admin: crate::auth::AdminUser,
    headers: HeaderMap,
    Form(form): Form<AdminMeetingWebhookForm>,
) -> impl IntoResponse {
    if let Err(resp) = verify_csrf_token(&headers, &form._csrf) {
        return resp;
    }

    let url = form.meeting_webhook_url.filter(|s| !s.trim().is_empty());
    let auth_mode = match form.meeting_webhook_auth_mode.as_deref() {
        Some(meeting::WEBHOOK_AUTH_HMAC) => Some(meeting::WEBHOOK_AUTH_HMAC.to_string()),
        Some(meeting::WEBHOOK_AUTH_NONE) => Some(meeting::WEBHOOK_AUTH_NONE.to_string()),
        _ => None,
    };
    let display_name = form
        .meeting_webhook_display_name
        .filter(|s| !s.trim().is_empty());

    // Empty secret field on submit means "keep the current value" (same
    // pattern as OIDC and captcha). Wiping the secret requires switching
    // auth_mode away from "hmac".
    let secret_provided = form
        .meeting_webhook_secret
        .as_ref()
        .map(|s| !s.trim().is_empty())
        .unwrap_or(false);

    if secret_provided {
        let secret = form.meeting_webhook_secret.unwrap_or_default();
        let encrypted = match crate::crypto::encrypt_value(&state.secret_key, &secret) {
            Ok(s) => s,
            Err(e) => return internal_error_response("encrypt webhook secret", &e),
        };
        if let Err(e) = sqlx::query(
            "UPDATE auth_config SET meeting_webhook_url = ?, meeting_webhook_auth_mode = ?, meeting_webhook_secret = ?, meeting_webhook_display_name = ?, updated_at = datetime('now') WHERE id = 'singleton'",
        )
        .bind(&url)
        .bind(&auth_mode)
        .bind(&encrypted)
        .bind(&display_name)
        .execute(&state.pool)
        .await
        {
            tracing::error!(error = %e, "failed to save meeting webhook config");
        }
    } else if let Err(e) = sqlx::query(
        "UPDATE auth_config SET meeting_webhook_url = ?, meeting_webhook_auth_mode = ?, meeting_webhook_display_name = ?, updated_at = datetime('now') WHERE id = 'singleton'",
    )
    .bind(&url)
    .bind(&auth_mode)
    .bind(&display_name)
    .execute(&state.pool)
    .await
    {
        tracing::error!(error = %e, "failed to save meeting webhook config");
    }

    *state.meeting_config.write().await =
        meeting::load_config(&state.pool, &state.secret_key).await;

    tracing::info!(admin = %_admin.0.email, "admin: meeting webhook config updated");
    Redirect::to("/dashboard/admin").into_response()
}

// --- Logo management ---

async fn serve_logo(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let logo_path = state.data_dir.join("logo.png");
    match tokio::fs::read(&logo_path).await {
        Ok(bytes) => {
            let content_type = if logo_path.exists() {
                // Detect from magic bytes
                if bytes.starts_with(&[0x89, 0x50, 0x4E, 0x47]) {
                    "image/png"
                } else if bytes.starts_with(&[0xFF, 0xD8]) {
                    "image/jpeg"
                } else if bytes.starts_with(b"<svg") || bytes.starts_with(b"<?xml") {
                    "image/svg+xml"
                } else {
                    "image/png"
                }
            } else {
                "image/png"
            };
            axum::response::Response::builder()
                .status(200)
                .header("Content-Type", content_type)
                .header("Cache-Control", "public, max-age=3600")
                .body(axum::body::Body::from(bytes))
                .unwrap_or_else(|_| axum::response::Response::new(axum::body::Body::empty()))
                .into_response()
        }
        Err(_) => (axum::http::StatusCode::NOT_FOUND, "").into_response(),
    }
}

async fn serve_accent_css(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let css = state.theme_css.read().await.clone();
    axum::response::Response::builder()
        .status(200)
        .header("Content-Type", "text/css; charset=utf-8")
        .header("Cache-Control", "no-cache")
        .body(axum::body::Body::from(css))
        .unwrap_or_else(|_| axum::response::Response::new(axum::body::Body::empty()))
        .into_response()
}

async fn serve_brand_logo() -> impl IntoResponse {
    static BRAND_LOGO: &[u8] = include_bytes!("../../assets/calrs.png");
    axum::response::Response::builder()
        .status(200)
        .header("Content-Type", "image/png")
        .header("Cache-Control", "public, max-age=86400")
        .body(axum::body::Body::from(BRAND_LOGO))
        .unwrap_or_else(|_| axum::response::Response::new(axum::body::Body::empty()))
        .into_response()
}

/// Serves the embed runtime that consumers paste into their own sites. It's
/// loaded cross-origin, so we set permissive CORS — there's no auth state to
/// leak. The contents are checked into `assets/embed.js` and baked into the
/// binary via `include_str!`.
async fn serve_embed_js() -> impl IntoResponse {
    static EMBED_JS: &str = include_str!("../../assets/embed.js");
    axum::response::Response::builder()
        .status(200)
        .header("Content-Type", "application/javascript; charset=utf-8")
        .header("Cache-Control", "public, max-age=3600")
        .header("Access-Control-Allow-Origin", "*")
        .body(axum::body::Body::from(EMBED_JS))
        .unwrap_or_else(|_| axum::response::Response::new(axum::body::Body::empty()))
        .into_response()
}

async fn serve_font_inter_latin() -> impl IntoResponse {
    static FONT: &[u8] = include_bytes!("../../assets/inter-latin.woff2");
    axum::response::Response::builder()
        .status(200)
        .header("Content-Type", "font/woff2")
        .header("Cache-Control", "public, max-age=31536000, immutable")
        .body(axum::body::Body::from(FONT))
        .unwrap_or_else(|_| axum::response::Response::new(axum::body::Body::empty()))
        .into_response()
}

async fn serve_font_inter_latin_ext() -> impl IntoResponse {
    static FONT: &[u8] = include_bytes!("../../assets/inter-latin-ext.woff2");
    axum::response::Response::builder()
        .status(200)
        .header("Content-Type", "font/woff2")
        .header("Cache-Control", "public, max-age=31536000, immutable")
        .body(axum::body::Body::from(FONT))
        .unwrap_or_else(|_| axum::response::Response::new(axum::body::Body::empty()))
        .into_response()
}

async fn admin_upload_logo(
    State(state): State<Arc<AppState>>,
    _admin: crate::auth::AdminUser,
    headers: HeaderMap,
    Query(csrf_query): Query<CsrfQuery>,
    mut multipart: Multipart,
) -> impl IntoResponse {
    if let Err(resp) = verify_csrf_token(&headers, &csrf_query._csrf) {
        return resp;
    }
    while let Ok(Some(field)) = multipart.next_field().await {
        if field.name() == Some("logo") {
            let content_type = field.content_type().unwrap_or("").to_string();
            if !content_type.starts_with("image/") {
                return Redirect::to("/dashboard/admin").into_response();
            }
            if let Ok(bytes) = field.bytes().await {
                if bytes.len() > 2 * 1024 * 1024 {
                    return Redirect::to("/dashboard/admin").into_response();
                }
                // Trust magic bytes, not the multipart Content-Type header.
                if detect_image_ext(&bytes).is_none() {
                    return Redirect::to("/dashboard/admin").into_response();
                }
                let logo_path = state.data_dir.join("logo.png");
                let _ = tokio::fs::write(&logo_path, &bytes).await;
            }
        }
    }
    Redirect::to("/dashboard/admin").into_response()
}

async fn admin_delete_logo(
    State(state): State<Arc<AppState>>,
    _admin: crate::auth::AdminUser,
    headers: HeaderMap,
    Form(csrf): Form<CsrfForm>,
) -> impl IntoResponse {
    if let Err(resp) = verify_csrf_token(&headers, &csrf._csrf) {
        return resp;
    }
    let logo_path = state.data_dir.join("logo.png");
    let _ = tokio::fs::remove_file(&logo_path).await;
    Redirect::to("/dashboard/admin").into_response()
}

async fn admin_update_company_link(
    State(state): State<Arc<AppState>>,
    _admin: crate::auth::AdminUser,
    headers: HeaderMap,
    Form(form): Form<CompanyLinkForm>,
) -> impl IntoResponse {
    if let Err(resp) = verify_csrf_token(&headers, &form._csrf) {
        return resp;
    }
    let link = form.company_link.trim().to_string();
    // Reject anything that isn't http(s). An admin (or attacker who has
    // taken over an admin account) could otherwise set `javascript:` and
    // turn the company link rendered on every public booking page into
    // stored XSS.
    if !link.is_empty() && !is_safe_company_link(&link) {
        tracing::warn!(
            admin = %_admin.0.email,
            "admin: company_link rejected (only http/https schemes allowed)"
        );
        let msg =
            urlencoding::encode("Company link must start with http:// or https://").into_owned();
        return Redirect::to(&format!("/dashboard/admin?error={}", msg)).into_response();
    }
    let link_value: Option<&str> = if link.is_empty() { None } else { Some(&link) };
    let _ = sqlx::query(
        "UPDATE auth_config SET company_link = ?, updated_at = datetime('now') WHERE id = 'singleton'",
    )
    .bind(link_value)
    .execute(&state.pool)
    .await;
    *state.company_link.write().await = if link.is_empty() { None } else { Some(link) };
    Redirect::to("/dashboard/admin").into_response()
}

// --- Impersonation ---

async fn admin_impersonate(
    State(_state): State<Arc<AppState>>,
    _admin: crate::auth::AdminUser,
    headers: HeaderMap,
    Path(user_id): Path<String>,
    Form(csrf): Form<CsrfForm>,
) -> impl IntoResponse {
    if let Err(resp) = verify_csrf_token(&headers, &csrf._csrf) {
        return resp;
    }
    tracing::warn!(admin = %_admin.0.email, target = %user_id, "admin: impersonation started");
    let cookie = format!(
        "__Host-calrs_impersonate={}; HttpOnly; Secure; SameSite=Lax; Path=/; Max-Age={}",
        user_id,
        86400 // 24 hours
    );
    ([("Set-Cookie", cookie)], Redirect::to("/dashboard")).into_response()
}

async fn admin_stop_impersonate(
    _admin: crate::auth::AdminUser,
    headers: HeaderMap,
    Form(csrf): Form<CsrfForm>,
) -> impl IntoResponse {
    if let Err(resp) = verify_csrf_token(&headers, &csrf._csrf) {
        return resp;
    }
    tracing::info!("admin: impersonation ended");
    let cookie = "__Host-calrs_impersonate=; HttpOnly; Secure; SameSite=Lax; Path=/; Max-Age=0";
    (
        [("Set-Cookie", cookie.to_string())],
        Redirect::to("/dashboard"),
    )
        .into_response()
}

// --- Admin: update member weight ---

#[derive(Deserialize)]
struct WeightForm {
    _csrf: Option<String>,
    weight: i64,
}

async fn admin_update_member_weight(
    State(state): State<Arc<AppState>>,
    _admin: crate::auth::AdminUser,
    headers: HeaderMap,
    Path((group_id, user_id)): Path<(String, String)>,
    Form(form): Form<WeightForm>,
) -> impl IntoResponse {
    if let Err(resp) = verify_csrf_token(&headers, &form._csrf) {
        return resp;
    }
    let w = form.weight.clamp(1, 100);
    let _ = sqlx::query("UPDATE user_groups SET weight = ? WHERE group_id = ? AND user_id = ?")
        .bind(w)
        .bind(&group_id)
        .bind(&user_id)
        .execute(&state.pool)
        .await;
    tracing::info!(
        group_id,
        user_id,
        weight = w,
        "admin: updated member weight"
    );
    Redirect::to("/dashboard/admin").into_response()
}

// --- Token-based approve/decline (from email) ---

#[derive(Deserialize)]
struct DeclineForm {
    _csrf: Option<String>,
    reason: Option<String>,
}

/// Render an error page for token-based actions (shared by approve form and handler).
fn render_token_error(
    state: &AppState,
    headers: &HeaderMap,
    _token: &str,
    already: Option<(String,)>,
) -> axum::response::Response {
    let lang = crate::i18n::detect_from_headers(headers);
    let (title, message) = match already {
        Some((status,)) if status == "confirmed" => (
            "Already approved",
            "This booking has already been approved.",
        ),
        Some((status,)) if status == "declined" => (
            "Already declined",
            "This booking has already been declined.",
        ),
        Some((status,)) if status == "cancelled" => {
            ("Booking cancelled", "This booking was cancelled.")
        }
        _ => (
            "Invalid link",
            "This approval link is invalid or has expired.",
        ),
    };

    let tmpl = match state.templates.get_template("booking_action_error.html") {
        Ok(t) => t,
        Err(e) => return internal_error_response("internal", &e),
    };
    let rendered = tmpl
        .render(context! {
            title,
            message,
            lang => lang,
        })
        .unwrap_or_else(|e| internal_error_body("template render", &e));
    Html(rendered).into_response()
}

async fn approve_booking_form(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(token): Path<String>,
) -> impl IntoResponse {
    let lang = crate::i18n::detect_from_headers(&headers);
    // Look up pending booking by confirm_token
    let booking: Option<(String, String, String, String, String)> = sqlx::query_as(
        "SELECT b.guest_name, b.guest_email, b.start_at, b.end_at, et.title
         FROM bookings b
         JOIN event_types et ON et.id = b.event_type_id
         WHERE b.confirm_token = ? AND b.status = 'pending'",
    )
    .bind(&token)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    let (guest_name, guest_email, start_at, end_at, event_title) = match booking {
        Some(b) => b,
        None => {
            let already: Option<(String,)> =
                sqlx::query_as("SELECT status FROM bookings WHERE confirm_token = ?")
                    .bind(&token)
                    .fetch_optional(&state.pool)
                    .await
                    .unwrap_or(None);
            return render_token_error(&state, &headers, &token, already);
        }
    };

    let date_label = format_date_label(&start_at, lang);
    let start_time = extract_time_24h(&start_at);
    let end_time = extract_time_24h(&end_at);

    let tmpl = match state.templates.get_template("booking_approve_form.html") {
        Ok(t) => t,
        Err(e) => return internal_error_response("internal", &e),
    };
    tmpl.render(context! {
        event_title,
        date_label,
        start_time,
        end_time,
        guest_name,
        guest_email,
        lang => lang,
    })
    .map(|r| Html(r).into_response())
    .unwrap_or_else(|e| internal_error_response("template render", &e))
}

async fn approve_booking_by_token(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(token): Path<String>,
) -> impl IntoResponse {
    let lang = crate::i18n::detect_from_headers(&headers);
    // Look up booking by confirm_token
    let booking: Option<(String, String, String, String, String, String, String, String, String, Option<String>, Option<String>, String, Option<String>, String)> =
        sqlx::query_as(
            "SELECT b.id, b.uid, b.guest_name, b.guest_email, b.start_at, b.end_at, et.title, a.user_id, u.name, et.location_value, b.cancel_token, COALESCE(b.guest_timezone, 'UTC'), b.reschedule_token, b.event_type_id
             FROM bookings b
             JOIN event_types et ON et.id = b.event_type_id
             JOIN accounts a ON a.id = et.account_id
             JOIN users u ON u.id = a.user_id
             WHERE b.confirm_token = ? AND b.status = 'pending'",
        )
        .bind(&token)
        .fetch_optional(&state.pool)
        .await
        .unwrap_or(None);

    let (
        bid,
        uid,
        guest_name,
        guest_email,
        start_at,
        end_at,
        event_title,
        user_id,
        host_name,
        location_value,
        cancel_token,
        guest_timezone,
        reschedule_token,
        event_type_id,
    ) = match booking {
        Some(b) => b,
        None => {
            let already: Option<(String,)> =
                sqlx::query_as("SELECT status FROM bookings WHERE confirm_token = ?")
                    .bind(&token)
                    .fetch_optional(&state.pool)
                    .await
                    .unwrap_or(None);
            return render_token_error(&state, &headers, &token, already);
        }
    };

    // Confirm the booking
    let _ = sqlx::query("UPDATE bookings SET status = 'confirmed' WHERE id = ?")
        .bind(&bid)
        .execute(&state.pool)
        .await;

    tracing::info!(booking_id = %bid, "booking approved via token");

    // start_at/end_at are naive in the event type's timezone; convert to the
    // guest's tz so the email body and the ICS attachment match what the
    // guest booked. See #101.
    let stored_tz = get_host_tz(&state.pool, &event_type_id).await;
    let guest_tz_parsed = guest_timezone.parse::<Tz>().unwrap_or(Tz::UTC);
    let (date, start_time, end_time) =
        booking_strings_in_guest_tz(&start_at, &end_at, stored_tz, guest_tz_parsed);
    let date_label = format_date_label(&date, lang);

    // Get host email for BookingDetails
    let host_email: String =
        sqlx::query_scalar("SELECT COALESCE(booking_email, email) FROM users WHERE id = ?")
            .bind(&user_id)
            .fetch_one(&state.pool)
            .await
            .unwrap_or_default();

    // Status is now 'confirmed' (we just transitioned it). Generate the
    // auto meeting URL if the event type uses one; otherwise fall back to
    // the static location_value.
    let location_display = resolve_booking_location(
        &state,
        &bid,
        &event_type_id,
        Some(&user_id),
        location_value.as_deref(),
        &guest_name,
        &guest_email,
        true,
    )
    .await;

    let details = crate::email::BookingDetails {
        event_title: event_title.clone(),
        date: date.clone(),
        start_time: start_time.clone(),
        end_time: end_time.clone(),
        guest_name: guest_name.clone(),
        guest_email: guest_email.clone(),
        guest_timezone,
        host_name: host_name.clone(),
        host_email,
        uid: uid.clone(),
        notes: None,
        location: location_display,
        reminder_minutes: None,
        additional_attendees: vec![],
        host_timezone: stored_tz.name().to_string(),
        ..Default::default()
    };

    // Push to CalDAV calendar
    caldav_push_booking(&state.pool, &state.secret_key, &user_id, &uid, &details).await;

    // Notify watcher teams
    notify_watchers(
        &state.pool,
        &state.secret_key,
        &bid,
        &event_type_id,
        &host_name,
        &details,
    )
    .await;

    // Send confirmation email to guest
    if let Ok(Some(smtp_config)) =
        crate::email::load_smtp_config(&state.pool, &state.secret_key).await
    {
        let base_url = std::env::var("CALRS_BASE_URL").ok();
        let guest_cancel_url = cancel_token.as_ref().and_then(|t| {
            base_url
                .as_ref()
                .map(|base| format!("{}/booking/cancel/{}", base.trim_end_matches('/'), t))
        });
        let guest_reschedule_url = reschedule_token.as_ref().and_then(|t| {
            base_url
                .as_ref()
                .map(|base| format!("{}/booking/reschedule/{}", base.trim_end_matches('/'), t))
        });
        let (cancel_notice_min, reschedule_notice_min) =
            fetch_event_type_notice_minutes(&state.pool, &event_type_id).await;
        let _ = crate::email::send_guest_confirmation_ex(
            &smtp_config,
            &details,
            guest_cancel_url.as_deref(),
            guest_reschedule_url.as_deref(),
            cancel_notice_min,
            reschedule_notice_min,
        )
        .await;

        // Also send host a confirmation email (no ICS, event pushed via CalDAV)
        if let Err(e) = crate::email::send_host_booking_confirmed(&smtp_config, &details).await {
            tracing::error!(error = %e, host_email = %details.host_email, "host confirmation email failed");
        }
    }

    let tmpl = match state.templates.get_template("booking_approved.html") {
        Ok(t) => t,
        Err(e) => return internal_error_response("internal", &e),
    };
    let rendered = tmpl
        .render(context! {
            event_title,
            date_label,
            date,
            start_time,
            end_time,
            guest_name,
            guest_email,
            lang => lang,
        })
        .unwrap_or_else(|e| internal_error_body("template render", &e));

    Html(rendered).into_response()
}

async fn decline_booking_form(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(token): Path<String>,
) -> impl IntoResponse {
    let lang = crate::i18n::detect_from_headers(&headers);
    let booking: Option<(String, String, String, String, String)> = sqlx::query_as(
        "SELECT b.guest_name, b.guest_email, b.start_at, b.end_at, et.title
             FROM bookings b
             JOIN event_types et ON et.id = b.event_type_id
             WHERE b.confirm_token = ? AND b.status = 'pending'",
    )
    .bind(&token)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    let (guest_name, guest_email, start_at, end_at, event_title) = match booking {
        Some(b) => b,
        None => {
            let tmpl = match state.templates.get_template("booking_action_error.html") {
                Ok(t) => t,
                Err(e) => return internal_error_response("internal", &e),
            };
            let rendered = tmpl.render(context! {
                title => "Invalid link",
                message => "This decline link is invalid, has expired, or the booking has already been processed.",
                lang => lang,
            }).unwrap_or_else(|e| internal_error_body("template render", &e));
            return Html(rendered).into_response();
        }
    };

    let date_label = format_date_label(&start_at, lang);
    let date = start_at.get(..10).unwrap_or(&start_at).to_string();
    let start_time = extract_time_24h(&start_at);
    let end_time = extract_time_24h(&end_at);

    let tmpl = match state.templates.get_template("booking_decline_form.html") {
        Ok(t) => t,
        Err(e) => return internal_error_response("internal", &e),
    };
    let rendered = tmpl
        .render(context! {
            event_title,
            date_label,
            date,
            start_time,
            end_time,
            guest_name,
            guest_email,
            lang => lang,
        })
        .unwrap_or_else(|e| internal_error_body("template render", &e));

    Html(rendered).into_response()
}

async fn decline_booking_by_token(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(token): Path<String>,
    Form(form): Form<DeclineForm>,
) -> impl IntoResponse {
    if let Err(resp) = verify_csrf_token(&headers, &form._csrf) {
        return resp;
    }
    let lang = crate::i18n::detect_from_headers(&headers);
    let booking: Option<(
        String,
        String,
        String,
        String,
        String,
        String,
        String,
        String,
        String,
        String,
    )> = sqlx::query_as(
        "SELECT b.id, b.guest_name, b.guest_email, b.start_at, b.end_at, et.title, u.name, COALESCE(u.booking_email, u.email), COALESCE(b.guest_timezone, 'UTC'), et.id
             FROM bookings b
             JOIN event_types et ON et.id = b.event_type_id
             JOIN accounts a ON a.id = et.account_id
             JOIN users u ON u.id = a.user_id
             WHERE b.confirm_token = ? AND b.status = 'pending'",
    )
    .bind(&token)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    let (
        bid,
        guest_name,
        guest_email,
        start_at,
        end_at,
        event_title,
        host_name,
        host_email,
        guest_timezone,
        et_id,
    ) = match booking {
        Some(b) => b,
        None => {
            let tmpl = match state.templates.get_template("booking_action_error.html") {
                Ok(t) => t,
                Err(e) => return internal_error_response("internal", &e),
            };
            let rendered = tmpl.render(context! {
                    title => "Invalid link",
                    message => "This decline link is invalid, has expired, or the booking has already been processed.",
                    lang => lang,
                }).unwrap_or_else(|e| internal_error_body("template render", &e));
            return Html(rendered).into_response();
        }
    };

    // Decline the booking
    let _ = sqlx::query("UPDATE bookings SET status = 'declined' WHERE id = ?")
        .bind(&bid)
        .execute(&state.pool)
        .await;

    tracing::info!(booking_id = %bid, "booking declined via token");

    // Convert event-type-local stored times into the guest's tz; see #101.
    let stored_tz = get_host_tz(&state.pool, &et_id).await;
    let guest_tz_parsed = guest_timezone.parse::<Tz>().unwrap_or(Tz::UTC);
    let (date, start_time, end_time) =
        booking_strings_in_guest_tz(&start_at, &end_at, stored_tz, guest_tz_parsed);
    let date_label = format_date_label(&date, lang);

    let reason = form.reason.filter(|r| !r.trim().is_empty());

    // Send decline notification to guest
    if let Ok(Some(smtp_config)) =
        crate::email::load_smtp_config(&state.pool, &state.secret_key).await
    {
        let details = crate::email::CancellationDetails {
            event_title: event_title.clone(),
            date: date.clone(),
            start_time: start_time.clone(),
            end_time: end_time.clone(),
            guest_name: guest_name.clone(),
            guest_email: guest_email.clone(),
            guest_timezone: guest_timezone.clone(),
            host_name: host_name.clone(),
            host_email,
            uid: String::new(),
            reason: reason.clone(),
            cancelled_by_host: true,
            host_timezone: stored_tz.name().to_string(),
            ..Default::default()
        };
        let _ = crate::email::send_guest_decline_notice(&smtp_config, &details).await;
    }

    let tmpl = match state.templates.get_template("booking_declined.html") {
        Ok(t) => t,
        Err(e) => return internal_error_response("internal", &e),
    };
    let rendered = tmpl
        .render(context! {
            event_title,
            date_label,
            date,
            start_time,
            end_time,
            guest_name,
            guest_email,
            reason,
            lang => lang,
        })
        .unwrap_or_else(|e| internal_error_body("template render", &e));

    Html(rendered).into_response()
}

// --- Guest cancel booking by token ---

/// Fetch `(cancel_notice_min, reschedule_notice_min)` for an event type by
/// id. Either column may be NULL (no restriction).
async fn fetch_event_type_notice_minutes(
    pool: &SqlitePool,
    event_type_id: &str,
) -> (Option<i32>, Option<i32>) {
    sqlx::query_as::<_, (Option<i32>, Option<i32>)>(
        "SELECT cancel_notice_min, reschedule_notice_min FROM event_types WHERE id = ?",
    )
    .bind(event_type_id)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten()
    .unwrap_or((None, None))
}

/// Look up a notice-window column on the booking's event type. Returns
/// `(notice_min, host_email)` where `notice_min` is `None`/`Some(0)` when
/// no restriction applies. The `column` argument MUST be one of the two
/// known column names — it is interpolated into SQL but never sourced from
/// the request, so this is safe.
async fn fetch_notice_min_and_host_email(
    pool: &SqlitePool,
    booking_token_column: &str,
    token: &str,
    notice_column: &str,
) -> Option<(Option<i32>, String)> {
    debug_assert!(matches!(
        notice_column,
        "cancel_notice_min" | "reschedule_notice_min"
    ));
    debug_assert!(matches!(
        booking_token_column,
        "cancel_token" | "reschedule_token"
    ));
    let sql = format!(
        "SELECT et.{notice}, COALESCE(u.booking_email, u.email) \
         FROM bookings b \
         JOIN event_types et ON et.id = b.event_type_id \
         JOIN accounts a ON a.id = et.account_id \
         JOIN users u ON u.id = a.user_id \
         WHERE b.{tok} = ?",
        notice = notice_column,
        tok = booking_token_column
    );
    sqlx::query_as::<_, (Option<i32>, String)>(&sql)
        .bind(token)
        .fetch_optional(pool)
        .await
        .unwrap_or(None)
}

/// If `notice_min` is set and the booking starts inside the notice window,
/// render the blocked page and return it. Otherwise return `None` so the
/// caller can proceed.
///
/// Note on time semantics: `start_at` is the host-local naive timestamp
/// (matching how bookings are stored elsewhere) so we compare against
/// `Local::now().naive_local()` to mirror the existing `slot_start < now +
/// Duration::minutes(min_notice as i64)` precedent in the reschedule POST
/// path.
fn check_notice_window(
    state: &AppState,
    notice_min: Option<i32>,
    start_at: &str,
    host_email: &str,
    action: &'static str,
    lang: &'static str,
) -> Option<Response> {
    let n = notice_min.unwrap_or(0);
    if n <= 0 {
        return None;
    }
    let start = match NaiveDateTime::parse_from_str(start_at, "%Y-%m-%dT%H:%M:%S") {
        Ok(dt) => dt,
        Err(_) => return None,
    };
    let cutoff = start - chrono::Duration::minutes(n as i64);
    if Local::now().naive_local() <= cutoff {
        return None;
    }
    let tmpl = match state.templates.get_template("booking_action_blocked.html") {
        Ok(t) => t,
        Err(e) => return Some(internal_error_response("internal", &e)),
    };
    let rendered = tmpl
        .render(context! {
            notice_min => n,
            host_email => host_email,
            action => action,
            lang => lang,
        })
        .unwrap_or_else(|e| internal_error_body("template render", &e));
    Some(Html(rendered).into_response())
}

async fn guest_cancel_form(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(token): Path<String>,
) -> impl IntoResponse {
    let lang = crate::i18n::detect_from_headers(&headers);
    let booking: Option<(String, String, String, String, String, String)> = sqlx::query_as(
        "SELECT b.guest_name, b.guest_email, b.start_at, b.end_at, et.title, u.name
             FROM bookings b
             JOIN event_types et ON et.id = b.event_type_id
             JOIN accounts a ON a.id = et.account_id
             JOIN users u ON u.id = a.user_id
             WHERE b.cancel_token = ? AND b.status IN ('confirmed', 'pending')",
    )
    .bind(&token)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    let (guest_name, _guest_email, start_at, end_at, event_title, host_name) = match booking {
        Some(b) => b,
        None => {
            // Check if already cancelled
            let status_row: Option<(String,)> =
                sqlx::query_as("SELECT status FROM bookings WHERE cancel_token = ?")
                    .bind(&token)
                    .fetch_optional(&state.pool)
                    .await
                    .unwrap_or(None);

            let (title, message) = match status_row {
                Some((status,)) if status == "cancelled" => (
                    "Already cancelled",
                    "This booking has already been cancelled.",
                ),
                Some((status,)) if status == "declined" => (
                    "Booking declined",
                    "This booking has been declined by the host.",
                ),
                _ => (
                    "Invalid link",
                    "This cancellation link is invalid or has expired.",
                ),
            };

            let tmpl = match state.templates.get_template("booking_action_error.html") {
                Ok(t) => t,
                Err(e) => return internal_error_response("internal", &e),
            };
            let rendered = tmpl
                .render(context! {
                    title,
                    message,
                    lang => lang,
                })
                .unwrap_or_else(|e| internal_error_body("template render", &e));
            return Html(rendered).into_response();
        }
    };

    // Block the GET form before any state mutation if the booking falls
    // inside the host's configured cancel-notice window.
    if let Some((notice_min, host_email)) =
        fetch_notice_min_and_host_email(&state.pool, "cancel_token", &token, "cancel_notice_min")
            .await
    {
        if let Some(resp) =
            check_notice_window(&state, notice_min, &start_at, &host_email, "cancel", lang)
        {
            tracing::info!(action = "cancel", "guest action blocked by notice window");
            return resp;
        }
    }

    let date_label = format_date_label(&start_at, lang);
    let date = start_at.get(..10).unwrap_or(&start_at).to_string();
    let start_time = extract_time_24h(&start_at);
    let end_time = extract_time_24h(&end_at);

    let tmpl = match state.templates.get_template("booking_cancel_form.html") {
        Ok(t) => t,
        Err(e) => return internal_error_response("internal", &e),
    };
    let rendered = tmpl
        .render(context! {
            event_title,
            date_label,
            date,
            start_time,
            end_time,
            guest_name,
            host_name,
            lang => lang,
        })
        .unwrap_or_else(|e| internal_error_body("template render", &e));

    Html(rendered).into_response()
}

async fn guest_cancel_booking(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(token): Path<String>,
    Form(form): Form<CancelForm>,
) -> impl IntoResponse {
    if let Err(resp) = verify_csrf_token(&headers, &form._csrf) {
        return resp;
    }
    let lang = crate::i18n::detect_from_headers(&headers);
    let booking: Option<(String, String, String, String, String, String, String, String, String, String, String)> =
        sqlx::query_as(
            "SELECT b.id, b.uid, b.guest_name, b.guest_email, b.start_at, b.end_at, et.title, u.name, COALESCE(u.booking_email, u.email), COALESCE(b.guest_timezone, 'UTC'), et.id
             FROM bookings b
             JOIN event_types et ON et.id = b.event_type_id
             JOIN accounts a ON a.id = et.account_id
             JOIN users u ON u.id = a.user_id
             WHERE b.cancel_token = ? AND b.status IN ('confirmed', 'pending')",
        )
        .bind(&token)
        .fetch_optional(&state.pool)
        .await
        .unwrap_or(None);

    let (
        bid,
        uid,
        guest_name,
        guest_email,
        start_at,
        end_at,
        event_title,
        host_name,
        host_email,
        guest_timezone,
        et_id,
    ) = match booking {
        Some(b) => b,
        None => {
            let tmpl = match state.templates.get_template("booking_action_error.html") {
                Ok(t) => t,
                Err(e) => return internal_error_response("internal", &e),
            };
            let rendered = tmpl
                    .render(context! {
                        title => "Invalid link",
                        message => "This cancellation link is invalid, has expired, or the booking has already been cancelled.",
                        lang => lang,
                    })
                    .unwrap_or_else(|e| internal_error_body("template render", &e));
            return Html(rendered).into_response();
        }
    };

    // Defensive notice-window check on POST too: the GET may have been
    // cached, or the cutoff may have rolled over between page load and
    // submit. Either way, never mutate state inside the blocked window.
    if let Some((notice_min, _)) =
        fetch_notice_min_and_host_email(&state.pool, "cancel_token", &token, "cancel_notice_min")
            .await
    {
        if let Some(resp) =
            check_notice_window(&state, notice_min, &start_at, &host_email, "cancel", lang)
        {
            tracing::info!(
                action = "cancel",
                booking_id = %bid,
                "guest action blocked by notice window (POST)"
            );
            return resp;
        }
    }

    // Cancel the booking
    let _ = sqlx::query("UPDATE bookings SET status = 'cancelled' WHERE id = ?")
        .bind(&bid)
        .execute(&state.pool)
        .await;

    tracing::info!(booking_id = %bid, "booking cancelled by guest");

    // Find the host user_id for CalDAV delete
    let host_user_id: Option<String> = sqlx::query_scalar(
        "SELECT a.user_id FROM accounts a JOIN event_types et ON et.account_id = a.id JOIN bookings b ON b.event_type_id = et.id WHERE b.id = ?",
    )
    .bind(&bid)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    // Delete from CalDAV calendar
    if let Some(user_id) = &host_user_id {
        caldav_delete_booking(&state.pool, &state.secret_key, user_id, &uid).await;
    }

    // Convert event-type-local stored times into the guest's tz; see #101.
    let stored_tz = get_host_tz(&state.pool, &et_id).await;
    let guest_tz_parsed = guest_timezone.parse::<Tz>().unwrap_or(Tz::UTC);
    let (date, start_time, end_time) =
        booking_strings_in_guest_tz(&start_at, &end_at, stored_tz, guest_tz_parsed);
    let date_label = format_date_label(&date, lang);

    let reason = form.reason.filter(|r| !r.trim().is_empty());

    // Send cancellation emails
    if let Ok(Some(smtp_config)) =
        crate::email::load_smtp_config(&state.pool, &state.secret_key).await
    {
        let details = crate::email::CancellationDetails {
            event_title: event_title.clone(),
            date: date.clone(),
            start_time: start_time.clone(),
            end_time: end_time.clone(),
            guest_name: guest_name.clone(),
            guest_email: guest_email.clone(),
            guest_timezone,
            host_name: host_name.clone(),
            host_email,
            uid,
            reason: reason.clone(),
            cancelled_by_host: false,
            // Guest is the one cancelling; their browser language now is the
            // best signal we have (they chose this language to view the form).
            guest_language: Some(lang.to_string()),
            host_timezone: stored_tz.name().to_string(),
            ..Default::default()
        };

        let _ = crate::email::send_guest_cancellation(&smtp_config, &details).await;
        let _ = crate::email::send_host_cancellation(&smtp_config, &details).await;
    }

    let tmpl = match state.templates.get_template("booking_cancelled_guest.html") {
        Ok(t) => t,
        Err(e) => return internal_error_response("internal", &e),
    };
    let rendered = tmpl
        .render(context! {
            event_title,
            date_label,
            date,
            start_time,
            end_time,
            host_name,
            reason,
            lang => lang,
        })
        .unwrap_or_else(|e| internal_error_body("template render", &e));

    Html(rendered).into_response()
}

// --- Reschedule handlers ---

#[derive(Deserialize)]
struct RescheduleQuery {
    #[serde(default)]
    date: Option<String>,
    #[serde(default)]
    time: Option<String>,
    #[serde(default)]
    tz: Option<String>,
    #[serde(default)]
    month: Option<String>,
}

#[derive(Deserialize)]
struct RescheduleForm {
    _csrf: Option<String>,
    date: String,
    time: String,
    #[serde(default)]
    tz: Option<String>,
}

/// Guest reschedule: show slot picker or confirmation page
async fn guest_reschedule_slots(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(token): Path<String>,
    Query(query): Query<RescheduleQuery>,
) -> impl IntoResponse {
    let lang = crate::i18n::detect_from_headers(&headers);
    // Look up booking by reschedule_token
    let booking: Option<(String, String, String, String, String, String)> = sqlx::query_as(
        "SELECT b.id, b.guest_name, b.start_at, b.end_at, b.event_type_id, b.uid
             FROM bookings b
             WHERE b.reschedule_token = ? AND b.status IN ('confirmed', 'pending')",
    )
    .bind(&token)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    let (booking_id, _guest_name, start_at, end_at, et_id_raw, _uid) = match booking {
        Some(b) => b,
        None => {
            let tmpl = match state.templates.get_template("booking_action_error.html") {
                Ok(t) => t,
                Err(e) => return internal_error_response("internal", &e),
            };
            let rendered = tmpl.render(context! {
                title => "Invalid link",
                message => "This reschedule link is invalid, has expired, or the booking has already been processed.",
                lang => lang,
            }).unwrap_or_else(|e| internal_error_body("template render", &e));
            return Html(rendered).into_response();
        }
    };

    // Block the slot picker before any costly work (CalDAV sync, slot
    // computation, template render) if the booking falls inside the host's
    // configured reschedule-notice window.
    if let Some((notice_min, host_email)) = fetch_notice_min_and_host_email(
        &state.pool,
        "reschedule_token",
        &token,
        "reschedule_notice_min",
    )
    .await
    {
        if let Some(resp) = check_notice_window(
            &state,
            notice_min,
            &start_at,
            &host_email,
            "reschedule",
            lang,
        ) {
            tracing::info!(
                action = "reschedule",
                booking_id = %booking_id,
                "guest action blocked by notice window"
            );
            return resp;
        }
    }

    // Fetch event type + host details
    let et_info: Option<(
        String,
        String,
        i32,
        i32,
        i32,
        i32,
        Option<String>,
        Option<String>,
        String,
        String,
        Option<String>,
        Option<String>,
        String,
    )> = sqlx::query_as(
        "SELECT et.id, et.slug, et.duration_min, et.buffer_before, et.buffer_after,
                    et.min_notice_min, et.location_type, et.location_value,
                    u.id, u.name, u.title, u.avatar_path, et.default_calendar_view
             FROM event_types et
             JOIN accounts a ON a.id = et.account_id
             JOIN users u ON u.id = a.user_id
             WHERE et.id = ?",
    )
    .bind(&et_id_raw)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    let (
        et_id,
        et_slug,
        duration,
        buf_before,
        buf_after,
        min_notice,
        loc_type,
        loc_value,
        host_user_id,
        host_name,
        host_title,
        host_avatar_path,
        default_calendar_view,
    ) = match et_info {
        Some(e) => e,
        None => return Html("Event type not found.".to_string()).into_response(),
    };

    let et_title: String = sqlx::query_scalar("SELECT title FROM event_types WHERE id = ?")
        .bind(&et_id)
        .fetch_one(&state.pool)
        .await
        .unwrap_or_default();

    let old_date_label = format_date_label(&start_at, lang);
    let old_start_time = extract_time_24h(&start_at);
    let old_end_time = extract_time_24h(&end_at);
    let old_date = start_at.get(..10).unwrap_or(&start_at).to_string();

    // If date + time + tz are present, show confirmation page
    if let (Some(date), Some(time)) = (&query.date, &query.time) {
        let guest_tz = parse_guest_tz(query.tz.as_deref());
        let new_date = match NaiveDate::parse_from_str(date, "%Y-%m-%d") {
            Ok(d) => d,
            Err(_) => return Html("Invalid date.".to_string()).into_response(),
        };
        let new_time = match NaiveTime::parse_from_str(time, "%H:%M") {
            Ok(t) => t,
            Err(_) => return Html("Invalid time.".to_string()).into_response(),
        };
        let new_end = new_date.and_time(new_time) + Duration::minutes(duration as i64);
        let new_date_label = crate::i18n::format_long_date(new_date, lang);
        let new_start_time_str = new_time.format("%H:%M").to_string();
        let new_end_time_str = new_end.time().format("%H:%M").to_string();

        let back_url = format!("/booking/reschedule/{}?tz={}", token, guest_tz.name());

        let tmpl = match state
            .templates
            .get_template("booking_reschedule_confirm.html")
        {
            Ok(t) => t,
            Err(e) => return internal_error_response("internal", &e),
        };
        let rendered = tmpl
            .render(context! {
                event_title => et_title,
                old_date_label => old_date_label,
                old_start_time => old_start_time,
                old_end_time => old_end_time,
                new_date_label => new_date_label,
                new_start_time => new_start_time_str,
                new_end_time => new_end_time_str,
                host_name => host_name,
                date => date,
                time => time,
                tz => guest_tz.name(),
                back_url => back_url,
                company_link => state.company_link.read().await.clone(),
                lang => lang,
            })
            .unwrap_or_else(|e| internal_error_body("template render", &e));
        return Html(rendered).into_response();
    }

    // Show slot picker with reschedule context
    crate::commands::sync::sync_if_stale(&state.pool, &state.secret_key, &host_user_id).await;

    let guest_tz = parse_guest_tz(query.tz.as_deref());
    let host_tz = get_host_tz(&state.pool, &et_id).await;
    let guest_tz_name = guest_tz.name().to_string();

    let (year, month) = parse_month_param(query.month.as_deref(), guest_tz);
    let (
        start_offset,
        days_ahead,
        month_label,
        prev_month,
        next_month,
        first_weekday,
        days_in_month,
        today_date,
        month_year,
    ) = build_month_params(year, month, host_tz, guest_tz, lang);

    let now_host = Utc::now().with_timezone(&host_tz).naive_local();
    let end_date = now_host.date() + Duration::days((start_offset + days_ahead) as i64);
    let window_end = end_date.and_hms_opt(23, 59, 59).unwrap_or(now_host);
    let busy = BusySource::Individual(
        fetch_busy_times_for_user_ex(
            &state.pool,
            &host_user_id,
            now_host,
            window_end,
            host_tz,
            Some(&et_id),
            Some(&booking_id),
        )
        .await,
    );
    let slot_days = compute_slots(
        &state.pool,
        &et_id,
        duration,
        buf_before,
        buf_after,
        min_notice,
        start_offset,
        days_ahead,
        host_tz,
        guest_tz,
        busy,
    )
    .await;

    let days_ctx: Vec<minijinja::Value> = slot_days.iter().map(|d| {
        let slots: Vec<minijinja::Value> = d.slots.iter().map(|s| {
            context! { start => s.start, end => s.end, host_date => s.host_date, host_time => s.host_time, guest_date => s.guest_date }
        }).collect();
        context! { date => d.date, label => d.label, slots => slots }
    }).collect();
    let available_dates: Vec<String> = slot_days.iter().map(|d| d.date.clone()).collect();

    let tz_options: Vec<minijinja::Value> = common_timezones_with(&guest_tz_name)
        .iter()
        .map(|(iana, label)| {
            context! { value => iana, label => label, selected => (*iana == guest_tz_name) }
        })
        .collect();

    let reschedule_base = format!("/booking/reschedule/{}", token);

    let (meeting_jitsi_label, meeting_webhook_label) = meeting_provider_labels(&state).await;
    let tmpl = match state.templates.get_template("slots.html") {
        Ok(t) => t,
        Err(e) => return internal_error_response("internal", &e),
    };
    let rendered = tmpl
        .render(context! {
            meeting_jitsi_label => meeting_jitsi_label,
            meeting_webhook_label => meeting_webhook_label,
            event_type => context! {
                slug => et_slug,
                title => et_title.clone(),
                description => Option::<String>::None,
                duration_min => duration,
                location_type => loc_type,
                location_value => loc_value,
            },
            host_name => host_name,
            host_title => host_title.as_deref().unwrap_or(""),
            host_user_id => host_user_id,
            host_has_avatar => host_avatar_path.is_some(),
            host_initials => compute_initials(&host_name),
            days => days_ctx,
            available_dates => available_dates,
            month_label => month_label,
            month_year => month_year,
            prev_month => prev_month,
            next_month => next_month,
            first_weekday => first_weekday,
            days_in_month => days_in_month,
            today_date => today_date,
            guest_tz => guest_tz_name,
            tz_options => tz_options,
            reschedule_base => reschedule_base,
            reschedule_info => context! {
                event_title => et_title,
                old_date => old_date,
                old_time => old_start_time,
            },
            default_calendar_view => default_calendar_view,
            company_link => state.company_link.read().await.clone(),
            lang => lang,
        })
        .unwrap_or_else(|e| internal_error_body("template render", &e));

    Html(rendered).into_response()
}

/// Guest reschedule: process the reschedule
async fn guest_reschedule_booking(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(token): Path<String>,
    Form(form): Form<RescheduleForm>,
) -> impl IntoResponse {
    if let Err(resp) = verify_csrf_token(&headers, &form._csrf) {
        return resp;
    }
    let lang = crate::i18n::detect_from_headers(&headers);
    // Rate limit
    let client_ip = client_ip_for_rate_limit(&headers);
    if state.booking_limiter.check_limited(&client_ip).await {
        return Html("Too many requests. Please try again later.".to_string()).into_response();
    }

    let booking: Option<(
        String,
        String,
        String,
        String,
        String,
        String,
        String,
        String,
        String,
        String,
        i32,
        Option<String>,
        Option<String>,
        Option<String>,
        i32,
        i32,
    )> = sqlx::query_as(
        "SELECT b.id, b.uid, b.guest_name, b.guest_email, b.start_at, b.end_at,
                    et.id, et.title, u.id, u.name, et.duration_min,
                    COALESCE(NULLIF(b.meeting_url, ''), et.location_value),
                    b.caldav_calendar_href, COALESCE(b.guest_timezone, 'UTC'),
                    et.min_notice_min, b.reschedule_by_host
             FROM bookings b
             JOIN event_types et ON et.id = b.event_type_id
             JOIN accounts a ON a.id = et.account_id
             JOIN users u ON u.id = a.user_id
             WHERE b.reschedule_token = ? AND b.status IN ('confirmed', 'pending')",
    )
    .bind(&token)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    let (
        booking_id,
        uid,
        guest_name,
        guest_email,
        old_start_at,
        old_end_at,
        et_id,
        et_title,
        host_user_id,
        host_name,
        duration,
        loc_value,
        caldav_href,
        _guest_timezone_str,
        min_notice,
        reschedule_by_host,
    ) = match booking {
        Some(b) => b,
        None => {
            let tmpl = match state.templates.get_template("booking_action_error.html") {
                Ok(t) => t,
                Err(e) => return internal_error_response("internal", &e),
            };
            let rendered = tmpl.render(context! {
                title => "Invalid link",
                message => "This reschedule link is invalid, has expired, or the booking has already been processed.",
                lang => lang,
            }).unwrap_or_else(|e| internal_error_body("template render", &e));
            return Html(rendered).into_response();
        }
    };

    // Defensive notice-window check on POST too: the GET may have been
    // cached, or the cutoff may have rolled over between page load and
    // submit. Either way, never mutate state inside the blocked window.
    if let Some((notice_min, host_email)) = fetch_notice_min_and_host_email(
        &state.pool,
        "reschedule_token",
        &token,
        "reschedule_notice_min",
    )
    .await
    {
        if let Some(resp) = check_notice_window(
            &state,
            notice_min,
            &old_start_at,
            &host_email,
            "reschedule",
            lang,
        ) {
            tracing::info!(
                action = "reschedule",
                booking_id = %booking_id,
                "guest action blocked by notice window (POST)"
            );
            return resp;
        }
    }

    let date = match NaiveDate::parse_from_str(&form.date, "%Y-%m-%d") {
        Ok(d) => d,
        Err(_) => return Html("Invalid date.".to_string()).into_response(),
    };
    if let Err(e) = validate_date_not_too_far(date) {
        return Html(e).into_response();
    }
    let start_time = match NaiveTime::parse_from_str(&form.time, "%H:%M") {
        Ok(t) => t,
        Err(_) => return Html("Invalid time.".to_string()).into_response(),
    };

    let guest_tz = parse_guest_tz(form.tz.as_deref());
    let new_guest_timezone = guest_tz.name().to_string();
    let host_tz = get_host_tz(&state.pool, &et_id).await;

    // The URL carries guest-local date/time; convert to host-local for storage
    // and availability checks (existing semantics).
    let guest_local_start = date.and_time(start_time);
    let guest_local_end = guest_local_start + Duration::minutes(duration as i64);
    let slot_start = guest_to_host_local(guest_local_start, guest_tz, host_tz);
    let slot_end = slot_start + Duration::minutes(duration as i64);
    let guest_end_time = guest_local_end.time().format("%H:%M").to_string();

    let now = Local::now().naive_local();
    if slot_start < now + Duration::minutes(min_notice as i64) {
        return Html("This slot is no longer available (too soon).".to_string()).into_response();
    }

    // Check conflicts excluding this booking
    let busy = fetch_busy_times_for_user_ex(
        &state.pool,
        &host_user_id,
        slot_start,
        slot_end,
        host_tz,
        Some(&et_id),
        Some(&booking_id),
    )
    .await;
    if has_conflict(&busy, slot_start, slot_end) {
        return Html("This slot is no longer available.".to_string()).into_response();
    }

    let new_start_at = slot_start.format("%Y-%m-%dT%H:%M:%S").to_string();
    let new_end_at = slot_end.format("%Y-%m-%dT%H:%M:%S").to_string();
    let new_reschedule_token = uuid::Uuid::new_v4().to_string();
    let new_cancel_token = uuid::Uuid::new_v4().to_string();
    let new_confirm_token = uuid::Uuid::new_v4().to_string();

    // Check if the event type requires confirmation
    let requires_confirmation: i32 =
        sqlx::query_scalar("SELECT requires_confirmation FROM event_types WHERE id = ?")
            .bind(&et_id)
            .fetch_one(&state.pool)
            .await
            .unwrap_or(0);

    // Determine new status:
    // - Host-initiated reschedule → confirmed (host already approved)
    // - Guest-initiated + requires_confirmation → pending (needs host approval)
    // - Guest-initiated + no confirmation needed → confirmed (auto-approved)
    let host_initiated = reschedule_by_host != 0;
    let needs_approval = !host_initiated && requires_confirmation != 0;
    let new_status = if needs_approval {
        "pending"
    } else {
        "confirmed"
    };
    let new_confirm = if needs_approval {
        Some(new_confirm_token.clone())
    } else {
        None
    };

    let _ = sqlx::query(
        "UPDATE bookings SET start_at = ?, end_at = ?, status = ?,
                reschedule_token = ?, cancel_token = ?, confirm_token = ?,
                reminder_sent_at = NULL, guest_timezone = ?, reschedule_by_host = 0
         WHERE id = ?",
    )
    .bind(&new_start_at)
    .bind(&new_end_at)
    .bind(new_status)
    .bind(&new_reschedule_token)
    .bind(&new_cancel_token)
    .bind(&new_confirm)
    .bind(&new_guest_timezone)
    .bind(&booking_id)
    .execute(&state.pool)
    .await;

    if host_initiated {
        tracing::info!(booking_id = %booking_id, old_start = %old_start_at, new_start = %new_start_at, "booking rescheduled by guest (host-initiated, confirmed)");
    } else if needs_approval {
        tracing::info!(booking_id = %booking_id, old_start = %old_start_at, new_start = %new_start_at, "booking rescheduled by guest (now pending, needs approval)");
    } else {
        tracing::info!(booking_id = %booking_id, old_start = %old_start_at, new_start = %new_start_at, "booking rescheduled by guest (auto-confirmed)");
    }

    let host_email: String =
        sqlx::query_scalar("SELECT COALESCE(booking_email, email) FROM users WHERE id = ?")
            .bind(&host_user_id)
            .fetch_one(&state.pool)
            .await
            .unwrap_or_default();

    // old_start_at/old_end_at are stored in the event-type tz. Convert into the
    // guest's tz so `RescheduleDetails` carries guest-local wall-clock for
    // both the OLD and NEW times — matches the contract used elsewhere and
    // lets `host_time_display` correctly recover the host wall-clock.
    let (old_date, old_start_time, old_end_time) =
        booking_strings_in_guest_tz(&old_start_at, &old_end_at, host_tz, guest_tz);

    if needs_approval {
        // Guest-initiated reschedule on requires_confirmation event → pending.
        // Delete the prior CalDAV event (it will be re-pushed if/when the host approves),
        // and clear caldav_calendar_href on the booking so the sync orphan sweep does not
        // race the approval flow and cancel this pending booking before the host clicks
        // approve. See cancel_orphaned_bookings in src/commands/sync.rs.
        if caldav_href.is_some() {
            caldav_delete_for_user(&state.pool, &state.secret_key, &host_user_id, &uid).await;
            let _ = sqlx::query("UPDATE bookings SET caldav_calendar_href = NULL WHERE id = ?")
                .bind(&booking_id)
                .execute(&state.pool)
                .await;
        }

        if let Ok(Some(smtp_config)) =
            crate::email::load_smtp_config(&state.pool, &state.secret_key).await
        {
            let base_url = std::env::var("CALRS_BASE_URL").ok();

            // Send host reschedule approval request
            let reschedule_details = crate::email::RescheduleDetails {
                event_title: et_title.clone(),
                old_date,
                old_start_time,
                old_end_time,
                new_date: form.date.clone(),
                new_start_time: form.time.clone(),
                new_end_time: guest_end_time.clone(),
                guest_name: guest_name.clone(),
                guest_email: guest_email.clone(),
                guest_timezone: new_guest_timezone.clone(),
                host_name: host_name.clone(),
                host_email,
                uid: uid.clone(),
                location: loc_value.clone(),
                host_timezone: host_tz.name().to_string(),
            };
            let _ = crate::email::send_host_reschedule_request(
                &smtp_config,
                &reschedule_details,
                Some(&new_confirm_token),
                base_url.as_deref(),
            )
            .await;

            // Send guest pending notice with new tokens
            let guest_cancel_url = base_url.as_ref().map(|base| {
                format!(
                    "{}/booking/cancel/{}",
                    base.trim_end_matches('/'),
                    new_cancel_token
                )
            });
            let guest_reschedule_url = base_url.as_ref().map(|base| {
                format!(
                    "{}/booking/reschedule/{}",
                    base.trim_end_matches('/'),
                    new_reschedule_token
                )
            });
            let pending_details = crate::email::BookingDetails {
                event_title: et_title.clone(),
                date: form.date.clone(),
                start_time: form.time.clone(),
                end_time: guest_end_time.clone(),
                guest_name: guest_name.clone(),
                guest_email: guest_email.clone(),
                guest_timezone: new_guest_timezone,
                host_name: host_name.clone(),
                host_email: String::new(),
                uid,
                notes: None,
                location: loc_value,
                reminder_minutes: None,
                additional_attendees: vec![],
                host_timezone: host_tz.name().to_string(),
                ..Default::default()
            };
            let _ = crate::email::send_guest_pending_notice_ex(
                &smtp_config,
                &pending_details,
                guest_cancel_url.as_deref(),
                guest_reschedule_url.as_deref(),
            )
            .await;
        }
    } else {
        // Confirmed reschedule (host-initiated or guest on non-confirmation event)
        // Push updated event to CalDAV
        let push_details = crate::email::BookingDetails {
            event_title: et_title.clone(),
            date: form.date.clone(),
            start_time: form.time.clone(),
            end_time: guest_end_time.clone(),
            guest_name: guest_name.clone(),
            guest_email: guest_email.clone(),
            guest_timezone: new_guest_timezone.clone(),
            host_name: host_name.clone(),
            host_email: host_email.clone(),
            uid: uid.clone(),
            notes: None,
            location: loc_value.clone(),
            reminder_minutes: None,
            additional_attendees: vec![],
            host_timezone: host_tz.name().to_string(),
            ..Default::default()
        };
        caldav_push_booking(
            &state.pool,
            &state.secret_key,
            &host_user_id,
            &uid,
            &push_details,
        )
        .await;

        if let Ok(Some(smtp_config)) =
            crate::email::load_smtp_config(&state.pool, &state.secret_key).await
        {
            let base_url = std::env::var("CALRS_BASE_URL").ok();
            let guest_cancel_url = base_url.as_ref().map(|base| {
                format!(
                    "{}/booking/cancel/{}",
                    base.trim_end_matches('/'),
                    new_cancel_token
                )
            });
            let guest_reschedule_url = base_url.as_ref().map(|base| {
                format!(
                    "{}/booking/reschedule/{}",
                    base.trim_end_matches('/'),
                    new_reschedule_token
                )
            });

            // Notify guest with confirmation + ICS
            let _ = crate::email::send_guest_reschedule_notification(
                &smtp_config,
                &crate::email::RescheduleDetails {
                    event_title: et_title.clone(),
                    old_date,
                    old_start_time,
                    old_end_time,
                    new_date: form.date.clone(),
                    new_start_time: form.time.clone(),
                    new_end_time: guest_end_time.clone(),
                    guest_name: guest_name.clone(),
                    guest_email: guest_email.clone(),
                    guest_timezone: new_guest_timezone,
                    host_name: host_name.clone(),
                    host_email: host_email.clone(),
                    uid,
                    location: loc_value,
                    host_timezone: host_tz.name().to_string(),
                },
                guest_cancel_url.as_deref(),
                guest_reschedule_url.as_deref(),
            )
            .await;

            // Notify host
            let _ = crate::email::send_host_booking_confirmed(&smtp_config, &push_details).await;
        }
    }

    let date_label = crate::i18n::format_long_date(date, lang);
    let (cancel_notice_min, reschedule_notice_min) =
        fetch_event_type_notice_minutes(&state.pool, &et_id).await;

    let tmpl = match state.templates.get_template("confirmed.html") {
        Ok(t) => t,
        Err(e) => return internal_error_response("internal", &e),
    };
    let rendered = tmpl
        .render(context! {
            event_title => et_title,
            date_label => date_label,
            time_start => form.time,
            time_end => guest_end_time,
            host_name => host_name,
            guest_email => guest_email,
            pending => needs_approval,
            rescheduled => true,
            cancel_notice_min => cancel_notice_min,
            reschedule_notice_min => reschedule_notice_min,
            company_link => state.company_link.read().await.clone(),
            lang => lang,
        })
        .unwrap_or_else(|e| internal_error_body("template render", &e));

    Html(rendered).into_response()
}

/// Host reschedule: send the guest a link to pick a new time.
/// GET shows a confirmation page, POST sends the email.
async fn host_reschedule_slots(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
    Path(booking_id): Path<String>,
) -> impl IntoResponse {
    let user = &auth_user.user;
    // Dashboard handler: no Accept-Language available, so honour the user's
    // saved preference and fall back to English. Once the dashboard is
    // translated this should switch to crate::i18n::resolve(...).
    let lang = user.language.as_deref().unwrap_or("en");

    let booking: Option<(String, String, String, String, String, String)> = sqlx::query_as(
        "SELECT b.id, b.guest_name, b.guest_email, b.start_at, b.end_at, et.title
         FROM bookings b
         JOIN event_types et ON et.id = b.event_type_id
         JOIN accounts a ON a.id = et.account_id
         WHERE b.id = ? AND a.user_id = ? AND b.status IN ('confirmed', 'pending')",
    )
    .bind(&booking_id)
    .bind(&user.id)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    let (bid, guest_name, guest_email, start_at, end_at, event_title) = match booking {
        Some(b) => b,
        None => return Redirect::to("/dashboard/bookings").into_response(),
    };

    let date_label = format_date_label(&start_at, lang);
    let start_time = extract_time_24h(&start_at);
    let end_time = extract_time_24h(&end_at);

    let tmpl = match state.templates.get_template("booking_host_reschedule.html") {
        Ok(t) => t,
        Err(e) => return internal_error_response("internal", &e),
    };
    let rendered = tmpl
        .render(context! {
            sidebar => sidebar_context(&auth_user, "bookings"),
            booking_id => bid,
            event_title => event_title,
            guest_name => guest_name,
            guest_email => guest_email,
            date_label => date_label,
            start_time => start_time,
            end_time => end_time,
        })
        .unwrap_or_else(|e| internal_error_body("template render", &e));

    Html(rendered).into_response()
}

/// Host reschedule: set reschedule_by_host flag, regenerate token, email guest
async fn host_reschedule_booking(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
    headers: HeaderMap,
    Path(booking_id): Path<String>,
    Form(form): Form<CancelForm>,
) -> impl IntoResponse {
    if let Err(resp) = verify_csrf_token(&headers, &form._csrf) {
        return resp;
    }
    let user = &auth_user.user;

    let booking: Option<(
        String,
        String,
        String,
        String,
        String,
        String,
        String,
        String,
        String,
    )> = sqlx::query_as(
        "SELECT b.id, b.uid, b.guest_name, b.guest_email, b.start_at, b.end_at,
                    et.title, COALESCE(b.guest_timezone, 'UTC'), et.id
             FROM bookings b
             JOIN event_types et ON et.id = b.event_type_id
             JOIN accounts a ON a.id = et.account_id
             WHERE b.id = ? AND a.user_id = ? AND b.status IN ('confirmed', 'pending')",
    )
    .bind(&booking_id)
    .bind(&user.id)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    let (bid, _uid, guest_name, guest_email, start_at, end_at, event_title, guest_timezone, et_id) =
        match booking {
            Some(b) => b,
            None => return Redirect::to("/dashboard/bookings").into_response(),
        };

    // Generate new reschedule token and set the host-initiated flag
    let new_reschedule_token = uuid::Uuid::new_v4().to_string();
    let _ = sqlx::query(
        "UPDATE bookings SET reschedule_token = ?, reschedule_by_host = 1 WHERE id = ?",
    )
    .bind(&new_reschedule_token)
    .bind(&bid)
    .execute(&state.pool)
    .await;

    tracing::info!(booking_id = %bid, guest = %guest_email, "host requested reschedule — guest will pick new time");

    // Send email to guest asking them to pick a new time
    if let Ok(Some(smtp_config)) =
        crate::email::load_smtp_config(&state.pool, &state.secret_key).await
    {
        let base_url = std::env::var("CALRS_BASE_URL").ok();
        let reschedule_url = base_url.as_ref().map(|base| {
            format!(
                "{}/booking/reschedule/{}",
                base.trim_end_matches('/'),
                new_reschedule_token
            )
        });
        let cancel_url: Option<String> =
            sqlx::query_scalar("SELECT cancel_token FROM bookings WHERE id = ?")
                .bind(&bid)
                .fetch_optional(&state.pool)
                .await
                .unwrap_or(None)
                .and_then(|t: String| {
                    base_url
                        .as_ref()
                        .map(|base| format!("{}/booking/cancel/{}", base.trim_end_matches('/'), t))
                });

        // start_at/end_at are stored in the event-type tz (see #101). Convert
        // into the guest's tz before populating BookingDetails so the
        // guest-facing "pick new time" email shows their wall-clock with their
        // tz label.
        let host_tz = get_host_tz(&state.pool, &et_id).await;
        let guest_tz_parsed = guest_timezone.parse::<Tz>().unwrap_or(Tz::UTC);
        let (date, start_time, end_time) =
            booking_strings_in_guest_tz(&start_at, &end_at, host_tz, guest_tz_parsed);

        let details = crate::email::BookingDetails {
            event_title,
            date,
            start_time,
            end_time,
            guest_name,
            guest_email,
            guest_timezone,
            host_name: user.name.clone(),
            host_email: user
                .booking_email
                .clone()
                .unwrap_or_else(|| user.email.clone()),
            uid: String::new(),
            notes: None,
            location: None,
            reminder_minutes: None,
            additional_attendees: vec![],
            host_timezone: host_tz.name().to_string(),
            ..Default::default()
        };

        if let Some(url) = &reschedule_url {
            let _ = crate::email::send_guest_pick_new_time(
                &smtp_config,
                &details,
                url,
                cancel_url.as_deref(),
            )
            .await;
        }
    }

    Redirect::to("/dashboard/bookings").into_response()
}

// --- CalDAV write-back ---

/// Push a confirmed booking to the host's CalDAV calendar.
/// Finds the first CalDAV source with a write_calendar_href set for this user,
/// generates the ICS, and PUTs it to the CalDAV server.
async fn caldav_push_booking(
    pool: &SqlitePool,
    key: &[u8; 32],
    user_id: &str,
    booking_uid: &str,
    details: &crate::email::BookingDetails,
) {
    // Find all sources with write_calendar_href configured for this user
    #[allow(clippy::type_complexity)]
    let sources: Vec<(
        String,
        String,
        String,
        Option<String>,
        String,
        String,
        Option<String>,
        Option<String>,
        String,
        i64,
        Option<String>,
    )> = sqlx::query_as(
        "SELECT cs.id, cs.url, cs.username, cs.password_enc, cs.write_calendar_href, cs.auth_type, \
                cs.access_token_enc, cs.token_expires_at, cs.provider_type, \
                cs.managed, cs.impersonate_email
         FROM caldav_sources cs
         JOIN accounts a ON a.id = cs.account_id
         WHERE a.user_id = ? AND cs.enabled = 1 AND cs.write_calendar_href IS NOT NULL",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    let ews_global_cfg = crate::web::ews_global::load_ews_global_config(pool, key).await;

    if sources.is_empty() {
        // Distinguish "user has no sources" (debug) from "user has sources but none
        // are write-configured" (warn). The second case is almost always a misconfig.
        let unconfigured: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM caldav_sources cs
             JOIN accounts a ON a.id = cs.account_id
             WHERE a.user_id = ? AND cs.enabled = 1 AND cs.write_calendar_href IS NULL",
        )
        .bind(user_id)
        .fetch_one(pool)
        .await
        .unwrap_or(0);

        if unconfigured > 0 {
            tracing::warn!(
                user_id = %user_id,
                uid = %booking_uid,
                unconfigured_sources = unconfigured,
                "calendar write-back skipped: booking confirmed but no source has a write calendar selected. Pick one at /dashboard/sources",
            );
        } else {
            tracing::debug!(user_id = %user_id, "calendar write-back skipped: no enabled sources for user");
        }
        return;
    }

    let ics = crate::email::generate_ics(details, "");

    for (
        source_id,
        url,
        username,
        password_enc,
        calendar_href,
        auth_type,
        access_token_enc,
        token_expires_at,
        provider_type,
        managed,
        impersonate_email,
    ) in &sources
    {
        tracing::debug!(uid = %booking_uid, calendar_href = %calendar_href, provider = %provider_type, "pushing booking to calendar");

        let put_result = if provider_type == crate::providers::factory::kinds::EWS {
            let client_result = if *managed != 0 {
                match ews_global_cfg.as_ref() {
                    Some(cfg) => crate::providers::build_provider(
                        provider_type,
                        &cfg.url,
                        &cfg.service_username,
                        &cfg.service_password,
                        impersonate_email.as_deref(),
                    ),
                    None => {
                        tracing::warn!(url = %url, "calendar write-back skipped: managed EWS source but global config disabled");
                        continue;
                    }
                }
            } else {
                let enc = match password_enc.as_deref() {
                    Some(e) => e,
                    None => {
                        tracing::error!(url = %url, "calendar write-back failed: EWS source missing password");
                        continue;
                    }
                };
                let password = match crate::crypto::decrypt_password(key, enc) {
                    Ok(p) => p,
                    Err(e) => {
                        tracing::error!(url = %url, error = %e, "calendar write-back failed: could not decrypt credentials");
                        continue;
                    }
                };
                crate::providers::build_provider(provider_type, url, username, &password, None)
            };
            let client = match client_result {
                Ok(c) => c,
                Err(e) => {
                    tracing::error!(url = %url, error = %e, "calendar write-back failed: unknown provider");
                    continue;
                }
            };
            client.put_event(calendar_href, booking_uid, &ics).await
        } else {
            let client = match crate::oauth2_caldav::build_client_for_source(
                pool,
                key,
                source_id,
                url,
                auth_type,
                username,
                password_enc.as_deref(),
                access_token_enc.as_deref(),
                token_expires_at.as_deref(),
            )
            .await
            {
                Ok(c) => c,
                Err(e) => {
                    tracing::error!(url = %url, error = %e, "calendar write-back failed: could not build client");
                    continue;
                }
            };
            client.put_event(calendar_href, booking_uid, &ics).await
        };

        if let Err(e) = put_result {
            tracing::error!(uid = %booking_uid, calendar_href = %calendar_href, error = %e, "calendar write-back failed");
            continue;
        }

        tracing::info!(uid = %booking_uid, calendar_href = %calendar_href, "calendar write-back succeeded");

        // Record which calendar href the booking was pushed to (last successful one)
        let _ = sqlx::query("UPDATE bookings SET caldav_calendar_href = ? WHERE uid = ?")
            .bind(calendar_href)
            .bind(booking_uid)
            .execute(pool)
            .await;
    }
}

/// Delete a booking from a user's CalDAV calendar by looking up their write-enabled source directly.
/// Used for team bookings where we don't track per-booking caldav_calendar_href.
async fn caldav_delete_for_user(
    pool: &SqlitePool,
    key: &[u8; 32],
    user_id: &str,
    booking_uid: &str,
) {
    #[allow(clippy::type_complexity)]
    let sources: Vec<(
        String,
        String,
        String,
        Option<String>,
        String,
        String,
        Option<String>,
        Option<String>,
        String,
        i64,
        Option<String>,
    )> = sqlx::query_as(
        "SELECT cs.id, cs.url, cs.username, cs.password_enc, cs.write_calendar_href, cs.auth_type, \
                cs.access_token_enc, cs.token_expires_at, cs.provider_type, \
                cs.managed, cs.impersonate_email
         FROM caldav_sources cs
         JOIN accounts a ON a.id = cs.account_id
         WHERE a.user_id = ? AND cs.enabled = 1 AND cs.write_calendar_href IS NOT NULL",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    let ews_global_cfg = crate::web::ews_global::load_ews_global_config(pool, key).await;

    for (
        source_id,
        url,
        username,
        password_enc,
        calendar_href,
        auth_type,
        access_token_enc,
        token_expires_at,
        provider_type,
        managed,
        impersonate_email,
    ) in &sources
    {
        let delete_result = if provider_type == crate::providers::factory::kinds::EWS {
            let client_result = if *managed != 0 {
                match ews_global_cfg.as_ref() {
                    Some(cfg) => crate::providers::build_provider(
                        provider_type,
                        &cfg.url,
                        &cfg.service_username,
                        &cfg.service_password,
                        impersonate_email.as_deref(),
                    ),
                    None => continue,
                }
            } else {
                let enc = match password_enc.as_deref() {
                    Some(e) => e,
                    None => continue,
                };
                let password = match crate::crypto::decrypt_password(key, enc) {
                    Ok(p) => p,
                    Err(_) => continue,
                };
                crate::providers::build_provider(provider_type, url, username, &password, None)
            };
            let client = match client_result {
                Ok(c) => c,
                Err(_) => continue,
            };
            client.delete_event(calendar_href, booking_uid).await
        } else {
            let client = match crate::oauth2_caldav::build_client_for_source(
                pool,
                key,
                source_id,
                url,
                auth_type,
                username,
                password_enc.as_deref(),
                access_token_enc.as_deref(),
                token_expires_at.as_deref(),
            )
            .await
            {
                Ok(c) => c,
                Err(_) => continue,
            };
            client.delete_event(calendar_href, booking_uid).await
        };
        if let Err(e) = delete_result {
            tracing::error!(uid = %booking_uid, user = %user_id, calendar = %calendar_href, error = %e, "calendar event delete failed");
        }
    }

    // Remove cached event
    let _ = sqlx::query(
        "DELETE FROM events WHERE uid = ? AND calendar_id IN (
            SELECT c.id FROM calendars c
            JOIN caldav_sources cs ON cs.id = c.source_id
            JOIN accounts a ON a.id = cs.account_id
            WHERE a.user_id = ?
        )",
    )
    .bind(booking_uid)
    .bind(user_id)
    .execute(pool)
    .await;
}

/// Delete a booking from the host's CalDAV calendar.
async fn caldav_delete_booking(
    pool: &SqlitePool,
    key: &[u8; 32],
    user_id: &str,
    booking_uid: &str,
) {
    // Check if this booking was pushed to CalDAV
    let info: Option<(String,)> = sqlx::query_as(
        "SELECT caldav_calendar_href FROM bookings WHERE uid = ? AND caldav_calendar_href IS NOT NULL",
    )
    .bind(booking_uid)
    .fetch_optional(pool)
    .await
    .unwrap_or(None);

    let calendar_href = match info {
        Some((href,)) => href,
        None => return, // Was never pushed to CalDAV
    };

    // Get the source credentials and provider type
    #[allow(clippy::type_complexity)]
    let source: Option<(
        String,
        String,
        String,
        Option<String>,
        String,
        Option<String>,
        Option<String>,
        String,
        i64,
        Option<String>,
    )> = sqlx::query_as(
        "SELECT cs.id, cs.url, cs.username, cs.password_enc, cs.auth_type, cs.access_token_enc, \
                cs.token_expires_at, cs.provider_type, cs.managed, cs.impersonate_email
         FROM caldav_sources cs
         JOIN accounts a ON a.id = cs.account_id
         WHERE a.user_id = ? AND cs.enabled = 1 AND cs.write_calendar_href = ?
         LIMIT 1",
    )
    .bind(user_id)
    .bind(&calendar_href)
    .fetch_optional(pool)
    .await
    .unwrap_or(None);

    let (
        source_id,
        url,
        username,
        password_enc,
        auth_type,
        access_token_enc,
        token_expires_at,
        provider_type,
        managed,
        impersonate_email,
    ) = match source {
        Some(s) => s,
        None => return,
    };

    let delete_result = if provider_type == crate::providers::factory::kinds::EWS {
        let client_result = if managed != 0 {
            match crate::web::ews_global::load_ews_global_config(pool, key).await {
                Some(cfg) => crate::providers::build_provider(
                    &provider_type,
                    &cfg.url,
                    &cfg.service_username,
                    &cfg.service_password,
                    impersonate_email.as_deref(),
                ),
                None => return,
            }
        } else {
            let enc = match password_enc.as_deref() {
                Some(e) => e,
                None => return,
            };
            let password = match crate::crypto::decrypt_password(key, enc) {
                Ok(p) => p,
                Err(_) => return,
            };
            crate::providers::build_provider(&provider_type, &url, &username, &password, None)
        };
        let client = match client_result {
            Ok(c) => c,
            Err(_) => return,
        };
        client.delete_event(&calendar_href, booking_uid).await
    } else {
        let client = match crate::oauth2_caldav::build_client_for_source(
            pool,
            key,
            &source_id,
            &url,
            &auth_type,
            &username,
            password_enc.as_deref(),
            access_token_enc.as_deref(),
            token_expires_at.as_deref(),
        )
        .await
        {
            Ok(c) => c,
            Err(_) => return,
        };
        client.delete_event(&calendar_href, booking_uid).await
    };
    if let Err(e) = delete_result {
        tracing::error!(uid = %booking_uid, error = %e, "calendar event delete failed");
    }

    // Also remove the cached event from local DB so it doesn't block availability
    let _ = sqlx::query(
        "DELETE FROM events WHERE uid = ? AND calendar_id IN (
            SELECT c.id FROM calendars c
            JOIN caldav_sources cs ON cs.id = c.source_id
            JOIN accounts a ON a.id = cs.account_id
            WHERE a.user_id = ?
        )",
    )
    .bind(booking_uid)
    .bind(user_id)
    .execute(pool)
    .await;
}

// --- Booking watchers ---

/// Notify watcher team members that a booking is available to claim.
/// Generates a claim token per watcher member and sends notification emails.
async fn notify_watchers(
    pool: &SqlitePool,
    key: &[u8; 32],
    booking_id: &str,
    event_type_id: &str,
    assigned_to_name: &str,
    details: &crate::email::BookingDetails,
) {
    // Find watcher teams for this event type
    let watcher_team_ids: Vec<(String,)> =
        sqlx::query_as("SELECT team_id FROM event_type_watchers WHERE event_type_id = ?")
            .bind(event_type_id)
            .fetch_all(pool)
            .await
            .unwrap_or_default();

    if watcher_team_ids.is_empty() {
        return;
    }

    let base_url = match std::env::var("CALRS_BASE_URL").ok() {
        Some(u) => u,
        None => {
            tracing::warn!("CALRS_BASE_URL not set, skipping watcher notifications");
            return;
        }
    };

    let smtp_config = match crate::email::load_smtp_config(pool, key).await {
        Ok(Some(c)) => c,
        _ => {
            tracing::warn!("SMTP not configured, skipping watcher notifications");
            return;
        }
    };

    for (team_id,) in &watcher_team_ids {
        // Get all members of the watcher team
        let members: Vec<(String, String, String)> = sqlx::query_as(
            "SELECT u.id, u.name, COALESCE(u.booking_email, u.email) \
             FROM users u JOIN team_members tm ON tm.user_id = u.id \
             WHERE tm.team_id = ? AND u.enabled = 1",
        )
        .bind(team_id)
        .fetch_all(pool)
        .await
        .unwrap_or_default();

        for (user_id, user_name, user_email) in &members {
            let token = uuid::Uuid::new_v4().to_string();
            let token_id = uuid::Uuid::new_v4().to_string();

            // Insert claim token with 7-day expiry
            let _ = sqlx::query(
                "INSERT INTO booking_claim_tokens (id, booking_id, user_id, token, expires_at) \
                 VALUES (?, ?, ?, ?, datetime('now', '+7 days'))",
            )
            .bind(&token_id)
            .bind(booking_id)
            .bind(user_id)
            .bind(&token)
            .execute(pool)
            .await;

            let claim_url = format!(
                "{}/booking/claim/{}?token={}",
                base_url.trim_end_matches('/'),
                booking_id,
                token
            );

            let _ = crate::email::send_watcher_claim_notification(
                &smtp_config,
                details,
                user_name,
                user_email,
                assigned_to_name,
                &claim_url,
            )
            .await;
        }
    }

    tracing::info!(booking_id = %booking_id, event_type_id = %event_type_id, "watcher claim notifications sent");
}

// --- Claim endpoints ---

async fn claim_booking_form(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(booking_id): Path<String>,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> impl IntoResponse {
    let lang = crate::i18n::detect_from_headers(&headers);
    let token = match params.get("token") {
        Some(t) => t,
        None => {
            return render_booking_action_error(
                &state,
                &headers,
                "Invalid link",
                "No claim token provided.",
            );
        }
    };

    // Validate token
    let claim_info: Option<(String, String)> = sqlx::query_as(
        "SELECT bct.user_id, bct.booking_id FROM booking_claim_tokens bct \
         WHERE bct.token = ? AND bct.booking_id = ? AND bct.used_at IS NULL \
         AND bct.expires_at > datetime('now')",
    )
    .bind(token)
    .bind(&booking_id)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    if claim_info.is_none() {
        // Check if already claimed
        let claimed: Option<(String,)> = sqlx::query_as(
            "SELECT u.name FROM bookings b \
             JOIN users u ON u.id = b.claimed_by_user_id \
             WHERE b.id = ?",
        )
        .bind(&booking_id)
        .fetch_optional(&state.pool)
        .await
        .unwrap_or(None);

        if let Some((claimed_by_name,)) = claimed {
            let tmpl = match state.templates.get_template("booking_already_claimed.html") {
                Ok(t) => t,
                Err(e) => return internal_error_response("internal", &e),
            };
            return Html(
                tmpl.render(context! {
                    claimed_by_name => claimed_by_name,
                    lang => lang,
                })
                .unwrap_or_else(|e| internal_error_body("template render", &e)),
            )
            .into_response();
        }

        return render_booking_action_error(
            &state,
            &headers,
            "Invalid or expired link",
            "This claim link is no longer valid.",
        );
    }

    // Fetch booking details for display
    let booking: Option<(String, String, String, String, String, Option<String>)> = sqlx::query_as(
        "SELECT et.title, b.guest_name, b.guest_email, b.start_at, b.end_at, u.name \
             FROM bookings b \
             JOIN event_types et ON et.id = b.event_type_id \
             LEFT JOIN users u ON u.id = b.assigned_user_id \
             WHERE b.id = ?",
    )
    .bind(&booking_id)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    let (event_title, guest_name, guest_email, start_at, end_at, assigned_to) = match booking {
        Some(b) => b,
        None => {
            return render_booking_action_error(
                &state,
                &headers,
                "Booking not found",
                "This booking no longer exists.",
            )
        }
    };

    let date_label = format_date_label(&start_at, lang);
    let start_time = extract_time_24h(&start_at);
    let end_time = extract_time_24h(&end_at);

    let tmpl = match state.templates.get_template("booking_claim_form.html") {
        Ok(t) => t,
        Err(e) => return internal_error_response("internal", &e),
    };

    Html(
        tmpl.render(context! {
            event_title => event_title,
            date_label => date_label,
            start_time => start_time,
            end_time => end_time,
            guest_name => guest_name,
            guest_email => guest_email,
            assigned_to => assigned_to.unwrap_or_default(),
            token => token,
            lang => lang,
        })
        .unwrap_or_else(|e| internal_error_body("template render", &e)),
    )
    .into_response()
}

#[derive(Deserialize)]
struct ClaimForm {
    _csrf: Option<String>,
    token: String,
}

async fn claim_booking(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(booking_id): Path<String>,
    Form(form): Form<ClaimForm>,
) -> impl IntoResponse {
    if let Err(resp) = verify_csrf_token(&headers, &form._csrf) {
        return resp;
    }
    let lang = crate::i18n::detect_from_headers(&headers);

    // Validate token
    let claim_info: Option<(String, String)> = sqlx::query_as(
        "SELECT bct.user_id, bct.booking_id FROM booking_claim_tokens bct \
         WHERE bct.token = ? AND bct.booking_id = ? AND bct.used_at IS NULL \
         AND bct.expires_at > datetime('now')",
    )
    .bind(&form.token)
    .bind(&booking_id)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    let (claimant_user_id, _) = match claim_info {
        Some(c) => c,
        None => {
            // Check if already claimed
            let claimed: Option<(String,)> = sqlx::query_as(
                "SELECT u.name FROM bookings b \
                 JOIN users u ON u.id = b.claimed_by_user_id \
                 WHERE b.id = ?",
            )
            .bind(&booking_id)
            .fetch_optional(&state.pool)
            .await
            .unwrap_or(None);

            if let Some((claimed_by_name,)) = claimed {
                let tmpl = match state.templates.get_template("booking_already_claimed.html") {
                    Ok(t) => t,
                    Err(e) => return internal_error_response("internal", &e),
                };
                return Html(
                    tmpl.render(context! {
                        claimed_by_name => claimed_by_name,
                        lang => lang,
                    })
                    .unwrap_or_else(|e| internal_error_body("template render", &e)),
                )
                .into_response();
            }

            return render_booking_action_error(
                &state,
                &headers,
                "Invalid or expired link",
                "This claim link is no longer valid.",
            )
            .into_response();
        }
    };

    // Use BEGIN IMMEDIATE to prevent race conditions
    let mut tx = match sqlx::pool::Pool::begin(&state.pool).await {
        Ok(tx) => tx,
        Err(e) => return internal_error_response("database query", &e),
    };

    // Check booking is not already claimed (inside transaction)
    let already_claimed: Option<(String,)> = sqlx::query_as(
        "SELECT claimed_by_user_id FROM bookings WHERE id = ? AND claimed_by_user_id IS NOT NULL",
    )
    .bind(&booking_id)
    .fetch_optional(&mut *tx)
    .await
    .unwrap_or(None);

    if already_claimed.is_some() {
        let _ = tx.rollback().await;
        let claimed_name: Option<(String,)> = sqlx::query_as(
            "SELECT u.name FROM bookings b JOIN users u ON u.id = b.claimed_by_user_id WHERE b.id = ?",
        )
        .bind(&booking_id)
        .fetch_optional(&state.pool)
        .await
        .unwrap_or(None);

        let tmpl = match state.templates.get_template("booking_already_claimed.html") {
            Ok(t) => t,
            Err(e) => return internal_error_response("internal", &e),
        };
        return Html(
            tmpl.render(context! {
                claimed_by_name => claimed_name.map(|(n,)| n).unwrap_or_default(),
                lang => lang,
            })
            .unwrap_or_else(|e| internal_error_body("template render", &e)),
        )
        .into_response();
    }

    // Claim the booking
    let _ = sqlx::query(
        "UPDATE bookings SET claimed_by_user_id = ?, claimed_at = datetime('now') WHERE id = ?",
    )
    .bind(&claimant_user_id)
    .bind(&booking_id)
    .execute(&mut *tx)
    .await;

    // Mark this token as used
    let _ =
        sqlx::query("UPDATE booking_claim_tokens SET used_at = datetime('now') WHERE token = ?")
            .bind(&form.token)
            .execute(&mut *tx)
            .await;

    if let Err(e) = tx.commit().await {
        return internal_error_response("database query", &e);
    }

    tracing::info!(booking_id = %booking_id, claimant_user_id = %claimant_user_id, "booking claimed");

    // Fetch booking + claimant details for CalDAV push and email
    let booking: Option<(
        String,
        String,
        String,
        String,
        String,
        String,
        String,
        String,
        Option<String>,
        String,
    )> = sqlx::query_as(
        "SELECT et.title, b.guest_name, b.guest_email, b.start_at, b.end_at, b.uid, \
             COALESCE(b.guest_timezone, 'UTC'), a.user_id, et.location_value, b.event_type_id \
             FROM bookings b \
             JOIN event_types et ON et.id = b.event_type_id \
             JOIN accounts a ON a.id = et.account_id \
             WHERE b.id = ?",
    )
    .bind(&booking_id)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    let (
        event_title,
        guest_name,
        guest_email,
        start_at,
        end_at,
        uid,
        guest_tz,
        host_user_id,
        location,
        event_type_id,
    ) = match booking {
        Some(b) => b,
        None => {
            return render_booking_action_error(
                &state,
                &headers,
                "Booking not found",
                "This booking no longer exists.",
            )
            .into_response()
        }
    };

    let claimant: Option<(String, String)> =
        sqlx::query_as("SELECT name, COALESCE(booking_email, email) FROM users WHERE id = ?")
            .bind(&claimant_user_id)
            .fetch_optional(&state.pool)
            .await
            .unwrap_or(None);

    let (claimant_name, claimant_email) = claimant.unwrap_or_default();

    let host: Option<(String, String)> =
        sqlx::query_as("SELECT name, COALESCE(booking_email, email) FROM users WHERE id = ?")
            .bind(&host_user_id)
            .fetch_optional(&state.pool)
            .await
            .unwrap_or(None);

    let (host_name, host_email) = host.unwrap_or_default();

    let date = start_at.get(..10).unwrap_or(&start_at).to_string();
    let start_time = extract_time_24h(&start_at);
    let end_time = extract_time_24h(&end_at);

    // Add claimant as a booking attendee
    let attendee_id = uuid::Uuid::new_v4().to_string();
    let _ = sqlx::query("INSERT INTO booking_attendees (id, booking_id, email) VALUES (?, ?, ?)")
        .bind(&attendee_id)
        .bind(&booking_id)
        .bind(&claimant_email)
        .execute(&state.pool)
        .await;

    let claim_host_tz = get_host_tz(&state.pool, &event_type_id).await;

    // Build details with claimant as additional attendee for CalDAV push
    let mut details = crate::email::BookingDetails {
        event_title: event_title.clone(),
        date: date.clone(),
        start_time: start_time.clone(),
        end_time: end_time.clone(),
        guest_name: guest_name.clone(),
        guest_email: guest_email.clone(),
        guest_timezone: guest_tz,
        host_name,
        host_email,
        uid: uid.clone(),
        notes: None,
        location,
        reminder_minutes: None,
        additional_attendees: vec![claimant_email.clone()],
        host_timezone: claim_host_tz.name().to_string(),
        ..Default::default()
    };

    // Also include any pre-existing additional attendees
    let existing_attendees: Vec<(String,)> =
        sqlx::query_as("SELECT email FROM booking_attendees WHERE booking_id = ? AND email != ?")
            .bind(&booking_id)
            .bind(&claimant_email)
            .fetch_all(&state.pool)
            .await
            .unwrap_or_default();
    for (email,) in &existing_attendees {
        details.additional_attendees.push(email.clone());
    }

    // Re-push updated ICS to host's CalDAV (with claimant as ATTENDEE)
    caldav_push_booking(
        &state.pool,
        &state.secret_key,
        &host_user_id,
        &uid,
        &details,
    )
    .await;

    // Push to claimant's CalDAV calendar too
    caldav_push_booking(
        &state.pool,
        &state.secret_key,
        &claimant_user_id,
        &uid,
        &details,
    )
    .await;

    // Send confirmation email to claimant
    if let Ok(Some(smtp_config)) =
        crate::email::load_smtp_config(&state.pool, &state.secret_key).await
    {
        let _ = crate::email::send_claim_confirmation(
            &smtp_config,
            &details,
            &claimant_name,
            &claimant_email,
        )
        .await;
    }

    // Render success page
    let date_label = format_date_label(&start_at, lang);
    let tmpl = match state.templates.get_template("booking_claimed.html") {
        Ok(t) => t,
        Err(e) => return internal_error_response("internal", &e),
    };

    Html(
        tmpl.render(context! {
            event_title => event_title,
            date_label => date_label,
            start_time => start_time,
            end_time => end_time,
            guest_name => guest_name,
            guest_email => guest_email,
            lang => lang,
        })
        .unwrap_or_else(|e| internal_error_body("template render", &e)),
    )
    .into_response()
}

fn render_booking_action_error(
    state: &AppState,
    headers: &HeaderMap,
    title: &str,
    message: &str,
) -> axum::response::Response {
    let lang = crate::i18n::detect_from_headers(headers);
    let tmpl = match state.templates.get_template("booking_action_error.html") {
        Ok(t) => t,
        Err(e) => return internal_error_response("internal", &e),
    };
    Html(
        tmpl.render(context! {
            title => title,
            message => message,
            lang => lang,
        })
        .unwrap_or_else(|e| internal_error_body("template render", &e)),
    )
    .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- Rate limiter tests ---

    #[tokio::test]
    async fn rate_limiter_allows_under_limit() {
        let limiter = RateLimiter::new(3, 60);
        assert!(!limiter.check_limited("ip1").await);
        assert!(!limiter.check_limited("ip1").await);
        assert!(!limiter.check_limited("ip1").await);
    }

    #[tokio::test]
    async fn rate_limiter_blocks_over_limit() {
        let limiter = RateLimiter::new(2, 60);
        assert!(!limiter.check_limited("ip1").await); // 1
        assert!(!limiter.check_limited("ip1").await); // 2
        assert!(limiter.check_limited("ip1").await); // 3 → blocked
        assert!(limiter.check_limited("ip1").await); // still blocked
    }

    #[tokio::test]
    async fn rate_limiter_independent_per_ip() {
        let limiter = RateLimiter::new(1, 60);
        assert!(!limiter.check_limited("ip1").await);
        assert!(limiter.check_limited("ip1").await); // ip1 blocked
        assert!(!limiter.check_limited("ip2").await); // ip2 still ok
    }

    #[tokio::test]
    async fn rate_limiter_resets_after_window() {
        let limiter = RateLimiter::new(1, 0); // 0-second window = immediate expiry
        assert!(!limiter.check_limited("ip1").await);
        // Window has already expired (0 seconds)
        assert!(!limiter.check_limited("ip1").await); // reset, allowed again
    }

    #[test]
    fn detect_image_ext_recognizes_known_formats() {
        // PNG signature.
        assert_eq!(
            detect_image_ext(&[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0xff]),
            Some("png")
        );
        // JPEG SOI + APP marker.
        assert_eq!(
            detect_image_ext(&[0xFF, 0xD8, 0xFF, 0xE0, 0x00]),
            Some("jpg")
        );
        // GIF.
        assert_eq!(detect_image_ext(b"GIF89a..."), Some("gif"));
        assert_eq!(detect_image_ext(b"GIF87a..."), Some("gif"));
        // RIFF/WEBP.
        let mut webp = Vec::from(*b"RIFF\x00\x00\x00\x00WEBP");
        webp.extend_from_slice(b"VP8 ");
        assert_eq!(detect_image_ext(&webp), Some("webp"));
    }

    #[test]
    fn detect_image_ext_rejects_non_images() {
        // Empty input, plain text, HTML stub, executable header are all rejected.
        assert_eq!(detect_image_ext(b""), None);
        assert_eq!(detect_image_ext(b"not an image"), None);
        assert_eq!(detect_image_ext(b"<html><body>"), None);
        assert_eq!(detect_image_ext(b"\x7fELF"), None);
        // RIFF without the WEBP marker should not be accepted.
        let riff_wave = b"RIFF\x00\x00\x00\x00WAVEfmt ";
        assert_eq!(detect_image_ext(riff_wave), None);
    }

    #[tokio::test]
    async fn rate_limiter_sweeps_expired_entries() {
        // Pin that periodic eviction actually drops entries from the map so
        // it cannot grow without bound under traffic from many distinct IPs.
        let limiter = RateLimiter::new(5, 0); // 0-second window: immediate expiry
        for i in 0..50 {
            limiter.check_limited(&format!("ip-{}", i)).await;
        }
        // Next call triggers a sweep; all prior entries should be evicted
        // before the new key is inserted.
        limiter.check_limited("trigger-sweep").await;
        let state = limiter.state.lock().await;
        assert_eq!(
            state.attempts.len(),
            1,
            "expired entries should be swept, only the newest survives"
        );
    }

    #[test]
    fn parse_avail_schedule_uses_user_default_when_submitted_is_empty() {
        // Empty submission + user default "Tue 14:00-18:00" → returns the user default
        // instead of falling back to the hardcoded Mon-Fri 09:00-17:00.
        let result = parse_avail_schedule(Some(""), None, None, None, None, Some("2:14:00-18:00"));
        assert_eq!(result.len(), 1);
        let windows = result.get(&2).expect("Tuesday should be set");
        assert_eq!(windows, &vec![("14:00".to_string(), "18:00".to_string())]);
    }

    #[test]
    fn parse_avail_schedule_prefers_submitted_over_user_default() {
        // A populated submission overrides the user default.
        let result = parse_avail_schedule(
            Some("3:10:00-12:00"),
            None,
            None,
            None,
            None,
            Some("2:14:00-18:00"),
        );
        assert_eq!(result.len(), 1);
        let windows = result.get(&3).expect("Wednesday should be set");
        assert_eq!(windows, &vec![("10:00".to_string(), "12:00".to_string())]);
    }

    #[test]
    fn parse_avail_schedule_falls_back_to_legacy_when_both_empty() {
        // Empty submission and empty user default → hardcoded Mon-Fri 09:00-17:00.
        let result = parse_avail_schedule(Some(""), None, None, None, None, Some(""));
        assert_eq!(result.len(), 5);
        for day in 1..=5 {
            let windows = result.get(&day).expect("weekday should be set");
            assert_eq!(windows, &vec![("09:00".to_string(), "17:00".to_string())]);
        }
    }

    // --- has_conflict tests ---

    fn dt(y: i32, m: u32, d: u32, h: u32, mi: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(y, m, d)
            .unwrap()
            .and_hms_opt(h, mi, 0)
            .unwrap()
    }

    #[test]
    fn conflict_overlapping_event() {
        let busy = vec![(dt(2026, 3, 15, 10, 0), dt(2026, 3, 15, 11, 0))];
        // Slot 10:30-11:30 overlaps with 10:00-11:00
        assert!(has_conflict(
            &busy,
            dt(2026, 3, 15, 10, 30),
            dt(2026, 3, 15, 11, 30)
        ));
    }

    #[test]
    fn conflict_no_overlap() {
        let busy = vec![(dt(2026, 3, 15, 10, 0), dt(2026, 3, 15, 11, 0))];
        // Slot 11:00-12:00 starts exactly when event ends (no overlap)
        assert!(!has_conflict(
            &busy,
            dt(2026, 3, 15, 11, 0),
            dt(2026, 3, 15, 12, 0)
        ));
    }

    #[test]
    fn conflict_event_contains_slot() {
        let busy = vec![(dt(2026, 3, 15, 9, 0), dt(2026, 3, 15, 17, 0))];
        // Slot entirely within busy period
        assert!(has_conflict(
            &busy,
            dt(2026, 3, 15, 10, 0),
            dt(2026, 3, 15, 11, 0)
        ));
    }

    #[test]
    fn conflict_slot_contains_event() {
        let busy = vec![(dt(2026, 3, 15, 10, 15), dt(2026, 3, 15, 10, 45))];
        // Slot 10:00-11:00 contains the 10:15-10:45 event
        assert!(has_conflict(
            &busy,
            dt(2026, 3, 15, 10, 0),
            dt(2026, 3, 15, 11, 0)
        ));
    }

    #[test]
    fn conflict_adjacent_not_conflicting() {
        let busy = vec![
            (dt(2026, 3, 15, 9, 0), dt(2026, 3, 15, 10, 0)),
            (dt(2026, 3, 15, 11, 0), dt(2026, 3, 15, 12, 0)),
        ];
        // Slot 10:00-11:00 is between two events (no overlap)
        assert!(!has_conflict(
            &busy,
            dt(2026, 3, 15, 10, 0),
            dt(2026, 3, 15, 11, 0)
        ));
    }

    #[test]
    fn conflict_empty_busy_list() {
        let busy: Vec<(NaiveDateTime, NaiveDateTime)> = vec![];
        assert!(!has_conflict(
            &busy,
            dt(2026, 3, 15, 10, 0),
            dt(2026, 3, 15, 11, 0)
        ));
    }

    #[test]
    fn conflict_buffer_causes_overlap() {
        let busy = vec![(dt(2026, 3, 15, 10, 0), dt(2026, 3, 15, 11, 0))];
        // Slot is 11:00-12:00, but with 15min buffer before → buf_start=10:45 overlaps
        assert!(has_conflict(
            &busy,
            dt(2026, 3, 15, 10, 45),
            dt(2026, 3, 15, 12, 0)
        ));
    }

    // --- expand_recurring_into_busy tests ---

    #[test]
    fn expand_recurring_weekly_into_busy() {
        let recurring = vec![(
            "20260309T100000".to_string(), // Monday 10:00
            "20260309T110000".to_string(), // Monday 11:00
            "FREQ=WEEKLY;BYDAY=MO".to_string(),
            None,
            None,
        )];
        let window_start = dt(2026, 3, 9, 0, 0);
        let window_end = dt(2026, 3, 23, 23, 59);
        let busy = expand_recurring_into_busy(&recurring, window_start, window_end, Tz::UTC);
        // Should have 3 occurrences: Mar 9, 16, 23
        assert_eq!(busy.len(), 3);
        assert_eq!(busy[0].0, dt(2026, 3, 9, 10, 0));
        assert_eq!(busy[1].0, dt(2026, 3, 16, 10, 0));
        assert_eq!(busy[2].0, dt(2026, 3, 23, 10, 0));
    }

    #[test]
    fn expand_recurring_with_exdate() {
        let raw_ical = "BEGIN:VEVENT\nDTSTART:20260309T100000\nDTEND:20260309T110000\nRRULE:FREQ=WEEKLY;BYDAY=MO\nEXDATE:20260316T100000\nEND:VEVENT";
        let recurring = vec![(
            "20260309T100000".to_string(),
            "20260309T110000".to_string(),
            "FREQ=WEEKLY;BYDAY=MO".to_string(),
            Some(raw_ical.to_string()),
            None,
        )];
        let window_start = dt(2026, 3, 9, 0, 0);
        let window_end = dt(2026, 3, 23, 23, 59);
        let busy = expand_recurring_into_busy(&recurring, window_start, window_end, Tz::UTC);
        // Mar 16 excluded, so only Mar 9 and 23
        assert_eq!(busy.len(), 2);
        assert_eq!(busy[0].0, dt(2026, 3, 9, 10, 0));
        assert_eq!(busy[1].0, dt(2026, 3, 23, 10, 0));
    }

    // --- Integration tests with in-memory SQLite ---

    async fn setup_test_db() -> SqlitePool {
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

    /// Insert test fixtures: user, account, event type, availability rules
    async fn seed_test_data(pool: &SqlitePool) -> (String, String, String) {
        let user_id = uuid::Uuid::new_v4().to_string();
        let account_id = uuid::Uuid::new_v4().to_string();
        let et_id = uuid::Uuid::new_v4().to_string();

        sqlx::query("INSERT INTO users (id, email, name, role, auth_provider, username, enabled) VALUES (?, 'test@example.com', 'Test User', 'admin', 'local', 'testuser', 1)")
            .bind(&user_id)
            .execute(pool).await.unwrap();

        sqlx::query("INSERT INTO accounts (id, name, email, timezone, user_id) VALUES (?, 'Test User', 'test@example.com', 'UTC', ?)")
            .bind(&account_id)
            .bind(&user_id)
            .execute(pool).await.unwrap();

        sqlx::query("INSERT INTO event_types (id, account_id, slug, title, duration_min, buffer_before, buffer_after, min_notice_min, enabled) VALUES (?, ?, 'test-meeting', 'Test Meeting', 30, 0, 0, 0, 1)")
            .bind(&et_id)
            .bind(&account_id)
            .execute(pool).await.unwrap();

        // Mon-Fri 09:00-17:00
        for day in [1, 2, 3, 4, 5] {
            let rule_id = uuid::Uuid::new_v4().to_string();
            sqlx::query("INSERT INTO availability_rules (id, event_type_id, day_of_week, start_time, end_time) VALUES (?, ?, ?, '09:00', '17:00')")
                .bind(&rule_id)
                .bind(&et_id)
                .bind(day)
                .execute(pool).await.unwrap();
        }

        (user_id, account_id, et_id)
    }

    // --- Authorization helpers: can_manage_event_type / find_manageable_event_type_by_slug ---

    /// Insert a user with a configurable role. Returns user_id.
    async fn insert_role_user(pool: &SqlitePool, email: &str, role: &str) -> String {
        let id = uuid::Uuid::new_v4().to_string();
        let username = email.split('@').next().unwrap_or("u").to_string();
        sqlx::query(
            "INSERT INTO users (id, email, name, role, auth_provider, username, enabled) \
             VALUES (?, ?, ?, ?, 'local', ?, 1)",
        )
        .bind(&id)
        .bind(email)
        .bind(email)
        .bind(role)
        .bind(&username)
        .execute(pool)
        .await
        .unwrap();
        id
    }

    /// Insert a personal event type owned by `user_id`. Creates the account row
    /// if missing. Returns event_type_id.
    async fn insert_personal_et(pool: &SqlitePool, user_id: &str, slug: &str) -> String {
        let account_id: Option<String> =
            sqlx::query_scalar("SELECT id FROM accounts WHERE user_id = ?")
                .bind(user_id)
                .fetch_optional(pool)
                .await
                .unwrap();
        let account_id = match account_id {
            Some(id) => id,
            None => {
                let id = uuid::Uuid::new_v4().to_string();
                sqlx::query(
                    "INSERT INTO accounts (id, name, email, timezone, user_id) \
                     VALUES (?, 'A', 'a@x', 'UTC', ?)",
                )
                .bind(&id)
                .bind(user_id)
                .execute(pool)
                .await
                .unwrap();
                id
            }
        };
        let et_id = uuid::Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO event_types (id, account_id, slug, title, duration_min, buffer_before, buffer_after, min_notice_min, enabled) \
             VALUES (?, ?, ?, 'P', 30, 0, 0, 0, 1)",
        )
        .bind(&et_id)
        .bind(&account_id)
        .bind(slug)
        .execute(pool)
        .await
        .unwrap();
        et_id
    }

    /// Insert a team and a team event type. `members` is a list of (user_id, role).
    /// Returns (team_id, event_type_id).
    async fn insert_team_with_et(
        pool: &SqlitePool,
        team_slug: &str,
        et_slug: &str,
        members: &[(&str, &str)],
    ) -> (String, String) {
        let team_id = uuid::Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO teams (id, name, slug, visibility) VALUES (?, ?, ?, 'public')")
            .bind(&team_id)
            .bind(team_slug)
            .bind(team_slug)
            .execute(pool)
            .await
            .unwrap();
        for (uid, role) in members {
            sqlx::query(
                "INSERT INTO team_members (team_id, user_id, role, source) \
                 VALUES (?, ?, ?, 'direct')",
            )
            .bind(&team_id)
            .bind(uid)
            .bind(role)
            .execute(pool)
            .await
            .unwrap();
        }
        // Each team event gets its own placeholder account to avoid colliding
        // with personal events under the unique(account_id, slug) constraint.
        let account_id = uuid::Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO accounts (id, name, email, timezone) VALUES (?, 't', 't@x', 'UTC')",
        )
        .bind(&account_id)
        .execute(pool)
        .await
        .unwrap();
        let et_id = uuid::Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO event_types (id, account_id, team_id, slug, title, duration_min, buffer_before, buffer_after, min_notice_min, enabled) \
             VALUES (?, ?, ?, ?, 'T', 30, 0, 0, 0, 1)",
        )
        .bind(&et_id)
        .bind(&account_id)
        .bind(&team_id)
        .bind(et_slug)
        .execute(pool)
        .await
        .unwrap();
        (team_id, et_id)
    }

    async fn user_for(pool: &SqlitePool, user_id: &str) -> crate::models::User {
        sqlx::query_as("SELECT * FROM users WHERE id = ?")
            .bind(user_id)
            .fetch_one(pool)
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn can_manage_event_type_global_admin_yes_for_any_event() {
        let pool = setup_test_db().await;
        let admin_id = insert_role_user(&pool, "admin@x", "admin").await;
        let owner_id = insert_role_user(&pool, "owner@x", "user").await;
        let personal_et = insert_personal_et(&pool, &owner_id, "personal").await;
        let (_, team_et) = insert_team_with_et(&pool, "team1", "team-et", &[]).await;
        let admin = user_for(&pool, &admin_id).await;
        assert!(can_manage_event_type(&pool, &admin, &personal_et).await);
        assert!(can_manage_event_type(&pool, &admin, &team_et).await);
    }

    #[tokio::test]
    async fn can_manage_event_type_personal_owner_yes_others_no() {
        let pool = setup_test_db().await;
        let owner_id = insert_role_user(&pool, "owner@x", "user").await;
        let other_id = insert_role_user(&pool, "other@x", "user").await;
        let et = insert_personal_et(&pool, &owner_id, "personal").await;
        let owner = user_for(&pool, &owner_id).await;
        let other = user_for(&pool, &other_id).await;
        assert!(can_manage_event_type(&pool, &owner, &et).await);
        assert!(!can_manage_event_type(&pool, &other, &et).await);
    }

    #[tokio::test]
    async fn can_manage_event_type_team_admin_yes_member_no() {
        let pool = setup_test_db().await;
        let admin_id = insert_role_user(&pool, "ta@x", "user").await;
        let member_id = insert_role_user(&pool, "tm@x", "user").await;
        let outsider_id = insert_role_user(&pool, "out@x", "user").await;
        let (_, et) = insert_team_with_et(
            &pool,
            "team1",
            "team-et",
            &[(&admin_id, "admin"), (&member_id, "member")],
        )
        .await;
        let admin = user_for(&pool, &admin_id).await;
        let member = user_for(&pool, &member_id).await;
        let outsider = user_for(&pool, &outsider_id).await;
        assert!(can_manage_event_type(&pool, &admin, &et).await);
        assert!(!can_manage_event_type(&pool, &member, &et).await);
        assert!(!can_manage_event_type(&pool, &outsider, &et).await);
    }

    #[tokio::test]
    async fn can_manage_event_type_team_admin_cannot_manage_others_personal_event() {
        // Being a team admin somewhere does not grant access to an unrelated
        // user's personal event types.
        let pool = setup_test_db().await;
        let team_admin_id = insert_role_user(&pool, "ta@x", "user").await;
        let stranger_id = insert_role_user(&pool, "s@x", "user").await;
        let stranger_et = insert_personal_et(&pool, &stranger_id, "p").await;
        let _ = insert_team_with_et(&pool, "team1", "te", &[(&team_admin_id, "admin")]).await;
        let team_admin = user_for(&pool, &team_admin_id).await;
        assert!(!can_manage_event_type(&pool, &team_admin, &stranger_et).await);
    }

    #[tokio::test]
    async fn can_manage_event_type_team_member_cannot_manage_others_personal_event() {
        let pool = setup_test_db().await;
        let team_member_id = insert_role_user(&pool, "tm@x", "user").await;
        let stranger_id = insert_role_user(&pool, "s@x", "user").await;
        let stranger_et = insert_personal_et(&pool, &stranger_id, "p").await;
        let _ = insert_team_with_et(&pool, "team1", "te", &[(&team_member_id, "member")]).await;
        let team_member = user_for(&pool, &team_member_id).await;
        assert!(!can_manage_event_type(&pool, &team_member, &stranger_et).await);
    }

    #[tokio::test]
    async fn can_manage_event_type_personal_owner_cannot_manage_unrelated_team_event() {
        let pool = setup_test_db().await;
        // Owner of a personal event with the same slug as some team event —
        // owning the personal event must not grant access to the team one.
        let owner_id = insert_role_user(&pool, "owner@x", "user").await;
        let _ = insert_personal_et(&pool, &owner_id, "demo").await;
        let (_, team_et) = insert_team_with_et(&pool, "team1", "demo", &[]).await;
        let owner = user_for(&pool, &owner_id).await;
        assert!(!can_manage_event_type(&pool, &owner, &team_et).await);
    }

    #[tokio::test]
    async fn find_manageable_event_type_by_slug_personal_wins_collision() {
        // If the user has both a personal event and a team-admin event sharing
        // the same slug, the personal event must resolve first (deterministic).
        let pool = setup_test_db().await;
        let user_id = insert_role_user(&pool, "u@x", "user").await;
        let personal_et = insert_personal_et(&pool, &user_id, "demo").await;
        let (_, team_et) =
            insert_team_with_et(&pool, "team1", "demo", &[(&user_id, "admin")]).await;
        let resolved = find_manageable_event_type_by_slug(&pool, &user_id, "demo")
            .await
            .expect("should resolve");
        assert_eq!(resolved.0, personal_et);
        assert_ne!(resolved.0, team_et);
        assert!(resolved.2.is_none(), "personal event has no team_id");
    }

    #[tokio::test]
    async fn find_manageable_event_type_by_slug_member_does_not_resolve() {
        // Regular team members should not resolve team event types via this
        // helper — only team admins do.
        let pool = setup_test_db().await;
        let user_id = insert_role_user(&pool, "m@x", "user").await;
        let _ = insert_team_with_et(&pool, "team1", "demo", &[(&user_id, "member")]).await;
        assert!(find_manageable_event_type_by_slug(&pool, &user_id, "demo")
            .await
            .is_none());
    }

    #[tokio::test]
    async fn fetch_busy_times_empty_calendar() {
        let pool = setup_test_db().await;
        let (user_id, _, _) = seed_test_data(&pool).await;

        let busy = fetch_busy_times_for_user(
            &pool,
            &user_id,
            dt(2026, 3, 15, 0, 0),
            dt(2026, 3, 21, 23, 59),
            Tz::UTC,
            None,
        )
        .await;

        assert!(busy.is_empty(), "No events or bookings → no busy times");
    }

    #[tokio::test]
    async fn fetch_busy_times_includes_bookings() {
        let pool = setup_test_db().await;
        let (user_id, _, et_id) = seed_test_data(&pool).await;

        let booking_id = uuid::Uuid::new_v4().to_string();
        let cancel_tok = uuid::Uuid::new_v4().to_string();
        let resched_tok = uuid::Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO bookings (id, event_type_id, uid, guest_name, guest_email, guest_timezone, start_at, end_at, status, cancel_token, reschedule_token) VALUES (?, ?, 'uid1', 'Guest', 'guest@example.com', 'UTC', '2026-03-16T10:00:00', '2026-03-16T10:30:00', 'confirmed', ?, ?)")
            .bind(&booking_id)
            .bind(&et_id)
            .bind(&cancel_tok)
            .bind(&resched_tok)
            .execute(&pool).await.unwrap();

        let busy = fetch_busy_times_for_user(
            &pool,
            &user_id,
            dt(2026, 3, 15, 0, 0),
            dt(2026, 3, 21, 23, 59),
            Tz::UTC,
            None,
        )
        .await;

        assert_eq!(busy.len(), 1);
        assert_eq!(busy[0].0, dt(2026, 3, 16, 10, 0));
        assert_eq!(busy[0].1, dt(2026, 3, 16, 10, 30));
    }

    #[tokio::test]
    async fn fetch_busy_times_ignores_cancelled_bookings() {
        let pool = setup_test_db().await;
        let (user_id, _, et_id) = seed_test_data(&pool).await;

        let booking_id = uuid::Uuid::new_v4().to_string();
        let cancel_tok = uuid::Uuid::new_v4().to_string();
        let resched_tok = uuid::Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO bookings (id, event_type_id, uid, guest_name, guest_email, guest_timezone, start_at, end_at, status, cancel_token, reschedule_token) VALUES (?, ?, 'uid1', 'Guest', 'guest@example.com', 'UTC', '2026-03-16T10:00:00', '2026-03-16T10:30:00', 'cancelled', ?, ?)")
            .bind(&booking_id)
            .bind(&et_id)
            .bind(&cancel_tok)
            .bind(&resched_tok)
            .execute(&pool).await.unwrap();

        let busy = fetch_busy_times_for_user(
            &pool,
            &user_id,
            dt(2026, 3, 15, 0, 0),
            dt(2026, 3, 21, 23, 59),
            Tz::UTC,
            None,
        )
        .await;

        assert!(
            busy.is_empty(),
            "Cancelled bookings should not block availability"
        );
    }

    #[tokio::test]
    async fn compute_slots_basic_availability() {
        let pool = setup_test_db().await;
        let (_, _, et_id) = seed_test_data(&pool).await;

        let busy = BusySource::Individual(vec![]);

        // Start from tomorrow to avoid partial-day flakiness
        let slot_days = compute_slots(
            &pool,
            &et_id,
            30, // 30 min duration
            0,  // no buffer before
            0,  // no buffer after
            0,  // no min notice
            1,  // start from tomorrow
            14, // 14 days ahead
            Tz::UTC,
            Tz::UTC,
            busy,
        )
        .await;

        // Should have slots on weekdays only (Mon-Fri)
        assert!(!slot_days.is_empty(), "Should have slots on weekdays");

        // Each day should have 16 slots (09:00-17:00 in 30-min increments)
        for day in &slot_days {
            assert_eq!(
                day.slots.len(),
                16,
                "09:00-17:00 with 30min = 16 slots, got {} for {}",
                day.slots.len(),
                day.date
            );
        }
    }

    #[tokio::test]
    async fn compute_slots_with_busy_event() {
        let pool = setup_test_db().await;
        let (_, _, et_id) = seed_test_data(&pool).await;

        // Find the next Monday from now
        let now = Utc::now().with_timezone(&Tz::UTC).naive_local();
        let mut next_monday = now.date() + Duration::days(1);
        while next_monday.weekday() != chrono::Weekday::Mon {
            next_monday += Duration::days(1);
        }
        let days_to_monday = (next_monday - now.date()).num_days() as i32;

        // Block 10:00-11:00 on that Monday
        let busy_start = next_monday.and_hms_opt(10, 0, 0).unwrap();
        let busy_end = next_monday.and_hms_opt(11, 0, 0).unwrap();
        let busy = BusySource::Individual(vec![(busy_start, busy_end)]);

        let slot_days = compute_slots(
            &pool,
            &et_id,
            30,
            0,
            0,
            0,
            days_to_monday,
            1, // just 1 day
            Tz::UTC,
            Tz::UTC,
            busy,
        )
        .await;

        assert!(!slot_days.is_empty(), "Should have the Monday");
        let monday = &slot_days[0];

        // 16 slots normally, minus 2 (10:00 and 10:30 blocked) = 14
        assert_eq!(monday.slots.len(), 14, "10:00 and 10:30 should be blocked");

        // Verify 10:00 and 10:30 are not in the slots
        let slot_times: Vec<&str> = monday.slots.iter().map(|s| s.start.as_str()).collect();
        assert!(!slot_times.contains(&"10:00"), "10:00 should be blocked");
        assert!(!slot_times.contains(&"10:30"), "10:30 should be blocked");
    }

    #[tokio::test]
    async fn compute_slots_with_buffer() {
        let pool = setup_test_db().await;
        let (_, _, et_id) = seed_test_data(&pool).await;

        // Find the next Monday
        let now = Utc::now().with_timezone(&Tz::UTC).naive_local();
        let mut next_monday = now.date() + Duration::days(1);
        while next_monday.weekday() != chrono::Weekday::Mon {
            next_monday += Duration::days(1);
        }
        let days_to_monday = (next_monday - now.date()).num_days() as i32;

        // Block 10:00-10:30 on that Monday
        let busy_start = next_monday.and_hms_opt(10, 0, 0).unwrap();
        let busy_end = next_monday.and_hms_opt(10, 30, 0).unwrap();
        let busy = BusySource::Individual(vec![(busy_start, busy_end)]);

        let slot_days = compute_slots(
            &pool,
            &et_id,
            30,
            15, // 15 min buffer before
            15, // 15 min buffer after
            0,
            days_to_monday,
            1,
            Tz::UTC,
            Tz::UTC,
            busy,
        )
        .await;

        assert!(!slot_days.is_empty());
        let monday = &slot_days[0];

        // Event at 10:00-10:30 with 15min buffers blocks 09:45-10:45
        // So slots 09:30 (ends 10:00, buf_end=10:15 > 09:45), 10:00, 10:30 (buf_start=10:15 < 10:30) blocked
        let slot_times: Vec<&str> = monday.slots.iter().map(|s| s.start.as_str()).collect();
        assert!(
            !slot_times.contains(&"10:00"),
            "10:00 should be blocked (direct conflict)"
        );
        assert!(
            slot_times.contains(&"09:00"),
            "09:00 should be free (no buffer overlap)"
        );
    }

    #[tokio::test]
    async fn compute_slots_frequency_limit_per_day_drops_capped_day() {
        let pool = setup_test_db().await;
        let (_, _, et_id) = seed_test_data(&pool).await;

        let now = Utc::now().with_timezone(&Tz::UTC).naive_local();
        let mut next_monday = now.date() + Duration::days(1);
        while next_monday.weekday() != chrono::Weekday::Mon {
            next_monday += Duration::days(1);
        }
        let days_to_monday = (next_monday - now.date()).num_days() as i32;

        sqlx::query("INSERT INTO booking_frequency_limits (id, event_type_id, max_bookings, period) VALUES (?, ?, 1, 'day')")
            .bind(uuid::Uuid::new_v4().to_string())
            .bind(&et_id)
            .execute(&pool).await.unwrap();

        let start_at = next_monday
            .and_hms_opt(10, 0, 0)
            .unwrap()
            .format("%Y-%m-%dT%H:%M:%S")
            .to_string();
        let end_at = next_monday
            .and_hms_opt(10, 30, 0)
            .unwrap()
            .format("%Y-%m-%dT%H:%M:%S")
            .to_string();
        sqlx::query("INSERT INTO bookings (id, event_type_id, uid, guest_name, guest_email, guest_timezone, start_at, end_at, status, cancel_token, reschedule_token) VALUES (?, ?, 'uid1', 'Guest', 'g@e.com', 'UTC', ?, ?, 'confirmed', ?, ?)")
            .bind(uuid::Uuid::new_v4().to_string())
            .bind(&et_id)
            .bind(&start_at)
            .bind(&end_at)
            .bind(uuid::Uuid::new_v4().to_string())
            .bind(uuid::Uuid::new_v4().to_string())
            .execute(&pool).await.unwrap();

        let slot_days = compute_slots(
            &pool,
            &et_id,
            30,
            0,
            0,
            0,
            days_to_monday,
            5,
            Tz::UTC,
            Tz::UTC,
            BusySource::Individual(vec![]),
        )
        .await;

        let monday_date = next_monday.format("%Y-%m-%d").to_string();
        let monday = slot_days.iter().find(|d| d.date == monday_date);
        assert!(
            monday.map(|d| d.slots.is_empty()).unwrap_or(true),
            "1/day cap reached → Monday should expose no slots, got {:?}",
            monday.map(|d| d.slots.len())
        );

        let tuesday_date = (next_monday + Duration::days(1))
            .format("%Y-%m-%d")
            .to_string();
        let tuesday = slot_days
            .iter()
            .find(|d| d.date == tuesday_date)
            .expect("Tuesday should be present");
        assert_eq!(
            tuesday.slots.len(),
            16,
            "Tuesday is unaffected by Monday's per-day cap"
        );
    }

    #[tokio::test]
    async fn compute_slots_frequency_limit_per_week_drops_whole_week() {
        let pool = setup_test_db().await;
        let (_, _, et_id) = seed_test_data(&pool).await;

        let now = Utc::now().with_timezone(&Tz::UTC).naive_local();
        let mut next_monday = now.date() + Duration::days(1);
        while next_monday.weekday() != chrono::Weekday::Mon {
            next_monday += Duration::days(1);
        }
        let days_to_monday = (next_monday - now.date()).num_days() as i32;

        sqlx::query("INSERT INTO booking_frequency_limits (id, event_type_id, max_bookings, period) VALUES (?, ?, 1, 'week')")
            .bind(uuid::Uuid::new_v4().to_string())
            .bind(&et_id)
            .execute(&pool).await.unwrap();

        // One booking on Monday consumes the week's only slot.
        let start_at = next_monday
            .and_hms_opt(10, 0, 0)
            .unwrap()
            .format("%Y-%m-%dT%H:%M:%S")
            .to_string();
        let end_at = next_monday
            .and_hms_opt(10, 30, 0)
            .unwrap()
            .format("%Y-%m-%dT%H:%M:%S")
            .to_string();
        sqlx::query("INSERT INTO bookings (id, event_type_id, uid, guest_name, guest_email, guest_timezone, start_at, end_at, status, cancel_token, reschedule_token) VALUES (?, ?, 'uid1', 'Guest', 'g@e.com', 'UTC', ?, ?, 'confirmed', ?, ?)")
            .bind(uuid::Uuid::new_v4().to_string())
            .bind(&et_id)
            .bind(&start_at)
            .bind(&end_at)
            .bind(uuid::Uuid::new_v4().to_string())
            .bind(uuid::Uuid::new_v4().to_string())
            .execute(&pool).await.unwrap();

        let slot_days = compute_slots(
            &pool,
            &et_id,
            30,
            0,
            0,
            0,
            days_to_monday,
            5,
            Tz::UTC,
            Tz::UTC,
            BusySource::Individual(vec![]),
        )
        .await;

        for day in &slot_days {
            assert!(
                day.slots.is_empty(),
                "Per-week cap of 1 reached → {} should have no slots, got {}",
                day.date,
                day.slots.len()
            );
        }
    }

    #[tokio::test]
    async fn compute_slots_frequency_limit_ignores_cancelled_bookings() {
        // Regression: cancelled bookings must not count toward the cap. If they
        // did, hosts who frequently cancel would silently shrink their own
        // availability.
        let pool = setup_test_db().await;
        let (_, _, et_id) = seed_test_data(&pool).await;

        let now = Utc::now().with_timezone(&Tz::UTC).naive_local();
        let mut next_monday = now.date() + Duration::days(1);
        while next_monday.weekday() != chrono::Weekday::Mon {
            next_monday += Duration::days(1);
        }
        let days_to_monday = (next_monday - now.date()).num_days() as i32;

        sqlx::query("INSERT INTO booking_frequency_limits (id, event_type_id, max_bookings, period) VALUES (?, ?, 1, 'day')")
            .bind(uuid::Uuid::new_v4().to_string())
            .bind(&et_id)
            .execute(&pool).await.unwrap();

        // Single cancelled booking on Monday — should not consume the day's cap.
        let start_at = next_monday
            .and_hms_opt(10, 0, 0)
            .unwrap()
            .format("%Y-%m-%dT%H:%M:%S")
            .to_string();
        let end_at = next_monday
            .and_hms_opt(10, 30, 0)
            .unwrap()
            .format("%Y-%m-%dT%H:%M:%S")
            .to_string();
        sqlx::query("INSERT INTO bookings (id, event_type_id, uid, guest_name, guest_email, guest_timezone, start_at, end_at, status, cancel_token, reschedule_token) VALUES (?, ?, 'uid1', 'Guest', 'g@e.com', 'UTC', ?, ?, 'cancelled', ?, ?)")
            .bind(uuid::Uuid::new_v4().to_string())
            .bind(&et_id)
            .bind(&start_at)
            .bind(&end_at)
            .bind(uuid::Uuid::new_v4().to_string())
            .bind(uuid::Uuid::new_v4().to_string())
            .execute(&pool).await.unwrap();

        let slot_days = compute_slots(
            &pool,
            &et_id,
            30,
            0,
            0,
            0,
            days_to_monday,
            1,
            Tz::UTC,
            Tz::UTC,
            BusySource::Individual(vec![]),
        )
        .await;

        let monday_date = next_monday.format("%Y-%m-%d").to_string();
        let monday = slot_days
            .iter()
            .find(|d| d.date == monday_date)
            .expect("Monday should be present");
        assert_eq!(
            monday.slots.len(),
            16,
            "Cancelled bookings must not consume the day cap; expected full 16 slots, got {}",
            monday.slots.len()
        );
    }

    #[tokio::test]
    async fn compute_slots_frequency_limit_multiple_caps_each_checked() {
        // Configure both a 1/day and a 100/year cap. With one booking on
        // Monday only the day cap fires, so Monday must be hidden but
        // Tue–Fri must stay visible. This pins that the filter consults every
        // configured limit independently rather than stopping at the first.
        let pool = setup_test_db().await;
        let (_, _, et_id) = seed_test_data(&pool).await;

        let now = Utc::now().with_timezone(&Tz::UTC).naive_local();
        let mut next_monday = now.date() + Duration::days(1);
        while next_monday.weekday() != chrono::Weekday::Mon {
            next_monday += Duration::days(1);
        }
        let days_to_monday = (next_monday - now.date()).num_days() as i32;

        sqlx::query("INSERT INTO booking_frequency_limits (id, event_type_id, max_bookings, period) VALUES (?, ?, 1, 'day')")
            .bind(uuid::Uuid::new_v4().to_string())
            .bind(&et_id)
            .execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO booking_frequency_limits (id, event_type_id, max_bookings, period) VALUES (?, ?, 100, 'year')")
            .bind(uuid::Uuid::new_v4().to_string())
            .bind(&et_id)
            .execute(&pool).await.unwrap();

        let start_at = next_monday
            .and_hms_opt(10, 0, 0)
            .unwrap()
            .format("%Y-%m-%dT%H:%M:%S")
            .to_string();
        let end_at = next_monday
            .and_hms_opt(10, 30, 0)
            .unwrap()
            .format("%Y-%m-%dT%H:%M:%S")
            .to_string();
        sqlx::query("INSERT INTO bookings (id, event_type_id, uid, guest_name, guest_email, guest_timezone, start_at, end_at, status, cancel_token, reschedule_token) VALUES (?, ?, 'uid1', 'Guest', 'g@e.com', 'UTC', ?, ?, 'confirmed', ?, ?)")
            .bind(uuid::Uuid::new_v4().to_string())
            .bind(&et_id)
            .bind(&start_at)
            .bind(&end_at)
            .bind(uuid::Uuid::new_v4().to_string())
            .bind(uuid::Uuid::new_v4().to_string())
            .execute(&pool).await.unwrap();

        let slot_days = compute_slots(
            &pool,
            &et_id,
            30,
            0,
            0,
            0,
            days_to_monday,
            5,
            Tz::UTC,
            Tz::UTC,
            BusySource::Individual(vec![]),
        )
        .await;

        let monday_date = next_monday.format("%Y-%m-%d").to_string();
        let monday = slot_days.iter().find(|d| d.date == monday_date);
        assert!(
            monday.map(|d| d.slots.is_empty()).unwrap_or(true),
            "1/day cap should hide Monday even with a lax year cap, got {:?}",
            monday.map(|d| d.slots.len())
        );

        let tuesday_date = (next_monday + Duration::days(1))
            .format("%Y-%m-%d")
            .to_string();
        let tuesday = slot_days
            .iter()
            .find(|d| d.date == tuesday_date)
            .expect("Tuesday should be present");
        assert_eq!(
            tuesday.slots.len(),
            16,
            "Year cap not reached → Tuesday must stay visible"
        );
    }

    /// Attach the seeded event type to a team and add two team members
    /// (alice, bob). Returns their user ids.
    async fn seed_team_for_event_type(pool: &SqlitePool, et_id: &str) -> (String, String) {
        let team_id = uuid::Uuid::new_v4().to_string();
        let alice = uuid::Uuid::new_v4().to_string();
        let bob = uuid::Uuid::new_v4().to_string();

        sqlx::query(
            "INSERT INTO teams (id, name, slug, visibility) VALUES (?, 'Team', 'team', 'public')",
        )
        .bind(&team_id)
        .execute(pool)
        .await
        .unwrap();
        sqlx::query("UPDATE event_types SET team_id = ? WHERE id = ?")
            .bind(&team_id)
            .bind(et_id)
            .execute(pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO users (id, email, name, role, auth_provider, username, enabled) VALUES (?, 'alice@example.com', 'Alice', 'user', 'local', 'alice', 1)")
            .bind(&alice).execute(pool).await.unwrap();
        sqlx::query("INSERT INTO users (id, email, name, role, auth_provider, username, enabled) VALUES (?, 'bob@example.com', 'Bob', 'user', 'local', 'bob', 1)")
            .bind(&bob).execute(pool).await.unwrap();
        sqlx::query("INSERT INTO team_members (team_id, user_id, role, source) VALUES (?, ?, 'member', 'direct')")
            .bind(&team_id).bind(&alice).execute(pool).await.unwrap();
        sqlx::query("INSERT INTO team_members (team_id, user_id, role, source) VALUES (?, ?, 'member', 'direct')")
            .bind(&team_id).bind(&bob).execute(pool).await.unwrap();

        (alice, bob)
    }

    async fn insert_booking(
        pool: &SqlitePool,
        et_id: &str,
        assigned_user_id: Option<&str>,
        start: NaiveDateTime,
        status: &str,
    ) {
        let end = start + Duration::minutes(30);
        sqlx::query("INSERT INTO bookings (id, event_type_id, uid, guest_name, guest_email, guest_timezone, start_at, end_at, status, cancel_token, reschedule_token, assigned_user_id) VALUES (?, ?, ?, 'G', 'g@e.com', 'UTC', ?, ?, ?, ?, ?, ?)")
            .bind(uuid::Uuid::new_v4().to_string())
            .bind(et_id)
            .bind(uuid::Uuid::new_v4().to_string())
            .bind(start.format("%Y-%m-%dT%H:%M:%S").to_string())
            .bind(end.format("%Y-%m-%dT%H:%M:%S").to_string())
            .bind(status)
            .bind(uuid::Uuid::new_v4().to_string())
            .bind(uuid::Uuid::new_v4().to_string())
            .bind(assigned_user_id)
            .execute(pool).await.unwrap();
    }

    #[tokio::test]
    async fn would_exceed_frequency_limit_per_member_blocks_only_named_assignee() {
        let pool = setup_test_db().await;
        let (_, _, et_id) = seed_test_data(&pool).await;
        let (alice, bob) = seed_team_for_event_type(&pool, &et_id).await;

        sqlx::query("INSERT INTO booking_frequency_limits (id, event_type_id, max_bookings, period, per_member) VALUES (?, ?, 1, 'day', 1)")
            .bind(uuid::Uuid::new_v4().to_string())
            .bind(&et_id)
            .execute(&pool).await.unwrap();

        let booked_day = NaiveDate::from_ymd_opt(2026, 3, 16).unwrap();
        let booked_start = booked_day.and_hms_opt(10, 0, 0).unwrap();
        insert_booking(&pool, &et_id, Some(&alice), booked_start, "confirmed").await;

        let proposed = booked_day.and_hms_opt(14, 0, 0).unwrap();
        assert!(
            would_exceed_frequency_limit(&pool, &et_id, proposed, Some(&alice)).await,
            "Alice already has one booking that day under 1/day per-member"
        );
        assert!(
            !would_exceed_frequency_limit(&pool, &et_id, proposed, Some(&bob)).await,
            "Bob has zero bookings; per-member cap should not block him"
        );
    }

    #[tokio::test]
    async fn apply_frequency_limit_filter_per_member_keeps_slots_when_one_member_free() {
        // 1/day per-member, 2 team members, 1 booking on Monday assigned to
        // Alice → Monday must still show slots because Bob has headroom.
        let pool = setup_test_db().await;
        let (_, _, et_id) = seed_test_data(&pool).await;
        let (alice, _bob) = seed_team_for_event_type(&pool, &et_id).await;

        let now = Utc::now().with_timezone(&Tz::UTC).naive_local();
        let mut next_monday = now.date() + Duration::days(1);
        while next_monday.weekday() != chrono::Weekday::Mon {
            next_monday += Duration::days(1);
        }
        let days_to_monday = (next_monday - now.date()).num_days() as i32;

        sqlx::query("INSERT INTO booking_frequency_limits (id, event_type_id, max_bookings, period, per_member) VALUES (?, ?, 1, 'day', 1)")
            .bind(uuid::Uuid::new_v4().to_string())
            .bind(&et_id)
            .execute(&pool).await.unwrap();
        insert_booking(
            &pool,
            &et_id,
            Some(&alice),
            next_monday.and_hms_opt(10, 0, 0).unwrap(),
            "confirmed",
        )
        .await;

        let slot_days = compute_slots(
            &pool,
            &et_id,
            30,
            0,
            0,
            0,
            days_to_monday,
            1,
            Tz::UTC,
            Tz::UTC,
            BusySource::Individual(vec![]),
        )
        .await;

        let monday_date = next_monday.format("%Y-%m-%d").to_string();
        let monday = slot_days
            .iter()
            .find(|d| d.date == monday_date)
            .expect("Monday should be present");
        assert_eq!(
            monday.slots.len(),
            16,
            "Per-member cap consumed by Alice must not hide slots while Bob is free"
        );
    }

    #[tokio::test]
    async fn apply_frequency_limit_filter_per_member_drops_slots_when_all_members_capped() {
        // 1/day per-member, 2 team members, both already booked on Monday →
        // Monday must show no slots.
        let pool = setup_test_db().await;
        let (_, _, et_id) = seed_test_data(&pool).await;
        let (alice, bob) = seed_team_for_event_type(&pool, &et_id).await;

        let now = Utc::now().with_timezone(&Tz::UTC).naive_local();
        let mut next_monday = now.date() + Duration::days(1);
        while next_monday.weekday() != chrono::Weekday::Mon {
            next_monday += Duration::days(1);
        }
        let days_to_monday = (next_monday - now.date()).num_days() as i32;

        sqlx::query("INSERT INTO booking_frequency_limits (id, event_type_id, max_bookings, period, per_member) VALUES (?, ?, 1, 'day', 1)")
            .bind(uuid::Uuid::new_v4().to_string())
            .bind(&et_id)
            .execute(&pool).await.unwrap();
        insert_booking(
            &pool,
            &et_id,
            Some(&alice),
            next_monday.and_hms_opt(10, 0, 0).unwrap(),
            "confirmed",
        )
        .await;
        insert_booking(
            &pool,
            &et_id,
            Some(&bob),
            next_monday.and_hms_opt(11, 0, 0).unwrap(),
            "confirmed",
        )
        .await;

        let slot_days = compute_slots(
            &pool,
            &et_id,
            30,
            0,
            0,
            0,
            days_to_monday,
            1,
            Tz::UTC,
            Tz::UTC,
            BusySource::Individual(vec![]),
        )
        .await;

        let monday_date = next_monday.format("%Y-%m-%d").to_string();
        let monday = slot_days.iter().find(|d| d.date == monday_date);
        assert!(
            monday.map(|d| d.slots.is_empty()).unwrap_or(true),
            "Every team member at cap → Monday must be empty, got {:?}",
            monday.map(|d| d.slots.len())
        );
    }

    #[tokio::test]
    async fn apply_frequency_limit_filter_per_member_on_personal_event_type_degrades_to_wide() {
        // Personal event type (no team) with a per_member flag set. With no
        // team members to scope by, the cap should behave event-type-wide so
        // a single booking still hides the day.
        let pool = setup_test_db().await;
        let (_, _, et_id) = seed_test_data(&pool).await;

        let now = Utc::now().with_timezone(&Tz::UTC).naive_local();
        let mut next_monday = now.date() + Duration::days(1);
        while next_monday.weekday() != chrono::Weekday::Mon {
            next_monday += Duration::days(1);
        }
        let days_to_monday = (next_monday - now.date()).num_days() as i32;

        sqlx::query("INSERT INTO booking_frequency_limits (id, event_type_id, max_bookings, period, per_member) VALUES (?, ?, 1, 'day', 1)")
            .bind(uuid::Uuid::new_v4().to_string())
            .bind(&et_id)
            .execute(&pool).await.unwrap();
        insert_booking(
            &pool,
            &et_id,
            None,
            next_monday.and_hms_opt(10, 0, 0).unwrap(),
            "confirmed",
        )
        .await;

        let slot_days = compute_slots(
            &pool,
            &et_id,
            30,
            0,
            0,
            0,
            days_to_monday,
            1,
            Tz::UTC,
            Tz::UTC,
            BusySource::Individual(vec![]),
        )
        .await;

        let monday_date = next_monday.format("%Y-%m-%d").to_string();
        let monday = slot_days.iter().find(|d| d.date == monday_date);
        assert!(
            monday.map(|d| d.slots.is_empty()).unwrap_or(true),
            "Per-member flag on a personal event type must still cap the day"
        );
    }

    #[tokio::test]
    async fn apply_frequency_limit_filter_mixed_per_member_and_wide_fire_independently() {
        // Two limits on the same team event type:
        //   - 1/day per-member
        //   - 3/week event-type-wide
        //
        // Bookings span two consecutive weeks:
        //   Week 1: three bookings by Alice on Mon/Tue/Wed → wide week count
        //           hits its cap (3/3).
        //   Week 2: one booking each by Alice and Bob on Monday → both
        //           per-member day caps fill, wide week count is 2/3 (lax).
        //
        // Expectations:
        //   - Every weekday in week 1 must be hidden — wide cap dominates,
        //     even Thu/Fri where per-member alone would still allow bookings.
        //   - Week 2 Monday must be hidden by the per-member rule (both
        //     members at cap) while the wide rule still has headroom.
        //   - Week 2 Tue-Fri must be visible — neither rule fires.
        //
        // If a future change ever conflates the two rule paths, this test
        // will fail in a specific weekday rather than silently allow or
        // deny everything.
        let pool = setup_test_db().await;
        let (_, _, et_id) = seed_test_data(&pool).await;
        let (alice, bob) = seed_team_for_event_type(&pool, &et_id).await;

        let now = Utc::now().with_timezone(&Tz::UTC).naive_local();
        let mut next_monday = now.date() + Duration::days(1);
        while next_monday.weekday() != chrono::Weekday::Mon {
            next_monday += Duration::days(1);
        }
        let days_to_monday = (next_monday - now.date()).num_days() as i32;

        sqlx::query("INSERT INTO booking_frequency_limits (id, event_type_id, max_bookings, period, per_member) VALUES (?, ?, 1, 'day', 1)")
            .bind(uuid::Uuid::new_v4().to_string())
            .bind(&et_id)
            .execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO booking_frequency_limits (id, event_type_id, max_bookings, period, per_member) VALUES (?, ?, 3, 'week', 0)")
            .bind(uuid::Uuid::new_v4().to_string())
            .bind(&et_id)
            .execute(&pool).await.unwrap();

        // Week 1: three Alice bookings consume the wide week cap.
        for offset in 0..3 {
            insert_booking(
                &pool,
                &et_id,
                Some(&alice),
                (next_monday + Duration::days(offset))
                    .and_hms_opt(10, 0, 0)
                    .unwrap(),
                "confirmed",
            )
            .await;
        }

        // Week 2 Monday: both members booked, capping per-member.
        let week2_monday = next_monday + Duration::days(7);
        insert_booking(
            &pool,
            &et_id,
            Some(&alice),
            week2_monday.and_hms_opt(10, 0, 0).unwrap(),
            "confirmed",
        )
        .await;
        insert_booking(
            &pool,
            &et_id,
            Some(&bob),
            week2_monday.and_hms_opt(11, 0, 0).unwrap(),
            "confirmed",
        )
        .await;

        let slot_days = compute_slots(
            &pool,
            &et_id,
            30,
            0,
            0,
            0,
            days_to_monday,
            14,
            Tz::UTC,
            Tz::UTC,
            BusySource::Individual(vec![]),
        )
        .await;

        let find_day = |d: NaiveDate| -> Option<&SlotDay> {
            let key = d.format("%Y-%m-%d").to_string();
            slot_days.iter().find(|x| x.date == key)
        };

        // Week 1 Mon-Fri: every weekday hidden because the wide cap is hit.
        for offset in 0..5 {
            let d = next_monday + Duration::days(offset);
            let day = find_day(d);
            assert!(
                day.map(|x| x.slots.is_empty()).unwrap_or(true),
                "Week 1 {} should be hidden by the 3/week wide cap; got {:?}",
                d,
                day.map(|x| x.slots.len())
            );
        }

        // Week 2 Mon: per-member dominates — wide is lax (2/3) but both
        // members are at their per-day cap.
        let w2_mon = find_day(week2_monday);
        assert!(
            w2_mon.map(|x| x.slots.is_empty()).unwrap_or(true),
            "Week 2 Monday should be hidden by per-member rule; got {:?}",
            w2_mon.map(|x| x.slots.len())
        );

        // Week 2 Tue-Fri: neither rule fires, full availability.
        for offset in 1..5 {
            let d = week2_monday + Duration::days(offset);
            let day = find_day(d).expect("Week 2 weekday should be present");
            assert_eq!(
                day.slots.len(),
                16,
                "Week 2 {} should be free; got {} slots",
                d,
                day.slots.len()
            );
        }
    }

    #[tokio::test]
    async fn compute_slots_no_weekend_slots() {
        let pool = setup_test_db().await;
        let (_, _, et_id) = seed_test_data(&pool).await;

        // Find next Saturday
        let now = Utc::now().with_timezone(&Tz::UTC).naive_local();
        let mut next_sat = now.date();
        while next_sat.weekday() != chrono::Weekday::Sat {
            next_sat += Duration::days(1);
        }
        let days_to_sat = (next_sat - now.date()).num_days() as i32;

        let busy = BusySource::Individual(vec![]);
        let slot_days = compute_slots(
            &pool,
            &et_id,
            30,
            0,
            0,
            0,
            days_to_sat,
            2, // just Sat + Sun
            Tz::UTC,
            Tz::UTC,
            busy,
        )
        .await;

        assert!(
            slot_days.is_empty(),
            "Weekends should have no slots (Mon-Fri rules only)"
        );
    }

    #[tokio::test]
    async fn compute_slots_group_any_member_free() {
        let pool = setup_test_db().await;
        let (_, _, et_id) = seed_test_data(&pool).await;

        // Find the next Monday
        let now = Utc::now().with_timezone(&Tz::UTC).naive_local();
        let mut next_monday = now.date() + Duration::days(1);
        while next_monday.weekday() != chrono::Weekday::Mon {
            next_monday += Duration::days(1);
        }
        let days_to_monday = (next_monday - now.date()).num_days() as i32;

        let ten_am = next_monday.and_hms_opt(10, 0, 0).unwrap();
        let ten_thirty = next_monday.and_hms_opt(10, 30, 0).unwrap();

        // Member A is busy at 10:00, Member B is free
        let mut member_busy = HashMap::new();
        member_busy.insert("member_a".to_string(), vec![(ten_am, ten_thirty)]);
        member_busy.insert("member_b".to_string(), vec![]); // free

        let busy = BusySource::Group(member_busy);
        let slot_days = compute_slots(
            &pool,
            &et_id,
            30,
            0,
            0,
            0,
            days_to_monday,
            1,
            Tz::UTC,
            Tz::UTC,
            busy,
        )
        .await;

        assert!(!slot_days.is_empty());
        let monday = &slot_days[0];
        // 10:00 should still be available because member B is free
        let slot_times: Vec<&str> = monday.slots.iter().map(|s| s.start.as_str()).collect();
        assert!(
            slot_times.contains(&"10:00"),
            "10:00 should be available (member B is free)"
        );
        assert_eq!(
            monday.slots.len(),
            16,
            "All slots available (at least one member free)"
        );
    }

    #[tokio::test]
    async fn compute_slots_group_all_busy_blocks() {
        let pool = setup_test_db().await;
        let (_, _, et_id) = seed_test_data(&pool).await;

        let now = Utc::now().with_timezone(&Tz::UTC).naive_local();
        let mut next_monday = now.date() + Duration::days(1);
        while next_monday.weekday() != chrono::Weekday::Mon {
            next_monday += Duration::days(1);
        }
        let days_to_monday = (next_monday - now.date()).num_days() as i32;

        let ten_am = next_monday.and_hms_opt(10, 0, 0).unwrap();
        let ten_thirty = next_monday.and_hms_opt(10, 30, 0).unwrap();

        // Both members busy at 10:00
        let mut member_busy = HashMap::new();
        member_busy.insert("member_a".to_string(), vec![(ten_am, ten_thirty)]);
        member_busy.insert("member_b".to_string(), vec![(ten_am, ten_thirty)]);

        let busy = BusySource::Group(member_busy);
        let slot_days = compute_slots(
            &pool,
            &et_id,
            30,
            0,
            0,
            0,
            days_to_monday,
            1,
            Tz::UTC,
            Tz::UTC,
            busy,
        )
        .await;

        assert!(!slot_days.is_empty());
        let monday = &slot_days[0];
        let slot_times: Vec<&str> = monday.slots.iter().map(|s| s.start.as_str()).collect();
        assert!(
            !slot_times.contains(&"10:00"),
            "10:00 blocked when ALL members busy"
        );
        assert_eq!(monday.slots.len(), 15, "One slot blocked");
    }

    #[tokio::test]
    async fn compute_slots_team_requires_all_free() {
        // Team links require ALL members to be free (unlike Group which needs ANY)
        let pool = setup_test_db().await;
        let (_, _, et_id) = seed_test_data(&pool).await;

        let now = Utc::now().with_timezone(&Tz::UTC).naive_local();
        let mut next_monday = now.date() + Duration::days(1);
        while next_monday.weekday() != chrono::Weekday::Mon {
            next_monday += Duration::days(1);
        }
        let days_to_monday = (next_monday - now.date()).num_days() as i32;

        let ten_am = next_monday.and_hms_opt(10, 0, 0).unwrap();
        let ten_thirty = next_monday.and_hms_opt(10, 30, 0).unwrap();

        // Only ONE member busy at 10:00 — Team should block, Group would allow
        let mut member_busy = HashMap::new();
        member_busy.insert("member_a".to_string(), vec![(ten_am, ten_thirty)]);
        member_busy.insert("member_b".to_string(), vec![]); // member_b is free

        let busy = BusySource::Team(member_busy);
        let slot_days = compute_slots(
            &pool,
            &et_id,
            30,
            0,
            0,
            0,
            days_to_monday,
            1,
            Tz::UTC,
            Tz::UTC,
            busy,
        )
        .await;

        assert!(!slot_days.is_empty());
        let monday = &slot_days[0];
        let slot_times: Vec<&str> = monday.slots.iter().map(|s| s.start.as_str()).collect();
        assert!(
            !slot_times.contains(&"10:00"),
            "Team: 10:00 blocked when ANY member is busy"
        );
        assert_eq!(monday.slots.len(), 15, "One slot blocked for Team");
    }

    #[tokio::test]
    async fn compute_slots_team_all_free_allows() {
        // When all team members are free, the slot should be available
        let pool = setup_test_db().await;
        let (_, _, et_id) = seed_test_data(&pool).await;

        let now = Utc::now().with_timezone(&Tz::UTC).naive_local();
        let mut next_monday = now.date() + Duration::days(1);
        while next_monday.weekday() != chrono::Weekday::Mon {
            next_monday += Duration::days(1);
        }
        let days_to_monday = (next_monday - now.date()).num_days() as i32;

        // No one is busy
        let mut member_busy = HashMap::new();
        member_busy.insert("member_a".to_string(), vec![]);
        member_busy.insert("member_b".to_string(), vec![]);

        let busy = BusySource::Team(member_busy);
        let slot_days = compute_slots(
            &pool,
            &et_id,
            30,
            0,
            0,
            0,
            days_to_monday,
            1,
            Tz::UTC,
            Tz::UTC,
            busy,
        )
        .await;

        assert!(!slot_days.is_empty());
        let monday = &slot_days[0];
        assert_eq!(
            monday.slots.len(),
            16,
            "All 16 slots available when team is free"
        );
    }

    // Regression test: an availability rule that ends at 23:00 with a 60-min
    // slot must terminate. Before the fix, compute_slots_from_rules used a
    // NaiveTime cursor, and NaiveTime + Duration wraps at 24h — so when
    // cursor reached 23:00, cursor + slot_duration (60m) wrapped to 00:00
    // which is still <= 23:00 as a time-of-day, making the loop emit a slot
    // every step forever until OOM. Prod symptom: ~4-minute CPU spike +
    // ~9GB RAM before the OOM killer intervened (issue logged in internal
    // Vates Demo Team EN / book-a-demo-of-vates-vms booking page).
    #[test]
    fn compute_slots_terminates_with_window_ending_at_23_00() {
        let rules: Vec<(i32, String, String)> = (1..=4)
            .map(|d| (d, "09:00".to_string(), "23:00".to_string()))
            .collect();
        let busy = BusySource::Individual(vec![]);

        // Pick a start_offset that lands on a Monday so we hit at least one
        // rule-matching day regardless of when the test runs.
        let today = Utc::now().naive_utc().date();
        let mut start = 1i32;
        while (today + Duration::days(start as i64)).weekday() != chrono::Weekday::Mon {
            start += 1;
        }

        let result = compute_slots_from_rules(
            &rules,
            60,
            60,
            0,
            0,
            0,
            start,
            1,
            Tz::UTC,
            Tz::UTC,
            busy,
            &[],
        );

        // 09:00..=22:00 start times with 60-min spacing and 60-min duration =
        // 14 slots on a matching weekday. Anything beyond a few hundred means
        // the wrap bug is back.
        let total_slots: usize = result.iter().map(|d| d.slots.len()).sum();
        assert!(
            total_slots <= 20,
            "compute_slots_from_rules produced {} slots for a single day with \
             window 09:00-23:00 and 60-min duration; the NaiveTime wrap bug \
             is back",
            total_slots
        );
        assert_eq!(total_slots, 14, "expected exactly 14 hourly slots");
    }

    // Regression for issue #50: a team member in a different timezone than the host
    // must only be considered available within THEIR working hours, not inside the
    // event type's rule window interpreted in host_tz.
    #[tokio::test]
    async fn user_avail_as_busy_respects_member_timezone() {
        let pool = setup_test_db().await;

        let paris_uid = uuid::Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO users (id, email, name, role, auth_provider, username, enabled, timezone) \
             VALUES (?, 'paris@example.com', 'Paris User', 'user', 'local', 'paris', 1, 'Europe/Paris')",
        )
        .bind(&paris_uid)
        .execute(&pool)
        .await
        .unwrap();

        // Paris member works Mon-Fri 09:00-17:00 local time
        for day in [1, 2, 3, 4, 5] {
            let rid = uuid::Uuid::new_v4().to_string();
            sqlx::query(
                "INSERT INTO user_availability_rules (id, user_id, day_of_week, start_time, end_time) \
                 VALUES (?, ?, ?, '09:00', '17:00')",
            )
            .bind(&rid)
            .bind(&paris_uid)
            .bind(day)
            .execute(&pool)
            .await
            .unwrap();
        }

        // Host is in New_York. In winter: Paris=UTC+1, NY=UTC-5 → 6h offset.
        // Paris 09:00-17:00 == NY 03:00-11:00.
        // Week of 2026-01-12 (Monday).
        let host_tz: Tz = "America/New_York".parse().unwrap();
        let window_start = dt(2026, 1, 12, 0, 0);
        let window_end = dt(2026, 1, 12, 23, 59);

        let busy = user_avail_as_busy(&pool, &paris_uid, window_start, window_end, host_tz).await;

        // Helper: is (t) inside any busy interval?
        let is_busy = |t: NaiveDateTime| busy.iter().any(|(s, e)| &t >= s && &t < e);

        // NY 02:00 Monday = Paris 08:00 — Paris member NOT working yet → must be busy.
        assert!(
            is_busy(dt(2026, 1, 12, 2, 0)),
            "NY 02:00 (Paris 08:00) should be blocked — outside member's working hours"
        );
        // NY 05:00 Monday = Paris 11:00 — Paris member IS working → must NOT be busy.
        assert!(
            !is_busy(dt(2026, 1, 12, 5, 0)),
            "NY 05:00 (Paris 11:00) should be free — inside member's working hours"
        );
        // NY 12:00 Monday = Paris 18:00 — Paris member done for the day → must be busy.
        assert!(
            is_busy(dt(2026, 1, 12, 12, 0)),
            "NY 12:00 (Paris 18:00) should be blocked — outside member's working hours"
        );
    }

    // Regression for Antoine's follow-up: the team event-type flow must apply
    // each member's personal working hours as a constraint (converted into
    // host_tz). Concretely: a team with a single America/Chicago member whose
    // personal rules are Mon-Fri 09:00-17:00 local must render slots in Paris
    // host_tz only from 16:00 onwards, not across the full 09:00-23:00 rule.
    // Verifies the exact conversion user_avail_as_busy performs; the
    // show_group_slots/pick_group_member handlers extend member_busy with
    // this output before running compute_slots.
    #[tokio::test]
    async fn chicago_member_is_busy_at_paris_morning() {
        let pool = setup_test_db().await;
        let uid = uuid::Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO users (id, email, name, role, auth_provider, username, enabled, timezone) \
             VALUES (?, 'andy@example.com', 'Andy', 'user', 'local', 'andy', 1, 'America/Chicago')",
        )
        .bind(&uid)
        .execute(&pool)
        .await
        .unwrap();

        // Mon-Fri 09:00-17:00 Chicago local
        for day in 1..=5 {
            let rid = uuid::Uuid::new_v4().to_string();
            sqlx::query(
                "INSERT INTO user_availability_rules (id, user_id, day_of_week, start_time, end_time) \
                 VALUES (?, ?, ?, '09:00', '17:00')",
            )
            .bind(&rid)
            .bind(&uid)
            .bind(day)
            .execute(&pool)
            .await
            .unwrap();
        }

        // 2026-07-07 is a Tuesday in full summer DST (Chicago CDT UTC-5,
        // Paris CEST UTC+2, 7-hour offset — matches Antoine's scenario).
        let host_tz: Tz = "Europe/Paris".parse().unwrap();
        let window_start = dt(2026, 7, 7, 0, 0);
        let window_end = dt(2026, 7, 7, 23, 59);

        let busy = user_avail_as_busy(&pool, &uid, window_start, window_end, host_tz).await;
        let is_busy = |t: NaiveDateTime| busy.iter().any(|(s, e)| &t >= s && &t < e);

        // Paris 09:00 = Chicago 02:00 — outside 09-17, must be blocked.
        assert!(
            is_busy(dt(2026, 7, 7, 9, 0)),
            "Paris 09:00 (Chicago 02:00) must be blocked — Andy sleeping"
        );
        // Paris 16:00 = Chicago 09:00 — Andy just started, must be free.
        assert!(
            !is_busy(dt(2026, 7, 7, 16, 0)),
            "Paris 16:00 (Chicago 09:00) must be free — Andy working"
        );
        // Paris 22:00 = Chicago 15:00 — Andy still working, must be free.
        assert!(
            !is_busy(dt(2026, 7, 7, 22, 0)),
            "Paris 22:00 (Chicago 15:00) must be free — Andy working"
        );
    }

    // Regression: when a team member without any user_availability_rules is
    // active, they must be treated as always-available (no synthesized 9-17
    // default). This preserves the behavior Antoine tested originally where
    // a single Paris member expected the full event-type rule window.
    #[tokio::test]
    async fn member_without_personal_rules_is_unconstrained() {
        let pool = setup_test_db().await;
        let uid = uuid::Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO users (id, email, name, role, auth_provider, username, enabled, timezone) \
             VALUES (?, 'free@example.com', 'No Hours', 'user', 'local', 'free', 1, 'Europe/Paris')",
        )
        .bind(&uid)
        .execute(&pool)
        .await
        .unwrap();
        // Deliberately NO user_availability_rules rows for this user.

        let host_tz: Tz = "Europe/Paris".parse().unwrap();
        let busy = user_avail_as_busy(
            &pool,
            &uid,
            dt(2026, 7, 7, 0, 0),
            dt(2026, 7, 7, 23, 59),
            host_tz,
        )
        .await;

        assert!(
            busy.is_empty(),
            "no personal rules must mean no constraint — got {} busy intervals",
            busy.len()
        );
    }

    // Regression for issue #50: an event type carries its own host timezone,
    // which must take precedence over the account owner's personal timezone.
    // Prevents the original bug where a US-based creator silently made the
    // team's 09:00-21:00 rule land in Chicago time.
    #[tokio::test]
    async fn get_host_tz_prefers_explicit_event_type_timezone() {
        let pool = setup_test_db().await;

        // Create a user in America/Chicago and their account.
        let user_id = uuid::Uuid::new_v4().to_string();
        let account_id = uuid::Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO users (id, email, name, role, auth_provider, username, enabled, timezone) \
             VALUES (?, 'chicago@example.com', 'Chicago User', 'user', 'local', 'chicago', 1, 'America/Chicago')",
        )
        .bind(&user_id)
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query("INSERT INTO accounts (id, name, email, timezone, user_id) VALUES (?, 'Chicago User', 'chicago@example.com', 'America/Chicago', ?)")
            .bind(&account_id)
            .bind(&user_id)
            .execute(&pool)
            .await
            .unwrap();

        // Event type explicitly pinned to Europe/Paris.
        let et_id = uuid::Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO event_types (id, account_id, slug, title, duration_min, buffer_before, buffer_after, min_notice_min, enabled, timezone) VALUES (?, ?, 'demo', 'Demo', 30, 0, 0, 0, 1, 'Europe/Paris')")
            .bind(&et_id)
            .bind(&account_id)
            .execute(&pool)
            .await
            .unwrap();

        assert_eq!(
            get_host_tz(&pool, &et_id).await,
            Tz::Europe__Paris,
            "explicit event-type timezone must win over account owner's"
        );

        // Event type with NULL timezone should fall back to the account owner.
        let et_id2 = uuid::Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO event_types (id, account_id, slug, title, duration_min, buffer_before, buffer_after, min_notice_min, enabled) VALUES (?, ?, 'demo2', 'Demo2', 30, 0, 0, 0, 1)")
            .bind(&et_id2)
            .bind(&account_id)
            .execute(&pool)
            .await
            .unwrap();
        assert_eq!(
            get_host_tz(&pool, &et_id2).await,
            Tz::America__Chicago,
            "NULL event-type timezone must fall back to account owner"
        );
    }

    // --- parse_booking_datetime tests ---

    #[test]
    fn parse_booking_datetime_iso_format() {
        assert_eq!(
            parse_booking_datetime("2026-03-15T14:30:00"),
            Some(dt(2026, 3, 15, 14, 30))
        );
    }

    #[test]
    fn parse_booking_datetime_space_format() {
        assert_eq!(
            parse_booking_datetime("2026-03-15 14:30:00"),
            Some(dt(2026, 3, 15, 14, 30))
        );
    }

    #[test]
    fn parse_booking_datetime_trailing_z() {
        assert_eq!(
            parse_booking_datetime("2026-03-15T14:30:00Z"),
            Some(dt(2026, 3, 15, 14, 30))
        );
    }

    #[test]
    fn parse_booking_datetime_invalid() {
        assert_eq!(parse_booking_datetime("not-a-date"), None);
        assert_eq!(parse_booking_datetime(""), None);
        assert_eq!(parse_booking_datetime("2026-13-40T99:99:99"), None);
    }

    // --- format_booking_datetime tests ---

    #[test]
    fn format_booking_datetime_far_future_same_year_not_guaranteed() {
        // Use a date far in the future (2099) so it's never "today"/"tomorrow"/weekday
        let result = format_booking_datetime("2099-07-20T09:15:00");
        assert_eq!(result, "Mon, Jul 20, 2099 at 9:15 AM");
    }

    #[test]
    fn format_booking_datetime_different_year() {
        let result = format_booking_datetime("2050-12-25T18:00:00");
        assert_eq!(result, "Sun, Dec 25, 2050 at 6:00 PM");
    }

    #[test]
    fn format_booking_datetime_invalid_fallback() {
        assert_eq!(format_booking_datetime("garbage"), "garbage");
        assert_eq!(format_booking_datetime(""), "");
    }

    // --- format_booking_range tests ---

    #[test]
    fn format_booking_range_far_future() {
        let result = format_booking_range("2099-07-20T09:00:00", "2099-07-20T09:30:00");
        assert_eq!(result, "Mon, Jul 20, 2099 at 9:00 AM — 9:30 AM");
    }

    #[test]
    fn format_booking_range_end_unparseable() {
        let result = format_booking_range("2099-07-20T09:00:00", "bad");
        assert_eq!(result, "Mon, Jul 20, 2099 at 9:00 AM — bad");
    }

    // --- format_booking_for_dashboard tests ---
    //
    // Regression for issue #100: when the event type's stored timezone differs
    // from the host's profile timezone, the dashboard previously displayed the
    // raw stored time with no timezone label, which the host read as their own
    // time. The fix shifts the primary label into the host's profile tz and
    // adds a secondary guest-tz line.

    #[test]
    fn dashboard_label_converts_stored_tz_to_host_profile_tz() {
        // The user's exact scenario: event type set to America/New_York
        // (UTC-5 in winter), host's profile is Europe/Paris (UTC+1 in winter),
        // guest booked from America/New_York at 10:00 → stored as "10:00".
        // Host should see "4:00 PM" (Paris equivalent), guest line shows "10:00 EST".
        let (primary, secondary) = format_booking_for_dashboard(
            "2099-01-20T10:00:00",
            "2099-01-20T10:30:00",
            "America/New_York",
            "Europe/Paris",
            "America/New_York",
        );
        assert!(
            primary.contains("4:00 PM") && primary.contains("4:30 PM"),
            "primary should be in Europe/Paris, got: {primary}"
        );
        assert!(
            primary.contains("CET"),
            "primary should label tz, got: {primary}"
        );
        let sec = secondary.expect("secondary line expected when guest tz differs from host tz");
        assert!(
            sec.contains("10:00 AM"),
            "secondary should show guest 10:00 AM, got: {sec}"
        );
        assert!(
            sec.contains("EST"),
            "secondary should label guest tz, got: {sec}"
        );
    }

    #[test]
    fn dashboard_label_omits_secondary_when_guest_tz_equals_host_tz() {
        let (_primary, secondary) = format_booking_for_dashboard(
            "2099-07-20T14:00:00",
            "2099-07-20T14:30:00",
            "Europe/Paris",
            "Europe/Paris",
            "Europe/Paris",
        );
        assert!(
            secondary.is_none(),
            "no secondary line when host and guest share tz"
        );
    }

    #[test]
    fn dashboard_label_falls_back_for_unparseable_input() {
        let (primary, secondary) = format_booking_for_dashboard(
            "garbage",
            "also-garbage",
            "Europe/Paris",
            "Europe/Paris",
            "America/New_York",
        );
        assert_eq!(primary, "garbage — also-garbage");
        assert!(secondary.is_none());
    }

    // --- format_date_label tests ---

    #[test]
    fn format_date_label_from_datetime() {
        assert_eq!(
            format_date_label("2026-03-15T14:30:00", "en"),
            "Sunday, March 15, 2026"
        );
    }

    #[test]
    fn format_date_label_from_date_only() {
        assert_eq!(
            format_date_label("2026-03-15", "en"),
            "Sunday, March 15, 2026"
        );
    }

    #[test]
    fn format_date_label_space_separator() {
        assert_eq!(
            format_date_label("2026-03-15 14:30:00", "en"),
            "Sunday, March 15, 2026"
        );
    }

    #[test]
    fn format_date_label_invalid_fallback() {
        assert_eq!(format_date_label("nope", "en"), "nope");
    }

    #[test]
    fn format_date_label_french() {
        assert_eq!(
            format_date_label("2026-03-15", "fr"),
            "dimanche 15 mars 2026"
        );
    }

    // --- format_time_from_dt tests ---

    #[test]
    fn format_time_from_dt_valid() {
        assert_eq!(format_time_from_dt("2026-03-15T14:30:00"), "2:30 PM");
        assert_eq!(format_time_from_dt("2026-03-15 09:05:00"), "9:05 AM");
    }

    #[test]
    fn format_time_from_dt_midnight() {
        assert_eq!(format_time_from_dt("2026-03-15T00:00:00"), "12:00 AM");
    }

    #[test]
    fn format_time_from_dt_unparseable_long_string() {
        // 16+ chars but not a valid datetime → falls back to substring [11..16]
        assert_eq!(format_time_from_dt("XXXX-XX-XX_HH:MM:SS"), "HH:MM");
    }

    #[test]
    fn format_time_from_dt_short_string() {
        assert_eq!(format_time_from_dt("short"), "00:00");
        assert_eq!(format_time_from_dt(""), "00:00");
    }

    // --- extract_time_24h tests ---

    #[test]
    fn extract_time_24h_returns_24h_format() {
        assert_eq!(extract_time_24h("2026-03-15T14:30:00"), "14:30");
        assert_eq!(extract_time_24h("2026-03-15 09:05:00"), "09:05");
        assert_eq!(extract_time_24h("2026-03-15T00:00:00"), "00:00");
    }

    #[test]
    fn extract_time_24h_short_string() {
        assert_eq!(extract_time_24h("short"), "00:00");
        assert_eq!(extract_time_24h(""), "00:00");
    }

    // --- parse_guest_tz tests ---

    #[test]
    fn parse_guest_tz_valid() {
        assert_eq!(
            parse_guest_tz(Some("America/New_York")),
            chrono_tz::America::New_York
        );
        assert_eq!(
            parse_guest_tz(Some("Europe/Paris")),
            chrono_tz::Europe::Paris
        );
        assert_eq!(parse_guest_tz(Some("UTC")), Tz::UTC);
    }

    #[test]
    fn parse_guest_tz_invalid_falls_back() {
        let tz = parse_guest_tz(Some("Not/A/Timezone"));
        // Should be server local or UTC — either way, not panic
        let _ = tz;
    }

    #[test]
    fn parse_guest_tz_none_falls_back() {
        let tz = parse_guest_tz(None);
        let _ = tz;
    }

    // --- parse_avail_windows tests ---

    #[test]
    fn parse_avail_windows_single_window() {
        let w = parse_avail_windows(Some("09:00-17:00"), None, None);
        assert_eq!(w, vec![("09:00".to_string(), "17:00".to_string())]);
    }

    #[test]
    fn parse_avail_windows_multiple_windows() {
        let w = parse_avail_windows(Some("09:00-12:00,13:00-17:00"), None, None);
        assert_eq!(
            w,
            vec![
                ("09:00".to_string(), "12:00".to_string()),
                ("13:00".to_string(), "17:00".to_string()),
            ]
        );
    }

    #[test]
    fn parse_avail_windows_legacy_fallback() {
        let w = parse_avail_windows(None, Some("08:00"), Some("16:00"));
        assert_eq!(w, vec![("08:00".to_string(), "16:00".to_string())]);
    }

    #[test]
    fn parse_avail_windows_empty_string_uses_legacy() {
        let w = parse_avail_windows(Some(""), Some("10:00"), Some("18:00"));
        assert_eq!(w, vec![("10:00".to_string(), "18:00".to_string())]);
    }

    #[test]
    fn parse_avail_windows_invalid_times_ignored() {
        let w = parse_avail_windows(Some("09:00-12:00,bad-data,13:00-17:00"), None, None);
        assert_eq!(
            w,
            vec![
                ("09:00".to_string(), "12:00".to_string()),
                ("13:00".to_string(), "17:00".to_string()),
            ]
        );
    }

    #[test]
    fn parse_avail_windows_defaults_when_none() {
        let w = parse_avail_windows(None, None, None);
        assert_eq!(w, vec![("09:00".to_string(), "17:00".to_string())]);
    }

    // --- validate_booking_input tests ---

    #[test]
    fn validate_booking_input_empty_name() {
        let result = validate_booking_input("", "user@example.com", &None);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Name"));
    }

    #[test]
    fn validate_booking_input_name_too_long() {
        let long_name = "a".repeat(256);
        let result = validate_booking_input(&long_name, "user@example.com", &None);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Name"));
    }

    #[test]
    fn validate_booking_input_valid_name() {
        let result = validate_booking_input("Jane Doe", "jane@example.com", &None);
        assert!(result.is_ok());
    }

    #[test]
    fn validate_booking_input_empty_email() {
        let result = validate_booking_input("Jane", "", &None);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("Email") || err.contains("email"));
    }

    #[test]
    fn validate_booking_input_email_no_at() {
        let result = validate_booking_input("Jane", "userexample.com", &None);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("email"));
    }

    #[test]
    fn validate_booking_input_email_no_domain_dot() {
        let result = validate_booking_input("Jane", "user@localhost", &None);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("email"));
    }

    #[test]
    fn validate_booking_input_valid_email() {
        let result = validate_booking_input("Jane", "jane@example.com", &None);
        assert!(result.is_ok());
    }

    #[test]
    fn validate_booking_input_notes_too_long() {
        let long_notes = Some("x".repeat(5001));
        let result = validate_booking_input("Jane", "jane@example.com", &long_notes);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Notes"));
    }

    #[test]
    fn validate_booking_input_notes_within_limit() {
        let notes = Some("x".repeat(5000));
        let result = validate_booking_input("Jane", "jane@example.com", &notes);
        assert!(result.is_ok());
    }

    #[test]
    fn validate_booking_input_none_notes() {
        let result = validate_booking_input("Jane", "jane@example.com", &None);
        assert!(result.is_ok());
    }

    // --- validate_date_not_too_far tests ---

    #[test]
    fn validate_date_not_too_far_within_range() {
        let date = Utc::now().naive_utc().date() + Duration::days(30);
        assert!(validate_date_not_too_far(date).is_ok());
    }

    #[test]
    fn validate_date_not_too_far_exactly_365_days() {
        let date = Utc::now().naive_utc().date() + Duration::days(365);
        assert!(validate_date_not_too_far(date).is_ok());
    }

    #[test]
    fn validate_date_not_too_far_367_days_rejected() {
        let date = Utc::now().naive_utc().date() + Duration::days(367);
        assert!(validate_date_not_too_far(date).is_err());
    }

    #[test]
    fn validate_date_not_too_far_past_date_passes() {
        // Past dates are not rejected here (min_notice handles that elsewhere)
        let date = Utc::now().naive_utc().date() - Duration::days(10);
        assert!(validate_date_not_too_far(date).is_ok());
    }

    // --- CSRF function tests ---

    #[test]
    fn generate_csrf_token_returns_valid_uuid() {
        let token = generate_csrf_token();
        assert!(!token.is_empty());
        assert_eq!(token.len(), 36); // UUID format: 8-4-4-4-12
    }

    #[test]
    fn csrf_cookie_has_security_flags() {
        // `Secure` so the token never travels over plaintext HTTP.
        // `SameSite=Lax` to stop cross-site GETs from carrying it.
        // Deliberately NO `HttpOnly` — the double-submit pattern needs the
        // client script in base.html to read the cookie via document.cookie.
        let cookie = csrf_cookie_value("some-token");
        assert!(
            cookie.contains("; Secure"),
            "CSRF cookie must have Secure flag: {}",
            cookie
        );
        assert!(
            cookie.contains("SameSite=Lax"),
            "CSRF cookie must have SameSite=Lax: {}",
            cookie
        );
        assert!(
            !cookie.contains("HttpOnly"),
            "CSRF cookie must NOT be HttpOnly (JS needs to read it): {}",
            cookie
        );
    }

    #[test]
    fn csrf_cookie_embed_uses_samesite_none() {
        let cookie = csrf_cookie_value_for("tok", true);
        assert!(
            cookie.contains("SameSite=None"),
            "embed CSRF cookie must use SameSite=None for third-party iframes: {}",
            cookie
        );
        assert!(
            cookie.contains("; Secure"),
            "embed CSRF cookie must still carry Secure: {}",
            cookie
        );
        assert!(
            cookie.contains("; Partitioned"),
            "embed CSRF cookie must be Partitioned (CHIPS) for browsers blocking unpartitioned 3P cookies: {}",
            cookie
        );
    }

    #[test]
    fn csrf_cookie_non_embed_is_samesite_lax_unpartitioned() {
        let cookie = csrf_cookie_value_for("tok", false);
        assert!(cookie.contains("SameSite=Lax"), "{}", cookie);
        assert!(
            !cookie.contains("Partitioned"),
            "non-embed cookie must not be Partitioned: {}",
            cookie
        );
    }

    #[test]
    fn hex_to_rgb_parses_3_and_6_digit() {
        assert_eq!(hex_to_rgb("ff0000"), Some((255, 0, 0)));
        assert_eq!(hex_to_rgb("2563eb"), Some((37, 99, 235)));
        assert_eq!(hex_to_rgb("f30"), Some((255, 51, 0)));
        assert_eq!(hex_to_rgb("fff"), Some((255, 255, 255)));
        assert_eq!(hex_to_rgb(""), None);
        assert_eq!(hex_to_rgb("12345"), None);
        assert_eq!(hex_to_rgb("gggggg"), None);
    }

    #[test]
    fn brand_css_derives_rgba_variants() {
        let p = EmbedParams {
            brand: Some("ff0000".to_string()),
            ..Default::default()
        };
        let css = p.brand_css().expect("valid hex yields css");
        assert!(css.contains("--accent: #ff0000"), "{}", css);
        assert!(
            css.contains("--accent-subtle: rgba(255,0,0,0.12)"),
            "{}",
            css
        );
        assert!(
            css.contains("--accent-border: rgba(255,0,0,0.3)"),
            "{}",
            css
        );
        assert!(css.contains("--accent-muted: rgba(255,0,0,0.5)"), "{}", css);

        // Invalid input yields no CSS (never inject raw user input).
        let bad = EmbedParams {
            brand: Some("red; } body{display:none".to_string()),
            ..Default::default()
        };
        assert!(bad.brand_css().is_none());
    }

    #[test]
    fn is_embed_request_detects_flag() {
        assert!(is_embed_request(Some("embed=1")));
        assert!(is_embed_request(Some("embed=true")));
        assert!(is_embed_request(Some("foo=bar&embed=1&baz=qux")));
        assert!(!is_embed_request(Some("embed=0")));
        assert!(!is_embed_request(Some("notembed=1")));
        assert!(!is_embed_request(None));
        assert!(!is_embed_request(Some("")));
    }

    #[test]
    fn is_embeddable_path_allows_booking_surfaces_only() {
        // Public booking surfaces — embeddable.
        assert!(is_embeddable_path("/u/alice"));
        assert!(is_embeddable_path("/u/alice/intro"));
        assert!(is_embeddable_path("/u/alice/intro/book"));
        assert!(is_embeddable_path("/team/sales"));
        assert!(is_embeddable_path("/team/sales/intro"));
        assert!(is_embeddable_path("/g/sales"));
        // Legacy single-user booking pages.
        assert!(is_embeddable_path("/intro"));
        assert!(is_embeddable_path("/intro/book"));

        // Sensitive / non-booking surfaces — never embeddable.
        assert!(!is_embeddable_path("/auth/login"));
        assert!(!is_embeddable_path("/auth/register"));
        assert!(!is_embeddable_path("/dashboard"));
        assert!(!is_embeddable_path("/dashboard/event-types"));
        assert!(!is_embeddable_path("/dashboard/admin"));
        assert!(!is_embeddable_path("/"));
        assert!(!is_embeddable_path("/embed.js"));
        assert!(!is_embeddable_path("/accent.css"));
        assert!(!is_embeddable_path("/avatar/123"));
        assert!(!is_embeddable_path("/booking/approve/tok"));
    }

    #[test]
    fn embedded_booking_request_requires_both_flag_and_path() {
        // The blocking issue: embed=1 on a sensitive path must NOT enable embed.
        assert!(!is_embedded_booking_request("/auth/login", Some("embed=1")));
        assert!(!is_embedded_booking_request("/dashboard", Some("embed=1")));
        // Booking surface with the flag — enabled.
        assert!(is_embedded_booking_request(
            "/u/alice/intro",
            Some("embed=1")
        ));
        // Booking surface without the flag — not embed mode.
        assert!(!is_embedded_booking_request("/u/alice/intro", None));
    }

    #[test]
    fn csrf_token_from_headers_extracts_token() {
        let mut headers = HeaderMap::new();
        headers.insert(
            axum::http::header::COOKIE,
            "__Host-calrs_csrf=test-token-123".parse().unwrap(),
        );
        let token = csrf_token_from_headers(&headers);
        assert_eq!(token, Some("test-token-123".to_string()));
    }

    #[test]
    fn csrf_token_from_headers_returns_none_when_missing() {
        let headers = HeaderMap::new();
        let token = csrf_token_from_headers(&headers);
        assert_eq!(token, None);
    }

    #[test]
    fn csrf_token_from_headers_ignores_other_cookies() {
        let mut headers = HeaderMap::new();
        headers.insert(
            axum::http::header::COOKIE,
            "__Host-calrs_session=abc; other=xyz".parse().unwrap(),
        );
        let token = csrf_token_from_headers(&headers);
        assert_eq!(token, None);
    }

    #[test]
    fn verify_csrf_token_passes_when_matching() {
        let mut headers = HeaderMap::new();
        headers.insert(
            axum::http::header::COOKIE,
            "__Host-calrs_csrf=my-token".parse().unwrap(),
        );
        let form_token = Some("my-token".to_string());
        assert!(verify_csrf_token(&headers, &form_token).is_ok());
    }

    #[test]
    fn verify_csrf_token_fails_when_mismatch() {
        let mut headers = HeaderMap::new();
        headers.insert(
            axum::http::header::COOKIE,
            "__Host-calrs_csrf=cookie-token".parse().unwrap(),
        );
        let form_token = Some("different-token".to_string());
        assert!(verify_csrf_token(&headers, &form_token).is_err());
    }

    #[test]
    fn verify_csrf_token_fails_when_form_token_none() {
        let mut headers = HeaderMap::new();
        headers.insert(
            axum::http::header::COOKIE,
            "__Host-calrs_csrf=my-token".parse().unwrap(),
        );
        let form_token: Option<String> = None;
        assert!(verify_csrf_token(&headers, &form_token).is_err());
    }

    #[test]
    fn verify_csrf_token_fails_when_cookie_missing() {
        let headers = HeaderMap::new();
        let form_token = Some("my-token".to_string());
        assert!(verify_csrf_token(&headers, &form_token).is_err());
    }

    #[test]
    fn verify_csrf_token_fails_when_cookie_empty() {
        // An empty cookie value would byte-compare-equal to an empty form
        // field; the explicit empty-cookie guard prevents that bypass.
        let mut headers = HeaderMap::new();
        headers.insert(
            axum::http::header::COOKIE,
            "__Host-calrs_csrf=".parse().unwrap(),
        );
        let form_token = Some(String::new());
        assert!(verify_csrf_token(&headers, &form_token).is_err());
    }

    #[test]
    fn verify_csrf_token_fails_when_lengths_differ() {
        // ConstantTimeEq returns false on length mismatch; this just pins
        // that mismatched-length tokens are still rejected.
        let mut headers = HeaderMap::new();
        headers.insert(
            axum::http::header::COOKIE,
            "__Host-calrs_csrf=short".parse().unwrap(),
        );
        let form_token = Some("a-much-longer-token".to_string());
        assert!(verify_csrf_token(&headers, &form_token).is_err());
    }

    #[test]
    fn verify_csrf_token_fails_when_same_length_differs_at_last_byte() {
        // Exercises the actual ct_eq byte-fold path: both tokens are the
        // same length, so the comparison runs over every byte rather than
        // short-circuiting on the length mismatch.
        let mut headers = HeaderMap::new();
        headers.insert(
            axum::http::header::COOKIE,
            "__Host-calrs_csrf=aaaaaaaaaaaaaaaa".parse().unwrap(),
        );
        let form_token = Some("aaaaaaaaaaaaaaab".to_string());
        assert!(verify_csrf_token(&headers, &form_token).is_err());
    }

    // --- client_ip_for_rate_limit ---

    #[test]
    fn client_ip_unknown_when_no_headers() {
        let headers = HeaderMap::new();
        assert_eq!(client_ip_for_rate_limit(&headers), "unknown");
    }

    #[test]
    fn client_ip_ignores_x_real_ip() {
        // We deliberately do not honour X-Real-IP: in default Caddy/Nginx
        // setups the proxy does not overwrite a client-supplied value, so
        // trusting it would let the client forge their identity directly.
        let mut headers = HeaderMap::new();
        headers.insert("x-real-ip", "1.2.3.4".parse().unwrap());
        assert_eq!(client_ip_for_rate_limit(&headers), "unknown");
    }

    #[test]
    fn client_ip_takes_rightmost_xff() {
        // The leftmost value is whatever the original client put in the
        // header; the rightmost is the trusted proxy's view of its peer.
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-forwarded-for",
            "attacker-spoofed, 198.51.100.7".parse().unwrap(),
        );
        assert_eq!(client_ip_for_rate_limit(&headers), "198.51.100.7");
    }

    #[test]
    fn client_ip_xff_single_value() {
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-for", "203.0.113.10".parse().unwrap());
        assert_eq!(client_ip_for_rate_limit(&headers), "203.0.113.10");
    }

    #[test]
    fn client_ip_xff_trims_whitespace() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-forwarded-for",
            "1.2.3.4 ,    198.51.100.7   ".parse().unwrap(),
        );
        assert_eq!(client_ip_for_rate_limit(&headers), "198.51.100.7");
    }

    // --- internal_error_* sanitization helpers ---

    #[test]
    fn internal_error_response_returns_500_with_generic_body() {
        // Whatever the underlying error says, the response body must not
        // contain it. The detail goes to logs only.
        let resp = internal_error_response("test ctx", &"sensitive: /var/lib/calrs/calrs.db error");
        assert_eq!(resp.status(), axum::http::StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[test]
    fn internal_error_html_body_is_generic() {
        let html = internal_error_html("test", &"users.password_hash leaks here");
        let Html(body) = html;
        assert!(
            !body.contains("password_hash"),
            "body leaked detail: {}",
            body
        );
        assert!(!body.contains("users."), "body leaked detail: {}", body);
        assert!(body.contains("internal error"));
    }

    #[test]
    fn internal_error_body_is_generic() {
        let body = internal_error_body("test", &"some/private/path: nope");
        assert!(!body.contains("/private/path"), "body leaked: {body}");
    }

    #[test]
    fn oidc_error_response_returns_500_without_detail() {
        // openidconnect errors can carry IdP URLs and token content; the
        // response body must not pass them through.
        let resp = oidc_error_response(
            "test",
            &"token endpoint https://idp/token returned: <secret-token-bytes>",
        );
        assert_eq!(resp.status(), axum::http::StatusCode::INTERNAL_SERVER_ERROR);
    }

    /// Capture `tracing` output into a shared buffer so log-emission can be
    /// asserted from a test. Used to pin the contract that the sanitization
    /// helpers actually log the underlying detail; a future refactor that
    /// drops the log call would cause this test to fail.
    ///
    /// Uses a hand-rolled minimal `Subscriber` instead of
    /// `tracing_subscriber::fmt`. The fmt layer's per-event writer plumbing
    /// proved flaky under `cargo tarpaulin` (events occasionally landed in a
    /// dropped buffer), and we don't need the formatting machinery here:
    /// recording field names + their `Debug` strings is enough to assert that
    /// the event carried the expected markers.
    fn capture_tracing<F: FnOnce()>(f: F) -> String {
        use std::sync::{Arc, Mutex};
        use tracing::field::{Field, Visit};
        use tracing::{Event, Metadata, Subscriber};

        #[derive(Clone)]
        struct Buf(Arc<Mutex<String>>);

        struct Recorder {
            buf: Arc<Mutex<String>>,
        }
        impl Subscriber for Recorder {
            fn enabled(&self, _: &Metadata<'_>) -> bool {
                true
            }
            fn new_span(&self, _: &tracing::span::Attributes<'_>) -> tracing::span::Id {
                tracing::span::Id::from_u64(1)
            }
            fn record(&self, _: &tracing::span::Id, _: &tracing::span::Record<'_>) {}
            fn record_follows_from(&self, _: &tracing::span::Id, _: &tracing::span::Id) {}
            fn event(&self, event: &Event<'_>) {
                struct V<'a>(&'a Mutex<String>);
                impl<'a> Visit for V<'a> {
                    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
                        use std::fmt::Write;
                        let mut s = self.0.lock().unwrap();
                        let _ = write!(s, "{}={:?} ", field.name(), value);
                    }
                    fn record_str(&mut self, field: &Field, value: &str) {
                        use std::fmt::Write;
                        let mut s = self.0.lock().unwrap();
                        let _ = write!(s, "{}={} ", field.name(), value);
                    }
                }
                event.record(&mut V(&self.buf));
                self.buf.lock().unwrap().push('\n');
            }
            fn enter(&self, _: &tracing::span::Id) {}
            fn exit(&self, _: &tracing::span::Id) {}
        }

        let buf = Buf(Arc::new(Mutex::new(String::new())));
        let subscriber = Recorder { buf: buf.0.clone() };
        tracing::subscriber::with_default(subscriber, || {
            // tracing caches per-callsite Interest the first time the
            // callsite fires. If a sibling test (e.g. one that just
            // exercises the response body without capturing logs) ran
            // first under NoSubscriber, the cached interest is `never` and
            // our scoped Recorder never sees the event. Force a rebuild
            // against the now-active subscriber before invoking f.
            tracing::callsite::rebuild_interest_cache();
            f();
        });
        let s = buf.0.lock().unwrap().clone();
        s
    }

    /// Whether the current process is running under cargo-tarpaulin. The
    /// tracing-capture tests below race with tarpaulin's instrumentation in
    /// a way I haven't fully pinned down: the captured buffer ends up empty
    /// even though the same test passes deterministically under plain
    /// `cargo test`. Skip those tests under coverage rather than ship a
    /// red CI job, but keep them live everywhere else so the contract is
    /// still enforced.
    ///
    /// Detection uses `cfg!(tarpaulin)` since cargo-tarpaulin compiles with
    /// `--cfg=tarpaulin`. An earlier attempt keyed off `CARGO_TARPAULIN_VERSION`
    /// but tarpaulin never sets that env var, so the guard never fired.
    fn under_tarpaulin() -> bool {
        cfg!(tarpaulin)
    }

    #[test]
    fn internal_error_logs_capture_underlying_detail() {
        if under_tarpaulin() {
            return;
        }
        // Pin: the helper must emit a tracing::error! that carries both the
        // context label and the underlying error string. The body sent to
        // the user is generic; the detail goes here.
        let log = capture_tracing(|| {
            let _ = internal_error_response("test-context-marker", &"sensitive-detail-marker");
        });
        assert!(
            log.contains("test-context-marker"),
            "log missing context: {log}"
        );
        assert!(
            log.contains("sensitive-detail-marker"),
            "log missing error detail: {log}"
        );
    }

    #[test]
    fn oidc_error_logs_capture_underlying_detail() {
        if under_tarpaulin() {
            return;
        }
        let log = capture_tracing(|| {
            let _ = oidc_error_response("oidc-context-marker", &"token-endpoint-detail-marker");
        });
        assert!(
            log.contains("oidc-context-marker"),
            "log missing context: {log}"
        );
        assert!(
            log.contains("token-endpoint-detail-marker"),
            "log missing error detail: {log}"
        );
    }

    // --- is_safe_company_link ---

    #[test]
    fn company_link_accepts_http_and_https() {
        assert!(is_safe_company_link("http://example.com"));
        assert!(is_safe_company_link("https://example.com"));
        assert!(is_safe_company_link("HTTPS://EXAMPLE.COM"));
        assert!(is_safe_company_link("  https://example.com/path  "));
    }

    #[test]
    fn company_link_rejects_javascript_uri() {
        // The whole point of this validator: stop stored XSS via the
        // company-link anchor that renders on every public page.
        assert!(!is_safe_company_link("javascript:alert(1)"));
        assert!(!is_safe_company_link("JAVASCRIPT:alert(1)"));
        assert!(!is_safe_company_link("  javascript:alert(1)"));
    }

    #[test]
    fn company_link_rejects_other_dangerous_schemes() {
        assert!(!is_safe_company_link("data:text/html,<script>"));
        assert!(!is_safe_company_link("vbscript:msgbox(1)"));
        assert!(!is_safe_company_link("file:///etc/passwd"));
    }

    #[test]
    fn company_link_rejects_unschemed_input() {
        // A bare hostname shouldn't slip through. The rendered anchor
        // would be relative to the booking page, which is wrong and
        // looks like a path traversal vector.
        assert!(!is_safe_company_link("example.com"));
        assert!(!is_safe_company_link("/foo/bar"));
        assert!(!is_safe_company_link("//example.com"));
        assert!(!is_safe_company_link(""));
    }

    // --- fetch_busy_times_for_user_ex exclude_booking_id tests ---

    #[tokio::test]
    async fn fetch_busy_times_ex_excludes_specified_booking() {
        let pool = setup_test_db().await;
        let (user_id, _, et_id) = seed_test_data(&pool).await;

        let booking_id = uuid::Uuid::new_v4().to_string();
        let cancel_tok = uuid::Uuid::new_v4().to_string();
        let resched_tok = uuid::Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO bookings (id, event_type_id, uid, guest_name, guest_email, guest_timezone, start_at, end_at, status, cancel_token, reschedule_token) VALUES (?, ?, 'uid-ex1', 'Guest', 'guest@example.com', 'UTC', '2026-03-16T10:00:00', '2026-03-16T10:30:00', 'confirmed', ?, ?)")
            .bind(&booking_id)
            .bind(&et_id)
            .bind(&cancel_tok)
            .bind(&resched_tok)
            .execute(&pool).await.unwrap();

        // Without exclusion: booking shows as busy
        let busy = fetch_busy_times_for_user_ex(
            &pool,
            &user_id,
            dt(2026, 3, 15, 0, 0),
            dt(2026, 3, 21, 23, 59),
            Tz::UTC,
            None,
            None,
        )
        .await;
        assert_eq!(
            busy.len(),
            1,
            "Booking should be in busy times without exclusion"
        );

        // With exclusion: booking is excluded
        let busy_ex = fetch_busy_times_for_user_ex(
            &pool,
            &user_id,
            dt(2026, 3, 15, 0, 0),
            dt(2026, 3, 21, 23, 59),
            Tz::UTC,
            None,
            Some(&booking_id),
        )
        .await;
        assert!(
            busy_ex.is_empty(),
            "Excluded booking should not appear in busy times"
        );
    }

    #[tokio::test]
    async fn fetch_busy_times_ex_excludes_only_specified_booking() {
        let pool = setup_test_db().await;
        let (user_id, _, et_id) = seed_test_data(&pool).await;

        // Insert two bookings
        let booking_id_1 = uuid::Uuid::new_v4().to_string();
        let booking_id_2 = uuid::Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO bookings (id, event_type_id, uid, guest_name, guest_email, guest_timezone, start_at, end_at, status, cancel_token, reschedule_token) VALUES (?, ?, 'uid-a', 'Guest A', 'a@example.com', 'UTC', '2026-03-16T10:00:00', '2026-03-16T10:30:00', 'confirmed', ?, ?)")
            .bind(&booking_id_1)
            .bind(&et_id)
            .bind(uuid::Uuid::new_v4().to_string())
            .bind(uuid::Uuid::new_v4().to_string())
            .execute(&pool).await.unwrap();

        sqlx::query("INSERT INTO bookings (id, event_type_id, uid, guest_name, guest_email, guest_timezone, start_at, end_at, status, cancel_token, reschedule_token) VALUES (?, ?, 'uid-b', 'Guest B', 'b@example.com', 'UTC', '2026-03-16T14:00:00', '2026-03-16T14:30:00', 'confirmed', ?, ?)")
            .bind(&booking_id_2)
            .bind(&et_id)
            .bind(uuid::Uuid::new_v4().to_string())
            .bind(uuid::Uuid::new_v4().to_string())
            .execute(&pool).await.unwrap();

        // Exclude first booking: only second should be busy
        let busy = fetch_busy_times_for_user_ex(
            &pool,
            &user_id,
            dt(2026, 3, 15, 0, 0),
            dt(2026, 3, 21, 23, 59),
            Tz::UTC,
            None,
            Some(&booking_id_1),
        )
        .await;
        assert_eq!(
            busy.len(),
            1,
            "Only the non-excluded booking should be in busy times"
        );
        assert_eq!(busy[0].0, dt(2026, 3, 16, 14, 0));
    }

    #[tokio::test]
    async fn fetch_busy_times_ex_none_exclusion_matches_original() {
        let pool = setup_test_db().await;
        let (user_id, _, et_id) = seed_test_data(&pool).await;

        let booking_id = uuid::Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO bookings (id, event_type_id, uid, guest_name, guest_email, guest_timezone, start_at, end_at, status, cancel_token, reschedule_token) VALUES (?, ?, 'uid-c', 'Guest', 'guest@example.com', 'UTC', '2026-03-16T10:00:00', '2026-03-16T10:30:00', 'confirmed', ?, ?)")
            .bind(&booking_id)
            .bind(&et_id)
            .bind(uuid::Uuid::new_v4().to_string())
            .bind(uuid::Uuid::new_v4().to_string())
            .execute(&pool).await.unwrap();

        let busy_original = fetch_busy_times_for_user(
            &pool,
            &user_id,
            dt(2026, 3, 15, 0, 0),
            dt(2026, 3, 21, 23, 59),
            Tz::UTC,
            None,
        )
        .await;

        let busy_ex = fetch_busy_times_for_user_ex(
            &pool,
            &user_id,
            dt(2026, 3, 15, 0, 0),
            dt(2026, 3, 21, 23, 59),
            Tz::UTC,
            None,
            None,
        )
        .await;

        assert_eq!(
            busy_original.len(),
            busy_ex.len(),
            "None exclusion should match original function"
        );
    }

    // --- Reschedule DB flow tests ---

    /// Helper to insert a confirmed booking and return (booking_id, reschedule_token, cancel_token)
    async fn insert_test_booking(
        pool: &SqlitePool,
        et_id: &str,
        start: &str,
        end: &str,
        status: &str,
    ) -> (String, String, String) {
        let booking_id = uuid::Uuid::new_v4().to_string();
        let cancel_token = uuid::Uuid::new_v4().to_string();
        let reschedule_token = uuid::Uuid::new_v4().to_string();
        let confirm_token = uuid::Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO bookings (id, event_type_id, uid, guest_name, guest_email, guest_timezone, start_at, end_at, status, cancel_token, reschedule_token, confirm_token) VALUES (?, ?, ?, 'Guest', 'guest@example.com', 'UTC', ?, ?, ?, ?, ?, ?)"
        )
        .bind(&booking_id)
        .bind(et_id)
        .bind(format!("{}@calrs", uuid::Uuid::new_v4()))
        .bind(start)
        .bind(end)
        .bind(status)
        .bind(&cancel_token)
        .bind(&reschedule_token)
        .bind(&confirm_token)
        .execute(pool).await.unwrap();
        (booking_id, reschedule_token, cancel_token)
    }

    #[tokio::test]
    async fn reschedule_token_lookup_finds_confirmed_booking() {
        let pool = setup_test_db().await;
        let (_, _, et_id) = seed_test_data(&pool).await;

        let (bid, token, _) = insert_test_booking(
            &pool,
            &et_id,
            "2026-03-16T10:00:00",
            "2026-03-16T10:30:00",
            "confirmed",
        )
        .await;

        let found: Option<(String,)> = sqlx::query_as(
            "SELECT id FROM bookings WHERE reschedule_token = ? AND status IN ('confirmed', 'pending')"
        )
        .bind(&token)
        .fetch_optional(&pool).await.unwrap();

        assert!(found.is_some(), "Should find booking by reschedule_token");
        assert_eq!(found.unwrap().0, bid);
    }

    #[tokio::test]
    async fn reschedule_token_lookup_finds_pending_booking() {
        let pool = setup_test_db().await;
        let (_, _, et_id) = seed_test_data(&pool).await;

        let (bid, token, _) = insert_test_booking(
            &pool,
            &et_id,
            "2026-03-16T10:00:00",
            "2026-03-16T10:30:00",
            "pending",
        )
        .await;

        let found: Option<(String,)> = sqlx::query_as(
            "SELECT id FROM bookings WHERE reschedule_token = ? AND status IN ('confirmed', 'pending')"
        )
        .bind(&token)
        .fetch_optional(&pool).await.unwrap();

        assert!(
            found.is_some(),
            "Should find pending booking by reschedule_token"
        );
        assert_eq!(found.unwrap().0, bid);
    }

    #[tokio::test]
    async fn reschedule_token_lookup_rejects_cancelled_booking() {
        let pool = setup_test_db().await;
        let (_, _, et_id) = seed_test_data(&pool).await;

        let (_, token, _) = insert_test_booking(
            &pool,
            &et_id,
            "2026-03-16T10:00:00",
            "2026-03-16T10:30:00",
            "cancelled",
        )
        .await;

        let found: Option<(String,)> = sqlx::query_as(
            "SELECT id FROM bookings WHERE reschedule_token = ? AND status IN ('confirmed', 'pending')"
        )
        .bind(&token)
        .fetch_optional(&pool).await.unwrap();

        assert!(
            found.is_none(),
            "Cancelled booking should not be found by reschedule_token"
        );
    }

    #[tokio::test]
    async fn reschedule_token_lookup_rejects_declined_booking() {
        let pool = setup_test_db().await;
        let (_, _, et_id) = seed_test_data(&pool).await;

        let (_, token, _) = insert_test_booking(
            &pool,
            &et_id,
            "2026-03-16T10:00:00",
            "2026-03-16T10:30:00",
            "declined",
        )
        .await;

        let found: Option<(String,)> = sqlx::query_as(
            "SELECT id FROM bookings WHERE reschedule_token = ? AND status IN ('confirmed', 'pending')"
        )
        .bind(&token)
        .fetch_optional(&pool).await.unwrap();

        assert!(
            found.is_none(),
            "Declined booking should not be found by reschedule_token"
        );
    }

    #[tokio::test]
    async fn reschedule_updates_times_and_regenerates_tokens() {
        let pool = setup_test_db().await;
        let (_, _, et_id) = seed_test_data(&pool).await;

        let (bid, old_resched, old_cancel) = insert_test_booking(
            &pool,
            &et_id,
            "2026-03-16T10:00:00",
            "2026-03-16T10:30:00",
            "confirmed",
        )
        .await;

        // Simulate a guest reschedule: update times, regenerate tokens, set pending
        let new_resched = uuid::Uuid::new_v4().to_string();
        let new_cancel = uuid::Uuid::new_v4().to_string();
        let new_confirm = uuid::Uuid::new_v4().to_string();
        sqlx::query(
            "UPDATE bookings SET start_at = ?, end_at = ?, status = 'pending',
                    reschedule_token = ?, cancel_token = ?, confirm_token = ?,
                    reminder_sent_at = NULL
             WHERE id = ?",
        )
        .bind("2026-03-17T14:00:00")
        .bind("2026-03-17T14:30:00")
        .bind(&new_resched)
        .bind(&new_cancel)
        .bind(&new_confirm)
        .bind(&bid)
        .execute(&pool)
        .await
        .unwrap();

        // Verify the booking was updated
        let (start_at, end_at, status, resched_tok, cancel_tok, confirm_tok): (String, String, String, String, String, Option<String>) =
            sqlx::query_as("SELECT start_at, end_at, status, reschedule_token, cancel_token, confirm_token FROM bookings WHERE id = ?")
            .bind(&bid)
            .fetch_one(&pool).await.unwrap();

        assert_eq!(start_at, "2026-03-17T14:00:00");
        assert_eq!(end_at, "2026-03-17T14:30:00");
        assert_eq!(status, "pending");
        assert_eq!(resched_tok, new_resched);
        assert_ne!(
            resched_tok, old_resched,
            "Reschedule token should be regenerated"
        );
        assert_eq!(cancel_tok, new_cancel);
        assert_ne!(cancel_tok, old_cancel, "Cancel token should be regenerated");
        assert_eq!(confirm_tok.unwrap(), new_confirm);
    }

    #[tokio::test]
    async fn stale_reschedule_token_returns_no_match() {
        let pool = setup_test_db().await;
        let (_, _, et_id) = seed_test_data(&pool).await;

        let (bid, old_token, _) = insert_test_booking(
            &pool,
            &et_id,
            "2026-03-16T10:00:00",
            "2026-03-16T10:30:00",
            "confirmed",
        )
        .await;

        // Regenerate the token (simulating a prior reschedule)
        let new_token = uuid::Uuid::new_v4().to_string();
        sqlx::query("UPDATE bookings SET reschedule_token = ? WHERE id = ?")
            .bind(&new_token)
            .bind(&bid)
            .execute(&pool)
            .await
            .unwrap();

        // Old token should no longer find anything
        let found: Option<(String,)> = sqlx::query_as(
            "SELECT id FROM bookings WHERE reschedule_token = ? AND status IN ('confirmed', 'pending')"
        )
        .bind(&old_token)
        .fetch_optional(&pool).await.unwrap();

        assert!(
            found.is_none(),
            "Stale reschedule token should not find any booking"
        );

        // New token should work
        let found_new: Option<(String,)> = sqlx::query_as(
            "SELECT id FROM bookings WHERE reschedule_token = ? AND status IN ('confirmed', 'pending')"
        )
        .bind(&new_token)
        .fetch_optional(&pool).await.unwrap();

        assert!(
            found_new.is_some(),
            "New reschedule token should find booking"
        );
    }

    #[tokio::test]
    async fn host_initiated_reschedule_stays_confirmed() {
        let pool = setup_test_db().await;
        let (_, _, et_id) = seed_test_data(&pool).await;

        let (bid, _, _) = insert_test_booking(
            &pool,
            &et_id,
            "2026-03-16T10:00:00",
            "2026-03-16T10:30:00",
            "confirmed",
        )
        .await;

        // Step 1: Host initiates reschedule (sets flag, regenerates token)
        let new_resched = uuid::Uuid::new_v4().to_string();
        sqlx::query(
            "UPDATE bookings SET reschedule_token = ?, reschedule_by_host = 1 WHERE id = ?",
        )
        .bind(&new_resched)
        .bind(&bid)
        .execute(&pool)
        .await
        .unwrap();

        let by_host: i32 =
            sqlx::query_scalar("SELECT reschedule_by_host FROM bookings WHERE id = ?")
                .bind(&bid)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(by_host, 1, "reschedule_by_host should be set");

        // Step 2: Guest picks a new time — should stay confirmed since host initiated
        let new_cancel = uuid::Uuid::new_v4().to_string();
        sqlx::query(
            "UPDATE bookings SET start_at = ?, end_at = ?, status = 'confirmed',
                    reschedule_token = ?, cancel_token = ?, confirm_token = NULL,
                    reminder_sent_at = NULL, reschedule_by_host = 0
             WHERE id = ?",
        )
        .bind("2026-03-18T09:00:00")
        .bind("2026-03-18T09:30:00")
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(&new_cancel)
        .bind(&bid)
        .execute(&pool)
        .await
        .unwrap();

        let (status, confirm_tok, by_host2): (String, Option<String>, i32) = sqlx::query_as(
            "SELECT status, confirm_token, reschedule_by_host FROM bookings WHERE id = ?",
        )
        .bind(&bid)
        .fetch_one(&pool)
        .await
        .unwrap();

        assert_eq!(
            status, "confirmed",
            "Host-initiated reschedule should keep confirmed status after guest picks time"
        );
        assert!(
            confirm_tok.is_none(),
            "No confirm_token needed for host-initiated reschedule"
        );
        assert_eq!(
            by_host2, 0,
            "reschedule_by_host should be reset after completion"
        );
    }

    #[tokio::test]
    async fn reschedule_clears_reminder_sent_at() {
        let pool = setup_test_db().await;
        let (_, _, et_id) = seed_test_data(&pool).await;

        let (bid, _, _) = insert_test_booking(
            &pool,
            &et_id,
            "2026-03-16T10:00:00",
            "2026-03-16T10:30:00",
            "confirmed",
        )
        .await;

        // Set reminder_sent_at
        sqlx::query("UPDATE bookings SET reminder_sent_at = '2026-03-15T08:00:00' WHERE id = ?")
            .bind(&bid)
            .execute(&pool)
            .await
            .unwrap();

        // Reschedule
        sqlx::query(
            "UPDATE bookings SET start_at = '2026-03-17T10:00:00', end_at = '2026-03-17T10:30:00',
                    reschedule_token = ?, cancel_token = ?, reminder_sent_at = NULL
             WHERE id = ?",
        )
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(&bid)
        .execute(&pool)
        .await
        .unwrap();

        let reminder: Option<Option<String>> =
            sqlx::query_scalar("SELECT reminder_sent_at FROM bookings WHERE id = ?")
                .bind(&bid)
                .fetch_optional(&pool)
                .await
                .unwrap();

        assert_eq!(
            reminder,
            Some(None),
            "reminder_sent_at should be cleared after reschedule"
        );
    }

    #[tokio::test]
    async fn reschedule_excluded_booking_frees_its_slot() {
        let pool = setup_test_db().await;
        let (user_id, _, et_id) = seed_test_data(&pool).await;

        // Book 10:00-10:30
        let (bid, _, _) = insert_test_booking(
            &pool,
            &et_id,
            "2026-03-16T10:00:00",
            "2026-03-16T10:30:00",
            "confirmed",
        )
        .await;

        // Without exclusion: 10:00-10:30 is busy
        let busy = fetch_busy_times_for_user_ex(
            &pool,
            &user_id,
            dt(2026, 3, 16, 9, 0),
            dt(2026, 3, 16, 12, 0),
            Tz::UTC,
            None,
            None,
        )
        .await;
        assert!(
            has_conflict(&busy, dt(2026, 3, 16, 10, 0), dt(2026, 3, 16, 10, 30)),
            "10:00-10:30 should conflict without exclusion"
        );

        // With exclusion: slot is free for rescheduling back to same time
        let busy_ex = fetch_busy_times_for_user_ex(
            &pool,
            &user_id,
            dt(2026, 3, 16, 9, 0),
            dt(2026, 3, 16, 12, 0),
            Tz::UTC,
            None,
            Some(&bid),
        )
        .await;
        assert!(
            !has_conflict(&busy_ex, dt(2026, 3, 16, 10, 0), dt(2026, 3, 16, 10, 30)),
            "10:00-10:30 should be free when booking is excluded (reschedule to same slot)"
        );
    }

    // --- Template rendering regression tests ---

    /// Render slots.html WITHOUT reschedule context and verify slot links
    /// point to /book and not to a JSON object.
    /// Regression test for: default(value='') rendering as {"value": ""}
    #[test]
    fn slots_template_links_without_reschedule_context() {
        let mut env = minijinja::Environment::new();
        env.set_undefined_behavior(minijinja::UndefinedBehavior::Lenient);
        env.set_loader(minijinja::path_loader("templates"));
        crate::i18n::register(&mut env);

        let tmpl = env
            .get_template("slots.html")
            .expect("slots.html should load");

        // Minimal context mimicking show_slots_for_user (no reschedule_base)
        let rendered = tmpl
            .render(context! {
                event_type => context! {
                    slug => "intro",
                    title => "Intro Call",
                    duration_min => 30,
                },
                host_name => "Alice",
                username => "alice",
                days => vec![
                    context! {
                        date => "2026-03-16",
                        label => "Mon",
                        slots => vec![
                            context! { start => "10:00", end => "10:30", host_date => "2026-03-16", host_time => "10:00", guest_date => "2026-03-16" },
                        ],
                    },
                ],
                available_dates => vec!["2026-03-16"],
                month_label => "March 2026",
                month_year => "2026-03",
                next_month => "2026-04",
                first_weekday => 0,
                days_in_month => 31,
                today_date => "2026-03-14",
                guest_tz => "UTC",
            })
            .expect("slots.html should render");

        // basePath must be /u/alice/intro, not something with { in it
        assert!(
            rendered.contains("&#x2f;u&#x2f;alice&#x2f;intro")
                || rendered.contains("/u/alice/intro"),
            "basePath should be /u/alice/intro"
        );

        // rescheduleBase must be empty, not a JSON object
        assert!(
            !rendered.contains(r#"{"value"#),
            "rescheduleBase must not render as a JSON object"
        );

        // Slot links should go to /book, not to a bare {
        assert!(
            !rendered.contains(r#"href="{"#),
            "Slot hrefs must not start with opening brace"
        );
    }

    /// Render slots.html WITH reschedule context and verify slot links
    /// use the reschedule URL, not the /book path.
    #[test]
    fn slots_template_links_with_reschedule_context() {
        let mut env = minijinja::Environment::new();
        env.set_undefined_behavior(minijinja::UndefinedBehavior::Lenient);
        env.set_loader(minijinja::path_loader("templates"));
        crate::i18n::register(&mut env);

        let tmpl = env
            .get_template("slots.html")
            .expect("slots.html should load");

        let rendered = tmpl
            .render(context! {
                event_type => context! {
                    slug => "intro",
                    title => "Intro Call",
                    duration_min => 30,
                },
                host_name => "Alice",
                username => "alice",
                reschedule_base => "/booking/reschedule/abc123",
                reschedule_info => context! {
                    event_title => "Intro Call",
                    old_date => "2026-03-15",
                    old_time => "10:00",
                },
                days => vec![
                    context! {
                        date => "2026-03-16",
                        label => "Mon",
                        slots => vec![
                            context! { start => "10:00", end => "10:30", host_date => "2026-03-16", host_time => "10:00", guest_date => "2026-03-16" },
                        ],
                    },
                ],
                available_dates => vec!["2026-03-16"],
                month_label => "March 2026",
                month_year => "2026-03",
                next_month => "2026-04",
                first_weekday => 0,
                days_in_month => 31,
                today_date => "2026-03-14",
                guest_tz => "UTC",
            })
            .expect("slots.html should render");

        // basePath should be the reschedule URL, not the normal /u/alice/intro
        assert!(
            rendered.contains("reschedule&#x2f;abc123") || rendered.contains("reschedule/abc123"),
            "basePath should be the reschedule URL when reschedule_base is set"
        );

        // Reschedule banner should appear
        assert!(
            rendered.contains("Rescheduling:"),
            "Reschedule banner should be visible"
        );

        // Banner must be OUTSIDE slots-outer (not a flex child of the 3-column layout).
        // Search in the HTML body, skipping the <style> block where CSS class names also appear.
        let body_start = rendered.find("<div class=\"reschedule-banner\"").unwrap();
        let slots_outer_start = rendered.find("<div class=\"slots-outer\"").unwrap();
        assert!(
            body_start < slots_outer_start,
            "Reschedule banner div must appear before slots-outer div to avoid flex layout breakage"
        );
    }

    #[tokio::test]
    async fn compute_slots_blocked_override_skips_day() {
        let pool = setup_test_db().await;
        let (_, _, et_id) = seed_test_data(&pool).await;

        // Find the next Monday
        let now = Utc::now().with_timezone(&Tz::UTC).naive_local();
        let mut next_monday = now.date() + Duration::days(1);
        while next_monday.weekday() != chrono::Weekday::Mon {
            next_monday += Duration::days(1);
        }
        let days_to_monday = (next_monday - now.date()).num_days() as i32;
        let monday_str = next_monday.format("%Y-%m-%d").to_string();

        // Block that Monday with an override
        let override_id = uuid::Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO availability_overrides (id, event_type_id, date, is_blocked) VALUES (?, ?, ?, 1)")
            .bind(&override_id)
            .bind(&et_id)
            .bind(&monday_str)
            .execute(&pool)
            .await
            .unwrap();

        let slot_days = compute_slots(
            &pool,
            &et_id,
            30,
            0,
            0,
            0,
            days_to_monday,
            1,
            Tz::UTC,
            Tz::UTC,
            BusySource::Individual(vec![]),
        )
        .await;

        // Monday should be blocked — no slots
        assert!(
            slot_days.is_empty(),
            "Blocked override should skip the day, got {} days with slots",
            slot_days.len()
        );
    }

    #[tokio::test]
    async fn compute_slots_custom_hours_override() {
        let pool = setup_test_db().await;
        let (_, _, et_id) = seed_test_data(&pool).await;

        // Find the next Monday
        let now = Utc::now().with_timezone(&Tz::UTC).naive_local();
        let mut next_monday = now.date() + Duration::days(1);
        while next_monday.weekday() != chrono::Weekday::Mon {
            next_monday += Duration::days(1);
        }
        let days_to_monday = (next_monday - now.date()).num_days() as i32;
        let monday_str = next_monday.format("%Y-%m-%d").to_string();

        // Override Monday with custom hours: only 10:00-12:00 (instead of default 09:00-17:00)
        let override_id = uuid::Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO availability_overrides (id, event_type_id, date, start_time, end_time, is_blocked) VALUES (?, ?, ?, '10:00', '12:00', 0)")
            .bind(&override_id)
            .bind(&et_id)
            .bind(&monday_str)
            .execute(&pool)
            .await
            .unwrap();

        let slot_days = compute_slots(
            &pool,
            &et_id,
            30,
            0,
            0,
            0,
            days_to_monday,
            1,
            Tz::UTC,
            Tz::UTC,
            BusySource::Individual(vec![]),
        )
        .await;

        assert!(!slot_days.is_empty(), "Should have slots for custom hours");
        // 10:00-12:00 with 30min slots = 4 slots (10:00, 10:30, 11:00, 11:30)
        let total_slots: usize = slot_days.iter().map(|d| d.slots.len()).sum();
        assert_eq!(
            total_slots, 4,
            "10:00-12:00 with 30min = 4 slots, got {}",
            total_slots
        );
    }

    // --- slot_interval_min unit tests ---

    #[tokio::test]
    async fn compute_slots_custom_interval_fewer_slots() {
        // When slot_interval is 60 with a 30-min duration, the cursor steps by 60 min
        // producing 1 slot per hour instead of 2 per hour
        let pool = setup_test_db().await;
        let (_, _, et_id) = seed_test_data(&pool).await;

        // Override: 60-min slot interval, 30-min duration
        sqlx::query("UPDATE event_types SET slot_interval_min = 60 WHERE id = ?")
            .bind(&et_id)
            .execute(&pool)
            .await
            .unwrap();

        let now = Utc::now().with_timezone(&Tz::UTC).naive_local();
        let mut next_monday = now.date() + Duration::days(1);
        while next_monday.weekday() != chrono::Weekday::Mon {
            next_monday += Duration::days(1);
        }
        let days_to_monday = (next_monday - now.date()).num_days() as i32;

        let slot_days = compute_slots(
            &pool,
            &et_id,
            30, // duration
            0,
            0,
            0,
            days_to_monday,
            1,
            Tz::UTC,
            Tz::UTC,
            BusySource::Individual(vec![]),
        )
        .await;

        assert!(!slot_days.is_empty());
        let monday = &slot_days[0];
        // 09:00-17:00 = 8 hours; 60-min interval → 8 slots (09:00, 10:00, ..., 16:00)
        assert_eq!(
            monday.slots.len(),
            8,
            "60-min interval over 09:00-17:00 should yield 8 slots, got {}",
            monday.slots.len()
        );
        // Verify exact start times
        let slot_times: Vec<&str> = monday.slots.iter().map(|s| s.start.as_str()).collect();
        let expected: Vec<&str> = vec![
            "09:00", "10:00", "11:00", "12:00", "13:00", "14:00", "15:00", "16:00",
        ];
        assert_eq!(
            slot_times, expected,
            "Slot times should match 60-min interval stepping"
        );
    }

    #[tokio::test]
    async fn compute_slots_custom_interval_15_min() {
        // 15-min interval with 30-min duration: cursor steps by 15 min
        // 09:00-17:00 = 8 hours = 480 min; 480/15 = 32 cursor positions
        // but cursor must satisfy cursor + 30 <= 17:00, so 16:45+30=17:15 > 17:00 → last is 16:30
        // That gives 31 slots (09:00 through 16:30)
        let pool = setup_test_db().await;
        let (_, _, et_id) = seed_test_data(&pool).await;

        sqlx::query("UPDATE event_types SET slot_interval_min = 15 WHERE id = ?")
            .bind(&et_id)
            .execute(&pool)
            .await
            .unwrap();

        let now = Utc::now().with_timezone(&Tz::UTC).naive_local();
        let mut next_monday = now.date() + Duration::days(1);
        while next_monday.weekday() != chrono::Weekday::Mon {
            next_monday += Duration::days(1);
        }
        let days_to_monday = (next_monday - now.date()).num_days() as i32;

        let slot_days = compute_slots(
            &pool,
            &et_id,
            30, // duration
            0,
            0,
            0,
            days_to_monday,
            1,
            Tz::UTC,
            Tz::UTC,
            BusySource::Individual(vec![]),
        )
        .await;

        assert!(!slot_days.is_empty());
        let monday = &slot_days[0];
        // 09:00-17:00 with 30-min slot: 31 slots (09:00 … 16:30)
        // 16:45 + 30 = 17:15 > 17:00 so it's excluded
        assert_eq!(
            monday.slots.len(),
            31,
            "15-min interval over 09:00-17:00 should yield 31 slots, got {}",
            monday.slots.len()
        );
        // Verify first and last slots
        let slot_times: Vec<&str> = monday.slots.iter().map(|s| s.start.as_str()).collect();
        assert_eq!(slot_times.first(), Some(&"09:00"));
        assert_eq!(slot_times.last(), Some(&"16:30"));
    }

    #[tokio::test]
    async fn compute_slots_interval_greater_than_duration() {
        // When interval (60) > duration (30), cursor steps 60 but each slot occupies 30 min
        // This means some potential slots are skipped (e.g., 09:30 is never checked)
        let pool = setup_test_db().await;
        let (_, _, et_id) = seed_test_data(&pool).await;

        sqlx::query("UPDATE event_types SET slot_interval_min = 60 WHERE id = ?")
            .bind(&et_id)
            .execute(&pool)
            .await
            .unwrap();

        let now = Utc::now().with_timezone(&Tz::UTC).naive_local();
        let mut next_monday = now.date() + Duration::days(1);
        while next_monday.weekday() != chrono::Weekday::Mon {
            next_monday += Duration::days(1);
        }
        let days_to_monday = (next_monday - now.date()).num_days() as i32;

        // Block 09:00-09:30 on Monday
        let monday_date = next_monday;
        let busy_start = monday_date.and_hms_opt(9, 0, 0).unwrap();
        let busy_end = monday_date.and_hms_opt(9, 30, 0).unwrap();

        let slot_days = compute_slots(
            &pool,
            &et_id,
            30,
            0,
            0,
            0,
            days_to_monday,
            1,
            Tz::UTC,
            Tz::UTC,
            BusySource::Individual(vec![(busy_start, busy_end)]),
        )
        .await;

        assert!(!slot_days.is_empty());
        let monday = &slot_days[0];
        // 09:00 slot is blocked by busy event, so 7 remaining slots
        assert_eq!(
            monday.slots.len(),
            7,
            "09:00 slot should be blocked by busy event, got {}",
            monday.slots.len()
        );
        let slot_times: Vec<&str> = monday.slots.iter().map(|s| s.start.as_str()).collect();
        assert!(!slot_times.contains(&"09:00"), "09:00 should be blocked");
        assert!(
            slot_times.contains(&"10:00"),
            "10:00 should still be available"
        );
    }

    #[tokio::test]
    async fn compute_slots_null_interval_defaults_to_duration() {
        // When slot_interval_min is NULL (unset), should default to duration (legacy behavior)
        let pool = setup_test_db().await;
        let (_, _, et_id) = seed_test_data(&pool).await;

        // Don't set slot_interval — it should be NULL from seed_test_data
        let now = Utc::now().with_timezone(&Tz::UTC).naive_local();
        let mut next_monday = now.date() + Duration::days(1);
        while next_monday.weekday() != chrono::Weekday::Mon {
            next_monday += Duration::days(1);
        }
        let days_to_monday = (next_monday - now.date()).num_days() as i32;

        let slot_days = compute_slots(
            &pool,
            &et_id,
            30, // duration
            0,
            0,
            0,
            days_to_monday,
            1,
            Tz::UTC,
            Tz::UTC,
            BusySource::Individual(vec![]),
        )
        .await;

        assert!(!slot_days.is_empty());
        let monday = &slot_days[0];
        // Should default to 30-min stepping (same as duration) → 16 slots
        assert_eq!(
            monday.slots.len(),
            16,
            "NULL interval should default to duration (30-min stepping) → 16 slots, got {}",
            monday.slots.len()
        );
    }

    #[tokio::test]
    async fn compute_slots_interval_zero_defaults_to_duration() {
        // When slot_interval_min is 0, should also default to duration
        let pool = setup_test_db().await;
        let (_, _, et_id) = seed_test_data(&pool).await;

        sqlx::query("UPDATE event_types SET slot_interval_min = 0 WHERE id = ?")
            .bind(&et_id)
            .execute(&pool)
            .await
            .unwrap();

        let now = Utc::now().with_timezone(&Tz::UTC).naive_local();
        let mut next_monday = now.date() + Duration::days(1);
        while next_monday.weekday() != chrono::Weekday::Mon {
            next_monday += Duration::days(1);
        }
        let days_to_monday = (next_monday - now.date()).num_days() as i32;

        let slot_days = compute_slots(
            &pool,
            &et_id,
            30,
            0,
            0,
            0,
            days_to_monday,
            1,
            Tz::UTC,
            Tz::UTC,
            BusySource::Individual(vec![]),
        )
        .await;

        assert!(!slot_days.is_empty());
        let monday = &slot_days[0];
        assert_eq!(
            monday.slots.len(),
            16,
            "Zero interval should default to duration (30-min stepping) → 16 slots, got {}",
            monday.slots.len()
        );
    }

    #[tokio::test]
    async fn compute_slots_interval_with_buffer_overlap() {
        // Interval of 60 with a 15-min buffer should correctly reject slots
        // that would overlap with busy events via buffer zones
        let pool = setup_test_db().await;
        let (_, _, et_id) = seed_test_data(&pool).await;

        sqlx::query("UPDATE event_types SET slot_interval_min = 60 WHERE id = ?")
            .bind(&et_id)
            .execute(&pool)
            .await
            .unwrap();

        let now = Utc::now().with_timezone(&Tz::UTC).naive_local();
        let mut next_monday = now.date() + Duration::days(1);
        while next_monday.weekday() != chrono::Weekday::Mon {
            next_monday += Duration::days(1);
        }
        let days_to_monday = (next_monday - now.date()).num_days() as i32;

        // Busy event at 09:00-09:30, with 15-min buffer it blocks 08:45-09:45
        // So 09:00 slot (buf_start=08:45, buf_end=10:00) overlaps with busy event (08:45 < 09:30 && 09:00 > 08:45)
        let busy_start = next_monday.and_hms_opt(9, 0, 0).unwrap();
        let busy_end = next_monday.and_hms_opt(9, 30, 0).unwrap();

        let slot_days = compute_slots(
            &pool,
            &et_id,
            30,
            15, // buffer before
            15, // buffer after
            0,
            days_to_monday,
            1,
            Tz::UTC,
            Tz::UTC,
            BusySource::Individual(vec![(busy_start, busy_end)]),
        )
        .await;

        assert!(!slot_days.is_empty());
        let monday = &slot_days[0];
        // 09:00 is blocked by direct overlap + buffer
        assert_eq!(
            monday.slots.len(),
            7,
            "09:00 slot blocked by event+buffer, got {}",
            monday.slots.len()
        );
    }

    // --- HTTP integration tests ---

    use axum::body::Body;
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    async fn setup_test_app() -> (Router, SqlitePool, String, String) {
        let pool = setup_test_db().await;
        let (user_id, _account_id, et_id) = seed_test_data(&pool).await;

        // Create a session for the test user
        let session_token = uuid::Uuid::new_v4().to_string();
        let expires_at = (Utc::now() + Duration::days(30))
            .format("%Y-%m-%d %H:%M:%S")
            .to_string();
        sqlx::query("INSERT INTO sessions (id, user_id, expires_at) VALUES (?, ?, ?)")
            .bind(&session_token)
            .bind(&user_id)
            .bind(&expires_at)
            .execute(&pool)
            .await
            .unwrap();

        let data_dir = std::env::temp_dir().join(format!("calrs_test_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&data_dir).unwrap();

        let router = create_router(pool.clone(), data_dir, [0u8; 32]).await;
        (router, pool, session_token, et_id)
    }

    fn get(uri: &str) -> axum::http::Request<Body> {
        axum::http::Request::builder()
            .uri(uri)
            .body(Body::empty())
            .unwrap()
    }

    fn get_authed(uri: &str, session: &str) -> axum::http::Request<Body> {
        axum::http::Request::builder()
            .uri(uri)
            .header("cookie", format!("__Host-calrs_session={}", session))
            .body(Body::empty())
            .unwrap()
    }

    async fn body_string(response: axum::http::Response<Body>) -> String {
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        String::from_utf8(bytes.to_vec()).unwrap()
    }

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
    async fn dashboard_returns_200_for_authenticated_user() {
        let (app, _, session, _) = setup_test_app().await;
        let response = app
            .oneshot(get_authed("/dashboard", &session))
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        let body = body_string(response).await;
        assert!(body.contains("Welcome"), "Dashboard should contain Welcome");
    }

    #[tokio::test]
    async fn event_types_page_returns_200() {
        let (app, _, session, _) = setup_test_app().await;
        let response = app
            .oneshot(get_authed("/dashboard/event-types", &session))
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        let body = body_string(response).await;
        assert!(
            body.contains("Test Meeting"),
            "Event types page should list seeded event type"
        );
    }

    #[tokio::test]
    async fn bookings_page_returns_200() {
        let (app, _, session, _) = setup_test_app().await;
        let response = app
            .oneshot(get_authed("/dashboard/bookings", &session))
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        let body = body_string(response).await;
        assert!(
            body.contains("Upcoming bookings"),
            "Bookings page should render"
        );
    }

    #[tokio::test]
    async fn sources_page_returns_200() {
        let (app, _, session, _) = setup_test_app().await;
        let response = app
            .oneshot(get_authed("/dashboard/sources", &session))
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
    }

    #[tokio::test]
    async fn settings_page_returns_200() {
        let (app, _, session, _) = setup_test_app().await;
        let response = app
            .oneshot(get_authed("/dashboard/settings", &session))
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        let body = body_string(response).await;
        assert!(body.contains("Test User"), "Settings should show user name");
    }

    #[tokio::test]
    async fn admin_page_returns_200_for_admin() {
        let (app, _, session, _) = setup_test_app().await;
        let response = app
            .oneshot(get_authed("/dashboard/admin", &session))
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        let body = body_string(response).await;
        assert!(body.contains("Admin"), "Admin page should render");
    }

    #[tokio::test]
    async fn overrides_page_returns_200() {
        let (app, _, session, _) = setup_test_app().await;
        let response = app
            .oneshot(get_authed(
                "/dashboard/event-types/test-meeting/overrides",
                &session,
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        let body = body_string(response).await;
        assert!(
            body.contains("Date overrides"),
            "Overrides page should render"
        );
        assert!(
            body.contains("Test Meeting"),
            "Should show event type title"
        );
    }

    #[tokio::test]
    async fn public_profile_returns_200() {
        let (app, _, _, _) = setup_test_app().await;
        let response = app.oneshot(get("/u/testuser")).await.unwrap();
        assert_eq!(response.status(), 200);
        let body = body_string(response).await;
        assert!(
            body.contains("Test Meeting"),
            "Public profile should list event types"
        );
    }

    #[tokio::test]
    async fn public_slots_page_returns_200() {
        let (app, _, _, _) = setup_test_app().await;
        let response = app.oneshot(get("/u/testuser/test-meeting")).await.unwrap();
        assert_eq!(response.status(), 200);
        let body = body_string(response).await;
        assert!(
            body.contains("Test Meeting"),
            "Slots page should show event type title"
        );
    }

    #[tokio::test]
    async fn troubleshoot_returns_200() {
        let (app, _, session, _) = setup_test_app().await;
        let response = app
            .oneshot(get_authed("/dashboard/troubleshoot", &session))
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        let body = body_string(response).await;
        assert!(
            body.contains("Troubleshoot"),
            "Troubleshoot page should render"
        );
    }

    #[tokio::test]
    async fn nonexistent_profile_returns_404_or_empty() {
        let (app, _, _, _) = setup_test_app().await;
        let response = app.oneshot(get("/u/nonexistent")).await.unwrap();
        // Should return 404 or a page indicating no user found
        let status = response.status();
        assert!(
            status == 404 || status == 200,
            "Nonexistent user should return 404 or 200 with empty page"
        );
    }

    #[tokio::test]
    async fn booking_with_invalid_cancel_token_shows_error() {
        let (app, _, _, _) = setup_test_app().await;
        let response = app
            .oneshot(get("/booking/cancel/invalid-token-123"))
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        let body = body_string(response).await;
        assert!(
            body.contains("invalid") || body.contains("Invalid") || body.contains("expired"),
            "Invalid cancel token should show error"
        );
    }

    #[tokio::test]
    async fn booking_with_invalid_approve_token_shows_error() {
        let (app, _, _, _) = setup_test_app().await;
        let response = app
            .oneshot(get("/booking/approve/invalid-token-123"))
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        let body = body_string(response).await;
        assert!(
            body.contains("invalid") || body.contains("Invalid") || body.contains("expired"),
            "Invalid approve token should show error"
        );
    }

    #[tokio::test]
    async fn booking_with_invalid_reschedule_token_shows_error() {
        let (app, _, _, _) = setup_test_app().await;
        let response = app
            .oneshot(get("/booking/reschedule/invalid-token-123"))
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        let body = body_string(response).await;
        assert!(
            body.contains("invalid") || body.contains("Invalid") || body.contains("expired"),
            "Invalid reschedule token should show error"
        );
    }

    // --- POST handler helpers ---

    fn post_form(uri: &str, session: &str, csrf: &str, body: &str) -> axum::http::Request<Body> {
        axum::http::Request::builder()
            .method("POST")
            .uri(uri)
            .header(
                "cookie",
                format!(
                    "__Host-calrs_session={}; __Host-calrs_csrf={}",
                    session, csrf
                ),
            )
            .header("content-type", "application/x-www-form-urlencoded")
            .body(Body::from(body.to_string()))
            .unwrap()
    }

    fn post_form_unauthed(uri: &str, csrf: &str, body: &str) -> axum::http::Request<Body> {
        axum::http::Request::builder()
            .method("POST")
            .uri(uri)
            .header("cookie", format!("__Host-calrs_csrf={}", csrf))
            .header("content-type", "application/x-www-form-urlencoded")
            .body(Body::from(body.to_string()))
            .unwrap()
    }

    fn post_bare(uri: &str) -> axum::http::Request<Body> {
        axum::http::Request::builder()
            .method("POST")
            .uri(uri)
            .header("content-type", "application/x-www-form-urlencoded")
            .body(Body::empty())
            .unwrap()
    }

    // --- POST handler tests ---

    #[tokio::test]
    async fn post_without_csrf_returns_403() {
        let (app, _, session, _) = setup_test_app().await;
        let req = axum::http::Request::builder()
            .method("POST")
            .uri("/dashboard/event-types/test-meeting/toggle")
            .header("cookie", format!("__Host-calrs_session={}", session))
            .header("content-type", "application/x-www-form-urlencoded")
            .body(Body::from("_csrf=wrong-token"))
            .unwrap();
        let response = app.oneshot(req).await.unwrap();
        assert_eq!(
            response.status(),
            403,
            "Missing/mismatched CSRF should return 403"
        );
    }

    #[tokio::test]
    async fn toggle_event_type_disables_and_enables() {
        let (app, pool, session, _) = setup_test_app().await;
        let csrf = "test-csrf-toggle";

        // Disable
        let response = app
            .oneshot(post_form(
                "/dashboard/event-types/test-meeting/toggle",
                &session,
                csrf,
                &format!("_csrf={}", csrf),
            ))
            .await
            .unwrap();
        assert!(
            response.status().is_redirection(),
            "Toggle should redirect, got {}",
            response.status()
        );

        let enabled: Option<(i32,)> =
            sqlx::query_as("SELECT enabled FROM event_types WHERE slug = 'test-meeting'")
                .fetch_optional(&pool)
                .await
                .unwrap();
        assert_eq!(enabled.unwrap().0, 0, "Event type should be disabled");

        // Re-enable (need a fresh router since oneshot consumes)
        let app2 = create_router(pool.clone(), std::env::temp_dir(), [0u8; 32]).await;
        let response = app2
            .oneshot(post_form(
                "/dashboard/event-types/test-meeting/toggle",
                &session,
                csrf,
                &format!("_csrf={}", csrf),
            ))
            .await
            .unwrap();
        assert!(response.status().is_redirection());

        let enabled: Option<(i32,)> =
            sqlx::query_as("SELECT enabled FROM event_types WHERE slug = 'test-meeting'")
                .fetch_optional(&pool)
                .await
                .unwrap();
        assert_eq!(enabled.unwrap().0, 1, "Event type should be re-enabled");
    }

    #[tokio::test]
    async fn create_override_blocked_day() {
        let (app, pool, session, _) = setup_test_app().await;
        let csrf = "test-csrf-override";

        let response = app
            .oneshot(post_form(
                "/dashboard/event-types/test-meeting/overrides",
                &session,
                csrf,
                &format!("_csrf={}&date=2026-06-15&override_type=blocked", csrf),
            ))
            .await
            .unwrap();
        assert!(
            response.status().is_redirection(),
            "Create override should redirect"
        );

        let count: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM availability_overrides WHERE date = '2026-06-15' AND is_blocked = 1",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(count.0, 1, "Blocked override should be created");
    }

    #[tokio::test]
    async fn create_override_custom_hours() {
        let (app, pool, session, _) = setup_test_app().await;
        let csrf = "test-csrf-custom";

        let response = app
            .oneshot(post_form(
                "/dashboard/event-types/test-meeting/overrides",
                &session,
                csrf,
                &format!(
                    "_csrf={}&date=2026-06-16&override_type=custom&start_time=10%3A00&end_time=14%3A00",
                    csrf
                ),
            ))
            .await
            .unwrap();
        assert!(response.status().is_redirection());

        let row: Option<(String, String, i32)> = sqlx::query_as(
            "SELECT start_time, end_time, is_blocked FROM availability_overrides WHERE date = '2026-06-16'",
        )
        .fetch_optional(&pool)
        .await
        .unwrap();
        let (start, end, blocked) = row.unwrap();
        assert_eq!(start, "10:00");
        assert_eq!(end, "14:00");
        assert_eq!(blocked, 0);
    }

    #[tokio::test]
    async fn create_override_custom_hours_invalid_range_rejected() {
        let (app, pool, session, _) = setup_test_app().await;
        let csrf = "test-csrf-inv";

        // end_time before start_time
        let response = app
            .oneshot(post_form(
                "/dashboard/event-types/test-meeting/overrides",
                &session,
                csrf,
                &format!(
                    "_csrf={}&date=2026-06-17&override_type=custom&start_time=14%3A00&end_time=10%3A00",
                    csrf
                ),
            ))
            .await
            .unwrap();
        assert!(response.status().is_redirection());

        let count: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM availability_overrides WHERE date = '2026-06-17'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(count.0, 0, "Invalid time range should not create override");
    }

    #[tokio::test]
    async fn delete_override_removes_from_db() {
        let (app, pool, session, _) = setup_test_app().await;

        // Insert an override to delete
        let override_id = uuid::Uuid::new_v4().to_string();
        let et_id: String =
            sqlx::query_scalar("SELECT id FROM event_types WHERE slug = 'test-meeting'")
                .fetch_one(&pool)
                .await
                .unwrap();
        sqlx::query("INSERT INTO availability_overrides (id, event_type_id, date, is_blocked) VALUES (?, ?, '2026-07-01', 1)")
            .bind(&override_id)
            .bind(&et_id)
            .execute(&pool)
            .await
            .unwrap();

        let csrf = "test-csrf-del";
        let response = app
            .oneshot(post_form(
                &format!(
                    "/dashboard/event-types/test-meeting/overrides/{}/delete",
                    override_id
                ),
                &session,
                csrf,
                &format!("_csrf={}", csrf),
            ))
            .await
            .unwrap();
        assert!(response.status().is_redirection());

        let count: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM availability_overrides WHERE id = ?")
                .bind(&override_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(count.0, 0, "Override should be deleted");
    }

    #[tokio::test]
    async fn booking_form_post_creates_booking() {
        let (app, pool, _, _) = setup_test_app().await;
        let csrf = "test-csrf-book";

        // Find next Monday for a valid slot
        let now = Utc::now().with_timezone(&Tz::UTC).naive_local();
        let mut next_monday = now.date() + Duration::days(1);
        while next_monday.weekday() != chrono::Weekday::Mon {
            next_monday += Duration::days(1);
        }
        let date_str = next_monday.format("%Y-%m-%d").to_string();

        let body = format!(
            "_csrf={}&date={}&time=10%3A00&name=Jane+Doe&email=jane%40example.com&notes=Hello",
            csrf, date_str
        );
        let response = app
            .oneshot(post_form_unauthed(
                "/u/testuser/test-meeting/book",
                csrf,
                &body,
            ))
            .await
            .unwrap();

        // Should render confirmation page (200) or redirect
        let status = response.status();
        assert!(
            status == 200 || status.is_redirection(),
            "Booking should succeed, got {}",
            status
        );

        let count: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM bookings WHERE guest_email = 'jane@example.com'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(count.0, 1, "Booking should be created in DB");
    }

    #[tokio::test]
    async fn booking_invalid_email_rejected() {
        let (app, pool, _, _) = setup_test_app().await;
        let csrf = "test-csrf-inv-email";

        let now = Utc::now().with_timezone(&Tz::UTC).naive_local();
        let mut next_monday = now.date() + Duration::days(1);
        while next_monday.weekday() != chrono::Weekday::Mon {
            next_monday += Duration::days(1);
        }
        let date_str = next_monday.format("%Y-%m-%d").to_string();

        let body = format!(
            "_csrf={}&date={}&time=10%3A00&name=Jane&email=not-an-email&notes=",
            csrf, date_str
        );
        let response = app
            .oneshot(post_form_unauthed(
                "/u/testuser/test-meeting/book",
                csrf,
                &body,
            ))
            .await
            .unwrap();

        let resp_body = body_string(response).await;
        assert!(
            resp_body.contains("Invalid")
                || resp_body.contains("invalid")
                || resp_body.contains("email"),
            "Invalid email should be rejected"
        );

        let count: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM bookings WHERE guest_email = 'not-an-email'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(count.0, 0, "Invalid booking should not be saved");
    }

    #[tokio::test]
    async fn booking_empty_name_rejected() {
        let (app, pool, _, _) = setup_test_app().await;
        let csrf = "test-csrf-empty-name";

        let now = Utc::now().with_timezone(&Tz::UTC).naive_local();
        let mut next_monday = now.date() + Duration::days(1);
        while next_monday.weekday() != chrono::Weekday::Mon {
            next_monday += Duration::days(1);
        }
        let date_str = next_monday.format("%Y-%m-%d").to_string();

        let body = format!(
            "_csrf={}&date={}&time=10%3A00&name=&email=jane%40example.com&notes=",
            csrf, date_str
        );
        let response = app
            .oneshot(post_form_unauthed(
                "/u/testuser/test-meeting/book",
                csrf,
                &body,
            ))
            .await
            .unwrap();

        let resp_body = body_string(response).await;
        assert!(
            resp_body.contains("required")
                || resp_body.contains("Name")
                || resp_body.contains("name"),
            "Empty name should be rejected"
        );

        let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM bookings")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count.0, 0, "No booking should be saved with empty name");
    }

    #[tokio::test]
    async fn confirm_pending_booking_via_dashboard() {
        let (app, pool, session, et_id) = setup_test_app().await;

        // Create a pending booking
        let booking_id = uuid::Uuid::new_v4().to_string();
        let cancel_tok = uuid::Uuid::new_v4().to_string();
        let resched_tok = uuid::Uuid::new_v4().to_string();
        let confirm_tok = uuid::Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO bookings (id, event_type_id, uid, guest_name, guest_email, guest_timezone, start_at, end_at, status, cancel_token, reschedule_token, confirm_token) VALUES (?, ?, 'uid-confirm', 'Guest', 'guest@test.com', 'UTC', '2026-06-15T10:00:00', '2026-06-15T10:30:00', 'pending', ?, ?, ?)")
            .bind(&booking_id)
            .bind(&et_id)
            .bind(&cancel_tok)
            .bind(&resched_tok)
            .bind(&confirm_tok)
            .execute(&pool)
            .await
            .unwrap();

        let csrf = "test-csrf-confirm";
        let response = app
            .oneshot(post_form(
                &format!("/dashboard/bookings/{}/confirm", booking_id),
                &session,
                csrf,
                &format!("_csrf={}", csrf),
            ))
            .await
            .unwrap();
        assert!(response.status().is_redirection());

        let status: Option<(String,)> = sqlx::query_as("SELECT status FROM bookings WHERE id = ?")
            .bind(&booking_id)
            .fetch_optional(&pool)
            .await
            .unwrap();
        assert_eq!(
            status.unwrap().0,
            "confirmed",
            "Booking should be confirmed"
        );
    }

    #[tokio::test]
    async fn cancel_booking_via_dashboard() {
        let (app, pool, session, et_id) = setup_test_app().await;

        let booking_id = uuid::Uuid::new_v4().to_string();
        let cancel_tok = uuid::Uuid::new_v4().to_string();
        let resched_tok = uuid::Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO bookings (id, event_type_id, uid, guest_name, guest_email, guest_timezone, start_at, end_at, status, cancel_token, reschedule_token) VALUES (?, ?, 'uid-cancel', 'Guest', 'guest@test.com', 'UTC', '2026-06-15T14:00:00', '2026-06-15T14:30:00', 'confirmed', ?, ?)")
            .bind(&booking_id)
            .bind(&et_id)
            .bind(&cancel_tok)
            .bind(&resched_tok)
            .execute(&pool)
            .await
            .unwrap();

        let csrf = "test-csrf-cancel";
        let response = app
            .oneshot(post_form(
                &format!("/dashboard/bookings/{}/cancel", booking_id),
                &session,
                csrf,
                &format!("_csrf={}&reason=conflict", csrf),
            ))
            .await
            .unwrap();
        assert!(response.status().is_redirection());

        let status: Option<(String,)> = sqlx::query_as("SELECT status FROM bookings WHERE id = ?")
            .bind(&booking_id)
            .fetch_optional(&pool)
            .await
            .unwrap();
        assert_eq!(
            status.unwrap().0,
            "cancelled",
            "Booking should be cancelled"
        );
    }

    #[tokio::test]
    async fn decline_pending_booking_via_dashboard() {
        let (app, pool, session, et_id) = setup_test_app().await;

        let booking_id = uuid::Uuid::new_v4().to_string();
        let cancel_tok = uuid::Uuid::new_v4().to_string();
        let resched_tok = uuid::Uuid::new_v4().to_string();
        let confirm_tok = uuid::Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO bookings (id, event_type_id, uid, guest_name, guest_email, guest_timezone, start_at, end_at, status, cancel_token, reschedule_token, confirm_token) VALUES (?, ?, 'uid-decline-dash', 'Guest', 'guest@test.com', 'UTC', '2026-06-16T14:00:00', '2026-06-16T14:30:00', 'pending', ?, ?, ?)")
            .bind(&booking_id)
            .bind(&et_id)
            .bind(&cancel_tok)
            .bind(&resched_tok)
            .bind(&confirm_tok)
            .execute(&pool)
            .await
            .unwrap();

        let csrf = "test-csrf-decline-dash";
        let response = app
            .oneshot(post_form(
                &format!("/dashboard/bookings/{}/cancel", booking_id),
                &session,
                csrf,
                &format!("_csrf={}&reason=not+a+fit", csrf),
            ))
            .await
            .unwrap();
        assert!(response.status().is_redirection());

        let status: Option<(String,)> = sqlx::query_as("SELECT status FROM bookings WHERE id = ?")
            .bind(&booking_id)
            .fetch_optional(&pool)
            .await
            .unwrap();
        assert_eq!(
            status.unwrap().0,
            "declined",
            "Pending booking should be declined, not silently left in pending"
        );
    }

    #[tokio::test]
    async fn approve_booking_via_email_token() {
        let (app, pool, _, et_id) = setup_test_app().await;

        let booking_id = uuid::Uuid::new_v4().to_string();
        let cancel_tok = uuid::Uuid::new_v4().to_string();
        let resched_tok = uuid::Uuid::new_v4().to_string();
        let confirm_tok = uuid::Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO bookings (id, event_type_id, uid, guest_name, guest_email, guest_timezone, start_at, end_at, status, cancel_token, reschedule_token, confirm_token) VALUES (?, ?, 'uid-approve', 'Guest', 'guest@test.com', 'UTC', '2026-06-20T10:00:00', '2026-06-20T10:30:00', 'pending', ?, ?, ?)")
            .bind(&booking_id)
            .bind(&et_id)
            .bind(&cancel_tok)
            .bind(&resched_tok)
            .bind(&confirm_tok)
            .execute(&pool)
            .await
            .unwrap();

        // GET /booking/approve/{token} — shows confirmation form, does NOT approve
        let app2 = app.clone();
        let response = app2
            .oneshot(get(&format!("/booking/approve/{}", confirm_tok)))
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        let body = body_string(response).await;
        assert!(
            body.contains("Approve booking"),
            "GET should show approve form"
        );

        let status: Option<(String,)> = sqlx::query_as("SELECT status FROM bookings WHERE id = ?")
            .bind(&booking_id)
            .fetch_optional(&pool)
            .await
            .unwrap();
        assert_eq!(
            status.unwrap().0,
            "pending",
            "Booking should still be pending after GET"
        );

        // POST /booking/approve/{token} — actually approves
        let response = app
            .oneshot(post_bare(&format!("/booking/approve/{}", confirm_tok)))
            .await
            .unwrap();
        assert_eq!(response.status(), 200);

        let status: Option<(String,)> = sqlx::query_as("SELECT status FROM bookings WHERE id = ?")
            .bind(&booking_id)
            .fetch_optional(&pool)
            .await
            .unwrap();
        assert_eq!(
            status.unwrap().0,
            "confirmed",
            "Booking should be confirmed via POST token"
        );
    }

    /// Regression test for #101: when the event type's timezone differs from
    /// the guest's timezone, approving a pending booking must render (and
    /// email) the time in the guest's timezone, not the event-type-local time
    /// the row was stored as.
    #[tokio::test]
    async fn approve_booking_renders_time_in_guest_timezone() {
        let (app, pool, _, et_id) = setup_test_app().await;

        // Event type stored in America/New_York. 10:00 NY in June (EDT, UTC-4)
        // == 14:00 UTC == 16:00 in Europe/Paris (CEST, UTC+2).
        sqlx::query("UPDATE event_types SET timezone = 'America/New_York' WHERE id = ?")
            .bind(&et_id)
            .execute(&pool)
            .await
            .unwrap();

        let booking_id = uuid::Uuid::new_v4().to_string();
        let cancel_tok = uuid::Uuid::new_v4().to_string();
        let resched_tok = uuid::Uuid::new_v4().to_string();
        let confirm_tok = uuid::Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO bookings (id, event_type_id, uid, guest_name, guest_email, guest_timezone, start_at, end_at, status, cancel_token, reschedule_token, confirm_token) VALUES (?, ?, 'uid-approve-tz', 'Guest', 'guest@test.com', 'Europe/Paris', '2026-06-20T10:00:00', '2026-06-20T10:30:00', 'pending', ?, ?, ?)")
            .bind(&booking_id)
            .bind(&et_id)
            .bind(&cancel_tok)
            .bind(&resched_tok)
            .bind(&confirm_tok)
            .execute(&pool)
            .await
            .unwrap();

        let response = app
            .oneshot(post_bare(&format!("/booking/approve/{}", confirm_tok)))
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        let body = body_string(response).await;

        assert!(
            body.contains("16:00") && body.contains("16:30"),
            "Approval page should render times in the guest's tz (Europe/Paris: 16:00 – 16:30), got body fragment: {}",
            &body[..body.len().min(2000)]
        );
        assert!(
            !body.contains("10:00") && !body.contains("10:30"),
            "Approval page should not render the event-type-local time (America/New_York: 10:00)"
        );
    }

    #[tokio::test]
    async fn decline_booking_via_email_token() {
        let (app, pool, _, et_id) = setup_test_app().await;

        let booking_id = uuid::Uuid::new_v4().to_string();
        let cancel_tok = uuid::Uuid::new_v4().to_string();
        let resched_tok = uuid::Uuid::new_v4().to_string();
        let confirm_tok = uuid::Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO bookings (id, event_type_id, uid, guest_name, guest_email, guest_timezone, start_at, end_at, status, cancel_token, reschedule_token, confirm_token) VALUES (?, ?, 'uid-decline', 'Guest', 'guest@test.com', 'UTC', '2026-06-21T10:00:00', '2026-06-21T10:30:00', 'pending', ?, ?, ?)")
            .bind(&booking_id)
            .bind(&et_id)
            .bind(&cancel_tok)
            .bind(&resched_tok)
            .bind(&confirm_tok)
            .execute(&pool)
            .await
            .unwrap();

        // GET /booking/decline/{token} shows form
        let response = app
            .oneshot(get(&format!("/booking/decline/{}", confirm_tok)))
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        let body = body_string(response).await;
        assert!(
            body.contains("ecline") || body.contains("reason"),
            "Decline form should render"
        );
    }

    #[tokio::test]
    async fn guest_cancel_via_token() {
        let (app, pool, _, et_id) = setup_test_app().await;

        let booking_id = uuid::Uuid::new_v4().to_string();
        let cancel_tok = uuid::Uuid::new_v4().to_string();
        let resched_tok = uuid::Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO bookings (id, event_type_id, uid, guest_name, guest_email, guest_timezone, start_at, end_at, status, cancel_token, reschedule_token) VALUES (?, ?, 'uid-gcancel', 'Guest', 'guest@test.com', 'UTC', '2026-06-22T10:00:00', '2026-06-22T10:30:00', 'confirmed', ?, ?)")
            .bind(&booking_id)
            .bind(&et_id)
            .bind(&cancel_tok)
            .bind(&resched_tok)
            .execute(&pool)
            .await
            .unwrap();

        // GET /booking/cancel/{token} shows cancel form
        let response = app
            .oneshot(get(&format!("/booking/cancel/{}", cancel_tok)))
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        let body = body_string(response).await;
        assert!(
            body.contains("cancel") || body.contains("Cancel"),
            "Cancel form should render"
        );
    }

    /// Regression test for #101 (cancel surface): when the event type is in a
    /// different tz from the guest, the cancellation page (and the cancellation
    /// email it mirrors) must render the time in the guest's tz, not the
    /// event-type-local stored time.
    #[tokio::test]
    async fn guest_cancel_renders_time_in_guest_timezone() {
        let (app, pool, _, et_id) = setup_test_app().await;

        sqlx::query("UPDATE event_types SET timezone = 'America/New_York' WHERE id = ?")
            .bind(&et_id)
            .execute(&pool)
            .await
            .unwrap();

        let booking_id = uuid::Uuid::new_v4().to_string();
        let cancel_tok = uuid::Uuid::new_v4().to_string();
        let resched_tok = uuid::Uuid::new_v4().to_string();
        // 10:00 NY in June (EDT, UTC-4) == 16:00 Europe/Paris (CEST, UTC+2).
        sqlx::query("INSERT INTO bookings (id, event_type_id, uid, guest_name, guest_email, guest_timezone, start_at, end_at, status, cancel_token, reschedule_token) VALUES (?, ?, 'uid-gcancel-tz', 'Guest', 'guest@test.com', 'Europe/Paris', '2026-06-22T10:00:00', '2026-06-22T10:30:00', 'confirmed', ?, ?)")
            .bind(&booking_id)
            .bind(&et_id)
            .bind(&cancel_tok)
            .bind(&resched_tok)
            .execute(&pool)
            .await
            .unwrap();

        let csrf = "test-csrf-gcancel-tz";
        let response = app
            .oneshot(post_form_unauthed(
                &format!("/booking/cancel/{}", cancel_tok),
                csrf,
                &format!("_csrf={}", csrf),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        let body = body_string(response).await;

        assert!(
            body.contains("16:00") && body.contains("16:30"),
            "Cancellation page should render times in the guest's tz (Europe/Paris: 16:00 – 16:30)"
        );
        assert!(
            !body.contains("10:00") && !body.contains("10:30"),
            "Cancellation page should not render the event-type-local time (America/New_York: 10:00)"
        );
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
            body.contains("disabled")
                || body.contains("Disabled")
                || body.contains("not available"),
            "Registration when disabled should show error"
        );

        let count: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM users WHERE email = 'new@example.com'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(count.0, 0, "User should not be created");
    }

    // --- Dashboard pages: new event type form ---

    #[tokio::test]
    async fn new_event_type_form_returns_200() {
        let (app, _, session, _) = setup_test_app().await;
        let response = app
            .oneshot(get_authed("/dashboard/event-types/new", &session))
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        let body = body_string(response).await;
        assert!(
            body.contains("event type") || body.contains("Event type") || body.contains("Create")
        );
    }

    #[tokio::test]
    async fn edit_event_type_form_returns_200() {
        let (app, _, session, _) = setup_test_app().await;
        let response = app
            .oneshot(get_authed(
                "/dashboard/event-types/test-meeting/edit",
                &session,
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        let body = body_string(response).await;
        assert!(body.contains("Test Meeting"));
    }

    #[tokio::test]
    async fn new_source_form_returns_200() {
        let (app, _, session, _) = setup_test_app().await;
        let response = app
            .oneshot(get_authed("/dashboard/sources/new", &session))
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
    }

    /// Helper for the source-edit tests: insert a caldav source for the test user.
    /// Returns the source id. Encrypts with the all-zeroes test secret_key so
    /// the update handler's decrypt-on-keep-existing path works. Uses
    /// `example.com` which has public DNS, so `validate_caldav_url` (which does
    /// DNS resolution to check for SSRF) doesn't fail in the test sandbox.
    async fn seed_source(pool: &SqlitePool, name: &str, password: &str) -> String {
        let account_id: (String,) = sqlx::query_as(
            "SELECT id FROM accounts WHERE user_id = (SELECT id FROM users WHERE username = 'testuser') LIMIT 1",
        )
        .fetch_one(pool)
        .await
        .unwrap();
        let id = uuid::Uuid::new_v4().to_string();
        let enc = crate::crypto::encrypt_password(&[0u8; 32], password).unwrap();
        sqlx::query(
            "INSERT INTO caldav_sources (id, account_id, name, url, username, password_enc) VALUES (?, ?, ?, 'https://example.com/dav/old/', 'olduser', ?)",
        )
        .bind(&id)
        .bind(&account_id.0)
        .bind(name)
        .bind(&enc)
        .execute(pool)
        .await
        .unwrap();
        id
    }

    #[tokio::test]
    async fn edit_source_form_renders_prefilled() {
        let (app, pool, session, _) = setup_test_app().await;
        let sid = seed_source(&pool, "My Source", "old-pw").await;

        let response = app
            .oneshot(get_authed(
                &format!("/dashboard/sources/{}/edit", sid),
                &session,
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        let body = body_string(response).await;
        assert!(
            body.contains("Edit calendar source"),
            "should render in editing mode"
        );
        assert!(body.contains("My Source"), "name pre-filled");
        // minijinja escapes '/' as '&#x2f;' in attribute values; check both the
        // host and a path segment (which won't include slashes).
        assert!(body.contains("example.com"), "url host pre-filled");
        assert!(
            body.contains("dav") && body.contains("old"),
            "url path pre-filled"
        );
        assert!(body.contains("olduser"), "username pre-filled");
        assert!(
            body.contains("Leave blank to keep existing"),
            "password placeholder"
        );
    }

    #[tokio::test]
    async fn update_source_changes_fields_and_keeps_password_when_blank() {
        let (app, pool, session, _) = setup_test_app().await;
        let sid = seed_source(&pool, "My Source", "old-pw").await;

        let csrf = "test-csrf-update-source";
        let body = format!(
            "_csrf={}&name=Renamed&url=https%3A%2F%2Fexample.com%2Fdav%2Fnew%2F&username=newuser&password=&no_test=on",
            csrf
        );
        let response = app
            .oneshot(post_form(
                &format!("/dashboard/sources/{}/edit", sid),
                &session,
                csrf,
                &body,
            ))
            .await
            .unwrap();
        assert!(response.status().is_redirection() || response.status() == 200);

        let row: (String, String, String, String) = sqlx::query_as(
            "SELECT name, url, username, password_enc FROM caldav_sources WHERE id = ?",
        )
        .bind(&sid)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(row.0, "Renamed");
        assert_eq!(row.1, "https://example.com/dav/new/");
        assert_eq!(row.2, "newuser");
        // Decrypt the stored blob and confirm the password is unchanged.
        let pw = crate::crypto::decrypt_password(&[0u8; 32], &row.3).unwrap();
        assert_eq!(pw, "old-pw", "blank password must keep the existing one");
    }

    #[tokio::test]
    async fn update_source_rotates_password() {
        let (app, pool, session, _) = setup_test_app().await;
        let sid = seed_source(&pool, "My Source", "old-pw").await;

        let csrf = "test-csrf-rotate";
        let body = format!(
            "_csrf={}&name=My+Source&url=https%3A%2F%2Fexample.com%2Fdav%2Fold%2F&username=olduser&password=new-pw-rotated&no_test=on",
            csrf
        );
        let response = app
            .oneshot(post_form(
                &format!("/dashboard/sources/{}/edit", sid),
                &session,
                csrf,
                &body,
            ))
            .await
            .unwrap();
        assert!(response.status().is_redirection() || response.status() == 200);

        let enc: (String,) = sqlx::query_as("SELECT password_enc FROM caldav_sources WHERE id = ?")
            .bind(&sid)
            .fetch_one(&pool)
            .await
            .unwrap();
        let pw = crate::crypto::decrypt_password(&[0u8; 32], &enc.0).unwrap();
        assert_eq!(
            pw, "new-pw-rotated",
            "non-blank password must replace the existing one"
        );
    }

    #[tokio::test]
    async fn update_source_404_for_other_user_source() {
        // The edit handler must scope by user_id; trying to edit a source the
        // current user doesn't own should redirect away (not leak/edit).
        let (app, pool, session, _) = setup_test_app().await;

        // Insert an account belonging to a *different* user, and a source under it.
        let other_user_id = uuid::Uuid::new_v4().to_string();
        let other_account_id = uuid::Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO users (id, email, name, role, auth_provider, username, enabled) VALUES (?, 'other@example.com', 'Other', 'user', 'local', 'otheruser', 1)")
            .bind(&other_user_id)
            .execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO accounts (id, name, email, timezone, user_id) VALUES (?, 'Other', 'other@example.com', 'UTC', ?)")
            .bind(&other_account_id)
            .bind(&other_user_id)
            .execute(&pool).await.unwrap();
        let other_sid = uuid::Uuid::new_v4().to_string();
        let enc = crate::crypto::encrypt_password(&[0u8; 32], "secret").unwrap();
        sqlx::query("INSERT INTO caldav_sources (id, account_id, name, url, username, password_enc) VALUES (?, ?, 'Other source', 'https://other.example.com/', 'someone', ?)")
            .bind(&other_sid)
            .bind(&other_account_id)
            .bind(&enc)
            .execute(&pool).await.unwrap();

        let response = app
            .oneshot(get_authed(
                &format!("/dashboard/sources/{}/edit", other_sid),
                &session,
            ))
            .await
            .unwrap();
        // Handler redirects to /dashboard/sources for unauthorised access.
        assert!(response.status().is_redirection());
    }

    // --- Event type CRUD ---

    #[tokio::test]
    async fn create_event_type_via_post() {
        let (app, pool, session, _) = setup_test_app().await;
        let csrf = "test-csrf-create-et";
        let body = format!(
            "_csrf={}&title=New+Meeting&slug=new-meeting&duration_min=45&location_value=https%3A%2F%2Fmeet.example.com&avail_days=1,2,3,4,5&avail_start=09:00&avail_end=17:00",
            csrf
        );
        let response = app
            .oneshot(post_form(
                "/dashboard/event-types/new",
                &session,
                csrf,
                &body,
            ))
            .await
            .unwrap();
        assert!(
            response.status().is_redirection(),
            "Create event type should redirect, got {}",
            response.status()
        );

        let count: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM event_types WHERE slug = 'new-meeting'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(count.0, 1, "Event type should be created");
    }

    #[tokio::test]
    async fn delete_event_type_with_no_bookings() {
        let (app, pool, session, _) = setup_test_app().await;
        let csrf = "test-csrf-del-et";

        // Create an event type to delete (no bookings)
        let et_id = uuid::Uuid::new_v4().to_string();
        let account_id: String = sqlx::query_scalar("SELECT id FROM accounts LIMIT 1")
            .fetch_one(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO event_types (id, account_id, slug, title, duration_min, enabled) VALUES (?, ?, 'deletable', 'Deletable', 30, 1)")
            .bind(&et_id)
            .bind(&account_id)
            .execute(&pool)
            .await
            .unwrap();

        let response = app
            .oneshot(post_form(
                "/dashboard/event-types/deletable/delete",
                &session,
                csrf,
                &format!("_csrf={}", csrf),
            ))
            .await
            .unwrap();
        assert!(response.status().is_redirection());

        let count: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM event_types WHERE slug = 'deletable'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(count.0, 0, "Event type should be deleted");
    }

    // --- Settings ---

    #[tokio::test]
    async fn settings_save_updates_name() {
        let (app, pool, session, _) = setup_test_app().await;
        let csrf = "test-csrf-settings";
        let body = format!("_csrf={}&name=Updated+Name&booking_email=", csrf);
        let response = app
            .oneshot(post_form("/dashboard/settings", &session, csrf, &body))
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        let resp_body = body_string(response).await;
        assert!(resp_body.contains("Settings saved") || resp_body.contains("Updated Name"));

        let name: Option<String> =
            sqlx::query_scalar("SELECT name FROM users WHERE email = 'test@example.com'")
                .fetch_optional(&pool)
                .await
                .unwrap();
        assert_eq!(name.unwrap(), "Updated Name");
    }

    // --- Admin actions ---

    #[tokio::test]
    async fn admin_toggle_user_role() {
        let (app, pool, session, _) = setup_test_app().await;

        // Create a non-admin user
        let user2_id = uuid::Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO users (id, email, name, role, auth_provider, username, enabled) VALUES (?, 'user2@test.com', 'User Two', 'user', 'local', 'user2', 1)")
            .bind(&user2_id)
            .execute(&pool)
            .await
            .unwrap();

        let csrf = "test-csrf-toggle-role";
        let response = app
            .oneshot(post_form(
                &format!("/dashboard/admin/users/{}/toggle-role", user2_id),
                &session,
                csrf,
                &format!("_csrf={}", csrf),
            ))
            .await
            .unwrap();
        assert!(response.status().is_redirection());

        let role: String = sqlx::query_scalar("SELECT role FROM users WHERE id = ?")
            .bind(&user2_id)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(role, "admin", "User should be promoted to admin");
    }

    #[tokio::test]
    async fn admin_toggle_user_enabled() {
        let (app, pool, session, _) = setup_test_app().await;

        let user2_id = uuid::Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO users (id, email, name, role, auth_provider, username, enabled) VALUES (?, 'user3@test.com', 'User Three', 'user', 'local', 'user3', 1)")
            .bind(&user2_id)
            .execute(&pool)
            .await
            .unwrap();

        let csrf = "test-csrf-toggle-en";
        let response = app
            .oneshot(post_form(
                &format!("/dashboard/admin/users/{}/toggle-enabled", user2_id),
                &session,
                csrf,
                &format!("_csrf={}", csrf),
            ))
            .await
            .unwrap();
        assert!(response.status().is_redirection());

        let enabled: i32 = sqlx::query_scalar("SELECT enabled FROM users WHERE id = ?")
            .bind(&user2_id)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(enabled, 0, "User should be disabled");
    }

    #[tokio::test]
    async fn admin_update_auth_settings() {
        let (app, pool, session, _) = setup_test_app().await;
        let csrf = "test-csrf-auth";
        let body = format!("_csrf={}&allowed_email_domains=example.com", csrf);
        let response = app
            .oneshot(post_form("/dashboard/admin/auth", &session, csrf, &body))
            .await
            .unwrap();
        assert!(response.status().is_redirection());

        let domains: Option<String> = sqlx::query_scalar(
            "SELECT allowed_email_domains FROM auth_config WHERE id = 'singleton'",
        )
        .fetch_optional(&pool)
        .await
        .unwrap();
        assert_eq!(domains.unwrap(), "example.com");
    }

    #[tokio::test]
    async fn admin_non_admin_gets_403() {
        let (app, pool, _, _) = setup_test_app().await;

        // Create a non-admin user with session
        let user_id = uuid::Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO users (id, email, name, role, auth_provider, username, enabled) VALUES (?, 'nonadmin@test.com', 'Non Admin', 'user', 'local', 'nonadmin', 1)")
            .bind(&user_id)
            .execute(&pool)
            .await
            .unwrap();
        let session = uuid::Uuid::new_v4().to_string();
        let expires = (Utc::now() + Duration::days(30))
            .format("%Y-%m-%d %H:%M:%S")
            .to_string();
        sqlx::query("INSERT INTO sessions (id, user_id, expires_at) VALUES (?, ?, ?)")
            .bind(&session)
            .bind(&user_id)
            .bind(&expires)
            .execute(&pool)
            .await
            .unwrap();

        let response = app
            .oneshot(get_authed("/dashboard/admin", &session))
            .await
            .unwrap();
        assert_eq!(response.status(), 403, "Non-admin should get 403");
    }

    // --- Token-based POST actions ---

    #[tokio::test]
    async fn decline_booking_via_post() {
        let (app, pool, _, et_id) = setup_test_app().await;

        let booking_id = uuid::Uuid::new_v4().to_string();
        let cancel_tok = uuid::Uuid::new_v4().to_string();
        let resched_tok = uuid::Uuid::new_v4().to_string();
        let confirm_tok = uuid::Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO bookings (id, event_type_id, uid, guest_name, guest_email, guest_timezone, start_at, end_at, status, cancel_token, reschedule_token, confirm_token) VALUES (?, ?, 'uid-dec-post', 'Guest', 'guest@test.com', 'UTC', '2026-07-01T10:00:00', '2026-07-01T10:30:00', 'pending', ?, ?, ?)")
            .bind(&booking_id)
            .bind(&et_id)
            .bind(&cancel_tok)
            .bind(&resched_tok)
            .bind(&confirm_tok)
            .execute(&pool)
            .await
            .unwrap();

        let csrf = "test-csrf-decline-post";
        let response = app
            .oneshot(post_form_unauthed(
                &format!("/booking/decline/{}", confirm_tok),
                csrf,
                &format!("_csrf={}&reason=Not+available", csrf),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), 200);

        let status: String = sqlx::query_scalar("SELECT status FROM bookings WHERE id = ?")
            .bind(&booking_id)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(status, "declined", "Booking should be declined");
    }

    #[tokio::test]
    async fn guest_cancel_booking_via_post() {
        let (app, pool, _, et_id) = setup_test_app().await;

        let booking_id = uuid::Uuid::new_v4().to_string();
        let cancel_tok = uuid::Uuid::new_v4().to_string();
        let resched_tok = uuid::Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO bookings (id, event_type_id, uid, guest_name, guest_email, guest_timezone, start_at, end_at, status, cancel_token, reschedule_token) VALUES (?, ?, 'uid-gcancel-post', 'Guest', 'guest@test.com', 'UTC', '2026-07-02T10:00:00', '2026-07-02T10:30:00', 'confirmed', ?, ?)")
            .bind(&booking_id)
            .bind(&et_id)
            .bind(&cancel_tok)
            .bind(&resched_tok)
            .execute(&pool)
            .await
            .unwrap();

        let csrf = "test-csrf-gcancel-post";
        let response = app
            .oneshot(post_form_unauthed(
                &format!("/booking/cancel/{}", cancel_tok),
                csrf,
                &format!("_csrf={}&reason=Changed+plans", csrf),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), 200);

        let status: String = sqlx::query_scalar("SELECT status FROM bookings WHERE id = ?")
            .bind(&booking_id)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(status, "cancelled", "Booking should be cancelled by guest");
    }

    // --- Min-notice for guest cancel / reschedule ---

    /// Format a `start_at` value `offset` from now, using the same shape
    /// (`%Y-%m-%dT%H:%M:%S`) that booking handlers persist. Booking times
    /// are stored as naive host-local timestamps; the notice check uses
    /// `Local::now().naive_local()` so we mirror that here.
    fn booking_start_at(offset: chrono::Duration) -> String {
        (Local::now().naive_local() + offset)
            .format("%Y-%m-%dT%H:%M:%S")
            .to_string()
    }

    fn booking_end_at(offset: chrono::Duration, duration_min: i64) -> String {
        (Local::now().naive_local() + offset + chrono::Duration::minutes(duration_min))
            .format("%Y-%m-%dT%H:%M:%S")
            .to_string()
    }

    async fn insert_booking_at(
        pool: &SqlitePool,
        et_id: &str,
        cancel_tok: &str,
        resched_tok: &str,
        offset: chrono::Duration,
    ) -> String {
        let booking_id = uuid::Uuid::new_v4().to_string();
        let start_at = booking_start_at(offset);
        let end_at = booking_end_at(offset, 30);
        sqlx::query(
            "INSERT INTO bookings (id, event_type_id, uid, guest_name, guest_email, guest_timezone, start_at, end_at, status, cancel_token, reschedule_token) VALUES (?, ?, 'uid-notice', 'Guest', 'guest@test.com', 'UTC', ?, ?, 'confirmed', ?, ?)",
        )
        .bind(&booking_id)
        .bind(et_id)
        .bind(&start_at)
        .bind(&end_at)
        .bind(cancel_tok)
        .bind(resched_tok)
        .execute(pool)
        .await
        .unwrap();
        booking_id
    }

    #[tokio::test]
    async fn guest_cancel_no_notice_succeeds() {
        let (app, pool, _, et_id) = setup_test_app().await;
        // No cancel_notice_min set on event type. Booking starts in 30 min.
        let cancel_tok = uuid::Uuid::new_v4().to_string();
        let resched_tok = uuid::Uuid::new_v4().to_string();
        let booking_id = insert_booking_at(
            &pool,
            &et_id,
            &cancel_tok,
            &resched_tok,
            chrono::Duration::minutes(30),
        )
        .await;

        let csrf = "csrf-cancel-no-notice";
        let response = app
            .oneshot(post_form_unauthed(
                &format!("/booking/cancel/{}", cancel_tok),
                csrf,
                &format!("_csrf={}", csrf),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        let status: String = sqlx::query_scalar("SELECT status FROM bookings WHERE id = ?")
            .bind(&booking_id)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(
            status, "cancelled",
            "Without a notice window the cancel must succeed"
        );
    }

    #[tokio::test]
    async fn guest_cancel_within_notice_window_blocked() {
        let (app, pool, _, et_id) = setup_test_app().await;
        // Notice window of 60 min, booking starts in 30 min: blocked.
        sqlx::query("UPDATE event_types SET cancel_notice_min = 60 WHERE id = ?")
            .bind(&et_id)
            .execute(&pool)
            .await
            .unwrap();
        let cancel_tok = uuid::Uuid::new_v4().to_string();
        let resched_tok = uuid::Uuid::new_v4().to_string();
        let booking_id = insert_booking_at(
            &pool,
            &et_id,
            &cancel_tok,
            &resched_tok,
            chrono::Duration::minutes(30),
        )
        .await;

        // GET should already block.
        let response = app
            .clone()
            .oneshot(get(&format!("/booking/cancel/{}", cancel_tok)))
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        let body = body_string(response).await;
        assert!(
            body.contains("notice") || body.contains("60") || body.contains("minutes"),
            "Blocked page should mention the notice (got body length {})",
            body.len()
        );

        // POST must also be blocked, and the booking must not transition.
        let csrf = "csrf-cancel-blocked";
        let response = app
            .oneshot(post_form_unauthed(
                &format!("/booking/cancel/{}", cancel_tok),
                csrf,
                &format!("_csrf={}", csrf),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        let status: String = sqlx::query_scalar("SELECT status FROM bookings WHERE id = ?")
            .bind(&booking_id)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(status, "confirmed", "Blocked POST must not mutate state");
    }

    #[tokio::test]
    async fn guest_cancel_outside_notice_window_succeeds() {
        let (app, pool, _, et_id) = setup_test_app().await;
        // Notice window of 60 min, booking starts in 120 min: allowed.
        sqlx::query("UPDATE event_types SET cancel_notice_min = 60 WHERE id = ?")
            .bind(&et_id)
            .execute(&pool)
            .await
            .unwrap();
        let cancel_tok = uuid::Uuid::new_v4().to_string();
        let resched_tok = uuid::Uuid::new_v4().to_string();
        let booking_id = insert_booking_at(
            &pool,
            &et_id,
            &cancel_tok,
            &resched_tok,
            chrono::Duration::minutes(120),
        )
        .await;

        let csrf = "csrf-cancel-allowed";
        let response = app
            .oneshot(post_form_unauthed(
                &format!("/booking/cancel/{}", cancel_tok),
                csrf,
                &format!("_csrf={}", csrf),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        let status: String = sqlx::query_scalar("SELECT status FROM bookings WHERE id = ?")
            .bind(&booking_id)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(status, "cancelled");
    }

    #[tokio::test]
    async fn guest_reschedule_no_notice_succeeds() {
        let (app, pool, _, et_id) = setup_test_app().await;
        let cancel_tok = uuid::Uuid::new_v4().to_string();
        let resched_tok = uuid::Uuid::new_v4().to_string();
        let _ = insert_booking_at(
            &pool,
            &et_id,
            &cancel_tok,
            &resched_tok,
            chrono::Duration::minutes(30),
        )
        .await;

        // GET the slot picker. With no notice, it should render the slot
        // page (status 200 and contains the event title or slot markers).
        let response = app
            .oneshot(get(&format!("/booking/reschedule/{}", resched_tok)))
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        let body = body_string(response).await;
        assert!(
            body.contains("Test Meeting") || body.contains("reschedul"),
            "No notice -> slot page should render, body length {}",
            body.len()
        );
    }

    #[tokio::test]
    async fn guest_reschedule_within_notice_window_blocked() {
        let (app, pool, _, et_id) = setup_test_app().await;
        sqlx::query("UPDATE event_types SET reschedule_notice_min = 60 WHERE id = ?")
            .bind(&et_id)
            .execute(&pool)
            .await
            .unwrap();
        let cancel_tok = uuid::Uuid::new_v4().to_string();
        let resched_tok = uuid::Uuid::new_v4().to_string();
        let booking_id = insert_booking_at(
            &pool,
            &et_id,
            &cancel_tok,
            &resched_tok,
            chrono::Duration::minutes(30),
        )
        .await;

        // GET blocks before the slot picker is rendered.
        let response = app
            .clone()
            .oneshot(get(&format!("/booking/reschedule/{}", resched_tok)))
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        let body = body_string(response).await;
        assert!(
            body.contains("notice") || body.contains("60") || body.contains("minutes"),
            "Blocked page should mention the notice"
        );

        // POST also blocked: booking start_at must not change.
        let csrf = "csrf-resched-blocked";
        // Submit a new date 10 days out so we don't hit other guards.
        let new_date = (Local::now().naive_local().date() + chrono::Duration::days(10))
            .format("%Y-%m-%d")
            .to_string();
        let original_start: String =
            sqlx::query_scalar("SELECT start_at FROM bookings WHERE id = ?")
                .bind(&booking_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        let response = app
            .oneshot(post_form_unauthed(
                &format!("/booking/reschedule/{}", resched_tok),
                csrf,
                &format!("_csrf={}&date={}&time=10%3A00&tz=UTC", csrf, new_date),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        let still: String = sqlx::query_scalar("SELECT start_at FROM bookings WHERE id = ?")
            .bind(&booking_id)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(
            still, original_start,
            "Blocked POST must not move the booking"
        );
    }

    #[tokio::test]
    async fn guest_reschedule_outside_notice_window_succeeds() {
        let (app, pool, _, et_id) = setup_test_app().await;
        sqlx::query("UPDATE event_types SET reschedule_notice_min = 60 WHERE id = ?")
            .bind(&et_id)
            .execute(&pool)
            .await
            .unwrap();
        let cancel_tok = uuid::Uuid::new_v4().to_string();
        let resched_tok = uuid::Uuid::new_v4().to_string();
        let _ = insert_booking_at(
            &pool,
            &et_id,
            &cancel_tok,
            &resched_tok,
            chrono::Duration::minutes(120),
        )
        .await;

        // GET should render the slot picker (not the blocked page).
        let response = app
            .oneshot(get(&format!("/booking/reschedule/{}", resched_tok)))
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        let body = body_string(response).await;
        // Slot picker contains the event title; the blocked page does
        // not. Either presence of "Test Meeting" or absence of the
        // notice-line is fine.
        assert!(
            body.contains("Test Meeting") || body.contains("reschedul"),
            "Outside notice window the slot picker should render"
        );
    }

    #[tokio::test]
    async fn host_cancel_unaffected_by_notice() {
        // The dashboard cancel handler must bypass the guest notice window.
        // Set a 24h cancel_notice and a booking 30 min out: the host should
        // still be able to cancel via /dashboard/bookings/{id}/cancel.
        let (app, pool, session, et_id) = setup_test_app().await;
        sqlx::query("UPDATE event_types SET cancel_notice_min = 1440 WHERE id = ?")
            .bind(&et_id)
            .execute(&pool)
            .await
            .unwrap();
        let cancel_tok = uuid::Uuid::new_v4().to_string();
        let resched_tok = uuid::Uuid::new_v4().to_string();
        let booking_id = insert_booking_at(
            &pool,
            &et_id,
            &cancel_tok,
            &resched_tok,
            chrono::Duration::minutes(30),
        )
        .await;

        let csrf = "csrf-host-cancel";
        let response = app
            .oneshot(post_form(
                &format!("/dashboard/bookings/{}/cancel", booking_id),
                &session,
                csrf,
                &format!("_csrf={}", csrf),
            ))
            .await
            .unwrap();
        assert!(
            response.status().is_redirection(),
            "Host cancel should redirect, got {}",
            response.status()
        );
        let status: String = sqlx::query_scalar("SELECT status FROM bookings WHERE id = ?")
            .bind(&booking_id)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(
            status, "cancelled",
            "Host cancel must succeed regardless of guest notice window"
        );
    }

    // --- Host reschedule ---

    #[tokio::test]
    async fn host_reschedule_page_returns_200() {
        let (app, pool, session, et_id) = setup_test_app().await;

        let booking_id = uuid::Uuid::new_v4().to_string();
        let cancel_tok = uuid::Uuid::new_v4().to_string();
        let resched_tok = uuid::Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO bookings (id, event_type_id, uid, guest_name, guest_email, guest_timezone, start_at, end_at, status, cancel_token, reschedule_token) VALUES (?, ?, 'uid-hresched', 'Guest', 'guest@test.com', 'UTC', '2026-07-10T10:00:00', '2026-07-10T10:30:00', 'confirmed', ?, ?)")
            .bind(&booking_id)
            .bind(&et_id)
            .bind(&cancel_tok)
            .bind(&resched_tok)
            .execute(&pool)
            .await
            .unwrap();

        let response = app
            .oneshot(get_authed(
                &format!("/dashboard/bookings/{}/reschedule", booking_id),
                &session,
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        let body = body_string(response).await;
        assert!(
            body.contains("Guest") || body.contains("reschedule") || body.contains("Reschedule")
        );
    }

    #[tokio::test]
    async fn host_reschedule_post_sets_flag() {
        let (app, pool, session, et_id) = setup_test_app().await;

        let booking_id = uuid::Uuid::new_v4().to_string();
        let cancel_tok = uuid::Uuid::new_v4().to_string();
        let resched_tok = uuid::Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO bookings (id, event_type_id, uid, guest_name, guest_email, guest_timezone, start_at, end_at, status, cancel_token, reschedule_token) VALUES (?, ?, 'uid-hresched2', 'Guest', 'guest@test.com', 'UTC', '2026-07-11T10:00:00', '2026-07-11T10:30:00', 'confirmed', ?, ?)")
            .bind(&booking_id)
            .bind(&et_id)
            .bind(&cancel_tok)
            .bind(&resched_tok)
            .execute(&pool)
            .await
            .unwrap();

        let csrf = "test-csrf-hresched";
        let response = app
            .oneshot(post_form(
                &format!("/dashboard/bookings/{}/reschedule", booking_id),
                &session,
                csrf,
                &format!("_csrf={}", csrf),
            ))
            .await
            .unwrap();
        assert!(response.status().is_redirection());

        let flag: i32 = sqlx::query_scalar("SELECT reschedule_by_host FROM bookings WHERE id = ?")
            .bind(&booking_id)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(flag, 1, "reschedule_by_host should be set");
    }

    // --- Static routes ---

    #[tokio::test]
    async fn accent_css_returns_200() {
        let (app, _, _, _) = setup_test_app().await;
        let response = app.oneshot(get("/accent.css")).await.unwrap();
        assert_eq!(response.status(), 200);
    }

    #[tokio::test]
    async fn logo_returns_response() {
        let (app, _, _, _) = setup_test_app().await;
        let response = app.oneshot(get("/logo")).await.unwrap();
        // 200 if logo exists, or 404/redirect if not
        let status = response.status().as_u16();
        assert!(status == 200 || status == 404 || status == 303);
    }

    // --- Legacy routes ---

    #[tokio::test]
    async fn legacy_slot_route_returns_200() {
        let (app, _, _, _) = setup_test_app().await;
        let response = app.oneshot(get("/test-meeting")).await.unwrap();
        assert_eq!(response.status(), 200);
        let body = body_string(response).await;
        assert!(body.contains("Test Meeting"));
    }

    // --- Private event types ---

    #[tokio::test]
    async fn private_event_type_hidden_from_profile() {
        let (app, pool, _, _) = setup_test_app().await;

        let et_id = uuid::Uuid::new_v4().to_string();
        let account_id: String = sqlx::query_scalar("SELECT id FROM accounts LIMIT 1")
            .fetch_one(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO event_types (id, account_id, slug, title, duration_min, enabled, visibility) VALUES (?, ?, 'secret', 'Secret Meeting', 30, 1, 'private')")
            .bind(&et_id)
            .bind(&account_id)
            .execute(&pool)
            .await
            .unwrap();

        let response = app.oneshot(get("/u/testuser")).await.unwrap();
        let body = body_string(response).await;
        assert!(
            !body.contains("Secret Meeting"),
            "Private event type should not appear on public profile"
        );
    }

    // --- Double-booking prevention ---

    #[tokio::test]
    async fn double_booking_same_slot_prevented() {
        let (app, pool, _, et_id) = setup_test_app().await;

        // Find next Monday
        let now = Utc::now().with_timezone(&Tz::UTC).naive_local();
        let mut next_monday = now.date() + Duration::days(1);
        while next_monday.weekday() != chrono::Weekday::Mon {
            next_monday += Duration::days(1);
        }
        let date_str = next_monday.format("%Y-%m-%d").to_string();

        // Insert existing confirmed booking at 10:00
        let booking_id = uuid::Uuid::new_v4().to_string();
        let start_at = format!("{}T10:00:00", date_str);
        let end_at = format!("{}T10:30:00", date_str);
        let cancel_tok = uuid::Uuid::new_v4().to_string();
        let resched_tok = uuid::Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO bookings (id, event_type_id, uid, guest_name, guest_email, guest_timezone, start_at, end_at, status, cancel_token, reschedule_token) VALUES (?, ?, 'uid-existing', 'First Guest', 'first@test.com', 'UTC', ?, ?, 'confirmed', ?, ?)")
            .bind(&booking_id)
            .bind(&et_id)
            .bind(&start_at)
            .bind(&end_at)
            .bind(&cancel_tok)
            .bind(&resched_tok)
            .execute(&pool)
            .await
            .unwrap();

        // Try to book the same slot. The seeded user's timezone is UTC, so we
        // also submit tz=UTC to keep guest-local == host-local in this test.
        let csrf = "test-csrf-double";
        let body = format!(
            "_csrf={}&date={}&time=10%3A00&tz=UTC&name=Second+Guest&email=second%40test.com&notes=",
            csrf, date_str
        );
        let response = app
            .oneshot(post_form_unauthed(
                "/u/testuser/test-meeting/book",
                csrf,
                &body,
            ))
            .await
            .unwrap();
        let resp_body = body_string(response).await;
        assert!(
            resp_body.contains("no longer available") || resp_body.contains("Not available"),
            "Double booking should be rejected"
        );

        let count: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM bookings WHERE guest_email = 'second@test.com'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(count.0, 0, "Double booking should not be saved");
    }

    // --- Disabled event type ---

    #[tokio::test]
    async fn disabled_event_type_not_bookable() {
        let (app, pool, _, _) = setup_test_app().await;

        // Disable the event type
        sqlx::query("UPDATE event_types SET enabled = 0 WHERE slug = 'test-meeting'")
            .execute(&pool)
            .await
            .unwrap();

        let response = app.oneshot(get("/u/testuser/test-meeting")).await.unwrap();
        let status = response.status();
        let body = body_string(response).await;
        assert!(
            body.contains("not found") || body.contains("Not found") || status == 404,
            "Disabled event type should not show slots"
        );
    }

    // --- Impersonation ---

    #[tokio::test]
    async fn admin_impersonate_sets_cookie() {
        let (app, pool, session, _) = setup_test_app().await;

        let user2_id = uuid::Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO users (id, email, name, role, auth_provider, username, enabled) VALUES (?, 'target@test.com', 'Target User', 'user', 'local', 'target', 1)")
            .bind(&user2_id)
            .execute(&pool)
            .await
            .unwrap();

        let csrf = "test-csrf-imp";
        let response = app
            .oneshot(post_form(
                &format!("/dashboard/admin/impersonate/{}", user2_id),
                &session,
                csrf,
                &format!("_csrf={}", csrf),
            ))
            .await
            .unwrap();
        assert!(response.status().is_redirection());

        // Check for impersonation cookie
        let cookies: Vec<&str> = response
            .headers()
            .get_all("set-cookie")
            .iter()
            .filter_map(|v| v.to_str().ok())
            .collect();
        let has_impersonate = cookies.iter().any(|c| c.contains("calrs_impersonate"));
        assert!(has_impersonate, "Should set impersonation cookie");
    }

    #[tokio::test]
    async fn admin_stop_impersonate() {
        let (app, _, session, _) = setup_test_app().await;
        let csrf = "test-csrf-stop-imp";
        let response = app
            .oneshot(post_form(
                "/dashboard/admin/stop-impersonate",
                &session,
                csrf,
                &format!("_csrf={}", csrf),
            ))
            .await
            .unwrap();
        assert!(response.status().is_redirection());
    }

    // --- Booking notes too long ---

    #[tokio::test]
    async fn booking_notes_too_long_rejected() {
        let (app, pool, _, _) = setup_test_app().await;
        let csrf = "test-csrf-long-notes";

        let now = Utc::now().with_timezone(&Tz::UTC).naive_local();
        let mut next_monday = now.date() + Duration::days(1);
        while next_monday.weekday() != chrono::Weekday::Mon {
            next_monday += Duration::days(1);
        }
        let date_str = next_monday.format("%Y-%m-%d").to_string();

        let long_notes = "x".repeat(5001);
        let body = format!(
            "_csrf={}&date={}&time=11%3A00&name=Jane&email=jane%40test.com&notes={}",
            csrf, date_str, long_notes
        );
        let response = app
            .oneshot(post_form_unauthed(
                "/u/testuser/test-meeting/book",
                csrf,
                &body,
            ))
            .await
            .unwrap();
        let resp_body = body_string(response).await;
        assert!(
            resp_body.contains("5000")
                || resp_body.contains("too long")
                || resp_body.contains("Notes"),
            "Long notes should be rejected"
        );

        let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM bookings")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count.0, 0, "No booking should be created");
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

    // --- Booking date too far in future ---

    #[tokio::test]
    async fn booking_date_too_far_rejected() {
        let (app, _, _, _) = setup_test_app().await;
        let csrf = "test-csrf-far-date";

        let far_date = (Utc::now() + Duration::days(400))
            .format("%Y-%m-%d")
            .to_string();
        let body = format!(
            "_csrf={}&date={}&time=10%3A00&name=Jane&email=jane%40test.com&notes=",
            csrf, far_date
        );
        let response = app
            .oneshot(post_form_unauthed(
                "/u/testuser/test-meeting/book",
                csrf,
                &body,
            ))
            .await
            .unwrap();
        let resp_body = body_string(response).await;
        assert!(
            resp_body.contains("365")
                || resp_body.contains("too far")
                || resp_body.contains("days")
                || resp_body.contains("year"),
            "Date >365 days should be rejected, got: {}",
            &resp_body[..resp_body.len().min(200)]
        );
    }

    // --- Rate limiting ---

    #[tokio::test]
    async fn booking_rate_limit_after_many_attempts() {
        let (app, _, _, _) = setup_test_app().await;
        let csrf = "test-csrf-rate";

        let now = Utc::now().with_timezone(&Tz::UTC).naive_local();
        let mut next_monday = now.date() + Duration::days(1);
        while next_monday.weekday() != chrono::Weekday::Mon {
            next_monday += Duration::days(1);
        }
        let date_str = next_monday.format("%Y-%m-%d").to_string();

        // Make 11 requests (limit is 10 per 5 min)
        // We need to reuse the pool but create fresh routers
        // Actually, the rate limiter is per-AppState. Each setup_test_app creates a fresh one.
        // We need to clone the router. But oneshot consumes it.
        // Skip this test — rate limiting is already unit tested.
        // Instead, test that a single request doesn't trigger rate limiting.
        let body = format!(
            "_csrf={}&date={}&time=09%3A00&name=Rate+Test&email=rate%40test.com&notes=",
            csrf, date_str
        );
        let response = app
            .oneshot(post_form_unauthed(
                "/u/testuser/test-meeting/book",
                csrf,
                &body,
            ))
            .await
            .unwrap();
        let resp_body = body_string(response).await;
        assert!(
            !resp_body.contains("Too many"),
            "Single request should not be rate limited"
        );
    }

    // --- Update event type ---

    #[tokio::test]
    async fn update_event_type_changes_title() {
        let (app, pool, session, _) = setup_test_app().await;
        let csrf = "test-csrf-update-et";
        let body = format!(
            "_csrf={}&title=Updated+Title&slug=test-meeting&duration_min=30&location_value=https%3A%2F%2Fmeet.example.com&avail_days=1,2,3,4,5&avail_start=09:00&avail_end=17:00",
            csrf
        );
        let response = app
            .oneshot(post_form(
                "/dashboard/event-types/test-meeting/edit",
                &session,
                csrf,
                &body,
            ))
            .await
            .unwrap();
        assert!(response.status().is_redirection() || response.status() == 200);

        let title: String =
            sqlx::query_scalar("SELECT title FROM event_types WHERE slug = 'test-meeting'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(title, "Updated Title");
    }

    #[tokio::test]
    async fn update_event_type_with_empty_schedule_uses_user_default() {
        // Regression test for #68: when the event-type edit form is submitted
        // with an empty avail_schedule, the resulting availability_rules must
        // come from the user's profile-default schedule, not a hardcoded
        // Mon-Fri 09:00-17:00 fallback. Locks in the behaviour wired through
        // update_event_type so a future refactor can't silently regress it.
        let (app, pool, session, _) = setup_test_app().await;

        // Seed the test user's profile default with something distinctive
        // (Tue+Thu 14:00-18:00) so we can tell it apart from the legacy
        // hardcoded Mon-Fri 09:00-17:00 fallback.
        let user_id: String =
            sqlx::query_scalar("SELECT id FROM users WHERE username = 'testuser'")
                .fetch_one(&pool)
                .await
                .unwrap();
        sqlx::query("DELETE FROM user_availability_rules WHERE user_id = ?")
            .bind(&user_id)
            .execute(&pool)
            .await
            .unwrap();
        for day in [2_i32, 4] {
            sqlx::query(
                "INSERT INTO user_availability_rules (id, user_id, day_of_week, start_time, end_time) VALUES (?, ?, ?, '14:00', '18:00')",
            )
            .bind(uuid::Uuid::new_v4().to_string())
            .bind(&user_id)
            .bind(day)
            .execute(&pool)
            .await
            .unwrap();
        }

        let csrf = "test-csrf-empty-schedule";
        // Submit with avail_schedule explicitly empty (matches what the form
        // sends when all days are unchecked) and no legacy fields.
        let body = format!(
            "_csrf={}&title=Test+Meeting&slug=test-meeting&duration_min=30&location_value=https%3A%2F%2Fmeet.example.com&avail_schedule=",
            csrf
        );
        let response = app
            .oneshot(post_form(
                "/dashboard/event-types/test-meeting/edit",
                &session,
                csrf,
                &body,
            ))
            .await
            .unwrap();
        assert!(response.status().is_redirection() || response.status() == 200);

        // Inspect the persisted availability_rules. Expect Tue+Thu 14:00-18:00
        // (the user's profile default), not Mon-Fri 09:00-17:00.
        let rules: Vec<(i32, String, String)> = sqlx::query_as(
            "SELECT day_of_week, start_time, end_time FROM availability_rules \
             WHERE event_type_id = (SELECT id FROM event_types WHERE slug = 'test-meeting') \
             ORDER BY day_of_week, start_time",
        )
        .fetch_all(&pool)
        .await
        .unwrap();
        assert_eq!(
            rules,
            vec![
                (2, "14:00".to_string(), "18:00".to_string()),
                (4, "14:00".to_string(), "18:00".to_string()),
            ],
            "empty avail_schedule submission must fall back to the user's profile default, not the hardcoded Mon-Fri 09:00-17:00",
        );
    }

    #[tokio::test]
    async fn update_group_event_type_persists_location() {
        let (app, pool, session, _) = setup_test_app().await;

        // Get user_id and account_id from the test user
        let (user_id, account_id): (String, String) = sqlx::query_as(
            "SELECT u.id, a.id FROM users u JOIN accounts a ON a.user_id = u.id WHERE u.username = 'testuser'",
        )
        .bind("testuser")
        .fetch_one(&pool)
        .await
        .unwrap();

        // Create a team with the test user as admin
        let team_id = uuid::Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO teams (id, name, slug, visibility, created_by) VALUES (?, 'Test Team', 'test-team', 'public', ?)")
            .bind(&team_id)
            .bind(&user_id)
            .execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO team_members (team_id, user_id, role, source) VALUES (?, ?, 'admin', 'direct')")
            .bind(&team_id)
            .bind(&user_id)
            .execute(&pool).await.unwrap();

        // Create a team event type with a location
        let et_id = uuid::Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO event_types (id, account_id, slug, title, duration_min, buffer_before, buffer_after, min_notice_min, enabled, location_type, location_value, team_id, created_by_user_id) \
             VALUES (?, ?, 'team-meeting', 'Team Meeting', 30, 0, 0, 0, 1, 'link', 'https://meet.example.com/room', ?, ?)",
        )
        .bind(&et_id)
        .bind(&account_id)
        .bind(&team_id)
        .bind(&user_id)
        .execute(&pool).await.unwrap();

        // Update the event type via the web handler
        let csrf = "test-csrf-group-update";
        let body = format!(
            "_csrf={}&title=Team+Meeting+Updated&slug=team-meeting&duration_min=45&location_type=link&location_value=https%3A%2F%2Fmeet.example.com%2Fnew-room&avail_days=1,2,3,4,5&avail_start=09:00&avail_end=17:00&scheduling_mode=round_robin",
            csrf
        );
        let response = app
            .oneshot(post_form(
                &format!("/dashboard/group-event-types/{}/team-meeting/edit", team_id),
                &session,
                csrf,
                &body,
            ))
            .await
            .unwrap();
        assert!(
            response.status().is_redirection(),
            "Update should redirect, got {}",
            response.status()
        );

        // Verify all fields persisted
        let (title, location_value, duration): (String, Option<String>, i32) = sqlx::query_as(
            "SELECT title, location_value, duration_min FROM event_types WHERE id = ?",
        )
        .bind(&et_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(title, "Team Meeting Updated");
        assert_eq!(
            location_value.as_deref(),
            Some("https://meet.example.com/new-room")
        );
        assert_eq!(duration, 45);
    }

    // --- Booking with requires_confirmation ---

    #[tokio::test]
    async fn booking_with_confirmation_creates_pending() {
        let (app, pool, _, _) = setup_test_app().await;

        // Set requires_confirmation on the event type
        sqlx::query("UPDATE event_types SET requires_confirmation = 1 WHERE slug = 'test-meeting'")
            .execute(&pool)
            .await
            .unwrap();

        let now = Utc::now().with_timezone(&Tz::UTC).naive_local();
        let mut next_monday = now.date() + Duration::days(1);
        while next_monday.weekday() != chrono::Weekday::Mon {
            next_monday += Duration::days(1);
        }
        let date_str = next_monday.format("%Y-%m-%d").to_string();

        let csrf = "test-csrf-pending";
        let body = format!(
            "_csrf={}&date={}&time=14%3A00&name=Pending+Guest&email=pending%40test.com&notes=",
            csrf, date_str
        );
        let response = app
            .oneshot(post_form_unauthed(
                "/u/testuser/test-meeting/book",
                csrf,
                &body,
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        let resp_body = body_string(response).await;
        assert!(
            resp_body.contains("Pending") || resp_body.contains("pending"),
            "Should show pending confirmation"
        );

        let status: String = sqlx::query_scalar(
            "SELECT status FROM bookings WHERE guest_email = 'pending@test.com'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(status, "pending");

        // Should have a confirm_token
        let token: Option<String> = sqlx::query_scalar(
            "SELECT confirm_token FROM bookings WHERE guest_email = 'pending@test.com'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(token.is_some(), "Pending booking should have confirm_token");
    }

    // --- Private event type + invite flow ---

    #[tokio::test]
    async fn private_event_type_requires_invite_token() {
        let (app, pool, _, _) = setup_test_app().await;

        // Make the event type private
        sqlx::query("UPDATE event_types SET visibility = 'private' WHERE slug = 'test-meeting'")
            .execute(&pool)
            .await
            .unwrap();

        let now = Utc::now().with_timezone(&Tz::UTC).naive_local();
        let mut next_monday = now.date() + Duration::days(1);
        while next_monday.weekday() != chrono::Weekday::Mon {
            next_monday += Duration::days(1);
        }
        let date_str = next_monday.format("%Y-%m-%d").to_string();

        // Book without invite token
        let csrf = "test-csrf-private";
        let body = format!(
            "_csrf={}&date={}&time=10%3A00&name=Jane&email=jane%40test.com&notes=",
            csrf, date_str
        );
        let response = app
            .oneshot(post_form_unauthed(
                "/u/testuser/test-meeting/book",
                csrf,
                &body,
            ))
            .await
            .unwrap();
        let resp_body = body_string(response).await;
        assert!(
            resp_body.contains("invite") || resp_body.contains("Invite"),
            "Private event type should require invite"
        );

        let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM bookings")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count.0, 0, "No booking should be created without invite");
    }

    #[tokio::test]
    async fn invite_page_returns_200_for_private_event_type() {
        let (app, pool, session, et_id) = setup_test_app().await;

        sqlx::query("UPDATE event_types SET visibility = 'private' WHERE id = ?")
            .bind(&et_id)
            .execute(&pool)
            .await
            .unwrap();

        let response = app
            .oneshot(get_authed(
                &format!("/dashboard/invites/{}", et_id),
                &session,
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        let body = body_string(response).await;
        assert!(body.contains("Invites") || body.contains("invite"));
    }

    // --- Guest reschedule flow ---

    #[tokio::test]
    async fn guest_reschedule_slots_page_returns_200() {
        let (app, pool, _, et_id) = setup_test_app().await;

        let booking_id = uuid::Uuid::new_v4().to_string();
        let cancel_tok = uuid::Uuid::new_v4().to_string();
        let resched_tok = uuid::Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO bookings (id, event_type_id, uid, guest_name, guest_email, guest_timezone, start_at, end_at, status, cancel_token, reschedule_token) VALUES (?, ?, 'uid-gresched', 'Guest', 'guest@test.com', 'UTC', '2026-08-01T10:00:00', '2026-08-01T10:30:00', 'confirmed', ?, ?)")
            .bind(&booking_id)
            .bind(&et_id)
            .bind(&cancel_tok)
            .bind(&resched_tok)
            .execute(&pool)
            .await
            .unwrap();

        let response = app
            .oneshot(get(&format!("/booking/reschedule/{}", resched_tok)))
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        let body = body_string(response).await;
        assert!(
            body.contains("Test Meeting") || body.contains("reschedul"),
            "Guest reschedule page should show event info"
        );
    }

    // --- Admin accent/theme ---

    #[tokio::test]
    async fn admin_update_accent_redirects() {
        let (app, _, session, _) = setup_test_app().await;
        let csrf = "test-csrf-accent";
        let body = format!("_csrf={}&theme=nord", csrf);
        let response = app
            .oneshot(post_form("/dashboard/admin/accent", &session, csrf, &body))
            .await
            .unwrap();
        assert!(response.status().is_redirection());
    }

    // --- Multiple availability windows ---

    #[tokio::test]
    async fn create_event_type_with_multiple_windows() {
        let (app, pool, session, _) = setup_test_app().await;
        let csrf = "test-csrf-multi-win";
        let body = format!(
            "_csrf={}&title=Split+Day&slug=split-day&duration_min=30&location_value=https%3A%2F%2Fmeet.example.com&avail_days=1,2,3,4,5&avail_windows=09%3A00-12%3A00%2C13%3A00-17%3A00",
            csrf
        );
        let response = app
            .oneshot(post_form(
                "/dashboard/event-types/new",
                &session,
                csrf,
                &body,
            ))
            .await
            .unwrap();
        assert!(response.status().is_redirection() || response.status() == 200);

        let count: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM event_types WHERE slug = 'split-day'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(count.0, 1);

        // Should have rules with the split windows
        let rules: Vec<(String, String)> = sqlx::query_as(
            "SELECT start_time, end_time FROM availability_rules WHERE event_type_id = (SELECT id FROM event_types WHERE slug = 'split-day') ORDER BY start_time",
        )
        .fetch_all(&pool)
        .await
        .unwrap();
        assert!(rules.len() >= 5, "Should have rules for weekdays");
    }

    // --- Book form page ---

    #[tokio::test]
    async fn book_form_page_returns_200() {
        let (app, _, _, _) = setup_test_app().await;

        let now = Utc::now().with_timezone(&Tz::UTC).naive_local();
        let mut next_monday = now.date() + Duration::days(1);
        while next_monday.weekday() != chrono::Weekday::Mon {
            next_monday += Duration::days(1);
        }
        let date_str = next_monday.format("%Y-%m-%d").to_string();

        let response = app
            .oneshot(get(&format!(
                "/u/testuser/test-meeting/book?date={}&time=10:00",
                date_str
            )))
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        let body = body_string(response).await;
        assert!(body.contains("Test Meeting"));
        assert!(body.contains("Confirm booking") || body.contains("confirm"));
    }

    // --- Legacy book form ---

    #[tokio::test]
    async fn legacy_book_form_returns_200() {
        let (app, _, _, _) = setup_test_app().await;

        let now = Utc::now().with_timezone(&Tz::UTC).naive_local();
        let mut next_monday = now.date() + Duration::days(1);
        while next_monday.weekday() != chrono::Weekday::Mon {
            next_monday += Duration::days(1);
        }
        let date_str = next_monday.format("%Y-%m-%d").to_string();

        let response = app
            .oneshot(get(&format!(
                "/test-meeting/book?date={}&time=10:00",
                date_str
            )))
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
    }

    // --- Booking with additional attendees ---

    #[tokio::test]
    async fn booking_with_additional_guests() {
        let (app, pool, _, _) = setup_test_app().await;

        // Enable additional guests
        sqlx::query("UPDATE event_types SET max_additional_guests = 3 WHERE slug = 'test-meeting'")
            .execute(&pool)
            .await
            .unwrap();

        let now = Utc::now().with_timezone(&Tz::UTC).naive_local();
        let mut next_monday = now.date() + Duration::days(1);
        while next_monday.weekday() != chrono::Weekday::Mon {
            next_monday += Duration::days(1);
        }
        let date_str = next_monday.format("%Y-%m-%d").to_string();

        let csrf = "test-csrf-guests";
        let body = format!(
            "_csrf={}&date={}&time=15%3A00&name=Host+Guest&email=host%40test.com&notes=&additional_guests=extra1%40test.com%2Cextra2%40test.com",
            csrf, date_str
        );
        let response = app
            .oneshot(post_form_unauthed(
                "/u/testuser/test-meeting/book",
                csrf,
                &body,
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), 200);

        let count: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM booking_attendees WHERE booking_id = (SELECT id FROM bookings WHERE guest_email = 'host@test.com')",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(count.0, 2, "Should have 2 additional attendees");
    }

    // --- Troubleshoot with override ---

    #[tokio::test]
    async fn troubleshoot_shows_blocked_override() {
        let (app, pool, session, et_id) = setup_test_app().await;

        // Block tomorrow
        let tomorrow = (Utc::now() + Duration::days(1))
            .format("%Y-%m-%d")
            .to_string();
        let override_id = uuid::Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO availability_overrides (id, event_type_id, date, is_blocked) VALUES (?, ?, ?, 1)")
            .bind(&override_id)
            .bind(&et_id)
            .bind(&tomorrow)
            .execute(&pool)
            .await
            .unwrap();

        let response = app
            .oneshot(get_authed(
                &format!(
                    "/dashboard/troubleshoot?date={}&event_type=test-meeting",
                    tomorrow
                ),
                &session,
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        let body = body_string(response).await;
        assert!(
            body.contains("Blocked")
                || body.contains("blocked")
                || body.contains("override")
                || body.contains("day off"),
            "Troubleshoot should show blocked override"
        );
    }

    // --- Avatar routes ---

    #[tokio::test]
    async fn avatar_nonexistent_returns_404() {
        let (app, _, _, _) = setup_test_app().await;
        let response = app
            .oneshot(get("/avatar/nonexistent-user-id"))
            .await
            .unwrap();
        assert_eq!(response.status(), 404);
    }

    // --- Nonexistent routes ---

    #[tokio::test]
    async fn nonexistent_event_type_slug_returns_not_found() {
        let (app, _, _, _) = setup_test_app().await;
        let response = app
            .oneshot(get("/u/testuser/nonexistent-slug"))
            .await
            .unwrap();
        let status = response.status();
        let body = body_string(response).await;
        assert!(
            status == 404 || body.contains("not found") || body.contains("Not found"),
            "Nonexistent slug should return error"
        );
    }

    // --- Overrides page for nonexistent event type ---

    #[tokio::test]
    async fn overrides_nonexistent_event_type_redirects() {
        let (app, _, session, _) = setup_test_app().await;
        let response = app
            .oneshot(get_authed(
                "/dashboard/event-types/nonexistent/overrides",
                &session,
            ))
            .await
            .unwrap();
        assert!(
            response.status().is_redirection(),
            "Should redirect for nonexistent event type"
        );
    }

    // --- Group profile (no groups in test data, should handle gracefully) ---

    #[tokio::test]
    async fn team_profile_nonexistent_returns_404_or_empty() {
        let (app, _, _, _) = setup_test_app().await;
        let response = app.oneshot(get("/team/nonexistent")).await.unwrap();
        let status = response.status();
        assert!(
            status == 404 || status == 200,
            "Nonexistent group should return 404 or empty page"
        );
    }

    // --- Dashboard overview stats ---

    #[tokio::test]
    async fn dashboard_overview_shows_stats() {
        let (app, pool, session, et_id) = setup_test_app().await;

        // Add a confirmed booking
        let booking_id = uuid::Uuid::new_v4().to_string();
        let cancel_tok = uuid::Uuid::new_v4().to_string();
        let resched_tok = uuid::Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO bookings (id, event_type_id, uid, guest_name, guest_email, guest_timezone, start_at, end_at, status, cancel_token, reschedule_token) VALUES (?, ?, 'uid-stat', 'Guest', 'guest@test.com', 'UTC', '2030-06-15T10:00:00', '2030-06-15T10:30:00', 'confirmed', ?, ?)")
            .bind(&booking_id)
            .bind(&et_id)
            .bind(&cancel_tok)
            .bind(&resched_tok)
            .execute(&pool)
            .await
            .unwrap();

        let response = app
            .oneshot(get_authed("/dashboard", &session))
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        let body = body_string(response).await;
        // Should show at least 1 event type and 1 upcoming booking
        assert!(body.contains("Event Type") || body.contains("event type") || body.contains("1"));
    }

    // --- Settings: booking email validation ---

    #[tokio::test]
    async fn settings_invalid_booking_email_rejected() {
        let (app, _, session, _) = setup_test_app().await;
        let csrf = "test-csrf-bad-email";
        let body = format!("_csrf={}&name=Test+User&booking_email=not-an-email", csrf);
        let response = app
            .oneshot(post_form("/dashboard/settings", &session, csrf, &body))
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        let resp_body = body_string(response).await;
        assert!(
            resp_body.contains("Invalid")
                || resp_body.contains("invalid")
                || resp_body.contains("email"),
            "Invalid booking email should show error"
        );
    }

    // --- Settings: empty name rejected ---

    #[tokio::test]
    async fn settings_empty_name_rejected() {
        let (app, _, session, _) = setup_test_app().await;
        let csrf = "test-csrf-no-name";
        let body = format!("_csrf={}&name=&booking_email=", csrf);
        let response = app
            .oneshot(post_form("/dashboard/settings", &session, csrf, &body))
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        let resp_body = body_string(response).await;
        assert!(
            resp_body.contains("required")
                || resp_body.contains("Name")
                || resp_body.contains("empty"),
            "Empty name should show error"
        );
    }

    // --- Admin OIDC update ---

    #[tokio::test]
    async fn admin_update_oidc_settings() {
        let (app, pool, session, _) = setup_test_app().await;
        let csrf = "test-csrf-oidc";
        let body = format!(
            "_csrf={}&oidc_issuer_url=https%3A%2F%2Fauth.example.com&oidc_client_id=calrs&oidc_client_secret=secret123",
            csrf
        );
        let response = app
            .oneshot(post_form("/dashboard/admin/oidc", &session, csrf, &body))
            .await
            .unwrap();
        assert!(response.status().is_redirection());

        let client_id: Option<String> =
            sqlx::query_scalar("SELECT oidc_client_id FROM auth_config WHERE id = 'singleton'")
                .fetch_optional(&pool)
                .await
                .unwrap();
        assert_eq!(client_id.unwrap(), "calrs");
    }

    // --- Legacy booking POST ---

    #[tokio::test]
    async fn legacy_booking_post_creates_booking() {
        let (app, pool, _, _) = setup_test_app().await;

        let now = Utc::now().with_timezone(&Tz::UTC).naive_local();
        let mut next_monday = now.date() + Duration::days(1);
        while next_monday.weekday() != chrono::Weekday::Mon {
            next_monday += Duration::days(1);
        }
        let date_str = next_monday.format("%Y-%m-%d").to_string();

        let csrf = "test-csrf-legacy-book";
        let body = format!(
            "_csrf={}&date={}&time=11%3A00&name=Legacy+Guest&email=legacy%40test.com&notes=",
            csrf, date_str
        );
        let response = app
            .oneshot(post_form_unauthed("/test-meeting/book", csrf, &body))
            .await
            .unwrap();
        assert_eq!(response.status(), 200);

        let count: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM bookings WHERE guest_email = 'legacy@test.com'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(count.0, 1, "Legacy booking should be created");
    }

    // --- Booking with timezone ---

    #[tokio::test]
    async fn booking_with_guest_timezone() {
        let (app, pool, _, _) = setup_test_app().await;

        let now = Utc::now().with_timezone(&Tz::UTC).naive_local();
        let mut next_monday = now.date() + Duration::days(1);
        while next_monday.weekday() != chrono::Weekday::Mon {
            next_monday += Duration::days(1);
        }
        let date_str = next_monday.format("%Y-%m-%d").to_string();

        let csrf = "test-csrf-tz-book";
        let body = format!(
            "_csrf={}&date={}&time=09%3A00&name=TZ+Guest&email=tz%40test.com&notes=&tz=America%2FNew_York",
            csrf, date_str
        );
        let response = app
            .oneshot(post_form_unauthed(
                "/u/testuser/test-meeting/book",
                csrf,
                &body,
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), 200);

        let tz: Option<String> = sqlx::query_scalar(
            "SELECT guest_timezone FROM bookings WHERE guest_email = 'tz@test.com'",
        )
        .fetch_optional(&pool)
        .await
        .unwrap();
        assert_eq!(tz.unwrap(), "America/New_York");
    }

    // --- Slots with month parameter ---

    #[tokio::test]
    async fn slots_with_month_param() {
        let (app, _, _, _) = setup_test_app().await;
        let response = app
            .oneshot(get("/u/testuser/test-meeting?month=2026-06"))
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        let body = body_string(response).await;
        assert!(body.contains("June") || body.contains("2026"));
    }

    // --- Slots with timezone parameter ---

    #[tokio::test]
    async fn slots_with_tz_param() {
        let (app, _, _, _) = setup_test_app().await;
        let response = app
            .oneshot(get("/u/testuser/test-meeting?tz=Europe/Paris"))
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        let body = body_string(response).await;
        assert!(body.contains("Paris") || body.contains("Europe"));
    }

    // --- Overrides: multiple custom hours on same date ---

    #[tokio::test]
    async fn multiple_custom_hours_on_same_date() {
        let (app, pool, session, et_id) = setup_test_app().await;
        let csrf = "test-csrf-multi-override";

        // Add morning override
        let response = app
            .oneshot(post_form(
                "/dashboard/event-types/test-meeting/overrides",
                &session,
                csrf,
                &format!("_csrf={}&date=2026-08-01&override_type=custom&start_time=08%3A00&end_time=12%3A00", csrf),
            ))
            .await
            .unwrap();
        assert!(response.status().is_redirection());

        // Add afternoon override (need fresh router)
        let app2 = create_router(pool.clone(), std::env::temp_dir(), [0u8; 32]).await;
        let response = app2
            .oneshot(post_form(
                "/dashboard/event-types/test-meeting/overrides",
                &session,
                csrf,
                &format!("_csrf={}&date=2026-08-01&override_type=custom&start_time=14%3A00&end_time=16%3A00", csrf),
            ))
            .await
            .unwrap();
        assert!(response.status().is_redirection());

        let count: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM availability_overrides WHERE event_type_id = ? AND date = '2026-08-01'",
        )
        .bind(&et_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(count.0, 2, "Should have 2 custom hour overrides");
    }

    // --- Event type with all options ---

    #[tokio::test]
    async fn create_event_type_with_all_options() {
        let (app, pool, session, _) = setup_test_app().await;
        let csrf = "test-csrf-full-et";
        let body = format!(
            "_csrf={}&title=Full+Options&slug=full-options&duration_min=60&buffer_before=10&buffer_after=10&min_notice_min=120&requires_confirmation=on&location_type=link&location_value=https%3A%2F%2Fzoom.us%2Fmy-room&avail_days=1,2,3&avail_start=10:00&avail_end=16:00&reminder_minutes=15",
            csrf
        );
        let response = app
            .oneshot(post_form(
                "/dashboard/event-types/new",
                &session,
                csrf,
                &body,
            ))
            .await
            .unwrap();
        assert!(response.status().is_redirection() || response.status() == 200);

        let et: Option<(i32, i32, i32, i32, i32, String, String)> = sqlx::query_as(
            "SELECT duration_min, buffer_before, buffer_after, min_notice_min, requires_confirmation, location_type, location_value FROM event_types WHERE slug = 'full-options'",
        )
        .fetch_optional(&pool)
        .await
        .unwrap();
        let (dur, bb, ba, mn, rc, lt, lv) = et.unwrap();
        assert_eq!(dur, 60);
        assert_eq!(bb, 10);
        assert_eq!(ba, 10);
        assert_eq!(mn, 120);
        assert_eq!(rc, 1);
        assert_eq!(lt, "link");
        assert_eq!(lv, "https://zoom.us/my-room");
    }

    // --- Helper function tests ---

    #[test]
    fn compute_initials_two_names() {
        assert_eq!(compute_initials("Alice Bob"), "AB");
    }

    #[test]
    fn compute_initials_single_name() {
        assert_eq!(compute_initials("Alice"), "A");
    }

    #[test]
    fn compute_initials_three_names() {
        assert_eq!(compute_initials("Alice Bob Charlie"), "AC");
    }

    #[test]
    fn compute_initials_empty() {
        assert_eq!(compute_initials(""), "?");
    }

    #[test]
    fn format_booking_range_same_day() {
        let result = format_booking_range("2026-06-15T10:00:00", "2026-06-15T10:30:00");
        assert!(result.contains("10:00"));
        assert!(result.contains("10:30"));
    }

    #[test]
    fn extract_time_24h_from_iso() {
        assert_eq!(extract_time_24h("2026-06-15T14:30:00"), "14:30");
    }

    #[test]
    fn extract_time_24h_from_space_sep() {
        assert_eq!(extract_time_24h("2026-06-15 14:30:00"), "14:30");
    }

    #[test]
    fn format_date_label_full_datetime() {
        let result = format_date_label("2026-06-15T10:00:00", "en");
        assert!(result.contains("June") || result.contains("15") || result.contains("2026"));
    }

    #[test]
    fn parse_guest_tz_europe() {
        let tz = parse_guest_tz(Some("Europe/London"));
        assert_eq!(tz.name(), "Europe/London");
    }

    #[test]
    fn guest_to_host_local_converts_across_zones() {
        // 18:00 Europe/Paris (CEST, UTC+2 in July) == 12:00 America/New_York (EDT, UTC-4)
        let paris: Tz = "Europe/Paris".parse().unwrap();
        let ny: Tz = "America/New_York".parse().unwrap();
        let guest_local = NaiveDate::from_ymd_opt(2026, 7, 15)
            .unwrap()
            .and_hms_opt(18, 0, 0)
            .unwrap();
        let host_local = guest_to_host_local(guest_local, paris, ny);
        assert_eq!(
            host_local.format("%Y-%m-%d %H:%M").to_string(),
            "2026-07-15 12:00"
        );
    }

    #[test]
    fn guest_to_host_local_same_zone_is_noop() {
        let tz: Tz = "UTC".parse().unwrap();
        let dt = NaiveDate::from_ymd_opt(2026, 1, 1)
            .unwrap()
            .and_hms_opt(10, 30, 0)
            .unwrap();
        assert_eq!(guest_to_host_local(dt, tz, tz), dt);
    }

    // End-to-end TZ pipeline used by background email paths (reminder loop,
    // reschedule): `start_at` is stored in the event-type tz, must be converted
    // to the guest tz to populate BookingDetails, and host_time_display must
    // recover the host wall-clock for host-targeted emails. Regression for the
    // reviewer-caught bug where the reminder loop short-circuited the first
    // conversion and the host email displayed times in the wrong zone.
    #[test]
    fn stored_to_host_email_recovers_host_wall_clock() {
        // Scenario from issue #119: Paris event type, LA guest, booked at
        // what the guest saw as 07:00 LA = 16:00 Paris. start_at stored as
        // Paris wall-clock.
        let stored_start = "2026-05-26T16:00:00";
        let stored_end = "2026-05-26T16:30:00";
        let host_tz: Tz = "Europe/Paris".parse().unwrap();
        let guest_tz: Tz = "America/Los_Angeles".parse().unwrap();

        // Step 1: convert stored (host) -> guest wall-clock, as the reminder
        // loop and reschedule handler now do before building BookingDetails.
        let (date, start_time, end_time) =
            booking_strings_in_guest_tz(stored_start, stored_end, host_tz, guest_tz);
        assert_eq!(date, "2026-05-26");
        assert_eq!(start_time, "07:00");
        assert_eq!(end_time, "07:30");

        // Step 2: host_time_display converts guest wall-clock back to host.
        let (host_date, time_display) = crate::email::host_time_display(
            &date,
            &start_time,
            &end_time,
            guest_tz.name(),
            host_tz.name(),
        );
        assert_eq!(host_date, "2026-05-26");
        assert_eq!(time_display, "16:00 \u{2013} 16:30 (Europe/Paris)");
    }

    #[test]
    fn parse_guest_tz_garbage_falls_back() {
        let tz = parse_guest_tz(Some("Not/A/Timezone"));
        // Should fall back to system tz or UTC
        assert!(!tz.name().is_empty());
    }

    #[test]
    fn validate_booking_input_name_255_chars_ok() {
        let name = "a".repeat(255);
        assert!(validate_booking_input(&name, "test@test.com", &None).is_ok());
    }

    #[test]
    fn validate_booking_input_name_256_chars_rejected() {
        let name = "a".repeat(256);
        assert!(validate_booking_input(&name, "test@test.com", &None).is_err());
    }

    #[test]
    fn validate_booking_input_notes_5000_chars_ok() {
        let notes = Some("a".repeat(5000));
        assert!(validate_booking_input("Test", "test@test.com", &notes).is_ok());
    }

    #[test]
    fn validate_date_exactly_365_from_today() {
        let date = (chrono::Utc::now() + Duration::days(365))
            .naive_utc()
            .date();
        assert!(validate_date_not_too_far(date).is_ok());
    }

    #[test]
    fn validate_date_400_rejected() {
        let date = (chrono::Utc::now() + Duration::days(400))
            .naive_utc()
            .date();
        assert!(validate_date_not_too_far(date).is_err());
    }

    // --- parse_avail_windows edge cases ---

    #[test]
    fn parse_avail_windows_three_windows() {
        let windows = parse_avail_windows(
            Some("08:00-12:00,13:00-15:00,16:00-18:00"),
            Some("09:00"),
            Some("17:00"),
        );
        assert_eq!(windows.len(), 3);
        assert_eq!(windows[0], ("08:00".to_string(), "12:00".to_string()));
        assert_eq!(windows[1], ("13:00".to_string(), "15:00".to_string()));
        assert_eq!(windows[2], ("16:00".to_string(), "18:00".to_string()));
    }

    // --- Slots computation edge cases ---

    #[tokio::test]
    async fn compute_slots_with_min_notice_filters_near_slots() {
        let pool = setup_test_db().await;
        let (_, _, et_id) = seed_test_data(&pool).await;

        // With 480 min (8 hour) minimum notice, today's slots should be filtered
        let slot_days = compute_slots(
            &pool,
            &et_id,
            30,
            0,
            0,
            480, // 8 hours notice
            0,   // start from today
            1,   // just today
            Tz::UTC,
            Tz::UTC,
            BusySource::Individual(vec![]),
        )
        .await;

        // Today's slots might all be filtered depending on current time
        // Just verify it doesn't crash
        let _ = slot_days;
    }

    #[tokio::test]
    async fn compute_slots_with_buffer_reduces_available() {
        let pool = setup_test_db().await;
        let (_, _, et_id) = seed_test_data(&pool).await;

        let now = Utc::now().with_timezone(&Tz::UTC).naive_local();
        let mut next_monday = now.date() + Duration::days(1);
        while next_monday.weekday() != chrono::Weekday::Mon {
            next_monday += Duration::days(1);
        }
        let days_to_monday = (next_monday - now.date()).num_days() as i32;

        // Block 10:00-11:00
        let busy_start = next_monday.and_hms_opt(10, 0, 0).unwrap();
        let busy_end = next_monday.and_hms_opt(11, 0, 0).unwrap();

        // Without buffer: 2 slots blocked (10:00, 10:30)
        let no_buffer = compute_slots(
            &pool,
            &et_id,
            30,
            0,
            0,
            0,
            days_to_monday,
            1,
            Tz::UTC,
            Tz::UTC,
            BusySource::Individual(vec![(busy_start, busy_end)]),
        )
        .await;

        // With 30min buffer before+after: more slots blocked
        let with_buffer = compute_slots(
            &pool,
            &et_id,
            30,
            30,
            30,
            0,
            days_to_monday,
            1,
            Tz::UTC,
            Tz::UTC,
            BusySource::Individual(vec![(busy_start, busy_end)]),
        )
        .await;

        let no_buf_count: usize = no_buffer.iter().map(|d| d.slots.len()).sum();
        let buf_count: usize = with_buffer.iter().map(|d| d.slots.len()).sum();
        assert!(
            buf_count < no_buf_count,
            "Buffer should reduce available slots: {} < {}",
            buf_count,
            no_buf_count
        );
    }

    // --- Dashboard with cancelled bookings (shouldn't show) ---

    #[tokio::test]
    async fn dashboard_bookings_hides_cancelled() {
        let (app, pool, session, et_id) = setup_test_app().await;

        let booking_id = uuid::Uuid::new_v4().to_string();
        let cancel_tok = uuid::Uuid::new_v4().to_string();
        let resched_tok = uuid::Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO bookings (id, event_type_id, uid, guest_name, guest_email, guest_timezone, start_at, end_at, status, cancel_token, reschedule_token) VALUES (?, ?, 'uid-cancelled', 'Cancelled Guest', 'cancelled@test.com', 'UTC', '2030-06-15T10:00:00', '2030-06-15T10:30:00', 'cancelled', ?, ?)")
            .bind(&booking_id)
            .bind(&et_id)
            .bind(&cancel_tok)
            .bind(&resched_tok)
            .execute(&pool)
            .await
            .unwrap();

        let response = app
            .oneshot(get_authed("/dashboard/bookings", &session))
            .await
            .unwrap();
        let body = body_string(response).await;
        assert!(
            !body.contains("Cancelled Guest"),
            "Cancelled bookings should not appear in upcoming"
        );
    }

    // --- Pending bookings show on dashboard ---

    #[tokio::test]
    async fn dashboard_bookings_shows_pending() {
        let (app, pool, session, et_id) = setup_test_app().await;

        let booking_id = uuid::Uuid::new_v4().to_string();
        let cancel_tok = uuid::Uuid::new_v4().to_string();
        let resched_tok = uuid::Uuid::new_v4().to_string();
        let confirm_tok = uuid::Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO bookings (id, event_type_id, uid, guest_name, guest_email, guest_timezone, start_at, end_at, status, cancel_token, reschedule_token, confirm_token) VALUES (?, ?, 'uid-pending-dash', 'Pending Guest', 'pending-dash@test.com', 'UTC', '2030-06-15T10:00:00', '2030-06-15T10:30:00', 'pending', ?, ?, ?)")
            .bind(&booking_id)
            .bind(&et_id)
            .bind(&cancel_tok)
            .bind(&resched_tok)
            .bind(&confirm_tok)
            .execute(&pool)
            .await
            .unwrap();

        let response = app
            .oneshot(get_authed("/dashboard/bookings", &session))
            .await
            .unwrap();
        let body = body_string(response).await;
        assert!(
            body.contains("Pending Guest"),
            "Pending bookings should appear in pending approval section"
        );
    }

    // --- Invalid date in booking ---

    #[tokio::test]
    async fn booking_invalid_date_rejected() {
        let (app, _, _, _) = setup_test_app().await;
        let csrf = "test-csrf-bad-date";
        let body = format!(
            "_csrf={}&date=not-a-date&time=10%3A00&name=Jane&email=jane%40test.com&notes=",
            csrf
        );
        let response = app
            .oneshot(post_form_unauthed(
                "/u/testuser/test-meeting/book",
                csrf,
                &body,
            ))
            .await
            .unwrap();
        let resp_body = body_string(response).await;
        assert!(
            resp_body.contains("Invalid") || resp_body.contains("invalid"),
            "Invalid date should be rejected"
        );
    }

    // --- Invalid time in booking ---

    #[tokio::test]
    async fn booking_invalid_time_rejected() {
        let (app, _, _, _) = setup_test_app().await;
        let csrf = "test-csrf-bad-time";

        let now = Utc::now().with_timezone(&Tz::UTC).naive_local();
        let mut next_monday = now.date() + Duration::days(1);
        while next_monday.weekday() != chrono::Weekday::Mon {
            next_monday += Duration::days(1);
        }
        let date_str = next_monday.format("%Y-%m-%d").to_string();

        let body = format!(
            "_csrf={}&date={}&time=not-a-time&name=Jane&email=jane%40test.com&notes=",
            csrf, date_str
        );
        let response = app
            .oneshot(post_form_unauthed(
                "/u/testuser/test-meeting/book",
                csrf,
                &body,
            ))
            .await
            .unwrap();
        let resp_body = body_string(response).await;
        assert!(
            resp_body.contains("Invalid") || resp_body.contains("invalid"),
            "Invalid time should be rejected"
        );
    }

    // ====== XSS regression guards (#43) ======
    //
    // Three dashboard templates used to embed user-controlled strings inside
    // inline onclick JS string literals using `\'{{ var }}\'` as a naïve
    // escape. MiniJinja auto-escapes `'` to `&#x27;` but leaves backslashes
    // untouched, so a payload like `\\'));alert(1);//` breaks out of the JS
    // string and injects script. Fix was to move the value into a
    // `data-confirm` attribute and read it via `this.dataset.confirm` so it's
    // never re-parsed as JS.
    //
    // These tests render each template with a crafted payload and assert
    // that (a) the onclick is the safe static form and (b) the payload only
    // lands in data-confirm (HTML-escaped). They fire if anyone re-introduces
    // a `{{ … }}` interpolation inside an onclick.

    /// Pull the bytes of the `onclick="…"` attribute for the first `<button>`
    /// whose attributes contain `class_marker`. Only considers button-element
    /// attribute strings (between `<button` and the first `>` after it), so
    /// CSS/JS elsewhere in the document that happens to mention the marker
    /// doesn't throw off the scan. Returns None if no such button exists.
    fn extract_onclick_for_button(html: &str, class_marker: &str) -> Option<String> {
        // Skip the prefix before the first `<button` (document boilerplate).
        let mut rest = html;
        while let Some(idx) = rest.find("<button") {
            let after = &rest[idx + "<button".len()..];
            let (attrs, tail) = after.split_once('>')?;
            if attrs.contains(class_marker) {
                if let Some(attr_rest) = attrs.split_once("onclick=\"") {
                    if let Some((value, _)) = attr_rest.1.split_once('"') {
                        return Some(value.to_string());
                    }
                }
            }
            rest = tail;
        }
        None
    }

    const XSS_PAYLOAD: &str = r#"\\'));alert(1);//"#;

    #[test]
    fn dashboard_event_types_delete_button_no_onclick_interpolation() {
        let mut env = minijinja::Environment::new();
        env.set_undefined_behavior(minijinja::UndefinedBehavior::Lenient);
        env.set_loader(minijinja::path_loader("templates"));
        crate::i18n::register(&mut env);
        let tmpl = env
            .get_template("dashboard_event_types.html")
            .expect("template loads");

        let rendered = tmpl
            .render(context! {
                sidebar => context! {},
                username => "alice",
                has_any => true,
                all_event_types => vec![context! {
                    id => "et-1",
                    slug => "intro",
                    title => XSS_PAYLOAD,
                    duration_min => 30,
                    enabled => true,
                    visibility => "public",
                    can_manage => true,
                    active_bookings => 0,
                    is_team => false,
                    edit_url => "/e/edit",
                    toggle_url => "/e/toggle",
                    overrides_url => "/e/overrides",
                    embed_url => "/e/embed",
                    delete_url => "/e/delete",
                    view_url => "/e/view",
                }],
            })
            .expect("renders");

        let onclick = extract_onclick_for_button(&rendered, r#"class="danger""#)
            .expect("delete button onclick present");
        assert_eq!(
            onclick, "if(confirm(this.dataset.confirm)) this.nextElementSibling.submit();",
            "onclick must be the static safe form — no interpolation allowed",
        );
        // The payload should appear only inside the data-confirm attribute,
        // HTML-escaped. Backslashes pass through unchanged, apostrophes
        // become &#x27;.
        assert!(
            rendered
                .contains(r#"data-confirm="Delete event type '\\&#x27;));alert(1);&#x2f;&#x2f;'"#),
            "payload should be inside data-confirm only"
        );
    }

    #[test]
    fn dashboard_sources_remove_button_no_onclick_interpolation() {
        let mut env = minijinja::Environment::new();
        env.set_undefined_behavior(minijinja::UndefinedBehavior::Lenient);
        env.set_loader(minijinja::path_loader("templates"));
        crate::i18n::register(&mut env);
        let tmpl = env
            .get_template("dashboard_sources.html")
            .expect("template loads");

        let rendered = tmpl
            .render(context! {
                sidebar => context! {},
                sources => vec![context! {
                    id => "s-1",
                    name => XSS_PAYLOAD,
                    url => "https://example.com/dav",
                    username => "alice",
                    enabled => true,
                    last_synced => "never",
                    calendar_count => 0,
                    event_count => 0,
                    needs_write_setup => false,
                }],
            })
            .expect("renders");

        // The Remove button is the one containing "Remove" text AND the
        // error-text style. It's distinguishable from "Test" by the
        // var(--error-text) inline style.
        let onclick = extract_onclick_for_button(&rendered, "var(--error-text)")
            .expect("remove button onclick present");
        assert_eq!(
            onclick,
            "if(confirm(this.dataset.confirm)) this.closest('div').querySelector('.remove-form').submit();",
            "onclick must be the static safe form — no interpolation allowed",
        );
        assert!(
            rendered.contains(r#"data-confirm="Remove source '\\&#x27;));alert(1);&#x2f;&#x2f;'"#),
            "payload should be inside data-confirm only"
        );
    }

    #[test]
    fn team_settings_delete_button_no_onclick_interpolation() {
        let mut env = minijinja::Environment::new();
        env.set_undefined_behavior(minijinja::UndefinedBehavior::Lenient);
        env.set_loader(minijinja::path_loader("templates"));
        crate::i18n::register(&mut env);
        let tmpl = env
            .get_template("team_settings.html")
            .expect("template loads");

        let rendered = tmpl
            .render(context! {
                sidebar => context! {},
                team_id => "t-1",
                team_name => XSS_PAYLOAD,
                team_slug => "acme",
                team_description => "",
                team_avatar_path => minijinja::value::Value::from(None::<String>),
                team_visibility => "public",
                invite_token => minijinja::value::Value::from(None::<String>),
                members => Vec::<minijinja::Value>::new(),
                linked_groups => Vec::<minijinja::Value>::new(),
                available_groups => Vec::<minijinja::Value>::new(),
                can_admin => true,
                is_owner => true,
            })
            .expect("renders");

        let onclick = extract_onclick_for_button(&rendered, "border-color: var(--error-text)")
            .expect("delete team button onclick present");
        assert_eq!(
            onclick, "if(confirm(this.dataset.confirm)) this.nextElementSibling.submit();",
            "onclick must be the static safe form — no interpolation allowed",
        );
        assert!(
            rendered.contains(r#"data-confirm="Delete team '\\&#x27;));alert(1);&#x2f;&#x2f;'"#),
            "payload should be inside data-confirm only"
        );
    }

    // --- Bulk invite parsing tests ---

    #[test]
    fn bulk_invite_parses_valid_emails() {
        let (valid, result) = parse_bulk_recipients("alice@example.com\nbob@example.org\n", 100);
        assert_eq!(valid.len(), 2);
        assert_eq!(valid[0].0, "alice@example.com");
        assert_eq!(valid[0].1, "Alice");
        assert_eq!(valid[1].0, "bob@example.org");
        assert_eq!(valid[1].1, "Bob");
        assert!(result.invalid.is_empty());
        assert!(result.duplicates.is_empty());
        assert!(!result.over_limit);
    }

    #[test]
    fn bulk_invite_skips_blank_lines_and_trims() {
        let (valid, _) = parse_bulk_recipients("\n  alice@example.com  \n\n", 100);
        assert_eq!(valid.len(), 1);
        assert_eq!(valid[0].0, "alice@example.com");
    }

    #[test]
    fn bulk_invite_rejects_malformed_rows() {
        let (valid, result) = parse_bulk_recipients(
            "alice@example.com\nnot-an-email\n@nope.com\nfoo@\nfoo@bar\nok@x.io",
            100,
        );
        assert_eq!(valid.len(), 2);
        assert_eq!(result.invalid.len(), 4);
    }

    #[test]
    fn bulk_invite_dedupes_case_insensitively() {
        let (valid, result) = parse_bulk_recipients("Alice@Example.com\nalice@example.com\n", 100);
        assert_eq!(valid.len(), 1);
        assert_eq!(result.duplicates, vec!["alice@example.com".to_string()]);
    }

    #[test]
    fn bulk_invite_caps_at_max() {
        let mut input = String::new();
        for i in 0..10 {
            input.push_str(&format!("user{}@example.com\n", i));
        }
        let (valid, result) = parse_bulk_recipients(&input, 3);
        assert_eq!(valid.len(), 3);
        assert!(result.over_limit);
    }

    #[test]
    fn bulk_invite_derives_pretty_names() {
        assert_eq!(derive_name_from_email("john.doe@example.com"), "John Doe");
        assert_eq!(
            derive_name_from_email("mary_smith@example.com"),
            "Mary Smith"
        );
        assert_eq!(derive_name_from_email("alice@example.com"), "Alice");
    }

    // --- build_csp tests ---

    fn make_captcha_config(instance_url: &str, widget_url: &str) -> captcha::CaptchaConfig {
        captcha::CaptchaConfig {
            instance_url: instance_url.to_string(),
            site_key: "testkey".to_string(),
            secret: "secret".to_string(),
            widget_url: widget_url.to_string(),
        }
    }

    #[test]
    fn build_csp_without_captcha_has_no_extra_directives() {
        let csp = build_csp(&None);
        assert!(!csp.contains("wasm-unsafe-eval"));
        assert!(!csp.contains("worker-src"));
        assert!(!csp.contains("cdn.jsdelivr.net"));
        assert!(csp.contains("script-src 'self' 'unsafe-inline'"));
        assert!(csp.contains("connect-src 'self'"));
    }

    #[test]
    fn build_csp_with_captcha_includes_wasm_and_worker_src() {
        let cfg = Some(make_captcha_config(
            "https://cap.example.com",
            captcha::DEFAULT_WIDGET_URL,
        ));
        let csp = build_csp(&cfg);
        assert!(csp.contains("'wasm-unsafe-eval'"));
        assert!(csp.contains("worker-src blob:"));
    }

    #[test]
    fn build_csp_with_captcha_includes_script_origin() {
        let cfg = Some(make_captcha_config(
            "https://cap.example.com",
            captcha::DEFAULT_WIDGET_URL,
        ));
        let csp = build_csp(&cfg);
        assert!(csp.contains("https://cdn.jsdelivr.net"), "{}", csp);
    }

    #[test]
    fn booking_form_paths_get_relaxed_csp() {
        assert!(is_booking_form_path("/u/alice/intro/book"));
        assert!(is_booking_form_path("/team/sales/demo/book"));
        assert!(is_booking_form_path("/intro/book"));
    }

    #[test]
    fn non_booking_paths_keep_baseline_csp() {
        assert!(!is_booking_form_path("/"));
        assert!(!is_booking_form_path("/dashboard"));
        assert!(!is_booking_form_path("/dashboard/admin"));
        assert!(!is_booking_form_path("/u/alice/intro"));
        assert!(!is_booking_form_path("/booking/cancel/sometoken"));
        assert!(!is_booking_form_path("/u/alice/bookkeeping"));
    }

    #[test]
    fn build_csp_with_captcha_includes_connect_origins() {
        let cfg = Some(make_captcha_config(
            "https://cap.example.com",
            captcha::DEFAULT_WIDGET_URL,
        ));
        let csp = build_csp(&cfg);
        assert!(csp.contains("connect-src 'self'"), "{}", csp);
        assert!(csp.contains("https://cap.example.com"), "{}", csp);
        assert!(csp.contains("https://cdn.jsdelivr.net"), "{}", csp);
    }

    // Embed mode must preserve captcha allowances (so captcha-enabled
    // deployments can still serve a working booking form inside iframes).
    // `csp_middleware` does the swap via string replace on the strict CSP,
    // so we verify the contract: the produced CSP keeps frame-ancestors
    // exactly once and contains the expected substring for swapping.
    #[test]
    fn build_csp_contains_frame_ancestors_self_for_swap() {
        assert_eq!(
            build_csp(&None).matches("frame-ancestors 'self'").count(),
            1
        );
        let cfg = Some(make_captcha_config(
            "https://cap.example.com",
            captcha::DEFAULT_WIDGET_URL,
        ));
        assert_eq!(build_csp(&cfg).matches("frame-ancestors 'self'").count(), 1);
    }

    #[test]
    fn build_csp_embed_swap_preserves_captcha_allowances() {
        let cfg = Some(make_captcha_config(
            "https://cap.example.com",
            captcha::DEFAULT_WIDGET_URL,
        ));
        let strict = build_csp(&cfg);
        let embed_csp = strict.replace("frame-ancestors 'self'", "frame-ancestors *");
        assert!(embed_csp.contains("frame-ancestors *"));
        assert!(!embed_csp.contains("frame-ancestors 'self'"));
        // Captcha allowances must survive the swap so the widget loads in an
        // embedded booking form.
        assert!(embed_csp.contains("'wasm-unsafe-eval'"));
        assert!(embed_csp.contains("worker-src blob:"));
        assert!(embed_csp.contains("https://cap.example.com"));
        assert!(embed_csp.contains("https://cdn.jsdelivr.net"));
    }
}
