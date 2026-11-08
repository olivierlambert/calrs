//! API tokens (#169): personal access tokens for the REST API, created and
//! revoked by their owner at `/dashboard/settings/api-tokens`, plus the
//! admin switch letting every user create them (`auth_config.
//! api_tokens_allow_all_users`, admins only by default).
use super::*;

/// Tokens one user may hold.
const MAX_TOKENS: i64 = 20;
/// How long a created token's secret waits to be shown.
const FLASH_LIFETIME: std::time::Duration = std::time::Duration::from_secs(300);
/// The lifetimes offered, in days; 0 = never expires.
const EXPIRY_DAYS: [i64; 5] = [7, 30, 90, 365, 0];

pub(super) fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route(
            "/dashboard/settings/api-tokens",
            get(tokens_page).post(create_token),
        )
        .route(
            "/dashboard/settings/api-tokens/{id}/revoke",
            post(revoke_token),
        )
        .route("/dashboard/admin/api-tokens", post(admin_update_api_policy))
}

/// Whether `user` may create personal access tokens: admins, and everyone
/// when the admin turned the switch on (`oauth::api_allowed`).
pub(super) async fn can_create_api_tokens(pool: &SqlitePool, user: &crate::models::User) -> bool {
    match pool.acquire().await {
        Ok(mut conn) => crate::oauth::api_allowed(&mut conn, user)
            .await
            .unwrap_or(false),
        Err(_) => false,
    }
}

/// The day a token event happened, in the reader's language and timezone.
fn token_day(timestamp: i64, tz: Tz, lang: &str) -> String {
    chrono::DateTime::from_timestamp(timestamp, 0)
        .map(|t| crate::i18n::format_long_date(t.with_timezone(&tz).date_naive(), lang))
        .unwrap_or_default()
}

/// What the page shows besides the list: a token just created (its secret,
/// shown this once) or an error of the form.
#[derive(Default)]
struct PageExtras {
    created: Option<String>,
    error: Option<&'static str>,
    revoked: bool,
}

async fn render_page(
    state: &AppState,
    auth_user: &crate::auth::AuthUser,
    extras: PageExtras,
) -> Response {
    let user = &auth_user.user;
    #[allow(clippy::type_complexity)]
    let rows: Vec<(
        String,
        String,
        String,
        String,
        i64,
        Option<i64>,
        Option<i64>,
    )> = match sqlx::query_as(
        "SELECT id, name, hint, scope, created_at, expires_at, last_used_at FROM api_tokens
             WHERE user_id = ? AND (expires_at IS NULL OR expires_at > ?)
             ORDER BY created_at DESC, id",
    )
    .bind(&user.id)
    .bind(Utc::now().timestamp())
    .fetch_all(&state.pool)
    .await
    {
        Ok(rows) => rows,
        Err(e) => return internal_error_response("load api tokens", &e),
    };
    let lang = auth_user.lang;
    let tz: Tz = user.timezone.parse().unwrap_or(Tz::UTC);
    let tokens: Vec<minijinja::Value> = rows
        .into_iter()
        .map(|(id, name, hint, scope, created, expires, used)| {
            let has = |s: &str| scope.split_whitespace().any(|g| g == s);
            context! {
                id => id,
                name => name,
                hint => format!("{}{hint}", api::PAT_PREFIX),
                events => has(crate::oauth::API_SCOPES[0]),
                slots => has(crate::oauth::API_SCOPES[1]),
                bookings_read => has(crate::oauth::API_SCOPES[2]),
                bookings_write => has(crate::oauth::API_SCOPES[3]),
                created => token_day(created, tz, lang),
                expires => expires.map(|t| token_day(t, tz, lang)),
                last_used => used.map(|t| token_day(t, tz, lang)),
            }
        })
        .collect();
    let allowed = can_create_api_tokens(&state.pool, user).await;
    let (impersonating, impersonating_name, _) = impersonation_ctx(auth_user);
    let tmpl = match state.templates.get_template("dashboard_api_tokens.html") {
        Ok(t) => t,
        Err(e) => return internal_error_response("api tokens template", &e),
    };
    match tmpl.render(context! {
        lang => lang,
        sidebar => sidebar_context(auth_user, "settings"),
        tokens => tokens,
        allowed => allowed,
        created => extras.created,
        error => extras.error.map(|key| crate::i18n::translate(lang, key, None)),
        revoked => extras.revoked,
        impersonating => impersonating,
        impersonating_name => impersonating_name,
    }) {
        Ok(body) => (
            // The page can carry a secret: keep it out of every cache.
            [
                ("Cache-Control", "no-store"),
                ("Referrer-Policy", "no-referrer"),
            ],
            Html(body),
        )
            .into_response(),
        Err(e) => internal_error_response("render api tokens", &e),
    }
}

