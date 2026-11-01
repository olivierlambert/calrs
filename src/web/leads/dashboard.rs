//! The host's leads page and its worklist actions.

use axum::extract::{Form, Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{Html, IntoResponse, Redirect};
use minijinja::context;
use serde::Deserialize;
use std::sync::Arc;

use crate::leads::LeadView;

use crate::web::{
    impersonation_ctx, internal_error_body, internal_error_html, sidebar_context,
    verify_csrf_token, AppState,
};

#[derive(Deserialize, Default)]
pub(in crate::web) struct LeadsQuery {
    #[serde(default)]
    view: Option<String>,
    /// Text to look for in the name, email or phone.
    #[serde(default)]
    q: Option<String>,
}

/// A worklist button: the CSRF token, plus the view and search it was pressed
/// in, so the host lands back where they were.
#[derive(Deserialize)]
pub(in crate::web) struct LeadActionForm {
    _csrf: Option<String>,
    #[serde(default)]
    view: Option<String>,
    #[serde(default)]
    q: Option<String>,
}

impl LeadActionForm {
    fn back(&self) -> Redirect {
        let view = LeadView::parse(self.view.as_deref());
        let mut url = format!("/dashboard/leads?view={}", view.as_str());
        if let Some(q) = self.q.as_deref().map(str::trim).filter(|q| !q.is_empty()) {
            url.push_str("&q=");
            url.push_str(&urlencoding::encode(q));
        }
        Redirect::to(&url)
    }
}

pub(in crate::web) async fn dashboard_leads(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
    Query(query): Query<LeadsQuery>,
) -> impl IntoResponse {
    let user = &auth_user.user;
    let view = LeadView::parse(query.view.as_deref());
    let search = query.q.as_deref().unwrap_or("").trim().to_string();
    let global = crate::leads::config::global_settings(&state.pool).await;

    // Admins see every lead; everyone else the ones of their own event types
    // and of the teams they belong to.
    let scope = (user.role != "admin").then_some(user.id.as_str());
    let leads = match crate::leads::list_leads(&state.pool, scope, view, &search, 200).await {
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
                email_linkable => l.email.as_deref().is_some_and(crate::leads::is_plain_email),
                phone => l.phone.as_deref().unwrap_or(""),
                phone_linkable => l.phone.as_deref().is_some_and(is_dialable),
                notes => l.notes.as_deref().unwrap_or(""),
                target_date => l.target_date.as_deref().unwrap_or(""),
                target_time => l.target_time.as_deref().unwrap_or(""),
                target_tz => l.target_tz.as_deref().unwrap_or(""),
                source => source_label(l),
                contacted => l.contacted_at.is_some(),
                completed => l.completed_at.is_some(),
                archived => l.archived_at.is_some(),
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
            view => view.as_str(),
            search_query => urlencoding::encode(&search).into_owned(),
            search => search,
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

/// Run one worklist action on a lead the user may act on, then go back to the
/// view the button was pressed in.
async fn act(
    state: &AppState,
    auth_user: &crate::auth::AuthUser,
    headers: &HeaderMap,
    id: &str,
    form: &LeadActionForm,
    action: impl std::future::Future<Output = anyhow::Result<()>>,
) -> axum::response::Response {
    if let Err(resp) = verify_csrf_token(headers, &form._csrf) {
        return resp;
    }
    if !may_act(state, auth_user, id).await {
        return StatusCode::FORBIDDEN.into_response();
    }
    if let Err(e) = action.await {
        tracing::warn!(lead = %id, error = %e, "lead capture: worklist action failed");
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }
    form.back().into_response()
}

/// Toggle a lead's "contacted" flag.
pub(in crate::web) async fn lead_set_contacted(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
    headers: HeaderMap,
    Path(id): Path<String>,
    Form(form): Form<LeadActionForm>,
) -> impl IntoResponse {
    let work = crate::leads::toggle_contacted(&state.pool, &id);
    act(&state, &auth_user, &headers, &id, &form, work).await
}

/// Archive a lead, which drops it from the worklist.
pub(in crate::web) async fn lead_archive(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
    headers: HeaderMap,
    Path(id): Path<String>,
    Form(form): Form<LeadActionForm>,
) -> impl IntoResponse {
    let work = crate::leads::archive(&state.pool, &id);
    act(&state, &auth_user, &headers, &id, &form, work).await
}

/// Put an archived lead back on the worklist.
pub(in crate::web) async fn lead_unarchive(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
    headers: HeaderMap,
    Path(id): Path<String>,
    Form(form): Form<LeadActionForm>,
) -> impl IntoResponse {
    let work = crate::leads::unarchive(&state.pool, &id);
    act(&state, &auth_user, &headers, &id, &form, work).await
}

/// Erase every lead of the view and search the button was pressed in.
pub(in crate::web) async fn lead_delete_all(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
    headers: HeaderMap,
    Form(form): Form<LeadActionForm>,
) -> impl IntoResponse {
    if let Err(resp) = verify_csrf_token(&headers, &form._csrf) {
        return resp;
    }
    let user = &auth_user.user;
    let scope = (user.role != "admin").then_some(user.id.as_str());
    let view = LeadView::parse(form.view.as_deref());
    let search = form.q.as_deref().unwrap_or("").trim();
    match crate::leads::delete_leads(&state.pool, scope, view, search).await {
        Ok(count) => {
            tracing::info!(user = %user.email, count, "lead capture: leads deleted in bulk");
            form.back().into_response()
        }
        Err(e) => {
            tracing::warn!(error = %e, "lead capture: bulk delete failed");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

/// Erase a lead for good, whatever state it is in. This is what answers a
/// guest who asks for their data to be deleted.
pub(in crate::web) async fn lead_delete(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
    headers: HeaderMap,
    Path(id): Path<String>,
    Form(form): Form<LeadActionForm>,
) -> impl IntoResponse {
    let work = async {
        crate::leads::delete_lead(&state.pool, &id)
            .await
            .map(|_| ())
    };
    let response = act(&state, &auth_user, &headers, &id, &form, work).await;
    if response.status().is_redirection() {
        tracing::info!(user = %auth_user.user.email, lead = %id, "lead capture: lead deleted");
    }
    response
}

#[cfg(test)]
mod tests {
    use super::is_dialable;

    #[test]
    fn a_tel_link_needs_a_number() {
        assert!(is_dialable("+33 6 12 34 56 78"));
        assert!(is_dialable("(555) 010-1234"));
        assert!(!is_dialable("javascript:alert(1)"));
        assert!(!is_dialable("+++"));
        assert!(!is_dialable(""));
    }
}
