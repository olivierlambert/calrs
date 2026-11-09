//! Microsoft consent is separate from the user's Cal.rs sign-in identity.
use super::*;

#[derive(Deserialize)]
pub(super) struct ConnectForm {
    _csrf: Option<String>,
    source_id: Option<String>,
}

/// Where a failed step sends the user back to.
#[derive(Clone, Copy)]
enum Back {
    Sources,
    Admin,
}

/// The error page for a failed connection step, in the user's language, with
/// a link back. Only fixed, translated messages: no provider-controlled text,
/// token or error detail reaches the page.
fn microsoft_failure(state: &AppState, lang: &str, message_key: &str, back: Back) -> Response {
    use crate::i18n::translate;
    let (heading, href, label) = match back {
        Back::Sources => (
            translate(lang, "microsoft-error-heading", None),
            "/dashboard/sources",
            translate(lang, "source-calendars-back", None),
        ),
        Back::Admin => (
            translate(lang, "microsoft-error-settings-heading", None),
            "/dashboard/admin",
            translate(lang, "microsoft-error-back-admin", None),
        ),
    };
    let tmpl = match state.templates.get_template("booking_action_error.html") {
        Ok(t) => t,
        Err(e) => return internal_error_response("internal", &e),
    };
    let body = tmpl
        .render(context! {
            page_title => heading,
            title => heading,
            message => translate(lang, message_key, None),
            back_href => href,
            back_label => label,
            lang => lang,
        })
        .unwrap_or_else(|e| internal_error_body("template render", &e));
    (
        axum::http::StatusCode::BAD_REQUEST,
        [
            ("Cache-Control", "no-store"),
            ("Referrer-Policy", "no-referrer"),
        ],
        Html(body),
    )
        .into_response()
}

pub(super) async fn microsoft_connect(
    State(state): State<Arc<AppState>>,
    auth: crate::auth::AuthUser,
    headers: HeaderMap,
    Form(form): Form<ConnectForm>,
) -> Response {
    if let Err(response) = verify_csrf_token(&headers, &form._csrf) {
        return response;
    }
    if auth.impersonation.is_some() {
        return microsoft_failure(
            &state,
            auth.lang,
            "microsoft-error-impersonation",
            Back::Sources,
        );
    }
    match crate::microsoft_oauth::start_authorization(
        &state.pool,
        &state.secret_key,
        &auth.user.id,
        form.source_id.as_deref(),
        &crate::settings::base_url().unwrap_or_default(),
    )
    .await
    {
        Ok(url) => (
            [
                ("Cache-Control", "no-store"),
                ("Referrer-Policy", "no-referrer"),
            ],
            Redirect::to(&url),
        )
            .into_response(),
        Err(error) => {
            tracing::warn!(user_id = %auth.user.id, error = %format!("{error:#}"), "could not start Microsoft authorization");
            microsoft_failure(&state, auth.lang, "microsoft-error-start", Back::Sources)
        }
    }
}

#[derive(Deserialize)]
pub(super) struct CallbackQuery {
    state: Option<String>,
    code: Option<String>,
    error: Option<String>,
}

