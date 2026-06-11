//! Lead capture HTTP layer.
//!
//! Owns every web-facing piece of the iClosed-style lead capture feature:
//! the public `/api/lead-capture` endpoint, the host's `/dashboard/leads`
//! page and worklist actions, the admin-side toggles (global capture +
//! legal-mentions URL), the per-event-type toggle helper used by the main
//! event-type form, and the background notifier/purge loop.
//!
//! The storage layer (`src/leads/`) stays provider-agnostic; only the
//! presentation/HTTP concerns live here. Phone-number form helpers stay in
//! `super::mod` because they cross-cut the booking flow, not lead capture.
//!
//! Wiring:
//! - `super::create_router()` registers the routes and references handlers
//!   by their short names via `use super::leads::*` at the top of `mod.rs`.
//! - `main.rs` spawns [`run_lead_purge_loop`] via `web::run_lead_purge_loop`,
//!   re-exported from `super`.

use axum::extract::{Form, Path, State};
use axum::http::HeaderMap;
use axum::response::{Html, IntoResponse, Redirect};
use minijinja::context;
use serde::Deserialize;
use sqlx::SqlitePool;
use std::sync::Arc;

use super::{
    client_ip_for_rate_limit, impersonation_ctx, internal_error_body, internal_error_html,
    is_safe_company_link, sidebar_context, verify_csrf_token, AppState, CsrfForm,
};

// --- Legal mentions URL -----------------------------------------------------

#[derive(Deserialize)]
pub(crate) struct LegalMentionsForm {
    legal_mentions_url: String,
    _csrf: Option<String>,
}

/// Read the admin-configured legal-mentions URL from `auth_config`. Returns
/// `None` when unset, empty, or anything other than `http(s)://` — defence
/// in depth against a tampered DB row.
pub(crate) async fn get_legal_mentions_url(pool: &SqlitePool) -> Option<String> {
    sqlx::query_scalar::<_, Option<String>>(
        "SELECT legal_mentions_url FROM auth_config WHERE id = 'singleton'",
    )
    .fetch_optional(pool)
    .await
    .ok()
    .flatten()
    .flatten()
    .filter(|s| !s.is_empty())
    .filter(|s| is_safe_company_link(s))
}

pub(crate) async fn admin_update_legal_mentions(
    State(state): State<Arc<AppState>>,
    _admin: crate::auth::AdminUser,
    headers: HeaderMap,
    Form(form): Form<LegalMentionsForm>,
) -> impl IntoResponse {
    if let Err(resp) = verify_csrf_token(&headers, &form._csrf) {
        return resp;
    }
    let link = form.legal_mentions_url.trim().to_string();
    if !link.is_empty() && !is_safe_company_link(&link) {
        tracing::warn!(
            admin = %_admin.0.email,
            "admin: legal_mentions_url rejected (only http/https schemes allowed)"
        );
        let msg = urlencoding::encode("Legal mentions URL must start with http:// or https://")
            .into_owned();
        return Redirect::to(&format!("/dashboard/admin?error={}", msg)).into_response();
    }
    let value: Option<&str> = if link.is_empty() { None } else { Some(&link) };
    let _ = sqlx::query(
        "UPDATE auth_config SET legal_mentions_url = ?, updated_at = datetime('now') WHERE id = 'singleton'",
    )
    .bind(value)
    .execute(&state.pool)
    .await;
    *state.legal_mentions_url.write().await = if link.is_empty() { None } else { Some(link) };
    Redirect::to("/dashboard/admin").into_response()
}

// --- Acknowledgement timestamp formatting ----------------------------------

/// Render a SQLite `datetime('now')` value as a bare `YYYY-MM-DD` date for
/// the dashboard caption. Falls back to the raw string when the format isn't
/// what SQLite produces, so we never lose information even if the column was
/// populated by a future migration that uses a different format.
pub(crate) fn format_ack_date(raw: &str) -> String {
    chrono::NaiveDateTime::parse_from_str(raw.trim(), "%Y-%m-%d %H:%M:%S")
        .map(|dt| dt.format("%Y-%m-%d").to_string())
        .unwrap_or_else(|_| raw.to_string())
}

