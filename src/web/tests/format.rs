//! Date and time formatting helpers.

use super::*;

// --- parse_booking_datetime tests ---

#[test]
fn parse_booking_datetime_iso_format() {
    assert_eq!(
        parse_booking_datetime("2026-03-15T14:30:00"),
        Some(dt(2026, 3, 15, 14, 30))
    );
}

#[test]
fn parse_booking_datetime_space_format() {
    assert_eq!(
        parse_booking_datetime("2026-03-15 14:30:00"),
        Some(dt(2026, 3, 15, 14, 30))
    );
}

#[test]
fn parse_booking_datetime_trailing_z() {
    assert_eq!(
        parse_booking_datetime("2026-03-15T14:30:00Z"),
        Some(dt(2026, 3, 15, 14, 30))
    );
}

#[test]
fn parse_booking_datetime_invalid() {
    assert_eq!(parse_booking_datetime("not-a-date"), None);
    assert_eq!(parse_booking_datetime(""), None);
    assert_eq!(parse_booking_datetime("2026-13-40T99:99:99"), None);
}

// --- format_booking_datetime tests ---

#[test]
fn format_booking_datetime_far_future_same_year_not_guaranteed() {
    // Use a date far in the future (2099) so it's never "today"/"tomorrow"/weekday
    let result = format_booking_datetime("2099-07-20T09:15:00");
    assert_eq!(result, "Mon, Jul 20, 2099 at 9:15 AM");
}

#[test]
fn format_booking_datetime_different_year() {
    let result = format_booking_datetime("2050-12-25T18:00:00");
    assert_eq!(result, "Sun, Dec 25, 2050 at 6:00 PM");
}

#[test]
fn format_booking_datetime_invalid_fallback() {
    assert_eq!(format_booking_datetime("garbage"), "garbage");
    assert_eq!(format_booking_datetime(""), "");
}

// --- format_booking_range tests ---

#[test]
fn format_booking_range_far_future() {
    let result = format_booking_range("2099-07-20T09:00:00", "2099-07-20T09:30:00");
    assert_eq!(result, "Mon, Jul 20, 2099 at 9:00 AM — 9:30 AM");
}

#[test]
fn format_booking_range_end_unparseable() {
    let result = format_booking_range("2099-07-20T09:00:00", "bad");
    assert_eq!(result, "Mon, Jul 20, 2099 at 9:00 AM — bad");
}

// --- format_booking_for_dashboard tests ---
//
// Regression for issue #100: when the event type's stored timezone differs
// from the host's profile timezone, the dashboard previously displayed the
// raw stored time with no timezone label, which the host read as their own
// time. The fix shifts the primary label into the host's profile tz and
// adds a secondary guest-tz line.

#[test]
fn dashboard_label_converts_stored_tz_to_host_profile_tz() {
    // The user's exact scenario: event type set to America/New_York
    // (UTC-5 in winter), host's profile is Europe/Paris (UTC+1 in winter),
    // guest booked from America/New_York at 10:00 → stored as "10:00".
    // Host should see "4:00 PM" (Paris equivalent), guest line shows "10:00 EST".
    let (primary, secondary) = format_booking_for_dashboard(
        "2099-01-20T10:00:00",
        "2099-01-20T10:30:00",
        "America/New_York",
        "Europe/Paris",
        "America/New_York",
    );
    assert!(
        primary.contains("4:00 PM") && primary.contains("4:30 PM"),
        "primary should be in Europe/Paris, got: {primary}"
    );
    assert!(
        primary.contains("CET"),
        "primary should label tz, got: {primary}"
    );
    let sec = secondary.expect("secondary line expected when guest tz differs from host tz");
    assert!(
        sec.contains("10:00 AM"),
        "secondary should show guest 10:00 AM, got: {sec}"
    );
    assert!(
        sec.contains("EST"),
        "secondary should label guest tz, got: {sec}"
    );
}

#[test]
fn dashboard_label_omits_secondary_when_guest_tz_equals_host_tz() {
    let (_primary, secondary) = format_booking_for_dashboard(
        "2099-07-20T14:00:00",
        "2099-07-20T14:30:00",
        "Europe/Paris",
        "Europe/Paris",
        "Europe/Paris",
    );
    assert!(
        secondary.is_none(),
        "no secondary line when host and guest share tz"
    );
}

#[test]
fn dashboard_label_falls_back_for_unparseable_input() {
    let (primary, secondary) = format_booking_for_dashboard(
        "garbage",
        "also-garbage",
        "Europe/Paris",
        "Europe/Paris",
        "America/New_York",
    );
    assert_eq!(primary, "garbage — also-garbage");
    assert!(secondary.is_none());
}

// --- format_date_label tests ---

#[test]
fn format_date_label_from_datetime() {
    assert_eq!(
        format_date_label("2026-03-15T14:30:00", "en"),
        "Sunday, March 15, 2026"
    );
}

#[test]
fn format_date_label_from_date_only() {
    assert_eq!(
        format_date_label("2026-03-15", "en"),
        "Sunday, March 15, 2026"
    );
}

#[test]
fn format_date_label_space_separator() {
    assert_eq!(
        format_date_label("2026-03-15 14:30:00", "en"),
        "Sunday, March 15, 2026"
    );
}

