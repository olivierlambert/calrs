//! Global EWS impersonation: one Exchange server + one service-account
//! holding the `ApplicationImpersonation` RBAC role, auto-provisioned for
//! every user via a "managed" row in `caldav_sources`.
//!
//! The shape mirrors `captcha`: a struct, a loader, and runtime caching via a
//! `RwLock<Option<EwsGlobalConfig>>` on `AppState`. `None` means the feature is
//! disabled: the sync loop falls back to per-user EWS / CalDAV sources.
//!
//! A managed source opens the mailbox its user's address names, so only an
//! address calrs can trust is provisioned (`users.email_trusted`): an account
//! an admin created, an SSO account whose address the identity provider
//! verified (re-checked at every login), or one an admin confirmed. Every
//! provisioning path goes through [`provision_managed_ews_source_for_user`],
//! which checks it, and [`provider_for_source`] checks it again before every
//! sync or write.

use anyhow::Result;
use sqlx::SqlitePool;

#[derive(Clone)]
pub struct EwsGlobalConfig {
    pub url: String,
    pub service_username: String,
    pub service_password: String,
    pub lock_user_sources: bool,
    pub auto_provision: bool,
    /// Optional mailbox-domain override, for an AD whose login addresses
    /// (`example.local`) are not the mailboxes' SMTP addresses
    /// (`example.com`). Only addresses in exactly its `from` domain are
    /// rewritten.
    pub domain_override: Option<DomainOverride>,
}

/// Rewrites addresses in the users' domain (`from`) to the mailbox domain
/// (`to`), keeping the local part. Both are lowercase, without `@`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DomainOverride {
    pub from: String,
    pub to: String,
}

impl DomainOverride {
    /// The override when both domains are set and valid, `None` otherwise.
    pub fn new(from: Option<&str>, to: Option<&str>) -> Option<Self> {
        let from = normalize_domain(from?).ok().flatten()?;
        let to = normalize_domain(to?).ok().flatten()?;
        Some(Self { from, to })
    }
}

/// A domain as typed in the admin card: trimmed, a leading `@` dropped,
/// lowercased. `Ok(None)` when empty; `Err` when it still holds an `@` or
/// whitespace, or has no dot, since it then cannot be a mail domain.
pub fn normalize_domain(raw: &str) -> Result<Option<String>, ()> {
    let d = raw.trim().trim_start_matches('@').to_ascii_lowercase();
    if d.is_empty() {
        return Ok(None);
    }
    if !d.is_ascii() || d.contains('@') || d.chars().any(char::is_whitespace) || !d.contains('.') {
        return Err(());
    }
    Ok(Some(d))
}

/// The mailbox a calrs address impersonates, lowercased (Exchange matches
/// SMTP addresses without regard to case). `None` unless the address is
/// plain ASCII (Exchange 2019 has no SMTPUTF8, and Unicode case rules would
/// fold `\u{212A}` (the Kelvin sign) onto `k`) and has exactly one `@` with
/// something on both sides: `ceo@x@corp.local` names no mailbox. With an
/// override, only an address in exactly its `from` domain gets the mailbox
/// domain; any other address is used as it is.
pub fn mailbox_for(addr: &str, domain_override: Option<&DomainOverride>) -> Option<String> {
    let addr = addr.trim();
    if !addr.is_ascii()
        || addr
            .chars()
            .any(|c| c.is_ascii_whitespace() || c.is_ascii_control())
    {
        return None;
    }
    let addr = addr.to_ascii_lowercase();
    let mut parts = addr.split('@');
    let (Some(local), Some(domain), None) = (parts.next(), parts.next(), parts.next()) else {
        return None;
    };
    if local.is_empty() || domain.is_empty() {
        return None;
    }
    match domain_override {
        Some(o) if domain == o.from => Some(format!("{local}@{}", o.to)),
        _ => Some(addr.clone()),
    }
}

impl EwsGlobalConfig {
    /// The mailbox impersonated for a calrs user address; see [`mailbox_for`].
    pub fn impersonation_target(&self, addr: &str) -> Option<String> {
        mailbox_for(addr, self.domain_override.as_ref())
    }
}

/// Whether a stored endpoint may receive the service account's password:
/// https, or plain http to the loopback interface only (a local test
/// server). The admin card already refuses anything but https; this keeps a
/// database written before that rule from sending it in clear.
pub fn endpoint_is_acceptable(url: &str) -> bool {
    let Ok(parsed) = reqwest::Url::parse(url.trim()) else {
        return false;
    };
    match parsed.scheme() {
        "https" => true,
        "http" => parsed.host_str().is_some_and(|h| {
            h.eq_ignore_ascii_case("localhost")
                || h.trim_matches(['[', ']'])
                    .parse::<std::net::IpAddr>()
                    .is_ok_and(|ip| ip.is_loopback())
        }),
        _ => false,
    }
}

