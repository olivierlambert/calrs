//! Calendar sources: add, edit, test, sync, remove, write-back setup, Google connect.

use super::*;

// --- Dashboard: Sources ---

pub(super) async fn dashboard_sources(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
) -> impl IntoResponse {
    let user = &auth_user.user;

    let sources: Vec<(
        String,
        String,
        String,
        String,
        Option<String>,
        bool,
        Option<String>,
        String,
        String,
        Option<String>,
        Option<String>,
    )> = sqlx::query_as(
        "SELECT cs.id, cs.name, cs.url, cs.username, cs.last_synced, cs.enabled, cs.write_calendar_href, cs.auth_type, cs.provider_type, cs.last_sync_error, cs.sync_failing_since
         FROM caldav_sources cs
         JOIN accounts a ON a.id = cs.account_id
         WHERE a.user_id = ?
         ORDER BY cs.created_at",
    )
    .bind(&user.id)
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();

    // Write-back choices: a calendar the account cannot write to is left out,
    // unless it is already the one configured.
    let all_calendars: Vec<(String, String, Option<String>)> = sqlx::query_as(
        "SELECT c.source_id, c.href, c.display_name
         FROM calendars c
         JOIN caldav_sources cs ON cs.id = c.source_id
         JOIN accounts a ON a.id = cs.account_id
         WHERE a.user_id = ? AND (c.read_only = 0 OR c.href = cs.write_calendar_href)
         ORDER BY c.display_name",
    )
    .bind(&user.id)
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();

    let sources_ctx: Vec<minijinja::Value> = sources
        .iter()
        .map(
            |(
                id,
                name,
                url,
                username,
                last_synced,
                enabled,
                write_cal,
                auth_type,
                provider_type,
                last_sync_error,
                sync_failing_since,
            )| {
                let cals: Vec<minijinja::Value> = all_calendars
                    .iter()
                    .filter(|(sid, _, _)| sid == id)
                    .map(|(_, href, display)| {
                        context! {
                            href => href,
                            name => display.as_deref().unwrap_or(href),
                        }
                    })
                    .collect();
                context! {
                    id => id,
                    id_short => &id[..8.min(id.len())],
                    name => name,
                    url => url,
                    username => username,
                    last_synced => last_synced.as_deref().unwrap_or("never"),
                    // Availability is still served from the last good sync;
                    // this tells the owner it is stale (#162).
                    sync_error => last_sync_error.as_deref().unwrap_or(""),
                    sync_failing_since => sync_failing_since.as_deref().unwrap_or(""),
                    enabled => enabled,
                    write_calendar_href => write_cal.as_deref().unwrap_or(""),
                    auth_type => auth_type,
                    provider_type => provider_type,
                    provider_label => crate::providers::factory::label(provider_type),
                    calendars => cals,
                }
            },
        )
        .collect();

    let tmpl = match state.templates.get_template("dashboard_sources.html") {
        Ok(t) => t,
        Err(e) => return internal_error_html("template render", &e),
    };

    let (impersonating, impersonating_name, _) = impersonation_ctx(&auth_user);

    Html(
        tmpl.render(context! {
            sidebar => sidebar_context(&auth_user, "sources"),
            lang => auth_user.lang,
            sources => sources_ctx,
            impersonating => impersonating,
            impersonating_name => impersonating_name,
        })
        .unwrap_or_else(|e| internal_error_body("template render", &e)),
    )
}

#[derive(Deserialize)]
pub(super) struct SourceForm {
    _csrf: Option<String>,
    provider: Option<String>,
    /// Backend protocol: "caldav" (default) or "ews".
    #[serde(default)]
    provider_type: Option<String>,
    name: String,
    url: String,
    username: String,
    password: String,
    no_test: Option<String>,
}

/// Resolve and validate the provider type from a form input. Defaults to
/// `caldav` when the field is missing (older clients) and rejects unknown
/// values rather than silently coercing.
fn parse_provider_type(raw: Option<&str>) -> Result<String, String> {
    let value = raw
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("caldav");
    match value {
        crate::providers::factory::kinds::CALDAV => Ok("caldav".to_string()),
        crate::providers::factory::kinds::EWS => Ok("ews".to_string()),
        other => Err(format!("Unknown provider type '{}'", other)),
    }
}

/// Preset list shown in the source-add form. Tuple is
/// `(id, display name, default URL, backend)`, where `backend` is `caldav`
/// or `ews`. The template uses the backend tag to keep the Backend dropdown
/// in sync with the preset choice.
fn caldav_providers() -> Vec<(&'static str, &'static str, &'static str, &'static str)> {
    vec![
        (
            "bluemind",
            "BlueMind",
            "https://mail.example.com/dav/",
            "caldav",
        ),
        (
            "nextcloud",
            "Nextcloud",
            "https://cloud.example.com/remote.php/dav",
            "caldav",
        ),
        (
            "fastmail",
            "Fastmail",
            "https://caldav.fastmail.com/dav/calendars/user/you@fastmail.com/",
            "caldav",
        ),
        ("icloud", "iCloud", "https://caldav.icloud.com/", "caldav"),
        (
            "zimbra",
            "Zimbra",
            "https://mail.example.com/dav/",
            "caldav",
        ),
        (
            "sogo",
            "SOGo",
            "https://mail.example.com/SOGo/dav/",
            "caldav",
        ),
        ("radicale", "Radicale", "https://cal.example.com/", "caldav"),
        ("google", "Google Calendar", "", "caldav"),
        (
            "exchange",
            "Microsoft Exchange on-premises (EWS)",
            "https://mail.example.com/EWS/Exchange.asmx",
            "ews",
        ),
        ("other", "Other / Generic CalDAV", "", "caldav"),
    ]
}

