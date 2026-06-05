//! Auto-generated video meeting links (issue #45).
//!
//! Two providers ship today:
//!
//! * **Jitsi** — a fresh room is computed locally from a pattern of tokens
//!   (`{username}`, `{event}`, `{date}`, `{random}`) and appended to a base
//!   URL (e.g. `https://meet.dyb.fr`). No external network call.
//! * **Generic webhook** — calrs POSTs the booking payload to a configured URL
//!   when the booking is confirmed and expects `{"url": "..."}` back. The
//!   request is optionally signed with HMAC-SHA256 so the receiver can prove
//!   the call came from calrs.
//!
//! The generated URL is persisted to `bookings.meeting_url` so the guest,
//! host, ICS attachment, CalDAV write-back and reminder emails all see the
//! same value. Recomputing each time would otherwise produce a different
//! `{random}`.

use rand::RngCore;
use sqlx::SqlitePool;

/// Default Jitsi pattern when neither org-wide nor per-event-type pattern is
/// configured. Chosen to mirror cal.com's behaviour (random room name with
/// just enough context to be greppable in server logs).
pub const DEFAULT_JITSI_PATTERN: &str = "{event}-{random}";

/// Location type stored in `event_types.location_type` for the auto providers.
pub const LOCATION_TYPE_JITSI: &str = "jitsi_auto";
pub const LOCATION_TYPE_WEBHOOK: &str = "webhook_auto";

/// Webhook auth mode stored in `auth_config.meeting_webhook_auth_mode`.
pub const WEBHOOK_AUTH_NONE: &str = "none";
pub const WEBHOOK_AUTH_HMAC: &str = "hmac";

/// Org-wide meeting provider configuration.
///
/// `None` for either provider means "not configured" — the corresponding
/// `location_type` on an event type will fall back to the static location
/// behaviour (treat `location_value` as the URL). This way enabling the
/// feature is opt-in and existing event types are unaffected.
#[derive(Clone, Default)]
pub struct MeetingConfig {
    pub jitsi: Option<JitsiConfig>,
    pub webhook: Option<WebhookConfig>,
}

#[derive(Clone)]
pub struct JitsiConfig {
    /// Base URL, e.g. `https://meet.dyb.fr`. Trailing slash tolerated.
    pub base_url: String,
    /// Pattern with `{token}` placeholders, or empty for `DEFAULT_JITSI_PATTERN`.
    pub pattern: String,
    /// Human-readable label shown to guests in the slot/booking UI, e.g.
    /// "Meet DYB". `None` = use the generic "Video call" badge.
    pub display_name: Option<String>,
}

#[derive(Clone)]
pub struct WebhookConfig {
    pub url: String,
    pub auth_mode: WebhookAuthMode,
    /// Shared secret for HMAC; empty when `auth_mode` is `None`.
    pub secret: String,
    /// Human-readable label shown to guests; same semantics as
    /// [`JitsiConfig::display_name`].
    pub display_name: Option<String>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum WebhookAuthMode {
    None,
    Hmac,
}

impl WebhookAuthMode {
    pub fn from_str(s: &str) -> Self {
        match s {
            WEBHOOK_AUTH_HMAC => WebhookAuthMode::Hmac,
            _ => WebhookAuthMode::None,
        }
    }
    pub fn as_str(&self) -> &'static str {
        match self {
            WebhookAuthMode::Hmac => WEBHOOK_AUTH_HMAC,
            WebhookAuthMode::None => WEBHOOK_AUTH_NONE,
        }
    }
}

/// Tokens available to the pattern expander.
///
/// `start_at` is the booking start time in ISO8601 form (whatever is stored
/// in `bookings.start_at`). Only the date portion (first 10 chars stripped of
/// dashes) is used by `{date}`.
pub struct PatternTokens<'a> {
    pub username: &'a str,
    pub event_slug: &'a str,
    pub start_at: &'a str,
}