async fn tokens_page(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
    Query(query): Query<HashMap<String, String>>,
) -> Response {
    // The secret of a token just created, shown this once: the flash is
    // taken, so a reload (or the back button) shows it no more.
    let created = query.get("created").and_then(|id| {
        let mut flash = state.api_token_flash.lock().ok()?;
        // Another user opening the link takes nothing, so cannot burn it.
        if flash.get(id)?.0 != auth_user.user.id {
            return None;
        }
        let (_, secret, at) = flash.remove(id)?;
        (at.elapsed() < FLASH_LIFETIME).then_some(secret)
    });
    let extras = PageExtras {
        created,
        revoked: query.contains_key("revoked"),
        ..Default::default()
    };
    render_page(&state, &auth_user, extras).await
}

#[derive(Deserialize)]
pub(super) struct CreateTokenForm {
    _csrf: Option<String>,
    name: Option<String>,
    scope_event_types: Option<String>,
    scope_slots: Option<String>,
    scope_bookings_read: Option<String>,
    scope_bookings_write: Option<String>,
    expires_days: Option<i64>,
}

async fn create_token(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
    headers: HeaderMap,
    jar: axum_extra::extract::cookie::CookieJar,
    Form(form): Form<CreateTokenForm>,
) -> Response {
    if let Err(resp) = verify_csrf_token(&headers, &form._csrf) {
        return resp;
    }
    // The token acts as the signed-in user: never as someone an admin is
    // impersonating.
    if auth_user.impersonation.is_some() {
        return axum::http::StatusCode::FORBIDDEN.into_response();
    }
    let user = &auth_user.user;
    if !can_create_api_tokens(&state.pool, user).await {
        return axum::http::StatusCode::FORBIDDEN.into_response();
    }
    let error = |key| PageExtras {
        error: Some(key),
        ..Default::default()
    };
    let name = form.name.as_deref().unwrap_or("").trim();
    if name.is_empty()
        || name.chars().count() > 100
        || name.chars().any(crate::oauth::spoofing_char)
    {
        return render_page(&state, &auth_user, error("api-tokens-error-name")).await;
    }
    let scopes: Vec<&str> = [
        (
            form.scope_event_types.is_some(),
            crate::oauth::API_SCOPES[0],
        ),
        (form.scope_slots.is_some(), crate::oauth::API_SCOPES[1]),
        (
            form.scope_bookings_read.is_some(),
            crate::oauth::API_SCOPES[2],
        ),
        (
            form.scope_bookings_write.is_some(),
            crate::oauth::API_SCOPES[3],
        ),
    ]
    .into_iter()
    .filter_map(|(on, scope)| on.then_some(scope))
    .collect();
    if scopes.is_empty() {
        return render_page(&state, &auth_user, error("api-tokens-error-scope")).await;
    }
    let days = form.expires_days.unwrap_or(30);
    if !EXPIRY_DAYS.contains(&days) {
        return axum::http::StatusCode::BAD_REQUEST.into_response();
    }
    let now = Utc::now().timestamp();
    let secret = crate::oauth::random_token();
    let token = format!("{}{secret}", api::PAT_PREFIX);
    let session = jar
        .get(crate::auth::SESSION_COOKIE)
        .map(|c| c.value().to_string())
        .unwrap_or_default();
    let result = async {
        let mut tx = state.pool.begin().await?;
        // Write first, so two concurrent creations cannot both pass the cap.
        sqlx::query("UPDATE api_tokens SET name = name WHERE user_id = ?")
            .bind(&user.id)
            .execute(&mut *tx)
            .await?;
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM api_tokens WHERE user_id = ? AND (expires_at IS NULL OR expires_at > ?)",
        )
        .bind(&user.id)
        .bind(now)
        .fetch_one(&mut *tx)
        .await?;
        if count >= MAX_TOKENS {
            return Ok(false);
        }
        // The token is worth what the session creating it is worth: it
        // carries the session's MFA state, as an OAuth grant does.
        sqlx::query(
            "INSERT INTO api_tokens (id, user_id, name, token_hash, hint, scope, mfa_verified, created_at, expires_at)
             VALUES (?, ?, ?, ?, ?, ?, COALESCE((SELECT mfa_verified FROM sessions WHERE id = ? AND user_id = ?), 0), ?, ?)",
        )
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(&user.id)
        .bind(name)
        .bind(crate::oauth::hash(&token))
        .bind(&secret[..4])
        .bind(scopes.join(" "))
        .bind(&session)
        .bind(&user.id)
        .bind(now)
        .bind((days > 0).then_some(now + days * 86400))
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok::<_, sqlx::Error>(true)
    }
    .await;
    match result {
        Ok(true) => {
            tracing::info!(user = %user.email, scopes = %scopes.join(" "), days, "API token created");
            // Post/Redirect/Get: a reload of the answer creates nothing. The
            // secret waits server-side for the page the redirect opens.
            let flash_id = crate::oauth::random_token();
            if let Ok(mut flash) = state.api_token_flash.lock() {
                flash.retain(|_, (_, _, at)| at.elapsed() < FLASH_LIFETIME);
                flash.insert(
                    flash_id.clone(),
                    (user.id.clone(), token, std::time::Instant::now()),
                );
            }
            (
                [("Cache-Control", "no-store")],
                Redirect::to(&format!(
                    "/dashboard/settings/api-tokens?created={flash_id}"
                )),
            )
                .into_response()
        }
        Ok(false) => render_page(&state, &auth_user, error("api-tokens-error-limit")).await,
        Err(e) => internal_error_response("create api token", &e),
    }
}

