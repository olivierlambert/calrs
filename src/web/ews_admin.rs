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
    "override-domains",
    "provision-failed",
    "trusted",
    "removed",
    "https-required",
    "password-cleared",
    "source-removed",
    "trust-changed",
];

/// How many unconfirmed addresses the card lists at most.
const UNTRUSTED_SHOWN: usize = 20;

/// Template context for the admin panel card. `notice` is the raw `?ews=`
/// query value; anything unknown is dropped.
pub(super) async fn admin_context(
    state: &AppState,
    query: &std::collections::HashMap<String, String>,
) -> minijinja::Value {
    let notice = query.get("ews").map(String::as_str);
    let count = |k: &str| query.get(k).and_then(|n| n.parse::<u32>().ok());
    let cfg = state.ews_global.read().await.clone();
    // What the admin last saved, even when incomplete or switched off: the
    // live config is `None` then, but the form must not come back empty.
    #[allow(clippy::type_complexity)]
    let stored: Option<(
        i64,
        Option<String>,
        Option<String>,
        bool,
        i64,
        i64,
        Option<String>,
        Option<String>,
        Option<String>,
    )> = sqlx::query_as(
        "SELECT ews_global_enabled, ews_global_url, ews_service_username, \
                    ews_service_password_enc IS NOT NULL AND ews_service_password_enc != '', \
                    ews_lock_user_sources, ews_auto_provision, ews_impersonation_domain, \
                    ews_impersonation_from_domain, allowed_email_domains \
             FROM auth_config WHERE id = 'singleton'",
    )
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);
    let (_, url, username, has_password, lock, auto, domain, from_domain, allowed_domains) =
        stored.unwrap_or((0, None, None, false, 0, 0, None, None, None));

    let managed_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM caldav_sources WHERE managed = 1 AND provider_type = 'ews'",
    )
    .fetch_one(&state.pool)
    .await
    .unwrap_or(0);

    // Per-user status of the managed sources, so the admin can see which
    // mailboxes synced and which failed (e.g. a user without an Exchange
    // mailbox). Failing rows first.
    #[allow(clippy::type_complexity)]
    let rows: Vec<(
        String,
        Option<String>,
        Option<String>,
        Option<String>,
        bool,
        bool,
        bool,
        String,
        Option<String>,
        Option<String>,
    )> = sqlx::query_as(
        "SELECT u.email, cs.impersonate_email, cs.last_synced, cs.last_sync_error, cs.enabled, \
                u.enabled, u.email_trusted, u.id, cs.taken_over_from, cs.taken_over_at \
             FROM caldav_sources cs \
             JOIN accounts a ON a.id = cs.account_id \
             JOIN users u ON u.id = a.user_id \
             WHERE cs.managed = 1 AND cs.provider_type = 'ews' \
             ORDER BY (cs.last_sync_error IS NOT NULL) DESC, u.email",
    )
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();
    let mut ok_count = 0usize;
    let mut sources: Vec<minijinja::Value> = rows
        .iter()
        .map(
            |(
                email,
                mailbox,
                last_synced,
                last_error,
                enabled,
                user_enabled,
                trusted,
                user_id,
                taken_over_from,
                taken_over_at,
            )| {
                // `status` picks the wording in the template; `error` is the
                // server's own message, shown as-is when there is no better
                // one. The fault code leads the message (see
                // `extract_soap_fault`), so matching on it works whatever the
                // server's display language.
                let (status, error) = match last_error {
                    _ if !user_enabled => ("suspended", ""),
                    _ if !trusted => ("untrusted", ""),
                    Some(e) if e.contains("ErrorNonExistentMailbox") => ("no-mailbox", ""),
                    Some(e) => ("error", e.as_str()),
                    None if last_synced.is_some() => ("synced", ""),
                    None => ("never", ""),
                };
                if status == "synced" {
                    ok_count += 1;
                }
                context! {
                    user_id => user_id,
                    taken_over_from => taken_over_from,
                    taken_over_at => taken_over_at,
                    email => email,
                    // The address actually impersonated, after the override.
                    mailbox => mailbox.clone().unwrap_or_default(),
                    last_synced => last_synced,
                    ok => status == "synced",
                    enabled => enabled,
                    status => status,
                    error => error,
                }
            },
        )
        .collect();

    // Trusted users left without a source because their mailbox is already
    // another user's: listed too, so the admin sees why.
    if let Some(c) = cfg.as_ref() {
        let taken: std::collections::HashSet<String> =
            rows.iter().filter_map(|r| r.1.clone()).collect();
        let unprovisioned: Vec<String> = sqlx::query_scalar(
            "SELECT u.email FROM users u WHERE u.enabled = 1 AND u.email_trusted = 1 \
               AND NOT EXISTS (SELECT 1 FROM caldav_sources cs JOIN accounts a ON a.id = cs.account_id \
                               WHERE a.user_id = u.id AND cs.managed = 1 AND cs.provider_type = 'ews') \
             ORDER BY u.email",
        )
        .fetch_all(&state.pool)
        .await
        .unwrap_or_default();
        for email in unprovisioned {
            let Some(mailbox) = c.impersonation_target(&email) else {
                continue;
            };
            if taken.contains(&mailbox) {
                sources.insert(
                    0,
                    context! {
                        email => email,
                        mailbox => mailbox,
                        last_synced => None::<String>,
                        ok => false,
                        enabled => true,
                        status => "conflict",
                        error => "",
                    },
                );
            }
        }
    }

    // Enabled accounts in the allowed domains whose address nobody vouched
    // for: they get no managed source until the admin confirms them.
    let untrusted: Vec<(String, String, String)> = sqlx::query_as(
        "SELECT id, email, name FROM users WHERE enabled = 1 AND email_trusted = 0 \
         ORDER BY lower(email)",
    )
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();
    let untrusted: Vec<&(String, String, String)> = untrusted
        .iter()
        .filter(|(_, email, _)| crate::auth::is_email_allowed(email, &allowed_domains))
        .collect();
    let untrusted_count = untrusted.len();
    // The mailbox a confirmation would open, as the admin must judge it:
    // after the domain override, or "none" for an address that names none.
    let domain_override =
        ews_global::DomainOverride::new(from_domain.as_deref(), domain.as_deref());
    let untrusted_users: Vec<minijinja::Value> = untrusted
        .iter()
        .take(UNTRUSTED_SHOWN)
        .map(|(id, email, name)| {
            context! {
                id => id,
                email => email,
                name => name,
                mailbox => ews_global::mailbox_for(email, domain_override.as_ref()),
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
        ews_impersonation_from_domain => from_domain.unwrap_or_default(),
        ews_managed_source_count => managed_count,
        ews_managed_ok_count => ok_count,
        ews_managed_sources => sources,
        ews_untrusted_users => untrusted_users,
        ews_untrusted_count => untrusted_count,
        ews_notice => notice.filter(|n| NOTICES.contains(n)).unwrap_or(""),
        // How many managed sources the save just created, when it ran the
        // batch, or the removal deleted (`?n=`); absent otherwise.
        ews_provisioned => count("n"),
        // Event types that lost a calendar selection, and addresses to
        // confirm again, after a save that changed the mailboxes.
        ews_selections_lost => count("lost"),
        ews_reconfirm => count("reconfirm"),
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
    ews_impersonation_from_domain: Option<String>,
    /// Set by the "Save & provision all users now" button.
    provision_now: Option<String>,
}

/// A URL as compared for "did the endpoint change": parsed and re-serialized
/// (scheme and host lowercased, default port dropped), else trimmed.
fn normalized_url(url: Option<&str>) -> Option<String> {
    let url = url.map(str::trim).filter(|u| !u.is_empty())?;
    Some(
        reqwest::Url::parse(url)
            .map(|u| u.to_string())
            .unwrap_or_else(|_| url.to_string()),
    )
}

fn redirect_with(notice: &str) -> Response {
    Redirect::to(&format!("/dashboard/admin?ews={notice}#ews-global")).into_response()
}

/// A successful save that also ran the provisioning batch says how many
/// sources it created, so the admin sees that the click did something.
/// Also carries how many event types lost a calendar selection (`lost`) and
/// how many addresses need a new confirmation (`reconfirm`), when any did.
fn redirect_saved(provisioned: Option<usize>, lost: u64, reconfirm: u64) -> Response {
    let mut url = String::from("/dashboard/admin?ews=saved");
    if let Some(n) = provisioned {
        url.push_str(&format!("&n={n}"));
    }
    if lost > 0 {
        url.push_str(&format!("&lost={lost}"));
    }
    if reconfirm > 0 {
        url.push_str(&format!("&reconfirm={reconfirm}"));
    }
    url.push_str("#ews-global");
    Redirect::to(&url).into_response()
}

/// Scheme, host and port of a URL, lowercased: the server it names.
fn url_origin(url: Option<&str>) -> Option<String> {
    let url = url.map(str::trim).filter(|u| !u.is_empty())?;
    Some(match reqwest::Url::parse(url) {
        Ok(u) => u.origin().ascii_serialization(),
        Err(_) => url.to_ascii_lowercase(),
    })
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
    // The override rewrites addresses in exactly one domain (the users') to
    // the mailbox domain: both or neither, each a plain domain.
    let (from_domain, imp_domain) = match (
        ews_global::normalize_domain(form.ews_impersonation_from_domain.as_deref().unwrap_or("")),
        ews_global::normalize_domain(form.ews_impersonation_domain.as_deref().unwrap_or("")),
    ) {
        (Ok(from), Ok(to)) if from.is_some() == to.is_some() => (from, to),
        _ => return redirect_with("override-domains"),
    };
    let provision_now = form.provision_now.as_deref() == Some("1");

    // The endpoint and the domain override together decide which mailbox on
    // which server every managed source reads. Changing either makes what
    // they cached belong to a mailbox they no longer read, and Exchange
    // folder ids are only meaningful in the mailbox they came from.
    #[allow(clippy::type_complexity)]
    let previous: Option<(
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
        bool,
    )> = sqlx::query_as(
        "SELECT ews_global_url, ews_impersonation_domain, ews_impersonation_from_domain, \
                    ews_service_username, \
                    COALESCE(ews_service_password_enc, '') != '' \
             FROM auth_config WHERE id = 'singleton'",
    )
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);
    let (previous_url, previous_domain, previous_from, previous_username, has_stored_password) =
        previous.unwrap_or((None, None, None, None, false));
    let same = |stored: &Option<String>, new: &Option<String>| -> bool {
        stored.as_ref().filter(|s| !s.is_empty()) == new.as_ref()
    };
    let url_moved = normalized_url(previous_url.as_deref()) != normalized_url(url.as_deref());
    // Which mailbox each source reads depends on the server (scheme, host,
    // port) and the domain override, not on how the URL's path is written.
    let server_moved = url_origin(previous_url.as_deref()) != url_origin(url.as_deref());
    let override_moved =
        !same(&previous_domain, &imp_domain) || !same(&previous_from, &from_domain);
    // The stored password belongs to the account and the server it was typed
    // for. Kept across a new endpoint or a new account, it would be sent to
    // wherever the URL now points: never keep it then.
    let username_moved = previous_username
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        != username.as_deref();
    let password_cleared =
        password.is_none() && has_stored_password && (url_moved || username_moved);

    if let Some(u) = url.as_deref() {
        if let Err(e) =
            crate::providers::factory::validate_url(crate::providers::factory::kinds::EWS, u)
        {
            tracing::warn!(error = %e, "admin: invalid global EWS URL");
            return redirect_with("invalid-url");
        }
        // The service account's password goes out as Basic auth on every
        // request: never over plain http.
        if !u.trim().to_ascii_lowercase().starts_with("https://") {
            return redirect_with("https-required");
        }
    }
    if enabled && !password_cleared {
        // Switching on without everything the sync needs would save a config
        // that loads as "off": refuse it instead of pretending it worked.
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
    // An empty password field keeps the stored one, unless the endpoint or
    // the account changed (`password_cleared`): then it is dropped, the
    // config is incomplete and the feature idles until it is typed again.
    // The new override and the trust it withdraws from addresses it
    // re-maps are one transaction: never one without the other.
    let old_override =
        ews_global::DomainOverride::new(previous_from.as_deref(), previous_domain.as_deref());
    let new_override =
        ews_global::DomainOverride::new(from_domain.as_deref(), imp_domain.as_deref());
    let mut reconfirm: u64 = 0;
    let saved: anyhow::Result<()> = async {
        let mut tx = state.pool.begin().await?;
        sqlx::query(
            "UPDATE auth_config SET ews_global_enabled = ?, ews_global_url = ?, \
             ews_service_username = ?, \
             ews_service_password_enc = CASE WHEN ? THEN NULL \
                 ELSE COALESCE(?, ews_service_password_enc) END, \
             ews_lock_user_sources = ?, ews_auto_provision = ?, \
             ews_impersonation_domain = ?, ews_impersonation_from_domain = ?, \
             updated_at = datetime('now') \
         WHERE id = 'singleton'",
        )
        .bind(enabled as i64)
        .bind(&url)
        .bind(&username)
        .bind(password_cleared)
        .bind(&encrypted)
        .bind(lock as i64)
        .bind(auto_provision as i64)
        .bind(&imp_domain)
        .bind(&from_domain)
        .execute(&mut *tx)
        .await?;
        if override_moved {
            // An admin confirmed an address for the mailbox it opened then:
            // one that now maps elsewhere needs a new confirmation.
            reconfirm =
                ews_global::untrust_remapped(&mut tx, old_override.as_ref(), new_override.as_ref())
                    .await?;
        }
        tx.commit().await?;
        Ok(())
    }
    .await;
    if let Err(e) = saved {
        // No retargeting, cache reload, provisioning or success redirect:
        // the admin must not believe a config was saved when it was not.
        return internal_error_response("save global EWS config", &e);
    }

    // Event types that lost a calendar selection because their source now
    // reads another mailbox.
    let mut lost: u64 = 0;
    if override_moved {
        // Each source moves to the mailbox its user's address maps to (or
        // goes, when that mailbox is another user's) and forgets what it
        // cached.
        match ews_global::retarget_managed_sources(&state.pool, new_override.as_ref()).await {
            Ok(n) => lost += n,
            Err(e) => {
                tracing::warn!(error = %e, "could not retarget the managed sources after the domain override changed")
            }
        }
    }
    if server_moved {
        // Another Exchange server: folder ids mean nothing there, so every
        // managed source starts again from discovery.
        match ews_global::managed_source_ids(&state.pool).await {
            Ok(ids) => {
                for id in &ids {
                    match ews_global::forget_mailbox(&state.pool, id).await {
                        Ok(n) => lost += n,
                        Err(e) => {
                            tracing::warn!(error = %e, source_id = %id, "could not reset a managed source after the Exchange server changed")
                        }
                    }
                }
                if !ids.is_empty() {
                    tracing::info!(
                        sources = ids.len(),
                        "Exchange server changed: managed sources reset, their calendars will be rediscovered"
                    );
                }
            }
            Err(e) => {
                tracing::warn!(error = %e, "could not list managed sources after the Exchange server changed")
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
    if password_cleared {
        tracing::warn!(admin = %admin.user.email, "admin: EWS endpoint or service account changed without a new password: stored password cleared");
        return redirect_with("password-cleared");
    }
    if notice == "saved" {
        redirect_saved(provisioned, lost, reconfirm)
    } else {
        redirect_with(notice)
    }
}

#[derive(Deserialize)]
pub(super) struct AdminEwsTrustForm {
    _csrf: Option<String>,
    user_id: String,
    /// The address and the mailbox the admin saw (empty: none).
    email: String,
    #[serde(default)]
    mailbox: String,
}

/// The admin confirms that a user owns their address: from now on calrs may
/// open the Exchange mailbox it names, and does so right away while global
/// Exchange is on.
pub(super) async fn admin_trust_email(
    State(state): State<Arc<AppState>>,
    admin: crate::auth::AdminUser,
    headers: HeaderMap,
    Form(form): Form<AdminEwsTrustForm>,
) -> impl IntoResponse {
    if let Err(resp) = verify_csrf_token(&headers, &form._csrf) {
        return resp;
    }
    // The confirmation covers the address and the mailbox the admin saw:
    // if the domain override now maps it elsewhere, or the address itself
    // changed (an SSO login), they must look again.
    let overrides: Option<(Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT ews_impersonation_from_domain, ews_impersonation_domain \
         FROM auth_config WHERE id = 'singleton'",
    )
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);
    let (from, to) = overrides.unwrap_or((None, None));
    let current = ews_global::mailbox_for(
        &form.email,
        ews_global::DomainOverride::new(from.as_deref(), to.as_deref()).as_ref(),
    )
    .unwrap_or_default();
    if current != form.mailbox {
        return redirect_with("trust-changed");
    }
    match ews_global::trust_email(&state.pool, &form.user_id, &form.email).await {
        Ok(true) => {}
        Ok(false) => return redirect_with("trust-changed"),
        Err(e) => return internal_error_response("trust user email", &e),
    }
    tracing::info!(admin = %admin.user.email, user_id = %form.user_id, "admin: email address confirmed");
    let cfg = state.ews_global.read().await.clone();
    if let Some(cfg) = cfg {
        match ews_global::provision_managed_ews_source_for_user(&state.pool, &cfg, &form.user_id)
            .await
        {
            Ok(outcome) => {
                tracing::info!(user_id = %form.user_id, ?outcome, "admin: provisioned after confirming the address")
            }
            Err(e) => {
                tracing::error!(error = %e, "admin: provisioning after confirming the address failed");
                return redirect_with("provision-failed");
            }
        }
    }
    redirect_with("trusted")
}

#[derive(Deserialize)]
pub(super) struct AdminEwsUserForm {
    _csrf: Option<String>,
    user_id: String,
}

/// Remove one user's managed source (its calendars and cached busy times
/// cascade). Auto-provisioning gives it back at the next save, confirmation
/// or SSO login; to keep someone out for good, suspend the account.
pub(super) async fn admin_remove_user_source(
    State(state): State<Arc<AppState>>,
    admin: crate::auth::AdminUser,
    headers: HeaderMap,
    Form(form): Form<AdminEwsUserForm>,
) -> impl IntoResponse {
    if let Err(resp) = verify_csrf_token(&headers, &form._csrf) {
        return resp;
    }
    // Locked and deleted in `remove_managed_source`.
    let source: Option<String> = sqlx::query_scalar(
        "SELECT cs.id FROM caldav_sources cs JOIN accounts a ON a.id = cs.account_id \
         WHERE a.user_id = ? AND cs.managed = 1 AND cs.provider_type = 'ews'",
    )
    .bind(&form.user_id)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);
    if source.is_none() {
        return Redirect::to("/dashboard/admin#ews-global").into_response();
    }
    if let Err(e) = ews_global::remove_managed_source(&state.pool, &form.user_id).await {
        return internal_error_response("remove a managed EWS source", &e);
    }
    tracing::warn!(admin = %admin.user.email, user_id = %form.user_id, "admin: managed Exchange source removed");
    redirect_with("source-removed")
}

#[derive(Deserialize)]
pub(super) struct AdminEwsRemoveForm {
    _csrf: Option<String>,
}

/// Take global Exchange out of service: delete every managed source (their
/// calendars and cached busy times cascade), clear the stored service
/// password and switch the feature off. Bookings already written into the
/// mailboxes stay there: calrs no longer holds a way to reach them.
pub(super) async fn admin_remove_managed_sources(
    State(state): State<Arc<AppState>>,
    admin: crate::auth::AdminUser,
    headers: HeaderMap,
    Form(form): Form<AdminEwsRemoveForm>,
) -> impl IntoResponse {
    if let Err(resp) = verify_csrf_token(&headers, &form._csrf) {
        return resp;
    }
    // Off first, under the cache lock, so no sync or write-back starts with
    // the old config while the rows go.
    let mut live = state.ews_global.write().await;
    if let Err(e) = sqlx::query(
        "UPDATE auth_config SET ews_global_enabled = 0, ews_service_password_enc = NULL, \
             updated_at = datetime('now') WHERE id = 'singleton'",
    )
    .execute(&state.pool)
    .await
    {
        return internal_error_response("clear EWS service password", &e);
    }
    *live = None;
    drop(live);
    let removed = match ews_global::remove_all_managed_sources(&state.pool).await {
        Ok(n) => n,
        Err(e) => return internal_error_response("remove managed EWS sources", &e),
    };
    tracing::warn!(admin = %admin.user.email, removed, "admin: managed Exchange sources removed and service password cleared");
    Redirect::to(&format!(
        "/dashboard/admin?ews=removed&n={removed}#ews-global"
    ))
    .into_response()
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
