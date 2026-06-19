//! Runtime settings that can be configured either via an environment variable
//! or persisted in the DB (`auth_config` singleton) through the admin UI /
//! `calrs config general`.
//!
//! ## Precedence
//!
//! The **environment variable wins** when it is set and non-empty. The DB value
//! is only used as a fallback. This keeps ops overrides authoritative: a value
//! forced via the environment can never be silently changed from the web UI.
//! The admin panel surfaces an "set by environment" badge in that case.
//!
//! ## Why a process-global cache
//!
//! `private_host_allowlist()` is called from deep, synchronous code paths
//! (`validate_caldav_url`, the provider factory, EWS autodiscovery) and from the
//! CLI — none of which carry a `SqlitePool`. Rather than thread the pool (or an
//! allowlist argument) through every signature, the DB-backed values are loaded
//! once into a process-global cache at startup (`load_from_db`) and refreshed
//! whenever the admin saves changes. Reads stay synchronous and allocation-light.
//!
//! `base_url()` uses the same cache so that per-email / per-link reads don't hit
//! the DB.

use sqlx::SqlitePool;
use std::sync::RwLock;

/// DB-stored `CALRS_BASE_URL` fallback. `None` until `load_from_db` runs.
static BASE_URL_DB: RwLock<Option<String>> = RwLock::new(None);
/// DB-stored `CALRS_ALLOW_PRIVATE_HOSTS` fallback (already parsed/normalised).
static ALLOW_PRIVATE_HOSTS_DB: RwLock<Option<Vec<String>>> = RwLock::new(None);

/// Read `CALRS_BASE_URL` from the environment, trimmed; `None` when unset/empty.
fn base_url_env() -> Option<String> {
    std::env::var("CALRS_BASE_URL")
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

/// Whether `CALRS_BASE_URL` is forcing the value (env precedence is in effect).
pub fn base_url_from_env() -> bool {
    base_url_env().is_some()
}

/// The effective public base URL: env var if set, else the DB-stored value.
/// `None` when neither is configured (callers fall back to their own default).
pub fn base_url() -> Option<String> {
    if let Some(env) = base_url_env() {
        return Some(env);
    }
    BASE_URL_DB.read().ok().and_then(|g| g.clone())
}

/// The DB-stored base URL, ignoring the environment override. Used by the admin
/// UI / `config show` to display what is persisted even when env wins.
pub fn base_url_db() -> Option<String> {
    BASE_URL_DB.read().ok().and_then(|g| g.clone())
}

/// Parse a comma-separated host list into normalised entries (trimmed,
/// lowercased, empties dropped).
pub fn parse_host_list(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(|h| h.trim().to_ascii_lowercase())
        .filter(|h| !h.is_empty())
        .collect()
}

/// Read `CALRS_ALLOW_PRIVATE_HOSTS` from the environment, parsed; `None` when
/// the variable is unset or contains no non-empty entries.
fn allow_private_hosts_env() -> Option<Vec<String>> {
    let raw = std::env::var("CALRS_ALLOW_PRIVATE_HOSTS").ok()?;
    let list = parse_host_list(&raw);
    if list.is_empty() {
        None
    } else {
        Some(list)
    }
}

/// Whether `CALRS_ALLOW_PRIVATE_HOSTS` is forcing the value.
pub fn allow_private_hosts_from_env() -> bool {
    allow_private_hosts_env().is_some()
}

/// The effective private-host SSRF allowlist: env var if set, else the
/// DB-stored value, else empty.
pub fn private_host_allowlist() -> Vec<String> {
    if let Some(env) = allow_private_hosts_env() {
        return env;
    }
    ALLOW_PRIVATE_HOSTS_DB
        .read()
        .ok()
        .and_then(|g| g.clone())
        .unwrap_or_default()
}

/// The DB-stored allowlist, ignoring the environment override (for display).
pub fn private_host_allowlist_db() -> Vec<String> {
    ALLOW_PRIVATE_HOSTS_DB
        .read()
        .ok()
        .and_then(|g| g.clone())
        .unwrap_or_default()
}

/// Overwrite the in-memory DB-backed values. Called after `load_from_db` and
/// whenever the admin saves changes.
fn set_cache(base_url: Option<String>, allow_private_hosts: Option<Vec<String>>) {
    if let Ok(mut g) = BASE_URL_DB.write() {
        *g = base_url;
    }
    if let Ok(mut g) = ALLOW_PRIVATE_HOSTS_DB.write() {
        *g = allow_private_hosts;
    }
}

/// Load the DB-backed runtime settings from `auth_config` into the cache. Safe
/// to call multiple times (startup, after admin save, before CLI validation).
/// Silently leaves the cache untouched on error — the env fallback still works.
pub async fn load_from_db(pool: &SqlitePool) {
    let row: Option<(Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT base_url, allow_private_hosts FROM auth_config WHERE id = 'singleton'",
    )
    .fetch_optional(pool)
    .await
    .ok()
    .flatten();

    let (base_url, allow_raw) = row.unwrap_or((None, None));

    let base_url = base_url
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty());
    let allow_private_hosts = allow_raw
        .map(|raw| parse_host_list(&raw))
        .filter(|list| !list.is_empty());

    set_cache(base_url, allow_private_hosts);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_host_list_trims_lowercases_and_drops_empties() {
        assert_eq!(
            parse_host_list(" 127.0.0.1 , Radicale ,, "),
            vec!["127.0.0.1", "radicale"]
        );
        assert!(parse_host_list("").is_empty());
        assert!(parse_host_list("  ,  ").is_empty());
    }
}