// --- Per-event-type capture context ----------------------------------------

/// Resolve the per-event-type lead-capture state for a booking page render.
/// Returns `(active, retention_days)` — `active` already factors the admin
/// global toggle, so callers don't have to.
pub(crate) async fn lead_capture_ctx(pool: &SqlitePool, event_type_id: &str) -> (bool, i64) {
    let global = crate::leads::config::global_settings(pool).await;
    if !global.enabled {
        return (false, global.retention_days);
    }
    let enabled = crate::leads::config::event_type_capture_enabled(pool, event_type_id).await;
    (enabled, global.retention_days)
}

/// Persist the per-event-type lead-capture toggle. Returns `Err(msg)` when the
/// caller is turning capture on without a prior acknowledgement and without
/// confirming now (RGPD guard). On success, stamps `lead_capture_acknowledged_at`
/// the first time the host confirms — never overwrites a prior timestamp so the
/// audit trail stays intact.
pub(crate) async fn apply_lead_capture_toggle(
    pool: &SqlitePool,
    et_id: &str,
    want_on: bool,
    acknowledging_now: bool,
) -> Result<(), &'static str> {
    let already_acknowledged: Option<String> = sqlx::query_scalar::<_, Option<String>>(
        "SELECT lead_capture_acknowledged_at FROM event_types WHERE id = ?",
    )
    .bind(et_id)
    .fetch_optional(pool)
    .await
    .unwrap_or(None)
    .flatten();
    if want_on && already_acknowledged.is_none() && !acknowledging_now {
        return Err("Please confirm that you have informed bookers their input is captured.");
    }
    let new_value: i64 = if want_on { 1 } else { 0 };
    if acknowledging_now && already_acknowledged.is_none() {
        let _ = sqlx::query(
            "UPDATE event_types SET lead_capture = ?, lead_capture_acknowledged_at = datetime('now') WHERE id = ?",
        )
        .bind(new_value)
        .bind(et_id)
        .execute(pool)
        .await;
    } else {
        let _ = sqlx::query("UPDATE event_types SET lead_capture = ? WHERE id = ?")
            .bind(new_value)
            .bind(et_id)
            .execute(pool)
            .await;
    }
    Ok(())
}

// --- Public capture API: POST /api/lead-capture -----------------------------

#[derive(Deserialize)]
pub(crate) struct LeadCapturePayload {
    /// CSRF token mirrored from the cookie via the same JS that drives the
    /// booking form.
    #[serde(default)]
    _csrf: Option<String>,
    /// Stable id chosen by the browser (sessionStorage). Treated as opaque.
    lead_id: String,
    /// Slug of the event type the guest is booking — resolved server-side
    /// to (event_type_id, host_user_id).
    event_type_slug: String,
    /// Host username for the `/u/{username}/{slug}` public URL. Required to
    /// disambiguate single-host event types, whose slugs are only unique
    /// *per account* — without it, a slug shared across hosts would bind the
    /// lead to whichever row the DB returned first. Absent for the legacy
    /// single-user `/{slug}` route, where slugs are effectively global.
    #[serde(default)]
    username: Option<String>,
    /// Optional team slug for team event types (`/team/{team}/{event-slug}`).
    /// When empty, slug is resolved against single-host event types.
    #[serde(default)]
    team_slug: Option<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    email: Option<String>,
    #[serde(default)]
    phone: Option<String>,
    #[serde(default)]
    notes: Option<String>,
    #[serde(default)]
    target_date: Option<String>,
    #[serde(default)]
    target_time: Option<String>,
    #[serde(default)]
    target_tz: Option<String>,
    #[serde(default)]
    utm_source: Option<String>,
    #[serde(default)]
    utm_medium: Option<String>,
    #[serde(default)]
    utm_campaign: Option<String>,
    #[serde(default)]
    referrer: Option<String>,
}

