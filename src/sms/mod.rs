//! SMS notifications, provider-agnostic.
//!
//! Mirrors `providers/` (the calendar back-end abstraction): a thin
//! [`SmsProvider`] trait hides each gateway's protocol, `factory.rs` dispatches
//! on the configured kind, and everything above the trait (opt-in checks, phone
//! normalisation, localised message bodies) is written once and never changes
//! when a gateway is added.
//!
//! Configuration follows the SMTP pattern: a system-wide singleton row in
//! `sms_config`, editable from the admin panel, with the secret encrypted at
//! rest (`crate::crypto`) and a `CALRS_SMS_*` environment block that overrides
//! the database wholesale.
//!
//! The whole feature is opt-in twice over: with no `sms_config` row *and* every
//! event type leaving `sms_phone_mode` at `off`, nothing here ever runs.

use anyhow::Result;
use async_trait::async_trait;
use sqlx::SqlitePool;
use std::sync::OnceLock;
use std::time::Duration;

pub mod factory;
pub mod gatewayapi;
pub mod message;
pub mod phone;
pub mod sevenio;
pub mod twilio;
pub mod webhook;

pub use factory::{kinds, PROVIDER_SPECS};

/// Per-event-type phone policy, stored in `event_types.sms_phone_mode`.
///
/// Three states rather than a boolean, because "the guest may leave it empty"
/// and "the message is the whole point of this event type" are genuinely
/// different products. Cal.com converged on the same shape: the phone field is
/// optional in general and becomes required once a workflow texts the attendee.
pub mod phone_mode {
    /// No field, no SMS. The default.
    pub const OFF: &str = "off";
    /// Field shown; an empty answer simply means no SMS for that booking.
    pub const OPTIONAL: &str = "optional";
    /// Field shown and enforced.
    pub const REQUIRED: &str = "required";

    pub const ALL: &[&str] = &[OFF, OPTIONAL, REQUIRED];

    /// Normalise anything stored or posted into a known mode, defaulting to
    /// the safe one.
    pub fn parse(value: &str) -> &'static str {
        match value.trim() {
            OPTIONAL => OPTIONAL,
            REQUIRED => REQUIRED,
            _ => OFF,
        }
    }
}

/// Whether an event type asks the guest for a phone number at all.
pub fn collects_phone(mode: &str) -> bool {
    phone_mode::parse(mode) != phone_mode::OFF
}

/// Whether the guest must supply one to book.
pub fn requires_phone(mode: &str) -> bool {
    phone_mode::parse(mode) == phone_mode::REQUIRED
}

/// Instance-wide SMS gateway configuration.
///
/// The field set is deliberately the union of what the supported gateways
/// need, kept small enough that a new provider is a file plus a registry entry:
///
/// * `api_key` is a non-secret account identifier (Twilio's Account SID).
///   Gateways that authenticate with a bare token leave it empty.
/// * `api_secret` is the actual credential, decrypted here.
/// * `sender` is a from-number or an alphanumeric sender ID.
/// * `base_url` picks a region (`gatewayapi.eu`) or a self-hosted endpoint,
///   and is the target URL for the `webhook` provider.
#[derive(Clone, Default)]
pub struct SmsConfig {
    pub provider: String,
    pub api_key: String,
    pub api_secret: String,
    pub sender: String,
    pub base_url: String,
    pub default_country_code: String,
    /// Messages per day across the whole instance, 0 meaning no limit.
    pub daily_cap: i64,
}

impl std::fmt::Debug for SmsConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SmsConfig")
            .field("provider", &self.provider)
            .field("api_key", &self.api_key)
            .field("api_secret", &"<redacted>")
            .field("sender", &self.sender)
            .field("base_url", &self.base_url)
            .field("default_country_code", &self.default_country_code)
            .field("daily_cap", &self.daily_cap)
            .finish()
    }
}

/// Non-secret view of the configuration for the admin panel.
pub struct SmsStatus {
    pub provider: String,
    pub provider_label: &'static str,
    pub api_key: String,
    pub sender: String,
    pub base_url: String,
    pub default_country_code: String,
    pub daily_cap: i64,
    pub enabled: bool,
    pub from_env: bool,
}

