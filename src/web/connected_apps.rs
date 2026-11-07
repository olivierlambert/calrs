//! Connected apps: the OAuth grants (MCP clients) a user approved, listed in
//! Profile & Settings with a revoke button, plus the admin's "revoke all".
use super::*;

pub(super) fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/dashboard/settings/apps", get(apps_page))
        .route(
            "/dashboard/settings/apps/{grant_id}/revoke",
            post(revoke_app),
        )
        .route(
            "/dashboard/admin/users/{id}/revoke-apps",
            post(admin_revoke_apps),
        )
}

/// The day a grant event happened, written out in the reader's language and
/// timezone, like the rest of the dashboard.
fn grant_day(timestamp: i64, tz: chrono_tz::Tz, lang: &str) -> String {
    chrono::DateTime::from_timestamp(timestamp, 0)
        .map(|t| crate::i18n::format_long_date(t.with_timezone(&tz).date_naive(), lang))
        .unwrap_or_default()
}

async fn apps_page(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
    Query(query): Query<std::collections::HashMap<String, String>>,
) -> Response {
    let user = &auth_user.user;
    let rows: Vec<(String, String, String, i64, Option<i64>)> = match sqlx::query_as(
        "SELECT g.id, c.name, g.scope, g.created_at, g.last_used_at
         FROM oauth_grants g JOIN oauth_clients c ON c.id = g.client_id
         WHERE g.user_id = ? AND g.revoked = 0 AND g.expires_at > ?
         ORDER BY g.created_at DESC, g.id",
    )
    .bind(&user.id)
    .bind(Utc::now().timestamp())
    .fetch_all(&state.pool)
    .await
    {
        Ok(rows) => rows,
        Err(e) => return internal_error_response("load connected apps", &e),
    };
    let lang = auth_user.lang;
    let tz: chrono_tz::Tz = user.timezone.parse().unwrap_or(chrono_tz::UTC);
    let apps: Vec<minijinja::Value> = rows
        .into_iter()
        .map(|(id, name, scope, created, used)| {
            let has = |s: &str| scope.split_whitespace().any(|g| g == s);
            context! {
                id => id,
                name => name,
                events => has(crate::oauth::SCOPES[0]),
                blocks => has(crate::oauth::SCOPES[1]),
                invites => has(crate::oauth::SCOPES[2]),
                approved => grant_day(created, tz, lang),
                last_used => used.map(|t| grant_day(t, tz, lang)),
            }
        })
        .collect();
    let (impersonating, impersonating_name, _) = impersonation_ctx(&auth_user);
    let tmpl = match state.templates.get_template("dashboard_apps.html") {
        Ok(t) => t,
        Err(e) => return internal_error_response("connected apps template", &e),
    };
    match tmpl.render(context! {
        lang => lang,
        sidebar => sidebar_context(&auth_user, "settings"),
        apps => apps,
        revoked => query.contains_key("revoked"),
        impersonating => impersonating,
        impersonating_name => impersonating_name,
    }) {
        Ok(body) => Html(body).into_response(),
        Err(e) => internal_error_response("render connected apps", &e),
    }
}

async fn revoke_app(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
    headers: HeaderMap,
    Path(grant_id): Path<String>,
    Form(csrf): Form<CsrfForm>,
) -> Response {
    if let Err(resp) = verify_csrf_token(&headers, &csrf._csrf) {
        return resp;
    }
    // An impersonating admin sees the list but does not act on it, as for MFA.
    if auth_user.impersonation.is_some() {
        return axum::http::StatusCode::FORBIDDEN.into_response();
    }
    let result = async {
        let mut tx = state.pool.begin().await?;
        // Scoped to the signed-in user: another user's grant id deletes nothing.
        let removed =
            crate::oauth::delete_grants(&mut tx, &auth_user.user.id, Some(&grant_id)).await?;
        tx.commit().await?;
        Ok::<_, sqlx::Error>(removed)
    }
    .await;
    match result {
        Ok(removed) => {
            if removed > 0 {
                tracing::info!(user = %auth_user.user.email, "connected app revoked");
            }
            Redirect::to("/dashboard/settings/apps?revoked=1").into_response()
        }
        Err(e) => internal_error_response("revoke connected app", &e),
    }
}

async fn admin_revoke_apps(
    State(state): State<Arc<AppState>>,
    admin: crate::auth::AdminUser,
    headers: HeaderMap,
    Path(user_id): Path<String>,
    Form(csrf): Form<CsrfForm>,
) -> Response {
    if let Err(resp) = verify_csrf_token(&headers, &csrf._csrf) {
        return resp;
    }
    let result = async {
        let mut tx = state.pool.begin().await?;
        let removed = crate::oauth::delete_grants(&mut tx, &user_id, None).await?;
        tx.commit().await?;
        Ok::<_, sqlx::Error>(removed)
    }
    .await;
    match result {
        Ok(removed) => {
            tracing::info!(target_user = %user_id, grants = removed, admin = %admin.user.email, "admin: connected apps revoked");
            Redirect::to("/dashboard/admin").into_response()
        }
        Err(e) => internal_error_response("admin revoke connected apps", &e),
    }
}
