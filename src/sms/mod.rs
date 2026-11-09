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
    /// Wiped from memory when dropped (#85).
    pub api_secret: zeroize::Zeroizing<String>,
    pub sender: String,
    pub base_url: String,
    pub default_country_code: String,
    /// Billed segments per day across the whole instance, 0 meaning no limit.
    /// Segments, not messages, because that is what gateways charge for: one
    /// message can bill two (see `message::estimate_segments`).
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
    /// The request may have reached the gateway, but no usable answer came
    /// back: a timeout once it was sent, a dropped connection, a 5xx, an
    /// unreadable response. The message may have been sent and billed.
    Transport(String),
    /// Nothing left calrs: connection refused, a DNS or TLS failure, or a
    /// request that could not be built. The message was not sent.
    Unreachable(String),
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
            SmsError::Transport(m) => write!(
                f,
                "no usable answer from the SMS gateway (the message may have been sent): {}",
                m
            ),
            SmsError::Unreachable(m) => write!(f, "could not reach the SMS gateway: {}", m),
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
            SmsError::Unreachable(_) => "unreachable",
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

/// Classify a failed HTTP request by whether the message can have left.
///
/// A connect failure (refused, DNS, TLS handshake, a connect timeout) or a
/// request that could not be built sent nothing: [`SmsError::Unreachable`],
/// and the cap's reservation is given back. Anything later (a timeout once
/// the request was written, a connection dropped while waiting for the
/// answer, a body that cannot be read) may have reached the gateway:
/// [`SmsError::Transport`], and the reservation is kept. `is_request()` alone
/// is not proof that nothing left, since reqwest reports a connection closed
/// after the request was written that way too.
pub(crate) fn request_error(e: reqwest::Error) -> SmsError {
    if e.is_connect() || e.is_builder() {
        SmsError::Unreachable(e.to_string())
    } else {
        SmsError::Transport(e.to_string())
    }
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
        api_secret: optional_env("CALRS_SMS_API_SECRET")
            .unwrap_or_default()
            .into(),
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
        None => zeroize::Zeroizing::default(),
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
    let body = message::compose(event, &ctx);
    match send_metered(pool, &config, event.as_str(), ctx.phone, &body).await {
        Ok(receipt) => tracing::info!(
            event = event.as_str(),
            provider = %config.provider,
            message_id = receipt.message_id.as_deref().unwrap_or(""),
            segments = receipt.segments.unwrap_or(0),
            "SMS sent"
        ),
        Err(MeteredSendError::OverCap) => tracing::warn!(
            event = event.as_str(),
            cap = config.daily_cap,
            "daily SMS segment cap reached; skipping SMS (email is unaffected)"
        ),
        Err(MeteredSendError::Ledger(e)) => tracing::warn!(
            event = event.as_str(),
            error = %e,
            "could not reserve SMS usage; skipping SMS (email is unaffected)"
        ),
        Err(MeteredSendError::Gateway(e)) => tracing::warn!(
            event = event.as_str(),
            provider = %config.provider,
            kind = e.kind(),
            error = %e,
            "SMS send failed"
        ),
    }
}

/// Why [`send_metered`] did not deliver a message.
#[derive(Debug)]
pub enum MeteredSendError {
    /// Sending would take today's billed segments past `daily_cap`. Nothing
    /// was sent.
    OverCap,
    /// The usage ledger could not be written, so the cap could not be
    /// enforced. Nothing was sent: an unmetered send is the one thing the cap
    /// exists to prevent.
    Ledger(sqlx::Error),
    /// The gateway refused the message or could not be reached.
    Gateway(SmsError),
}

/// Send one message, metered against the daily cap.
///
/// The cap and the ledger are one thing: before the network call, a row
/// holding the message's estimated segments is inserted only if today's total
/// plus that estimate still fits the cap (one `INSERT ... SELECT`, which SQLite
/// runs under its write lock, so concurrent sends cannot all pass a check none
/// of them has recorded yet). The gateway is then called with no connection
/// held, and the row is settled with what the gateway reported, kept with its
/// estimate as `unknown` when the request failed in transit (the message may
/// have gone out), or deleted when the gateway definitely refused it.
///
/// Used by [`notify_guest`] and by the admin panel's test button (event
/// `test`), so test messages are counted and capped like any other.
pub async fn send_metered(
    pool: &SqlitePool,
    config: &SmsConfig,
    event: &str,
    to: &str,
    body: &str,
) -> Result<SendReceipt, MeteredSendError> {
    let estimate = message::estimate_segments(body);
    let reservation = reserve_usage(pool, config, event, estimate)
        .await
        .map_err(MeteredSendError::Ledger)?
        .ok_or(MeteredSendError::OverCap)?;

    match send(config, to, body).await {
        Ok(receipt) => {
            settle_usage(pool, &reservation, estimate, &receipt).await;
            Ok(receipt)
        }
        Err(e) if outcome_unknown(&e) => {
            mark_usage_unknown(pool, &reservation).await;
            Err(MeteredSendError::Gateway(e))
        }
        Err(e) => {
            release_usage(pool, &reservation).await;
            Err(MeteredSendError::Gateway(e))
        }
    }
}

/// Whether a failed send may still have gone out (and been billed). A
/// transport failure (a timeout once the request was sent, a dropped
/// connection, a 5xx, an unreadable answer) can come after the gateway
/// accepted the message, so its reservation is kept: a spend cap errs toward
/// over-counting. Every other error is a definite refusal or a request that
/// never left (bad credentials, recipient or sender, no credit, throttled, a
/// parsed 4xx, a connect or DNS failure, a check calrs made before sending),
/// and gives its reservation back.
fn outcome_unknown(error: &SmsError) -> bool {
    matches!(error, SmsError::Transport(_))
}

