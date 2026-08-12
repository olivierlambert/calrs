//! SMS notifications via the Twilio REST API.
//!
//! Mirrors `email.rs`'s SMTP pattern on purpose: a system-wide singleton
//! config, `CALRS_TWILIO_*` environment variables taking precedence over a
//! DB-stored, AES-256-GCM-encrypted (`crate::crypto`) row editable from the
//! admin panel. SMS is entirely opt-in — with no Twilio config *and* no
//! event type enabling it, nothing changes for existing installs.
//!
//! Unlike SMTP, sending happens over plain HTTPS (Twilio's REST API), so we
//! reuse the `reqwest` client already used for CalDAV instead of pulling in a
//! Twilio SDK.

use anyhow::{bail, Context, Result};
use sqlx::SqlitePool;

pub struct TwilioConfig {
    pub account_sid: String,
    pub auth_token: String,
    pub from_number: String,
}

impl std::fmt::Debug for TwilioConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TwilioConfig")
            .field("account_sid", &self.account_sid)
            .field("auth_token", &"<redacted>")
            .field("from_number", &self.from_number)
            .finish()
    }
}

pub struct TwilioStatus {
    pub account_sid: String,
    pub from_number: String,
    pub enabled: bool,
    pub from_env: bool,
}

const TWILIO_ENV_VARS: &[&str] = &[
    "CALRS_TWILIO_ACCOUNT_SID",
    "CALRS_TWILIO_AUTH_TOKEN",
    "CALRS_TWILIO_FROM_NUMBER",
];

/// Read a Twilio env var, returning `Some(value)` only when set and non-empty.
fn optional_twilio_env(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .filter(|value| !value.trim().is_empty())
}

/// Load Twilio config from the `CALRS_TWILIO_*` environment block. Same
/// "full block override" semantics as `email::load_smtp_config_from_env`:
/// a complete block wins over the database; a partial block is ignored and
/// the caller falls back to the DB config.
fn load_twilio_config_from_env() -> Option<TwilioConfig> {
    if !TWILIO_ENV_VARS
        .iter()
        .any(|name| std::env::var_os(name).is_some())
    {
        return None;
    }

    match (
        optional_twilio_env("CALRS_TWILIO_ACCOUNT_SID"),
        optional_twilio_env("CALRS_TWILIO_AUTH_TOKEN"),
        optional_twilio_env("CALRS_TWILIO_FROM_NUMBER"),
    ) {
        (Some(account_sid), Some(auth_token), Some(from_number)) => Some(TwilioConfig {
            account_sid,
            auth_token,
            from_number,
        }),
        _ => {
            tracing::warn!(
                "partial CALRS_TWILIO_* environment block (missing one of ACCOUNT_SID/AUTH_TOKEN/FROM_NUMBER); falling back to database Twilio config"
            );
            None
        }
    }
}

/// Whether the `CALRS_TWILIO_*` environment block governs the config (locks
/// the admin form, same as `email::smtp_env_active`).
pub fn twilio_env_active() -> bool {
    load_twilio_config_from_env().is_some()
}

/// Load Twilio config from environment or database. Returns `Ok(None)` when
/// SMS is simply not configured — this is the normal, default state.
pub async fn load_twilio_config(pool: &SqlitePool, key: &[u8; 32]) -> Result<Option<TwilioConfig>> {
    if let Some(config) = load_twilio_config_from_env() {
        return Ok(Some(config));
    }

    let row: Option<(String, Option<String>, String)> = sqlx::query_as(
        "SELECT account_sid, auth_token_enc, from_number FROM twilio_config WHERE enabled = 1 LIMIT 1",
    )
    .fetch_optional(pool)
    .await?;

    match row {
        Some((account_sid, Some(auth_token_enc), from_number)) => {
            let auth_token = crate::crypto::decrypt_password(key, &auth_token_enc)?;
            Ok(Some(TwilioConfig {
                account_sid,
                auth_token,
                from_number,
            }))
        }
        Some((_, None, _)) | None => Ok(None),
    }
}

/// Load non-secret Twilio status for admin display (same "show disabled rows
/// too" behaviour as `email::load_smtp_status`).
pub async fn load_twilio_status(pool: &SqlitePool) -> Result<Option<TwilioStatus>> {
    if let Some(config) = load_twilio_config_from_env() {
        return Ok(Some(TwilioStatus {
            account_sid: config.account_sid,
            from_number: config.from_number,
            enabled: true,
            from_env: true,
        }));
    }

    let row: Option<(String, String, bool)> = sqlx::query_as(
        "SELECT account_sid, from_number, enabled FROM twilio_config ORDER BY enabled DESC LIMIT 1",
    )
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|(account_sid, from_number, enabled)| TwilioStatus {
        account_sid,
        from_number,
        enabled,
        from_env: false,
    }))
}