/// Load the org-wide meeting config from `auth_config`. Returns a `MeetingConfig`
/// with `jitsi` and/or `webhook` set when configured. Decrypts the webhook
/// secret on the fly.
pub async fn load_config(pool: &SqlitePool, key: &[u8; 32]) -> MeetingConfig {
    let row: Option<(
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
    )> = sqlx::query_as(
        "SELECT jitsi_base_url, jitsi_pattern, jitsi_display_name, \
         meeting_webhook_url, meeting_webhook_auth_mode, \
         meeting_webhook_secret, meeting_webhook_display_name \
         FROM auth_config WHERE id = 'singleton'",
    )
    .fetch_optional(pool)
    .await
    .ok()
    .flatten();

    let Some((jitsi_url, jitsi_pat, jitsi_name, hook_url, hook_mode, hook_secret_enc, hook_name)) =
        row
    else {
        return MeetingConfig::default();
    };

    let jitsi = jitsi_url
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|base_url| JitsiConfig {
            base_url: base_url.to_string(),
            pattern: jitsi_pat
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .unwrap_or(DEFAULT_JITSI_PATTERN)
                .to_string(),
            display_name: jitsi_name
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string),
        });

    let webhook = hook_url
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|url| {
            let auth_mode =
                WebhookAuthMode::from_str(hook_mode.as_deref().unwrap_or(WEBHOOK_AUTH_NONE));
            let secret = match (auth_mode, hook_secret_enc.as_deref()) {
                (WebhookAuthMode::Hmac, Some(enc)) if !enc.is_empty() => {
                    crate::crypto::decrypt_value(key, enc).unwrap_or_default()
                }
                _ => String::new(),
            };
            WebhookConfig {
                url: url.to_string(),
                auth_mode,
                secret,
                display_name: hook_name
                    .as_deref()
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_string),
            }
        });

    MeetingConfig { jitsi, webhook }
}

/// Pick the guest-facing label for a `location_type`, falling back to `None`
/// for non-auto providers. The caller decides what to show when `None`
/// (typically the generic "Video call" badge).
pub fn provider_label(location_type: &str, cfg: &MeetingConfig) -> Option<String> {
    match location_type {
        LOCATION_TYPE_JITSI => cfg.jitsi.as_ref().and_then(|j| j.display_name.clone()),
        LOCATION_TYPE_WEBHOOK => cfg.webhook.as_ref().and_then(|w| w.display_name.clone()),
        _ => None,
    }
}

/// Expand `{username}`, `{event}`, `{date}`, `{random}` in `pattern`.
///
/// Unknown placeholders are kept verbatim (e.g. `{foo}` stays `{foo}`) so a
/// typo in the admin panel is loud rather than silently swallowed.
pub fn expand_pattern(pattern: &str, tokens: &PatternTokens<'_>) -> String {
    let mut out = String::with_capacity(pattern.len() + 16);
    let mut chars = pattern.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '{' {
            out.push(c);
            continue;
        }
        let mut name = String::new();
        let mut closed = false;
        for nc in chars.by_ref() {
            if nc == '}' {
                closed = true;
                break;
            }
            name.push(nc);
        }
        if !closed {
            out.push('{');
            out.push_str(&name);
            continue;
        }
        match name.as_str() {
            "username" => out.push_str(&sanitize_for_url(tokens.username)),
            "event" => out.push_str(&sanitize_for_url(tokens.event_slug)),
            "date" => out.push_str(&extract_date(tokens.start_at)),
            "random" => out.push_str(&random_alphanumeric(8)),
            other => {
                out.push('{');
                out.push_str(other);
                out.push('}');
            }
        }
    }
    out
}

/// Build the Jitsi room URL by expanding the pattern and joining to `base_url`.
///
/// `override_pattern` (per-event-type) wins over `cfg.pattern` (org default).
pub fn build_jitsi_url(
    cfg: &JitsiConfig,
    override_pattern: Option<&str>,
    tokens: &PatternTokens<'_>,
) -> String {
    let pattern = override_pattern
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(&cfg.pattern);
    let pattern = if pattern.is_empty() {
        DEFAULT_JITSI_PATTERN
    } else {
        pattern
    };
    let room = expand_pattern(pattern, tokens);
    format!("{}/{}", cfg.base_url.trim_end_matches('/'), room)
}