/// Messages metered since midnight (UTC, matching how the rest of the schema
/// stores timestamps): accepted, in flight, or of unknown outcome.
pub async fn sent_today(pool: &SqlitePool) -> i64 {
    sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM sms_usage WHERE sent_at >= date('now')")
        .fetch_one(pool)
        .await
        .unwrap_or(0)
}

/// Messages since midnight (UTC) whose send failed in transit, so the gateway
/// may have sent them. Already included in [`sent_today`] and
/// [`segments_today`].
pub async fn unknown_today(pool: &SqlitePool) -> i64 {
    sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM sms_usage WHERE status = 'unknown' AND sent_at >= date('now')",
    )
    .fetch_one(pool)
    .await
    .unwrap_or(0)
}

/// Billed segments since midnight (UTC): what the daily cap counts. A row
/// with no segment count (written before sends were metered, or by a gateway
/// that reports none) counts as one.
pub async fn segments_today(pool: &SqlitePool) -> i64 {
    sqlx::query_scalar::<_, i64>(
        "SELECT COALESCE(SUM(COALESCE(segments, 1)), 0) FROM sms_usage \
         WHERE sent_at >= date('now')",
    )
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

/// Reserve `segments` of today's allowance in the usage ledger, returning the
/// reservation's id, or `None` when it does not fit. A cap of 0 means no
/// limit, which is the default, and always reserves.
///
/// One statement on purpose: SQLite takes the write lock before an `INSERT`
/// reads anything, so the sum and the insert are atomic across connections,
/// and no connection is held once it returns (never across the network send).
async fn reserve_usage(
    pool: &SqlitePool,
    config: &SmsConfig,
    event: &str,
    segments: u32,
) -> Result<Option<String>, sqlx::Error> {
    let id = uuid::Uuid::new_v4().to_string();
    let segments = i64::from(segments);
    let inserted = sqlx::query(
        "INSERT INTO sms_usage (id, event, provider, segments, status) \
         SELECT ?, ?, ?, ?, 'pending' \
         WHERE ? <= 0 \
            OR (SELECT COALESCE(SUM(COALESCE(segments, 1)), 0) FROM sms_usage \
                WHERE sent_at >= date('now')) + ? <= ?",
    )
    .bind(&id)
    .bind(event)
    .bind(&config.provider)
    .bind(segments)
    .bind(config.daily_cap)
    .bind(segments)
    .bind(config.daily_cap)
    .execute(pool)
    .await?
    .rows_affected();
    Ok((inserted == 1).then_some(id))
}

/// Replace a reservation's estimate with what the gateway reported. Keeps the
/// estimate when the gateway gives no segment count (a webhook receiver), so
/// the cap still counts the message. Deliberately records no recipient: this
/// is metering, not a message log.
async fn settle_usage(pool: &SqlitePool, id: &str, estimate: u32, receipt: &SendReceipt) {
    let result = sqlx::query(
        "UPDATE sms_usage SET segments = ?, cost = ?, currency = ?, status = 'sent' WHERE id = ?",
    )
    .bind(i64::from(receipt.segments.unwrap_or(estimate)))
    .bind(receipt.cost)
    .bind(receipt.currency.as_deref())
    .bind(id)
    .execute(pool)
    .await;

    if let Err(e) = result {
        // The reservation stays, with its estimate: still counted.
        tracing::warn!(error = %e, "could not record SMS usage");
    }
}

/// Keep a reservation whose send failed in transit, with its estimated
/// segments, as `unknown`: still counted against the cap and shown on the
/// admin panel.
async fn mark_usage_unknown(pool: &SqlitePool, id: &str) {
    if let Err(e) = sqlx::query("UPDATE sms_usage SET status = 'unknown' WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await
    {
        // The row stays 'pending', which counts the same.
        tracing::warn!(error = %e, "could not mark an SMS usage reservation as unknown");
    }
}

/// Give a reservation back after the gateway refused the message.
async fn release_usage(pool: &SqlitePool, id: &str) {
    if let Err(e) = sqlx::query("DELETE FROM sms_usage WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await
    {
        // Failing closed: the message stays counted against today's cap.
        tracing::warn!(error = %e, "could not release an SMS usage reservation");
    }
}

/// Send one message through the configured gateway, unmetered. Callers that
/// spend money go through [`send_metered`].
pub async fn send(config: &SmsConfig, to: &str, body: &str) -> Result<SendReceipt, SmsError> {
    let provider = factory::build_provider(config)?;
    provider.send(to, body).await
}

/// At startup, with `CALRS_SMS_TWILIO_TRIAL` set and Twilio configured, fetch
/// the account once, so a flag left on a full account shows in the log right
/// away rather than at the first booking. Sends nothing and only logs: the
/// send-time guard in `twilio.rs` is what actually refuses.
pub async fn check_twilio_trial_at_startup(pool: &SqlitePool, key: &[u8; 32]) {
    if !twilio::trial_mode_enabled() {
        return;
    }
    let Ok(Some(config)) = load_config(pool, key).await else {
        return;
    };
    if config.provider != kinds::TWILIO {
        return;
    }
    if let Err(e) = check(&config).await {
        tracing::error!(error = %e, "Twilio trial mode check failed at startup");
    }
}

/// Verify the stored credentials without sending a message.
pub async fn check(config: &SmsConfig) -> Result<(), SmsError> {
    let provider = factory::build_provider(config)?;
    provider.check().await
}
