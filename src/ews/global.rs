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
    /// Optional override for the mailbox domain used in the impersonation
    /// header. When the AD UPN / calrs login domain differs from the domain of
    /// the mailbox's addresses (e.g. `example.local` vs `example.com`) and every user is
    /// on the same one, set this to the mailbox domain so impersonation
    /// resolves to a real mailbox.
    pub impersonation_domain: Option<String>,
}

impl EwsGlobalConfig {
    /// Compute the actual impersonation target address for a calrs user email.
    /// When `impersonation_domain` is set, the local part is kept and the
    /// domain is replaced (e.g. `alice@example.local` -> `alice@example.com`). Otherwise
    /// the address is used as-is.
    pub fn impersonation_target(&self, addr: Option<&str>) -> Option<String> {
        let addr = addr?.trim();
        if addr.is_empty() {
            return None;
        }
        match self
            .impersonation_domain
            .as_deref()
            .map(str::trim)
            .filter(|d| !d.is_empty())
        {
            Some(domain) => {
                let local = addr.split('@').next().unwrap_or(addr);
                Some(format!("{local}@{domain}"))
            }
            None => Some(addr.to_string()),
        }
    }
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
        Option<String>,
    ) = sqlx::query_as(
        "SELECT ews_global_enabled, ews_global_url, ews_service_username, \
                ews_service_password_enc, ews_lock_user_sources, ews_auto_provision, \
                ews_impersonation_domain \
         FROM auth_config WHERE id = 'singleton'",
    )
    .fetch_optional(pool)
    .await
    .ok()
    .flatten()?;

    let (enabled, url, username, password_enc, lock_user_sources, auto_provision, imp_domain) = row;
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
        impersonation_domain: imp_domain.filter(|s| !s.trim().is_empty()),
    })
}

/// Idempotently insert a managed EWS source row for one user.
///
/// Idempotent and race-safe: relies on the partial unique index
/// `idx_caldav_sources_managed_ews_unique` (one managed EWS source per
/// account), so concurrent provisioning (e.g. an OIDC login racing the
/// admin's "provision now") inserts at most one row. Returns `true` only when
/// a row was actually created.
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

    // One managed source per account (partial unique index). An existing row
    // follows the user's current address, which an OIDC login may have
    // changed; anything else on it is left alone.
    if !override_permits(pool, cfg, user_email).await {
        tracing::warn!(
            user_id = %user_id,
            "managed EWS source not provisioned: the mailbox-domain override only applies to addresses in the allowed email domains"
        );
        return Ok(false);
    }

    let id = uuid::Uuid::new_v4().to_string();
    let touched: Option<String> = sqlx::query_scalar(
        "INSERT INTO caldav_sources \
            (id, account_id, name, url, username, password_enc, provider_type, \
             managed, impersonate_email, enabled, auth_type) \
         VALUES (?, ?, 'Exchange (managed)', ?, ?, NULL, 'ews', 1, ?, 1, 'basic') \
         ON CONFLICT (account_id) WHERE managed = 1 AND provider_type = 'ews' \
         DO UPDATE SET impersonate_email = excluded.impersonate_email \
             WHERE caldav_sources.impersonate_email IS NOT excluded.impersonate_email \
         RETURNING id",
    )
    .bind(&id)
    .bind(&account_id)
    .bind(&cfg.url)
    .bind(&cfg.service_username)
    .bind(user_email)
    .fetch_optional(pool)
    .await?;

    match touched {
        Some(existing) if existing != id => {
            tracing::info!(user_id = %user_id, source_id = %existing, "managed EWS source now impersonates the user's new address");
            forget_mailbox(pool, &existing).await?;
            return Ok(false);
        }
        None => return Ok(false),
        Some(_) => {}
    }

    tracing::info!(
        user_id = %user_id,
        user_email = %user_email,
        source_id = %id,
        "provisioned managed EWS source"
    );
    Ok(true)
}

