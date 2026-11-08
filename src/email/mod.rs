use anyhow::{bail, Result};
use chrono::NaiveDateTime;
use chrono_tz::Tz;
use fluent_bundle::{FluentArgs, FluentValue};
use lettre::message::header::ContentType;
use lettre::message::MessageBuilder;
use lettre::message::{Attachment, Mailbox, MultiPart, SinglePart};
use lettre::transport::smtp::authentication::Credentials;
use lettre::{AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};
use sqlx::SqlitePool;

mod guest;
mod host;
mod render;
mod transport;

pub use guest::*;
pub use host::*;
pub use render::*;
pub use transport::*;

/// Translate a Fluent message id with no arguments. Local helper so call
/// sites stay readable.
fn t(lang: &str, key: &str) -> String {
    crate::i18n::translate(lang, key, None)
}

/// Translate with named arguments. Each tuple is `(arg_name, value)`.
fn ta<const N: usize>(lang: &str, key: &str, args: [(&str, &str); N]) -> String {
    let mut fa = FluentArgs::new();
    for (k, v) in args.iter() {
        fa.set(*k, FluentValue::from(*v));
    }
    crate::i18n::translate(lang, key, Some(&fa))
}

/// A stored language code if calrs ships that locale, English otherwise.
/// Stored values can be NULL, empty, or a locale since removed.
fn lang_or_en(lang: Option<&str>) -> &str {
    match lang.map(str::trim) {
        Some(code) if crate::i18n::is_supported(code) => code,
        _ => "en",
    }
}

/// Resolve guest language from booking details, falling back to English.
fn guest_lang(details: &BookingDetails) -> &str {
    lang_or_en(details.guest_language.as_deref())
}

/// The host recipient's language (`users.language`), falling back to English.
fn host_lang(details: &BookingDetails) -> &str {
    lang_or_en(details.host_language.as_deref())
}

/// The language an additional attendee reads: their saved one when they are
/// a calrs user listed in `languages` (a claimant), else `fallback`, the
/// booking's language, since the guest invited them.
fn attendee_lang<'a>(languages: &'a [(String, String)], email: &str, fallback: &'a str) -> &'a str {
    languages
        .iter()
        .find(|(address, _)| address.eq_ignore_ascii_case(email))
        .map(|(_, lang)| lang_or_en(Some(lang)))
        .unwrap_or(fallback)
}

/// A stored language value (`users.language`, `bookings.language`) when it
/// names a shipped locale, trimmed; `None` otherwise, which means English.
pub fn supported_language(lang: Option<String>) -> Option<String> {
    lang.map(|l| l.trim().to_string())
        .filter(|l| crate::i18n::is_supported(l))
}

/// A user's saved UI language (`users.language`), when it names a shipped
/// locale. Host, team member and watcher emails are written in it (#209).
///
/// This is the stored half of what the `AuthUser` extractor does on a
/// request: there is no request behind a background email, so there is no
/// `Accept-Language` to fall back on, and `None` means English at send time.
pub async fn user_language(pool: &SqlitePool, user_id: &str) -> Option<String> {
    sqlx::query_scalar::<_, Option<String>>("SELECT language FROM users WHERE id = ?")
        .bind(user_id)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten()
        .and_then(supported_language)
}

/// Cancellation reason recorded when the host deleted the booking's event
/// from their calendar and a sync noticed, in `lang` (English when `None`).
pub fn calendar_deleted_reason(lang: Option<&str>) -> String {
    t(lang_or_en(lang), "email-cancel-reason-calendar-deleted")
}

/// The language the guest booked in, as stored on the booking row
/// (`bookings.language`, captured from the booking page).
///
/// Every guest email sent after the booking request itself (host approval,
/// decline, cancellation by the host or by a calendar sync) is triggered by
/// someone other than the guest, so the language has to come from here
/// rather than from the current request's `Accept-Language` (#209).
pub async fn stored_booking_language(pool: &SqlitePool, booking_id: &str) -> Option<String> {
    sqlx::query_scalar::<_, Option<String>>("SELECT language FROM bookings WHERE id = ?")
        .bind(booking_id)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten()
        .and_then(supported_language)
}

