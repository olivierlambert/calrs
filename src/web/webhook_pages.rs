//! The webhooks page (`/dashboard/webhooks`, #168): add, edit, test, switch
//! off and remove the endpoints a user may manage. Delivery itself lives in
//! `webhooks.rs`.
//!
//! Who manages what:
//! - an endpoint for "all my event types" belongs to the user who added it,
//!   and only they see it;
//! - an endpoint for one event type is managed by whoever may manage that
//!   event type (`can_manage_event_type`: its owner, the team's admins, the
//!   instance admins), whoever added it.
//!
//! Non-admins only get here when the admin allowed it
//! (`auth_config.webhooks_allow_all_users`).

use super::webhooks::{
    check_target, Attempt, MAX_ENDPOINTS_PER_SCOPE, SECRET_MAX_LEN, SECRET_MIN_LEN,
};
use super::*;

/// Scope value of the "all my event types" option.
const SCOPE_PERSONAL: &str = "personal";

/// Messages a redirect may name, so the page never renders an arbitrary key.
const NOTICES: &[&str] = &[
    "webhooks-notice-created",
    "webhooks-notice-updated",
    "webhooks-notice-deleted",
    "webhooks-notice-enabled",
    "webhooks-notice-disabled",
    "webhooks-notice-test-ok",
];
const ERRORS: &[&str] = &[
    "webhooks-error-url",
    "webhooks-error-https",
    "webhooks-error-credentials",
    "webhooks-error-private",
    "webhooks-error-resolve",
    "webhooks-error-secret",
    "webhooks-error-scope",
    "webhooks-error-limit",
    "webhooks-error-test",
];

/// Whether this user may add and manage webhooks at all: admins always,
/// everyone else when the admin allowed it.
pub(super) async fn can_use_webhooks(pool: &SqlitePool, user: &crate::models::User) -> bool {
    if user.role == "admin" {
        return true;
    }
    sqlx::query_scalar::<_, bool>(
        "SELECT webhooks_allow_all_users FROM auth_config WHERE id = 'singleton'",
    )
    .fetch_optional(pool)
    .await
    .unwrap_or(None)
    .unwrap_or(false)
}

