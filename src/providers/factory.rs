//! Construct a [`CalendarProvider`] from a `caldav_sources` row.
//!
//! Centralising the dispatch here keeps the rest of the codebase ignorant of
//! which protocol a source uses. Add a new back-end by extending the match in
//! `build_provider`.

use anyhow::{bail, Result};

use super::CalendarProvider;

/// Provider type stored in `caldav_sources.provider_type`.
pub mod kinds {
    pub const CALDAV: &str = "caldav";
    pub const EWS: &str = "ews";
}

/// Build a provider client for the given source row.
///
/// `provider_type` is the value stored in `caldav_sources.provider_type`. The
/// other parameters are the URL / username / decrypted password — any of them
/// may carry provider-specific meaning (e.g. for EWS the URL is the
/// `Exchange.asmx` endpoint, for CalDAV it is the discovery URL).
pub fn build_provider(
    provider_type: &str,
    url: &str,
    username: &str,
    password: &str,
) -> Result<Box<dyn CalendarProvider>> {
    match provider_type {
        kinds::CALDAV => Ok(Box::new(super::caldav::CaldavProvider::new(
            url, username, password,
        ))),
        kinds::EWS => Ok(Box::new(crate::ews::EwsProvider::new(
            url, username, password,
        ))),
        other => bail!("Unknown calendar provider type: '{}'", other),
    }
}

/// Validate a URL based on the provider type. CalDAV and EWS both reject
/// non-http(s) and SSRF-prone hostnames.
pub fn validate_url(provider_type: &str, url: &str) -> Result<()> {
    match provider_type {
        kinds::CALDAV | kinds::EWS => crate::caldav::validate_caldav_url(url),
        other => bail!("Unknown calendar provider type: '{}'", other),
    }
}

/// Human-readable label for UI listings.
pub fn label(provider_type: &str) -> &'static str {
    match provider_type {
        kinds::CALDAV => "CalDAV",
        kinds::EWS => "Microsoft Exchange (EWS)",
        _ => "Unknown",
    }
}
