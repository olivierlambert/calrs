//! The admin card: global switch, retention window and the privacy-page link.

use axum::extract::{Form, State};
use axum::http::HeaderMap;
use axum::response::{IntoResponse, Redirect};
use serde::Deserialize;
use std::sync::Arc;

use crate::web::{is_safe_company_link, verify_csrf_token, AppState};

/// Back to the admin page with a translated message in the error banner.
fn refused(lang: &str, key: &str) -> axum::response::Response {
    let msg = urlencoding::encode(&crate::i18n::translate(lang, key, None)).into_owned();
    Redirect::to(&format!("/dashboard/admin?error={msg}")).into_response()
}

#[derive(Deserialize)]
pub(in crate::web) struct AdminLeadCaptureForm {
    _csrf: Option<String>,
    #[serde(default)]
    enabled: Option<String>,
    #[serde(default)]
    retention_days: Option<i64>,
}

pub(in crate::web) async fn admin_update_lead_capture(
    State(state): State<Arc<AppState>>,
    admin: crate::auth::AdminUser,
    headers: HeaderMap,
    Form(form): Form<AdminLeadCaptureForm>,
) -> impl IntoResponse {
    if let Err(resp) = verify_csrf_token(&headers, &form._csrf) {
        return resp;
    }
    let enabled = form.enabled.as_deref() == Some("on");
    let retention = form
        .retention_days
        .unwrap_or(crate::leads::config::GLOBAL_DEFAULT_RETENTION_DAYS);
    if let Err(e) = crate::leads::config::set_global_settings(&state.pool, enabled, retention).await
    {
        tracing::warn!(error = %e, "lead capture: could not save the admin settings");
        return refused(admin.lang, "admin-lead-save-failed");
    }
    tracing::info!(admin = %admin.user.email, enabled, "admin: lead capture settings saved");
    Redirect::to("/dashboard/admin").into_response()
}

#[derive(Deserialize)]
pub(in crate::web) struct LegalMentionsForm {
    legal_mentions_url: String,
    _csrf: Option<String>,
}

pub(in crate::web) async fn admin_update_legal_mentions(
    State(state): State<Arc<AppState>>,
    admin: crate::auth::AdminUser,
    headers: HeaderMap,
    Form(form): Form<LegalMentionsForm>,
) -> impl IntoResponse {
    if let Err(resp) = verify_csrf_token(&headers, &form._csrf) {
        return resp;
    }
    let link = form.legal_mentions_url.trim();
    // Rendered as a link on every public booking page, so anything that is not
    // http(s) (`javascript:`) would be stored XSS.
    if !link.is_empty() && !is_safe_company_link(link) {
        tracing::warn!(admin = %admin.user.email, "admin: legal mentions URL refused (not http/https)");
        return refused(admin.lang, "admin-legal-mentions-invalid");
    }
    let value = Some(link).filter(|l| !l.is_empty());
    let saved = sqlx::query(
        "UPDATE auth_config SET legal_mentions_url = ?, updated_at = datetime('now')
         WHERE id = 'singleton'",
    )
    .bind(value)
    .execute(&state.pool)
    .await;
    if let Err(e) = saved {
        tracing::warn!(error = %e, "lead capture: could not save the legal mentions URL");
        return refused(admin.lang, "admin-lead-save-failed");
    }
    Redirect::to("/dashboard/admin").into_response()
}
