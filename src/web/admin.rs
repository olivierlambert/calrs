//! Admin panel: users, auth and OIDC, theme, captcha, SMTP, SMS, meeting links, resources.

use super::*;

#[derive(Deserialize)]
pub(super) struct CompanyLinkForm {
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

// --- Admin dashboard ---

pub(super) async fn admin_dashboard(
    State(state): State<Arc<AppState>>,
    admin: crate::auth::AdminUser,
    Query(query): Query<std::collections::HashMap<String, String>>,
) -> impl IntoResponse {
    let current_user = &admin.user;
    let error_message = query.get("error").cloned().unwrap_or_default();
    let resource_notice = query.get("resource_notice").cloned().unwrap_or_default();
    let resource_error = query.get("resource_error").cloned().unwrap_or_default();
    let upload_error = query
        .get("upload_error")
        .map(String::as_str)
        .filter(|reason| matches!(*reason, "logo" | "logo-save" | "favicon" | "favicon-save"))
        .unwrap_or_default();

    // Shared bookable resources
    let resource_rows: Vec<(
        String,
        String,
        String,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
    )> = sqlx::query_as(
        "SELECT id, name, feed_url, caldav_url, caldav_username, caldav_password, last_synced_at, last_sync_error
         FROM resources ORDER BY name",
    )
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();
    // Opted-in members only count as write candidates when they have an
    // enabled CalDAV source on the resource's own server.
    let lender_source_urls: Vec<(String,)> = sqlx::query_as(
        "SELECT cs.url FROM users u
         JOIN accounts a ON a.user_id = u.id
         JOIN caldav_sources cs ON cs.account_id = a.id
         WHERE u.lend_resource_write = 1 AND u.enabled = 1 AND cs.enabled = 1
           AND cs.password_enc IS NOT NULL",
    )
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();
    // All teams, for the per-resource "Teams allowed" checkboxes.
    let resource_teams_all: Vec<(String, String)> =
        sqlx::query_as("SELECT id, name FROM teams ORDER BY name")
            .fetch_all(&state.pool)
            .await
            .unwrap_or_default();
    let resource_teams_ctx: Vec<minijinja::Value> = resource_teams_all
        .iter()
        .map(|(id, name)| context! { id => id, name => name })
        .collect();
    let mut resources_ctx: Vec<minijinja::Value> = Vec::new();
    for (
        rid,
        rname,
        feed_url,
        caldav_url,
        caldav_username,
        caldav_password,
        last_synced_at,
        last_sync_error,
    ) in &resource_rows
    {
        let allowed_teams: Vec<(String, String)> = sqlx::query_as(
            "SELECT t.id, t.name FROM teams t
             JOIN resource_teams rt ON rt.team_id = t.id
             WHERE rt.resource_id = ? ORDER BY t.name",
        )
        .bind(rid)
        .fetch_all(&state.pool)
        .await
        .unwrap_or_default();
        let allowed_team_ids = allowed_teams
            .iter()
            .map(|(id, _)| id.as_str())
            .collect::<Vec<_>>()
            .join(",");
        let allowed_team_names = allowed_teams
            .iter()
            .map(|(_, name)| name.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        let event_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM resource_events WHERE resource_id = ?")
                .bind(rid)
                .fetch_one(&state.pool)
                .await
                .unwrap_or(0);
        let attached_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM event_type_resources WHERE resource_id = ?")
                .bind(rid)
                .fetch_one(&state.pool)
                .await
                .unwrap_or(0);
        let effective_url = caldav_url
            .clone()
            .filter(|u| !u.is_empty())
            .or_else(|| crate::resources::derive_caldav_url(feed_url));
        let has_service = caldav_username.as_deref().is_some_and(|u| !u.is_empty())
            && caldav_password.as_deref().is_some_and(|p| !p.is_empty());
        let lend_count = match effective_url
            .as_deref()
            .and_then(crate::resources::url_origin)
        {
            Some(origin) => lender_source_urls
                .iter()
                .filter(|(u,)| crate::resources::url_origin(u).as_ref() == Some(&origin))
                .count(),
            None => 0,
        };
        let write_configured = effective_url.is_some() && (has_service || lend_count > 0);
        let write_via = if has_service {
            format!(
                "service account {}",
                caldav_username.as_deref().unwrap_or("")
            )
        } else {
            format!(
                "member credentials, {} matching user(s) opted in",
                lend_count
            )
        };
        resources_ctx.push(context! {
            id => rid,
            name => rname,
            feed_url => feed_url,
            caldav_url => caldav_url.clone().unwrap_or_default(),
            caldav_username => caldav_username.clone().unwrap_or_default(),
            write_configured => write_configured,
            write_via => write_via,
            last_synced_at => last_synced_at.clone().unwrap_or_default(),
            last_sync_error => last_sync_error.clone().unwrap_or_default(),
            event_count => event_count,
            attached_count => attached_count,
            allowed_team_ids => allowed_team_ids,
            allowed_team_names => allowed_team_names,
        });
    }

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
    // Never load the secret into the template context.
    let (microsoft_oauth2_client_id, microsoft_oauth2_tenant, microsoft_oauth2_configured):
        (Option<String>, String, bool) = match sqlx::query_as(
            "SELECT microsoft_oauth2_client_id, microsoft_oauth2_tenant, COALESCE(length(microsoft_oauth2_client_id) > 0 AND length(microsoft_oauth2_client_secret) > 0, 0) FROM auth_config WHERE id = 'singleton'",
        ).fetch_one(&state.pool).await {
            Ok(config) => config,
            Err(e) => return internal_error_html("load Microsoft configuration", &e),
        };

    let (
        smtp_configured,
        smtp_host,
        smtp_port,
        smtp_username,
        smtp_from_email,
        smtp_from_name,
        smtp_tls_mode,
        smtp_enabled,
        smtp_from_env,
        smtp_error,
    ) = match crate::email::load_smtp_status(&state.pool).await {
        Ok(Some(status)) => (
            true,
            status.host,
            status.port,
            status.username,
            status.from_email,
            status.from_name.unwrap_or_default(),
            status.tls_mode,
            status.enabled,
            status.from_env,
            String::new(),
        ),
        Ok(None) => (
            false,
            String::new(),
            0u16,
            String::new(),
            String::new(),
            String::new(),
            "starttls".to_string(),
            false,
            false,
            String::new(),
        ),
        Err(e) => (
            false,
            String::new(),
            0u16,
            String::new(),
            String::new(),
            String::new(),
            "starttls".to_string(),
            false,
            true,
            e.to_string(),
        ),
    };

    // Flash banner after a "send test email" round-trip (?smtp_test=sent|error).
    let smtp_test_result = query.get("smtp_test").cloned().unwrap_or_default();

    // SMS gateway status. `sms_provider` drives which set of labels the form
    // shows; the labels themselves come from the provider registry so the
    // template never learns a gateway's name.
    let sms_status = crate::sms::load_status(&state.pool).await;
    let (
        sms_configured,
        sms_provider,
        sms_api_key,
        sms_sender,
        sms_base_url,
        sms_default_country,
        sms_daily_cap,
        sms_enabled,
        sms_from_env,
        sms_error,
    ) = match sms_status {
        Ok(Some(status)) => (
            true,
            status.provider,
            status.api_key,
            status.sender,
            status.base_url,
            status.default_country_code,
            status.daily_cap,
            status.enabled,
            status.from_env,
            String::new(),
        ),
        Ok(None) => (
            false,
            crate::sms::kinds::TWILIO.to_string(),
            String::new(),
            String::new(),
            String::new(),
            crate::sms::phone::DEFAULT_COUNTRY_CODE.to_string(),
            0,
            false,
            false,
            String::new(),
        ),
        Err(e) => (
            false,
            crate::sms::kinds::TWILIO.to_string(),
            String::new(),
            String::new(),
            String::new(),
            crate::sms::phone::DEFAULT_COUNTRY_CODE.to_string(),
            0,
            false,
            false,
            e.to_string(),
        ),
    };

    let (sms_cost_today, sms_cost_currency) = crate::sms::cost_today(&state.pool).await;
    let sms_sent_today = crate::sms::sent_today(&state.pool).await;
    let sms_allow_all_users: bool =
        sqlx::query_scalar("SELECT sms_allow_all_users FROM auth_config WHERE id = 'singleton'")
            .fetch_optional(&state.pool)
            .await
            .unwrap_or(None)
            .unwrap_or(false);

    // Flash banner after a test round-trip (?sms_test=sent|checked|error).
    let sms_test_result = query.get("sms_test").cloned().unwrap_or_default();
    let sms_test_detail = query.get("sms_test_detail").cloned().unwrap_or_default();

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

    let captcha_configured = state.captcha_config.read().await.is_some();
    // The form shows what is stored, not the active config, which is `None`
    // while the configuration is incomplete.
    let captcha::CaptchaFormValues {
        instance_url: captcha_instance_url,
        site_key: captcha_site_key,
        widget_url: captcha_widget_url,
        wasm_url: captcha_wasm_url,
    } = captcha::load_captcha_form_values(&state.pool).await;

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
            lang => admin.lang,
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
            microsoft_oauth2_client_id => microsoft_oauth2_client_id.unwrap_or_default(),
            microsoft_oauth2_tenant => microsoft_oauth2_tenant,
            microsoft_oauth2_configured => microsoft_oauth2_configured,
            microsoft_callback_uri => crate::microsoft_oauth::callback_uri(
                &crate::settings::base_url().unwrap_or_default(),
            )
            .ok(),
            base_url => crate::settings::base_url().unwrap_or_default(),
            base_url_db => crate::settings::base_url_db().unwrap_or_default(),
            base_url_from_env => crate::settings::base_url_from_env(),
            allow_private_hosts => crate::settings::private_host_allowlist().join(", "),
            allow_private_hosts_db => crate::settings::private_host_allowlist_db().join(", "),
            allow_private_hosts_from_env => crate::settings::allow_private_hosts_from_env(),
            smtp_configured => smtp_configured,
            smtp_host => smtp_host,
            smtp_port => smtp_port,
            smtp_username => smtp_username,
            smtp_from_email => smtp_from_email,
            smtp_from_name => smtp_from_name,
            smtp_tls_mode => smtp_tls_mode,
            smtp_enabled => smtp_enabled,
            smtp_from_env => smtp_from_env,
            smtp_error => smtp_error,
            smtp_test_result => smtp_test_result,
            sms_configured => sms_configured,
            sms_provider => sms_provider,
            sms_api_key => sms_api_key,
            sms_sender => sms_sender,
            sms_base_url => sms_base_url,
            sms_default_country => sms_default_country,
            sms_daily_cap => sms_daily_cap,
            sms_sent_today => sms_sent_today,
            sms_cost_today => format!("{:.2}", sms_cost_today),
            sms_cost_currency => sms_cost_currency,
            sms_allow_all_users => sms_allow_all_users,
            sms_enabled => sms_enabled,
            sms_from_env => sms_from_env,
            sms_error => sms_error,
            sms_test_result => sms_test_result,
            sms_test_detail => sms_test_detail,
            sms_supports_check => crate::sms::factory::provider_spec(&sms_provider)
                .map(|spec| spec.supports_check)
                .unwrap_or(false),
            sms_trial_mode => sms_provider == crate::sms::kinds::TWILIO
                && crate::sms::twilio::trial_mode_enabled(),
            sms_providers => crate::sms::PROVIDER_SPECS,
            sms_country_codes => crate::sms::phone::COUNTRY_CODES
                .iter()
                .map(|(code, label)| context! { code => code, label => label })
                .collect::<Vec<_>>(),
            captcha_configured => captcha_configured,
            captcha_instance_url => captcha_instance_url,
            captcha_site_key => captcha_site_key,
            captcha_widget_url => captcha_widget_url,
            captcha_wasm_url => captcha_wasm_url,
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
            has_favicon => state.data_dir.join(FAVICON_FILE).exists(),
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
            resources => resources_ctx,
            resource_teams_all => resource_teams_ctx,
            resource_notice => resource_notice,
            resource_error => resource_error,
            upload_error => upload_error,
            logo_max_bytes => LOGO_MAX_BYTES,
            favicon_max_bytes => FAVICON_MAX_BYTES,
        })
        .unwrap_or_else(|e| internal_error_body("template render", &e)),
    )
}