pub(super) async fn microsoft_callback(
    State(state): State<Arc<AppState>>,
    auth: crate::auth::AuthUser,
    Query(query): Query<CallbackQuery>,
) -> Response {
    if auth.impersonation.is_some() {
        return microsoft_failure(
            &state,
            auth.lang,
            "microsoft-error-impersonation",
            Back::Sources,
        );
    }
    let pending = match crate::microsoft_oauth::consume_state(
        &state.pool,
        &auth.user.id,
        query.state.as_deref().unwrap_or_default(),
    )
    .await
    {
        Ok(pending) => pending,
        Err(error) => {
            tracing::warn!(user_id = %auth.user.id, error = %format!("{error:#}"), "Microsoft authorization callback refused");
            return microsoft_failure(&state, auth.lang, "microsoft-error-state", Back::Sources);
        }
    };
    // Consume state even for denial or missing code; it must never be replayable.
    if let Some(error) = &query.error {
        // The OAuth error code (RFC 6749 4.1.2.1), logged only when it looks
        // like one: the query string is the visitor's to write.
        let code = Some(error.as_str())
            .filter(|e| e.len() <= 64 && e.bytes().all(|b| b.is_ascii_lowercase() || b == b'_'))
            .unwrap_or("unrecognised");
        tracing::warn!(user_id = %auth.user.id, oauth_error = code, "Microsoft authorization not granted");
        return microsoft_failure(&state, auth.lang, "microsoft-error-consent", Back::Sources);
    }
    let Some(code) = query.code.filter(|code| !code.is_empty()) else {
        tracing::warn!(user_id = %auth.user.id, "Microsoft authorization callback without a code");
        return microsoft_failure(&state, auth.lang, "microsoft-error-consent", Back::Sources);
    };
    let source_id = match crate::microsoft_oauth::finish_authorization(
        &state.pool,
        &state.secret_key,
        pending,
        &code,
    )
    .await
    {
        Ok(source_id) => source_id,
        Err(error) => {
            // TokenRefused::AppCredentials here is an expired or wrong
            // client secret: the operator has to see it.
            tracing::warn!(user_id = %auth.user.id, error = %format!("{error:#}"), "could not connect a Microsoft calendar");
            return microsoft_failure(&state, auth.lang, "microsoft-error-connect", Back::Sources);
        }
    };
    // A disabled source keeps its new tokens and stays disabled: nothing syncs.
    let enabled: bool = sqlx::query_scalar("SELECT enabled FROM caldav_sources WHERE id = ?")
        .bind(&source_id)
        .fetch_one(&state.pool)
        .await
        .unwrap_or(false);
    if !enabled {
        return (
            [
                ("Cache-Control", "no-store"),
                ("Referrer-Policy", "no-referrer"),
            ],
            Redirect::to("/dashboard/sources"),
        )
            .into_response();
    }
    // Only the calendars are listed here, for the write-calendar choice. The
    // events come from a background sync: a large mailbox can take longer
    // than a reverse proxy waits for this response, and the state is spent.
    // Do not imply successful discovery when Graph rejects calendar access.
    let provider = match crate::microsoft_oauth::build_provider_for_source(
        &state.pool,
        &state.secret_key,
        &source_id,
    )
    .await
    {
        Ok(provider) => provider,
        Err(error) => {
            tracing::warn!(user_id = %auth.user.id, source_id = %source_id, error = %format!("{error:#}"), "could not use a newly connected Microsoft calendar");
            return microsoft_failure(&state, auth.lang, "microsoft-error-sync", Back::Sources);
        }
    };
    if let Err(error) = crate::commands::sync::discover_provider_calendars(
        &state.pool,
        provider.as_ref(),
        &source_id,
    )
    .await
    {
        tracing::warn!(user_id = %auth.user.id, source_id = %source_id, error = %format!("{error:#}"), "could not list the calendars of a newly connected Microsoft account");
        return microsoft_failure(&state, auth.lang, "microsoft-error-sync", Back::Sources);
    }
    {
        let (pool, key, source_id) = (
            state.pool.clone(),
            state.secret_key.clone(),
            source_id.clone(),
        );
        tokio::spawn(async move {
            crate::commands::sync::sync_source_by_id(&pool, &key, &source_id).await;
        });
    }
    let selected: Option<String> = match sqlx::query_scalar(
        "SELECT write_calendar_href FROM caldav_sources WHERE id = ?",
    )
    .bind(&source_id)
    .fetch_one(&state.pool)
    .await
    {
        Ok(selected) => selected,
        Err(error) => {
            tracing::warn!(user_id = %auth.user.id, source_id = %source_id, error = %error, "could not read the Microsoft source's write calendar");
            return microsoft_failure(
                &state,
                auth.lang,
                "microsoft-error-selection",
                Back::Sources,
            );
        }
    };
    let destination = if selected.as_ref().is_some_and(|href| !href.is_empty()) {
        "/dashboard/sources".to_string()
    } else {
        format!("/dashboard/sources/{source_id}/setup-write")
    };
    (
        [
            ("Cache-Control", "no-store"),
            ("Referrer-Policy", "no-referrer"),
        ],
        Redirect::to(&destination),
    )
        .into_response()
}

#[derive(Deserialize)]
pub(super) struct MicrosoftConfigForm {
    _csrf: Option<String>,
    microsoft_oauth2_client_id: String,
    microsoft_oauth2_client_secret: String,
    microsoft_oauth2_tenant: String,
}

pub(super) async fn admin_update_microsoft_oauth2(
    State(state): State<Arc<AppState>>,
    admin: crate::auth::AdminUser,
    headers: HeaderMap,
    Form(form): Form<MicrosoftConfigForm>,
) -> Response {
    if let Err(response) = verify_csrf_token(&headers, &form._csrf) {
        return response;
    }
    match crate::microsoft_oauth::save_config(
        &state.pool,
        &state.secret_key,
        form.microsoft_oauth2_client_id.trim(),
        &form.microsoft_oauth2_client_secret,
        form.microsoft_oauth2_tenant.trim(),
    )
    .await
    {
        Ok(()) => Redirect::to("/dashboard/admin").into_response(),
        Err(error) => {
            tracing::warn!(user_id = %admin.user.id, error = %format!("{error:#}"), "Microsoft settings not saved");
            microsoft_failure(&state, admin.lang, "microsoft-error-settings", Back::Admin)
        }
    }
}
