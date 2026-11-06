//! Microsoft consent is separate from the user's Cal.rs sign-in identity.
use super::*;

#[derive(Deserialize)]
pub(super) struct ConnectForm {
    _csrf: Option<String>,
    source_id: Option<String>,
}

fn failure(message: String) -> Response {
    // Plain text and fixed messages: no provider-controlled HTML or token errors.
    (
        axum::http::StatusCode::BAD_REQUEST,
        [
            ("Cache-Control", "no-store"),
            ("Referrer-Policy", "no-referrer"),
        ],
        message,
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
        return failure(crate::i18n::translate(
            auth.lang,
            "microsoft-error-impersonation",
            None,
        ));
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
        Err(_) => failure(crate::i18n::translate(
            auth.lang,
            "microsoft-error-start",
            None,
        )),
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
        return failure(crate::i18n::translate(
            auth.lang,
            "microsoft-error-impersonation",
            None,
        ));
    }
    let pending = match crate::microsoft_oauth::consume_state(
        &state.pool,
        &auth.user.id,
        query.state.as_deref().unwrap_or_default(),
    )
    .await
    {
        Ok(pending) => pending,
        Err(_) => {
            return failure(crate::i18n::translate(
                auth.lang,
                "microsoft-error-state",
                None,
            ))
        }
    };
    // Consume state even for denial or missing code; it must never be replayable.
    if query.error.is_some() {
        return failure(crate::i18n::translate(
            auth.lang,
            "microsoft-error-consent",
            None,
        ));
    }
    let Some(code) = query.code.filter(|code| !code.is_empty()) else {
        return failure(crate::i18n::translate(
            auth.lang,
            "microsoft-error-consent",
            None,
        ));
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
        Err(_) => {
            return failure(crate::i18n::translate(
                auth.lang,
                "microsoft-error-connect",
                None,
            ))
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
        Err(_) => {
            return failure(crate::i18n::translate(
                auth.lang,
                "microsoft-error-sync",
                None,
            ))
        }
    };
    if crate::commands::sync::discover_provider_calendars(
        &state.pool,
        provider.as_ref(),
        &source_id,
    )
    .await
    .is_err()
    {
        return failure(crate::i18n::translate(
            auth.lang,
            "microsoft-error-sync",
            None,
        ));
    }
    {
        let (pool, key, source_id) = (state.pool.clone(), state.secret_key, source_id.clone());
        tokio::spawn(async move {
            crate::commands::sync::sync_source_by_id(&pool, &key, &source_id).await;
        });
    }
    let selected: Option<String> =
        match sqlx::query_scalar("SELECT write_calendar_href FROM caldav_sources WHERE id = ?")
            .bind(&source_id)
            .fetch_one(&state.pool)
            .await
        {
            Ok(selected) => selected,
            Err(_) => {
                return failure(crate::i18n::translate(
                    auth.lang,
                    "microsoft-error-selection",
                    None,
                ))
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
        Err(_) => failure(crate::i18n::translate(
            admin.lang,
            "microsoft-error-settings",
            None,
        )),
    }
}