#[derive(Deserialize)]
pub(super) struct AdminResourceForm {
    _csrf: Option<String>,
    name: Option<String>,
    feed_url: String,
    caldav_url: Option<String>,
    caldav_username: Option<String>,
    caldav_password: Option<String>,
    /// Comma-joined team ids allowed to use this resource (hidden field
    /// kept in sync with the checkboxes, same pattern as `resource_ids`
    /// on the event type form).
    team_ids: Option<String>,
}

/// Persist a resource's team allowlist (delete-then-insert). Unknown team
/// ids are dropped by inserting through a SELECT on `teams`.
async fn save_resource_teams(pool: &SqlitePool, resource_id: &str, team_ids: &Option<String>) {
    let _ = sqlx::query("DELETE FROM resource_teams WHERE resource_id = ?")
        .bind(resource_id)
        .execute(pool)
        .await;
    if let Some(ids) = team_ids {
        for tid in ids.split(',').map(str::trim).filter(|s| !s.is_empty()) {
            let _ = sqlx::query(
                "INSERT OR IGNORE INTO resource_teams (resource_id, team_id)
                 SELECT ?, id FROM teams WHERE id = ?",
            )
            .bind(resource_id)
            .bind(tid)
            .execute(pool)
            .await;
        }
    }
}

fn admin_resources_redirect(notice: &str) -> Response {
    Redirect::to(&format!(
        "/dashboard/admin?resource_notice={}",
        urlencoding::encode(notice)
    ))
    .into_response()
}

fn admin_resources_redirect_err(error: &str) -> Response {
    Redirect::to(&format!(
        "/dashboard/admin?resource_error={}",
        urlencoding::encode(error)
    ))
    .into_response()
}

