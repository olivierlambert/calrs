//! Twilio adapter.
//!
//! `POST /2010-04-01/Accounts/{AccountSid}/Messages.json`, form-encoded, HTTP
//! Basic with the Account SID as the username. Twilio wants recipients in full
//! E.164 with the leading `+`.
//!
//! Errors arrive as JSON with a numeric `code`; the ones worth distinguishing
//! are mapped onto [`SmsError`], the rest keep Twilio's own message.
//! Reference: <https://www.twilio.com/docs/api/errors>.

use async_trait::async_trait;

use super::{factory, SendReceipt, SmsConfig, SmsError, SmsProvider};

/// Twilio-provided template used in trial mode, picked from the list at
/// <https://www.twilio.com/docs/usage/trials/try-out-sms> as the closest in
/// meaning to what calrs actually sends.
const TRIAL_TEMPLATE: &str = "sms_appointment_reminders";

/// Switch for [trial mode](trial_mode_enabled), read from the environment only.
const TRIAL_ENV_VAR: &str = "CALRS_SMS_TWILIO_TRIAL";

/// Whether to send Twilio's predefined template instead of the real message.
///
/// Twilio trial accounts refuse custom message bodies outright: `Body` has to
/// carry the *name* of one of their predefined templates. That makes the whole
/// Twilio path untestable without a paid account, which is a poor deal for a
/// contributor who just wants to check that a booking reaches a phone.
///
/// Deliberately environment-only, and deliberately not part of the
/// `CALRS_SMS_*` block (it composes with a database-stored config too). An
/// `sms_config` column or an admin field would let an operator flip it and
/// quietly ship canned templates to real guests; turning it on has to be an act
/// of whoever runs the process. `CALRS_SMS_<PROVIDER>_<OPTION>` is the shape to
/// follow for any future gateway-specific extra.
pub fn trial_mode_enabled() -> bool {
    matches!(
        std::env::var(TRIAL_ENV_VAR)
            .ok()
            .map(|v| v.trim().to_ascii_lowercase())
            .as_deref(),
        Some("1" | "true" | "yes" | "on")
    )
}

pub struct TwilioProvider {
    account_sid: String,
    auth_token: zeroize::Zeroizing<String>,
    from: String,
    base_url: String,
    trial: bool,
}

impl TwilioProvider {
    pub fn new(config: &SmsConfig) -> Self {
        Self {
            account_sid: config.api_key.trim().to_string(),
            auth_token: config.api_secret.clone(),
            from: config.sender.trim().to_string(),
            base_url: factory::base_url(config),
            trial: trial_mode_enabled(),
        }
    }
}

/// The value to put in Twilio's `Body` parameter.
///
/// A substitution rather than an extra parameter, because that is what the
/// trial API asks for: the request keeps the same `To`/`From`/`Body` shape on
/// both paths, so the response, and therefore [`SendReceipt`] parsing, is
/// identical whether or not trial mode is on.
fn request_body(trial: bool, body: &str) -> &str {
    if trial {
        TRIAL_TEMPLATE
    } else {
        body
    }
}

#[derive(serde::Deserialize)]
struct MessageResponse {
    sid: Option<String>,
    num_segments: Option<String>,
    price: Option<String>,
    price_unit: Option<String>,
}

#[derive(serde::Deserialize)]
struct ErrorResponse {
    code: Option<i64>,
    message: Option<String>,
}

#[derive(serde::Deserialize)]
struct AccountResponse {
    #[serde(rename = "type")]
    account_type: Option<String>,
}

/// Guard the one way trial mode can cost money instead of saving it.
///
/// On a trial account `Body` must name a template. On a full account that same
/// value is just text, so a flag left set after the account is upgraded texts
/// every guest the literal string `sms_appointment_reminders`, at full price,
/// instead of their booking details. The credential check already fetches the
/// account, so it can say so without spending anything.
///
/// The opposite mistake (a trial account with the flag off) needs no guard
/// here: it fails closed at send time with Twilio's own refusal, which
/// [`parse_error`] already surfaces.
fn trial_mismatch(trial: bool, account_json: &str) -> Option<SmsError> {
    if !trial {
        return None;
    }
    let parsed: AccountResponse = serde_json::from_str(account_json).ok()?;
    match parsed.account_type.as_deref() {
        Some("Full") => Some(SmsError::Other(format!(
            "{} is set but this is a full Twilio account, not a trial one. \
             Guests would receive the literal text \"{}\" instead of their booking \
             details, and you would be billed for it. Unset the variable.",
            TRIAL_ENV_VAR, TRIAL_TEMPLATE
        ))),
        _ => None,
    }
}

/// Map an HTTP status plus Twilio's JSON error body onto a normalised error.
///
/// Split out from the request so it can be tested against the payloads in
/// Twilio's documentation without a network call or a mock server.
fn parse_error(status: u16, body: &str) -> SmsError {
    let parsed: Option<ErrorResponse> = serde_json::from_str(body).ok();
    let code = parsed.as_ref().and_then(|e| e.code);
    let message = parsed
        .as_ref()
        .and_then(|e| e.message.clone())
        .unwrap_or_else(|| body.trim().chars().take(200).collect());

    match (status, code) {
        (_, Some(21211)) | (_, Some(21614)) | (_, Some(21610)) => {
            SmsError::InvalidRecipient(message)
        }
        (_, Some(21212)) | (_, Some(21606)) | (_, Some(21659)) | (_, Some(21660)) => {
            SmsError::InvalidSender(message)
        }
        (_, Some(20003)) | (401, _) | (403, _) => SmsError::Auth(message),
        (429, _) | (_, Some(20429)) => SmsError::RateLimited(message),
        (_, Some(21608)) => SmsError::Other(format!(
            "{} (trial accounts can only message verified numbers)",
            message
        )),
        (s, _) if (500..600).contains(&s) => SmsError::Transport(message),
        _ => SmsError::Other(message),
    }
}

