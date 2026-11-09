//! seven.io adapter.
//!
//! `POST {base}/api/sms`, form-encoded, authenticated with an `X-Api-Key`
//! header. `Accept: application/json` is required to get a structured body.
//!
//! The important quirk: seven.io answers **HTTP 200 for failures too**, and
//! puts the outcome in a `success` string ("100" accepted, "900" auth failed,
//! "500" out of credit). Parsing that into [`SmsError`] is exactly the kind of
//! per-gateway knowledge the trait exists to contain.
//! Reference: <https://docs.seven.io/en/rest-api/endpoints/sms>.

use async_trait::async_trait;

use super::{factory, phone, SendReceipt, SmsConfig, SmsError, SmsProvider};

pub struct SevenIoProvider {
    api_key: zeroize::Zeroizing<String>,
    sender: String,
    base_url: String,
}

impl SevenIoProvider {
    pub fn new(config: &SmsConfig) -> Self {
        Self {
            api_key: config.api_secret.clone(),
            sender: config.sender.trim().to_string(),
            base_url: factory::base_url(config),
        }
    }
}

#[derive(serde::Deserialize)]
struct SendResponse {
    success: Option<String>,
    total_price: Option<f64>,
    messages: Option<Vec<MessageResult>>,
}

#[derive(serde::Deserialize)]
struct MessageResult {
    id: Option<String>,
    parts: Option<u32>,
    success: Option<bool>,
    error_text: Option<String>,
}

/// Map seven.io's `success` code onto a normalised error, or `Ok` for "100".
fn parse_status(code: &str, detail: Option<&str>) -> Result<(), SmsError> {
    let message = match detail {
        Some(d) if !d.trim().is_empty() => format!("{} (code {})", d.trim(), code),
        _ => format!("code {}", code),
    };

    match code {
        "100" => Ok(()),
        "900" | "902" | "903" => Err(SmsError::Auth(message)),
        "500" => Err(SmsError::InsufficientCredit(message)),
        "101" | "202" => Err(SmsError::InvalidRecipient(message)),
        "301" | "305" => Err(SmsError::Other(message)),
        _ => Err(SmsError::Other(message)),
    }
}

/// Parse a 200 response body into a receipt or a normalised error.
///
/// seven.io answers HTTP 200 whether or not it sent, and says which in the
/// body, so a 200 whose body cannot be read may well have been sent:
/// [`SmsError::Transport`], which keeps the cap's reservation.
fn parse_response(body: &str) -> Result<SendReceipt, SmsError> {
    let parsed: SendResponse = serde_json::from_str(body).map_err(|_| {
        SmsError::Transport(format!(
            "unreadable seven.io answer: {}",
            body.trim().chars().take(200).collect::<String>()
        ))
    })?;

    let first = parsed.messages.as_ref().and_then(|m| m.first());
    let detail = first.and_then(|m| m.error_text.as_deref());
    let code = parsed.success.as_deref().unwrap_or("");
    parse_status(code, detail)?;

    // "100" is an accepted batch; an individual recipient can still fail.
    if let Some(message) = first {
        if message.success == Some(false) {
            return Err(SmsError::InvalidRecipient(
                message
                    .error_text
                    .clone()
                    .unwrap_or_else(|| "recipient rejected".to_string()),
            ));
        }
    }

    Ok(SendReceipt {
        message_id: first.and_then(|m| m.id.clone()),
        segments: first.and_then(|m| m.parts),
        cost: parsed.total_price,
        currency: Some("EUR".to_string()),
    })
}

#[async_trait]
impl SmsProvider for SevenIoProvider {
    fn kind(&self) -> &'static str {
        factory::kinds::SEVENIO
    }

    async fn send(&self, to: &str, body: &str) -> Result<SendReceipt, SmsError> {
        let url = format!("{}/api/sms", self.base_url);
        let recipient = phone::without_plus(to);
        let mut params = vec![("to", recipient.as_str()), ("text", body)];
        if !self.sender.is_empty() {
            params.push(("from", self.sender.as_str()));
        }

        let response = super::http_client()
            .post(&url)
            .header("X-Api-Key", self.api_key.as_str())
            .header("Accept", "application/json")
            .form(&params)
            .send()
            .await
            .map_err(super::request_error)?;

        let status = response.status();
        let text = response.text().await.unwrap_or_default();

        // Transport-level failures still surface as real status codes.
        if !status.is_success() {
            let message = text.trim().chars().take(200).collect::<String>();
            return Err(match status.as_u16() {
                401 | 403 => SmsError::Auth(message),
                429 => SmsError::RateLimited(message),
                500..=599 => SmsError::Transport(message),
                _ => SmsError::Other(message),
            });
        }

        parse_response(&text)
    }

    async fn check(&self) -> Result<(), SmsError> {
        let url = format!("{}/api/balance", self.base_url);
        let response = super::http_client()
            .get(&url)
            .header("X-Api-Key", self.api_key.as_str())
            .header("Accept", "application/json")
            .send()
            .await
            .map_err(super::request_error)?;

        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        if !status.is_success() {
            return Err(SmsError::Auth(text.trim().chars().take(200).collect()));
        }
        // An invalid key answers 200 with a bare "900" rather than a balance.
        if text.trim() == "900" {
            return Err(SmsError::Auth("code 900".to_string()));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const OK_BODY: &str = r#"{"success":"100","total_price":0.075,"balance":593.994,
        "messages":[{"id":"77229318510","recipient":"49123456789","parts":1,
        "price":0.075,"success":true,"error":null,"error_text":null}]}"#;

    #[test]
    fn accepts_a_successful_send() {
        let receipt = parse_response(OK_BODY).expect("should parse");
        assert_eq!(receipt.message_id.as_deref(), Some("77229318510"));
        assert_eq!(receipt.segments, Some(1));
        assert_eq!(receipt.cost, Some(0.075));
    }

    #[test]
    fn treats_an_http_200_auth_failure_as_an_error() {
        let err = parse_response(r#"{"success":"900"}"#).unwrap_err();
        assert!(matches!(err, SmsError::Auth(_)), "got {:?}", err);
    }

    #[test]
    fn maps_credit_and_recipient_failures() {
        assert!(matches!(
            parse_response(r#"{"success":"500"}"#).unwrap_err(),
            SmsError::InsufficientCredit(_)
        ));
        assert!(matches!(
            parse_response(r#"{"success":"202"}"#).unwrap_err(),
            SmsError::InvalidRecipient(_)
        ));
    }

    #[test]
    fn rejects_a_per_recipient_failure_inside_an_accepted_batch() {
        let body = r#"{"success":"100","messages":[{"id":"1","success":false,
            "error_text":"invalid number"}]}"#;
        match parse_response(body).unwrap_err() {
            SmsError::InvalidRecipient(m) => assert_eq!(m, "invalid number"),
            other => panic!("unexpected mapping: {:?}", other),
        }
    }

    #[test]
    fn an_unreadable_200_may_have_been_sent() {
        // seven.io says in the body whether it sent; without a readable body
        // the outcome is unknown, so the message stays counted.
        assert!(matches!(
            parse_response("<html>oops</html>").unwrap_err(),
            SmsError::Transport(_)
        ));
        assert!(matches!(
            parse_response("").unwrap_err(),
            SmsError::Transport(_)
        ));
    }
}
