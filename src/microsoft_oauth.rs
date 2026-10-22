//! Delegated Microsoft Graph credentials. No SSO/email identity assumptions.
use anyhow::{anyhow, ensure, Result};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use rand::RngCore;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;
use std::sync::Arc;

use crate::providers::graph::{GraphError, GraphProvider};

pub const GRAPH_URL: &str = "https://graph.microsoft.com/v1.0";
const SCOPES: &str = "https://graph.microsoft.com/Calendars.ReadWrite https://graph.microsoft.com/User.Read offline_access";

pub fn validate_tenant(tenant: &str) -> Result<()> {
    let valid = matches!(tenant, "common" | "organizations" | "consumers")
        || uuid::Uuid::parse_str(tenant).is_ok()
        || (tenant.len() <= 253
            && tenant.contains('.')
            && tenant.split('.').all(|label| {
                !label.is_empty()
                    && label.len() <= 63
                    && label.as_bytes()[0].is_ascii_alphanumeric()
                    && label.as_bytes()[label.len() - 1].is_ascii_alphanumeric()
                    && label
                        .bytes()
                        .all(|c| c.is_ascii_alphanumeric() || c == b'-')
            }));
    ensure!(valid, "Microsoft tenant must be common, organizations, consumers, a tenant UUID or a verified domain");
    Ok(())
}

#[derive(sqlx::FromRow)]
pub struct Config {
    pub client_id: String,
    pub secret_enc: String,
    pub tenant: String,
}

pub async fn load_config(pool: &SqlitePool) -> Result<Config> {
    let config: Config = sqlx::query_as(
        "SELECT microsoft_oauth2_client_id AS client_id, microsoft_oauth2_client_secret AS secret_enc, microsoft_oauth2_tenant AS tenant FROM auth_config WHERE id = 'singleton' AND microsoft_oauth2_client_id IS NOT NULL AND microsoft_oauth2_client_secret IS NOT NULL",
    ).fetch_optional(pool).await?.ok_or_else(|| anyhow!("Microsoft Calendar is not configured"))?;
    ensure!(
        !config.client_id.is_empty() && !config.secret_enc.is_empty(),
        "Microsoft Calendar is not configured"
    );
    validate_tenant(&config.tenant)?;
    Ok(config)
}

pub async fn save_config(
    pool: &SqlitePool,
    key: &[u8; 32],
    client_id: &str,
    secret: &str,
    tenant: &str,
) -> Result<()> {
    validate_tenant(tenant)?;
    ensure!(
        client_id.is_empty() || uuid::Uuid::parse_str(client_id).is_ok(),
        "Microsoft client ID must be a UUID"
    );
    let secret_enc = if secret.trim().is_empty() {
        None
    } else {
        Some(crate::crypto::encrypt_value(key, secret)?)
    };
    let result = sqlx::query("UPDATE auth_config SET microsoft_oauth2_client_id = NULLIF(?, ''), microsoft_oauth2_client_secret = COALESCE(?, microsoft_oauth2_client_secret), microsoft_oauth2_tenant = ?, updated_at = datetime('now') WHERE id = 'singleton'")
        .bind(client_id).bind(secret_enc).bind(tenant).execute(pool).await?;
    ensure!(
        result.rows_affected() == 1,
        "Authentication configuration is missing"
    );
    Ok(())
}

fn http_client() -> Result<reqwest::Client> {
    // Never forward credentials through a redirect, even from a trusted endpoint.
    Ok(reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(std::time::Duration::from_secs(25))
        .build()?)
}

pub fn callback_uri(base: &str) -> Result<String> {
    let mut url = reqwest::Url::parse(base)
        .map_err(|_| anyhow!("Configure an HTTPS public base URL first"))?;
    ensure!(
        url.scheme() == "https"
            && url.host_str().is_some()
            && url.username().is_empty()
            && url.password().is_none()
            && url.query().is_none()
            && url.fragment().is_none()
            && matches!(url.path(), "" | "/"),
        "Configure an HTTPS origin as public base URL"
    );
    url.set_path("/dashboard/sources/microsoft/callback");
    Ok(url.to_string())
}

