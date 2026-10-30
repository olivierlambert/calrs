//! Lead capture, HTTP side.
//!
//! The iClosed-style feature records what a guest types into a public booking
//! form before they submit it, so the host can follow up on abandoned
//! bookings. Storage and gating live in [`crate::leads`]; this module owns
//! everything that touches axum:
//!
//! - [`capture`]: the public `POST /api/lead-capture` endpoint.
//! - [`dashboard`]: the host's `/dashboard/leads` page and its worklist.
//! - [`admin`]: the admin card (global switch, retention, legal-mentions URL).
//! - [`event_type`]: the per-event-type toggle saved with the event type form.
//! - [`page`]: the context the public slot and booking pages render with.
//! - [`notifier`]: the background purge and abandonment-alert loop.

mod admin;
mod capture;
mod dashboard;
mod event_type;
mod notifier;
mod page;

pub(super) use admin::{admin_update_lead_capture, admin_update_legal_mentions};
pub(super) use capture::lead_capture_record;
pub(super) use dashboard::{dashboard_leads, lead_archive, lead_set_contacted};
#[cfg(test)]
pub(super) use event_type::ToggleError;
pub(super) use event_type::{apply_toggle, form_ctx};
#[cfg(test)]
pub(super) use notifier::notify_abandoned;
pub use notifier::run_lead_purge_loop;
pub(super) use page::{admin_ctx, page_ctx, page_off};
