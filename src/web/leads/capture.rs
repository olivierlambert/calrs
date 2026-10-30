//! The public endpoint the booking pages post to as the guest types.

use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use serde::Deserialize;
use sqlx::SqlitePool;
use std::sync::Arc;

use crate::web::{client_ip_for_rate_limit, verify_csrf_token, AppState};

#[derive(Deserialize)]
pub(in crate::web) struct LeadCapturePayload {
    /// CSRF token, mirrored from the cookie by the page's script.
    #[serde(default)]
    _csrf: Option<String>,
    /// Stable id chosen by the browser (sessionStorage), opaque to us.
    lead_id: String,
    /// Slug of the event type the guest is booking.
    event_type_slug: String,
    /// Host username of a `/u/{username}/{slug}` page. Single-host slugs are
    /// only unique per account, so this is what ties the lead to the right
    /// host. Absent on the legacy `/{slug}` route.
    #[serde(default)]
    username: Option<String>,
    /// Team slug of a `/team/{team}/{slug}` page.
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

/// `lead_id` as a browser mints it: a UUID, or at least something that cannot
/// carry markup or control characters into the leads page.
fn is_valid_lead_id(id: &str) -> bool {
    (8..=64).contains(&id.len())
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// The public event type a page belongs to: `(event type id, host user id)`.
///
/// Mirrors how the booking pages themselves resolve a URL, so a lead always
/// lands on the event type the guest is looking at. Private and internal
/// event types are left out: their pages need an invite and we do not know who
/// is typing.
async fn resolve_event_type(
    pool: &SqlitePool,
    payload: &LeadCapturePayload,
) -> Option<(String, String)> {
    let slug = payload.event_type_slug.as_str();
    let team_slug = payload.team_slug.as_deref().filter(|s| !s.is_empty());
    let username = payload.username.as_deref().filter(|s| !s.is_empty());
    let found = if let Some(team_slug) = team_slug {
        sqlx::query_as(
            "SELECT et.id, COALESCE(et.created_by_user_id, '')
             FROM event_types et
             JOIN teams t ON t.id = et.team_id
             WHERE t.slug = ? AND et.slug = ? AND et.enabled = 1 AND et.visibility = 'public'",
        )
        .bind(team_slug)
        .bind(slug)
        .fetch_optional(pool)
        .await
    } else if let Some(username) = username {
        sqlx::query_as(
            "SELECT et.id, u.id
             FROM event_types et
             JOIN accounts a ON a.id = et.account_id
             JOIN users u ON u.id = a.user_id
             WHERE u.username = ? AND et.slug = ? AND et.enabled = 1 AND u.enabled = 1
               AND et.team_id IS NULL AND et.visibility = 'public'",
        )
        .bind(username)
        .bind(slug)
        .fetch_optional(pool)
        .await
    } else {
        // The legacy `/{slug}` page serves the first event type with that slug.
        sqlx::query_as(
            "SELECT et.id, COALESCE(a.user_id, '')
             FROM event_types et
             JOIN accounts a ON a.id = et.account_id
             WHERE et.slug = ? AND et.enabled = 1 AND et.team_id IS NULL
               AND et.visibility = 'public'
             ORDER BY et.rowid LIMIT 1",
        )
        .bind(slug)
        .fetch_optional(pool)
        .await
    };
    found.unwrap_or_else(|e| {
        tracing::warn!(error = %e, "lead capture: event type lookup failed");
        None
    })
}

pub(in crate::web) async fn lead_capture_record(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    axum::Json(payload): axum::Json<LeadCapturePayload>,
) -> impl IntoResponse {
    if let Err(resp) = verify_csrf_token(&headers, &payload._csrf) {
        return resp;
    }
    // Keystrokes plus the client's debounce make for about one call a second
    // from an honest guest; the cap leaves slack for that and stops a script.
    let ip = client_ip_for_rate_limit(&headers);
    if state.lead_capture_limiter.check_limited(&ip).await {
        return StatusCode::TOO_MANY_REQUESTS.into_response();
    }
    if !is_valid_lead_id(&payload.lead_id) {
        return StatusCode::BAD_REQUEST.into_response();
    }

    let Some((event_type_id, host_user_id)) = resolve_event_type(&state.pool, &payload).await
    else {
        return StatusCode::NOT_FOUND.into_response();
    };

    // 204 rather than an error when capture is off, so a stale tab quietly
    // stops recording instead of surfacing an alarming failure.
    if !crate::leads::is_capture_active(&state.pool, &event_type_id).await {
        return StatusCode::NO_CONTENT.into_response();
    }

    let user_agent = headers
        .get("User-Agent")
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);
    let input = crate::leads::PartialBookingInput {
        event_type_id,
        host_user_id: Some(host_user_id).filter(|id| !id.is_empty()),
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
    match crate::leads::upsert_partial(&state.pool, input).await {
        Ok(crate::leads::Upsert::Saved) => StatusCode::NO_CONTENT.into_response(),
        Ok(crate::leads::Upsert::Throttled) => StatusCode::TOO_MANY_REQUESTS.into_response(),
        Err(e) => {
            tracing::warn!(error = %e, "lead capture: upsert failed");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::is_valid_lead_id;

    #[test]
    fn lead_ids_are_uuid_shaped_or_refused() {
        assert!(is_valid_lead_id("3f1c2a9e-5b7d-4c1e-9a62-0d8e4b7f1a23"));
        assert!(is_valid_lead_id("abcdefgh"));
        assert!(!is_valid_lead_id("short"));
        assert!(!is_valid_lead_id(&"a".repeat(65)));
        assert!(!is_valid_lead_id("<script>alert(1)</script>"));
        assert!(!is_valid_lead_id("has space in it"));
        assert!(!is_valid_lead_id("new\nline-in-id"));
    }
}