#[test]
fn format_date_label_invalid_fallback() {
    assert_eq!(format_date_label("nope", "en"), "nope");
}

#[test]
fn format_date_label_french() {
    assert_eq!(
        format_date_label("2026-03-15", "fr"),
        "dimanche 15 mars 2026"
    );
}

// --- format_time_from_dt tests ---

#[test]
fn format_time_from_dt_valid() {
    assert_eq!(format_time_from_dt("2026-03-15T14:30:00"), "2:30 PM");
    assert_eq!(format_time_from_dt("2026-03-15 09:05:00"), "9:05 AM");
}

#[test]
fn format_time_from_dt_midnight() {
    assert_eq!(format_time_from_dt("2026-03-15T00:00:00"), "12:00 AM");
}

#[test]
fn format_time_from_dt_unparseable_long_string() {
    // 16+ chars but not a valid datetime → falls back to substring [11..16]
    assert_eq!(format_time_from_dt("XXXX-XX-XX_HH:MM:SS"), "HH:MM");
}

#[test]
fn format_time_from_dt_short_string() {
    assert_eq!(format_time_from_dt("short"), "00:00");
    assert_eq!(format_time_from_dt(""), "00:00");
}

// --- extract_time_24h tests ---

#[test]
fn extract_time_24h_returns_24h_format() {
    assert_eq!(extract_time_24h("2026-03-15T14:30:00"), "14:30");
    assert_eq!(extract_time_24h("2026-03-15 09:05:00"), "09:05");
    assert_eq!(extract_time_24h("2026-03-15T00:00:00"), "00:00");
}

#[test]
fn extract_time_24h_short_string() {
    assert_eq!(extract_time_24h("short"), "00:00");
    assert_eq!(extract_time_24h(""), "00:00");
}

#[test]
fn format_booking_range_same_day() {
    let result = format_booking_range("2026-06-15T10:00:00", "2026-06-15T10:30:00");
    assert!(result.contains("10:00"));
    assert!(result.contains("10:30"));
}

#[test]
fn extract_time_24h_from_iso() {
    assert_eq!(extract_time_24h("2026-06-15T14:30:00"), "14:30");
}

#[test]
fn extract_time_24h_from_space_sep() {
    assert_eq!(extract_time_24h("2026-06-15 14:30:00"), "14:30");
}

#[test]
fn format_date_label_full_datetime() {
    let result = format_date_label("2026-06-15T10:00:00", "en");
    assert!(result.contains("June") || result.contains("15") || result.contains("2026"));
}

// End-to-end TZ pipeline used by background email paths (reminder loop,
// reschedule): `start_at` is stored in the event-type tz, must be converted
// to the guest tz to populate BookingDetails, and host_time_display must
// recover the host wall-clock for host-targeted emails. Regression for the
// reviewer-caught bug where the reminder loop short-circuited the first
// conversion and the host email displayed times in the wrong zone.
#[test]
fn stored_to_host_email_recovers_host_wall_clock() {
    // Scenario from issue #119: Paris event type, LA guest, booked at
    // what the guest saw as 07:00 LA = 16:00 Paris. start_at stored as
    // Paris wall-clock.
    let stored_start = "2026-05-26T16:00:00";
    let stored_end = "2026-05-26T16:30:00";
    let host_tz: Tz = "Europe/Paris".parse().unwrap();
    let guest_tz: Tz = "America/Los_Angeles".parse().unwrap();

    // Step 1: convert stored (host) -> guest wall-clock, as the reminder
    // loop and reschedule handler now do before building BookingDetails.
    let (date, start_time, end_time) =
        booking_strings_in_guest_tz(stored_start, stored_end, host_tz, guest_tz);
    assert_eq!(date, "2026-05-26");
    assert_eq!(start_time, "07:00");
    assert_eq!(end_time, "07:30");

    // Step 2: host_time_display converts guest wall-clock back to host.
    let (host_date, time_display) = crate::email::host_time_display(
        &date,
        &start_time,
        &end_time,
        guest_tz.name(),
        host_tz.name(),
    );
    assert_eq!(host_date, "2026-05-26");
    assert_eq!(time_display, "16:00 \u{2013} 16:30 (Europe/Paris)");
}

#[test]
fn dashboard_when_lays_out_the_booking_in_the_host_zone() {
    let when = dashboard_when(
        "2099-01-20T10:00:00",
        "2099-01-20T10:30:00",
        "America/New_York",
        "Europe/Paris",
        "America/New_York",
        "fr",
    )
    .expect("readable times");
    let attr = |name: &str| when.get_attr(name).unwrap().to_string();
    assert_eq!(attr("time_range"), "16:00\u{2013}16:30");
    assert_eq!(attr("tz_label"), "CET");
    assert_eq!(attr("iso"), "2099-01-20T15:00:00Z");
    assert_eq!(attr("day"), "20");
    assert_eq!(attr("month"), "janv.");
    assert_eq!(attr("guest_range"), "10:00\u{2013}10:30 EST");
}

#[test]
fn dashboard_when_is_none_for_unreadable_times() {
    assert!(dashboard_when("garbage", "x", "UTC", "UTC", "UTC", "en").is_none());
}