#[derive(serde::Serialize)]
pub struct WebhookPayload<'a> {
    pub booking_uid: &'a str,
    pub event_slug: &'a str,
    pub host_username: &'a str,
    pub guest_name: &'a str,
    pub guest_email: &'a str,
    pub start_at: &'a str,
    pub end_at: &'a str,
}

#[derive(serde::Deserialize)]
struct WebhookResponse {
    url: String,
}

/// Call the configured webhook with the booking payload, expecting `{"url": ...}`
/// back. Returns the meeting URL on success.
///
/// When `auth_mode` is `Hmac` the request body is signed with HMAC-SHA256 over
/// the raw JSON body, hex-encoded, and sent in the `X-Calrs-Signature` header
/// as `sha256=<hex>`.
pub async fn call_webhook(cfg: &WebhookConfig, payload: &WebhookPayload<'_>) -> Result<String, ()> {
    let body = match serde_json::to_vec(payload) {
        Ok(b) => b,
        Err(e) => {
            tracing::error!(error = %e, "meeting webhook payload serialise failed");
            return Err(());
        }
    };

    let client = reqwest::Client::new();
    let mut req = client
        .post(&cfg.url)
        .header("content-type", "application/json")
        .header("user-agent", "calrs-meeting-webhook/1");

    if cfg.auth_mode == WebhookAuthMode::Hmac && !cfg.secret.is_empty() {
        let sig = sign_hmac_sha256(cfg.secret.as_bytes(), &body);
        req = req.header("X-Calrs-Signature", format!("sha256={}", sig));
    }

    let resp = match req
        .body(body)
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .await
    {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!(error = %e, "meeting webhook request failed");
            return Err(());
        }
    };

    if !resp.status().is_success() {
        tracing::warn!(status = %resp.status(), "meeting webhook returned non-2xx");
        return Err(());
    }

    let parsed: WebhookResponse = match resp.json().await {
        Ok(p) => p,
        Err(e) => {
            tracing::warn!(error = %e, "meeting webhook response parse failed");
            return Err(());
        }
    };

    let url = parsed.url.trim().to_string();
    if url.is_empty() {
        tracing::warn!("meeting webhook returned empty url");
        return Err(());
    }
    Ok(url)
}

/// Generate a meeting URL for a freshly-confirmed booking and persist it to
/// `bookings.meeting_url`. Returns `Some(url)` when an auto provider produced
/// a URL, `None` otherwise (event type uses a static location, the provider
/// is not configured, or a webhook call failed).
///
/// Looks up everything it needs from the DB so callers only have to hand over
/// the booking id, event type id, and assigned host user id. The "assigned"
/// host is whichever user the booking was routed to (for team / round-robin),
/// or the event type owner for personal event types. When `host_user_id` is
/// `None` (dynamic-group bookings have no single host) the `{username}` token
/// falls back to `"host"`.
pub async fn generate_and_persist(
    pool: &SqlitePool,
    secret_key: &[u8; 32],
    booking_id: &str,
    event_type_id: &str,
    host_user_id: Option<&str>,
    guest_name: &str,
    guest_email: &str,
) -> Option<String> {
    let et: Option<(String, String, Option<String>)> = sqlx::query_as(
        "SELECT location_type, slug, meeting_pattern_override \
         FROM event_types WHERE id = ?",
    )
    .bind(event_type_id)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten();
    let (location_type, event_slug, pattern_override) = et?;

    if location_type != LOCATION_TYPE_JITSI && location_type != LOCATION_TYPE_WEBHOOK {
        return None;
    }

    let booking: Option<(String, String, String)> =
        sqlx::query_as("SELECT uid, start_at, end_at FROM bookings WHERE id = ?")
            .bind(booking_id)
            .fetch_optional(pool)
            .await
            .ok()
            .flatten();
    let (booking_uid, start_at, end_at) = booking?;

    let host_username = match host_user_id {
        Some(uid) => {
            sqlx::query_scalar::<_, Option<String>>("SELECT username FROM users WHERE id = ?")
                .bind(uid)
                .fetch_optional(pool)
                .await
                .ok()
                .flatten()
                .flatten()
                .unwrap_or_else(|| "host".to_string())
        }
        None => "host".to_string(),
    };

    let tokens = PatternTokens {
        username: &host_username,
        event_slug: &event_slug,
        start_at: &start_at,
    };

    let cfg = load_config(pool, secret_key).await;

    let url = match location_type.as_str() {
        LOCATION_TYPE_JITSI => cfg
            .jitsi
            .as_ref()
            .map(|j| build_jitsi_url(j, pattern_override.as_deref(), &tokens)),
        LOCATION_TYPE_WEBHOOK => {
            let webhook_cfg = cfg.webhook.as_ref()?;
            let payload = WebhookPayload {
                booking_uid: &booking_uid,
                event_slug: &event_slug,
                host_username: &host_username,
                guest_name,
                guest_email,
                start_at: &start_at,
                end_at: &end_at,
            };
            call_webhook(webhook_cfg, &payload).await.ok()
        }
        _ => None,
    }?;

    let _ = sqlx::query("UPDATE bookings SET meeting_url = ? WHERE id = ?")
        .bind(&url)
        .bind(booking_id)
        .execute(pool)
        .await;

    Some(url)
}