pub(crate) async fn lead_capture_record(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    axum::Json(payload): axum::Json<LeadCapturePayload>,
) -> impl IntoResponse {
    if let Err(resp) = verify_csrf_token(&headers, &payload._csrf) {
        return resp;
    }
    // Rate-limit per IP. Keystrokes plus debounce (~750ms) → at most ~1.3
    // events/sec; the 60/min cap leaves comfortable slack for honest users
    // and squashes scripted abuse.
    let ip = client_ip_for_rate_limit(&headers);
    if state.lead_capture_limiter.check_limited(&ip).await {
        return axum::http::StatusCode::TOO_MANY_REQUESTS.into_response();
    }

    if payload.lead_id.is_empty() || payload.lead_id.len() > 128 {
        return axum::http::StatusCode::BAD_REQUEST.into_response();
    }
    let lead_id_safe = payload.lead_id.chars().all(|c| c.is_ascii_graphic());
    if !lead_id_safe {
        return axum::http::StatusCode::BAD_REQUEST.into_response();
    }

    // Resolve the event type: handles both single-user (slug only) and team
    // (team_slug + slug) bookings. Internal/private event types are
    // *intentionally* excluded — those flows use invite tokens, and we don't
    // know who's typing until the form is submitted.
    let event_type: Option<(String, String, i32)> = match payload.team_slug.as_deref() {
        Some(team_slug) if !team_slug.is_empty() => sqlx::query_as(
            "SELECT et.id, COALESCE(et.created_by_user_id, ''), et.lead_capture
             FROM event_types et
             JOIN teams t ON t.id = et.team_id
             WHERE t.slug = ? AND et.slug = ? AND et.enabled = 1
               AND et.visibility = 'public'",
        )
        .bind(team_slug)
        .bind(&payload.event_type_slug)
        .fetch_optional(&state.pool)
        .await
        .unwrap_or(None),
        // `/u/{username}/{slug}` — scope by username so a slug shared across
        // hosts resolves to the right account. Mirrors the booking handler's
        // own `WHERE u.username = ? AND et.slug = ?` lookup.
        _ => match payload.username.as_deref().filter(|u| !u.is_empty()) {
            Some(username) => sqlx::query_as(
                "SELECT et.id, COALESCE(u.id, ''), et.lead_capture
                 FROM event_types et
                 JOIN accounts a ON a.id = et.account_id
                 JOIN users u ON u.id = a.user_id
                 WHERE u.username = ? AND et.slug = ? AND et.enabled = 1
                   AND u.enabled = 1 AND et.team_id IS NULL
                   AND et.visibility = 'public'",
            )
            .bind(username)
            .bind(&payload.event_type_slug)
            .fetch_optional(&state.pool)
            .await
            .unwrap_or(None),
            // Legacy single-user `/{slug}` route: no username in the URL, so
            // slugs are effectively global.
            None => sqlx::query_as(
                "SELECT et.id, COALESCE(a.user_id, ''), et.lead_capture
                 FROM event_types et
                 JOIN accounts a ON a.id = et.account_id
                 WHERE et.slug = ? AND et.enabled = 1 AND et.team_id IS NULL
                   AND et.visibility = 'public'",
            )
            .bind(&payload.event_type_slug)
            .fetch_optional(&state.pool)
            .await
            .unwrap_or(None),
        },
    };

    let (et_id, host_user_id, lead_capture) = match event_type {
        Some(t) => t,
        None => return axum::http::StatusCode::NOT_FOUND.into_response(),
    };

    // Per-event-type opt-in must be on, and the admin global toggle too —
    // checked together via `is_capture_active` to keep gating in one place.
    if lead_capture == 0 || !crate::leads::is_capture_active(&state.pool, &et_id).await {
        // 204 (rather than 4xx) so a stale browser tab silently stops
        // recording without surfacing an alarming error.
        return axum::http::StatusCode::NO_CONTENT.into_response();
    }

    let host_user_id = if host_user_id.is_empty() {
        None
    } else {
        Some(host_user_id)
    };
    let user_agent = headers
        .get("User-Agent")
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);

    let input = crate::leads::PartialBookingInput {
        event_type_id: et_id,
        host_user_id,
        lead_id: payload.lead_id,
        name: payload.name,
        email: payload.email,
        phone: payload.phone,
        notes: payload.notes,
        ip: Some(ip),
        user_agent,
        target_date: payload.target_date,
        target_time: payload.target_time,
        target_tz: payload.target_tz,
        utm_source: payload.utm_source,
        utm_medium: payload.utm_medium,
        utm_campaign: payload.utm_campaign,
        referrer: payload.referrer,
    };

    if let Err(e) = crate::leads::upsert_partial(&state.pool, input).await {
        tracing::warn!(error = %e, "lead-capture upsert failed");
        return axum::http::StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }

    axum::http::StatusCode::NO_CONTENT.into_response()
}

