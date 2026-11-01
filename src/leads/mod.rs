//! Lead capture (iClosed-style).
//!
//! Records what guests type into a public booking form *before* they hit
//! "submit", so hosts can follow up on abandoned bookings. The feature is
//! gated three ways:
//!
//! 1. Global admin toggle — `auth_config.lead_capture_enabled`.
//! 2. Per-event-type toggle — `event_types.lead_capture` (host opt-in).
//! 3. GDPR: the booking pages tell guests their input is captured, and the
//!    host acknowledges once, per event type, that bookers are informed.
//!
//! Rows are auto-purged after `auth_config.lead_retention_days` days
//! (default 30) by [`purge_expired`], which `calrs serve` runs on a timer.
//!
//! ## Layout
//!
//! - `config`: the gating flags (admin switch and per-event-type toggle).
//! - `db`: persistence (`upsert_partial`, `mark_completed`, `purge_expired`,
//!   the leads list and worklist).
//!
//! The HTTP side lives in [`crate::web`] (`web/leads/`) and the alert email in
//! `crate::email`.

pub mod config;
pub mod db;

pub use config::{is_capture_active, retention_days};
pub use db::{
    alerts_sent_last_hour, archive, delete_lead, delete_leads, due_for_notification, list_leads,
    mark_completed, mark_notified, purge_expired, stats_for_user, toggle_contacted, unarchive,
    upsert_partial, user_can_access, LeadView, PartialBookingInput, Upsert,
};

/// Caps applied at the boundary so a hostile or buggy client can't blow up
/// the table (per-field byte caps + minimum debounce expected client-side).
pub mod limits {
    /// Max length of any captured text field (bytes).
    pub const MAX_FIELD_LEN: usize = 1024;
    /// Max length of a guest name, an email address (RFC 5321) and a phone number.
    pub const MAX_NAME_LEN: usize = 255;
    pub const MAX_EMAIL_LEN: usize = 320;
    pub const MAX_PHONE_LEN: usize = 32;
    /// Max length of the short fields: target date, time and time zone.
    pub const MAX_SHORT_LEN: usize = 64;
    /// Max length of the user agent string we persist.
    pub const MAX_UA_LEN: usize = 256;
    /// Requests one IP may make to the capture endpoint per minute. Above
    /// this it answers 429.
    pub const MAX_REQUESTS_PER_MINUTE: u32 = 60;
    /// New leads one event type accepts per hour. The endpoint is public, so
    /// this bounds what a script rotating IPs and lead ids can write.
    pub const MAX_NEW_LEADS_PER_EVENT_TYPE_PER_HOUR: i64 = 300;
}

/// An address plain enough to put in a link or mail to: one `@`, a dotted
/// domain, and none of the characters that would turn it into more than an
/// address (a `?bcc=` tail, a second recipient, markup).
pub fn is_plain_email(email: &str) -> bool {
    let email = email.trim();
    let Some((local, domain)) = email.split_once('@') else {
        return false;
    };
    email.len() <= limits::MAX_EMAIL_LEN
        && !local.is_empty()
        && domain.contains('.')
        && !domain.starts_with('.')
        && !domain.ends_with('.')
        && email.matches('@').count() == 1
        && !email
            .chars()
            .any(|c| c.is_whitespace() || c.is_control() || "?&,;<>\"'".contains(c))
}

#[cfg(test)]
mod tests {
    use super::is_plain_email;

    #[test]
    fn plain_addresses_only() {
        assert!(is_plain_email("pat@example.com"));
        assert!(is_plain_email("pat.o+tag@sub.example.co.uk"));
        for bad in [
            "pat@example.com?bcc=boss@example.com",
            "pat@example.com&body=x",
            "a@b.com, c@d.com",
            "pat@example.com;c@d.com",
            "pat@exam ple.com",
            "pat@example",
            "pat@",
            "@example.com",
            "a@@b.com",
            "<pat@example.com>",
            "",
        ] {
            assert!(!is_plain_email(bad), "{bad:?}");
        }
    }
}