pub(super) async fn admin_create_resource(
    State(state): State<Arc<AppState>>,
    _admin: crate::auth::AdminUser,
    headers: HeaderMap,
    Form(form): Form<AdminResourceForm>,
) -> impl IntoResponse {
    if let Err(resp) = verify_csrf_token(&headers, &form._csrf) {
        return resp;
    }
    let feed_url = form.feed_url.trim().to_string();
    if let Err(e) = crate::caldav::validate_caldav_url(&feed_url) {
        return admin_resources_redirect_err(&tr1(
            _admin.lang,
            "admin-resource-feed-invalid",
            "error",
            &e.to_string(),
        ));
    }
    // The feed must answer before we store anything, and it gives us the name.
    let body = match crate::resources::fetch_feed(&feed_url).await {
        Ok(b) => b,
        Err(e) => {
            return admin_resources_redirect_err(&tr1(
                _admin.lang,
                "admin-resource-feed-check-failed",
                "error",
                &e.to_string(),
            ));
        }
    };
    let name = form
        .name
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .or_else(|| crate::resources::feed_calendar_name(&body))
        .unwrap_or_else(|| "Resource".to_string());

    let caldav_url = form
        .caldav_url
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    if let Some(u) = caldav_url {
        if let Err(e) = crate::caldav::validate_caldav_url(u) {
            return admin_resources_redirect_err(&tr1(
                _admin.lang,
                "admin-resource-caldav-invalid",
                "error",
                &e.to_string(),
            ));
        }
    }
    let caldav_username = form
        .caldav_username
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let caldav_password_enc = match form.caldav_password.as_deref().filter(|s| !s.is_empty()) {
        Some(p) => match crate::crypto::encrypt_password(&state.secret_key, p) {
            Ok(enc) => Some(enc),
            Err(e) => {
                return admin_resources_redirect_err(&tr1(
                    _admin.lang,
                    "admin-resource-credentials-failed",
                    "error",
                    &e.to_string(),
                ));
            }
        },
        None => None,
    };

    let id = uuid::Uuid::new_v4().to_string();
    let _ = sqlx::query(
        "INSERT INTO resources (id, name, feed_url, caldav_url, caldav_username, caldav_password)
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(&name)
    .bind(&feed_url)
    .bind(caldav_url)
    .bind(caldav_username)
    .bind(&caldav_password_enc)
    .execute(&state.pool)
    .await;
    save_resource_teams(&state.pool, &id, &form.team_ids).await;

    let cached = crate::resources::sync_resource(&state.pool, &id, &feed_url)
        .await
        .unwrap_or(0);
    tracing::info!(resource_name = %name, admin = %_admin.user.email, "admin: resource created");
    let mut args = fluent_bundle::FluentArgs::new();
    args.set("name", fluent_bundle::FluentValue::from(name.as_str()));
    args.set("count", crate::i18n::number(cached as f64));
    admin_resources_redirect(&crate::i18n::translate(
        _admin.lang,
        "admin-resource-added",
        Some(&args),
    ))
}

pub(super) async fn admin_update_resource(
    State(state): State<Arc<AppState>>,
    _admin: crate::auth::AdminUser,
    headers: HeaderMap,
    Path(resource_id): Path<String>,
    Form(form): Form<AdminResourceForm>,
) -> impl IntoResponse {
    if let Err(resp) = verify_csrf_token(&headers, &form._csrf) {
        return resp;
    }
    let feed_url = form.feed_url.trim().to_string();
    if let Err(e) = crate::caldav::validate_caldav_url(&feed_url) {
        return admin_resources_redirect_err(&tr1(
            _admin.lang,
            "admin-resource-feed-invalid",
            "error",
            &e.to_string(),
        ));
    }
    let caldav_url = form
        .caldav_url
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    if let Some(u) = caldav_url {
        if let Err(e) = crate::caldav::validate_caldav_url(u) {
            return admin_resources_redirect_err(&tr1(
                _admin.lang,
                "admin-resource-caldav-invalid",
                "error",
                &e.to_string(),
            ));
        }
    }
    // Same contract as create: the feed must answer before we store it, so
    // a typo'd URL is caught now instead of silently serving stale events
    // for up to 5 minutes.
    if let Err(e) = crate::resources::fetch_feed(&feed_url).await {
        return admin_resources_redirect_err(&tr1(
            _admin.lang,
            "admin-resource-feed-check-failed",
            "error",
            &e.to_string(),
        ));
    }
    let name = form
        .name
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("Resource");
    let _ = sqlx::query(
        "UPDATE resources SET name = ?, feed_url = ?, caldav_url = ?, caldav_username = ? WHERE id = ?",
    )
    .bind(name)
    .bind(&feed_url)
    .bind(caldav_url)
    .bind(
        form.caldav_username
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty()),
    )
    .bind(&resource_id)
    .execute(&state.pool)
    .await;
    // Keep-current pattern: empty password field preserves the stored
    // secret. Clearing the username clears the password with it, so no
    // orphaned credential half survives.
    if form
        .caldav_username
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .is_none()
    {
        let _ = sqlx::query("UPDATE resources SET caldav_password = NULL WHERE id = ?")
            .bind(&resource_id)
            .execute(&state.pool)
            .await;
    } else if let Some(p) = form.caldav_password.as_deref().filter(|s| !s.is_empty()) {
        if let Ok(enc) = crate::crypto::encrypt_password(&state.secret_key, p) {
            let _ = sqlx::query("UPDATE resources SET caldav_password = ? WHERE id = ?")
                .bind(&enc)
                .bind(&resource_id)
                .execute(&state.pool)
                .await;
        }
    }
    save_resource_teams(&state.pool, &resource_id, &form.team_ids).await;
    let cached = crate::resources::sync_resource(&state.pool, &resource_id, &feed_url)
        .await
        .unwrap_or(0);
    let mut args = fluent_bundle::FluentArgs::new();
    args.set("count", crate::i18n::number(cached as f64));
    admin_resources_redirect(&crate::i18n::translate(
        _admin.lang,
        "admin-resource-updated",
        Some(&args),
    ))
}

pub(super) async fn admin_delete_resource(
    State(state): State<Arc<AppState>>,
    _admin: crate::auth::AdminUser,
    headers: HeaderMap,
    Path(resource_id): Path<String>,
    Form(csrf): Form<CsrfForm>,
) -> impl IntoResponse {
    if let Err(resp) = verify_csrf_token(&headers, &csrf._csrf) {
        return resp;
    }
    let _ = sqlx::query("DELETE FROM resources WHERE id = ?")
        .bind(&resource_id)
        .execute(&state.pool)
        .await;
    tracing::info!(resource_id = %resource_id, admin = %_admin.user.email, "admin: resource deleted");
    admin_resources_redirect(&crate::i18n::translate(
        _admin.lang,
        "admin-resource-deleted",
        None,
    ))
}

pub(super) async fn admin_sync_resource(
    State(state): State<Arc<AppState>>,
    _admin: crate::auth::AdminUser,
    headers: HeaderMap,
    Path(resource_id): Path<String>,
    Form(csrf): Form<CsrfForm>,
) -> impl IntoResponse {
    if let Err(resp) = verify_csrf_token(&headers, &csrf._csrf) {
        return resp;
    }
    let feed_url: Option<String> =
        sqlx::query_scalar("SELECT feed_url FROM resources WHERE id = ?")
            .bind(&resource_id)
            .fetch_optional(&state.pool)
            .await
            .unwrap_or(None);
    let Some(feed_url) = feed_url else {
        return admin_resources_redirect_err(&crate::i18n::translate(
            _admin.lang,
            "admin-resource-not-found",
            None,
        ));
    };
    match crate::resources::sync_resource(&state.pool, &resource_id, &feed_url).await {
        Ok(n) => {
            let mut args = fluent_bundle::FluentArgs::new();
            args.set("count", crate::i18n::number(n as f64));
            admin_resources_redirect(&crate::i18n::translate(
                _admin.lang,
                "admin-resource-synced",
                Some(&args),
            ))
        }
        Err(e) => admin_resources_redirect_err(&tr1(
            _admin.lang,
            "admin-resource-sync-error",
            "error",
            &e.to_string(),
        )),
    }
}

pub(super) async fn admin_test_write_resource(
    State(state): State<Arc<AppState>>,
    _admin: crate::auth::AdminUser,
    headers: HeaderMap,
    Path(resource_id): Path<String>,
    Form(csrf): Form<CsrfForm>,
) -> impl IntoResponse {
    if let Err(resp) = verify_csrf_token(&headers, &csrf._csrf) {
        return resp;
    }
    let row: Option<(String, Option<String>, Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT feed_url, caldav_url, caldav_username, caldav_password FROM resources WHERE id = ?",
    )
    .bind(&resource_id)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);
    let Some((feed_url, caldav_url, service_user, service_pw)) = row else {
        return admin_resources_redirect_err(&crate::i18n::translate(
            _admin.lang,
            "admin-resource-not-found",
            None,
        ));
    };
    let Some(url) = caldav_url
        .filter(|u| !u.is_empty())
        .or_else(|| crate::resources::derive_caldav_url(&feed_url))
    else {
        return admin_resources_redirect_err(&crate::i18n::translate(
            _admin.lang,
            "admin-resource-no-caldav-url",
            None,
        ));
    };
    let candidates = resource_write_candidates(
        &state.pool,
        &state.secret_key,
        &url,
        service_user.as_deref(),
        service_pw.as_deref(),
        None,
    )
    .await;
    if candidates.is_empty() {
        return admin_resources_redirect_err(&crate::i18n::translate(
            _admin.lang,
            "admin-resource-no-write-credentials",
            None,
        ));
    }
    // Full PUT / verify / DELETE / verify cycle with a clearly-labeled
    // temporary event placed 24h out.
    let test_uid = format!("calrs-write-test-{}", uuid::Uuid::new_v4());
    let start = chrono::Utc::now() + Duration::hours(24);
    let end = start + Duration::minutes(15);
    let ics = format!(
        "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//calrs//write-test//EN\r\nBEGIN:VEVENT\r\nUID:{}\r\nDTSTAMP:{}\r\nDTSTART:{}\r\nDTEND:{}\r\nSUMMARY:calrs write test (safe to delete)\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n",
        test_uid,
        chrono::Utc::now().format("%Y%m%dT%H%M%SZ"),
        start.format("%Y%m%dT%H%M%SZ"),
        end.format("%Y%m%dT%H%M%SZ"),
    );
    let mut last_err = String::new();
    for (username, password) in &candidates {
        let client = crate::caldav::CaldavClient::new(&url, username, password);
        match client.put_event(&url, &test_uid, &ics).await {
            Ok(()) => {
                let existed = client.event_exists(&url, &test_uid).await.unwrap_or(false);
                let _ = client.delete_event(&url, &test_uid).await;
                let gone = !client.event_exists(&url, &test_uid).await.unwrap_or(true);
                let lang = _admin.lang;
                let verified = crate::i18n::translate(lang, "admin-resource-write-verified", None);
                let unverified =
                    crate::i18n::translate(lang, "admin-resource-write-unverified", None);
                let check =
                    crate::i18n::translate(lang, "admin-resource-write-check-calendar", None);
                let mut args = fluent_bundle::FluentArgs::new();
                args.set("user", fluent_bundle::FluentValue::from(username.as_str()));
                args.set(
                    "created",
                    fluent_bundle::FluentValue::from(
                        if existed { &verified } else { &unverified }.as_str(),
                    ),
                );
                args.set(
                    "cleaned",
                    fluent_bundle::FluentValue::from(
                        if gone { &verified } else { &check }.as_str(),
                    ),
                );
                return admin_resources_redirect(&crate::i18n::translate(
                    lang,
                    "admin-resource-write-passed",
                    Some(&args),
                ));
            }
            Err(e) => last_err = format!("{}: {}", username, e),
        }
    }
    admin_resources_redirect_err(&tr1(
        _admin.lang,
        "admin-resource-write-failed",
        "error",
        &last_err,
    ))
}

