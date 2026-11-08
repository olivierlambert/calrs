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
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

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
    auth_token: String,
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

    /// Identifies the configuration the account type was fetched for. A hash
    /// rather than the fields, so the cache never holds the auth token.
    fn cache_key(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        (&self.base_url, &self.account_sid, &self.auth_token).hash(&mut hasher);
        hasher.finish()
    }

    /// `GET /Accounts/{SID}.json`: free, and what both the credential check
    /// and the trial-mode send guard read the account type from.
    async fn fetch_account(&self) -> Result<String, SmsError> {
        let url = format!(
            "{}/2010-04-01/Accounts/{}.json",
            self.base_url, self.account_sid
        );
        let response = super::http_client()
            .get(&url)
            .basic_auth(&self.account_sid, Some(&self.auth_token))
            .send()
            .await
            .map_err(super::request_error)?;

        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        if !status.is_success() {
            return Err(parse_error(status.as_u16(), &text));
        }
        if let Ok(mut cache) = account_types().lock() {
            remember_account_type(
                &mut cache,
                self.cache_key(),
                is_full_account(&text),
                Instant::now(),
            );
        }
        Ok(text)
    }

    /// The send-time half of [`trial_mismatch`]: in trial mode, refuse to send
    /// the template to a full account. The account type is fetched before the
    /// first send and again after [`ACCOUNT_TYPE_TTL`] or a configuration
    /// change. A failed fetch refuses too, without caching: whether the
    /// template would be billed as text is unknown, and this mode is a testing
    /// aid, so not sending is the cheap side. Either way the caller logs it and
    /// the booking goes on, as for any SMS failure.
    async fn refuse_full_account(&self) -> Result<(), SmsError> {
        let key = self.cache_key();
        let cached = account_types()
            .lock()
            .ok()
            .and_then(|cache| cached_full_account(&cache, key, Instant::now()));
        let full = match cached {
            Some(full) => full,
            // Nothing has been sent yet, so a failed lookup is a definite
            // refusal (`Other`), never `Transport`: the cap gives its
            // reservation back.
            None => is_full_account(&self.fetch_account().await.map_err(|e| {
                SmsError::Other(format!(
                    "could not read the Twilio account type, so trial mode sent nothing: {}",
                    e
                ))
            })?),
        };
        if full {
            let error = full_account_error();
            tracing::error!(error = %error, "Twilio trial mode on a full account; SMS not sent");
            return Err(error);
        }
        Ok(())
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

/// How long a fetched account type is trusted before the send-time guard
/// fetches it again. An account upgraded to Full while the flag is still set
/// is caught within this window.
const ACCOUNT_TYPE_TTL: Duration = Duration::from_secs(3600);

/// Whether each configured Twilio account is a full one, as last fetched by
/// the trial-mode send guard or the credential check, with when it was
/// fetched. Keyed by [`TwilioProvider::cache_key`], so a configuration change
/// (another account, token or endpoint) misses and fetches afresh.
static ACCOUNT_TYPES: OnceLock<Mutex<HashMap<u64, (Instant, bool)>>> = OnceLock::new();

fn account_types() -> &'static Mutex<HashMap<u64, (Instant, bool)>> {
    ACCOUNT_TYPES.get_or_init(|| Mutex::new(HashMap::new()))
}

/// A cached "is this a full account" answer still within its TTL.
fn cached_full_account(
    cache: &HashMap<u64, (Instant, bool)>,
    key: u64,
    now: Instant,
) -> Option<bool> {
    cache
        .get(&key)
        .filter(|(fetched_at, _)| now.saturating_duration_since(*fetched_at) < ACCOUNT_TYPE_TTL)
        .map(|(_, full)| *full)
}

/// Remember an account type, dropping entries past their TTL so the map
/// stays as small as the number of configurations seen within the hour.
fn remember_account_type(
    cache: &mut HashMap<u64, (Instant, bool)>,
    key: u64,
    full: bool,
    now: Instant,
) {
    cache
        .retain(|_, (fetched_at, _)| now.saturating_duration_since(*fetched_at) < ACCOUNT_TYPE_TTL);
    cache.insert(key, (now, full));
}