#[async_trait]
impl SmsProvider for TwilioProvider {
    fn kind(&self) -> &'static str {
        factory::kinds::TWILIO
    }

    async fn send(&self, to: &str, body: &str) -> Result<SendReceipt, SmsError> {
        let url = format!(
            "{}/2010-04-01/Accounts/{}/Messages.json",
            self.base_url, self.account_sid
        );
        if self.trial {
            // Loud on purpose: with the template substituted, all four
            // SmsEvent kinds look identical on the handset, so the log is the
            // only place a tester can tell which one fired. The composed body
            // is still built and logged, which keeps message.rs on the
            // exercised path in the only mode most contributors can run.
            tracing::warn!(
                template = TRIAL_TEMPLATE,
                "Twilio trial mode is on; sending a predefined template instead of the composed message"
            );
            tracing::debug!(body = %body, "composed SMS body (replaced by the trial template)");
        }

        let params = [
            ("To", to),
            ("From", self.from.as_str()),
            ("Body", request_body(self.trial, body)),
        ];

        let response = super::http_client()
            .post(&url)
            .basic_auth(&self.account_sid, Some(self.auth_token.as_str()))
            .form(&params)
            .send()
            .await
            .map_err(|e| SmsError::Transport(e.to_string()))?;

        let status = response.status();
        let text = response.text().await.unwrap_or_default();

        if !status.is_success() {
            return Err(parse_error(status.as_u16(), &text));
        }

        let parsed: MessageResponse = serde_json::from_str(&text).unwrap_or(MessageResponse {
            sid: None,
            num_segments: None,
            price: None,
            price_unit: None,
        });

        Ok(SendReceipt {
            message_id: parsed.sid,
            segments: parsed.num_segments.and_then(|s| s.parse().ok()),
            // Twilio reports the price as a negative string ("-0.0750") and
            // often only after the message has left the queue.
            cost: parsed
                .price
                .and_then(|p| p.parse::<f64>().ok())
                .map(f64::abs),
            currency: parsed.price_unit,
        })
    }

    async fn check(&self) -> Result<(), SmsError> {
        let url = format!(
            "{}/2010-04-01/Accounts/{}.json",
            self.base_url, self.account_sid
        );
        let response = super::http_client()
            .get(&url)
            .basic_auth(&self.account_sid, Some(self.auth_token.as_str()))
            .send()
            .await
            .map_err(|e| SmsError::Transport(e.to_string()))?;

        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        if !status.is_success() {
            return Err(parse_error(status.as_u16(), &text));
        }

        match trial_mismatch(self.trial, &text) {
            Some(e) => Err(e),
            None => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_invalid_recipient() {
        let body = r#"{"code": 21211, "message": "The 'To' number is not a valid phone number.", "status": 400}"#;
        assert!(matches!(
            parse_error(400, body),
            SmsError::InvalidRecipient(_)
        ));
    }

    #[test]
    fn maps_bad_credentials() {
        let body = r#"{"code": 20003, "message": "Authenticate", "status": 401}"#;
        assert!(matches!(parse_error(401, body), SmsError::Auth(_)));
        // Even without a parseable body, a 401 is an auth failure.
        assert!(matches!(parse_error(401, "nope"), SmsError::Auth(_)));
    }

    #[test]
    fn maps_sender_and_rate_limit_and_server_errors() {
        let sender =
            r#"{"code": 21606, "message": "The From number is not a valid, SMS-capable number"}"#;
        assert!(matches!(
            parse_error(400, sender),
            SmsError::InvalidSender(_)
        ));
        assert!(matches!(parse_error(429, "{}"), SmsError::RateLimited(_)));
        assert!(matches!(parse_error(503, "{}"), SmsError::Transport(_)));
    }

    #[test]
    fn trial_mode_substitutes_the_predefined_template() {
        let composed = "Your booking is confirmed for 3 Sep at 14:00 (Europe/Paris).";
        assert_eq!(request_body(false, composed), composed);
        assert_eq!(request_body(true, composed), TRIAL_TEMPLATE);
        // Trial accounts match the template name exactly; a body that merely
        // contains it is still a custom body and would be refused.
        assert_eq!(TRIAL_TEMPLATE, "sms_appointment_reminders");
    }

    #[test]
    fn trial_mode_on_a_full_account_is_refused() {
        let full = r#"{"sid": "AC123", "status": "active", "type": "Full"}"#;
        let trial = r#"{"sid": "AC123", "status": "active", "type": "Trial"}"#;

        // The expensive mistake: the flag outliving the trial account.
        assert!(matches!(
            trial_mismatch(true, full),
            Some(SmsError::Other(_))
        ));
        assert!(trial_mismatch(true, trial).is_none());

        // With trial mode off, the account type is none of our business.
        assert!(trial_mismatch(false, full).is_none());

        // An unparseable or unexpected payload must not turn a working
        // credential check into a failure.
        assert!(trial_mismatch(true, "not json").is_none());
        assert!(trial_mismatch(true, "{}").is_none());
    }

    #[test]
    fn keeps_the_gateway_message_for_unmapped_codes() {
        let body = r#"{"code": 30001, "message": "Queue overflow"}"#;
        match parse_error(400, body) {
            SmsError::Other(m) => assert_eq!(m, "Queue overflow"),
            other => panic!("unexpected mapping: {:?}", other),
        }
    }
}
