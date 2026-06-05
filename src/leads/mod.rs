//! Lead capture (iClosed-style).
//!
//! Records what guests type into a public booking form *before* they hit
//! "submit", so hosts can follow up on abandoned bookings. The feature is
//! gated three ways:
//!
//! 1. Global admin toggle — `auth_config.lead_capture_enabled`.
//! 2. Per-event-type toggle — `event_types.lead_capture` (host opt-in).
//! 3. RGPD: the booking form must show a clear notice telling guests their
//!    input is captured. The notice is rendered when capture is active and
//!    the host's opt-in form requires acknowledging it.
//!
//! Rows are auto-purged after `auth_config.lead_retention_days` days
//! (default 30) by [`purge_expired`], which `calrs serve` runs on a timer.
//!
//! ## Layout
//!
//! - `config` — read the gating flags (admin + per event type).
//! - `db` — pure DB ops (`upsert_partial`, `mark_completed`, `purge_expired`,
//!   `list_for_user`).
//! - The HTTP handler that exposes this to browsers lives in
//!   [`crate::web`] alongside the other booking handlers.

pub mod config;
pub mod db;

pub use config::{is_capture_active, retention_days};
pub use db::{
    archive, due_for_notification, list_recent_for_user, mark_completed, mark_notified,
    purge_expired, set_contacted, stats_for_user, upsert_partial, user_can_access,
    PartialBookingInput,
};

/// Caps applied at the boundary so a hostile or buggy client can't blow up
/// the table (per-field byte caps + minimum debounce expected client-side).
pub mod limits {
    /// Max length of any captured text field (bytes).
    pub const MAX_FIELD_LEN: usize = 1024;
    /// Max length of the user agent string we persist.
    pub const MAX_UA_LEN: usize = 256;
    /// Hard cap on the number of partial rows per IP per minute. Above this
    /// the lead-capture endpoint returns 429.
    pub const MAX_REQUESTS_PER_MINUTE: u32 = 60;
}
