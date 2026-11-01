//! Global Exchange (EWS) impersonation, web side: the admin panel card, the
//! form that saves it, and the guards that keep regular users away from the
//! sources the admin provisions. The config itself, provisioning and the
//! provider for a managed source live in [`crate::ews::global`].

use super::*;
use crate::ews::global as ews_global;

/// Outcomes of a save, carried back to the admin panel as `?ews=<value>`.
const NOTICES: &[&str] = &[
    "saved",
    "invalid-url",
    "incomplete",
    "override-needs-domains",
    "provision-failed",
];

/// Template context for the admin panel card. `notice` is the raw `?ews=`
/// query value; anything unknown is dropped.
pub(super) async fn admin_context(
    state: &AppState,
    notice: Option<&str>,
    provisioned: Option<&str>,
) -> minijinja::Value {
    let cfg = state.ews_global.read().await.clone();
    // What the admin last saved, even when incomplete or switched off: the
    // live config is `None` then, but the form must not come back empty.
    let stored: Option<(
        i64,
        Option<String>,
        Option<String>,
        bool,
        i64,
        i64,
        Option<String>,
    )> = sqlx::query_as(
        "SELECT ews_global_enabled, ews_global_url, ews_service_username, \
                    ews_service_password_enc IS NOT NULL AND ews_service_password_enc != '', \
                    ews_lock_user_sources, ews_auto_provision, ews_impersonation_domain \
             FROM auth_config WHERE id = 'singleton'",
    )
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);
    let (_, url, username, has_password, lock, auto, domain) =
        stored.unwrap_or((0, None, None, false, 0, 0, None));

    let managed_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM caldav_sources WHERE managed = 1 AND provider_type = 'ews'",
    )
    .fetch_one(&state.pool)
    .await
    .unwrap_or(0);

    // Per-user status of the managed sources, so the admin can see which
    // mailboxes synced and which failed (e.g. a user without an Exchange
    // mailbox). Failing rows first.
    let rows: Vec<(String, Option<String>, Option<String>, Option<String>, bool)> = sqlx::query_as(
        "SELECT u.email, cs.impersonate_email, cs.last_synced, cs.last_sync_error, cs.enabled \
             FROM caldav_sources cs \
             JOIN accounts a ON a.id = cs.account_id \
             JOIN users u ON u.id = a.user_id \
             WHERE cs.managed = 1 AND cs.provider_type = 'ews' \
             ORDER BY (cs.last_sync_error IS NOT NULL) DESC, u.email",
    )
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();
    let ok_count = rows
        .iter()
        .filter(|(_, _, synced, error, _)| error.is_none() && synced.is_some())
        .count();
    let sources: Vec<minijinja::Value> = rows
        .iter()
        .map(|(email, mailbox, last_synced, last_error, enabled)| {
            // `status` picks the wording in the template; `error` is the
            // server's own message, shown as-is when there is no better one.
            // The fault code leads the message (see `extract_soap_fault`), so
            // matching on it works whatever the server's display language.
            let (status, error) = match last_error {
                Some(e) if e.contains("ErrorNonExistentMailbox") => ("no-mailbox", ""),
                Some(e) => ("error", e.as_str()),
                None if last_synced.is_some() => ("synced", ""),
                None => ("never", ""),
            };
            // The address actually impersonated, after the domain override.
            let target = cfg
                .as_ref()
                .and_then(|c| c.impersonation_target(mailbox.as_deref()))
                .or_else(|| mailbox.clone())
                .unwrap_or_default();
            context! {
                email => email,
                mailbox => target,
                last_synced => last_synced,
                ok => status == "synced",
                enabled => enabled,
                status => status,
                error => error,
            }
        })
        .collect();

    let url = url.unwrap_or_default();
    context! {
        ews_global_enabled => cfg.is_some(),
        ews_url_insecure => url.trim_start().to_ascii_lowercase().starts_with("http://"),
        ews_global_url => url,
        ews_service_username => username.unwrap_or_default(),
        ews_has_password => has_password,
        ews_lock_user_sources => lock != 0,
        ews_auto_provision => auto != 0,
        ews_impersonation_domain => domain.unwrap_or_default(),
        ews_managed_source_count => managed_count,
        ews_managed_ok_count => ok_count,
        ews_managed_sources => sources,
        ews_notice => notice.filter(|n| NOTICES.contains(n)).unwrap_or(""),
        // How many managed sources the save just created, when it ran the
        // batch; absent otherwise.
        ews_provisioned => provisioned.and_then(|n| n.parse::<u32>().ok()),
    }
}