/// The mailbox-domain override keeps the local part of an address and swaps
/// the domain, so on an instance that accepts addresses from anywhere,
/// `ceo@gmail.com` would impersonate `ceo@corp.example`. It therefore only
/// applies to addresses within the instance's allowed email domains, and not
/// at all while that list is empty.
pub async fn override_permits(pool: &SqlitePool, cfg: &EwsGlobalConfig, email: &str) -> bool {
    if cfg.impersonation_domain.is_none() {
        return true;
    }
    let allowed: Option<String> =
        sqlx::query_scalar("SELECT allowed_email_domains FROM auth_config WHERE id = 'singleton'")
            .fetch_optional(pool)
            .await
            .ok()
            .flatten()
            .flatten();
    allowed.as_deref().is_some_and(|d| !d.trim().is_empty())
        && crate::auth::is_email_allowed(email, &allowed)
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

/// Drop everything a managed source cached from the mailbox it used to read:
/// its calendars (events and event-type links cascade with them) and its
/// write calendar. Call this when a source starts impersonating a different
/// mailbox, because Exchange folder ids belong to one mailbox: kept around,
/// the old calendars would never sync again while their events went on
/// blocking the user's availability, and a write calendar from the old
/// mailbox would send bookings to a folder this source no longer reads. The
/// next sync discovers the new mailbox's calendars, and the user picks a
/// write calendar again (the sources page already asks for one).
pub async fn forget_mailbox(pool: &SqlitePool, source_id: &str) -> Result<()> {
    sqlx::query("DELETE FROM calendars WHERE source_id = ?")
        .bind(source_id)
        .execute(pool)
        .await?;
    sqlx::query(
        "UPDATE caldav_sources SET write_calendar_href = NULL, last_synced = NULL, \
             last_full_sync = NULL WHERE id = ?",
    )
    .bind(source_id)
    .execute(pool)
    .await?;
    Ok(())
}

/// Every managed EWS source, so a change of impersonation domain can reset
/// them all: the override decides which mailbox each one reads.
pub async fn managed_source_ids(pool: &SqlitePool) -> Result<Vec<String>> {
    Ok(sqlx::query_scalar(
        "SELECT id FROM caldav_sources WHERE managed = 1 AND provider_type = 'ews'",
    )
    .fetch_all(pool)
    .await?)
}

/// The EWS client for one source row. Managed rows go through the global
/// service account and the impersonation header; the rest use their own
/// stored credentials. The managed flag is looked up by id so callers keep
/// selecting the columns they always did.
///
/// `Ok(None)` means a managed row whose global config is gone (the admin
/// switched Exchange off): there is nothing to reach, and that is not a
/// failure of the source.
pub async fn provider_for_source(
    pool: &SqlitePool,
    key: &[u8; 32],
    source_id: &str,
    url: &str,
    username: &str,
    password_enc: Option<&str>,
) -> Result<Option<Box<dyn crate::providers::CalendarProvider>>> {
    let (managed, impersonate_email): (i64, Option<String>) =
        sqlx::query_as("SELECT managed, impersonate_email FROM caldav_sources WHERE id = ?")
            .bind(source_id)
            .fetch_optional(pool)
            .await?
            .unwrap_or((0, None));
    if managed == 0 {
        let enc = password_enc.ok_or_else(|| anyhow::anyhow!("EWS source missing password"))?;
        let password = crate::crypto::decrypt_password(key, enc)
            .map_err(|e| anyhow::anyhow!("could not decrypt credentials: {e}"))?;
        return Ok(Some(Box::new(super::EwsProvider::new(
            url, username, &password,
        ))));
    }
    let Some(cfg) = load_ews_global_config(pool, key).await else {
        return Ok(None);
    };
    let mailbox = cfg
        .impersonation_target(impersonate_email.as_deref())
        .ok_or_else(|| anyhow::anyhow!("managed Exchange source has no mailbox to impersonate"))?;
    Ok(Some(Box::new(super::EwsProvider::with_impersonation(
        &cfg.url,
        &cfg.service_username,
        &cfg.service_password,
        &mailbox,
    ))))
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
    use std::str::FromStr;

    async fn memory_pool() -> SqlitePool {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(
                SqliteConnectOptions::from_str("sqlite::memory:")
                    .unwrap()
                    .foreign_keys(true),
            )
            .await
            .unwrap();
        crate::db::migrate(&pool).await.unwrap();
        pool
    }

    async fn seed_user(pool: &SqlitePool, email: &str) -> String {
        let user_id = uuid::Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO users (id, email, name, role, auth_provider, username, enabled) VALUES (?, ?, 'U', 'user', 'local', ?, 1)")
            .bind(&user_id)
            .bind(email)
            .bind(email.split('@').next().unwrap())
            .execute(pool)
            .await
            .unwrap();
        let account_id = uuid::Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO accounts (id, name, email, timezone, user_id) VALUES (?, 'U', ?, 'UTC', ?)")
            .bind(&account_id)
            .bind(email)
            .bind(&user_id)
            .execute(pool)
            .await
            .unwrap();
        user_id
    }

    #[tokio::test]
    async fn provision_is_idempotent_and_race_safe() {
        let pool = memory_pool().await;
        let cfg = cfg_with_domain(None);
        let user_id = seed_user(&pool, "alice@example.com").await;

        // First provision creates a row; the second is a no-op.
        assert!(
            provision_managed_ews_source_for_user(&pool, &cfg, &user_id, "alice@example.com")
                .await
                .unwrap()
        );
        assert!(
            !provision_managed_ews_source_for_user(&pool, &cfg, &user_id, "alice@example.com")
                .await
                .unwrap()
        );

        let count: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM caldav_sources WHERE managed = 1 AND provider_type = 'ews'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(count.0, 1, "exactly one managed EWS source per account");
    }

    fn cfg_with_domain(domain: Option<&str>) -> EwsGlobalConfig {
        EwsGlobalConfig {
            url: "https://mail.example.com/EWS/Exchange.asmx".to_string(),
            service_username: "svc".to_string(),
            service_password: "pw".to_string(),
            lock_user_sources: false,
            auto_provision: true,
            impersonation_domain: domain.map(str::to_string),
        }
    }

    #[test]
    fn impersonation_target_without_override_is_identity() {
        let cfg = cfg_with_domain(None);
        assert_eq!(
            cfg.impersonation_target(Some("alice@example.com"))
                .as_deref(),
            Some("alice@example.com")
        );
    }

    #[test]
    fn impersonation_target_rewrites_domain() {
        let cfg = cfg_with_domain(Some("example.com"));
        assert_eq!(
            cfg.impersonation_target(Some("alice@example.local"))
                .as_deref(),
            Some("alice@example.com")
        );
    }

    #[test]
    fn impersonation_target_rewrites_bare_local_part() {
        let cfg = cfg_with_domain(Some("example.com"));
        assert_eq!(
            cfg.impersonation_target(Some("alice")).as_deref(),
            Some("alice@example.com")
        );
    }

    #[test]
    fn impersonation_target_handles_none_and_empty() {
        let cfg = cfg_with_domain(Some("example.com"));
        assert_eq!(cfg.impersonation_target(None), None);
        assert_eq!(cfg.impersonation_target(Some("   ")), None);
    }
}
