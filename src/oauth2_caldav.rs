use anyhow::{bail, Result};
use serde::Deserialize;
use sqlx::SqlitePool;

/// Google OAuth2 endpoints and CalDAV configuration.
const GOOGLE_AUTH_URL: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const GOOGLE_TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
const GOOGLE_CALDAV_BASE: &str = "https://apidata.googleusercontent.com/caldav/v2/";
const GOOGLE_CALENDAR_SCOPE: &str = "https://www.googleapis.com/auth/calendar";
const GOOGLE_EMAIL_SCOPE: &str = "openid email";

/// Buffer before expiry to trigger proactive refresh (5 minutes).
const REFRESH_BUFFER_SECS: i64 = 300;

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    refresh_token: Option<String>,
    expires_in: Option<i64>,
}

/// Build a Google OAuth2 authorization URL.
/// Returns the URL the user should be redirected to.
pub fn build_google_auth_url(client_id: &str, redirect_uri: &str, state: &str) -> String {
    format!(
        "{}?client_id={}&redirect_uri={}&response_type=code&scope={}&access_type=offline&prompt=consent&state={}",
        GOOGLE_AUTH_URL,
        urlencoding::encode(client_id),
        urlencoding::encode(redirect_uri),
        urlencoding::encode(&format!("{} {}", GOOGLE_CALENDAR_SCOPE, GOOGLE_EMAIL_SCOPE)),
        urlencoding::encode(state),
    )
}

/// POST form-encoded params to Google's OAuth2 token endpoint and parse the response.
///
/// Bound with a request timeout: Meet confirmation calls this via
/// `get_valid_access_token` *before* `ATTACH_DEADLINE` wraps `attach_meet`, so
/// an unbounded token refresh could hang the guest booking forever.
async fn post_to_google_token(op: &str, form: &[(&str, &str)]) -> Result<TokenResponse> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .unwrap_or_default();
    let resp = client.post(GOOGLE_TOKEN_URL).form(form).send().await?;

    if !resp.status().is_success() {
        let body = resp.text().await.unwrap_or_default();
        bail!("Google token {} failed: {}", op, body);
    }

    Ok(resp.json().await?)
}

/// Exchange an authorization code for access + refresh tokens.
/// Returns (access_token, refresh_token, expires_in_seconds).
pub async fn exchange_google_code(
    client_id: &str,
    client_secret: &str,
    code: &str,
    redirect_uri: &str,
) -> Result<(String, String, i64)> {
    let token = post_to_google_token(
        "exchange",
        &[
            ("client_id", client_id),
            ("client_secret", client_secret),
            ("code", code),
            ("redirect_uri", redirect_uri),
            ("grant_type", "authorization_code"),
        ],
    )
    .await?;

    let refresh_token = token
        .refresh_token
        .ok_or_else(|| anyhow::anyhow!("No refresh token received. Ensure prompt=consent"))?;
    let expires_in = token.expires_in.unwrap_or(3600);

    Ok((token.access_token, refresh_token, expires_in))
}

/// Refresh an OAuth2 access token using a stored refresh token.
/// Updates the database with the new access token and expiry.
/// Returns the new plaintext access token.
pub async fn refresh_access_token(
    pool: &SqlitePool,
    key: &[u8; 32],
    source_id: &str,
) -> Result<String> {
    // Load source credentials
    let row: (String, String) = sqlx::query_as(
        "SELECT refresh_token_enc, oauth2_provider FROM caldav_sources WHERE id = ?",
    )
    .bind(source_id)
    .fetch_one(pool)
    .await?;
    let (refresh_token_enc, provider) = row;

    if provider != "google" {
        bail!("Unsupported OAuth2 provider: {}", provider);
    }

    let refresh_token = crate::crypto::decrypt_password(key, &refresh_token_enc)?;

    // Load admin-configured Google OAuth2 credentials. The client_secret is
    // encrypted at rest (see crypto::encrypt_value); decrypt before use.
    let creds: (String, String) = sqlx::query_as(
        "SELECT google_oauth2_client_id, google_oauth2_client_secret FROM auth_config LIMIT 1",
    )
    .fetch_one(pool)
    .await?;
    let (client_id, client_secret_enc) = creds;
    let client_secret = crate::crypto::decrypt_value(key, &client_secret_enc)
        .map_err(|e| anyhow::anyhow!("Google OAuth2 client secret decryption failed: {}", e))?;

    let token = post_to_google_token(
        "refresh",
        &[
            ("client_id", &client_id),
            ("client_secret", &client_secret),
            ("refresh_token", &refresh_token),
            ("grant_type", "refresh_token"),
        ],
    )
    .await?;
    let expires_in = token.expires_in.unwrap_or(3600);
    let expires_at = chrono::Utc::now() + chrono::Duration::seconds(expires_in);

    // Encrypt and store the new access token
    let access_token_enc = crate::crypto::encrypt_password(key, &token.access_token)?;
    sqlx::query(
        "UPDATE caldav_sources SET access_token_enc = ?, token_expires_at = ? WHERE id = ?",
    )
    .bind(&access_token_enc)
    .bind(expires_at.to_rfc3339())
    .bind(source_id)
    .execute(pool)
    .await?;

    // Google may rotate the refresh token on a refresh response. Persist it so
    // the next refresh doesn't fail with the now-invalid stored token.
    if let Some(new_refresh) = token.refresh_token {
        let refresh_enc = crate::crypto::encrypt_password(key, &new_refresh)?;
        sqlx::query("UPDATE caldav_sources SET refresh_token_enc = ? WHERE id = ?")
            .bind(&refresh_enc)
            .bind(source_id)
            .execute(pool)
            .await?;
        tracing::info!(source_id = %source_id, "rotated OAuth2 refresh token");
    }

    tracing::info!(source_id = %source_id, "refreshed OAuth2 access token");
    Ok(token.access_token)
}