/// Event types a user may attach an endpoint to: their personal ones and
/// those of the teams they administer. (id, title, team name)
async fn attachable_event_types(
    pool: &SqlitePool,
    user_id: &str,
) -> Vec<(String, String, Option<String>)> {
    sqlx::query_as(
        "SELECT et.id, et.title, NULL FROM event_types et
         JOIN accounts a ON a.id = et.account_id
         WHERE a.user_id = ?1 AND et.team_id IS NULL
         UNION ALL
         SELECT et.id, et.title, t.name FROM event_types et
         JOIN teams t ON t.id = et.team_id
         JOIN team_members tm ON tm.team_id = et.team_id
         WHERE tm.user_id = ?1 AND tm.role = 'admin'
         ORDER BY 3, 2",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
    .unwrap_or_default()
}

/// Whether `user` may see and change this endpoint.
async fn may_manage_webhook(
    pool: &SqlitePool,
    user: &crate::models::User,
    webhook_id: &str,
) -> bool {
    let row: Option<(String, Option<String>)> =
        sqlx::query_as("SELECT user_id, event_type_id FROM webhooks WHERE id = ?")
            .bind(webhook_id)
            .fetch_optional(pool)
            .await
            .unwrap_or(None);
    match row {
        Some((owner, None)) => owner == user.id,
        Some((_, Some(et_id))) => can_manage_event_type(pool, user, &et_id).await,
        None => false,
    }
}

/// The checks every POST shares: CSRF, the admin switch, and for an existing
/// endpoint the right to manage it.
async fn guard(
    state: &AppState,
    headers: &HeaderMap,
    csrf: &Option<String>,
    user: &crate::models::User,
    webhook_id: Option<&str>,
) -> Result<(), Response> {
    verify_csrf_token(headers, csrf)?;
    if !can_use_webhooks(&state.pool, user).await {
        return Err(Redirect::to("/dashboard/webhooks").into_response());
    }
    if let Some(id) = webhook_id {
        if !may_manage_webhook(&state.pool, user, id).await {
            return Err(Redirect::to("/dashboard/webhooks").into_response());
        }
    }
    Ok(())
}

fn back(param: &str, key: &str) -> Response {
    Redirect::to(&format!("/dashboard/webhooks?{param}={key}")).into_response()
}

fn secret_ok(secret: &str) -> bool {
    let n = secret.chars().count();
    (SECRET_MIN_LEN..=SECRET_MAX_LEN).contains(&n) && !secret.chars().any(char::is_control)
}

#[derive(Deserialize)]
pub(super) struct WebhooksQuery {
    notice: Option<String>,
    error: Option<String>,
}

#[allow(clippy::type_complexity)]
pub(super) async fn webhooks_page(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
    Query(query): Query<WebhooksQuery>,
) -> impl IntoResponse {
    let user = &auth_user.user;
    let allowed = can_use_webhooks(&state.pool, user).await;

    let mut endpoints_ctx = Vec::new();
    let mut scopes_ctx = Vec::new();
    if allowed {
        let rows: Vec<(
            String,
            String,
            Option<String>,
            Option<String>,
            Option<String>,
            bool,
            Option<String>,
            Option<i64>,
            Option<String>,
            String,
            i64,
            i64,
        )> = sqlx::query_as(
            "SELECT w.id, w.url, w.event_type_id, et.title, t.name, w.enabled,
                    w.last_attempt_at, w.last_status, w.last_error, cu.name,
                    (SELECT COUNT(*) FROM webhook_deliveries d WHERE d.webhook_id = w.id AND d.state = 'pending'),
                    (SELECT COUNT(*) FROM webhook_deliveries d WHERE d.webhook_id = w.id AND d.state = 'failed')
             FROM webhooks w
             JOIN users cu ON cu.id = w.user_id
             LEFT JOIN event_types et ON et.id = w.event_type_id
             LEFT JOIN teams t ON t.id = et.team_id
             WHERE (w.event_type_id IS NULL AND w.user_id = ?1)
                OR w.event_type_id IN (
                    SELECT et2.id FROM event_types et2 JOIN accounts a ON a.id = et2.account_id
                    WHERE a.user_id = ?1 AND et2.team_id IS NULL
                    UNION
                    SELECT et3.id FROM event_types et3 JOIN team_members tm ON tm.team_id = et3.team_id
                    WHERE tm.user_id = ?1 AND tm.role = 'admin')
             ORDER BY w.created_at, w.id",
        )
        .bind(&user.id)
        .fetch_all(&state.pool)
        .await
        .unwrap_or_default();
        for (
            id,
            url,
            et_id,
            et_title,
            team_name,
            enabled,
            last_attempt_at,
            last_status,
            last_error,
            added_by,
            queued,
            failed,
        ) in rows
        {
            endpoints_ctx.push(context! {
                id => id,
                url => url,
                personal => et_id.is_none(),
                event_type_title => et_title,
                team_name => team_name,
                enabled => enabled,
                last_attempt_at => last_attempt_at
                    .as_deref()
                    .map(|s| s.trim_end_matches('Z').replace('T', " ")),
                last_attempt_iso => last_attempt_at,
                last_status => last_status,
                last_error => last_error,
                added_by => added_by,
                queued => queued,
                failed => failed,
            });
        }
        for (id, title, team) in attachable_event_types(&state.pool, &user.id).await {
            scopes_ctx.push(context! { id => id, title => title, team => team });
        }
    }

    let notice = query
        .notice
        .filter(|k| NOTICES.contains(&k.as_str()))
        .map(|k| crate::i18n::translate(auth_user.lang, &k, None));
    let error = query
        .error
        .filter(|k| ERRORS.contains(&k.as_str()))
        .map(|k| crate::i18n::translate(auth_user.lang, &k, None));

    let tmpl = match state.templates.get_template("webhooks.html") {
        Ok(t) => t,
        Err(e) => return internal_error_response("template render", &e),
    };
    let (impersonating, impersonating_name, _) = impersonation_ctx(&auth_user);
    Html(
        tmpl.render(context! {
            sidebar => sidebar_context(&auth_user, "event-types"),
            lang => auth_user.lang,
            allowed => allowed,
            endpoints => endpoints_ctx,
            scopes => scopes_ctx,
            notice => notice,
            error => error,
            secret_min => SECRET_MIN_LEN,
            impersonating => impersonating,
            impersonating_name => impersonating_name,
        })
        .unwrap_or_default(),
    )
    .into_response()
}

#[derive(Deserialize)]
pub(super) struct CreateWebhookForm {
    _csrf: Option<String>,
    scope: String,
    url: String,
    secret: String,
}

pub(super) async fn create_webhook(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
    headers: HeaderMap,
    Form(form): Form<CreateWebhookForm>,
) -> impl IntoResponse {
    let user = &auth_user.user;
    if let Err(resp) = guard(&state, &headers, &form._csrf, user, None).await {
        return resp;
    }
    let event_type_id = if form.scope == SCOPE_PERSONAL {
        None
    } else if can_manage_event_type(&state.pool, user, &form.scope).await
        // Instance admins pass the check for any id, so make sure it is one.
        && sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM event_types WHERE id = ?)")
            .bind(&form.scope)
            .fetch_one(&state.pool)
            .await
            .unwrap_or(false)
    {
        Some(form.scope.clone())
    } else {
        return back("error", "webhooks-error-scope");
    };
    if !secret_ok(&form.secret) {
        return back("error", "webhooks-error-secret");
    }
    let allowlist = crate::settings::private_host_allowlist();
    if let Err(e) = check_target(&form.url, &allowlist).await {
        return back("error", e.key());
    }
    let existing: i64 = match &event_type_id {
        Some(et) => sqlx::query_scalar("SELECT COUNT(*) FROM webhooks WHERE event_type_id = ?")
            .bind(et)
            .fetch_one(&state.pool)
            .await
            .unwrap_or(0),
        None => sqlx::query_scalar(
            "SELECT COUNT(*) FROM webhooks WHERE event_type_id IS NULL AND user_id = ?",
        )
        .bind(&user.id)
        .fetch_one(&state.pool)
        .await
        .unwrap_or(0),
    };
    if existing >= MAX_ENDPOINTS_PER_SCOPE {
        return back("error", "webhooks-error-limit");
    }
    let secret_enc = match crate::crypto::encrypt_value(&state.secret_key, &form.secret) {
        Ok(v) => v,
        Err(e) => return internal_error_response("encrypt webhook secret", &e),
    };
    let id = uuid::Uuid::new_v4().to_string();
    if let Err(e) = sqlx::query(
        "INSERT INTO webhooks (id, user_id, event_type_id, url, secret_enc) VALUES (?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(&user.id)
    .bind(&event_type_id)
    .bind(form.url.trim())
    .bind(&secret_enc)
    .execute(&state.pool)
    .await
    {
        return internal_error_response("save webhook", &e);
    }
    tracing::info!(webhook = %id, user = %user.email, event_type = ?event_type_id, "webhook added");
    back("notice", "webhooks-notice-created")
}

#[derive(Deserialize)]
pub(super) struct UpdateWebhookForm {
    _csrf: Option<String>,
    url: String,
    #[serde(default)]
    secret: String,
}

/// Change the URL, and the secret when a new one is typed (an empty field
/// keeps the stored one).
pub(super) async fn update_webhook(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
    headers: HeaderMap,
    Path(id): Path<String>,
    Form(form): Form<UpdateWebhookForm>,
) -> impl IntoResponse {
    let user = &auth_user.user;
    if let Err(resp) = guard(&state, &headers, &form._csrf, user, Some(&id)).await {
        return resp;
    }
    let allowlist = crate::settings::private_host_allowlist();
    if let Err(e) = check_target(&form.url, &allowlist).await {
        return back("error", e.key());
    }
    let secret_enc = if form.secret.is_empty() {
        None
    } else if secret_ok(&form.secret) {
        match crate::crypto::encrypt_value(&state.secret_key, &form.secret) {
            Ok(v) => Some(v),
            Err(e) => return internal_error_response("encrypt webhook secret", &e),
        }
    } else {
        return back("error", "webhooks-error-secret");
    };
    if let Err(e) = sqlx::query(
        "UPDATE webhooks SET url = ?, secret_enc = COALESCE(?, secret_enc) WHERE id = ?",
    )
    .bind(form.url.trim())
    .bind(&secret_enc)
    .bind(&id)
    .execute(&state.pool)
    .await
    {
        return internal_error_response("update webhook", &e);
    }
    tracing::info!(webhook = %id, user = %user.email, secret_changed = secret_enc.is_some(), "webhook updated");
    back("notice", "webhooks-notice-updated")
}

#[derive(Deserialize)]
pub(super) struct WebhookActionForm {
    _csrf: Option<String>,
}

/// Switch an endpoint on or off. Switching it off drops what is still
/// queued for it: switched back on, it starts from new events rather than
/// replaying stale ones.
pub(super) async fn toggle_webhook(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
    headers: HeaderMap,
    Path(id): Path<String>,
    Form(form): Form<WebhookActionForm>,
) -> impl IntoResponse {
    let user = &auth_user.user;
    if let Err(resp) = guard(&state, &headers, &form._csrf, user, Some(&id)).await {
        return resp;
    }
    let enabled: bool = sqlx::query_scalar(
        "UPDATE webhooks SET enabled = 1 - enabled WHERE id = ? RETURNING enabled",
    )
    .bind(&id)
    .fetch_one(&state.pool)
    .await
    .unwrap_or(false);
    if !enabled {
        let _ = sqlx::query("DELETE FROM webhook_deliveries WHERE webhook_id = ?")
            .bind(&id)
            .execute(&state.pool)
            .await;
    }
    tracing::info!(webhook = %id, user = %user.email, enabled, "webhook toggled");
    back(
        "notice",
        if enabled {
            "webhooks-notice-enabled"
        } else {
            "webhooks-notice-disabled"
        },
    )
}

pub(super) async fn delete_webhook(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
    headers: HeaderMap,
    Path(id): Path<String>,
    Form(form): Form<WebhookActionForm>,
) -> impl IntoResponse {
    let user = &auth_user.user;
    if let Err(resp) = guard(&state, &headers, &form._csrf, user, Some(&id)).await {
        return resp;
    }
    if let Err(e) = sqlx::query("DELETE FROM webhooks WHERE id = ?")
        .bind(&id)
        .execute(&state.pool)
        .await
    {
        return internal_error_response("delete webhook", &e);
    }
    tracing::info!(webhook = %id, user = %user.email, "webhook removed");
    back("notice", "webhooks-notice-deleted")
}

/// "Send test": one signed `ping` now. The outcome is shown in the
/// endpoint's row, which the redirect lands on.
pub(super) async fn test_webhook(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
    headers: HeaderMap,
    Path(id): Path<String>,
    Form(form): Form<WebhookActionForm>,
) -> impl IntoResponse {
    let user = &auth_user.user;
    if let Err(resp) = guard(&state, &headers, &form._csrf, user, Some(&id)).await {
        return resp;
    }
    let allowlist = crate::settings::private_host_allowlist();
    let attempt =
        super::webhooks::send_test_with(&state.pool, &state.secret_key, &id, &allowlist).await;
    match attempt {
        Some(Attempt { error: None, .. }) => back("notice", "webhooks-notice-test-ok"),
        _ => back("error", "webhooks-error-test"),
    }
}

/// Admin panel: whether non-admins may add webhooks.
#[derive(Deserialize)]
pub(super) struct AdminWebhookPolicyForm {
    _csrf: Option<String>,
    allow_all_users: Option<String>,
}

pub(super) async fn admin_update_webhook_policy(
    State(state): State<Arc<AppState>>,
    admin: crate::auth::AdminUser,
    headers: HeaderMap,
    Form(form): Form<AdminWebhookPolicyForm>,
) -> impl IntoResponse {
    if let Err(resp) = verify_csrf_token(&headers, &form._csrf) {
        return resp;
    }
    let allow_all = form.allow_all_users.is_some();
    if let Err(e) =
        sqlx::query("UPDATE auth_config SET webhooks_allow_all_users = ? WHERE id = 'singleton'")
            .bind(allow_all)
            .execute(&state.pool)
            .await
    {
        return internal_error_response("save webhook policy", &e);
    }
    tracing::info!(admin = %admin.user.email, allow_all, "admin: webhook policy updated");
    Redirect::to("/dashboard/admin#panel-notifications").into_response()
}