/// Supported/default international calling codes for the guest phone prefix selector.
pub const COUNTRY_CODES: &[(&str, &str)] = &[
    ("+1", "United States / Canada (+1)"),
    ("+20", "Egypt (+20)"),
    ("+27", "South Africa (+27)"),
    ("+30", "Greece (+30)"),
    ("+31", "Netherlands (+31)"),
    ("+32", "Belgium (+32)"),
    ("+33", "France (+33)"),
    ("+34", "Spain (+34)"),
    ("+36", "Hungary (+36)"),
    ("+39", "Italy (+39)"),
    ("+40", "Romania (+40)"),
    ("+41", "Switzerland (+41)"),
    ("+43", "Austria (+43)"),
    ("+44", "United Kingdom (+44)"),
    ("+45", "Denmark (+45)"),
    ("+46", "Sweden (+46)"),
    ("+47", "Norway (+47)"),
    ("+48", "Poland (+48)"),
    ("+49", "Germany (+49)"),
    ("+52", "Mexico (+52)"),
    ("+53", "Cuba (+53)"),
    ("+54", "Argentina (+54)"),
    ("+55", "Brazil (+55)"),
    ("+56", "Chile (+56)"),
    ("+57", "Colombia (+57)"),
    ("+58", "Venezuela (+58)"),
    ("+60", "Malaysia (+60)"),
    ("+61", "Australia (+61)"),
    ("+62", "Indonesia (+62)"),
    ("+63", "Philippines (+63)"),
    ("+64", "New Zealand (+64)"),
    ("+65", "Singapore (+65)"),
    ("+66", "Thailand (+66)"),
    ("+7", "Russia / Kazakhstan (+7)"),
    ("+81", "Japan (+81)"),
    ("+82", "South Korea (+82)"),
    ("+84", "Vietnam (+84)"),
    ("+86", "China (+86)"),
    ("+90", "Turkey (+90)"),
    ("+91", "India (+91)"),
    ("+92", "Pakistan (+92)"),
    ("+93", "Afghanistan (+93)"),
    ("+94", "Sri Lanka (+94)"),
    ("+95", "Myanmar (+95)"),
    ("+98", "Iran (+98)"),
    ("+212", "Morocco (+212)"),
    ("+213", "Algeria (+213)"),
    ("+216", "Tunisia (+216)"),
    ("+218", "Libya (+218)"),
    ("+220", "Gambia (+220)"),
    ("+221", "Senegal (+221)"),
    ("+222", "Mauritania (+222)"),
    ("+223", "Mali (+223)"),
    ("+224", "Guinea (+224)"),
    ("+225", "Côte d'Ivoire (+225)"),
    ("+226", "Burkina Faso (+226)"),
    ("+227", "Niger (+227)"),
    ("+228", "Togo (+228)"),
    ("+229", "Benin (+229)"),
    ("+230", "Mauritius (+230)"),
    ("+231", "Liberia (+231)"),
    ("+232", "Sierra Leone (+232)"),
    ("+233", "Ghana (+233)"),
    ("+234", "Nigeria (+234)"),
    ("+235", "Chad (+235)"),
    ("+236", "Central African Republic (+236)"),
    ("+237", "Cameroon (+237)"),
    ("+238", "Cape Verde (+238)"),
    ("+239", "São Tomé and Príncipe (+239)"),
    ("+240", "Equatorial Guinea (+240)"),
    ("+241", "Gabon (+241)"),
    ("+242", "Republic of the Congo (+242)"),
    ("+243", "DR Congo (+243)"),
    ("+244", "Angola (+244)"),
    ("+245", "Guinea-Bissau (+245)"),
    ("+248", "Seychelles (+248)"),
    ("+249", "Sudan (+249)"),
    ("+250", "Rwanda (+250)"),
    ("+251", "Ethiopia (+251)"),
    ("+252", "Somalia (+252)"),
    ("+253", "Djibouti (+253)"),
    ("+254", "Kenya (+254)"),
    ("+255", "Tanzania (+255)"),
    ("+256", "Uganda (+256)"),
    ("+257", "Burundi (+257)"),
    ("+258", "Mozambique (+258)"),
    ("+260", "Zambia (+260)"),
    ("+261", "Madagascar (+261)"),
    ("+262", "Réunion / Mayotte (+262)"),
    ("+263", "Zimbabwe (+263)"),
    ("+264", "Namibia (+264)"),
    ("+265", "Malawi (+265)"),
    ("+266", "Lesotho (+266)"),
    ("+267", "Botswana (+267)"),
    ("+268", "Eswatini (+268)"),
    ("+269", "Comoros (+269)"),
    ("+290", "Saint Helena (+290)"),
    ("+291", "Eritrea (+291)"),
    ("+297", "Aruba (+297)"),
    ("+298", "Faroe Islands (+298)"),
    ("+299", "Greenland (+299)"),
    ("+350", "Gibraltar (+350)"),
    ("+351", "Portugal (+351)"),
    ("+352", "Luxembourg (+352)"),
    ("+353", "Ireland (+353)"),
    ("+354", "Iceland (+354)"),
    ("+355", "Albania (+355)"),
    ("+356", "Malta (+356)"),
    ("+357", "Cyprus (+357)"),
    ("+358", "Finland (+358)"),
    ("+359", "Bulgaria (+359)"),
    ("+370", "Lithuania (+370)"),
    ("+371", "Latvia (+371)"),
    ("+372", "Estonia (+372)"),
    ("+373", "Moldova (+373)"),
    ("+374", "Armenia (+374)"),
    ("+375", "Belarus (+375)"),
    ("+376", "Andorra (+376)"),
    ("+377", "Monaco (+377)"),
    ("+378", "San Marino (+378)"),
    ("+380", "Ukraine (+380)"),
    ("+381", "Serbia (+381)"),
    ("+382", "Montenegro (+382)"),
    ("+383", "Kosovo (+383)"),
    ("+385", "Croatia (+385)"),
    ("+386", "Slovenia (+386)"),
    ("+387", "Bosnia and Herzegovina (+387)"),
    ("+389", "North Macedonia (+389)"),
    ("+420", "Czech Republic (+420)"),
    ("+421", "Slovakia (+421)"),
    ("+423", "Liechtenstein (+423)"),
    ("+500", "Falkland Islands (+500)"),
    ("+501", "Belize (+501)"),
    ("+502", "Guatemala (+502)"),
    ("+503", "El Salvador (+503)"),
    ("+504", "Honduras (+504)"),
    ("+505", "Nicaragua (+505)"),
    ("+506", "Costa Rica (+506)"),
    ("+507", "Panama (+507)"),
    ("+508", "Saint Pierre and Miquelon (+508)"),
    ("+509", "Haiti (+509)"),
    ("+590", "Guadeloupe / Saint Martin (+590)"),
    ("+591", "Bolivia (+591)"),
    ("+592", "Guyana (+592)"),
    ("+593", "Ecuador (+593)"),
    ("+594", "French Guiana (+594)"),
    ("+595", "Paraguay (+595)"),
    ("+596", "Martinique (+596)"),
    ("+597", "Suriname (+597)"),
    ("+598", "Uruguay (+598)"),
    ("+599", "Caribbean Netherlands / Curaçao (+599)"),
    ("+670", "Timor-Leste (+670)"),
    ("+672", "Australian External Territories (+672)"),
    ("+673", "Brunei (+673)"),
    ("+674", "Nauru (+674)"),
    ("+675", "Papua New Guinea (+675)"),
    ("+676", "Tonga (+676)"),
    ("+677", "Solomon Islands (+677)"),
    ("+678", "Vanuatu (+678)"),
    ("+679", "Fiji (+679)"),
    ("+680", "Palau (+680)"),
    ("+681", "Wallis and Futuna (+681)"),
    ("+682", "Cook Islands (+682)"),
    ("+683", "Niue (+683)"),
    ("+685", "Samoa (+685)"),
    ("+686", "Kiribati (+686)"),
    ("+687", "New Caledonia (+687)"),
    ("+688", "Tuvalu (+688)"),
    ("+689", "French Polynesia (+689)"),
    ("+690", "Tokelau (+690)"),
    ("+691", "Micronesia (+691)"),
    ("+692", "Marshall Islands (+692)"),
    ("+850", "North Korea (+850)"),
    ("+852", "Hong Kong (+852)"),
    ("+853", "Macau (+853)"),
    ("+855", "Cambodia (+855)"),
    ("+856", "Laos (+856)"),
    ("+880", "Bangladesh (+880)"),
    ("+886", "Taiwan (+886)"),
    ("+960", "Maldives (+960)"),
    ("+961", "Lebanon (+961)"),
    ("+962", "Jordan (+962)"),
    ("+963", "Syria (+963)"),
    ("+964", "Iraq (+964)"),
    ("+965", "Kuwait (+965)"),
    ("+966", "Saudi Arabia (+966)"),
    ("+967", "Yemen (+967)"),
    ("+968", "Oman (+968)"),
    ("+970", "Palestine (+970)"),
    ("+971", "United Arab Emirates (+971)"),
    ("+972", "Israel (+972)"),
    ("+973", "Bahrain (+973)"),
    ("+974", "Qatar (+974)"),
    ("+975", "Bhutan (+975)"),
    ("+976", "Mongolia (+976)"),
    ("+977", "Nepal (+977)"),
    ("+992", "Tajikistan (+992)"),
    ("+993", "Turkmenistan (+993)"),
    ("+994", "Azerbaijan (+994)"),
    ("+995", "Georgia (+995)"),
    ("+996", "Kyrgyzstan (+996)"),
    ("+998", "Uzbekistan (+998)"),
];