#[derive(Clone, Default)]
pub struct BookingDetails {
    /// Exact UTC endpoints for new bookings; legacy records use wall-clock fields.
    pub utc_times: Option<(String, String)>,
    pub event_title: String,
    pub date: String,
    pub start_time: String,
    pub end_time: String,
    pub guest_name: String,
    pub guest_email: String,
    pub guest_timezone: String,
    pub host_name: String,
    pub host_email: String,
    pub uid: String,
    pub notes: Option<String>,
    /// Guest phone in E.164, only when the event type collects one. Shown in
    /// the host approval request so the host can call before deciding (#231).
    pub guest_phone: Option<String>,
    pub location: Option<String>,
    pub reminder_minutes: Option<i32>,
    pub additional_attendees: Vec<String>,
    /// Guest's preferred language at booking time (from `bookings.language`).
    /// `None` falls back to English at send time.
    pub guest_language: Option<String>,
    /// Host's saved UI-language preference (from `users.language`).
    /// `None` falls back to English at send time.
    pub host_language: Option<String>,
    /// Host's IANA timezone — the event type's tz when set (via `get_host_tz`),
    /// otherwise the host user's tz. When set and different from
    /// `guest_timezone`, host-targeted emails display the wall-clock time
    /// converted into this zone. Empty string falls back to guest-zone display.
    pub host_timezone: String,
    /// Shared resource(s) reserved for this booking (assigned resource in
    /// round-robin mode, attached resources in 'all' mode). Rendered in
    /// host-facing emails only; guests do not see internal resource names.
    pub resource_name: Option<String>,
    /// SEQUENCE of the booking's invite (`bookings.ics_sequence`): 0 until a
    /// reschedule moves it (#267).
    pub sequence: u32,
    /// Saved language of additional attendees who are calrs users, by
    /// address: a watcher who claimed the booking is listed as an attendee
    /// and reads their copies in their own language. Every other attendee
    /// reads the booking's language. See `attendee_lang`.
    pub attendee_languages: Vec<(String, String)>,
}

#[derive(Clone, Default)]
pub struct CancellationDetails {
    /// Exact UTC endpoints for new bookings; legacy records use wall-clock fields.
    pub utc_times: Option<(String, String)>,
    pub event_title: String,
    pub date: String,
    pub start_time: String,
    pub end_time: String,
    pub guest_name: String,
    pub guest_email: String,
    pub guest_timezone: String,
    pub host_name: String,
    pub host_email: String,
    pub uid: String,
    pub reason: Option<String>,
    pub cancelled_by_host: bool,
    pub guest_language: Option<String>,
    pub host_language: Option<String>,
    /// Host's IANA timezone (from `users.timezone`). See `BookingDetails::host_timezone`.
    pub host_timezone: String,
    /// See `BookingDetails::sequence`. A CANCEL below the copy's SEQUENCE is
    /// ignored as out of date.
    pub sequence: u32,
    /// Whether the guest and the additional attendees were sent the invite,
    /// i.e. the booking was confirmed at least once (`first_confirmed_at`).
    /// A declined or expired request then carries a CANCEL too: it was a
    /// reschedule awaiting approval, and their calendars still hold the
    /// confirmed time (#267).
    pub invite_sent: bool,
    /// The booking ended while a reschedule awaited approval: the times
    /// above are the requested ones, which the attendees never heard of, so
    /// their cancellation leaves them out.
    pub reschedule_pending: bool,
    /// Additional attendees holding a copy of the invite, empty unless
    /// `invite_sent`. Each is sent a cancellation with the CANCEL.
    pub additional_attendees: Vec<String>,
    /// See `BookingDetails::attendee_languages`.
    pub attendee_languages: Vec<(String, String)>,
}

// --- Reschedule emails ---

#[derive(Clone, Default)]
pub struct RescheduleDetails {
    pub old_utc_times: Option<(String, String)>,
    /// Exact UTC endpoints for new bookings; legacy records use wall-clock fields.
    pub utc_times: Option<(String, String)>,
    pub event_title: String,
    pub old_date: String,
    pub old_start_time: String,
    pub old_end_time: String,
    pub new_date: String,
    pub new_start_time: String,
    pub new_end_time: String,
    pub guest_name: String,
    pub guest_email: String,
    pub guest_timezone: String,
    pub host_name: String,
    pub host_email: String,
    pub uid: String,
    pub location: Option<String>,
    /// What the guest typed with the booking, shown in the host's approval
    /// request for the new time as in the original one (#266). See
    /// `BookingDetails::notes` and `BookingDetails::guest_phone`.
    pub notes: Option<String>,
    pub guest_phone: Option<String>,
    /// Host's IANA timezone (from `users.timezone`). See `BookingDetails::host_timezone`.
    pub host_timezone: String,
    /// Language the guest booked in (`bookings.language`). See
    /// `BookingDetails::guest_language`.
    pub guest_language: Option<String>,
    /// Host recipient's saved language (`users.language`). See
    /// `BookingDetails::host_language`.
    pub host_language: Option<String>,
    /// SEQUENCE of the moved invite. See `BookingDetails::sequence`.
    pub sequence: u32,
    /// Listed in the guest's moved invite, as in their confirmation: the
    /// higher SEQUENCE replaces that copy whole (#267).
    pub additional_attendees: Vec<String>,
}

#[cfg(test)]
mod tests;
