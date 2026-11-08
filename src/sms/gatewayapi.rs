//! GatewayAPI adapter.
//!
//! `POST {base}/rest/mtsms` with a JSON body, authenticated by
//! `Authorization: Token <api token>`. The base URL selects the region:
//! `https://gatewayapi.com` (default) or `https://gatewayapi.eu`.
//!
//! Recipients go on the wire as `msisdn` digits without the leading `+`, and
//! the sender may be up to 11 alphanumeric characters or 15 digits.
//! Reference: <https://gatewayapi.com/docs/apis/rest/>.

use async_trait::async_trait;

use super::{factory, phone, SendReceipt, SmsConfig, SmsError, SmsProvider};

pub struct GatewayApiProvider {
    token: zeroize::Zeroizing<String>,
    sender: String,
    base_url: String,
}

impl GatewayApiProvider {
    pub fn new(config: &SmsConfig) -> Self {
        Self {
            token: config.api_secret.clone(),
            sender: config.sender.trim().to_string(),
            base_url: factory::base_url(config),
        }
    }
}

#[derive(serde::Serialize)]
struct Recipient {
    msisdn: String,
}

#[derive(serde::Serialize)]
struct SendRequest<'a> {
    sender: &'a str,
    message: &'a str,
    recipients: Vec<Recipient>,
}

#[derive(serde::Deserialize)]
struct SendResponse {
    ids: Option<Vec<serde_json::Value>>,
    usage: Option<Usage>,
}

#[derive(serde::Deserialize)]
struct Usage {
    currency: Option<String>,
    total_cost: Option<f64>,
}

#[derive(serde::Deserialize)]
struct ErrorResponse {
    code: Option<String>,
    message: Option<String>,
}

/// Map an HTTP status plus GatewayAPI's error body onto a normalised error.
///
/// GatewayAPI documents its failures by HTTP status (401 invalid key, 403
/// unauthorised IP, 422 invalid JSON) and carries a hexadecimal `code` plus a
/// human message in the body. The status is the reliable signal, so the codes
/// are passed through in the message rather than guessed at.
fn parse_error(status: u16, body: &str) -> SmsError {
    let parsed: Option<ErrorResponse> = serde_json::from_str(body).ok();
    let message = match parsed {
        Some(ErrorResponse {
            code: Some(code),
            message: Some(msg),
        }) => format!("{} ({})", msg, code),
        Some(ErrorResponse {
            message: Some(msg), ..
        }) => msg,
        _ => body.trim().chars().take(200).collect(),
    };

    match status {
        401 | 403 => SmsError::Auth(message),
        429 => SmsError::RateLimited(message),
        500..=599 => SmsError::Transport(message),
        _ => SmsError::Other(message),
    }
}

#[async_trait]
impl SmsProvider for GatewayApiProvider {
    fn kind(&self) -> &'static str {
        factory::kinds::GATEWAYAPI
    }

    async fn send(&self, to: &str, body: &str) -> Result<SendReceipt, SmsError> {
        let url = format!("{}/rest/mtsms", self.base_url);
        let payload = SendRequest {
            sender: &self.sender,
            message: body,
            recipients: vec![Recipient {
                msisdn: phone::without_plus(to),
            }],
        };

        let response = super::http_client()
            .post(&url)
            .header("Authorization", format!("Token {}", self.token.as_str()))
            .json(&payload)
            .send()
            .await
            .map_err(|e| SmsError::Transport(e.to_string()))?;

        let status = response.status();
        let text = response.text().await.unwrap_or_default();

        if !status.is_success() {
            return Err(parse_error(status.as_u16(), &text));
        }

        let parsed: SendResponse = serde_json::from_str(&text).unwrap_or(SendResponse {
            ids: None,
            usage: None,
        });

        Ok(SendReceipt {
            message_id: parsed
                .ids
                .and_then(|ids| ids.first().map(|id| id.to_string())),
            segments: None,
            cost: parsed.usage.as_ref().and_then(|u| u.total_cost),
            currency: parsed.usage.and_then(|u| u.currency),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_rejected_credentials_and_blocked_ip() {
        let body = r#"{"code": "0x0213", "message": "Invalid API key", "incident_uuid": "abc"}"#;
        match parse_error(401, body) {
            SmsError::Auth(m) => assert!(m.contains("Invalid API key") && m.contains("0x0213")),
            other => panic!("unexpected mapping: {:?}", other),
        }
        assert!(matches!(parse_error(403, "{}"), SmsError::Auth(_)));
    }

    #[test]
    fn keeps_the_gateway_message_for_other_statuses() {
        let body = r#"{"code": "0x0501", "message": "Invalid recipient"}"#;
        assert!(matches!(parse_error(400, body), SmsError::Other(_)));
        assert!(matches!(parse_error(502, "{}"), SmsError::Transport(_)));
    }

    #[test]
    fn falls_back_to_the_raw_body_when_it_is_not_json() {
        match parse_error(400, "Bad Request") {
            SmsError::Other(m) => assert_eq!(m, "Bad Request"),
            other => panic!("unexpected mapping: {:?}", other),
        }
    }

    #[test]
    fn recipients_go_on_the_wire_without_the_plus() {
        assert_eq!(phone::without_plus("+4512345678"), "4512345678");
    }
}