fn random_secret() -> String {
    let mut bytes = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

fn state_hash(state: &str) -> String {
    hex::encode(Sha256::digest(state.as_bytes()))
}
fn challenge(verifier: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

pub async fn start_authorization(
    pool: &SqlitePool,
    key: &[u8; 32],
    user_id: &str,
    reconnect_source: Option<&str>,
    base: &str,
) -> Result<String> {
    let config = load_config(pool).await?;
    let redirect_uri = callback_uri(base)?;
    let account_id: String = sqlx::query_scalar(
        "SELECT id FROM accounts WHERE user_id = ? ORDER BY created_at, id LIMIT 1",
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| anyhow!("No scheduling account exists"))?;
    if let Some(source) = reconnect_source {
        let owned: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM caldav_sources WHERE id = ? AND account_id = ? AND provider_type = 'microsoft' AND auth_type = 'oauth2' AND oauth2_provider = 'microsoft')")
            .bind(source).bind(&account_id).fetch_one(pool).await?;
        ensure!(owned, "Microsoft source not found");
    }
    let state = random_secret();
    let verifier = random_secret();
    let mut url = reqwest::Url::parse(&format!(
        "https://login.microsoftonline.com/{}/oauth2/v2.0/authorize",
        config.tenant
    ))?;
    url.query_pairs_mut().extend_pairs([
        ("client_id", config.client_id.as_str()),
        ("response_type", "code"),
        ("response_mode", "query"),
        ("redirect_uri", redirect_uri.as_str()),
        ("scope", SCOPES),
        ("state", state.as_str()),
        ("code_challenge", challenge(&verifier).as_str()),
        ("code_challenge_method", "S256"),
        ("prompt", "select_account"),
    ]);
    let mut tx = pool.begin().await?;
    // Expired states go, and so do this user's earlier pending ones: only the
    // latest flow stays valid, and a user cannot pile up rows.
    sqlx::query("DELETE FROM microsoft_oauth_states WHERE expires_at <= ? OR user_id = ?")
        .bind(chrono::Utc::now().timestamp())
        .bind(user_id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("INSERT INTO microsoft_oauth_states (state_hash,user_id,account_id,verifier_enc,redirect_uri,client_id,tenant,reconnect_source_id,expires_at) VALUES (?,?,?,?,?,?,?,?,?)")
        .bind(state_hash(&state)).bind(user_id).bind(account_id).bind(crate::crypto::encrypt_value(key, &verifier)?)
        .bind(redirect_uri).bind(config.client_id).bind(config.tenant).bind(reconnect_source)
        .bind(chrono::Utc::now().timestamp() + 600).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(url.to_string())
}

#[derive(sqlx::FromRow)]
pub struct PendingAuthorization {
    account_id: String,
    verifier_enc: String,
    redirect_uri: String,
    client_id: String,
    tenant: String,
    reconnect_source_id: Option<String>,
}

pub async fn consume_state(
    pool: &SqlitePool,
    user_id: &str,
    state: &str,
) -> Result<PendingAuthorization> {
    ensure!(
        state.len() == 43,
        "Invalid or expired Microsoft authorization state; start again"
    );
    // Atomic delete makes callbacks single-use across tabs, processes and restarts.
    // Explicit commit is required before returning the row: SQLite can keep a
    // DELETE ... RETURNING statement's implicit transaction open after its first row.
    let mut tx = pool.begin().await?;
    let pending = sqlx::query_as("DELETE FROM microsoft_oauth_states WHERE state_hash = ? AND user_id = ? AND expires_at > ? AND EXISTS (SELECT 1 FROM accounts WHERE accounts.id = microsoft_oauth_states.account_id AND accounts.user_id = ?) RETURNING account_id,verifier_enc,redirect_uri,client_id,tenant,reconnect_source_id")
        .bind(state_hash(state)).bind(user_id).bind(chrono::Utc::now().timestamp()).bind(user_id)
        .fetch_optional(&mut *tx).await?.ok_or_else(|| anyhow!("Invalid or expired Microsoft authorization state; start again"))?;
    tx.commit().await?;
    Ok(pending)
}

#[derive(Deserialize)]
struct Tokens {
    access_token: String,
    refresh_token: Option<String>,
    expires_in: i64,
    token_type: String,
    scope: String,
}

impl Tokens {
    fn validate(&self) -> Result<()> {
        ensure!(
            self.token_type.eq_ignore_ascii_case("bearer")
                && !self.access_token.is_empty()
                && self.expires_in > 0
                && self.expires_in <= 86400,
            "Microsoft returned an invalid token grant"
        );
        for required in ["Calendars.ReadWrite", "User.Read"] {
            ensure!(self.scope.split_whitespace().any(|s| s.strip_prefix("https://graph.microsoft.com/").unwrap_or(s).eq_ignore_ascii_case(required)), "Microsoft calendar or profile consent is missing; reconnect and grant the requested permissions");
        }
        Ok(())
    }
}

/// Why the token endpoint refused a grant, from the OAuth `error` field
/// (RFC 6749 §5.2) and nothing else of the response: the rest may echo
/// secrets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenRefused {
    /// `invalid_grant`: the refresh token expired or was revoked, or the
    /// tenant now requires the user to sign in again.
    Reconnect,
    /// `invalid_client`, `unauthorized_client`: the app registration's
    /// credentials, for an administrator to fix.
    AppCredentials,
    Other,
}

impl TokenRefused {
    fn from_body(body: &[u8]) -> Self {
        #[derive(Deserialize)]
        struct Refusal {
            error: String,
        }
        match serde_json::from_slice::<Refusal>(body).map(|r| r.error) {
            Ok(error) if error == "invalid_grant" => Self::Reconnect,
            Ok(error) if matches!(error.as_str(), "invalid_client" | "unauthorized_client") => {
                Self::AppCredentials
            }
            _ => Self::Other,
        }
    }
}

impl std::fmt::Display for TokenRefused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Reconnect => RECONNECT,
            Self::AppCredentials => {
                "Microsoft rejected the calrs application credentials; an administrator must check the client ID, secret and tenant"
            }
            Self::Other => {
                "Microsoft authorization was rejected; check the app settings or reconnect your account"
            }
        })
    }
}

impl std::error::Error for TokenRefused {}

const RECONNECT: &str =
    "Microsoft sign-in for this calendar expired or was revoked; reconnect the source";