// --- Host dashboard / worklist ---------------------------------------------

pub(crate) async fn dashboard_leads(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
) -> impl IntoResponse {
    let user = &auth_user.user;
    let global = crate::leads::config::global_settings(&state.pool).await;

    // Admins see everyone's leads; regular users only their own.
    let scope = if user.role == "admin" {
        None
    } else {
        Some(user.id.as_str())
    };

    let leads = crate::leads::list_recent_for_user(&state.pool, scope, 200)
        .await
        .unwrap_or_default();
    let stats = crate::leads::stats_for_user(&state.pool, scope).await;

    let leads_ctx: Vec<minijinja::Value> = leads
        .iter()
        .map(|l| {
            let source = lead_source_label(l);
            context! {
                id => l.id,
                event_type_id => l.event_type_id,
                event_type_title => l.event_type_title.as_deref().unwrap_or(""),
                team_name => l.team_name.as_deref().unwrap_or(""),
                lead_id => l.lead_id,
                name => l.name.as_deref().unwrap_or(""),
                email => l.email.as_deref().unwrap_or(""),
                phone => l.phone.as_deref().unwrap_or(""),
                notes => l.notes.as_deref().unwrap_or(""),
                target_date => l.target_date.as_deref().unwrap_or(""),
                target_time => l.target_time.as_deref().unwrap_or(""),
                target_tz => l.target_tz.as_deref().unwrap_or(""),
                source => source,
                contacted => l.contacted_at.is_some(),
                created_at => l.created_at,
                updated_at => l.updated_at,
            }
        })
        .collect();

    let tmpl = match state.templates.get_template("dashboard_leads.html") {
        Ok(t) => t,
        Err(e) => return internal_error_html("template render", &e),
    };
    let (impersonating, impersonating_name, _) = impersonation_ctx(&auth_user);
    Html(
        tmpl.render(context! {
            sidebar => sidebar_context(&auth_user, "leads"),
            leads => leads_ctx,
            global_enabled => global.enabled,
            retention_days => global.retention_days,
            is_admin => user.role == "admin",
            stats_started => stats.started,
            stats_completed => stats.completed,
            stats_abandoned => stats.abandoned,
            stats_conversion => stats.conversion_pct(),
            impersonating => impersonating,
            impersonating_name => impersonating_name,
        })
        .unwrap_or_else(|e| internal_error_body("template render", &e)),
    )
}