pub(super) async fn new_source_form(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
) -> impl IntoResponse {
    let tmpl = match state.templates.get_template("source_form.html") {
        Ok(t) => t,
        Err(e) => return internal_error_html("template render", &e),
    };

    let providers: Vec<minijinja::Value> = caldav_providers()
        .iter()
        .map(|(id, name, url, backend)| context! { id => id, name => name, url => url, backend => backend })
        .collect();

    let (google_configured, microsoft_configured) = oauth_connect_configured(&state).await;

    let (impersonating, impersonating_name, _) = impersonation_ctx(&auth_user);
    Html(
        tmpl.render(context! {
            providers => providers,
            form_provider => "bluemind",
            form_provider_type => "caldav",
            form_name => "",
            form_url => "https://mail.example.com/dav/",
            form_username => "",
            error => "",
            google_oauth2_configured => google_configured,
            microsoft_oauth2_configured => microsoft_configured,
            sidebar => sidebar_context(&auth_user, "sources"),
            lang => auth_user.lang,
            impersonating => impersonating,
            impersonating_name => impersonating_name,
        })
        .unwrap_or_else(|e| internal_error_body("template render", &e)),
    )
}

pub(super) async fn create_source(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
    headers: HeaderMap,
    Form(form): Form<SourceForm>,
) -> impl IntoResponse {
    if let Err(resp) = verify_csrf_token(&headers, &form._csrf) {
        return resp;
    }
    let user = &auth_user.user;

    let account_id: Option<String> =
        sqlx::query_scalar("SELECT id FROM accounts WHERE user_id = ? LIMIT 1")
            .bind(&user.id)
            .fetch_optional(&state.pool)
            .await
            .unwrap_or(None);

    let account_id = match account_id {
        Some(id) => id,
        None => {
            return render_source_form_error(
                &state,
                &auth_user,
                &crate::i18n::translate(auth_user.lang, "form-error-no-account", None),
                &form,
            )
            .await
            .into_response()
        }
    };

    let url = form.url.trim().to_string();
    let username = form.username.trim().to_string();
    let name = form.name.trim().to_string();

    if url.is_empty() || username.is_empty() || name.is_empty() || form.password.is_empty() {
        return render_source_form_error(
            &state,
            &auth_user,
            &crate::i18n::translate(auth_user.lang, "form-error-all-fields-required", None),
            &form,
        )
        .await
        .into_response();
    }

    let provider_type = match parse_provider_type(form.provider_type.as_deref()) {
        Ok(p) => p,
        Err(msg) => {
            return render_source_form_error(&state, &auth_user, &msg, &form)
                .await
                .into_response();
        }
    };

    // Validate URL against SSRF (HTTPS-only, no private targets) for both
    // CalDAV and EWS — the validator is shared.
    if let Err(e) = crate::providers::factory::validate_url(&provider_type, &url) {
        return render_source_form_error(&state, &auth_user, &e.to_string(), &form)
            .await
            .into_response();
    }

    // Test connection unless skip requested
    let skip_test = form.no_test.as_deref() == Some("on");
    if !skip_test {
        let client =
            match crate::providers::build_provider(&provider_type, &url, &username, &form.password)
            {
                Ok(c) => c,
                Err(e) => {
                    return render_source_form_error(&state, &auth_user, &e.to_string(), &form)
                        .await
                        .into_response();
                }
            };
        match client.check_connection().await {
            Ok(_) => {} // fine, even if features not explicitly advertised
            Err(e) => {
                let msg = tr1(
                    auth_user.lang,
                    "form-error-connection-failed",
                    "error",
                    &e.to_string(),
                );
                return render_source_form_error(&state, &auth_user, &msg, &form)
                    .await
                    .into_response();
            }
        }
    }

    let id = uuid::Uuid::new_v4().to_string();
    let password_enc = match crate::crypto::encrypt_password(&state.secret_key, &form.password) {
        Ok(enc) => enc,
        Err(_) => {
            return Html(crate::i18n::translate(
                auth_user.lang,
                "form-error-encryption",
                None,
            ))
            .into_response()
        }
    };

    let _ = sqlx::query(
        "INSERT INTO caldav_sources (id, account_id, name, url, username, password_enc, provider_type) VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(&account_id)
    .bind(&name)
    .bind(&url)
    .bind(&username)
    .bind(&password_enc)
    .bind(&provider_type)
    .execute(&state.pool)
    .await;

    tracing::info!(source_name = %name, provider = %provider_type, user = %auth_user.user.email, "calendar source added");

    // Auto-sync immediately after creating the source, then redirect to
    // write-back setup if calendars were found.
    let (messages, calendar_count) = run_sync(
        &state.pool,
        &state.secret_key,
        &id,
        &provider_type,
        &url,
        &username,
        &form.password,
    )
    .await;

    if calendar_count > 0 {
        let joined_messages = messages.join("\n");
        let encoded_messages = urlencoding::encode(&joined_messages);
        return Redirect::to(&format!(
            "/dashboard/sources/{}/setup-write?sync_messages={}",
            id, encoded_messages
        ))
        .into_response();
    }

    Redirect::to("/dashboard/sources").into_response()
}

/// Whether the Google and Microsoft connect cards can start a flow.
async fn oauth_connect_configured(state: &AppState) -> (bool, bool) {
    let google = sqlx::query_scalar::<_, Option<String>>(
        "SELECT google_oauth2_client_id FROM auth_config LIMIT 1",
    )
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None)
    .flatten()
    .map(|s| !s.is_empty())
    .unwrap_or(false);
    let microsoft = crate::microsoft_oauth::load_config(&state.pool)
        .await
        .is_ok();
    (google, microsoft)
}