async fn request_tokens(
    config: &Config,
    key: &[u8; 32],
    fields: &[(&str, &str)],
) -> Result<Tokens> {
    let secret = crate::crypto::decrypt_value(key, &config.secret_enc)
        .map_err(|_| anyhow!("Unable to decrypt Microsoft client credentials"))?;
    let mut form = vec![
        ("client_id", config.client_id.as_str()),
        ("client_secret", secret.as_str()),
        ("scope", SCOPES),
    ];
    form.extend_from_slice(fields);
    let response = http_client()?
        .post(format!(
            "https://login.microsoftonline.com/{}/oauth2/v2.0/token",
            config.tenant
        ))
        .form(&form)
        .send()
        .await
        .map_err(|_| anyhow!("Microsoft token endpoint could not be reached"))?;
    // Do not propagate provider response bodies, codes, URLs or request
    // errors: they may contain secrets. Only the OAuth error code is read.
    if !response.status().is_success() {
        let body = response.bytes().await.unwrap_or_default();
        return Err(TokenRefused::from_body(&body).into());
    }
    let tokens: Tokens = response
        .json()
        .await
        .map_err(|_| anyhow!("Microsoft returned an invalid token response"))?;
    tokens.validate()?;
    Ok(tokens)
}

#[derive(Deserialize)]
struct Profile {
    id: String,
    #[serde(rename = "userPrincipalName")]
    principal: Option<String>,
    mail: Option<String>,
}

pub async fn finish_authorization(
    pool: &SqlitePool,
    key: &[u8; 32],
    pending: PendingAuthorization,
    code: &str,
) -> Result<String> {
    ensure!(!code.is_empty(), "Microsoft authorization code is missing");
    let config = load_config(pool).await?;
    ensure!(
        config.client_id == pending.client_id && config.tenant == pending.tenant,
        "Microsoft configuration changed; start authorization again"
    );
    let verifier = crate::crypto::decrypt_value(key, &pending.verifier_enc)?;
    let tokens = request_tokens(
        &config,
        key,
        &[
            ("grant_type", "authorization_code"),
            ("code", code),
            ("redirect_uri", &pending.redirect_uri),
            ("code_verifier", &verifier),
        ],
    )
    .await?;
    ensure!(
        tokens.refresh_token.as_ref().is_some_and(|s| !s.is_empty()),
        "Microsoft did not grant offline access; reconnect your account"
    );
    let response = http_client()?
        .get(format!("{GRAPH_URL}/me?$select=id,mail,userPrincipalName"))
        .bearer_auth(&tokens.access_token)
        .send()
        .await
        .map_err(|_| anyhow!("Microsoft profile could not be reached"))?;
    ensure!(
        response.status().is_success(),
        "Microsoft profile access failed; grant User.Read consent"
    );
    let profile: Profile = response
        .json()
        .await
        .map_err(|_| anyhow!("Microsoft returned an invalid profile"))?;
    ensure!(
        !profile.id.is_empty() && profile.id.len() <= 256,
        "Microsoft profile has no stable identity"
    );
    save_authorized_source(pool, key, &pending, &profile, &tokens).await
}

async fn save_authorized_source(
    pool: &SqlitePool,
    key: &[u8; 32],
    pending: &PendingAuthorization,
    profile: &Profile,
    tokens: &Tokens,
) -> Result<String> {
    let access = crate::crypto::encrypt_value(key, &tokens.access_token)?;
    let refresh = crate::crypto::encrypt_value(
        key,
        tokens
            .refresh_token
            .as_deref()
            .ok_or_else(|| anyhow!("Microsoft did not grant offline access"))?,
    )?;
    let expires = (chrono::Utc::now() + chrono::Duration::seconds(tokens.expires_in)).to_rfc3339();
    let username = profile
        .mail
        .as_deref()
        .filter(|s| !s.is_empty())
        .or(profile.principal.as_deref())
        .unwrap_or(&profile.id);
    let mut tx = pool.begin().await?;
    // First statement takes SQLite's writer lock, serializing reconnect with refresh.
    sqlx::query("UPDATE accounts SET id = id WHERE id = ?")
        .bind(&pending.account_id)
        .execute(&mut *tx)
        .await?;
    if let Some(id) = &pending.reconnect_source_id {
        let matches: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM caldav_sources WHERE id = ? AND account_id = ? AND microsoft_user_id = ? AND provider_type = 'microsoft')")
            .bind(id).bind(&pending.account_id).bind(&profile.id).fetch_one(&mut *tx).await?;
        ensure!(matches, "A different Microsoft account was selected; reconnect with the original account or add a new source");
    }
    let source_id: String = sqlx::query_scalar("INSERT INTO caldav_sources (id,account_id,name,url,username,auth_type,oauth2_provider,provider_type,microsoft_user_id,access_token_enc,refresh_token_enc,token_expires_at) VALUES (?,?,'Microsoft Calendar',?,?,'oauth2','microsoft','microsoft',?,?,?,?) ON CONFLICT(account_id,microsoft_user_id) WHERE provider_type = 'microsoft' DO UPDATE SET username = excluded.username, access_token_enc = excluded.access_token_enc, refresh_token_enc = excluded.refresh_token_enc, token_expires_at = excluded.token_expires_at RETURNING id")
        .bind(uuid::Uuid::new_v4().to_string()).bind(&pending.account_id).bind(GRAPH_URL).bind(username).bind(&profile.id)
        .bind(access).bind(refresh).bind(expires).fetch_one(&mut *tx).await?;
    tx.commit().await?;
    Ok(source_id)
}