#[derive(Deserialize)]
pub(super) struct AdminEwsGlobalForm {
    _csrf: Option<String>,
    ews_global_enabled: Option<String>,
    ews_global_url: Option<String>,
    ews_service_username: Option<String>,
    ews_service_password: Option<String>,
    ews_lock_user_sources: Option<String>,
    ews_auto_provision: Option<String>,
    ews_impersonation_domain: Option<String>,
    /// Set by the "Save & provision all users now" button.
    provision_now: Option<String>,
}

fn redirect_with(notice: &str) -> Response {
    Redirect::to(&format!("/dashboard/admin?ews={notice}#ews-global")).into_response()
}

/// A successful save that also ran the provisioning batch says how many
/// sources it created, so the admin sees that the click did something.
fn redirect_saved(provisioned: Option<usize>) -> Response {
    match provisioned {
        Some(n) => Redirect::to(&format!("/dashboard/admin?ews=saved&n={n}#ews-global")),
        None => Redirect::to("/dashboard/admin?ews=saved#ews-global"),
    }
    .into_response()
}

pub(super) async fn admin_update_ews_global(
    State(state): State<Arc<AppState>>,
    admin: crate::auth::AdminUser,
    headers: HeaderMap,
    Form(form): Form<AdminEwsGlobalForm>,
) -> impl IntoResponse {
    if let Err(resp) = verify_csrf_token(&headers, &form._csrf) {
        return resp;
    }

    let trimmed = |v: Option<String>| v.map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
    let enabled = form.ews_global_enabled.as_deref() == Some("on");
    let url = trimmed(form.ews_global_url);
    let username = trimmed(form.ews_service_username);
    let password = form.ews_service_password.filter(|s| !s.trim().is_empty());
    let lock = form.ews_lock_user_sources.as_deref() == Some("on");
    let auto_provision = form.ews_auto_provision.as_deref() == Some("on");
    let imp_domain = trimmed(form.ews_impersonation_domain)
        .map(|s| s.trim_start_matches('@').to_string())
        .filter(|s| !s.is_empty());
    let provision_now = form.provision_now.as_deref() == Some("1");

    // The endpoint and the domain override together decide which mailbox on
    // which server every managed source reads. Changing either makes what
    // they cached belong to a mailbox they no longer read, and Exchange
    // folder ids are only meaningful in the mailbox they came from.
    let previous: Option<(Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT ews_global_url, ews_impersonation_domain FROM auth_config WHERE id = 'singleton'",
    )
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);
    let (previous_url, previous_domain) = previous.unwrap_or((None, None));
    let same = |stored: Option<String>, new: &Option<String>| -> bool {
        stored.filter(|s| !s.is_empty()).as_ref() == new.as_ref()
    };
    let mailboxes_moved = !same(previous_url, &url) || !same(previous_domain, &imp_domain);

    if let Some(u) = url.as_deref() {
        if let Err(e) =
            crate::providers::factory::validate_url(crate::providers::factory::kinds::EWS, u)
        {
            tracing::warn!(error = %e, "admin: invalid global EWS URL");
            return redirect_with("invalid-url");
        }
    }
    if imp_domain.is_some() {
        // The override swaps the domain of every address it applies to, so it
        // needs the instance's allowed email domains to say which those are.
        let allowed: Option<String> = sqlx::query_scalar(
            "SELECT allowed_email_domains FROM auth_config WHERE id = 'singleton'",
        )
        .fetch_optional(&state.pool)
        .await
        .ok()
        .flatten()
        .flatten();
        if !allowed.as_deref().is_some_and(|d| !d.trim().is_empty()) {
            return redirect_with("override-needs-domains");
        }
    }
    if enabled {
        // Switching on without everything the sync needs would save a config
        // that loads as "off": refuse it instead of pretending it worked.
        let has_stored_password: bool = sqlx::query_scalar(
            "SELECT ews_service_password_enc IS NOT NULL AND ews_service_password_enc != '' \
             FROM auth_config WHERE id = 'singleton'",
        )
        .fetch_optional(&state.pool)
        .await
        .ok()
        .flatten()
        .unwrap_or(false);
        if url.is_none() || username.is_none() || (password.is_none() && !has_stored_password) {
            return redirect_with("incomplete");
        }
    }

    let encrypted = match password.as_deref() {
        Some(p) => match crate::crypto::encrypt_value(&state.secret_key, p) {
            Ok(enc) => Some(enc),
            Err(e) => return internal_error_response("encrypt EWS service password", &e),
        },
        None => None,
    };
    // An empty password field keeps the stored one.
    let write = sqlx::query(
        "UPDATE auth_config SET ews_global_enabled = ?, ews_global_url = ?, \
             ews_service_username = ?, \
             ews_service_password_enc = COALESCE(?, ews_service_password_enc), \
             ews_lock_user_sources = ?, ews_auto_provision = ?, \
             ews_impersonation_domain = ?, updated_at = datetime('now') \
         WHERE id = 'singleton'",
    )
    .bind(enabled as i64)
    .bind(&url)
    .bind(&username)
    .bind(&encrypted)
    .bind(lock as i64)
    .bind(auto_provision as i64)
    .bind(&imp_domain)
    .execute(&state.pool)
    .await;
    if let Err(e) = write {
        // No cache reload, no provisioning, no success redirect: the admin
        // must not believe a config was saved when it was not.
        return internal_error_response("save global EWS config", &e);
    }

    if mailboxes_moved {
        match ews_global::managed_source_ids(&state.pool).await {
            Ok(ids) => {
                for id in &ids {
                    if let Err(e) = ews_global::forget_mailbox(&state.pool, id).await {
                        tracing::warn!(error = %e, source_id = %id, "could not reset a managed source after the Exchange target changed");
                    }
                }
                if !ids.is_empty() {
                    tracing::info!(
                        sources = ids.len(),
                        "Exchange target changed: managed sources reset, their calendars will be rediscovered"
                    );
                }
            }
            Err(e) => {
                tracing::warn!(error = %e, "could not list managed sources after the Exchange target changed")
            }
        }
    }

    let new_config = ews_global::load_ews_global_config(&state.pool, &state.secret_key).await;
    let mut notice = "saved";
    let mut provisioned: Option<usize> = None;
    if let Some(cfg) = new_config.as_ref() {
        // The managed rows carry the endpoint and account for display only
        // (sync reads the live config), so keep them in step with it.
        if let Err(e) = sqlx::query(
            "UPDATE caldav_sources SET url = ?, username = ? \
             WHERE managed = 1 AND provider_type = 'ews'",
        )
        .bind(&cfg.url)
        .bind(&cfg.service_username)
        .execute(&state.pool)
        .await
        {
            tracing::warn!(error = %e, "admin: could not refresh managed EWS source rows");
        }
        // Either intent means "give every existing user a managed source now".
        if provision_now || cfg.auto_provision {
            match ews_global::provision_managed_ews_source_for_all_users(&state.pool, cfg).await {
                Ok(n) => {
                    tracing::info!(provisioned = n, "admin: global EWS provisioning done");
                    provisioned = Some(n);
                }
                Err(e) => {
                    tracing::error!(error = %e, "admin: global EWS provisioning failed");
                    notice = "provision-failed";
                }
            }
        }
    }
    *state.ews_global.write().await = new_config;

    tracing::info!(admin = %admin.user.email, enabled, "admin: global EWS config updated");
    if notice == "saved" {
        redirect_saved(provisioned)
    } else {
        redirect_with(notice)
    }
}