async fn render_source_form_error(
    state: &AppState,
    auth_user: &crate::auth::AuthUser,
    error: &str,
    form: &SourceForm,
) -> Html<String> {
    let tmpl = match state.templates.get_template("source_form.html") {
        Ok(t) => t,
        Err(e) => return internal_error_html("template render", &e),
    };

    let providers: Vec<minijinja::Value> = caldav_providers()
        .iter()
        .map(|(id, name, url, backend)| context! { id => id, name => name, url => url, backend => backend })
        .collect();

    let (google_configured, microsoft_configured) = oauth_connect_configured(state).await;
    let (impersonating, impersonating_name, _) = impersonation_ctx(auth_user);
    Html(
        tmpl.render(context! {
            providers => providers,
            form_provider => form.provider.as_deref().unwrap_or("other"),
            form_provider_type => form.provider_type.as_deref().unwrap_or("caldav"),
            form_name => form.name.as_str(),
            form_url => form.url.as_str(),
            form_username => form.username.as_str(),
            error => error,
            google_oauth2_configured => google_configured,
            microsoft_oauth2_configured => microsoft_configured,
            sidebar => sidebar_context(auth_user, "sources"),
            lang => auth_user.lang,
            impersonating => impersonating,
            impersonating_name => impersonating_name,
        })
        .unwrap_or_else(|e| internal_error_body("template render", &e)),
    )
}

/// Render the source form in editing mode with the given values.
fn render_source_edit_form(
    state: &AppState,
    auth_user: &crate::auth::AuthUser,
    source_id: &str,
    name: &str,
    url: &str,
    username: &str,
    error: &str,
) -> Html<String> {
    let tmpl = match state.templates.get_template("source_form.html") {
        Ok(t) => t,
        Err(e) => return Html(format!("Template error: {}", e)),
    };

    let providers: Vec<minijinja::Value> = caldav_providers()
        .iter()
        .map(|(id, name, url, backend)| context! { id => id, name => name, url => url, backend => backend })
        .collect();

    let (impersonating, impersonating_name, _) = impersonation_ctx(auth_user);
    Html(
        tmpl.render(context! {
            editing => true,
            source_id => source_id,
            providers => providers,
            form_provider => "other",
            form_name => name,
            form_url => url,
            form_username => username,
            error => error,
            sidebar => sidebar_context(auth_user, "sources"),
            lang => auth_user.lang,
            impersonating => impersonating,
            impersonating_name => impersonating_name,
        })
        .unwrap_or_else(|e| format!("Template error: {}", e)),
    )
}

pub(super) async fn edit_source_form(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
    Path(source_id): Path<String>,
) -> impl IntoResponse {
    let user = &auth_user.user;
    // Verify ownership and load current values.
    let source: Option<(String, String, String, Option<String>)> = sqlx::query_as(
        "SELECT cs.name, cs.url, cs.username, cs.auth_type
         FROM caldav_sources cs
         JOIN accounts a ON a.id = cs.account_id
         WHERE cs.id = ? AND a.user_id = ?",
    )
    .bind(&source_id)
    .bind(&user.id)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    let (name, url, username, auth_type) = match source {
        Some(s) => s,
        None => return Redirect::to("/dashboard/sources").into_response(),
    };

    // OAuth2 sources can't be edited via the basic-auth form; the only
    // useful "edit" is re-running the consent flow.
    if auth_type.as_deref() == Some("oauth2") {
        return Redirect::to("/dashboard/sources").into_response();
    }

    render_source_edit_form(&state, &auth_user, &source_id, &name, &url, &username, "")
        .into_response()
}

pub(super) async fn update_source(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
    headers: HeaderMap,
    Path(source_id): Path<String>,
    Form(form): Form<SourceForm>,
) -> impl IntoResponse {
    if let Err(resp) = verify_csrf_token(&headers, &form._csrf) {
        return resp;
    }
    let user = &auth_user.user;

    // Confirm the source belongs to this user and grab the existing
    // password (used as fallback when the form leaves it blank).
    let existing: Option<(String, Option<String>)> = sqlx::query_as(
        "SELECT cs.password_enc, cs.auth_type
         FROM caldav_sources cs
         JOIN accounts a ON a.id = cs.account_id
         WHERE cs.id = ? AND a.user_id = ?",
    )
    .bind(&source_id)
    .bind(&user.id)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    let (existing_password_enc, auth_type) = match existing {
        Some((enc, at)) => (enc, at),
        None => return Redirect::to("/dashboard/sources").into_response(),
    };

    // OAuth2 sources don't have a password to decrypt and can't be reached
    // with basic auth — reject the update outright.
    if auth_type.as_deref() == Some("oauth2") {
        return Redirect::to("/dashboard/sources").into_response();
    }

    let url = form.url.trim().to_string();
    let username = form.username.trim().to_string();
    let name = form.name.trim().to_string();

    if url.is_empty() || username.is_empty() || name.is_empty() {
        return render_source_edit_form(
            &state,
            &auth_user,
            &source_id,
            &form.name,
            &form.url,
            &form.username,
            "Name, URL, and username are required.",
        )
        .into_response();
    }

    if let Err(e) = crate::caldav::validate_caldav_url(&url) {
        return render_source_edit_form(
            &state,
            &auth_user,
            &source_id,
            &name,
            &url,
            &username,
            &e.to_string(),
        )
        .into_response();
    }

    // Resolve the password we'll actually use for the test + storage.
    // Empty form password means "keep existing"; we decrypt the stored
    // blob so the connection test exercises the same credentials that
    // sync will use afterwards.
    let password_changed = !form.password.is_empty();
    let plaintext_password = if password_changed {
        form.password.clone()
    } else {
        match crate::crypto::decrypt_password(&state.secret_key, &existing_password_enc) {
            Ok(p) => p,
            Err(_) => {
                return Html(crate::i18n::translate(
                    auth_user.lang,
                    "error-decrypt-failed",
                    None,
                ))
                .into_response()
            }
        }
    };

    let skip_test = form.no_test.as_deref() == Some("on");
    if !skip_test {
        let client = crate::caldav::CaldavClient::new(&url, &username, &plaintext_password);
        if let Err(e) = client.check_connection().await {
            let msg = format!("Connection failed: {}. Check the URL and credentials, or check \"Skip connection test\" to save anyway.", e);
            return render_source_edit_form(
                &state, &auth_user, &source_id, &name, &url, &username, &msg,
            )
            .into_response();
        }
    }

    if password_changed {
        let new_enc = match crate::crypto::encrypt_password(&state.secret_key, &plaintext_password)
        {
            Ok(enc) => enc,
            Err(_) => {
                return Html(crate::i18n::translate(
                    auth_user.lang,
                    "form-error-encryption",
                    None,
                ))
                .into_response()
            }
        };
        let _ = sqlx::query(
            "UPDATE caldav_sources SET name = ?, url = ?, username = ?, password_enc = ? WHERE id = ?",
        )
        .bind(&name)
        .bind(&url)
        .bind(&username)
        .bind(&new_enc)
        .bind(&source_id)
        .execute(&state.pool)
        .await;
    } else {
        let _ =
            sqlx::query("UPDATE caldav_sources SET name = ?, url = ?, username = ? WHERE id = ?")
                .bind(&name)
                .bind(&url)
                .bind(&username)
                .bind(&source_id)
                .execute(&state.pool)
                .await;
    }

    tracing::info!(
        source_id = %source_id,
        source_name = %name,
        password_changed = %password_changed,
        user = %auth_user.user.email,
        "CalDAV source updated"
    );

    Redirect::to("/dashboard/sources").into_response()
}

