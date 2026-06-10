//! Global EWS impersonation: one Exchange server + one service-account
//! holding the `ApplicationImpersonation` RBAC role, auto-provisioned for
//! every user via a "managed" row in `caldav_sources`.
//!
//! The shape mirrors `captcha`: a struct, a loader, and runtime caching via a
//! `RwLock<Option<EwsGlobalConfig>>` on `AppState`. `None` means the feature is
//! disabled — the sync loop falls back to per-user EWS / CalDAV sources.

use anyhow::Result;
use sqlx::SqlitePool;

#[derive(Clone)]
pub struct EwsGlobalConfig {
    pub url: String,
    pub service_username: String,
    pub service_password: String,
    pub lock_user_sources: bool,
    pub auto_provision: bool,
}

/// Hydrate the global EWS config from `auth_config`. Returns `None` when the
/// feature is disabled or any required field is missing — both cases mean
/// "behave exactly as if the feature didn't exist".
pub async fn load_ews_global_config(pool: &SqlitePool, key: &[u8; 32]) -> Option<EwsGlobalConfig> {
    let row: (
        i64,
        Option<String>,
        Option<String>,
        Option<String>,
        i64,
        i64,
    ) = sqlx::query_as(
        "SELECT ews_global_enabled, ews_global_url, ews_service_username, \
                ews_service_password_enc, ews_lock_user_sources, ews_auto_provision \
         FROM auth_config WHERE id = 'singleton'",
    )
    .fetch_optional(pool)
    .await
    .ok()
    .flatten()?;

    let (enabled, url, username, password_enc, lock_user_sources, auto_provision) = row;
    if enabled == 0 {
        return None;
    }
    let url = url.filter(|s| !s.trim().is_empty())?;
    let username = username.filter(|s| !s.trim().is_empty())?;
    let password_enc = password_enc.filter(|s| !s.trim().is_empty())?;
    let password = crate::crypto::decrypt_value(key, &password_enc).ok()?;

    Some(EwsGlobalConfig {
        url,
        service_username: username,
        service_password: password,
        lock_user_sources: lock_user_sources != 0,
        auto_provision: auto_provision != 0,
    })
}

/// Idempotently insert a managed EWS source row for one user.
///
/// Pre-checks for an existing `managed=1, provider_type='ews'` row on the
/// user's account so calling this on every login / config-save / user-create
/// is safe.
///
/// The row holds the connect URL + service-account username (informational —
/// the sync path reads the live config from the cache), `password_enc=NULL`,
/// `impersonate_email=user_email` and `managed=1`. The SOAP layer will inject
/// the `t:ExchangeImpersonation` header on every request.
pub async fn provision_managed_ews_source_for_user(
    pool: &SqlitePool,
    cfg: &EwsGlobalConfig,
    user_id: &str,
    user_email: &str,
) -> Result<bool> {
    let account: Option<(String,)> =
        sqlx::query_as("SELECT id FROM accounts WHERE user_id = ? LIMIT 1")
            .bind(user_id)
            .fetch_optional(pool)
            .await?;
    let Some((account_id,)) = account else {
        return Ok(false);
    };

    let existing: Option<(String,)> = sqlx::query_as(
        "SELECT id FROM caldav_sources \
         WHERE account_id = ? AND provider_type = 'ews' AND managed = 1 \
         LIMIT 1",
    )
    .bind(&account_id)
    .fetch_optional(pool)
    .await?;
    if existing.is_some() {
        return Ok(false);
    }

    let id = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO caldav_sources \
            (id, account_id, name, url, username, password_enc, provider_type, \
             managed, impersonate_email, enabled, auth_type) \
         VALUES (?, ?, 'Exchange (managed)', ?, ?, NULL, 'ews', 1, ?, 1, 'basic')",
    )
    .bind(&id)
    .bind(&account_id)
    .bind(&cfg.url)
    .bind(&cfg.service_username)
    .bind(user_email)
    .execute(pool)
    .await?;

    tracing::info!(
        user_id = %user_id,
        user_email = %user_email,
        source_id = %id,
        "provisioned managed EWS source"
    );
    Ok(true)
}

/// Provision a managed source for every enabled user. Returns the count of
/// rows newly inserted (existing rows are left untouched).
pub async fn provision_managed_ews_source_for_all_users(
    pool: &SqlitePool,
    cfg: &EwsGlobalConfig,
) -> Result<usize> {
    let users: Vec<(String, String)> =
        sqlx::query_as("SELECT id, email FROM users WHERE enabled = 1")
            .fetch_all(pool)
            .await?;
    let mut count = 0usize;
    for (id, email) in users {
        if provision_managed_ews_source_for_user(pool, cfg, &id, &email).await? {
            count += 1;
        }
    }
    Ok(count)
}