/// Hydrate the global EWS config from `auth_config`. Returns `None` when the
/// feature is disabled or any required field is missing: both cases mean
/// "behave exactly as if the feature didn't exist".
pub async fn load_ews_global_config(pool: &SqlitePool, key: &[u8; 32]) -> Option<EwsGlobalConfig> {
    #[allow(clippy::type_complexity)]
    let row: (
        i64,
        Option<String>,
        Option<String>,
        Option<String>,
        i64,
        i64,
        Option<String>,
        Option<String>,
    ) = sqlx::query_as(
        "SELECT ews_global_enabled, ews_global_url, ews_service_username, \
                ews_service_password_enc, ews_lock_user_sources, ews_auto_provision, \
                ews_impersonation_from_domain, ews_impersonation_domain \
         FROM auth_config WHERE id = 'singleton'",
    )
    .fetch_optional(pool)
    .await
    .ok()
    .flatten()?;

    let (enabled, url, username, password_enc, lock_user_sources, auto_provision, from, to) = row;
    if enabled == 0 {
        return None;
    }
    let url = url.filter(|s| endpoint_is_acceptable(s))?;
    let username = username.filter(|s| !s.trim().is_empty())?;
    let password_enc = password_enc.filter(|s| !s.trim().is_empty())?;
    let password = crate::crypto::decrypt_value(key, &password_enc).ok()?;

    Some(EwsGlobalConfig {
        url,
        service_username: username,
        service_password: password,
        lock_user_sources: lock_user_sources != 0,
        auto_provision: auto_provision != 0,
        domain_override: DomainOverride::new(from.as_deref(), to.as_deref()),
    })
}

/// SQL condition, on a `caldav_sources cs` row, that is true unless the row is
/// a managed source with nothing to do: global Exchange off or incomplete, or
/// its user suspended or not trusted. Background candidates use it, since a
/// skipped sync records no attempt and the row would otherwise stay first in
/// line for ever. [`provider_for_source`] applies the same rules in Rust.
pub const ACTIVE_SOURCE_SQL: &str = "(cs.managed = 0 OR (\
     EXISTS (SELECT 1 FROM auth_config ac WHERE ac.id = 'singleton' \
             AND ac.ews_global_enabled = 1 \
             AND COALESCE(ac.ews_global_url, '') != '' \
             AND COALESCE(ac.ews_service_username, '') != '' \
             AND COALESCE(ac.ews_service_password_enc, '') != '') \
     AND EXISTS (SELECT 1 FROM accounts ma JOIN users mu ON mu.id = ma.user_id \
                 WHERE ma.id = cs.account_id AND mu.enabled = 1 AND mu.email_trusted = 1)))";

/// What [`provision_managed_ews_source_for_user`] did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Provisioned {
    /// A managed source was created.
    Created,
    /// The existing source now impersonates the user's current mailbox; what
    /// it cached from the previous one was dropped.
    Updated,
    /// The existing source already reads the right mailbox.
    Unchanged,
    /// No such user, or no account to attach a source to.
    NoAccount,
    /// The user is suspended: nothing is created, an existing source is kept
    /// (and not synced) until they are enabled again.
    Disabled,
    /// The address is not trusted. Any managed source the user had is gone.
    NotTrusted,
    /// The address names no mailbox (not exactly one `@`). Any managed
    /// source the user had is gone.
    InvalidAddress,
    /// Another user's managed source already impersonates this mailbox. Any
    /// managed source this user had is gone.
    Conflict,
    /// Nothing was written: global Exchange was switched off, or the user
    /// suspended or no longer trusted, while this ran.
    Inactive,
}

/// Name stored for the `n`-th (0-based) calendar of a managed source instead
/// of its folder name, which is mailbox content too ("Salary reviews").
pub fn neutral_calendar_name(n: usize) -> String {
    format!("Exchange calendar {}", n + 1)
}

/// Per-source lock held while a managed source changes mailbox and while an
/// EWS sync stores what it fetched, so a sync that started on the old
/// mailbox can never write its calendars back after [`forget_mailbox`].
pub fn mailbox_lock(source_id: &str) -> std::sync::Arc<tokio::sync::Mutex<()>> {
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex, OnceLock};
    static LOCKS: OnceLock<Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>> = OnceLock::new();
    LOCKS
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .entry(source_id.to_string())
        .or_default()
        .clone()
}

/// Why a managed source is idle, as a message key for the sources page;
/// `None` when it is not idle (or not managed).
pub async fn idle_reason(
    pool: &SqlitePool,
    key: &[u8; 32],
    source_id: &str,
) -> Option<&'static str> {
    if !is_managed(pool, source_id).await {
        return None;
    }
    if load_ews_global_config(pool, key).await.is_none() {
        return Some("sources-managed-global-disabled");
    }
    let user: Option<(bool, bool)> = sqlx::query_as(
        "SELECT u.enabled, u.email_trusted FROM caldav_sources cs \
         JOIN accounts a ON a.id = cs.account_id JOIN users u ON u.id = a.user_id \
         WHERE cs.id = ?",
    )
    .bind(source_id)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten();
    match user {
        Some((false, _)) | None => Some("sources-managed-user-suspended"),
        Some((true, false)) => Some("sources-managed-user-untrusted"),
        Some((true, true)) => None,
    }
}

impl Provisioned {
    pub fn created(self) -> bool {
        self == Provisioned::Created
    }
}