pub(super) async fn remove_source(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
    headers: HeaderMap,
    Path(source_id): Path<String>,
    Form(csrf): Form<CsrfForm>,
) -> impl IntoResponse {
    if let Err(resp) = verify_csrf_token(&headers, &csrf._csrf) {
        return resp;
    }
    let user = &auth_user.user;

    // Verify source belongs to this user before deleting
    let _ = sqlx::query(
        "DELETE FROM caldav_sources WHERE id = ? AND account_id IN (SELECT id FROM accounts WHERE user_id = ?)",
    )
    .bind(&source_id)
    .bind(&user.id)
    .execute(&state.pool)
    .await;

    tracing::info!(source_id = %source_id, user = %auth_user.user.email, "CalDAV source removed");

    Redirect::to("/dashboard/sources").into_response()
}

pub(super) async fn test_source(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
    headers: HeaderMap,
    Path(source_id): Path<String>,
    Form(csrf): Form<CsrfForm>,
) -> impl IntoResponse {
    if let Err(resp) = verify_csrf_token(&headers, &csrf._csrf) {
        return resp;
    }
    let user = &auth_user.user;

    let source: Option<(
        String,
        String,
        Option<String>,
        String,
        String,
        Option<String>,
        Option<String>,
        String,
    )> = sqlx::query_as(
        "SELECT cs.url, cs.username, cs.password_enc, cs.name, cs.auth_type, cs.access_token_enc, cs.token_expires_at, cs.provider_type
         FROM caldav_sources cs
         JOIN accounts a ON a.id = cs.account_id
         WHERE cs.id = ? AND a.user_id = ?",
    )
    .bind(&source_id)
    .bind(&user.id)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    let (
        url,
        username,
        password_enc,
        name,
        auth_type,
        access_token_enc,
        token_expires_at,
        provider_type,
    ) = match source {
        Some(s) => s,
        None => {
            return Html(crate::i18n::translate(
                auth_user.lang,
                "error-source-not-found",
                None,
            ))
            .into_response()
        }
    };

    let label = crate::providers::factory::label(&provider_type);

    // EWS sources go through the provider trait; CalDAV (basic or OAuth2) keeps
    // the existing CaldavClient path so OAuth2 refresh + ctag stay intact.
    let result = if provider_type == crate::providers::factory::kinds::MICROSOFT {
        match crate::microsoft_oauth::build_provider_for_source(
            &state.pool,
            &state.secret_key,
            &source_id,
        )
        .await
        {
            Ok(client) => match client.check_connection().await {
                Ok(true) => {
                    crate::i18n::translate(auth_user.lang, "source-test-microsoft-ok", None)
                }
                _ => crate::i18n::translate(auth_user.lang, "source-test-microsoft-failed", None),
            },
            Err(_) => {
                crate::i18n::translate(auth_user.lang, "source-test-microsoft-credentials", None)
            }
        }
    } else if provider_type == crate::providers::factory::kinds::EWS {
        let password = match password_enc.as_deref() {
            Some(enc) => match crate::crypto::decrypt_password(&state.secret_key, enc) {
                Ok(p) => p,
                Err(_) => {
                    return Html(crate::i18n::translate(
                        auth_user.lang,
                        "error-decrypt-failed",
                        None,
                    ))
                    .into_response()
                }
            },
            None => {
                return Html(crate::i18n::translate(
                    auth_user.lang,
                    "error-source-no-password",
                    None,
                ))
                .into_response();
            }
        };
        match crate::providers::build_provider(&provider_type, &url, &username, &password) {
            Ok(client) => match client.check_connection().await {
                Ok(true) => format!("'{}' — connection OK ({}).", name, label),
                Ok(false) => format!(
                    "'{}' — connected but {} features not explicitly advertised. Sync may still work.",
                    name, label,
                ),
                Err(e) => format!("'{}' — connection failed: {}", name, e),
            },
            Err(e) => format!("'{}' — could not build provider: {}", name, e),
        }
    } else {
        let client = match crate::oauth2_caldav::build_client_for_source(
            &state.pool,
            &state.secret_key,
            &source_id,
            &url,
            &auth_type,
            &username,
            password_enc.as_deref(),
            access_token_enc.as_deref(),
            token_expires_at.as_deref(),
        )
        .await
        {
            Ok(c) => c,
            Err(_) => {
                return Html(crate::i18n::translate(
                    auth_user.lang,
                    "error-decrypt-failed",
                    None,
                ))
                .into_response()
            }
        };
        match client.check_connection().await {
            Ok(true) => format!("'{}' — connection OK, CalDAV supported.", name),
            Ok(false) => format!(
                "'{}' — connected but CalDAV not explicitly detected. Sync may still work.",
                name
            ),
            Err(e) => format!("'{}' — connection failed: {}", name, e),
        }
    };

    // Return a simple page with back link
    let tmpl = match state.templates.get_template("source_test.html") {
        Ok(t) => t,
        Err(_) => {
            return Html(format!(
                "<p>{}</p><p><a href=\"/dashboard\">Back to dashboard</a></p>",
                result
            ))
            .into_response()
        }
    };
    let (impersonating, impersonating_name, _) = impersonation_ctx(&auth_user);
    Html(
        tmpl.render(context! {
            result => result,
            sidebar => sidebar_context(&auth_user, "sources"),
            lang => auth_user.lang,
            impersonating => impersonating,
            impersonating_name => impersonating_name,
        })
        .unwrap_or_else(|e| internal_error_body("template render", &e)),
    )
    .into_response()
}

