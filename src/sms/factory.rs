//! Construct an [`SmsProvider`] from an `sms_config` row, and describe each
//! gateway well enough that the admin panel can render its form generically.
//!
//! Adding a gateway is: one adapter file, one [`ProviderSpec`] entry, one arm
//! in [`build_provider`] and [`validate_config`]. No template changes.

use super::{SmsConfig, SmsError, SmsProvider};

/// Values stored in `sms_config.provider`.
pub mod kinds {
    pub const TWILIO: &str = "twilio";
    pub const GATEWAYAPI: &str = "gatewayapi";
    pub const SEVENIO: &str = "sevenio";
    pub const WEBHOOK: &str = "webhook";
}

/// What a gateway calls its fields, and which of them it needs.
///
/// The admin form is rendered from this: the same four inputs are relabelled
/// per provider ("Account SID" or nothing, "Auth token" or "API key",
/// "From number" or "Sender name"), which is why adding a gateway does not
/// touch `admin.html`.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ProviderSpec {
    pub kind: &'static str,
    pub label: &'static str,
    pub docs_url: &'static str,
    /// Label for the non-secret identifier, or `None` when unused.
    pub api_key_label: Option<&'static str>,
    pub api_secret_label: &'static str,
    pub sender_label: &'static str,
    pub sender_hint: &'static str,
    /// Label for the endpoint override, or `None` when the gateway has a
    /// single fixed endpoint.
    pub base_url_label: Option<&'static str>,
    pub base_url_hint: &'static str,
    pub base_url_required: bool,
    pub sender_required: bool,
    /// Whether a credential check is possible without sending a message.
    pub supports_check: bool,
}

pub const PROVIDER_SPECS: &[ProviderSpec] = &[
    ProviderSpec {
        kind: kinds::TWILIO,
        label: "Twilio",
        docs_url: "https://www.twilio.com/docs/sms/api",
        api_key_label: Some("Account SID"),
        api_secret_label: "Auth token",
        sender_label: "Sender",
        sender_hint: "A Twilio number in E.164 format (+15551234567), or an alphanumeric sender ID where the destination country allows one.",
        base_url_label: Some("API base URL"),
        base_url_hint: "Optional. Defaults to https://api.twilio.com. Use https, the credential travels on this connection.",
        base_url_required: false,
        sender_required: true,
        supports_check: true,
    },
    ProviderSpec {
        kind: kinds::GATEWAYAPI,
        label: "GatewayAPI",
        docs_url: "https://gatewayapi.com/docs/apis/rest/",
        api_key_label: None,
        api_secret_label: "API token",
        sender_label: "Sender",
        sender_hint: "Up to 11 alphanumeric characters, or 15 digits.",
        base_url_label: Some("Region base URL"),
        base_url_hint: "Optional. Defaults to https://gatewayapi.com; use https://gatewayapi.eu to keep traffic in the EU.",
        base_url_required: false,
        sender_required: true,
        supports_check: false,
    },
    ProviderSpec {
        kind: kinds::SEVENIO,
        label: "seven.io",
        docs_url: "https://docs.seven.io/en/rest-api/endpoints/sms",
        api_key_label: None,
        api_secret_label: "API key",
        sender_label: "Sender",
        sender_hint: "Up to 11 alphanumeric characters, or 16 digits.",
        base_url_label: Some("API base URL"),
        base_url_hint: "Optional. Defaults to https://gateway.seven.io.",
        base_url_required: false,
        sender_required: true,
        supports_check: true,
    },
    ProviderSpec {
        kind: kinds::WEBHOOK,
        label: "Generic webhook",
        docs_url: "",
        api_key_label: None,
        api_secret_label: "HMAC secret",
        sender_label: "Sender",
        sender_hint: "Optional. Passed through to your endpoint as \"sender\".",
        base_url_label: Some("Webhook URL"),
        base_url_hint: "calrs POSTs {\"to\", \"text\", \"sender\"} here and expects 2xx back.",
        base_url_required: true,
        sender_required: false,
        supports_check: false,
    },
];

pub fn provider_spec(kind: &str) -> Option<&'static ProviderSpec> {
    PROVIDER_SPECS.iter().find(|s| s.kind == kind)
}

/// Human-readable label for UI listings and logs.
pub fn label(kind: &str) -> &'static str {
    provider_spec(kind).map(|s| s.label).unwrap_or("Unknown")
}

/// The endpoint a provider talks to when the operator has not overridden it.
pub fn default_base_url(kind: &str) -> &'static str {
    match kind {
        kinds::TWILIO => "https://api.twilio.com",
        kinds::GATEWAYAPI => "https://gatewayapi.com",
        kinds::SEVENIO => "https://gateway.seven.io",
        _ => "",
    }
}

/// Resolve the effective endpoint for a config, trimming a trailing slash so
/// adapters can concatenate paths without doubling it.
pub fn base_url(config: &SmsConfig) -> String {
    let configured = config.base_url.trim().trim_end_matches('/');
    if configured.is_empty() {
        default_base_url(&config.provider).to_string()
    } else {
        configured.to_string()
    }
}