#[derive(sqlx::FromRow)]
struct StoredTokens {
    access_token_enc: Option<String>,
    refresh_token_enc: Option<String>,
    token_expires_at: Option<String>,
}
const TOKEN_QUERY: &str = "SELECT access_token_enc,refresh_token_enc,token_expires_at FROM caldav_sources WHERE id = ? AND provider_type = 'microsoft' AND auth_type = 'oauth2' AND oauth2_provider = 'microsoft'";
fn fresh_access(tokens: &StoredTokens, key: &[u8; 32]) -> Result<Option<String>> {
    let fresh = tokens
        .token_expires_at
        .as_deref()
        .and_then(|v| chrono::DateTime::parse_from_rfc3339(v).ok())
        .is_some_and(|expiry| expiry.timestamp() > chrono::Utc::now().timestamp() + 120);
    if !fresh {
        return Ok(None);
    }
    let encrypted = tokens
        .access_token_enc
        .as_deref()
        .ok_or_else(|| anyhow!("Microsoft access token is missing; reconnect"))?;
    Ok(Some(crate::crypto::decrypt_value(key, encrypted).map_err(
        |_| anyhow!("Unable to decrypt Microsoft access token"),
    )?))
}

/// Stores refreshed tokens only while the row still carries
/// `previous_expires_at`, so two concurrent refreshes cannot overwrite each
/// other's rotation. Returns `false` when another task refreshed first.
async fn store_refreshed_tokens(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    key: &[u8; 32],
    source_id: &str,
    previous_expires_at: Option<&str>,
    old_refresh: &str,
    tokens: &Tokens,
) -> Result<bool> {
    tokens.validate()?;
    let refresh = tokens
        .refresh_token
        .as_deref()
        .filter(|s| !s.is_empty())
        .unwrap_or(old_refresh);
    let changed = sqlx::query("UPDATE caldav_sources SET access_token_enc = ?,refresh_token_enc = ?,token_expires_at = ? WHERE id = ? AND provider_type = 'microsoft' AND token_expires_at IS ?")
        .bind(crate::crypto::encrypt_value(key, &tokens.access_token)?).bind(crate::crypto::encrypt_value(key, refresh)?)
        .bind((chrono::Utc::now() + chrono::Duration::seconds(tokens.expires_in)).to_rfc3339()).bind(source_id).bind(previous_expires_at).execute(&mut **tx).await?;
    Ok(changed.rows_affected() == 1)
}

pub async fn build_provider_for_source(
    pool: &SqlitePool,
    key: &[u8; 32],
    source_id: &str,
) -> Result<Box<dyn crate::providers::CalendarProvider>> {
    let access = access_token(pool, key, source_id, false).await?;
    Ok(Box::new(SourceGraph {
        pool: pool.clone(),
        key: *key,
        source_id: source_id.to_owned(),
        graph: tokio::sync::RwLock::new(Arc::new(GraphProvider::new(&access)?)),
    }))
}

/// The source's access token: the stored one while it has two minutes left
/// (unless `force`, after Graph refused it), else a refreshed one.
async fn access_token(
    pool: &SqlitePool,
    key: &[u8; 32],
    source_id: &str,
    force: bool,
) -> Result<String> {
    let stored: StoredTokens = sqlx::query_as(TOKEN_QUERY)
        .bind(source_id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| anyhow!("Microsoft source not found"))?;
    if !force {
        if let Some(access) = fresh_access(&stored, key)? {
            return Ok(access);
        }
    }
    // Cleared after the token endpoint refused it for good: until the user
    // reconnects, no sync or write-back retry calls the endpoint again.
    let encrypted = stored
        .refresh_token_enc
        .as_deref()
        .ok_or_else(|| anyhow!(RECONNECT))?;
    let refresh = crate::crypto::decrypt_value(key, encrypted)
        .map_err(|_| anyhow!("Unable to decrypt Microsoft refresh token"))?;
    let config = load_config(pool).await?;
    // The refresh runs outside any transaction: the HTTP request can outlast
    // SQLite's 5-second busy timeout, and holding the writer lock across it
    // would fail concurrent bookings and syncs with SQLITE_BUSY.
    let tokens = match request_tokens(
        &config,
        key,
        &[("grant_type", "refresh_token"), ("refresh_token", &refresh)],
    )
    .await
    {
        Ok(tokens) => tokens,
        Err(error) => {
            if error.downcast_ref::<TokenRefused>() == Some(&TokenRefused::Reconnect) {
                forget_refresh_token(pool, source_id, encrypted).await?;
            }
            return Err(error);
        }
    };
    // Short write guarded by the expiry read before the request: if another
    // task rotated the tokens meanwhile, keep its row and use its access
    // token. Our own access token stays valid server-side either way.
    let mut tx = pool.begin().await?;
    let stored_now = store_refreshed_tokens(
        &mut tx,
        key,
        source_id,
        stored.token_expires_at.as_deref(),
        &refresh,
        &tokens,
    )
    .await?;
    if stored_now {
        tx.commit().await?;
        return Ok(tokens.access_token);
    }
    tx.rollback().await?;
    let winner: StoredTokens = sqlx::query_as(TOKEN_QUERY)
        .bind(source_id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| anyhow!("Microsoft source not found"))?;
    Ok(fresh_access(&winner, key)?.unwrap_or(tokens.access_token))
}