/// Runs CalDAV sync using build_client_for_source (supports both basic and OAuth2).
async fn run_sync_for_source(
    pool: &SqlitePool,
    key: &[u8; 32],
    source_id: &str,
    url: &str,
    username: &str,
    password_enc: Option<&str>,
    auth_type: &str,
    access_token_enc: Option<&str>,
    token_expires_at: Option<&str>,
    provider_type: &str,
) -> (Vec<String>, usize) {
    // Same dispatch as the background and CLI syncs, so a failure here
    // (including credentials that no longer work) is recorded on the source
    // too. A calendar that failed to fetch fails the sync, so "Sync
    // complete." is only shown when every calendar synced (#162).
    match crate::commands::sync::sync_one_source(
        pool,
        key,
        source_id,
        url,
        username,
        password_enc,
        auth_type,
        access_token_enc,
        token_expires_at,
        provider_type,
    )
    .await
    {
        Ok(()) => {
            let cal_count: i64 =
                sqlx::query_scalar("SELECT COUNT(*) FROM calendars WHERE source_id = ?")
                    .bind(source_id)
                    .fetch_one(pool)
                    .await
                    .unwrap_or(0);
            (vec!["Sync complete.".to_string()], cal_count as usize)
        }
        Err(e) => (vec![format!("Sync failed: {}", e)], 0),
    }
}

/// Run discovery + sync for a freshly-created source with plaintext password.
/// Dispatches on `provider_type`: EWS goes through the trait-based path,
/// CalDAV reuses the existing `CaldavClient` + `sync_source` flow.
async fn run_sync(
    pool: &SqlitePool,
    key: &[u8; 32],
    source_id: &str,
    provider_type: &str,
    url: &str,
    username: &str,
    password: &str,
) -> (Vec<String>, usize) {
    if provider_type == crate::providers::factory::kinds::EWS {
        let provider =
            match crate::providers::build_provider(provider_type, url, username, password) {
                Ok(p) => p,
                Err(e) => return (vec![format!("Could not build provider: {}", e)], 0),
            };
        match crate::commands::sync::sync_provider_source(pool, key, provider.as_ref(), source_id)
            .await
        {
            Ok(()) => {
                let cal_count: i64 =
                    sqlx::query_scalar("SELECT COUNT(*) FROM calendars WHERE source_id = ?")
                        .bind(source_id)
                        .fetch_one(pool)
                        .await
                        .unwrap_or(0);
                (vec!["Sync complete.".to_string()], cal_count as usize)
            }
            Err(e) => (vec![format!("Sync failed: {}", e)], 0),
        }
    } else {
        let client = crate::caldav::CaldavClient::new(url, username, password);
        match crate::commands::sync::sync_source(pool, key, &client, source_id).await {
            Ok(()) => {
                let cal_count: i64 =
                    sqlx::query_scalar("SELECT COUNT(*) FROM calendars WHERE source_id = ?")
                        .bind(source_id)
                        .fetch_one(pool)
                        .await
                        .unwrap_or(0);
                (vec!["Sync complete.".to_string()], cal_count as usize)
            }
            Err(e) => (vec![format!("Sync failed: {}", e)], 0),
        }
    }
}

pub(super) async fn force_sync_source(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
    headers: HeaderMap,
    Path(source_id): Path<String>,
    Form(csrf): Form<CsrfForm>,
) -> impl IntoResponse {
    if let Err(resp) = verify_csrf_token(&headers, &csrf._csrf) {
        return resp;
    }
    let user = &auth_user.user;

    // Verify ownership
    let source: Option<(
        String,
        String,
        String,
        Option<String>,
        String,
        Option<String>,
        Option<String>,
        String,
    )> = sqlx::query_as(
        "SELECT cs.id, cs.url, cs.username, cs.password_enc, cs.auth_type, cs.access_token_enc, cs.token_expires_at, cs.provider_type
         FROM caldav_sources cs JOIN accounts a ON a.id = cs.account_id
         WHERE cs.id = ? AND a.user_id = ?",
    )
    .bind(&source_id)
    .bind(&user.id)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    let (
        sid,
        url,
        username,
        password_enc,
        auth_type,
        access_token_enc,
        token_expires_at,
        provider_type,
    ) = match source {
        Some(s) => s,
        None => {
            return Html(crate::i18n::translate(
                auth_user.lang,
                "error-source-not-found",
                None,
            ))
            .into_response()
        }
    };

    // Clear sync tokens to force a full fetch (same as `calrs sync --full`)
    let _ = sqlx::query("UPDATE calendars SET sync_token = NULL, ctag = NULL WHERE source_id = ?")
        .bind(&sid)
        .execute(&state.pool)
        .await;

    tracing::info!(source_id = %sid, "force full resync triggered from dashboard");

    let name: String = sqlx::query_scalar("SELECT name FROM caldav_sources WHERE id = ?")
        .bind(&sid)
        .fetch_one(&state.pool)
        .await
        .unwrap_or_else(|_| "Source".to_string());

    let (messages, _) = run_sync_for_source(
        &state.pool,
        &state.secret_key,
        &sid,
        &url,
        &username,
        password_enc.as_deref(),
        &auth_type,
        access_token_enc.as_deref(),
        token_expires_at.as_deref(),
        &provider_type,
    )
    .await;

    render_sync_result(&state, &auth_user, &name, &messages).into_response()
}