/// Give one user a managed EWS source, or bring the one they have in step
/// with their current address. The address is read from `users`, never taken
/// from the caller, and only a trusted one (`users.email_trusted`) is used:
/// see the module docs.
///
/// Idempotent and race-safe: the partial unique indexes allow one managed
/// EWS source per account and one per mailbox, so an OIDC login racing the
/// admin's "provision now" inserts at most one row, and two users can never
/// end up reading the same mailbox (`ceo@corp.com` and `CEO@corp.com`, or two
/// addresses the domain override maps onto one).
///
/// The row holds the connect URL + service-account username (informational:
/// the sync path reads the live config from the cache), `password_enc=NULL`,
/// `impersonate_email=<mailbox>` and `managed=1`. The SOAP layer injects
/// the `t:ExchangeImpersonation` header on every request.
pub async fn provision_managed_ews_source_for_user(
    pool: &SqlitePool,
    cfg: &EwsGlobalConfig,
    user_id: &str,
) -> Result<Provisioned> {
    let user: Option<(String, bool, bool, bool)> = sqlx::query_as(
        "SELECT email, enabled, email_trusted, email_verified_by_idp FROM users WHERE id = ?",
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await?;
    let Some((email, enabled, trusted, idp_verified)) = user else {
        return Ok(Provisioned::NoAccount);
    };
    if !trusted {
        if remove_managed_source(pool, user_id).await? > 0 {
            tracing::warn!(user_id = %user_id, "managed EWS source removed: the user's address is not trusted");
        }
        return Ok(Provisioned::NotTrusted);
    }
    if !enabled {
        return Ok(Provisioned::Disabled);
    }
    let Some(mailbox) = cfg.impersonation_target(&email) else {
        remove_managed_source(pool, user_id).await?;
        tracing::warn!(user_id = %user_id, "managed EWS source not provisioned: the address names no mailbox");
        return Ok(Provisioned::InvalidAddress);
    };

    let account: Option<(String,)> =
        sqlx::query_as("SELECT id FROM accounts WHERE user_id = ? LIMIT 1")
            .bind(user_id)
            .fetch_optional(pool)
            .await?;
    let Some((account_id,)) = account else {
        return Ok(Provisioned::NoAccount);
    };

    // An existing row may move to another mailbox below: hold its lock, so a
    // sync of the old mailbox cannot store its results afterwards.
    let existing: Option<String> = sqlx::query_scalar(
        "SELECT id FROM caldav_sources \
         WHERE account_id = ? AND managed = 1 AND provider_type = 'ews'",
    )
    .bind(&account_id)
    .fetch_optional(pool)
    .await?;
    let lock = existing.as_deref().map(mailbox_lock);
    let _guard = match &lock {
        Some(l) => Some(l.lock().await),
        None => None,
    };

    // One mailbox, one user. Checked up front for a clear outcome; the
    // unique index catches a concurrent provisioning of the same mailbox. An
    // owner the identity provider verified outranks an account that only an
    // admin vouched for: that account's source goes, the owner's is created.
    let holder: Option<(String, String, String, bool)> = sqlx::query_as(
        "SELECT cs.id, a.user_id, u.email, u.email_verified_by_idp FROM caldav_sources cs \
         JOIN accounts a ON a.id = cs.account_id JOIN users u ON u.id = a.user_id \
         WHERE cs.managed = 1 AND cs.provider_type = 'ews' \
           AND cs.impersonate_email = ? AND cs.account_id != ?",
    )
    .bind(&mailbox)
    .bind(&account_id)
    .fetch_optional(pool)
    .await?;
    // The eviction and the insert below are one transaction: if the insert
    // writes nothing (feature switched off meanwhile), the holder keeps it.
    let mut takeover: Option<(String, String, String)> = None;
    let holder_lock;
    let _holder_guard;
    if let Some((holder_source, holder_user, holder_email, holder_verified)) = holder {
        if !idp_verified || holder_verified {
            return conflict(pool, user_id).await;
        }
        holder_lock = mailbox_lock(&holder_source);
        _holder_guard = holder_lock.lock().await;
        takeover = Some((holder_source, holder_user, holder_email));
    }
    let mut tx = pool.begin().await?;
    if let Some((holder_source, _, _)) = &takeover {
        sqlx::query("DELETE FROM caldav_sources WHERE id = ?")
            .bind(holder_source)
            .execute(&mut *tx)
            .await?;
    }

    // An existing row follows the user's current mailbox, which an OIDC
    // login or a new domain override may have changed; anything else on it
    // is left alone. The feature and the user are checked again in the same
    // statement, so a concurrent switch-off or suspension cannot be undone
    // by a provisioning that read the old state.
    let id = uuid::Uuid::new_v4().to_string();
    let touched: Result<Option<String>, sqlx::Error> = sqlx::query_scalar(&format!(
        "INSERT INTO caldav_sources \
            (id, account_id, name, url, username, password_enc, provider_type, \
             managed, impersonate_email, enabled, auth_type) \
         SELECT ?, ?, 'Exchange (managed)', ?, ?, NULL, 'ews', 1, ?, 1, 'basic' \
         WHERE {FEATURE_ON_SQL} \
           AND EXISTS (SELECT 1 FROM users WHERE id = ? AND enabled = 1 AND email_trusted = 1) \
         ON CONFLICT (account_id) WHERE managed = 1 AND provider_type = 'ews' \
         DO UPDATE SET impersonate_email = excluded.impersonate_email \
             WHERE caldav_sources.impersonate_email IS NOT excluded.impersonate_email \
         RETURNING id"
    ))
    .bind(&id)
    .bind(&account_id)
    .bind(&cfg.url)
    .bind(&cfg.service_username)
    .bind(&mailbox)
    .bind(user_id)
    .fetch_optional(&mut *tx)
    .await;
    let touched = match touched {
        Ok(t) => t,
        Err(sqlx::Error::Database(e)) if e.is_unique_violation() => {
            tx.rollback().await?;
            return conflict(pool, user_id).await;
        }
        Err(e) => return Err(e.into()),
    };
    if let Some((_, holder_user, holder_email)) = &takeover {
        let Some(taker) = touched.as_deref() else {
            // Nothing written: the holder keeps the mailbox.
            tx.rollback().await?;
            return Ok(Provisioned::Inactive);
        };
        sqlx::query(
            "UPDATE caldav_sources SET taken_over_from = ?, taken_over_at = datetime('now') \
             WHERE id = ?",
        )
        .bind(holder_email)
        .bind(taker)
        .execute(&mut *tx)
        .await?;
        tracing::warn!(
            user_id = %user_id,
            previous_user_id = %holder_user,
            mailbox = %mailbox,
            "managed EWS source taken over: the identity provider verified this owner, the previous holder was only confirmed by an admin"
        );
    }
    tx.commit().await?;

    match touched {
        Some(existing) if existing != id => {
            tracing::info!(user_id = %user_id, source_id = %existing, "managed EWS source now impersonates the user's new address");
            forget_mailbox_held(pool, &existing).await?;
            Ok(Provisioned::Updated)
        }
        None => {
            let current: Option<String> = sqlx::query_scalar(
                "SELECT impersonate_email FROM caldav_sources \
                 WHERE account_id = ? AND managed = 1 AND provider_type = 'ews'",
            )
            .bind(&account_id)
            .fetch_optional(pool)
            .await?
            .flatten();
            if current.as_deref() == Some(mailbox.as_str()) {
                Ok(Provisioned::Unchanged)
            } else {
                Ok(Provisioned::Inactive)
            }
        }
        Some(_) => {
            tracing::info!(
                user_id = %user_id,
                mailbox = %mailbox,
                source_id = %id,
                "provisioned managed EWS source"
            );
            Ok(Provisioned::Created)
        }
    }
}

/// SQL condition true while global Exchange is on and complete.
const FEATURE_ON_SQL: &str = "EXISTS (SELECT 1 FROM auth_config ac WHERE ac.id = 'singleton' \
     AND ac.ews_global_enabled = 1 \
     AND COALESCE(ac.ews_global_url, '') != '' \
     AND COALESCE(ac.ews_service_username, '') != '' \
     AND COALESCE(ac.ews_service_password_enc, '') != '')";

/// The mailbox is another user's: this user keeps no managed source, since
/// one left in place would read the mailbox of an address they no longer
/// have.
async fn conflict(pool: &SqlitePool, user_id: &str) -> Result<Provisioned> {
    // The caller holds this user's source lock.
    remove_managed_source_held(pool, user_id).await?;
    tracing::warn!(user_id = %user_id, "managed EWS source not provisioned: another user's source already impersonates this mailbox");
    Ok(Provisioned::Conflict)
}

/// Delete a user's managed source, under its [`mailbox_lock`] so no sync
/// stores anything for it afterwards; its calendars and cached events
/// cascade. Returns how many rows went (0 or 1).
pub async fn remove_managed_source(pool: &SqlitePool, user_id: &str) -> Result<u64> {
    let ids: Vec<String> = sqlx::query_scalar(
        "SELECT cs.id FROM caldav_sources cs JOIN accounts a ON a.id = cs.account_id \
         WHERE a.user_id = ? AND cs.managed = 1 AND cs.provider_type = 'ews'",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;
    let mut removed = 0;
    for id in ids {
        let lock = mailbox_lock(&id);
        let _guard = lock.lock().await;
        removed += delete_source(pool, &id).await?;
    }
    Ok(removed)
}

/// [`remove_managed_source`] for a caller already holding the lock.
async fn remove_managed_source_held(pool: &SqlitePool, user_id: &str) -> Result<u64> {
    Ok(sqlx::query(
        "DELETE FROM caldav_sources WHERE managed = 1 AND provider_type = 'ews' \
         AND account_id IN (SELECT id FROM accounts WHERE user_id = ?)",
    )
    .bind(user_id)
    .execute(pool)
    .await?
    .rows_affected())
}

/// Delete every managed source whose user is not trusted (or is gone), so no
/// mailbox stays attached to an address calrs cannot vouch for.
pub async fn remove_untrusted_managed_sources(pool: &SqlitePool) -> Result<u64> {
    const UNTRUSTED: &str = "managed = 1 AND provider_type = 'ews' \
         AND NOT EXISTS (SELECT 1 FROM accounts a JOIN users u ON u.id = a.user_id \
                         WHERE a.id = caldav_sources.account_id AND u.email_trusted = 1)";
    let ids: Vec<String> =
        sqlx::query_scalar(&format!("SELECT id FROM caldav_sources WHERE {UNTRUSTED}"))
            .fetch_all(pool)
            .await?;
    let mut removed = 0;
    for id in ids {
        let lock = mailbox_lock(&id);
        let _guard = lock.lock().await;
        removed += sqlx::query(&format!(
            "DELETE FROM caldav_sources WHERE id = ? AND {UNTRUSTED}"
        ))
        .bind(&id)
        .execute(pool)
        .await?
        .rows_affected();
    }
    Ok(removed)
}

/// After the admin changed the domain override: an address trusted on an
/// admin's word only (not verified by the identity provider) that now maps
/// to another mailbox is no longer trusted, since the admin confirmed it for
/// the mailbox it opened then. It shows up again in the list of addresses to
/// confirm, with its new mailbox. Returns how many addresses that was.
pub async fn untrust_remapped(
    pool: &SqlitePool,
    old: Option<&DomainOverride>,
    new: Option<&DomainOverride>,
) -> Result<u64> {
    let users: Vec<(String, String)> = sqlx::query_as(
        "SELECT id, email FROM users WHERE email_trusted = 1 AND email_verified_by_idp = 0",
    )
    .fetch_all(pool)
    .await?;
    let mut count = 0;
    for (id, email) in users {
        if mailbox_for(&email, old) == mailbox_for(&email, new) {
            continue;
        }
        sqlx::query(
            "UPDATE users SET email_trusted = 0, updated_at = datetime('now') WHERE id = ?",
        )
        .bind(&id)
        .execute(pool)
        .await?;
        tracing::warn!(user_id = %id, "address to confirm again: the domain override now maps it to another mailbox");
        count += 1;
    }
    Ok(count)
}

/// Provision a managed source for every enabled, trusted user, oldest
/// account first (so of two addresses mapping onto one mailbox, the one that
/// was there first keeps it). Returns the count of rows newly inserted.
pub async fn provision_managed_ews_source_for_all_users(
    pool: &SqlitePool,
    cfg: &EwsGlobalConfig,
) -> Result<usize> {
    remove_untrusted_managed_sources(pool).await?;
    let users: Vec<String> = sqlx::query_scalar(
        "SELECT id FROM users WHERE enabled = 1 AND email_trusted = 1 ORDER BY created_at, id",
    )
    .fetch_all(pool)
    .await?;
    let mut count = 0usize;
    for id in users {
        if provision_managed_ews_source_for_user(pool, cfg, &id)
            .await?
            .created()
        {
            count += 1;
        }
    }
    Ok(count)
}

/// Mark a user's address as trusted: an admin confirmed that the person owns
/// it. Returns `false` when there is no such user.
pub async fn trust_email(pool: &SqlitePool, user_id: &str) -> Result<bool> {
    Ok(
        sqlx::query(
            "UPDATE users SET email_trusted = 1, updated_at = datetime('now') WHERE id = ?",
        )
        .bind(user_id)
        .execute(pool)
        .await?
        .rows_affected()
            == 1,
    )
}

/// After an OIDC login, which has just set `users.email_trusted` from the
/// `email_verified` claim: an untrusted address loses its managed source
/// whatever the config, and a trusted one is provisioned when auto-provision
/// is on, or kept in step with the address when it already has a source.
pub async fn after_oidc_login(pool: &SqlitePool, cfg: Option<&EwsGlobalConfig>, user_id: &str) {
    let trusted: bool = sqlx::query_scalar("SELECT email_trusted FROM users WHERE id = ?")
        .bind(user_id)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten()
        .unwrap_or(false);
    if !trusted {
        match remove_managed_source(pool, user_id).await {
            Ok(n) if n > 0 => {
                tracing::warn!(user_id = %user_id, "managed EWS source removed: the identity provider did not verify the address")
            }
            Ok(_) => {}
            Err(e) => {
                tracing::error!(user_id = %user_id, error = %e, "could not remove the managed EWS source of an unverified address")
            }
        }
        return;
    }
    let Some(cfg) = cfg else {
        return;
    };
    let has_source: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM caldav_sources cs JOIN accounts a ON a.id = cs.account_id \
         WHERE a.user_id = ? AND cs.managed = 1 AND cs.provider_type = 'ews')",
    )
    .bind(user_id)
    .fetch_one(pool)
    .await
    .unwrap_or(false);
    if cfg.auto_provision || has_source {
        if let Err(e) = provision_managed_ews_source_for_user(pool, cfg, user_id).await {
            tracing::warn!(error = %e, "failed to provision managed EWS source on OIDC login");
        }
    }
}