/// Hex-encoded HMAC-SHA256 of `body` keyed by `secret`.
pub fn sign_hmac_sha256(secret: &[u8], body: &[u8]) -> String {
    use hmac::{Hmac, Mac};
    use sha2::Sha256;
    type HmacSha256 = Hmac<Sha256>;
    let mut mac = HmacSha256::new_from_slice(secret).expect("HMAC accepts any key length");
    mac.update(body);
    let tag = mac.finalize().into_bytes();
    hex::encode(tag)
}

/// Restrict an arbitrary user-supplied string (a username or slug) to URL-safe
/// chars. Anything outside `[A-Za-z0-9_-]` is dropped. Lowercased.
fn sanitize_for_url(s: &str) -> String {
    let cleaned: String = s
        .trim()
        .to_lowercase()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .collect();
    if cleaned.is_empty() {
        "x".to_string()
    } else {
        cleaned
    }
}

/// Pull the YYYYMMDD prefix out of an ISO8601 datetime stored in
/// `bookings.start_at`. Falls back to an empty string if the value is shorter
/// than 10 chars (which would be invalid).
fn extract_date(start_at: &str) -> String {
    if start_at.len() < 10 {
        return String::new();
    }
    let (y, m, d) = (&start_at[0..4], &start_at[5..7], &start_at[8..10]);
    if y.chars().all(|c| c.is_ascii_digit())
        && m.chars().all(|c| c.is_ascii_digit())
        && d.chars().all(|c| c.is_ascii_digit())
    {
        format!("{}{}{}", y, m, d)
    } else {
        String::new()
    }
}