/// What a gateway reports back about an accepted message. Every field is
/// optional because coverage varies: Twilio returns a SID and a price, seven.io
/// returns an id and a price, GatewayAPI returns an id and a total cost, a
/// webhook receiver may return nothing at all.
#[derive(Debug, Default)]
pub struct SendReceipt {
    pub message_id: Option<String>,
    pub segments: Option<u32>,
    pub cost: Option<f64>,
    pub currency: Option<String>,
}

/// Gateway failures, normalised across providers so the admin panel and the
/// logs say the same thing whichever gateway is configured.
///
/// This normalisation is the main reason response parsing belongs to the
/// provider: the same failure is an HTTP 401 on Twilio, an HTTP 401 with a
/// `{"code": "0x..."}` body on GatewayAPI, and an HTTP **200** carrying
/// `{"success": "900"}` on seven.io.
#[derive(Debug)]
pub enum SmsError {
    /// Credentials rejected.
    Auth(String),
    /// The recipient number was refused by the gateway.
    InvalidRecipient(String),
    /// The sender number or alphanumeric sender ID was refused.
    InvalidSender(String),
    /// Out of credit / insufficient funds.
    InsufficientCredit(String),
    /// Throttled; retrying later may work.
    RateLimited(String),
    /// Network-level failure (DNS, TLS, timeout).
    Transport(String),
    /// Anything else, carrying the gateway's own message.
    Other(String),
}

impl std::fmt::Display for SmsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SmsError::Auth(m) => write!(f, "authentication rejected by the SMS gateway: {}", m),
            SmsError::InvalidRecipient(m) => write!(f, "recipient number refused: {}", m),
            SmsError::InvalidSender(m) => write!(f, "sender refused: {}", m),
            SmsError::InsufficientCredit(m) => write!(f, "insufficient credit: {}", m),
            SmsError::RateLimited(m) => write!(f, "rate limited by the SMS gateway: {}", m),
            SmsError::Transport(m) => write!(f, "could not reach the SMS gateway: {}", m),
            SmsError::Other(m) => write!(f, "SMS gateway error: {}", m),
        }
    }
}

impl std::error::Error for SmsError {}

/// Short machine-readable label used in structured logs.
impl SmsError {
    pub fn kind(&self) -> &'static str {
        match self {
            SmsError::Auth(_) => "auth",
            SmsError::InvalidRecipient(_) => "invalid_recipient",
            SmsError::InvalidSender(_) => "invalid_sender",
            SmsError::InsufficientCredit(_) => "insufficient_credit",
            SmsError::RateLimited(_) => "rate_limited",
            SmsError::Transport(_) => "transport",
            SmsError::Other(_) => "other",
        }
    }
}

/// Common operations every SMS gateway must support.
#[async_trait]
pub trait SmsProvider: Send + Sync {
    /// The `sms_config.provider` value this adapter serves.
    fn kind(&self) -> &'static str;

    /// Send one message. `to` is always E.164 (`+33612345678`); adapters that
    /// need another wire format convert internally. `body` is already
    /// localised and length-checked.
    async fn send(&self, to: &str, body: &str) -> Result<SendReceipt, SmsError>;

    /// Verify credentials without sending (and without spending money).
    /// Gateways with no such endpoint keep the default: report unsupported so
    /// the admin panel can tell the operator to use the test message instead.
    async fn check(&self) -> Result<(), SmsError> {
        Err(SmsError::Other(
            "this gateway has no credential check endpoint; send a test message instead"
                .to_string(),
        ))
    }
}

/// Shared HTTP client. Built once so connections are pooled, and always with a
/// timeout: these calls happen inline in the booking request path, where a
/// hanging gateway would otherwise hang the guest's booking.
///
/// Redirects are not followed, matching the resource-feed client. `reqwest`
/// strips `Authorization` on a cross-host redirect but leaves custom headers
/// alone, so following one would hand seven.io's `X-Api-Key` to whatever host
/// the redirect names.
pub(crate) fn http_client() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .timeout(Duration::from_secs(15))
            .redirect(reqwest::redirect::Policy::none())
            .user_agent("calrs-sms/1")
            .build()
            .unwrap_or_default()
    })
}