/// Point every managed source at the mailbox its user's address maps to
/// under `domain_override`, after the admin changed it. A source whose user
/// is not trusted, whose address names no mailbox, or whose new mailbox is
/// already another user's, is removed. What a source cached is dropped by
/// the caller ([`forget_mailbox`]).
pub async fn retarget_managed_sources(
    pool: &SqlitePool,
    domain_override: Option<&DomainOverride>,
) -> Result<u64> {
    remove_untrusted_managed_sources(pool).await?;
    // Owners the identity provider verified first, then oldest first: of two
    // sources moving onto one mailbox, the verified owner gets it.
    let rows: Vec<(String, String, String, bool)> = sqlx::query_as(
        "SELECT cs.id, u.email, COALESCE(cs.impersonate_email, ''), u.email_verified_by_idp \
         FROM caldav_sources cs \
         JOIN accounts a ON a.id = cs.account_id JOIN users u ON u.id = a.user_id \
         WHERE cs.managed = 1 AND cs.provider_type = 'ews' \
         ORDER BY u.email_verified_by_idp DESC, u.created_at, u.id",
    )
    .fetch_all(pool)
    .await?;
    let mut lost = 0;
    for (id, email, current, verified) in rows {
        let lock = mailbox_lock(&id);
        let _guard = lock.lock().await;
        let Some(mailbox) = mailbox_for(&email, domain_override) else {
            delete_source(pool, &id).await?;
            continue;
        };
        if mailbox == current {
            continue;
        }
        let holder: Option<(String, String, bool)> = sqlx::query_as(
            "SELECT cs.id, u.email, u.email_verified_by_idp FROM caldav_sources cs \
             JOIN accounts a ON a.id = cs.account_id JOIN users u ON u.id = a.user_id \
             WHERE cs.managed = 1 AND cs.provider_type = 'ews' \
               AND cs.impersonate_email = ? AND cs.id != ?",
        )
        .bind(&mailbox)
        .bind(&id)
        .fetch_optional(pool)
        .await?;
        let holder_lock;
        let _holder_guard;
        let mut tx = pool.begin().await?;
        if let Some((holder_id, holder_email, holder_verified)) = &holder {
            if !verified || *holder_verified {
                tx.rollback().await?;
                tracing::warn!(source_id = %id, "managed EWS source removed: its new mailbox is already another user's");
                delete_source(pool, &id).await?;
                continue;
            }
            holder_lock = mailbox_lock(holder_id);
            _holder_guard = holder_lock.lock().await;
            sqlx::query("DELETE FROM caldav_sources WHERE id = ?")
                .bind(holder_id)
                .execute(&mut *tx)
                .await?;
            sqlx::query(
                "UPDATE caldav_sources SET taken_over_from = ?, taken_over_at = datetime('now') \
                 WHERE id = ?",
            )
            .bind(holder_email)
            .bind(&id)
            .execute(&mut *tx)
            .await?;
            tracing::warn!(source_id = %id, mailbox = %mailbox, "managed EWS source taken over on retarget: the identity provider verified this owner, the previous holder was only confirmed by an admin");
        }
        let moved = sqlx::query("UPDATE caldav_sources SET impersonate_email = ? WHERE id = ?")
            .bind(&mailbox)
            .bind(&id)
            .execute(&mut *tx)
            .await;
        match moved {
            Ok(_) => {
                tx.commit().await?;
                lost += forget_mailbox_held(pool, &id).await?;
            }
            Err(sqlx::Error::Database(e)) if e.is_unique_violation() => {
                tx.rollback().await?;
                tracing::warn!(source_id = %id, "managed EWS source removed: its new mailbox is already another user's");
                delete_source(pool, &id).await?;
            }
            Err(e) => return Err(e.into()),
        }
    }
    Ok(lost)
}