async fn revoke_token(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
    headers: HeaderMap,
    Path(token_id): Path<String>,
    Form(csrf): Form<CsrfForm>,
) -> Response {
    if let Err(resp) = verify_csrf_token(&headers, &csrf._csrf) {
        return resp;
    }
    if auth_user.impersonation.is_some() {
        return axum::http::StatusCode::FORBIDDEN.into_response();
    }
    let result = async {
        let mut tx = state.pool.begin().await?;
        // Scoped to the signed-in user: another user's token id deletes nothing.
        let removed =
            crate::oauth::delete_api_tokens(&mut tx, &auth_user.user.id, Some(&token_id)).await?;
        tx.commit().await?;
        Ok::<_, sqlx::Error>(removed)
    }
    .await;
    match result {
        Ok(removed) => {
            if removed > 0 {
                tracing::info!(user = %auth_user.user.email, "API token revoked");
            }
            Redirect::to("/dashboard/settings/api-tokens?revoked=1").into_response()
        }
        Err(e) => internal_error_response("revoke api token", &e),
    }
}

#[derive(Deserialize)]
pub(super) struct ApiPolicyForm {
    _csrf: Option<String>,
    /// HTML checkbox: present when every user may create API tokens.
    allow_all_users: Option<String>,
}

async fn admin_update_api_policy(
    State(state): State<Arc<AppState>>,
    admin: crate::auth::AdminUser,
    headers: HeaderMap,
    Form(form): Form<ApiPolicyForm>,
) -> Response {
    if let Err(resp) = verify_csrf_token(&headers, &form._csrf) {
        return resp;
    }
    let allow_all = form.allow_all_users.is_some();
    if let Err(e) =
        sqlx::query("UPDATE auth_config SET api_tokens_allow_all_users = ? WHERE id = 'singleton'")
            .bind(allow_all)
            .execute(&state.pool)
            .await
    {
        return internal_error_response("save api token policy", &e);
    }
    tracing::info!(admin = %admin.user.email, allow_all, "admin: API token policy updated");
    Redirect::to("/dashboard/admin").into_response()
}