/// Back to the admin panel with the "last enabled admin" refusal, in the
/// acting admin's language.
fn admin_last_admin_refusal(lang: &str) -> Response {
    let message = crate::i18n::translate(lang, "admin-error-last-admin", None);
    Redirect::to(&format!(
        "/dashboard/admin?error={}",
        urlencoding::encode(&message)
    ))
    .into_response()
}

pub(super) async fn admin_toggle_role(
    State(state): State<Arc<AppState>>,
    admin: crate::auth::AdminUser,
    headers: HeaderMap,
    Path(user_id): Path<String>,
    Form(csrf): Form<CsrfForm>,
) -> impl IntoResponse {
    if let Err(resp) = verify_csrf_token(&headers, &csrf._csrf) {
        return resp;
    }
    let current_role: Option<String> = sqlx::query_scalar("SELECT role FROM users WHERE id = ?")
        .bind(&user_id)
        .fetch_optional(&state.pool)
        .await
        .unwrap_or(None);
    let Some(role) = current_role else {
        return Redirect::to("/dashboard/admin").into_response();
    };
    let make_admin = role != "admin";
    // The guard is part of the UPDATE, so two admins demoting each other at
    // the same moment cannot leave the instance with none.
    match crate::auth::set_user_admin(&state.pool, &user_id, make_admin).await {
        Ok(crate::auth::AdminChange::Changed) => {
            tracing::info!(target_user = %user_id, admin_role = make_admin, admin = %admin.user.email, "admin: role changed");
        }
        Ok(crate::auth::AdminChange::LastAdmin) => {
            tracing::warn!(target_user = %user_id, admin = %admin.user.email, "admin: refused to demote the last enabled admin");
            return admin_last_admin_refusal(admin.lang);
        }
        Ok(crate::auth::AdminChange::NotFound) => {}
        Err(e) => return internal_error_response("admin role change", &e),
    }

    Redirect::to("/dashboard/admin").into_response()
}

pub(super) async fn admin_toggle_enabled(
    State(state): State<Arc<AppState>>,
    admin: crate::auth::AdminUser,
    headers: HeaderMap,
    Path(user_id): Path<String>,
    Form(csrf): Form<CsrfForm>,
) -> impl IntoResponse {
    if let Err(resp) = verify_csrf_token(&headers, &csrf._csrf) {
        return resp;
    }
    let current: Option<bool> = sqlx::query_scalar("SELECT enabled FROM users WHERE id = ?")
        .bind(&user_id)
        .fetch_optional(&state.pool)
        .await
        .unwrap_or(None);
    let Some(enabled) = current else {
        return Redirect::to("/dashboard/admin").into_response();
    };
    let new_enabled = !enabled;
    match crate::auth::set_user_enabled(&state.pool, &user_id, new_enabled).await {
        Ok(crate::auth::AdminChange::Changed) => {
            tracing::info!(target_user = %user_id, enabled = %new_enabled, admin = %admin.user.email, "admin: user toggled");
        }
        Ok(crate::auth::AdminChange::LastAdmin) => {
            tracing::warn!(target_user = %user_id, admin = %admin.user.email, "admin: refused to disable the last enabled admin");
            return admin_last_admin_refusal(admin.lang);
        }
        Ok(crate::auth::AdminChange::NotFound) => {}
        Err(e) => return internal_error_response("admin user toggle", &e),
    }

    Redirect::to("/dashboard/admin").into_response()
}

pub(super) async fn admin_delete_user(
    State(state): State<Arc<AppState>>,
    admin: crate::auth::AdminUser,
    headers: HeaderMap,
    Path(user_id): Path<String>,
    Form(csrf): Form<CsrfForm>,
) -> impl IntoResponse {
    if let Err(resp) = verify_csrf_token(&headers, &csrf._csrf) {
        return resp;
    }
    let admin_user = &admin.user;
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
        Err(crate::auth::DeleteUserError::LastAdmin) => {
            tracing::warn!(target_user = %user_id, admin = %admin_user.email, "admin: refused to delete the last enabled admin");
            admin_last_admin_refusal(admin.lang)
        }
        Err(e) => {
            tracing::warn!(target_user = %user_id, admin = %admin_user.email, error = %e, "admin: user delete refused");
            let encoded = urlencoding::encode(&e.to_string()).into_owned();
            Redirect::to(&format!("/dashboard/admin?error={}", encoded)).into_response()
        }
    }
}

#[derive(Deserialize)]
pub(super) struct AdminAuthForm {
    _csrf: Option<String>,
    registration_enabled: Option<String>,
    allowed_email_domains: Option<String>,
}

pub(super) async fn admin_update_auth(
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

    tracing::info!(admin = %_admin.user.email, "admin: auth config updated");

    Redirect::to("/dashboard/admin").into_response()
}

#[derive(Deserialize)]
pub(super) struct AdminThemeForm {
    _csrf: Option<String>,
    theme: Option<String>,
    custom_accent: Option<String>,
    custom_accent_hover: Option<String>,
    custom_bg: Option<String>,
    custom_surface: Option<String>,
    custom_text: Option<String>,
}

pub(super) async fn admin_update_accent(
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

    tracing::info!(admin = %_admin.user.email, theme = %theme, "admin: theme updated");

    Redirect::to("/dashboard/admin").into_response()
}

#[derive(Deserialize)]
pub(super) struct AdminOidcForm {
    _csrf: Option<String>,
    oidc_enabled: Option<String>,
    oidc_issuer_url: Option<String>,
    oidc_client_id: Option<String>,
    oidc_client_secret: Option<String>,
    oidc_auto_register: Option<String>,
}

pub(super) async fn admin_update_oidc(
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

    tracing::info!(admin = %_admin.user.email, "admin: OIDC config updated");

    Redirect::to("/dashboard/admin").into_response()
}

#[derive(Deserialize)]
pub(super) struct AdminGoogleOAuth2Form {
    _csrf: Option<String>,
    google_oauth2_client_id: Option<String>,
    google_oauth2_client_secret: Option<String>,
}

pub(super) async fn admin_update_google_oauth2(
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

    tracing::info!(admin = %_admin.user.email, "admin: Google OAuth2 config updated");
    Redirect::to("/dashboard/admin").into_response()
}

// --- Captcha settings ---