/// Short human label for where a lead came from: prefer the UTM source
/// (with campaign in parens), else the referrer host, else empty.
fn lead_source_label(l: &crate::leads::db::PartialBooking) -> String {
    if let Some(src) = l.utm_source.as_deref().filter(|s| !s.is_empty()) {
        return match l.utm_campaign.as_deref().filter(|s| !s.is_empty()) {
            Some(c) => format!("{src} / {c}"),
            None => src.to_string(),
        };
    }
    if let Some(r) = l.referrer.as_deref().filter(|s| !s.is_empty()) {
        // Reduce a full URL to its host for compactness.
        let host = r
            .strip_prefix("https://")
            .or_else(|| r.strip_prefix("http://"))
            .unwrap_or(r);
        return host.split('/').next().unwrap_or(host).to_string();
    }
    String::new()
}

/// POST handler: toggle a lead's "contacted" flag (worklist).
pub(crate) async fn lead_set_contacted(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
    headers: HeaderMap,
    Path(id): Path<String>,
    Form(form): Form<CsrfForm>,
) -> impl IntoResponse {
    if let Err(resp) = verify_csrf_token(&headers, &form._csrf) {
        return resp;
    }
    let user = &auth_user.user;
    if user.role != "admin" && !crate::leads::user_can_access(&state.pool, &id, &user.id).await {
        return axum::http::StatusCode::FORBIDDEN.into_response();
    }
    // Toggle: if currently contacted, clear it; otherwise set it.
    let currently: Option<(Option<String>,)> =
        sqlx::query_as("SELECT contacted_at FROM partial_bookings WHERE id = ?")
            .bind(&id)
            .fetch_optional(&state.pool)
            .await
            .unwrap_or(None);
    let now_contacted = !matches!(currently, Some((Some(_),)));
    let _ = crate::leads::set_contacted(&state.pool, &id, now_contacted).await;
    Redirect::to("/dashboard/leads").into_response()
}

/// POST handler: archive a lead (drops it from the default worklist).
pub(crate) async fn lead_archive(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
    headers: HeaderMap,
    Path(id): Path<String>,
    Form(form): Form<CsrfForm>,
) -> impl IntoResponse {
    if let Err(resp) = verify_csrf_token(&headers, &form._csrf) {
        return resp;
    }
    let user = &auth_user.user;
    if user.role != "admin" && !crate::leads::user_can_access(&state.pool, &id, &user.id).await {
        return axum::http::StatusCode::FORBIDDEN.into_response();
    }
    let _ = crate::leads::archive(&state.pool, &id).await;
    Redirect::to("/dashboard/leads").into_response()
}

// --- Admin global toggle ---------------------------------------------------

#[derive(Deserialize)]
pub(crate) struct AdminLeadCaptureForm {
    _csrf: Option<String>,
    #[serde(default)]
    enabled: Option<String>,
    #[serde(default)]
    retention_days: Option<i64>,
}

pub(crate) async fn admin_update_lead_capture(
    State(state): State<Arc<AppState>>,
    _admin: crate::auth::AdminUser,
    headers: HeaderMap,
    Form(form): Form<AdminLeadCaptureForm>,
) -> impl IntoResponse {
    if let Err(resp) = verify_csrf_token(&headers, &form._csrf) {
        return resp;
    }
    let enabled = form.enabled.as_deref() == Some("on");
    let retention = form.retention_days.unwrap_or(30);
    if let Err(e) = crate::leads::config::set_global_settings(&state.pool, enabled, retention).await
    {
        tracing::warn!(error = %e, "failed to persist lead capture admin toggle");
    }
    Redirect::to("/dashboard/admin").into_response()
}

// --- Background loop: purge + abandonment alerts ---------------------------