/// Drops a refresh token the endpoint refused for good, unless a reconnect
/// already replaced it.
async fn forget_refresh_token(pool: &SqlitePool, source_id: &str, refused: &str) -> Result<()> {
    sqlx::query("UPDATE caldav_sources SET refresh_token_enc = NULL WHERE id = ? AND provider_type = 'microsoft' AND refresh_token_enc = ?")
        .bind(source_id)
        .bind(refused)
        .execute(pool)
        .await?;
    Ok(())
}

/// Graph with a source's tokens. A 401 means the access token was revoked
/// or expired early: the tokens are refreshed once and the call retried.
struct SourceGraph {
    pool: SqlitePool,
    key: [u8; 32],
    source_id: String,
    graph: tokio::sync::RwLock<Arc<GraphProvider>>,
}

impl SourceGraph {
    async fn call<T, F, Fut>(&self, operation: F) -> Result<T>
    where
        F: Fn(Arc<GraphProvider>) -> Fut,
        Fut: std::future::Future<Output = Result<T>>,
    {
        let graph = self.graph.read().await.clone();
        match operation(graph).await {
            Err(error) if error.downcast_ref::<GraphError>() == Some(&GraphError::Http(401)) => {
                let access = access_token(&self.pool, &self.key, &self.source_id, true).await?;
                let graph = Arc::new(GraphProvider::new(&access)?);
                *self.graph.write().await = graph.clone();
                operation(graph).await
            }
            result => result,
        }
    }
}

