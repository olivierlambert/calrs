//! Invite links for internal and private event types: Invite Links page, bulk invites, quick links.

use super::*;

pub(super) async fn dashboard_organization(
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
            lang => auth_user.lang,
            event_types => ets_ctx,
            impersonating => impersonating,
            impersonating_name => impersonating_name,
        })
        .unwrap_or_default(),
    )
}

// --- Invite management handlers ---

const MAX_BULK_INVITES: usize = 100;

#[derive(Deserialize)]
pub(super) struct BulkInviteForm {
    _csrf: Option<String>,
    recipients: String,
    message: Option<String>,
    expires_days: Option<i32>,
    single_use: Option<String>, // checkbox: "on" or absent
}

#[derive(Default)]
pub(super) struct BulkInviteResult {
    pub(super) sent: Vec<String>,
    pub(super) invalid: Vec<String>,
    pub(super) duplicates: Vec<String>,
    failed: Vec<String>,
    pub(super) over_limit: bool,
}

pub(super) fn parse_bulk_recipients(
    input: &str,
    max: usize,
) -> (Vec<(String, String)>, BulkInviteResult) {
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

pub(super) fn derive_name_from_email(email: &str) -> String {
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
        None => {
            return Html(crate::i18n::translate(
                auth_user.lang,
                "error-private-event-type-not-found",
                None,
            ))
            .into_response()
        }
    };

    // Internal event types: any authenticated user can issue invites — that's
    // the documented "any authenticated colleague can distribute" contract.
    // Private event types: invite issuance is management (admin / owner /
    // team admin only).
    if visibility == "private" && !can_manage_event_type(&state.pool, &auth_user.user, &et_id).await
    {
        return Html(crate::i18n::translate(
            auth_user.lang,
            "error-access-denied",
            None,
        ))
        .into_response();
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

    let base_url = crate::settings::base_url().unwrap_or_default();
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
            lang => auth_user.lang,
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

pub(super) async fn invite_management_page(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
    Path(event_type_id): Path<String>,
) -> impl IntoResponse {
    render_invite_management(&state, &auth_user, &event_type_id, None).await
}

pub(super) async fn send_invite_bulk(
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

    let base_url = crate::settings::base_url().unwrap_or_default();
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
                    auth_user.lang,
                    &name,
                    &email,
                    &et_title,
                    &auth_user.user.name,
                    // Account email, not booking_email: invites go to
                    // recipients the sender picks, and booking_email is an
                    // unverified free-form address, so replies must not be
                    // steerable off the identity OIDC/registration vouched for.
                    &auth_user.user.email,
                    message_opt,
                    &invite_url,
                    expires_at.as_deref(),
                )
                .await;
                if let Err(e) = send_res {
                    tracing::warn!(email = %email, error = format!("{e:#}"), "bulk invite SMTP send failed");
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
pub(super) struct DeleteInviteForm {
    _csrf: Option<String>,
}

pub(super) async fn delete_invite(
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

pub(super) async fn generate_quick_link(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
    headers: HeaderMap,
    Path(event_type_id): Path<String>,
    Form(form): Form<CsrfForm>,
) -> impl IntoResponse {
    if let Err(resp) = verify_csrf_token(&headers, &form._csrf) {
        return resp;
    }

    match create_quick_invite(&state.pool, &auth_user.user, &event_type_id, false, 7).await {
        Ok(Some(invite)) => axum::Json(serde_json::json!({ "url": invite.url })).into_response(),
        Ok(None) => Redirect::to("/dashboard/invite-links").into_response(),
        Err(e) => internal_error_response("create quick invite", &e),
    }
}

#[derive(serde::Serialize)]
pub(super) struct QuickInvite {
    pub url: String,
    pub expires_at: Option<String>,
    pub max_uses: Option<i32>,
}

/// Shared by the dashboard and MCP. Internal events are distributable by any
/// user; other event types require management permission. No email is sent.
pub(super) async fn create_quick_invite(
    pool: &SqlitePool,
    user: &crate::models::User,
    event_type_id: &str,
    include_public: bool,
    expires_days: i32,
) -> Result<Option<QuickInvite>, sqlx::Error> {
    let et: Option<(
        String,
        String,
        Option<String>,
        Option<String>,
        String,
        Option<String>,
    )> = sqlx::query_as(
        "SELECT et.id, et.slug,
                CASE WHEN et.team_id IS NOT NULL THEN t.slug ELSE NULL END,
                CASE WHEN et.team_id IS NULL THEN u.username ELSE NULL END,
                et.visibility,
                CASE WHEN t.visibility = 'private' THEN t.invite_token ELSE NULL END
         FROM event_types et
         LEFT JOIN teams t ON t.id = et.team_id
         LEFT JOIN accounts a ON a.id = et.account_id
         LEFT JOIN users u ON u.id = a.user_id
         WHERE et.id = ? AND et.enabled = 1
           AND (et.visibility IN ('private', 'internal') OR ?)",
    )
    .bind(event_type_id)
    .bind(include_public)
    .fetch_optional(pool)
    .await?;

    let (et_id, et_slug, team_slug, username, visibility, team_token) = match et {
        Some(e) => e,
        None => return Ok(None),
    };

    // Private event types: only owner (personal) or team admin (team event)
    // can generate quick links. Internal event types remain open to any
    // authenticated user — that's the documented "any authenticated colleague
    // can distribute a one-time link" contract.
    if visibility != "internal" && !can_manage_event_type(pool, user, &et_id).await {
        return Ok(None);
    }

    let base_url = crate::settings::base_url().unwrap_or_default();
    let booking_url = if let Some(ts) = &team_slug {
        format!("{}/team/{}/{}", base_url, ts, et_slug)
    } else if let Some(un) = &username {
        format!("{}/u/{}/{}", base_url, un, et_slug)
    } else {
        format!("{}/{}", base_url, et_slug)
    };
    // Public booking handlers do not consume booking_invites. Return their
    // real reusable URL rather than promising an unenforced single-use link.
    // Public events in private teams use the team's existing access token.
    if visibility == "public" {
        let url = match team_token {
            Some(token) => format!("{}?invite={}", booking_url, urlencoding::encode(&token)),
            None => booking_url,
        };
        return Ok(Some(QuickInvite {
            url,
            expires_at: None,
            max_uses: None,
        }));
    }

    let token = uuid::Uuid::new_v4().to_string();
    let expires_at = (expires_days > 0).then(|| {
        (chrono::Utc::now() + chrono::Duration::days(i64::from(expires_days)))
            .format("%Y-%m-%d %H:%M:%S")
            .to_string()
    });

    sqlx::query(
        "INSERT INTO booking_invites (id, event_type_id, token, guest_name, guest_email, expires_at, max_uses, created_by_user_id)
         VALUES (?, ?, ?, '', '', ?, 1, ?)",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(&et_id)
    .bind(&token)
    .bind(&expires_at)
    .bind(&user.id)
    .execute(pool)
    .await?;

    tracing::info!(event_type = %et_id, created_by = %user.email, "quick invite link generated");

    Ok(Some(QuickInvite {
        url: format!("{}?invite={}", booking_url, token),
        expires_at,
        max_uses: Some(1),
    }))
}
