//! The host's leads page and its worklist actions.

use axum::extract::{Form, Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{Html, IntoResponse, Redirect};
use minijinja::context;
use std::sync::Arc;

use crate::web::{
    impersonation_ctx, internal_error_body, internal_error_html, sidebar_context,
    verify_csrf_token, AppState, CsrfForm,
};

pub(in crate::web) async fn dashboard_leads(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
) -> impl IntoResponse {
    let user = &auth_user.user;
    let global = crate::leads::config::global_settings(&state.pool).await;

    // Admins see every lead; everyone else the ones of their own event types
    // and of the teams they belong to.
    let scope = (user.role != "admin").then_some(user.id.as_str());
    let leads = match crate::leads::list_recent_for_user(&state.pool, scope, 200).await {
        Ok(leads) => leads,
        Err(e) => return internal_error_html("database query", &e),
    };
    let stats = crate::leads::stats_for_user(&state.pool, scope).await;

    let leads_ctx: Vec<minijinja::Value> = leads
        .iter()
        .map(|l| {
            context! {
                id => l.id,
                event_type_id => l.event_type_id,
                event_type_title => l.event_type_title.as_deref().unwrap_or(""),
                team_name => l.team_name.as_deref().unwrap_or(""),
                name => l.name.as_deref().unwrap_or(""),
                email => l.email.as_deref().unwrap_or(""),
                email_linkable => l.email.as_deref().is_some_and(is_plain_email),
                phone => l.phone.as_deref().unwrap_or(""),
                phone_linkable => l.phone.as_deref().is_some_and(is_dialable),
                notes => l.notes.as_deref().unwrap_or(""),
                target_date => l.target_date.as_deref().unwrap_or(""),
                target_time => l.target_time.as_deref().unwrap_or(""),
                target_tz => l.target_tz.as_deref().unwrap_or(""),
                source => source_label(l),
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
            lang => auth_user.lang,
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

/// An address safe to put in a `mailto:` link. What a guest typed may carry a
/// `?bcc=...&body=...` tail that would prefill the host's mail client.
fn is_plain_email(email: &str) -> bool {
    email.matches('@').count() == 1
        && !email
            .chars()
            .any(|c| c.is_whitespace() || c.is_control() || "?&,;<>\"".contains(c))
}

/// A phone number worth a `tel:` link: digits, `+` and the usual separators.
fn is_dialable(phone: &str) -> bool {
    phone.chars().any(|c| c.is_ascii_digit())
        && phone
            .chars()
            .all(|c| c.is_ascii_digit() || " +-().".contains(c))
}

/// Where a lead came from: the UTM source (with its campaign), else the host
/// of the referrer, else nothing.
fn source_label(l: &crate::leads::db::PartialBooking) -> String {
    if let Some(src) = l.utm_source.as_deref().filter(|s| !s.is_empty()) {
        return match l.utm_campaign.as_deref().filter(|s| !s.is_empty()) {
            Some(campaign) => format!("{src} / {campaign}"),
            None => src.to_string(),
        };
    }
    l.referrer
        .as_deref()
        .filter(|r| !r.is_empty())
        .map(|r| {
            let rest = r
                .strip_prefix("https://")
                .or_else(|| r.strip_prefix("http://"))
                .unwrap_or(r);
            rest.split('/').next().unwrap_or(rest).to_string()
        })
        .unwrap_or_default()
}

/// Whether the signed-in user may act on a lead: admins always, others when
/// the lead is theirs or belongs to one of their teams.
async fn may_act(state: &AppState, auth_user: &crate::auth::AuthUser, lead_id: &str) -> bool {
    let user = &auth_user.user;
    user.role == "admin" || crate::leads::user_can_access(&state.pool, lead_id, &user.id).await
}

/// Toggle a lead's "contacted" flag.
pub(in crate::web) async fn lead_set_contacted(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
    headers: HeaderMap,
    Path(id): Path<String>,
    Form(form): Form<CsrfForm>,
) -> impl IntoResponse {
    if let Err(resp) = verify_csrf_token(&headers, &form._csrf) {
        return resp;
    }
    if !may_act(&state, &auth_user, &id).await {
        return StatusCode::FORBIDDEN.into_response();
    }
    if let Err(e) = crate::leads::toggle_contacted(&state.pool, &id).await {
        tracing::warn!(lead = %id, error = %e, "lead capture: could not toggle contacted");
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }
    Redirect::to("/dashboard/leads").into_response()
}

/// Archive a lead, which drops it from the default list.
pub(in crate::web) async fn lead_archive(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
    headers: HeaderMap,
    Path(id): Path<String>,
    Form(form): Form<CsrfForm>,
) -> impl IntoResponse {
    if let Err(resp) = verify_csrf_token(&headers, &form._csrf) {
        return resp;
    }
    if !may_act(&state, &auth_user, &id).await {
        return StatusCode::FORBIDDEN.into_response();
    }
    if let Err(e) = crate::leads::archive(&state.pool, &id).await {
        tracing::warn!(lead = %id, error = %e, "lead capture: could not archive");
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }
    Redirect::to("/dashboard/leads").into_response()
}

#[cfg(test)]
mod tests {
    use super::{is_dialable, is_plain_email};

    #[test]
    fn a_mailto_link_never_carries_a_query() {
        assert!(is_plain_email("pat@example.com"));
        assert!(!is_plain_email("pat@example.com?bcc=boss@example.com"));
        assert!(!is_plain_email("pat@example.com&body=hi"));
        assert!(!is_plain_email("a@b.com, c@d.com"));
        assert!(!is_plain_email("no-at-sign"));
    }

    #[test]
    fn a_tel_link_needs_a_number() {
        assert!(is_dialable("+33 6 12 34 56 78"));
        assert!(is_dialable("(555) 010-1234"));
        assert!(!is_dialable("javascript:alert(1)"));
        assert!(!is_dialable("+++"));
        assert!(!is_dialable(""));
    }
}