/// Generate `n` cryptographically-random alphanumeric characters using OsRng.
/// Used for `{random}` and elsewhere we want a non-guessable room suffix.
pub fn random_alphanumeric(n: usize) -> String {
    const ALPHABET: &[u8] = b"abcdefghijklmnopqrstuvwxyz0123456789";
    let mut out = String::with_capacity(n);
    let mut buf = vec![0u8; n];
    rand::rngs::OsRng.fill_bytes(&mut buf);
    for &b in &buf {
        out.push(ALPHABET[(b as usize) % ALPHABET.len()] as char);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expand_pattern_replaces_known_tokens() {
        let tokens = PatternTokens {
            username: "alice",
            event_slug: "intro-call",
            start_at: "2026-06-05T10:00:00",
        };
        let out = expand_pattern("{username}-{event}-{date}", &tokens);
        assert_eq!(out, "alice-intro-call-20260605");
    }

    #[test]
    fn expand_pattern_keeps_unknown_tokens_verbatim() {
        let tokens = PatternTokens {
            username: "alice",
            event_slug: "x",
            start_at: "2026-06-05",
        };
        let out = expand_pattern("hello-{foo}-{username}", &tokens);
        assert_eq!(out, "hello-{foo}-alice");
    }

    #[test]
    fn expand_pattern_random_is_8_alphanumeric() {
        let tokens = PatternTokens {
            username: "a",
            event_slug: "b",
            start_at: "2026-06-05",
        };
        let out = expand_pattern("{random}", &tokens);
        assert_eq!(out.len(), 8);
        assert!(out
            .chars()
            .all(|c| c.is_ascii_alphanumeric() && !c.is_ascii_uppercase()));
    }

    #[test]
    fn expand_pattern_unterminated_brace_kept() {
        let tokens = PatternTokens {
            username: "a",
            event_slug: "b",
            start_at: "2026-06-05",
        };
        let out = expand_pattern("oops-{username", &tokens);
        assert_eq!(out, "oops-{username");
    }

    #[test]
    fn sanitize_for_url_strips_unsafe_chars() {
        assert_eq!(sanitize_for_url("Alice O'Brien"), "aliceobrien");
        assert_eq!(sanitize_for_url("../etc/passwd"), "etcpasswd");
        assert_eq!(sanitize_for_url(""), "x");
        assert_eq!(sanitize_for_url("a_b-c"), "a_b-c");
    }

    #[test]
    fn extract_date_handles_iso_with_T() {
        assert_eq!(extract_date("2026-06-05T10:00:00"), "20260605");
    }

    #[test]
    fn extract_date_handles_iso_with_space() {
        assert_eq!(extract_date("2026-06-05 10:00:00"), "20260605");
    }

    #[test]
    fn extract_date_rejects_bad_input() {
        // Short and non-numeric inputs are rejected. The function does not
        // enforce a specific separator between Y/M/D — `bookings.start_at`
        // is always written in `YYYY-MM-DDTHH:MM:SS` form by calrs itself.
        assert_eq!(extract_date("nope"), "");
        assert_eq!(extract_date("short"), "");
        assert_eq!(extract_date("abcd-ef-gh"), "");
    }

    #[test]
    fn build_jitsi_url_uses_override_when_set() {
        let cfg = JitsiConfig {
            base_url: "https://meet.dyb.fr".to_string(),
            pattern: "{event}-{random}".to_string(),
            display_name: None,
        };
        let tokens = PatternTokens {
            username: "alice",
            event_slug: "intro",
            start_at: "2026-06-05",
        };
        let url = build_jitsi_url(&cfg, Some("custom-{username}"), &tokens);
        assert_eq!(url, "https://meet.dyb.fr/custom-alice");
    }

    #[test]
    fn build_jitsi_url_falls_back_to_default_pattern() {
        let cfg = JitsiConfig {
            base_url: "https://meet.dyb.fr/".to_string(),
            pattern: String::new(),
            display_name: None,
        };
        let tokens = PatternTokens {
            username: "alice",
            event_slug: "intro",
            start_at: "2026-06-05",
        };
        let url = build_jitsi_url(&cfg, None, &tokens);
        // default pattern is "{event}-{random}" → "intro-<8 chars>"
        assert!(url.starts_with("https://meet.dyb.fr/intro-"));
        assert_eq!(url.len(), "https://meet.dyb.fr/intro-".len() + 8);
    }

    #[test]
    fn hmac_sha256_known_vector() {
        // RFC 4231 test case 1
        let key = b"\x0b".repeat(20);
        let data = b"Hi There";
        let sig = sign_hmac_sha256(&key, data);
        assert_eq!(
            sig,
            "b0344c61d8db38535ca8afceaf0bf12b881dc200c9833da726e9376c2e32cff7"
        );
    }

    #[test]
    fn random_alphanumeric_lengths() {
        assert_eq!(random_alphanumeric(0).len(), 0);
        assert_eq!(random_alphanumeric(1).len(), 1);
        assert_eq!(random_alphanumeric(32).len(), 32);
    }

    #[test]
    fn webhook_auth_mode_roundtrip() {
        assert_eq!(WebhookAuthMode::from_str("hmac").as_str(), "hmac");
        assert_eq!(WebhookAuthMode::from_str("none").as_str(), "none");
        assert_eq!(WebhookAuthMode::from_str("").as_str(), "none");
        assert_eq!(WebhookAuthMode::from_str("garbage").as_str(), "none");
    }
}
