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
    alerts_sent_last_hour, archive, due_for_notification, list_recent_for_user, mark_completed,
    mark_notified, purge_expired, stats_for_user, toggle_contacted, upsert_partial,
    user_can_access, PartialBookingInput, Upsert,
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