/// Reject a configuration that cannot possibly send, before it is stored or
/// used. Shared by the admin form, the `CALRS_SMS_*` block, and the read path.
pub fn validate_config(config: &SmsConfig) -> Result<(), String> {
    let Some(spec) = provider_spec(config.provider.trim()) else {
        return Err(format!("unknown SMS provider '{}'", config.provider));
    };

    if spec.api_key_label.is_some() && config.api_key.trim().is_empty() {
        return Err(format!(
            "{} requires {}",
            spec.label,
            spec.api_key_label.unwrap_or("an account identifier")
        ));
    }

    // The webhook receiver may legitimately be unauthenticated (a bridge on
    // localhost); every real gateway needs its credential.
    if config.provider != kinds::WEBHOOK && config.api_secret.trim().is_empty() {
        return Err(format!("{} requires {}", spec.label, spec.api_secret_label));
    }

    if spec.sender_required && config.sender.trim().is_empty() {
        return Err(format!("{} requires {}", spec.label, spec.sender_label));
    }

    if spec.base_url_required && config.base_url.trim().is_empty() {
        return Err(format!(
            "{} requires {}",
            spec.label,
            spec.base_url_label.unwrap_or("a URL")
        ));
    }

    let url = base_url(config);
    if !(url.is_empty() || url.starts_with("http://") || url.starts_with("https://")) {
        return Err("the SMS endpoint must be an http(s) URL".to_string());
    }

    if !super::phone::is_valid_country_code(&config.default_country_code) {
        return Err(format!(
            "'{}' is not a known country calling code",
            config.default_country_code
        ));
    }

    Ok(())
}

/// Build the adapter for a configuration.
pub fn build_provider(config: &SmsConfig) -> Result<Box<dyn SmsProvider>, SmsError> {
    match config.provider.trim() {
        kinds::TWILIO => Ok(Box::new(super::twilio::TwilioProvider::new(config))),
        kinds::GATEWAYAPI => Ok(Box::new(super::gatewayapi::GatewayApiProvider::new(config))),
        kinds::SEVENIO => Ok(Box::new(super::sevenio::SevenIoProvider::new(config))),
        kinds::WEBHOOK => Ok(Box::new(super::webhook::WebhookProvider::new(config))),
        other => Err(SmsError::Other(format!("unknown SMS provider '{}'", other))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(provider: &str) -> SmsConfig {
        SmsConfig {
            provider: provider.to_string(),
            api_key: String::new(),
            api_secret: "secret".to_string().into(),
            sender: "calrs".to_string(),
            base_url: String::new(),
            default_country_code: "+33".to_string(),
            daily_cap: 0,
        }
    }

    #[test]
    fn every_spec_builds_and_labels() {
        for spec in PROVIDER_SPECS {
            assert_eq!(label(spec.kind), spec.label);
            let mut cfg = config(spec.kind);
            cfg.api_key = "AC123".to_string();
            cfg.base_url = "https://example.test".to_string();
            let provider = build_provider(&cfg).expect("adapter should build");
            assert_eq!(provider.kind(), spec.kind);
        }
    }

    #[test]
    fn twilio_needs_its_account_sid() {
        let cfg = config(kinds::TWILIO);
        assert!(validate_config(&cfg).is_err());

        let cfg = SmsConfig {
            api_key: "AC123".to_string(),
            ..config(kinds::TWILIO)
        };
        assert!(validate_config(&cfg).is_ok());
    }

    #[test]
    fn token_only_gateways_need_no_account_id() {
        assert!(validate_config(&config(kinds::GATEWAYAPI)).is_ok());
        assert!(validate_config(&config(kinds::SEVENIO)).is_ok());
    }

    #[test]
    fn a_secret_is_required_except_for_the_webhook() {
        let cfg = SmsConfig {
            api_secret: Default::default(),
            ..config(kinds::SEVENIO)
        };
        assert!(validate_config(&cfg).is_err());

        // An unauthenticated bridge on localhost is a legitimate setup.
        let cfg = SmsConfig {
            api_secret: Default::default(),
            base_url: "http://127.0.0.1:8080/sms".to_string(),
            ..config(kinds::WEBHOOK)
        };
        assert!(validate_config(&cfg).is_ok());
    }

    #[test]
    fn the_webhook_needs_a_url_and_every_endpoint_must_be_http() {
        assert!(validate_config(&config(kinds::WEBHOOK)).is_err());

        let cfg = SmsConfig {
            base_url: "ftp://example.test".to_string(),
            ..config(kinds::WEBHOOK)
        };
        assert!(validate_config(&cfg).is_err());
    }

    #[test]
    fn unknown_providers_and_country_codes_are_rejected() {
        assert!(validate_config(&config("carrier-pigeon")).is_err());
        assert!(build_provider(&config("carrier-pigeon")).is_err());

        let cfg = SmsConfig {
            default_country_code: "33".to_string(),
            ..config(kinds::SEVENIO)
        };
        assert!(validate_config(&cfg).is_err());
    }

    #[test]
    fn base_url_falls_back_to_the_documented_default_and_trims_slashes() {
        assert_eq!(base_url(&config(kinds::TWILIO)), "https://api.twilio.com");
        let cfg = SmsConfig {
            base_url: "https://gatewayapi.eu/".to_string(),
            ..config(kinds::GATEWAYAPI)
        };
        assert_eq!(base_url(&cfg), "https://gatewayapi.eu");
    }
}