pub(super) async fn sync_source(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
    headers: HeaderMap,
    Path(source_id): Path<String>,
    Form(csrf): Form<CsrfForm>,
) -> impl IntoResponse {
    if let Err(resp) = verify_csrf_token(&headers, &csrf._csrf) {
        return resp;
    }
    let user = &auth_user.user;

    let source: Option<(
        String,
        String,
        String,
        Option<String>,
        String,
        String,
        Option<String>,
        Option<String>,
        String,
    )> = sqlx::query_as(
        "SELECT cs.id, cs.url, cs.username, cs.password_enc, cs.name, cs.auth_type, cs.access_token_enc, cs.token_expires_at, cs.provider_type
         FROM caldav_sources cs
         JOIN accounts a ON a.id = cs.account_id
         WHERE cs.id = ? AND a.user_id = ?",
    )
    .bind(&source_id)
    .bind(&user.id)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    let (
        sid,
        url,
        username,
        password_enc,
        name,
        auth_type,
        access_token_enc,
        token_expires_at,
        provider_type,
    ) = match source {
        Some(s) => s,
        None => {
            return Html(crate::i18n::translate(
                auth_user.lang,
                "error-source-not-found",
                None,
            ))
            .into_response()
        }
    };

    tracing::info!(source_id = %sid, "calendar sync triggered from dashboard");

    let (messages, calendar_count) = run_sync_for_source(
        &state.pool,
        &state.secret_key,
        &sid,
        &url,
        &username,
        password_enc.as_deref(),
        &auth_type,
        access_token_enc.as_deref(),
        token_expires_at.as_deref(),
        &provider_type,
    )
    .await;

    // If write_calendar_href is not yet configured and we found calendars,
    // redirect to the write-calendar setup page (onboarding flow).
    let write_href: Option<String> =
        sqlx::query_scalar("SELECT write_calendar_href FROM caldav_sources WHERE id = ?")
            .bind(&sid)
            .fetch_optional(&state.pool)
            .await
            .unwrap_or(None)
            .flatten();

    if write_href.is_none() && calendar_count > 0 {
        let joined_messages = messages.join("\n");
        let encoded_messages = urlencoding::encode(&joined_messages);
        return Redirect::to(&format!(
            "/dashboard/sources/{}/setup-write?sync_messages={}",
            sid, encoded_messages
        ))
        .into_response();
    }

    render_sync_result(&state, &auth_user, &name, &messages).into_response()
}

fn render_sync_result(
    state: &AppState,
    auth_user: &crate::auth::AuthUser,
    source_name: &str,
    messages: &[String],
) -> Html<String> {
    let tmpl = match state.templates.get_template("source_test.html") {
        Ok(t) => t,
        Err(_) => {
            return Html(format!(
                "<p>{}</p><p><a href=\"/dashboard\">Back to dashboard</a></p>",
                messages.join("<br>")
            ))
        }
    };
    let (impersonating, impersonating_name, _) = impersonation_ctx(auth_user);
    Html(
        tmpl.render(context! {
            result => messages.join("\n"),
            source_name => source_name,
            sidebar => sidebar_context(auth_user, "sources"),
            lang => auth_user.lang,
            impersonating => impersonating,
            impersonating_name => impersonating_name,
        })
        .unwrap_or_else(|e| internal_error_body("template render", &e)),
    )
}

#[derive(Deserialize)]
pub(super) struct SetupWriteQuery {
    sync_messages: Option<String>,
}

pub(super) async fn setup_write_calendar(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
    Path(source_id): Path<String>,
    Query(query): Query<SetupWriteQuery>,
) -> impl IntoResponse {
    let user = &auth_user.user;

    // Fetch source name and verify ownership
    let source: Option<(String, String)> = sqlx::query_as(
        "SELECT cs.id, cs.name FROM caldav_sources cs
         JOIN accounts a ON a.id = cs.account_id
         WHERE cs.id = ? AND a.user_id = ?",
    )
    .bind(&source_id)
    .bind(&user.id)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    let (_sid, source_name) = match source {
        Some(s) => s,
        None => return Redirect::to("/dashboard/sources").into_response(),
    };

    // Get calendars for this source, sorted by event count (most events first)
    let calendars: Vec<(String, Option<String>, Option<String>, i64)> = sqlx::query_as(
        "SELECT c.href, c.display_name, c.color,
                (SELECT COUNT(*) FROM events e WHERE e.calendar_id = c.id) as event_count
         FROM calendars c WHERE c.source_id = ? AND c.read_only = 0
         ORDER BY event_count DESC, c.display_name",
    )
    .bind(&source_id)
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();

    if calendars.is_empty() {
        return Redirect::to("/dashboard/sources").into_response();
    }

    let cal_values: Vec<minijinja::Value> = calendars
        .iter()
        .map(|(href, name, color, event_count)| {
            context! {
                href => href,
                name => name.as_deref().unwrap_or(href),
                color => color,
                event_count => event_count,
            }
        })
        .collect();

    let tmpl = match state.templates.get_template("source_write_setup.html") {
        Ok(t) => t,
        Err(e) => return internal_error_response("template render", &e),
    };

    Html(
        tmpl.render(context! {
            lang => auth_user.lang,
            source_id => source_id,
            source_name => source_name,
            calendars => cal_values,
            sync_messages => query.sync_messages,
        })
        .unwrap_or_else(|e| internal_error_body("template render", &e)),
    )
    .into_response()
}

#[derive(Deserialize)]
pub(super) struct WriteCalendarForm {
    _csrf: Option<String>,
    calendar_href: String,
}