pub fn is_valid_country_code(code: &str) -> bool {
    COUNTRY_CODES.iter().any(|(value, _)| *value == code.trim())
}

/// Normalize a guest phone number using the configured default country code.
/// Local numbers beginning with 0 are converted to +<country><number without
/// the national trunk prefix>.  International + and 00 formats are preserved.
pub fn normalize_guest_phone(raw: &str, default_country_code: &str) -> Option<String> {
    let mut value: String = raw
        .trim()
        .chars()
        .filter(|c| !matches!(c, ' ' | '-' | '(' | ')' | '.'))
        .collect();
    if value.is_empty() || !is_valid_country_code(default_country_code) {
        return None;
    }

    if value.starts_with("00") {
        value.replace_range(..2, "+");
    } else if value.starts_with('+') {
        // already international
    } else {
        let digits = value.trim_start_matches('0');
        if digits.is_empty() {
            return None;
        }
        value = format!("{}{}", default_country_code, digits);
    }

    if is_plausible_e164(&value) {
        Some(value)
    } else {
        None
    }
}

/// Very loose E.164 sanity check for guest-entered phone numbers: leading
/// `+`, then 8-15 digits. Twilio itself is the source of truth on validity;
/// this only stops obviously-wrong input server-side (the HTML field also
/// has a `pattern` attribute as a first line of defence for the guest).
pub fn is_plausible_e164(raw: &str) -> bool {
    let raw = raw.trim();
    let mut chars = raw.chars();
    match chars.next() {
        Some('+') => {}
        _ => return false,
    }
    let digits: String = chars.collect();
    digits.len() >= 8 && digits.len() <= 15 && digits.chars().all(|c| c.is_ascii_digit())
}