#[async_trait::async_trait]
impl crate::providers::CalendarProvider for SourceGraph {
    async fn check_connection(&self) -> Result<bool> {
        self.call(|graph| async move { graph.check_connection().await })
            .await
    }
    async fn list_calendars(&self) -> Result<Vec<crate::providers::RemoteCalendar>> {
        self.call(|graph| async move { graph.list_calendars().await })
            .await
    }
    async fn fetch_events(&self, calendar_id: &str) -> Result<Vec<crate::providers::RawEvent>> {
        self.call(|graph| async move { graph.fetch_events(calendar_id).await })
            .await
    }
    async fn fetch_events_since(
        &self,
        calendar_id: &str,
        since_utc: &str,
    ) -> Result<Vec<crate::providers::RawEvent>> {
        self.call(|graph| async move { graph.fetch_events_since(calendar_id, since_utc).await })
            .await
    }
    async fn sync_delta(
        &self,
        calendar_id: &str,
        sync_state: Option<&str>,
    ) -> Result<crate::providers::DeltaResult> {
        self.call(|graph| async move { graph.sync_delta(calendar_id, sync_state).await })
            .await
    }
    async fn put_event(&self, calendar_id: &str, uid: &str, ics: &str) -> Result<()> {
        self.call(|graph| async move { graph.put_event(calendar_id, uid, ics).await })
            .await
    }
    async fn delete_event(&self, calendar_id: &str, uid: &str) -> Result<()> {
        self.call(|graph| async move { graph.delete_event(calendar_id, uid).await })
            .await
    }
    async fn event_exists(&self, calendar_id: &str, uid: &str) -> Result<bool> {
        self.call(|graph| async move { graph.event_exists(calendar_id, uid).await })
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn database() -> SqlitePool {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        seed_database(&pool).await;
        pool
    }

    async fn seed_database(pool: &SqlitePool) {
        crate::db::migrate(pool).await.unwrap();
        sqlx::raw_sql("INSERT INTO users (id,email,name,role,auth_provider,username,enabled) VALUES ('u1','local@example.com','Local','user','local','local',1),('u2','other@example.com','Other','user','local','other',1);
            INSERT INTO accounts (id,name,email,user_id) VALUES ('a1','Local','local@example.com','u1'),('a2','Other','other@example.com','u2');")
            .execute(pool).await.unwrap();
        save_config(
            pool,
            &[1; 32],
            "00000000-0000-0000-0000-000000000001",
            "secret",
            "common",
        )
        .await
        .unwrap();
    }

    fn grant(refresh: Option<&str>) -> Tokens {
        Tokens {
            access_token: "new-access".into(),
            refresh_token: refresh.map(String::from),
            expires_in: 3600,
            token_type: "Bearer".into(),
            scope: "Calendars.ReadWrite User.Read".into(),
        }
    }
    fn pending(account: &str) -> PendingAuthorization {
        PendingAuthorization {
            account_id: account.into(),
            verifier_enc: String::new(),
            redirect_uri: String::new(),
            client_id: String::new(),
            tenant: "common".into(),
            reconnect_source_id: None,
        }
    }
    fn profile(id: &str) -> Profile {
        Profile {
            id: id.into(),
            principal: Some("microsoft@example.net".into()),
            mail: None,
        }
    }

    #[test]
    fn tenant_and_callback_reject_url_injection() {
        for tenant in [
            "common",
            "organizations",
            "consumers",
            "example.onmicrosoft.com",
            "00000000-0000-0000-0000-000000000001",
        ] {
            assert!(validate_tenant(tenant).is_ok());
        }
        for tenant in [
            "",
            "../common",
            "common?x=1",
            "evil.com/path",
            "common%2f",
            "user@evil.com",
            "-evil.com",
            "evil..com",
        ] {
            assert!(validate_tenant(tenant).is_err());
        }
        assert_eq!(
            callback_uri("https://cal.example.com/").unwrap(),
            "https://cal.example.com/dashboard/sources/microsoft/callback"
        );
        for base in [
            "http://cal.example.com",
            "https://user:secret@cal.example.com",
            "https://cal.example.com/?redirect=evil",
            "https://cal.example.com/subpath",
        ] {
            assert!(callback_uri(base).is_err());
        }
        assert_eq!(
            challenge("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
    }

    #[tokio::test]
    async fn state_is_bound_expiring_encrypted_and_single_use() {
        let pool = database().await;
        let url = start_authorization(&pool, &[1; 32], "u1", None, "https://cal.example.com")
            .await
            .unwrap();
        let parsed = reqwest::Url::parse(&url).unwrap();
        let params: std::collections::HashMap<_, _> = parsed.query_pairs().into_owned().collect();
        let state = &params["state"];
        assert_eq!(params["code_challenge_method"], "S256");
        assert!(params["scope"].contains("offline_access"));
        let encrypted: String =
            sqlx::query_scalar("SELECT verifier_enc FROM microsoft_oauth_states")
                .fetch_one(&pool)
                .await
                .unwrap();
        let verifier = crate::crypto::decrypt_value(&[1; 32], &encrypted).unwrap();
        assert_ne!(encrypted, verifier);
        assert_eq!(challenge(&verifier), params["code_challenge"]);
        assert!(consume_state(&pool, "u2", state).await.is_err());
        assert!(consume_state(&pool, "u1", &random_secret()).await.is_err());
        assert!(consume_state(&pool, "u1", "").await.is_err());
        assert!(consume_state(&pool, "u1", state).await.is_ok());
        assert!(consume_state(&pool, "u1", state).await.is_err());
        let url = start_authorization(&pool, &[1; 32], "u1", None, "https://cal.example.com")
            .await
            .unwrap();
        let parsed = reqwest::Url::parse(&url).unwrap();
        let state = parsed
            .query_pairs()
            .find(|(k, _)| k == "state")
            .unwrap()
            .1
            .into_owned();
        sqlx::query("UPDATE microsoft_oauth_states SET expires_at = ?")
            .bind(chrono::Utc::now().timestamp() - 1)
            .execute(&pool)
            .await
            .unwrap();
        assert!(consume_state(&pool, "u1", &state).await.is_err());
    }

    #[tokio::test]
    async fn consumed_state_is_committed_before_callback_returns() {
        let directory =
            std::env::temp_dir().join(format!("calrs-microsoft-state-{}", uuid::Uuid::new_v4()));
        let pool = crate::db::connect(&directory).await.unwrap();
        seed_database(&pool).await;
        let observer = crate::db::connect(&directory).await.unwrap();
        let url = start_authorization(&pool, &[1; 32], "u1", None, "https://cal.example.com")
            .await
            .unwrap();
        let state = reqwest::Url::parse(&url)
            .unwrap()
            .query_pairs()
            .find(|(key, _)| key == "state")
            .unwrap()
            .1
            .into_owned();
        consume_state(&pool, "u1", &state).await.unwrap();
        let remaining: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM microsoft_oauth_states")
            .fetch_one(&observer)
            .await
            .unwrap();
        assert_eq!(
            remaining, 0,
            "Another process must observe consumption immediately"
        );
        assert!(consume_state(&observer, "u1", &state).await.is_err());
        observer.close().await;
        pool.close().await;
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[tokio::test]
    async fn blank_secret_retains_encrypted_value() {
        let pool = database().await;
        let before = load_config(&pool).await.unwrap().secret_enc;
        save_config(
            &pool,
            &[1; 32],
            "00000000-0000-0000-0000-000000000001",
            "  ",
            "organizations",
        )
        .await
        .unwrap();
        let config = load_config(&pool).await.unwrap();
        assert_eq!(before, config.secret_enc);
        assert_eq!(
            crate::crypto::decrypt_value(&[1; 32], &config.secret_enc).unwrap(),
            "secret"
        );
        assert_eq!(config.tenant, "organizations");
        assert!(
            save_config(&pool, &[1; 32], "not-a-client", "replacement", "common")
                .await
                .is_err()
        );
        assert_eq!(load_config(&pool).await.unwrap().secret_enc, before);
    }

    #[tokio::test]
    async fn reconnect_preserves_calendar_and_scopes_identity_to_local_account() {
        let pool = database().await;
        let p = pending("a1");
        let id = save_authorized_source(
            &pool,
            &[1; 32],
            &p,
            &profile("ms-1"),
            &grant(Some("refresh")),
        )
        .await
        .unwrap();
        sqlx::query(
            "UPDATE caldav_sources SET write_calendar_href = 'selected-calendar' WHERE id = ?",
        )
        .bind(&id)
        .execute(&pool)
        .await
        .unwrap();
        let reconnect = PendingAuthorization {
            reconnect_source_id: Some(id.clone()),
            ..pending("a1")
        };
        assert!(save_authorized_source(
            &pool,
            &[1; 32],
            &reconnect,
            &profile("ms-2"),
            &grant(Some("other"))
        )
        .await
        .is_err());
        let same = save_authorized_source(
            &pool,
            &[1; 32],
            &reconnect,
            &profile("ms-1"),
            &grant(Some("rotated")),
        )
        .await
        .unwrap();
        assert_eq!(id, same);
        let selected: String =
            sqlx::query_scalar("SELECT write_calendar_href FROM caldav_sources WHERE id = ?")
                .bind(&id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(selected, "selected-calendar");
        let distinct =
            save_authorized_source(&pool, &[1; 32], &p, &profile("ms-2"), &grant(Some("other")))
                .await
                .unwrap();
        let other_owner = save_authorized_source(
            &pool,
            &[1; 32],
            &pending("a2"),
            &profile("ms-1"),
            &grant(Some("other")),
        )
        .await
        .unwrap();
        assert_ne!(id, distinct);
        assert_ne!(id, other_owner);
        assert!(
            start_authorization(&pool, &[1; 32], "u2", Some(&id), "https://cal.example.com")
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn refresh_rotation_is_atomic_and_missing_rotation_keeps_refresh_token() {
        // Read the compare-and-swap value before opening the transaction: the
        // test pool has one connection, so a pool read inside it would wait
        // for the connection the transaction already holds.
        async fn stored_expiry(pool: &SqlitePool, id: &str) -> Option<String> {
            sqlx::query_scalar("SELECT token_expires_at FROM caldav_sources WHERE id = ?")
                .bind(id)
                .fetch_one(pool)
                .await
                .unwrap()
        }
        let pool = database().await;
        let id = save_authorized_source(
            &pool,
            &[1; 32],
            &pending("a1"),
            &profile("ms-1"),
            &grant(Some("original")),
        )
        .await
        .unwrap();
        let previous = stored_expiry(&pool, &id).await;
        let mut tx = pool.begin().await.unwrap();
        store_refreshed_tokens(
            &mut tx,
            &[1; 32],
            &id,
            previous.as_deref(),
            "original",
            &grant(Some("rotated")),
        )
        .await
        .unwrap();
        tx.rollback().await.unwrap();
        let stored: StoredTokens = sqlx::query_as(TOKEN_QUERY)
            .bind(&id)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(
            crate::crypto::decrypt_value(&[1; 32], stored.refresh_token_enc.as_deref().unwrap())
                .unwrap(),
            "original"
        );
        let previous = stored_expiry(&pool, &id).await;
        let mut tx = pool.begin().await.unwrap();
        store_refreshed_tokens(
            &mut tx,
            &[1; 32],
            &id,
            previous.as_deref(),
            "original",
            &grant(Some("rotated")),
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();
        let previous = stored_expiry(&pool, &id).await;
        let mut tx = pool.begin().await.unwrap();
        store_refreshed_tokens(
            &mut tx,
            &[1; 32],
            &id,
            previous.as_deref(),
            "rotated",
            &grant(None),
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();
        let stored: StoredTokens = sqlx::query_as(TOKEN_QUERY)
            .bind(&id)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(
            crate::crypto::decrypt_value(&[1; 32], stored.refresh_token_enc.as_deref().unwrap())
                .unwrap(),
            "rotated"
        );
        assert_eq!(
            fresh_access(&stored, &[1; 32]).unwrap().unwrap(),
            "new-access"
        );
        // A second task that refreshed against the pre-rotation row must not
        // overwrite the rotation: its compare-and-swap finds a stale expiry.
        let mut tx = pool.begin().await.unwrap();
        assert!(!store_refreshed_tokens(
            &mut tx,
            &[1; 32],
            &id,
            Some("2000-01-01T00:00:00+00:00"),
            "original",
            &grant(Some("must-not-store")),
        )
        .await
        .unwrap());
        tx.commit().await.unwrap();
        let stored: StoredTokens = sqlx::query_as(TOKEN_QUERY)
            .bind(&id)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(
            crate::crypto::decrypt_value(&[1; 32], stored.refresh_token_enc.as_deref().unwrap())
                .unwrap(),
            "rotated"
        );
        let previous = stored_expiry(&pool, &id).await;
        let mut invalid = grant(Some("must-not-save"));
        invalid.scope = "User.Read".into();
        let mut tx = pool.begin().await.unwrap();
        assert!(store_refreshed_tokens(
            &mut tx,
            &[1; 32],
            &id,
            previous.as_deref(),
            "rotated",
            &invalid
        )
        .await
        .is_err());
        tx.rollback().await.unwrap();
        let stored: StoredTokens = sqlx::query_as(TOKEN_QUERY)
            .bind(&id)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(
            crate::crypto::decrypt_value(&[1; 32], stored.refresh_token_enc.as_deref().unwrap())
                .unwrap(),
            "rotated"
        );
    }

    /// The grant Entra returns for calrs' scopes: OIDC scopes added, Graph
    /// scopes URL-qualified, and `offline_access` absent from `scope` even
    /// though a refresh token is issued.
    #[test]
    fn entra_token_response_is_accepted_without_offline_access_in_scope() {
        let tokens: Tokens = serde_json::from_str(
            r#"{"token_type":"Bearer","scope":"profile openid email https://graph.microsoft.com/Calendars.ReadWrite https://graph.microsoft.com/User.Read","expires_in":3646,"ext_expires_in":3646,"access_token":"access","refresh_token":"refresh","id_token":"id"}"#,
        )
        .unwrap();
        tokens.validate().unwrap();
        assert_eq!(tokens.refresh_token.as_deref(), Some("refresh"));
    }

    #[test]
    fn token_refusals_read_only_the_oauth_error_code() {
        for (body, expected) in [
            (
                r#"{"error":"invalid_grant","error_description":"AADSTS70008 secret"}"#,
                TokenRefused::Reconnect,
            ),
            (
                r#"{"error":"invalid_client"}"#,
                TokenRefused::AppCredentials,
            ),
            (
                r#"{"error":"unauthorized_client"}"#,
                TokenRefused::AppCredentials,
            ),
            (
                r#"{"error":"temporarily_unavailable"}"#,
                TokenRefused::Other,
            ),
            ("<html>proxy error</html>", TokenRefused::Other),
            ("", TokenRefused::Other),
        ] {
            let refused = TokenRefused::from_body(body.as_bytes());
            assert_eq!(refused, expected, "{body}");
            assert!(!refused.to_string().contains("AADSTS"));
        }
    }

    #[tokio::test]
    async fn a_refused_refresh_token_is_dropped_until_reconnect() {
        let pool = database().await;
        let id = save_authorized_source(
            &pool,
            &[1; 32],
            &pending("a1"),
            &profile("ms-1"),
            &grant(Some("revoked")),
        )
        .await
        .unwrap();
        let refused: String =
            sqlx::query_scalar("SELECT refresh_token_enc FROM caldav_sources WHERE id = ?")
                .bind(&id)
                .fetch_one(&pool)
                .await
                .unwrap();
        // A reconnect that landed meanwhile is kept.
        save_authorized_source(
            &pool,
            &[1; 32],
            &pending("a1"),
            &profile("ms-1"),
            &grant(Some("reconnected")),
        )
        .await
        .unwrap();
        forget_refresh_token(&pool, &id, &refused).await.unwrap();
        let stored: StoredTokens = sqlx::query_as(TOKEN_QUERY)
            .bind(&id)
            .fetch_one(&pool)
            .await
            .unwrap();
        let current = stored.refresh_token_enc.unwrap();
        assert_eq!(
            crate::crypto::decrypt_value(&[1; 32], &current).unwrap(),
            "reconnected"
        );
        // Otherwise it goes, and the next sync stops at "reconnect" without
        // calling the token endpoint (the access token has expired).
        forget_refresh_token(&pool, &id, &current).await.unwrap();
        sqlx::query(
            "UPDATE caldav_sources SET token_expires_at = '2000-01-01T00:00:00+00:00' WHERE id = ?",
        )
        .bind(&id)
        .execute(&pool)
        .await
        .unwrap();
        let error = access_token(&pool, &[1; 32], &id, false).await.unwrap_err();
        assert_eq!(error.to_string(), RECONNECT);
    }

    #[tokio::test]
    async fn refresh_with_a_cleared_client_id_says_not_configured() {
        let pool = database().await;
        let id = save_authorized_source(
            &pool,
            &[1; 32],
            &pending("a1"),
            &profile("ms-1"),
            &grant(Some("refresh")),
        )
        .await
        .unwrap();
        sqlx::query(
            "UPDATE caldav_sources SET token_expires_at = '2000-01-01T00:00:00+00:00' WHERE id = ?",
        )
        .bind(&id)
        .execute(&pool)
        .await
        .unwrap();
        save_config(&pool, &[1; 32], "", "", "common")
            .await
            .unwrap();
        let error = access_token(&pool, &[1; 32], &id, false).await.unwrap_err();
        assert_eq!(error.to_string(), "Microsoft Calendar is not configured");
    }

    #[tokio::test]
    async fn a_new_authorization_supersedes_the_users_pending_one() {
        let pool = database().await;
        let state_of = |url: String| {
            reqwest::Url::parse(&url)
                .unwrap()
                .query_pairs()
                .find(|(key, _)| key == "state")
                .unwrap()
                .1
                .into_owned()
        };
        let first = state_of(
            start_authorization(&pool, &[1; 32], "u1", None, "https://cal.example.com")
                .await
                .unwrap(),
        );
        let other_user = state_of(
            start_authorization(&pool, &[1; 32], "u2", None, "https://cal.example.com")
                .await
                .unwrap(),
        );
        let latest = state_of(
            start_authorization(&pool, &[1; 32], "u1", None, "https://cal.example.com")
                .await
                .unwrap(),
        );
        assert!(consume_state(&pool, "u1", &first).await.is_err());
        assert!(consume_state(&pool, "u2", &other_user).await.is_ok());
        assert!(consume_state(&pool, "u1", &latest).await.is_ok());
    }
}