/// Delete one source row; the caller holds its [`mailbox_lock`].
async fn delete_source(pool: &SqlitePool, source_id: &str) -> Result<u64> {
    Ok(sqlx::query("DELETE FROM caldav_sources WHERE id = ?")
        .bind(source_id)
        .execute(pool)
        .await?
        .rows_affected())
}

/// Remove every managed source (their calendars and cached events cascade).
/// Bookings already written into the mailboxes stay there. Returns how many
/// sources went.
pub async fn remove_all_managed_sources(pool: &SqlitePool) -> Result<u64> {
    let mut removed = 0;
    for id in managed_source_ids(pool).await? {
        let lock = mailbox_lock(&id);
        let _guard = lock.lock().await;
        removed += delete_source(pool, &id).await?;
    }
    Ok(removed)
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
///
/// Returns how many event types lost a calendar selection with it (each is
/// logged), for the admin's notice.
pub async fn forget_mailbox(pool: &SqlitePool, source_id: &str) -> Result<u64> {
    let lock = mailbox_lock(source_id);
    let _guard = lock.lock().await;
    forget_mailbox_held(pool, source_id).await
}

/// [`forget_mailbox`] for a caller already holding the source's
/// [`mailbox_lock`].
async fn forget_mailbox_held(pool: &SqlitePool, source_id: &str) -> Result<u64> {
    let event_types: Vec<String> = sqlx::query_scalar(
        "SELECT DISTINCT etc.event_type_id FROM event_type_calendars etc \
         JOIN calendars c ON c.id = etc.calendar_id WHERE c.source_id = ?",
    )
    .bind(source_id)
    .fetch_all(pool)
    .await?;
    if !event_types.is_empty() {
        tracing::warn!(
            source_id = %source_id,
            event_types = ?event_types,
            "managed EWS source reads another mailbox: these event types lost their calendar selection"
        );
    }
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
    Ok(event_types.len() as u64)
}

/// Every managed EWS source, so a change of endpoint or domain override can
/// reset them all: those decide which mailbox each one reads.
pub async fn managed_source_ids(pool: &SqlitePool) -> Result<Vec<String>> {
    Ok(sqlx::query_scalar(
        "SELECT id FROM caldav_sources WHERE managed = 1 AND provider_type = 'ews'",
    )
    .fetch_all(pool)
    .await?)
}

/// Whether `source_id` is a managed source (unknown ids read as not managed).
pub async fn is_managed(pool: &SqlitePool, source_id: &str) -> bool {
    sqlx::query_scalar::<_, i64>("SELECT managed FROM caldav_sources WHERE id = ?")
        .bind(source_id)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten()
        .is_some_and(|m| m != 0)
}

/// The EWS client for one source row. Managed rows go through the global
/// service account and the impersonation header; the rest use their own
/// stored credentials. The managed flag is looked up by id so callers keep
/// selecting the columns they always did.
///
/// `Ok(None)` means a managed row with nothing to do, which is not a failure
/// of the source: global Exchange is off or incomplete, or its user is
/// suspended or their address is not trusted. Callers skip it without
/// recording anything.
pub async fn provider_for_source(
    pool: &SqlitePool,
    key: &[u8; 32],
    source_id: &str,
    url: &str,
    username: &str,
    password_enc: Option<&str>,
) -> Result<Option<Box<dyn crate::providers::CalendarProvider>>> {
    #[allow(clippy::type_complexity)]
    let row: Option<(
        i64,
        Option<String>,
        Option<String>,
        Option<bool>,
        Option<bool>,
    )> = sqlx::query_as(
        "SELECT cs.managed, cs.impersonate_email, u.email, u.enabled, u.email_trusted \
             FROM caldav_sources cs \
             LEFT JOIN accounts a ON a.id = cs.account_id \
             LEFT JOIN users u ON u.id = a.user_id \
             WHERE cs.id = ?",
    )
    .bind(source_id)
    .fetch_optional(pool)
    .await?;
    let (managed, impersonate_email, user_email, user_enabled, user_trusted) =
        row.unwrap_or((0, None, None, None, None));
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
    if user_enabled != Some(true) || user_trusted != Some(true) {
        tracing::info!(source_id = %source_id, "managed Exchange source skipped: its user is suspended or their address is not trusted");
        return Ok(None);
    }
    // The mailbox is re-derived from the user's current address on every
    // build, not only when the row was provisioned: a row that no longer
    // matches (an address changed outside a login) must not go on reading
    // the old mailbox. An error, not a skip, so the admin table shows it.
    let expected = user_email
        .as_deref()
        .and_then(|e| cfg.impersonation_target(e))
        .ok_or_else(|| anyhow::anyhow!("the user's address names no Exchange mailbox"))?;
    if impersonate_email.as_deref() != Some(expected.as_str()) {
        anyhow::bail!(
            "managed Exchange source out of date: the user's address now maps to another mailbox (provision again)"
        );
    }
    Ok(Some(Box::new(super::EwsProvider::with_impersonation(
        &cfg.url,
        &cfg.service_username,
        &cfg.service_password,
        &expected,
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
        // Global Exchange on, as provisioning checks it when it writes.
        sqlx::query(
            "UPDATE auth_config SET ews_global_enabled = 1, \
             ews_global_url = 'https://mail.example.com/EWS/Exchange.asmx', \
             ews_service_username = 'svc', ews_service_password_enc = 'x' WHERE id = 'singleton'",
        )
        .execute(&pool)
        .await
        .unwrap();
        pool
    }

    /// A user with an account, trusted or not.
    async fn seed_user(pool: &SqlitePool, email: &str, trusted: bool) -> String {
        let user_id = uuid::Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO users (id, email, name, role, auth_provider, username, enabled, email_trusted) VALUES (?, ?, 'U', 'user', 'local', ?, 1, ?)")
            .bind(&user_id)
            .bind(email)
            .bind(uuid::Uuid::new_v4().to_string())
            .bind(trusted)
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

    async fn managed_mailbox(pool: &SqlitePool, user_id: &str) -> Option<String> {
        sqlx::query_scalar(
            "SELECT cs.impersonate_email FROM caldav_sources cs \
             JOIN accounts a ON a.id = cs.account_id \
             WHERE a.user_id = ? AND cs.managed = 1",
        )
        .bind(user_id)
        .fetch_optional(pool)
        .await
        .unwrap()
        .flatten()
    }

    fn cfg_with(domain_override: Option<(&str, &str)>) -> EwsGlobalConfig {
        EwsGlobalConfig {
            url: "https://mail.example.com/EWS/Exchange.asmx".to_string(),
            service_username: "svc".to_string(),
            service_password: "pw".to_string(),
            lock_user_sources: false,
            auto_provision: true,
            domain_override: domain_override.map(|(from, to)| DomainOverride {
                from: from.to_string(),
                to: to.to_string(),
            }),
        }
    }

    #[tokio::test]
    async fn provision_is_idempotent_and_race_safe() {
        let pool = memory_pool().await;
        let cfg = cfg_with(None);
        let user_id = seed_user(&pool, "alice@example.com", true).await;

        // First provision creates a row; the second is a no-op.
        assert_eq!(
            provision_managed_ews_source_for_user(&pool, &cfg, &user_id)
                .await
                .unwrap(),
            Provisioned::Created
        );
        assert_eq!(
            provision_managed_ews_source_for_user(&pool, &cfg, &user_id)
                .await
                .unwrap(),
            Provisioned::Unchanged
        );

        let count: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM caldav_sources WHERE managed = 1 AND provider_type = 'ews'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(count.0, 1, "exactly one managed EWS source per account");
    }

    /// The core rule: an address nobody vouched for is never provisioned,
    /// whichever path asks, and a source it had goes away.
    #[tokio::test]
    async fn an_untrusted_address_is_never_provisioned() {
        let pool = memory_pool().await;
        let cfg = cfg_with(None);
        let user_id = seed_user(&pool, "ceo@example.com", false).await;
        assert_eq!(
            provision_managed_ews_source_for_user(&pool, &cfg, &user_id)
                .await
                .unwrap(),
            Provisioned::NotTrusted
        );
        assert_eq!(
            provision_managed_ews_source_for_all_users(&pool, &cfg)
                .await
                .unwrap(),
            0
        );
        assert_eq!(managed_mailbox(&pool, &user_id).await, None);

        // Trusted, then no longer: the source goes.
        trust_email(&pool, &user_id).await.unwrap();
        provision_managed_ews_source_for_user(&pool, &cfg, &user_id)
            .await
            .unwrap();
        assert_eq!(
            managed_mailbox(&pool, &user_id).await.as_deref(),
            Some("ceo@example.com")
        );
        sqlx::query("UPDATE users SET email_trusted = 0 WHERE id = ?")
            .bind(&user_id)
            .execute(&pool)
            .await
            .unwrap();
        provision_managed_ews_source_for_all_users(&pool, &cfg)
            .await
            .unwrap();
        assert_eq!(managed_mailbox(&pool, &user_id).await, None);
    }

    /// Exchange matches addresses without regard to case, so a case variant
    /// is the same mailbox: the second user is refused, and the mailbox is
    /// stored lowercased.
    #[tokio::test]
    async fn a_second_user_on_the_same_mailbox_is_refused() {
        let pool = memory_pool().await;
        let cfg = cfg_with(None);
        let first = seed_user(&pool, "CEO@Example.com", true).await;
        let second = seed_user(&pool, "ceo@example.com", true).await;
        assert_eq!(
            provision_managed_ews_source_for_user(&pool, &cfg, &first)
                .await
                .unwrap(),
            Provisioned::Created
        );
        assert_eq!(
            managed_mailbox(&pool, &first).await.as_deref(),
            Some("ceo@example.com")
        );
        assert_eq!(
            provision_managed_ews_source_for_user(&pool, &cfg, &second)
                .await
                .unwrap(),
            Provisioned::Conflict
        );
        assert_eq!(managed_mailbox(&pool, &second).await, None);

        // Same through the override: jsmith@corp.local maps onto the
        // mailbox jsmith@corp.com already reads.
        let cfg = cfg_with(Some(("corp.local", "corp.com")));
        let owner = seed_user(&pool, "jsmith@corp.com", true).await;
        let alias = seed_user(&pool, "jsmith@corp.local", true).await;
        provision_managed_ews_source_for_user(&pool, &cfg, &owner)
            .await
            .unwrap();
        assert_eq!(
            provision_managed_ews_source_for_user(&pool, &cfg, &alias)
                .await
                .unwrap(),
            Provisioned::Conflict
        );
        assert_eq!(
            managed_mailbox(&pool, &owner).await.as_deref(),
            Some("jsmith@corp.com")
        );
        assert_eq!(managed_mailbox(&pool, &alias).await, None);
    }

    #[tokio::test]
    async fn a_suspended_user_is_not_provisioned() {
        let pool = memory_pool().await;
        let cfg = cfg_with(None);
        let user_id = seed_user(&pool, "bob@example.com", true).await;
        sqlx::query("UPDATE users SET enabled = 0 WHERE id = ?")
            .bind(&user_id)
            .execute(&pool)
            .await
            .unwrap();
        assert_eq!(
            provision_managed_ews_source_for_user(&pool, &cfg, &user_id)
                .await
                .unwrap(),
            Provisioned::Disabled
        );
        assert_eq!(managed_mailbox(&pool, &user_id).await, None);
    }

    #[test]
    fn impersonation_target_without_override_is_the_lowercased_address() {
        let cfg = cfg_with(None);
        assert_eq!(
            cfg.impersonation_target(" Alice@Example.com ").as_deref(),
            Some("alice@example.com")
        );
    }

    #[test]
    fn the_override_only_rewrites_its_from_domain() {
        let cfg = cfg_with(Some(("example.local", "example.com")));
        assert_eq!(
            cfg.impersonation_target("alice@example.local").as_deref(),
            Some("alice@example.com")
        );
        assert_eq!(
            cfg.impersonation_target("Alice@EXAMPLE.local").as_deref(),
            Some("alice@example.com")
        );
        // Another allowed domain keeps its own mailbox: jsmith@subsidiary.com
        // must not become jsmith@example.com.
        assert_eq!(
            cfg.impersonation_target("jsmith@subsidiary.com").as_deref(),
            Some("jsmith@subsidiary.com")
        );
        // Nor does a subdomain of the from-domain.
        assert_eq!(
            cfg.impersonation_target("a@eu.example.local").as_deref(),
            Some("a@eu.example.local")
        );
    }

    #[test]
    fn an_address_without_exactly_one_at_names_no_mailbox() {
        let cfg = cfg_with(Some(("corp.local", "corp.com")));
        for addr in [
            "ceo@x@corp.local",
            "alice",
            "@corp.local",
            "alice@",
            "",
            "   ",
            "a b@corp.local",
        ] {
            assert_eq!(cfg.impersonation_target(addr), None, "{addr:?}");
        }
    }

    /// Exchange 2019 has no SMTPUTF8, and Unicode lowercasing folds the
    /// Kelvin sign onto `k`: a non-ASCII address names no mailbox, so it can
    /// never map onto a real one. Spaces around it and letter case do not
    /// matter, the same way SQLite's ASCII `lower()` sees them.
    #[test]
    fn a_non_ascii_address_names_no_mailbox() {
        let cfg = cfg_with(None);
        assert_eq!(cfg.impersonation_target("\u{212A}en@corp.com"), None);
        assert_eq!(cfg.impersonation_target("j\u{f6}rg@corp.com"), None);
        assert_eq!(cfg.impersonation_target("ceo@corp.com\u{0}"), None);
        assert_eq!(
            cfg.impersonation_target("  CEO@Corp.COM ").as_deref(),
            Some("ceo@corp.com")
        );
        assert_eq!(normalize_domain("\u{212A}orp.com"), Err(()));
    }

    #[test]
    fn domains_are_normalized_and_checked() {
        assert_eq!(
            normalize_domain(" @Example.COM "),
            Ok(Some("example.com".to_string()))
        );
        assert_eq!(normalize_domain("  "), Ok(None));
        assert!(normalize_domain("a@example.com").is_err());
        assert!(normalize_domain("example com").is_err());
        assert!(normalize_domain("localhost").is_err());
        assert_eq!(DomainOverride::new(Some("example.local"), None), None);
        assert_eq!(
            DomainOverride::new(Some("example.local"), Some("@example.com")),
            Some(DomainOverride {
                from: "example.local".to_string(),
                to: "example.com".to_string()
            })
        );
    }
}
