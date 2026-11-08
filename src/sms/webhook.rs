//! Generic webhook adapter: bring your own gateway.
//!
//! calrs POSTs `{"to", "text", "sender"}` to the configured URL and treats any
//! 2xx as accepted. If the receiver answers with `{"id": "..."}` that id is
//! kept for the logs. When an HMAC secret is configured the raw body is signed
//! with HMAC-SHA256 and sent as `X-Calrs-Signature: sha256=<hex>`, the same
//! scheme as the meeting webhook, so the receiver can prove the call came from
//! calrs.
//!
//! This exists so a gateway calrs has no adapter for (an in-house SMPP bridge,
//! Kannel, a national operator's API) is a small script away rather than a
//! feature request.
//!
//! ## SSRF posture
//!
//! Same reasoning as the meeting webhook: the URL is admin-configured only and
//! is deliberately not run through the private-host guard, because pointing it
//! at a bridge on localhost is the main use case.

use async_trait::async_trait;

use super::{factory, SendReceipt, SmsConfig, SmsError, SmsProvider};

pub struct WebhookProvider {
    url: String,
    sender: String,
    secret: zeroize::Zeroizing<String>,
}

impl WebhookProvider {
    pub fn new(config: &SmsConfig) -> Self {
        Self {
            url: config.base_url.trim().to_string(),
            sender: config.sender.trim().to_string(),
            secret: config.api_secret.clone(),
        }
    }
}

#[derive(serde::Serialize)]
struct Payload<'a> {
    to: &'a str,
    text: &'a str,
    sender: &'a str,
}

#[derive(serde::Deserialize)]
struct Response {
    id: Option<String>,
}

#[async_trait]
impl SmsProvider for WebhookProvider {
    fn kind(&self) -> &'static str {
        factory::kinds::WEBHOOK
    }

    async fn send(&self, to: &str, text: &str) -> Result<SendReceipt, SmsError> {
        let payload = Payload {
            to,
            text,
            sender: &self.sender,
        };
        let body = serde_json::to_vec(&payload)
            .map_err(|e| SmsError::Other(format!("payload serialise failed: {}", e)))?;

        let mut request = super::http_client()
            .post(&self.url)
            .header("content-type", "application/json");

        if !self.secret.is_empty() {
            let signature = crate::web::meeting::sign_hmac_sha256(self.secret.as_bytes(), &body);
            request = request.header("X-Calrs-Signature", format!("sha256={}", signature));
        }

        let response = request
            .body(body)
            .send()
            .await
            .map_err(|e| SmsError::Transport(e.to_string()))?;

        let status = response.status();
        let text = response.text().await.unwrap_or_default();

        if !status.is_success() {
            let message = format!(
                "webhook returned {}: {}",
                status,
                text.trim().chars().take(200).collect::<String>()
            );
            return Err(match status.as_u16() {
                401 | 403 => SmsError::Auth(message),
                429 => SmsError::RateLimited(message),
                500..=599 => SmsError::Transport(message),
                _ => SmsError::Other(message),
            });
        }

        let parsed: Option<Response> = serde_json::from_str(&text).ok();
        Ok(SendReceipt {
            message_id: parsed.and_then(|r| r.id),
            ..Default::default()
        })
    }
}