pub(super) async fn set_write_calendar(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
    headers: HeaderMap,
    Path(source_id): Path<String>,
    Form(form): Form<WriteCalendarForm>,
) -> impl IntoResponse {
    if let Err(resp) = verify_csrf_token(&headers, &form._csrf) {
        return resp;
    }
    let user = &auth_user.user;

    // Verify source belongs to this user
    let owned: Option<(String,)> = sqlx::query_as(
        "SELECT cs.id FROM caldav_sources cs
         JOIN accounts a ON a.id = cs.account_id
         WHERE cs.id = ? AND a.user_id = ?",
    )
    .bind(&source_id)
    .bind(&user.id)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    if owned.is_none() {
        return Redirect::to("/dashboard/sources").into_response();
    }

    let href = if form.calendar_href.is_empty() {
        None
    } else {
        Some(form.calendar_href)
    };
    let read_only: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM calendars WHERE source_id = ? AND href = ? AND read_only = 1)",
    )
    .bind(&source_id)
    .bind(&href)
    .fetch_one(&state.pool)
    .await
    .unwrap_or(false);
    if read_only {
        return Redirect::to("/dashboard/sources").into_response();
    }

    let _ = sqlx::query("UPDATE caldav_sources SET write_calendar_href = ? WHERE id = ?")
        .bind(&href)
        .bind(&source_id)
        .execute(&state.pool)
        .await;

    tracing::info!(source_id = %source_id, "write calendar configured");

    Redirect::to("/dashboard/sources").into_response()
}

// --- Google Calendar OAuth2 connect flow ---

pub(super) async fn google_connect(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
) -> impl IntoResponse {
    // Load Google OAuth2 client credentials
    let creds: Option<(Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT google_oauth2_client_id, google_oauth2_client_secret FROM auth_config LIMIT 1",
    )
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    let client_id = match creds.and_then(|(id, _)| id).filter(|s| !s.is_empty()) {
        Some(id) => id,
        None => {
            return Html(crate::i18n::translate(
                auth_user.lang,
                "google-error-not-configured",
                None,
            ))
            .into_response();
        }
    };

    let base_url = crate::settings::base_url().unwrap_or_default();
    if base_url.is_empty() {
        return Html(crate::i18n::translate(
            auth_user.lang,
            "google-error-no-base-url",
            None,
        ))
        .into_response();
    }

    let redirect_uri = format!(
        "{}/dashboard/sources/google/callback",
        base_url.trim_end_matches('/')
    );
    let csrf_state = uuid::Uuid::new_v4().to_string();

    let auth_url =
        crate::oauth2_caldav::build_google_auth_url(&client_id, &redirect_uri, &csrf_state);

    // Store state in cookies via Set-Cookie headers (CookieJar's String→Cookie
    // conversion treats the whole formatted string as the cookie name and drops
    // attributes, so set the headers directly).
    let cookie_opts = "; HttpOnly; Secure; SameSite=Lax; Path=/; Max-Age=600";
    let state_cookie = format!("__Host-calrs_google_state={}{}", csrf_state, cookie_opts);
    let user_cookie = format!(
        "__Host-calrs_google_user={}{}",
        auth_user.user.id, cookie_opts
    );

    let mut headers = axum::http::HeaderMap::new();
    headers.append(
        axum::http::header::SET_COOKIE,
        state_cookie.parse().unwrap(),
    );
    headers.append(axum::http::header::SET_COOKIE, user_cookie.parse().unwrap());

    (headers, Redirect::to(&auth_url)).into_response()
}

#[derive(Deserialize)]
pub(super) struct GoogleCallbackQuery {
    code: Option<String>,
    state: Option<String>,
    error: Option<String>,
}