/// When the admin locked calendar sources, a regular user may not add one.
/// `Some` is the 403 to return; admins are never locked out.
pub(super) async fn source_lockdown(
    state: &AppState,
    auth_user: &crate::auth::AuthUser,
) -> Option<Response> {
    if auth_user.user.role == "admin" {
        return None;
    }
    let locked = state
        .ews_global
        .read()
        .await
        .as_ref()
        .is_some_and(|c| c.lock_user_sources);
    locked.then(|| forbidden(auth_user.lang, "sources-locked-by-admin"))
}

/// Whether `source_id` is one of `user_id`'s sources that the admin
/// provisioned. Unknown ids read as not managed; the caller's own ownership
/// check deals with them.
pub(super) async fn is_managed_source(pool: &SqlitePool, user_id: &str, source_id: &str) -> bool {
    sqlx::query_scalar::<_, i64>(
        "SELECT cs.managed FROM caldav_sources cs \
         JOIN accounts a ON a.id = cs.account_id \
         WHERE cs.id = ? AND a.user_id = ?",
    )
    .bind(source_id)
    .bind(user_id)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten()
    .is_some_and(|m| m != 0)
}

pub(super) fn forbidden(lang: &str, key: &str) -> Response {
    (
        axum::http::StatusCode::FORBIDDEN,
        Html(crate::i18n::translate(lang, key, None)),
    )
        .into_response()
}