/// The error for trial mode on a full account, shared by the credential
/// check and the send-time guard.
fn full_account_error() -> SmsError {
    SmsError::Other(format!(
        "{} is set but this is a full Twilio account, not a trial one. \
         Guests would receive the literal text \"{}\" instead of their booking \
         details, and you would be billed for it. Unset the variable.",
        TRIAL_ENV_VAR, TRIAL_TEMPLATE
    ))
}

/// Guard the one way trial mode can cost money instead of saving it.
///
/// On a trial account `Body` must name a template. On a full account that same
/// value is just text, so a flag left set after the account is upgraded texts
/// every guest the literal string `sms_appointment_reminders`, at full price,
/// instead of their booking details. The credential check already fetches the
/// account, so it can say so without spending anything, and every trial-mode
/// send checks it too (cached for an hour, see [`TwilioProvider::refuse_full_account`]).
///
/// The opposite mistake (a trial account with the flag off) needs no guard
/// here: it fails closed at send time with Twilio's own refusal, which
/// [`parse_error`] already surfaces.
fn trial_mismatch(trial: bool, account_json: &str) -> Option<SmsError> {
    if !trial {
        return None;
    }
    is_full_account(account_json).then(full_account_error)
}

/// Whether an account resource says `"type": "Full"`. An unparseable or
/// unexpected payload is not proof of a full account.
fn is_full_account(account_json: &str) -> bool {
    serde_json::from_str::<AccountResponse>(account_json)
        .ok()
        .and_then(|parsed| parsed.account_type)
        .as_deref()
        == Some("Full")
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
            self.refuse_full_account().await?;
        }

        let params = [
            ("To", to),
            ("From", self.from.as_str()),
            ("Body", request_body(self.trial, body)),
        ];

        let response = super::http_client()
            .post(&url)
            .basic_auth(&self.account_sid, Some(&self.auth_token))
            .form(&params)
            .send()
            .await
            .map_err(super::request_error)?;

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
        let text = self.fetch_account().await?;
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
    fn account_type_cache_expires_after_an_hour_and_per_configuration() {
        let mut cache = HashMap::new();
        let start = Instant::now();
        assert_eq!(cached_full_account(&cache, 1, start), None);

        remember_account_type(&mut cache, 1, true, start);
        assert_eq!(cached_full_account(&cache, 1, start), Some(true));
        let later = start + ACCOUNT_TYPE_TTL - Duration::from_secs(1);
        assert_eq!(cached_full_account(&cache, 1, later), Some(true));
        // Another configuration (account, token or endpoint) is a miss.
        assert_eq!(cached_full_account(&cache, 2, start), None);
        // Past the hour the account is fetched again.
        assert_eq!(
            cached_full_account(&cache, 1, start + ACCOUNT_TYPE_TTL),
            None
        );

        // Expired entries are dropped when another one is stored.
        remember_account_type(&mut cache, 2, false, start + ACCOUNT_TYPE_TTL);
        assert!(!cache.contains_key(&1));
        assert_eq!(cache.len(), 1);
    }

    #[test]
    fn the_cache_key_follows_the_configuration() {
        let provider = |sid: &str, token: &str| TwilioProvider {
            account_sid: sid.to_string(),
            auth_token: token.to_string(),
            from: "+15551234567".to_string(),
            base_url: "https://api.twilio.com".to_string(),
            trial: true,
        };
        assert_eq!(
            provider("AC1", "t").cache_key(),
            provider("AC1", "t").cache_key()
        );
        assert_ne!(
            provider("AC1", "t").cache_key(),
            provider("AC2", "t").cache_key()
        );
        assert_ne!(
            provider("AC1", "t").cache_key(),
            provider("AC1", "u").cache_key()
        );
    }

    /// A Twilio stand-in on a local port: answers the account resource with
    /// `account_json` and accepts every message, counting both.
    async fn spawn_twilio(
        account_json: &'static str,
    ) -> (String, std::sync::Arc<std::sync::Mutex<(usize, usize)>>) {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let counts = std::sync::Arc::new(std::sync::Mutex::new((0usize, 0usize)));
        let seen = counts.clone();
        tokio::spawn(async move {
            while let Ok((mut sock, _)) = listener.accept().await {
                // Read the whole request (headers, then Content-Length bytes)
                // before answering, so closing never resets the client.
                let mut buf = Vec::new();
                let mut chunk = [0u8; 4096];
                loop {
                    let n = sock.read(&mut chunk).await.unwrap_or(0);
                    if n == 0 {
                        break;
                    }
                    buf.extend_from_slice(&chunk[..n]);
                    let text = String::from_utf8_lossy(&buf);
                    if let Some(end) = text.find("\r\n\r\n") {
                        let len = text[..end]
                            .lines()
                            .find_map(|l| {
                                l.to_ascii_lowercase()
                                    .strip_prefix("content-length:")
                                    .map(|v| v.trim().parse::<usize>().unwrap_or(0))
                            })
                            .unwrap_or(0);
                        if buf.len() >= end + 4 + len {
                            break;
                        }
                    }
                }
                let request = String::from_utf8_lossy(&buf).into_owned();
                let body = if request.starts_with("GET ") {
                    seen.lock().unwrap().0 += 1;
                    account_json
                } else {
                    seen.lock().unwrap().1 += 1;
                    r#"{"sid": "SM1", "num_segments": "1"}"#
                };
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                let _ = sock.write_all(response.as_bytes()).await;
            }
        });
        (format!("http://127.0.0.1:{port}"), counts)
    }

    fn trial_provider(base_url: String) -> TwilioProvider {
        TwilioProvider {
            account_sid: "AC123".to_string(),
            auth_token: format!("token-{base_url}"),
            from: "+15551234567".to_string(),
            base_url,
            trial: true,
        }
    }

    #[tokio::test]
    async fn trial_mode_refuses_to_send_to_a_full_account() {
        let (base_url, counts) =
            spawn_twilio(r#"{"sid": "AC123", "status": "active", "type": "Full"}"#).await;
        let provider = trial_provider(base_url);

        for _ in 0..3 {
            let result = provider.send("+33612345678", "Booking confirmed").await;
            assert!(matches!(result, Err(SmsError::Other(_))), "{:?}", result);
        }
        // The account is fetched once, then cached; no message ever left.
        assert_eq!(*counts.lock().unwrap(), (1, 0));
    }

    #[tokio::test]
    async fn trial_mode_sends_to_a_trial_account() {
        let (base_url, counts) =
            spawn_twilio(r#"{"sid": "AC123", "status": "active", "type": "Trial"}"#).await;
        let provider = trial_provider(base_url);

        for _ in 0..2 {
            let receipt = provider
                .send("+33612345678", "Booking confirmed")
                .await
                .expect("a trial account takes the template");
            assert_eq!(receipt.segments, Some(1));
        }
        assert_eq!(*counts.lock().unwrap(), (1, 2));
    }

    /// Nothing was sent when the account lookup fails, so the guard reports
    /// a definite refusal (`Other`), which releases the cap's reservation,
    /// never a transport failure, which would keep it.
    #[tokio::test]
    async fn a_failed_account_lookup_refuses_without_sending() {
        let dead = {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            listener.local_addr().unwrap().port()
        };
        let provider = trial_provider(format!("http://127.0.0.1:{dead}"));
        let result = provider.send("+33612345678", "Booking confirmed").await;
        assert!(matches!(result, Err(SmsError::Other(_))), "{:?}", result);
    }

    #[tokio::test]
    async fn without_trial_mode_the_account_is_not_fetched() {
        let (base_url, counts) =
            spawn_twilio(r#"{"sid": "AC123", "status": "active", "type": "Full"}"#).await;
        let provider = TwilioProvider {
            trial: false,
            ..trial_provider(base_url)
        };
        provider
            .send("+33612345678", "Booking confirmed")
            .await
            .expect("a full account takes a real body");
        assert_eq!(*counts.lock().unwrap(), (0, 1));
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