#[derive(Deserialize)]
pub(super) struct AdminCaptchaForm {
    _csrf: Option<String>,
    captcha_instance_url: Option<String>,
    captcha_site_key: Option<String>,
    captcha_secret: Option<String>,
    captcha_widget_url: Option<String>,
    captcha_wasm_url: Option<String>,
}

// --- General / system runtime settings (base URL, private-host allowlist) ---

#[derive(Deserialize)]
pub(super) struct AdminGeneralForm {
    _csrf: Option<String>,
    base_url: Option<String>,
    allow_private_hosts: Option<String>,
}

pub(super) async fn admin_update_general(
    State(state): State<Arc<AppState>>,
    _admin: crate::auth::AdminUser,
    headers: HeaderMap,
    Form(form): Form<AdminGeneralForm>,
) -> impl IntoResponse {
    if let Err(resp) = verify_csrf_token(&headers, &form._csrf) {
        return resp;
    }

    // Only persist a field that isn't currently forced by an environment
    // variable. A forced field is read-only in the UI and submits the env
    // value back; writing it would either clobber the stored DB value with NULL
    // (disabled inputs) or leak the env value into the DB. The env keeps
    // precedence regardless, so there's nothing to save for those fields.
    if !crate::settings::base_url_from_env() {
        // Normalise (strip trailing slash). Empty clears the override; a
        // non-empty value must be an absolute http(s) URL with a host. We do
        // NOT run the SSRF guard here — the public base URL is legitimately
        // allowed to be localhost / a private host in dev.
        let raw = form
            .base_url
            .as_deref()
            .map(|s| s.trim().trim_end_matches('/'))
            .filter(|s| !s.is_empty());
        let base_url = match raw {
            None => None,
            Some(raw) => match reqwest::Url::parse(raw) {
                Ok(u) if matches!(u.scheme(), "http" | "https") && u.host_str().is_some() => {
                    Some(raw.to_string())
                }
                _ => {
                    let msg = urlencoding::encode(&crate::i18n::translate(
                        _admin.lang,
                        "admin-base-url-invalid",
                        None,
                    ))
                    .into_owned();
                    return Redirect::to(&format!("/dashboard/admin?error={}", msg))
                        .into_response();
                }
            },
        };
        if let Err(e) = sqlx::query(
            "UPDATE auth_config SET base_url = ?, updated_at = datetime('now') WHERE id = 'singleton'",
        )
        .bind(&base_url)
        .execute(&state.pool)
        .await
        {
            return internal_error_response("save base_url", &e);
        }
    }

    if !crate::settings::allow_private_hosts_from_env() {
        // Re-serialise through the canonical parser so what we store matches
        // what `settings` reads back. Empty → NULL (clears the DB override).
        let allow_private_hosts = form
            .allow_private_hosts
            .as_deref()
            .map(|s| crate::settings::parse_host_list(s).join(","))
            .filter(|s| !s.is_empty());
        if let Err(e) = sqlx::query(
            "UPDATE auth_config SET allow_private_hosts = ?, updated_at = datetime('now') WHERE id = 'singleton'",
        )
        .bind(&allow_private_hosts)
        .execute(&state.pool)
        .await
        {
            return internal_error_response("save allow_private_hosts", &e);
        }
    }

    // Refresh the process-global cache so the change takes effect immediately.
    crate::settings::load_from_db(&state.pool).await;

    tracing::info!(admin = %_admin.user.email, "admin: general settings updated");
    Redirect::to("/dashboard/admin").into_response()
}

pub(super) async fn admin_update_captcha(
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
    let wasm_url = form
        .captcha_wasm_url
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    if wasm_url
        .as_deref()
        .is_some_and(|u| !captcha::is_http_url(u))
    {
        let msg = crate::i18n::translate(_admin.lang, "admin-captcha-wasm-url-invalid", None);
        let encoded = urlencoding::encode(&msg).into_owned();
        return Redirect::to(&format!("/dashboard/admin?error={}", encoded)).into_response();
    }

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
            "UPDATE auth_config SET captcha_instance_url = ?, captcha_site_key = ?, captcha_secret = ?, captcha_widget_url = ?, captcha_wasm_url = ?, updated_at = datetime('now') WHERE id = 'singleton'",
        )
        .bind(&instance_url)
        .bind(&site_key)
        .bind(&encrypted)
        .bind(&widget_url)
        .bind(&wasm_url)
        .execute(&state.pool)
        .await
        {
            tracing::error!(error = %e, "failed to save captcha config");
        }
    } else {
        if let Err(e) = sqlx::query(
            "UPDATE auth_config SET captcha_instance_url = ?, captcha_site_key = ?, captcha_widget_url = ?, captcha_wasm_url = ?, updated_at = datetime('now') WHERE id = 'singleton'",
        )
        .bind(&instance_url)
        .bind(&site_key)
        .bind(&widget_url)
        .bind(&wasm_url)
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

    tracing::info!(admin = %_admin.user.email, "admin: captcha config updated");

    Redirect::to("/dashboard/admin").into_response()
}

// --- SMTP settings (database-backed, editable from the admin panel) ---

#[derive(Deserialize)]
pub(super) struct AdminSmtpForm {
    _csrf: Option<String>,
    host: Option<String>,
    port: Option<String>,
    tls_mode: Option<String>,
    username: Option<String>,
    /// Leave empty to keep the currently stored password (keep-current pattern).
    password: Option<String>,
    from_email: Option<String>,
    from_name: Option<String>,
    /// HTML checkbox: present (any value) when checked, absent when unchecked.
    enabled: Option<String>,
}