/// Get a valid access token for an OAuth2 source.
/// Refreshes proactively if the token expires within 5 minutes.
pub async fn get_valid_access_token(
    pool: &SqlitePool,
    key: &[u8; 32],
    source_id: &str,
    access_token_enc: &str,
    token_expires_at: Option<&str>,
) -> Result<String> {
    let needs_refresh = match token_expires_at {
        Some(exp) => {
            let expires = chrono::DateTime::parse_from_rfc3339(exp)
                .unwrap_or(chrono::DateTime::UNIX_EPOCH.into());
            let now = chrono::Utc::now();
            expires.signed_duration_since(now).num_seconds() < REFRESH_BUFFER_SECS
        }
        None => true,
    };

    if needs_refresh {
        refresh_access_token(pool, key, source_id).await
    } else {
        crate::crypto::decrypt_password(key, access_token_enc)
    }
}

/// Build a CaldavClient for a source, handling both basic and OAuth2 auth.
pub async fn build_client_for_source(
    pool: &SqlitePool,
    key: &[u8; 32],
    source_id: &str,
    url: &str,
    auth_type: &str,
    username: &str,
    password_enc: Option<&str>,
    access_token_enc: Option<&str>,
    token_expires_at: Option<&str>,
) -> Result<crate::caldav::CaldavClient> {
    match auth_type {
        "oauth2" => {
            let enc = access_token_enc
                .ok_or_else(|| anyhow::anyhow!("OAuth2 source missing access token"))?;
            let access_token =
                get_valid_access_token(pool, key, source_id, enc, token_expires_at).await?;
            Ok(crate::caldav::CaldavClient::with_bearer(url, &access_token))
        }
        _ => {
            let enc = password_enc
                .ok_or_else(|| anyhow::anyhow!("Basic auth source missing password"))?;
            let password = crate::crypto::decrypt_password(key, enc)?;
            Ok(crate::caldav::CaldavClient::new(url, username, &password))
        }
    }
}

/// The Google CalDAV base URL.
pub fn google_caldav_base_url() -> &'static str {
    GOOGLE_CALDAV_BASE
}

/// Build the per-user Google CalDAV principal URL.
/// Google requires PROPFIND to target `/caldav/v2/{userEmail}/user`. The bare
/// `/caldav/v2/` returns 403 for principal discovery.
pub fn google_caldav_url_for_email(email: &str) -> String {
    format!("{}{}/user", GOOGLE_CALDAV_BASE, urlencoding::encode(email))
}

/// Fetch the authenticated Google account's email via the OIDC userinfo endpoint.
pub async fn fetch_google_email(access_token: &str) -> Result<String> {
    let resp = reqwest::Client::new()
        .get("https://openidconnect.googleapis.com/v1/userinfo")
        .bearer_auth(access_token)
        .send()
        .await?;
    if !resp.status().is_success() {
        bail!("Failed to fetch Google userinfo: HTTP {}", resp.status());
    }
    let json: serde_json::Value = resp.json().await?;
    json.get("email")
        .and_then(|e| e.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| anyhow::anyhow!("Google userinfo response missing email claim"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_google_auth_url_encodes_components() {
        // A `+` in the client_id is the interesting case: if left as a literal
        // `+`, Google's server would decode it as a space (application/x-www-
        // form-urlencoded rules in the query string) and the OAuth2 lookup
        // would fail. RFC 3986 percent-encoding (%2B) is required.
        let client_id = "1234+abc.apps.googleusercontent.com";
        let redirect_uri = "https://cal.example.com/auth/google/callback";
        let state = "csrf token with spaces & symbols";

        let url = build_google_auth_url(client_id, redirect_uri, state);

        assert!(
            url.starts_with("https://accounts.google.com/o/oauth2/v2/auth?"),
            "url: {}",
            url
        );

        assert!(
            url.contains("client_id=1234%2Babc.apps.googleusercontent.com"),
            "client_id `+` not encoded as %2B: {}",
            url
        );
        assert!(
            !url.contains("client_id=1234+abc"),
            "raw + leaked into client_id: {}",
            url
        );

        assert!(
            url.contains("redirect_uri=https%3A%2F%2Fcal.example.com%2Fauth%2Fgoogle%2Fcallback"),
            "redirect_uri not percent-encoded: {}",
            url
        );

        assert!(
            url.contains(
                "scope=https%3A%2F%2Fwww.googleapis.com%2Fauth%2Fcalendar%20openid%20email"
            ),
            "scope not encoded with %20 between calendar + openid email: {}",
            url
        );

        assert!(
            url.contains("state=csrf%20token%20with%20spaces%20%26%20symbols"),
            "state spaces/`&` not encoded: {}",
            url
        );

        assert!(url.contains("response_type=code"), "url: {}", url);
        assert!(url.contains("access_type=offline"), "url: {}", url);
        assert!(url.contains("prompt=consent"), "url: {}", url);
    }
}