const SMS_ENV_VARS: &[&str] = &[
    "CALRS_SMS_PROVIDER",
    "CALRS_SMS_API_KEY",
    "CALRS_SMS_API_SECRET",
    "CALRS_SMS_SENDER",
    "CALRS_SMS_BASE_URL",
    "CALRS_SMS_DEFAULT_COUNTRY_CODE",
    "CALRS_SMS_DAILY_CAP",
];

/// Read an env var, returning `Some` only when set and non-empty.
fn optional_env(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

/// Load the config from the `CALRS_SMS_*` block. Same "full block wins"
/// semantics as `email::load_smtp_config_from_env`: a block that validates
/// overrides the database, an incomplete one is ignored with a warning so a
/// typo in a deployment unit does not silently disable SMS.
fn load_config_from_env() -> Option<SmsConfig> {
    if !SMS_ENV_VARS
        .iter()
        .any(|name| std::env::var_os(name).is_some())
    {
        return None;
    }

    let config = SmsConfig {
        provider: optional_env("CALRS_SMS_PROVIDER").unwrap_or_default(),
        api_key: optional_env("CALRS_SMS_API_KEY").unwrap_or_default(),
        api_secret: optional_env("CALRS_SMS_API_SECRET").unwrap_or_default(),
        sender: optional_env("CALRS_SMS_SENDER").unwrap_or_default(),
        base_url: optional_env("CALRS_SMS_BASE_URL").unwrap_or_default(),
        default_country_code: optional_env("CALRS_SMS_DEFAULT_COUNTRY_CODE")
            .unwrap_or_else(|| phone::DEFAULT_COUNTRY_CODE.to_string()),
        daily_cap: optional_env("CALRS_SMS_DAILY_CAP")
            .and_then(|v| v.parse().ok())
            .unwrap_or(0),
    };

    match factory::validate_config(&config) {
        Ok(()) => Some(config),
        Err(e) => {
            tracing::warn!(
                error = %e,
                "incomplete CALRS_SMS_* environment block; falling back to the database SMS config"
            );
            None
        }
    }
}

/// Whether the `CALRS_SMS_*` block governs the config (locks the admin form,
/// same as `email::smtp_env_active`).
pub fn sms_env_active() -> bool {
    load_config_from_env().is_some()
}

/// Load the SMS config from the environment or the database. `Ok(None)` means
/// SMS is simply not configured, which is the default state.
pub async fn load_config(pool: &SqlitePool, key: &[u8; 32]) -> Result<Option<SmsConfig>> {
    if let Some(config) = load_config_from_env() {
        return Ok(Some(config));
    }

    let row: Option<(
        String,
        Option<String>,
        Option<String>,
        String,
        Option<String>,
        String,
        i64,
    )> = sqlx::query_as(
        "SELECT provider, api_key, api_secret_enc, sender, base_url, default_country_code, daily_cap \
         FROM sms_config WHERE enabled = 1 LIMIT 1",
    )
    .fetch_optional(pool)
    .await?;

    let Some((
        provider,
        api_key,
        api_secret_enc,
        sender,
        base_url,
        default_country_code,
        daily_cap,
    )) = row
    else {
        return Ok(None);
    };

    let api_secret = match api_secret_enc.as_deref().filter(|s| !s.is_empty()) {
        Some(enc) => crate::crypto::decrypt_password(key, enc)?,
        None => String::new(),
    };

    let config = SmsConfig {
        provider,
        api_key: api_key.unwrap_or_default(),
        api_secret,
        sender,
        base_url: base_url.unwrap_or_default(),
        default_country_code,
        daily_cap,
    };

    // A row that no longer validates (e.g. saved before a provider gained a
    // required field) is treated as "not configured" rather than failing at
    // send time inside a booking request.
    if let Err(e) = factory::validate_config(&config) {
        tracing::warn!(error = %e, provider = %config.provider, "stored SMS config is incomplete; SMS disabled");
        return Ok(None);
    }

    Ok(Some(config))
}

/// Load the non-secret config for admin display. Unlike [`load_config`] this
/// also returns disabled rows, so the operator can see what is stored.
pub async fn load_status(pool: &SqlitePool) -> Result<Option<SmsStatus>> {
    if let Some(config) = load_config_from_env() {
        return Ok(Some(SmsStatus {
            provider_label: factory::label(&config.provider),
            provider: config.provider,
            api_key: config.api_key,
            sender: config.sender,
            base_url: config.base_url,
            default_country_code: config.default_country_code,
            daily_cap: config.daily_cap,
            enabled: true,
            from_env: true,
        }));
    }

    let row: Option<(
        String,
        Option<String>,
        String,
        Option<String>,
        String,
        i64,
        bool,
    )> = sqlx::query_as(
        "SELECT provider, api_key, sender, base_url, default_country_code, daily_cap, enabled \
         FROM sms_config ORDER BY enabled DESC LIMIT 1",
    )
    .fetch_optional(pool)
    .await?;

    Ok(row.map(
        |(provider, api_key, sender, base_url, default_country_code, daily_cap, enabled)| {
            SmsStatus {
                provider_label: factory::label(&provider),
                provider,
                api_key: api_key.unwrap_or_default(),
                sender,
                base_url: base_url.unwrap_or_default(),
                default_country_code,
                daily_cap,
                enabled,
                from_env: false,
            }
        },
    ))
}

/// The default country code to use when normalising a guest's local number.
/// Falls back to [`phone::DEFAULT_COUNTRY_CODE`] when SMS is unconfigured, so
/// the booking form can render before an admin has been through the panel.
pub async fn default_country_code(pool: &SqlitePool) -> String {
    sqlx::query_scalar::<_, String>("SELECT default_country_code FROM sms_config LIMIT 1")
        .fetch_optional(pool)
        .await
        .ok()
        .flatten()
        .filter(|c| phone::is_valid_country_code(c))
        .unwrap_or_else(|| phone::DEFAULT_COUNTRY_CODE.to_string())
}

/// Booking lifecycle events that produce a guest SMS.
///
/// Deliberately not the full email matrix: SMS costs money per message and
/// interrupts people, so it carries only what a guest needs on their phone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SmsEvent {
    Confirmed,
    Cancelled,
    Rescheduled,
    Reminder,
    /// A pending request the host did not answer before its deadline (#234).
    Expired,
}