/// Send a single SMS via the Twilio REST API
/// (`POST /2010-04-01/Accounts/{Sid}/Messages.json`).
async fn send_sms(config: &TwilioConfig, to: &str, body: &str) -> Result<()> {
    let url = format!(
        "https://api.twilio.com/2010-04-01/Accounts/{}/Messages.json",
        config.account_sid
    );

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .context("Failed to build HTTP client")?;
    let params = [
        ("To", to),
        ("From", config.from_number.as_str()),
        ("Body", body),
    ];

    let response = client
        .post(&url)
        .basic_auth(&config.account_sid, Some(&config.auth_token))
        .form(&params)
        .send()
        .await
        .context("Twilio request failed")?;

    let status = response.status();
    if !status.is_success() {
        let text = response.text().await.unwrap_or_default();
        bail!("Twilio API error ({}): {}", status, text);
    }

    Ok(())
}

/// Send a test SMS from the admin panel.
pub async fn send_test_sms(config: &TwilioConfig, to: &str) -> Result<()> {
    send_sms(config, to, "This is a test SMS from calrs. Twilio is working!").await
}

/// Send a booking confirmation SMS to the guest. Keep it short — SMS is
/// billed per segment (~160 chars for GSM-7).
pub async fn send_booking_confirmation_sms(
    config: &TwilioConfig,
    to: &str,
    event_title: &str,
    date: &str,
    start_time: &str,
) -> Result<()> {
    let body = format!(
        "calrs: your booking \"{}\" is confirmed for {} at {}.",
        event_title, date, start_time
    );
    send_sms(config, to, &body).await
}

/// Send a cancellation SMS to the guest.
pub async fn send_cancellation_sms(
    config: &TwilioConfig,
    to: &str,
    event_title: &str,
    date: &str,
    start_time: &str,
) -> Result<()> {
    let body = format!(
        "calrs: your booking \"{}\" on {} at {} was cancelled.",
        event_title, date, start_time
    );
    send_sms(config, to, &body).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_plausible_e164() {
        assert!(is_plausible_e164("+15551234567"));
        assert!(is_plausible_e164(" +447911123456 "));
    }

    #[test]
    fn rejects_missing_plus_or_bad_length_or_non_digits() {
        assert!(!is_plausible_e164("07911123456"));
        assert!(!is_plausible_e164("+1234"));
        assert!(!is_plausible_e164("+1555abc4567"));
        assert!(!is_plausible_e164(""));
    }

    #[test]
    fn env_config_requires_full_block() {
        // No env vars set at all -> None, falls back to DB.
        for var in TWILIO_ENV_VARS {
            std::env::remove_var(var);
        }
        assert!(load_twilio_config_from_env().is_none());
    }
}