pub(super) async fn google_callback(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
    jar: axum_extra::extract::CookieJar,
    Query(query): Query<GoogleCallbackQuery>,
) -> impl IntoResponse {
    // Check for errors from Google
    if let Some(error) = &query.error {
        // `error` comes from the query string: anyone can put markup in it.
        return Html(tr1(
            auth_user.lang,
            "google-error-authorization",
            "error",
            &crate::email::h(error),
        ))
        .into_response();
    }

    // Verify CSRF state
    let stored_state = jar
        .get("__Host-calrs_google_state")
        .map(|c| c.value().to_string())
        .unwrap_or_default();
    let query_state = query.state.unwrap_or_default();
    if stored_state.is_empty() || stored_state != query_state {
        return Html(crate::i18n::translate(
            auth_user.lang,
            "error-oauth-invalid-state",
            None,
        ))
        .into_response();
    }

    // Cross-tab defense-in-depth: the user cookie set at /connect must match
    // the current session. Guards against a tab swap where the OAuth flow was
    // started as user A but the callback fires while user B is logged in.
    let stored_user = jar
        .get("__Host-calrs_google_user")
        .map(|c| c.value().to_string())
        .unwrap_or_default();
    if stored_user.is_empty() || stored_user != auth_user.user.id {
        tracing::warn!(
            session_user = %auth_user.user.id,
            cookie_user = %stored_user,
            "Google OAuth2 callback: user cookie missing or does not match session"
        );
        return Html(
            "Session changed during Google authorization. Please start the connect flow again."
                .to_string(),
        )
        .into_response();
    }

    let code = match query.code {
        Some(c) => c,
        None => {
            return Html(crate::i18n::translate(
                auth_user.lang,
                "error-oauth-no-code",
                None,
            ))
            .into_response()
        }
    };

    // Load Google OAuth2 credentials
    let creds: Option<(String, String)> = sqlx::query_as(
        "SELECT google_oauth2_client_id, google_oauth2_client_secret FROM auth_config LIMIT 1",
    )
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    let (client_id, client_secret_enc) = match creds {
        Some(c) => c,
        None => {
            return Html(crate::i18n::translate(
                auth_user.lang,
                "error-oauth-not-configured",
                None,
            ))
            .into_response()
        }
    };

    // The stored Google OAuth2 client secret is encrypted at rest; the
    // startup migration ensures any pre-existing plaintext is upgraded
    // before we reach this path.
    let client_secret = match crate::crypto::decrypt_value(&state.secret_key, &client_secret_enc) {
        Ok(s) => s,
        Err(e) => return internal_error_response("decrypt google_oauth2 client_secret", &e),
    };

    let base_url = crate::settings::base_url().unwrap_or_default();
    let redirect_uri = format!(
        "{}/dashboard/sources/google/callback",
        base_url.trim_end_matches('/')
    );

    // Exchange code for tokens
    let (access_token, refresh_token, expires_in) =
        match crate::oauth2_caldav::exchange_google_code(
            &client_id,
            &client_secret,
            &code,
            &redirect_uri,
        )
        .await
        {
            Ok(t) => t,
            Err(e) => {
                return Html(tr1(
                    auth_user.lang,
                    "google-error-token-exchange",
                    "error",
                    &crate::email::h(&e.to_string()),
                ))
                .into_response()
            }
        };

    // Identify the Google account that authorized us; its email is what goes in the CalDAV URL.
    // The local calrs user's email is unrelated and may not match the Google account.
    let google_email = match crate::oauth2_caldav::fetch_google_email(&access_token).await {
        Ok(e) => e,
        Err(e) => {
            return Html(tr1(
                auth_user.lang,
                "google-error-account-email",
                "error",
                &crate::email::h(&e.to_string()),
            ))
            .into_response();
        }
    };
    let caldav_url = crate::oauth2_caldav::google_caldav_url_for_email(&google_email);

    // Test the CalDAV connection (PROPFIND requires the per-user URL)
    let client = crate::caldav::CaldavClient::with_bearer(&caldav_url, &access_token);
    if let Err(e) = client.check_connection().await {
        tracing::warn!(error = %e, "Google CalDAV connection test failed (non-fatal)");
    }

    // Get user's account
    let account: Option<(String,)> =
        sqlx::query_as("SELECT a.id FROM accounts a WHERE a.user_id = ? LIMIT 1")
            .bind(&auth_user.user.id)
            .fetch_optional(&state.pool)
            .await
            .unwrap_or(None);

    let account_id = match account {
        Some((id,)) => id,
        None => {
            return Html(crate::i18n::translate(
                auth_user.lang,
                "error-no-scheduling-account",
                None,
            ))
            .into_response()
        }
    };

    // Encrypt tokens
    let access_token_enc = match crate::crypto::encrypt_password(&state.secret_key, &access_token) {
        Ok(enc) => enc,
        Err(_) => {
            return Html(crate::i18n::translate(
                auth_user.lang,
                "form-error-encryption",
                None,
            ))
            .into_response()
        }
    };
    let refresh_token_enc = match crate::crypto::encrypt_password(&state.secret_key, &refresh_token)
    {
        Ok(enc) => enc,
        Err(_) => {
            return Html(crate::i18n::translate(
                auth_user.lang,
                "form-error-encryption",
                None,
            ))
            .into_response()
        }
    };
    let expires_at = chrono::Utc::now() + chrono::Duration::seconds(expires_in);

    // Reconnecting to a Google account that is already linked must refresh the
    // existing source's tokens, not create a duplicate. A Google CalDAV source
    // is uniquely identified by (account, google account email) — encoded in the
    // per-user caldav_url — so match on that before deciding insert vs. update.
    let existing_source: Option<(String,)> = sqlx::query_as(
        "SELECT id FROM caldav_sources WHERE account_id = ? AND auth_type = 'oauth2' AND oauth2_provider = 'google' AND url = ? LIMIT 1",
    )
    .bind(&account_id)
    .bind(&caldav_url)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    let source_id = match existing_source {
        Some((id,)) => {
            // Reconnect: refresh the stored tokens on the existing source.
            let _ = sqlx::query(
                "UPDATE caldav_sources SET access_token_enc = ?, refresh_token_enc = ?, token_expires_at = ? WHERE id = ?",
            )
            .bind(&access_token_enc)
            .bind(&refresh_token_enc)
            .bind(expires_at.to_rfc3339())
            .bind(&id)
            .execute(&state.pool)
            .await;
            tracing::info!(user = %auth_user.user.email, google_email = %google_email, "Google Calendar source reconnected via OAuth2");
            id
        }
        None => {
            // First-time connect: create the CalDAV source.
            let source_id = uuid::Uuid::new_v4().to_string();
            let _ = sqlx::query(
                "INSERT INTO caldav_sources (id, account_id, name, url, username, auth_type, oauth2_provider, access_token_enc, refresh_token_enc, token_expires_at)
                 VALUES (?, ?, 'Google Calendar', ?, ?, 'oauth2', 'google', ?, ?, ?)",
            )
            .bind(&source_id)
            .bind(&account_id)
            .bind(&caldav_url)
            .bind(&google_email)
            .bind(&access_token_enc)
            .bind(&refresh_token_enc)
            .bind(expires_at.to_rfc3339())
            .execute(&state.pool)
            .await;
            tracing::info!(user = %auth_user.user.email, google_email = %google_email, "Google Calendar source added via OAuth2");
            source_id
        }
    };

    // Auto-sync
    let (_, calendar_count) = run_sync_for_source(
        &state.pool,
        &state.secret_key,
        &source_id,
        &caldav_url,
        &google_email,
        None,
        "oauth2",
        Some(&access_token_enc),
        Some(&expires_at.to_rfc3339()),
        crate::providers::factory::kinds::CALDAV,
    )
    .await;

    // Clear transient cookies. The browser only honors removal of
    // __Host-prefixed cookies when the Set-Cookie also has Secure and Path=/,
    // so emit explicit expiring headers (matches admin_stop_impersonate).
    let clear_opts = "; HttpOnly; Secure; SameSite=Lax; Path=/; Max-Age=0";
    let mut headers = axum::http::HeaderMap::new();
    headers.append(
        axum::http::header::SET_COOKIE,
        format!("__Host-calrs_google_state={}", clear_opts)
            .parse()
            .unwrap(),
    );
    headers.append(
        axum::http::header::SET_COOKIE,
        format!("__Host-calrs_google_user={}", clear_opts)
            .parse()
            .unwrap(),
    );

    let redirect = if calendar_count > 0 {
        Redirect::to(&format!("/dashboard/sources/{}/setup-write", source_id))
    } else {
        Redirect::to("/dashboard/sources")
    };
    (headers, redirect).into_response()
}