impl SmsEvent {
    pub fn as_str(&self) -> &'static str {
        match self {
            SmsEvent::Confirmed => "confirmed",
            SmsEvent::Cancelled => "cancelled",
            SmsEvent::Rescheduled => "rescheduled",
            SmsEvent::Reminder => "reminder",
            SmsEvent::Expired => "expired",
        }
    }
}

/// Everything a message body needs, borrowed from whatever the caller already
/// built for the matching email. Times are pre-formatted in the guest's
/// timezone by the email layer, so an SMS names the same day and time as its
/// email. The date stays `YYYY-MM-DD` here, where the email writes it out:
/// a long date costs characters billed per segment.
pub struct SmsContext<'a> {
    pub phone: &'a str,
    pub event_title: &'a str,
    pub date: &'a str,
    pub start_time: &'a str,
    pub timezone: &'a str,
    pub lang: Option<&'a str>,
}

impl<'a> SmsContext<'a> {
    /// Build from the booking details already assembled for the guest email.
    /// Returns `None` when the event type has SMS off or the guest left no
    /// number, which keeps the guard at every call site to one `if let`.
    pub fn for_booking(
        details: &'a crate::email::BookingDetails,
        phone: Option<&'a str>,
        sms_enabled: bool,
    ) -> Option<Self> {
        let phone = Self::usable_phone(phone, sms_enabled)?;
        Some(SmsContext {
            phone,
            event_title: &details.event_title,
            date: &details.date,
            start_time: &details.start_time,
            timezone: &details.guest_timezone,
            lang: details.guest_language.as_deref(),
        })
    }

    /// Same, for the cancellation path.
    pub fn for_cancellation(
        details: &'a crate::email::CancellationDetails,
        phone: Option<&'a str>,
        sms_enabled: bool,
    ) -> Option<Self> {
        let phone = Self::usable_phone(phone, sms_enabled)?;
        Some(SmsContext {
            phone,
            event_title: &details.event_title,
            date: &details.date,
            start_time: &details.start_time,
            timezone: &details.guest_timezone,
            lang: details.guest_language.as_deref(),
        })
    }