/// Background task for lead capture: purges expired partial bookings and
/// emails hosts about abandoned ones. Spawned by `calrs serve`. The purge
/// is cheap and runs on a 6-hour cadence; the abandonment check runs every
/// 5 minutes so alerts stay timely.
///
/// A lead is "abandoned" when it has gone untouched for
/// [`ABANDON_NOTIFY_AFTER_MIN`] minutes without completing, and is no older
/// than [`ABANDON_NOTIFY_MAX_HOURS`] (so a first rollout doesn't blast the
/// host with alerts for a backlog of old rows). Each lead is emailed at most
/// once (`notified_at`).
pub async fn run_lead_purge_loop(pool: SqlitePool, secret_key: [u8; 32]) {
    use tokio::time::{sleep, Duration};

    /// How long a lead must sit untouched before we alert the host.
    const ABANDON_NOTIFY_AFTER_MIN: i64 = 30;
    /// Don't alert on leads older than this (avoids backlog spam).
    const ABANDON_NOTIFY_MAX_HOURS: i64 = 48;

    let mut last_purge = std::time::Instant::now();
    // Purge once shortly after startup, then every 6h below.
    let mut purge_due = true;

    loop {
        if purge_due {
            let retention = crate::leads::retention_days(&pool).await;
            match crate::leads::purge_expired(&pool, retention).await {
                Ok(0) => {}
                Ok(n) => tracing::info!(
                    count = n,
                    retention_days = retention,
                    "lead capture: expired rows purged"
                ),
                Err(e) => tracing::warn!(error = %e, "lead capture: purge failed"),
            }
            last_purge = std::time::Instant::now();
            purge_due = false;
        }

        notify_abandoned_leads(
            &pool,
            &secret_key,
            ABANDON_NOTIFY_AFTER_MIN,
            ABANDON_NOTIFY_MAX_HOURS,
        )
        .await;

        sleep(Duration::from_secs(5 * 60)).await;
        if last_purge.elapsed() >= Duration::from_secs(6 * 60 * 60) {
            purge_due = true;
        }
    }
}

/// Email hosts about leads that have been abandoned. Best-effort: SMTP
/// failures are logged and the lead is left un-notified for a future retry.
async fn notify_abandoned_leads(
    pool: &SqlitePool,
    secret_key: &[u8; 32],
    older_than_minutes: i64,
    max_age_hours: i64,
) {
    // Respect the global RGPD off-switch — no alerts when capture is off.
    if !crate::leads::config::global_settings(pool).await.enabled {
        return;
    }
    let due = crate::leads::due_for_notification(pool, older_than_minutes, max_age_hours).await;
    if due.is_empty() {
        return;
    }
    let smtp = match crate::email::load_smtp_config(pool, secret_key).await {
        Ok(Some(cfg)) => cfg,
        _ => return, // No SMTP configured: nothing to do.
    };
    let leads_url = std::env::var("CALRS_BASE_URL")
        .ok()
        .map(|b| format!("{}/dashboard/leads", b.trim_end_matches('/')));

    for (lead_id, host_user_id, name, email, et_id) in due {
        let host_email: Option<String> =
            sqlx::query_scalar("SELECT COALESCE(booking_email, email) FROM users WHERE id = ?")
                .bind(&host_user_id)
                .fetch_optional(pool)
                .await
                .unwrap_or(None);
        let host_email = match host_email.filter(|e| !e.is_empty()) {
            Some(e) => e,
            None => {
                // No address to reach: mark notified so we don't retry forever.
                crate::leads::mark_notified(pool, &lead_id).await;
                continue;
            }
        };
        let event_title: String = sqlx::query_scalar("SELECT title FROM event_types WHERE id = ?")
            .bind(&et_id)
            .fetch_optional(pool)
            .await
            .unwrap_or(None)
            .unwrap_or_else(|| "a booking".to_string());

        match crate::email::send_lead_abandoned_alert(
            &smtp,
            &host_email,
            &event_title,
            name.as_deref(),
            &email,
            leads_url.as_deref(),
        )
        .await
        {
            Ok(()) => {
                crate::leads::mark_notified(pool, &lead_id).await;
                tracing::info!(lead_id = %lead_id, "lead capture: abandonment alert sent");
            }
            Err(e) => tracing::warn!(error = %e, "lead capture: abandonment alert failed"),
        }
    }
}