pub(super) async fn admin_update_smtp(
    State(state): State<Arc<AppState>>,
    _admin: crate::auth::AdminUser,
    headers: HeaderMap,
    Form(form): Form<AdminSmtpForm>,
) -> impl IntoResponse {
    if let Err(resp) = verify_csrf_token(&headers, &form._csrf) {
        return resp;
    }

    // Defensive guard: when the env block governs, the DB config is shadowed and
    // the UI form is locked. Refuse to write so the two channels never diverge.
    if crate::email::smtp_env_active() {
        return Redirect::to("/dashboard/admin").into_response();
    }

    let redirect_err = |msg: &str| {
        let encoded = urlencoding::encode(msg).into_owned();
        Redirect::to(&format!("/dashboard/admin?error={}", encoded)).into_response()
    };

    let host = form.host.unwrap_or_default().trim().to_string();
    if host.is_empty() {
        return redirect_err(&crate::i18n::translate(
            _admin.lang,
            "admin-smtp-host-required",
            None,
        ));
    }

    let from_email = form.from_email.unwrap_or_default().trim().to_string();
    if from_email.len() > 320
        || !from_email.contains('@')
        || from_email
            .rsplit('@')
            .next()
            .is_none_or(|domain| !domain.contains('.'))
    {
        return redirect_err(&crate::i18n::translate(
            _admin.lang,
            "admin-smtp-from-invalid",
            None,
        ));
    }

    let port: i32 = match form.port.as_deref().map(str::trim) {
        Some("") | None => 587,
        Some(p) => match p.parse::<u16>() {
            Ok(p) => p as i32,
            Err(_) => {
                return redirect_err(&crate::i18n::translate(
                    _admin.lang,
                    "admin-smtp-port-invalid",
                    None,
                ))
            }
        },
    };

    let tls_mode = match form.tls_mode.as_deref().map(str::trim) {
        Some("tls") => "tls",
        Some("none") => "none",
        Some("starttls") | Some("") | None => "starttls",
        Some(_) => {
            return redirect_err(&crate::i18n::translate(
                _admin.lang,
                "admin-smtp-tls-invalid",
                None,
            ))
        }
    };

    let username = form.username.unwrap_or_default().trim().to_string();
    let from_name = form.from_name.and_then(|s| {
        let t = s.trim().to_string();
        if t.is_empty() {
            None
        } else {
            Some(t)
        }
    });
    let enabled = form.enabled.is_some();

    let password = form.password.unwrap_or_default();
    let password_provided = !password.trim().is_empty();

    let existing: Option<(String,)> = match sqlx::query_as("SELECT id FROM smtp_config LIMIT 1")
        .fetch_optional(&state.pool)
        .await
    {
        Ok(row) => row,
        Err(e) => return internal_error_response("check existing smtp config", &e),
    };

    let result = match existing {
        Some((id,)) if password_provided => {
            let password_enc = match crate::crypto::encrypt_password(&state.secret_key, &password) {
                Ok(s) => s,
                Err(e) => return internal_error_response("encrypt smtp password", &e),
            };
            sqlx::query(
                "UPDATE smtp_config SET host = ?, port = ?, username = ?, password_enc = ?, from_email = ?, from_name = ?, tls_mode = ?, enabled = ? WHERE id = ?",
            )
            .bind(&host)
            .bind(port)
            .bind(&username)
            .bind(&password_enc)
            .bind(&from_email)
            .bind(&from_name)
            .bind(tls_mode)
            .bind(enabled)
            .bind(&id)
            .execute(&state.pool)
            .await
        }
        Some((id,)) => {
            // Keep-current: do not touch password_enc.
            sqlx::query(
                "UPDATE smtp_config SET host = ?, port = ?, username = ?, from_email = ?, from_name = ?, tls_mode = ?, enabled = ? WHERE id = ?",
            )
            .bind(&host)
            .bind(port)
            .bind(&username)
            .bind(&from_email)
            .bind(&from_name)
            .bind(tls_mode)
            .bind(enabled)
            .bind(&id)
            .execute(&state.pool)
            .await
        }
        None => {
            // Fresh config needs a password — there is nothing to keep.
            if !password_provided {
                return redirect_err(&crate::i18n::translate(
                    _admin.lang,
                    "admin-smtp-password-required",
                    None,
                ));
            }
            let password_enc = match crate::crypto::encrypt_password(&state.secret_key, &password) {
                Ok(s) => s,
                Err(e) => return internal_error_response("encrypt smtp password", &e),
            };
            sqlx::query(
                "INSERT INTO smtp_config (id, host, port, username, password_enc, from_email, from_name, tls_mode, enabled) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(uuid::Uuid::new_v4().to_string())
            .bind(&host)
            .bind(port)
            .bind(&username)
            .bind(&password_enc)
            .bind(&from_email)
            .bind(&from_name)
            .bind(tls_mode)
            .bind(enabled)
            .execute(&state.pool)
            .await
        }
    };

    if let Err(e) = result {
        return internal_error_response("save smtp config", &e);
    }

    tracing::info!(admin = %_admin.user.email, "admin: smtp config updated");
    Redirect::to("/dashboard/admin").into_response()
}

#[derive(Deserialize)]
pub(super) struct AdminSmtpTestForm {
    _csrf: Option<String>,
    to: Option<String>,
}

pub(super) async fn admin_update_smtp_test(
    State(state): State<Arc<AppState>>,
    admin: crate::auth::AdminUser,
    headers: HeaderMap,
    Form(form): Form<AdminSmtpTestForm>,
) -> impl IntoResponse {
    if let Err(resp) = verify_csrf_token(&headers, &form._csrf) {
        return resp;
    }

    let to = form
        .to
        .and_then(|s| {
            let t = s.trim().to_string();
            if t.is_empty() {
                None
            } else {
                Some(t)
            }
        })
        .unwrap_or_else(|| admin.user.email.clone());

    let config = match crate::email::load_smtp_config(&state.pool, &state.secret_key).await {
        Ok(Some(c)) => c,
        Ok(None) => {
            return Redirect::to("/dashboard/admin?smtp_test=error").into_response();
        }
        Err(e) => {
            tracing::warn!(error = %e, "admin: smtp test could not load config");
            return Redirect::to("/dashboard/admin?smtp_test=error").into_response();
        }
    };

    match crate::email::send_test_email(&config, &to).await {
        Ok(()) => {
            tracing::info!(admin = %admin.user.email, %to, "admin: smtp test email sent");
            Redirect::to("/dashboard/admin?smtp_test=sent").into_response()
        }
        Err(e) => {
            tracing::warn!(admin = %admin.user.email, error = format!("{e:#}"), "admin: smtp test email failed");
            Redirect::to("/dashboard/admin?smtp_test=error").into_response()
        }
    }
}

#[derive(Deserialize)]
pub(super) struct AdminSmtpClearForm {
    _csrf: Option<String>,
}

pub(super) async fn admin_update_smtp_clear(
    State(state): State<Arc<AppState>>,
    _admin: crate::auth::AdminUser,
    headers: HeaderMap,
    Form(form): Form<AdminSmtpClearForm>,
) -> impl IntoResponse {
    if let Err(resp) = verify_csrf_token(&headers, &form._csrf) {
        return resp;
    }

    // Same guard as admin_update_smtp: when the env block governs, the DB config
    // is shadowed and the UI hides this button — refuse the write defensively.
    if crate::email::smtp_env_active() {
        return Redirect::to("/dashboard/admin").into_response();
    }

    if let Err(e) = sqlx::query("DELETE FROM smtp_config")
        .execute(&state.pool)
        .await
    {
        return internal_error_response("clear smtp config", &e);
    }

    tracing::info!(admin = %_admin.user.email, "admin: smtp config cleared");
    Redirect::to("/dashboard/admin").into_response()
}

// --- SMS settings (database-backed, editable from the admin panel) ---
//
// Deliberately mirrors the SMTP settings above: env-var override, DB singleton
// row, "keep current" pattern on the secret field, AES-256-GCM at rest via
// crate::crypto. The form is provider-agnostic: the same four inputs are
// relabelled per gateway from `sms::ProviderSpec`, so adding a gateway needs no
// change here. SMS stays entirely opt-in: with no config row, the event-type
// toggle simply has nothing to send through.

#[derive(Deserialize)]
pub(super) struct AdminSmsForm {
    _csrf: Option<String>,
    provider: Option<String>,
    /// Non-secret account identifier (Twilio's Account SID). Unused by gateways
    /// that authenticate with a bare token.
    api_key: Option<String>,
    /// Leave empty to keep the currently stored secret (keep-current pattern).
    api_secret: Option<String>,
    sender: Option<String>,
    /// Region or self-hosted endpoint, and the target URL for the webhook provider.
    base_url: Option<String>,
    /// Default country calling code used to normalise guest-entered local numbers.
    default_country_code: Option<String>,
    /// Instance-wide messages/day ceiling; empty or 0 means no limit.
    daily_cap: Option<String>,
    /// HTML checkbox: present (any value) when checked, absent when unchecked.
    enabled: Option<String>,
}

pub(super) async fn admin_update_sms(
    State(state): State<Arc<AppState>>,
    _admin: crate::auth::AdminUser,
    headers: HeaderMap,
    Form(form): Form<AdminSmsForm>,
) -> impl IntoResponse {
    if let Err(resp) = verify_csrf_token(&headers, &form._csrf) {
        return resp;
    }

    // Defensive guard: when the env block governs, the DB config is shadowed
    // and the UI form is locked. Refuse to write so the two channels never diverge.
    if crate::sms::sms_env_active() {
        return Redirect::to("/dashboard/admin").into_response();
    }

    let redirect_err = |msg: &str| {
        let encoded = urlencoding::encode(msg).into_owned();
        Redirect::to(&format!("/dashboard/admin?error={}", encoded)).into_response()
    };

    let provider = form.provider.unwrap_or_default().trim().to_string();
    let api_key = form.api_key.unwrap_or_default().trim().to_string();
    let sender = form.sender.unwrap_or_default().trim().to_string();
    let base_url = form.base_url.unwrap_or_default().trim().to_string();
    let default_country_code = form
        .default_country_code
        .unwrap_or_else(|| crate::sms::phone::DEFAULT_COUNTRY_CODE.to_string())
        .trim()
        .to_string();
    let enabled = form.enabled.is_some();
    let daily_cap: i64 = form
        .daily_cap
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .and_then(|v| v.parse().ok())
        .unwrap_or(0)
        .max(0);

    let api_secret = form.api_secret.unwrap_or_default();
    let secret_provided = !api_secret.trim().is_empty();

    let existing: Option<(String, Option<String>)> =
        match sqlx::query_as("SELECT id, api_secret_enc FROM sms_config LIMIT 1")
            .fetch_optional(&state.pool)
            .await
        {
            Ok(row) => row,
            Err(e) => return internal_error_response("check existing sms config", &e),
        };

    // Switching gateway must come with that gateway's own credential: keeping
    // the stored one would silently save, say, a Twilio auth token as a
    // GatewayAPI token and only fail at send time, inside a booking.
    let previous_provider: Option<String> =
        match sqlx::query_scalar("SELECT provider FROM sms_config LIMIT 1")
            .fetch_optional(&state.pool)
            .await
        {
            Ok(row) => row,
            Err(e) => return internal_error_response("check existing sms provider", &e),
        };
    let switching_provider = previous_provider
        .as_deref()
        .is_some_and(|prev| prev != provider);
    if switching_provider && !secret_provided && provider != crate::sms::kinds::WEBHOOK {
        return redirect_err(&crate::i18n::translate(
            _admin.lang,
            "admin-sms-switch-needs-secret",
            None,
        ));
    }

    // Validate the configuration as it will actually be stored, secret
    // included, so a keep-current save of an incomplete row still fails loudly.
    let effective_secret = if secret_provided {
        api_secret.trim().to_string()
    } else if switching_provider {
        String::new()
    } else {
        match existing.as_ref().and_then(|(_, enc)| enc.as_deref()) {
            Some(enc) if !enc.is_empty() => {
                crate::crypto::decrypt_password(&state.secret_key, enc).unwrap_or_default()
            }
            _ => String::new(),
        }
    };

    let candidate = crate::sms::SmsConfig {
        provider: provider.clone(),
        api_key: api_key.clone(),
        api_secret: effective_secret,
        sender: sender.clone(),
        base_url: base_url.clone(),
        default_country_code: default_country_code.clone(),
        daily_cap,
    };
    if let Err(e) = crate::sms::factory::validate_config(&candidate) {
        return redirect_err(&tr1(
            _admin.lang,
            "admin-sms-settings-invalid",
            "error",
            &e.to_string(),
        ));
    }

    // A from-number is a phone number; an alphanumeric sender ID is not, so
    // only numeric senders are held to E.164.
    if sender.starts_with('+') && !crate::sms::phone::is_e164(&sender) {
        return redirect_err(&crate::i18n::translate(
            _admin.lang,
            "admin-sms-sender-format",
            None,
        ));
    }

    let result = match existing {
        // A gateway switch always rewrites the secret column, even when the
        // field was left empty (only the credential-less webhook can get
        // here): carrying the previous gateway's secret over would otherwise
        // sign webhook calls with, say, a Twilio auth token.
        Some((id, _)) if secret_provided || switching_provider => {
            let api_secret_enc = if secret_provided {
                match crate::crypto::encrypt_password(&state.secret_key, api_secret.trim()) {
                    Ok(s) => Some(s),
                    Err(e) => return internal_error_response("encrypt sms secret", &e),
                }
            } else {
                None
            };
            sqlx::query(
                "UPDATE sms_config SET provider = ?, api_key = ?, api_secret_enc = ?, sender = ?, base_url = ?, default_country_code = ?, daily_cap = ?, enabled = ? WHERE id = ?",
            )
            .bind(&provider)
            .bind(&api_key)
            .bind(&api_secret_enc)
            .bind(&sender)
            .bind(&base_url)
            .bind(&default_country_code)
            .bind(daily_cap)
            .bind(enabled)
            .bind(&id)
            .execute(&state.pool)
            .await
        }
        Some((id, _)) => {
            // Keep-current: do not touch api_secret_enc. Only reachable when
            // the gateway is unchanged (or is the credential-less webhook),
            // so the stored secret still belongs to this gateway.
            sqlx::query(
                "UPDATE sms_config SET provider = ?, api_key = ?, sender = ?, base_url = ?, default_country_code = ?, daily_cap = ?, enabled = ? WHERE id = ?",
            )
            .bind(&provider)
            .bind(&api_key)
            .bind(&sender)
            .bind(&base_url)
            .bind(&default_country_code)
            .bind(daily_cap)
            .bind(enabled)
            .bind(&id)
            .execute(&state.pool)
            .await
        }
        None => {
            let api_secret_enc = if secret_provided {
                match crate::crypto::encrypt_password(&state.secret_key, api_secret.trim()) {
                    Ok(s) => Some(s),
                    Err(e) => return internal_error_response("encrypt sms secret", &e),
                }
            } else {
                None
            };
            sqlx::query(
                "INSERT INTO sms_config (id, provider, api_key, api_secret_enc, sender, base_url, default_country_code, daily_cap, enabled) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(uuid::Uuid::new_v4().to_string())
            .bind(&provider)
            .bind(&api_key)
            .bind(&api_secret_enc)
            .bind(&sender)
            .bind(&base_url)
            .bind(&default_country_code)
            .bind(daily_cap)
            .bind(enabled)
            .execute(&state.pool)
            .await
        }
    };

    if let Err(e) = result {
        return internal_error_response("save sms config", &e);
    }

    tracing::info!(admin = %_admin.user.email, provider = %provider, "admin: sms config updated");
    Redirect::to("/dashboard/admin").into_response()
}

#[derive(Deserialize)]
pub(super) struct AdminSmsTestForm {
    _csrf: Option<String>,
    to: Option<String>,
}

/// Send a real test message, or check the credentials for free when the
/// gateway supports it and no recipient was given.
pub(super) async fn admin_update_sms_test(
    State(state): State<Arc<AppState>>,
    admin: crate::auth::AdminUser,
    headers: HeaderMap,
    Form(form): Form<AdminSmsTestForm>,
) -> impl IntoResponse {
    if let Err(resp) = verify_csrf_token(&headers, &form._csrf) {
        return resp;
    }

    let redirect = |outcome: &str, detail: &str| {
        let encoded = urlencoding::encode(detail).into_owned();
        Redirect::to(&format!(
            "/dashboard/admin?sms_test={}&sms_test_detail={}",
            outcome, encoded
        ))
        .into_response()
    };

    let config = match crate::sms::load_config(&state.pool, &state.secret_key).await {
        Ok(Some(c)) => c,
        Ok(None) => {
            return redirect(
                "error",
                &crate::i18n::translate(admin.lang, "admin-sms-not-configured", None),
            )
        }
        Err(e) => {
            tracing::warn!(error = %e, "admin: SMS test could not load config");
            return redirect(
                "error",
                &crate::i18n::translate(admin.lang, "admin-sms-load-failed", None),
            );
        }
    };

    let to = form
        .to
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    let Some(to) = to else {
        // No recipient: verify the credentials instead of sending.
        return match crate::sms::check(&config).await {
            Ok(()) => {
                tracing::info!(admin = %admin.user.email, provider = %config.provider, "admin: SMS credentials verified");
                redirect(
                    "checked",
                    &crate::i18n::translate(admin.lang, "admin-sms-check-accepted", None),
                )
            }
            Err(e) => {
                tracing::warn!(admin = %admin.user.email, provider = %config.provider, error = %e, "admin: SMS credential check failed");
                redirect("error", &e.to_string())
            }
        };
    };

    let to = match crate::sms::phone::normalize(&to, &config.default_country_code) {
        Some(number) => number,
        None => {
            return redirect(
                "error",
                &crate::i18n::translate(admin.lang, "admin-sms-bad-number", None),
            )
        }
    };

    let body = crate::i18n::translate(admin.lang, "admin-sms-test-body", None);
    match crate::sms::send(&config, &to, &body).await {
        Ok(_) => {
            tracing::info!(admin = %admin.user.email, provider = %config.provider, %to, "admin: test SMS sent");
            redirect(
                "sent",
                &crate::i18n::translate(admin.lang, "admin-sms-test-accepted", None),
            )
        }
        Err(e) => {
            tracing::warn!(admin = %admin.user.email, provider = %config.provider, error = %e, "admin: test SMS failed");
            redirect("error", &e.to_string())
        }
    }
}

#[derive(Deserialize)]
pub(super) struct AdminSmsPolicyForm {
    _csrf: Option<String>,
    /// HTML checkbox: present when any user may put an event type into an
    /// SMS-sending mode, absent for admins only.
    allow_all_users: Option<String>,
}

/// Who may enable SMS on an event type.
///
/// Separate from the gateway settings on purpose: it is an instance policy,
/// not a credential, so it stays editable even when the `CALRS_SMS_*` block
/// governs the gateway itself.
pub(super) async fn admin_update_sms_policy(
    State(state): State<Arc<AppState>>,
    _admin: crate::auth::AdminUser,
    headers: HeaderMap,
    Form(form): Form<AdminSmsPolicyForm>,
) -> impl IntoResponse {
    if let Err(resp) = verify_csrf_token(&headers, &form._csrf) {
        return resp;
    }

    let allow_all = form.allow_all_users.is_some();
    if let Err(e) =
        sqlx::query("UPDATE auth_config SET sms_allow_all_users = ? WHERE id = 'singleton'")
            .bind(allow_all)
            .execute(&state.pool)
            .await
    {
        return internal_error_response("save sms policy", &e);
    }

    tracing::info!(admin = %_admin.user.email, allow_all, "admin: sms event-type policy updated");
    Redirect::to("/dashboard/admin").into_response()
}

#[derive(Deserialize)]
pub(super) struct AdminSmsClearForm {
    _csrf: Option<String>,
}

pub(super) async fn admin_update_sms_clear(
    State(state): State<Arc<AppState>>,
    _admin: crate::auth::AdminUser,
    headers: HeaderMap,
    Form(form): Form<AdminSmsClearForm>,
) -> impl IntoResponse {
    if let Err(resp) = verify_csrf_token(&headers, &form._csrf) {
        return resp;
    }

    // Same guard as admin_update_sms: when the env block governs, the DB
    // config is shadowed and the UI hides this button. Refuse the write.
    if crate::sms::sms_env_active() {
        return Redirect::to("/dashboard/admin").into_response();
    }

    if let Err(e) = sqlx::query("DELETE FROM sms_config")
        .execute(&state.pool)
        .await
    {
        return internal_error_response("clear sms config", &e);
    }

    tracing::info!(admin = %_admin.user.email, "admin: sms config cleared");
    Redirect::to("/dashboard/admin").into_response()
}

// --- Auto-generated meeting link settings ---

#[derive(Deserialize)]
pub(super) struct AdminJitsiForm {
    _csrf: Option<String>,
    jitsi_base_url: Option<String>,
    jitsi_pattern: Option<String>,
    jitsi_display_name: Option<String>,
}

pub(super) async fn admin_update_jitsi(
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

    tracing::info!(admin = %_admin.user.email, "admin: jitsi config updated");
    Redirect::to("/dashboard/admin").into_response()
}

#[derive(Deserialize)]
pub(super) struct AdminMeetingWebhookForm {
    _csrf: Option<String>,
    meeting_webhook_url: Option<String>,
    meeting_webhook_auth_mode: Option<String>,
    meeting_webhook_secret: Option<String>,
    meeting_webhook_display_name: Option<String>,
}

pub(super) async fn admin_update_meeting_webhook(
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

    tracing::info!(admin = %_admin.user.email, "admin: meeting webhook config updated");
    Redirect::to("/dashboard/admin").into_response()
}

/// Back to the admin page with the reason an upload was refused, shown under
/// the card it came from. `admin_dashboard` only reads back these fixed
/// values, never free text, so a crafted link cannot put words on the page.
fn admin_upload_refused(reason: &str) -> Response {
    Redirect::to(&format!("/dashboard/admin?upload_error={reason}")).into_response()
}

pub(super) async fn admin_upload_logo(
    State(state): State<Arc<AppState>>,
    admin: crate::auth::AdminUser,
    headers: HeaderMap,
    Query(csrf_query): Query<CsrfQuery>,
    mut multipart: Multipart,
) -> impl IntoResponse {
    if let Err(resp) = verify_csrf_token(&headers, &csrf_query._csrf) {
        return resp;
    }
    // A body over the size limit makes the multipart reader fail, which ends
    // this loop and lands on the refusal below like any other bad upload.
    while let Ok(Some(field)) = multipart.next_field().await {
        if field.name() != Some("logo") {
            continue;
        }
        let Ok(bytes) = field.bytes().await else {
            return admin_upload_refused("logo");
        };
        // Trust magic bytes, not the multipart Content-Type header. SVG is
        // not accepted, so an upload served from this origin can never carry
        // script.
        if bytes.len() > LOGO_MAX_BYTES || detect_image_ext(&bytes).is_none() {
            return admin_upload_refused("logo");
        }
        if let Err(e) = tokio::fs::write(state.data_dir.join("logo.png"), &bytes).await {
            tracing::error!(error = %e, "failed to save the logo");
            return admin_upload_refused("logo-save");
        }
        tracing::info!(admin = %admin.user.email, "admin: logo uploaded");
        return Redirect::to("/dashboard/admin").into_response();
    }
    admin_upload_refused("logo")
}

pub(super) async fn admin_delete_logo(
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

pub(super) async fn admin_upload_favicon(
    State(state): State<Arc<AppState>>,
    admin: crate::auth::AdminUser,
    headers: HeaderMap,
    Query(csrf_query): Query<CsrfQuery>,
    mut multipart: Multipart,
) -> impl IntoResponse {
    if let Err(resp) = verify_csrf_token(&headers, &csrf_query._csrf) {
        return resp;
    }
    while let Ok(Some(field)) = multipart.next_field().await {
        if field.name() != Some("favicon") {
            continue;
        }
        let Ok(bytes) = field.bytes().await else {
            return admin_upload_refused("favicon");
        };
        // Same rule as the logo: trust magic bytes, not the multipart
        // Content-Type, and no SVG.
        if bytes.len() > FAVICON_MAX_BYTES || detect_image_ext(&bytes).is_none() {
            return admin_upload_refused("favicon");
        }
        if let Err(e) = tokio::fs::write(state.data_dir.join(FAVICON_FILE), &bytes).await {
            tracing::error!(error = %e, "failed to save the favicon");
            return admin_upload_refused("favicon-save");
        }
        tracing::info!(admin = %admin.user.email, "admin: favicon uploaded");
        return Redirect::to("/dashboard/admin").into_response();
    }
    admin_upload_refused("favicon")
}

pub(super) async fn admin_delete_favicon(
    State(state): State<Arc<AppState>>,
    _admin: crate::auth::AdminUser,
    headers: HeaderMap,
    Form(csrf): Form<CsrfForm>,
) -> impl IntoResponse {
    if let Err(resp) = verify_csrf_token(&headers, &csrf._csrf) {
        return resp;
    }
    let _ = tokio::fs::remove_file(state.data_dir.join(FAVICON_FILE)).await;
    tracing::info!(admin = %_admin.user.email, "admin: favicon removed");
    Redirect::to("/dashboard/admin").into_response()
}

pub(super) async fn admin_update_company_link(
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
            admin = %_admin.user.email,
            "admin: company_link rejected (only http/https schemes allowed)"
        );
        let msg = urlencoding::encode(&crate::i18n::translate(
            _admin.lang,
            "admin-company-link-invalid",
            None,
        ))
        .into_owned();
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

pub(super) async fn admin_impersonate(
    State(_state): State<Arc<AppState>>,
    _admin: crate::auth::AdminUser,
    headers: HeaderMap,
    Path(user_id): Path<String>,
    Form(csrf): Form<CsrfForm>,
) -> impl IntoResponse {
    if let Err(resp) = verify_csrf_token(&headers, &csrf._csrf) {
        return resp;
    }
    tracing::warn!(admin = %_admin.user.email, target = %user_id, "admin: impersonation started");
    let cookie = format!(
        "__Host-calrs_impersonate={}; HttpOnly; Secure; SameSite=Lax; Path=/; Max-Age={}",
        user_id,
        86400 // 24 hours
    );
    ([("Set-Cookie", cookie)], Redirect::to("/dashboard")).into_response()
}

pub(super) async fn admin_stop_impersonate(
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
pub(super) struct WeightForm {
    _csrf: Option<String>,
    weight: i64,
}

pub(super) async fn admin_update_member_weight(
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