    fn usable_phone(phone: Option<&'a str>, sms_enabled: bool) -> Option<&'a str> {
        if !sms_enabled {
            return None;
        }
        phone.map(str::trim).filter(|p| !p.is_empty())
    }
}

/// Send a guest SMS, best-effort.
///
/// Never returns an error and never blocks a booking: an unconfigured gateway,
/// a number the gateway rejects, or an outage are all logged and swallowed,
/// exactly like the email sends around it (`let _ = send_...`).
pub async fn notify_guest(pool: &SqlitePool, key: &[u8; 32], event: SmsEvent, ctx: SmsContext<'_>) {
    let config = match load_config(pool, key).await {
        Ok(Some(c)) => c,
        Ok(None) => return,
        Err(e) => {
            tracing::warn!(error = %e, "could not load SMS config; skipping SMS");
            return;
        }
    };

    if !phone::is_e164(ctx.phone) {
        tracing::warn!(
            event = event.as_str(),
            "stored guest phone is not E.164; skipping SMS"
        );
        return;
    }

    // The booking form is public and the recipient is guest-controlled, so a
    // runaway event type or an SMS pumping attempt would otherwise bill the
    // operator without bound. Over the cap, email carries on alone: a booking
    // must never fail because the SMS budget ran out.
    if over_daily_cap(pool, &config).await {
        tracing::warn!(
            event = event.as_str(),
            cap = config.daily_cap,
            "daily SMS cap reached; skipping SMS (email is unaffected)"
        );
        return;
    }

    let body = message::compose(event, &ctx);
    match send(&config, ctx.phone, &body).await {
        Ok(receipt) => {
            tracing::info!(
                event = event.as_str(),
                provider = %config.provider,
                message_id = receipt.message_id.as_deref().unwrap_or(""),
                segments = receipt.segments.unwrap_or(0),
                "SMS sent"
            );
            record_usage(pool, &config.provider, event.as_str(), &receipt).await;
        }
        Err(e) => tracing::warn!(
            event = event.as_str(),
            provider = %config.provider,
            kind = e.kind(),
            error = %e,
            "SMS send failed"
        ),
    }
}

/// Messages accepted by the gateway since midnight (UTC, matching how the
/// rest of the schema stores timestamps).
pub async fn sent_today(pool: &SqlitePool) -> i64 {
    sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM sms_usage WHERE sent_at >= date('now')")
        .fetch_one(pool)
        .await
        .unwrap_or(0)
}

/// Total cost reported by the gateway since midnight, and its currency.
/// Gateways that do not price a message leave it out of the sum.
pub async fn cost_today(pool: &SqlitePool) -> (f64, String) {
    let row: Option<(Option<f64>, Option<String>)> = sqlx::query_as(
        "SELECT SUM(cost), MAX(currency) FROM sms_usage WHERE sent_at >= date('now')",
    )
    .fetch_optional(pool)
    .await
    .unwrap_or(None);

    match row {
        Some((cost, currency)) => (cost.unwrap_or(0.0), currency.unwrap_or_default()),
        None => (0.0, String::new()),
    }
}

/// Whether the instance has already sent its allowance for today. A cap of 0
/// means no limit, which is the default.
async fn over_daily_cap(pool: &SqlitePool, config: &SmsConfig) -> bool {
    if config.daily_cap <= 0 {
        return false;
    }
    sent_today(pool).await >= config.daily_cap
}

/// Append to the usage ledger the cap counts and the admin panel reports from.
/// Deliberately records no recipient: this is metering, not a message log.
async fn record_usage(pool: &SqlitePool, provider: &str, event: &str, receipt: &SendReceipt) {
    let result = sqlx::query(
        "INSERT INTO sms_usage (id, event, provider, segments, cost, currency) \
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(event)
    .bind(provider)
    .bind(receipt.segments.map(|s| s as i64))
    .bind(receipt.cost)
    .bind(receipt.currency.as_deref())
    .execute(pool)
    .await;

    if let Err(e) = result {
        tracing::warn!(error = %e, "could not record SMS usage");
    }
}

/// Send one message through the configured gateway. Used by [`notify_guest`]
/// and by the admin panel's test button.
pub async fn send(config: &SmsConfig, to: &str, body: &str) -> Result<SendReceipt, SmsError> {
    let provider = factory::build_provider(config)?;
    provider.send(to, body).await
}

/// Verify the stored credentials without sending a message.
pub async fn check(config: &SmsConfig) -> Result<(), SmsError> {
    let provider = factory::build_provider(config)?;
    provider.check().await
}
