//! Date and time formatting for booking labels, emails and the dashboard.

use super::*;

/// Parse a datetime string from the database, handling both space and T separators.
/// Supports: "2025-03-15 14:30:00", "2025-03-15T14:30:00", "2025-03-15T14:30:00Z"
pub(super) fn parse_booking_datetime(dt_str: &str) -> Option<NaiveDateTime> {
    let s = dt_str.trim_end_matches('Z');
    NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S")
        .or_else(|_| NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S"))
        .ok()
}

/// Format a booking datetime string into a human-friendly label.
/// Input: "2025-03-15 14:30:00" or "2025-03-15T14:30:00"
/// Output: "Tomorrow at 2:30 PM" or "Sat, Mar 15 at 2:30 PM" or "Sat, Mar 15, 2026 at 2:30 PM"
pub(super) fn format_booking_datetime(dt_str: &str) -> String {
    let ndt = match parse_booking_datetime(dt_str) {
        Some(d) => d,
        None => return dt_str.to_string(),
    };
    let now = Local::now().naive_local();
    let today = now.date();
    let date = ndt.date();
    let time = ndt.time().format("%-I:%M %p").to_string();

    let day_diff = (date - today).num_days();
    let date_part = if day_diff == 0 {
        "Today".to_string()
    } else if day_diff == 1 {
        "Tomorrow".to_string()
    } else if day_diff < 7 {
        date.format("%A").to_string() // e.g. "Wednesday"
    } else if date.year() == today.year() {
        date.format("%a, %b %-d").to_string() // e.g. "Sat, Mar 15"
    } else {
        date.format("%a, %b %-d, %Y").to_string() // e.g. "Sat, Mar 15, 2026"
    };
    format!("{} at {}", date_part, time)
}

/// Format a time range for booking display.
/// Returns e.g. "Tomorrow at 2:30 PM — 3:00 PM"
pub(super) fn format_booking_range(start_str: &str, end_str: &str) -> String {
    let start_label = format_booking_datetime(start_str);
    // For the end, only show the time (same day implied)
    if let Some(end_ndt) = parse_booking_datetime(end_str) {
        let end_time = end_ndt.time().format("%-I:%M %p").to_string();
        format!("{} — {}", start_label, end_time)
    } else {
        format!("{} — {}", start_label, end_str)
    }
}

/// Convert a naive datetime from one timezone to another, going via UTC. Used
/// when the dashboard needs to show a stored time (which is naive in the
/// event-type's timezone) in either the host's profile timezone or the guest's
/// selected timezone.
fn convert_naive_between_tz(naive: NaiveDateTime, from: Tz, to: Tz) -> NaiveDateTime {
    let utc = from
        .from_local_datetime(&naive)
        .earliest()
        .unwrap_or_else(|| from.from_utc_datetime(&naive))
        .with_timezone(&Utc);
    utc.with_timezone(&to).naive_local()
}

/// Convert stored booking start/end (naive in `stored_tz`) to the guest's
/// timezone and return (date, start_time, end_time) strings used to populate
/// `BookingDetails`. `email::generate_ics` calls `convert_to_utc(date,
/// start_time, end_time, guest_timezone)`, so these fields must be guest-local
/// for the ICS and the rendered email body to match what the guest booked.
/// Fixes #101.
pub(super) fn booking_strings_in_guest_tz(
    start_at: &str,
    end_at: &str,
    stored_tz: Tz,
    guest_tz: Tz,
) -> (String, String, String) {
    match (
        crate::booking_time::local(start_at, stored_tz, guest_tz),
        crate::booking_time::local(end_at, stored_tz, guest_tz),
    ) {
        (Some(s), Some(e)) => {
            let gs = s;
            let ge = e;
            (
                gs.date().format("%Y-%m-%d").to_string(),
                gs.time().format("%H:%M").to_string(),
                ge.time().format("%H:%M").to_string(),
            )
        }
        _ => (
            start_at.get(..10).unwrap_or(start_at).to_string(),
            extract_time_24h(start_at),
            extract_time_24h(end_at),
        ),
    }
}

/// Short timezone label for display next to a time, e.g. "CEST" or "EST".
/// Falls back to the IANA name when the abbreviation is numeric (zones like
/// Asia/Tehran which expose "+0330" rather than a letter code).
fn tz_short_label(tz: Tz, at: NaiveDateTime) -> String {
    let dt = tz
        .from_local_datetime(&at)
        .earliest()
        .unwrap_or_else(|| tz.from_utc_datetime(&at));
    let abbr = dt.offset().abbreviation();
    if abbr.len() >= 3 && abbr.chars().all(|c| c.is_ascii_alphabetic()) {
        abbr.to_string()
    } else {
        tz.name().to_string()
    }
}

/// Format a booking range for dashboard display: returns a primary label in the
/// host's profile timezone (with tz suffix), and an optional secondary label
/// showing the guest's view when the two timezones differ.
///
/// `start_str`/`end_str` are naive datetimes stored in `stored_tz` (the event
/// type's timezone, falling back to the host's profile tz for legacy rows).
pub(super) fn format_booking_for_dashboard(
    start_str: &str,
    end_str: &str,
    stored_tz: &str,
    host_tz: &str,
    guest_tz: &str,
) -> (String, Option<String>) {
    let stored = stored_tz.parse::<Tz>().unwrap_or(Tz::UTC);
    let host = host_tz.parse::<Tz>().unwrap_or(Tz::UTC);
    let guest = guest_tz.parse::<Tz>().unwrap_or(Tz::UTC);

    let (start, end) = match (
        crate::booking_time::local(start_str, stored, host),
        crate::booking_time::local(end_str, stored, host),
    ) {
        (Some(s), Some(e)) => (s, e),
        _ => return (format_booking_range(start_str, end_str), None),
    };

    let host_start = start;
    let host_end = end;
    let today_host = Utc::now().with_timezone(&host).date_naive();
    let primary = format_dashboard_range(
        host_start,
        host_end,
        today_host,
        &tz_short_label(host, host_start),
    );

    let secondary = if guest != host {
        let g_start = crate::booking_time::local(start_str, stored, guest).unwrap();
        let g_end = crate::booking_time::local(end_str, stored, guest).unwrap();
        let g_time_start = g_start.time().format("%-I:%M %p").to_string();
        let g_time_end = g_end.time().format("%-I:%M %p").to_string();
        Some(format!(
            "{} — {} {}",
            g_time_start,
            g_time_end,
            tz_short_label(guest, g_start)
        ))
    } else {
        None
    };

    (primary, secondary)
}

fn format_dashboard_range(
    start: NaiveDateTime,
    end: NaiveDateTime,
    today: NaiveDate,
    tz_label: &str,
) -> String {
    let date = start.date();
    let time_start = start.time().format("%-I:%M %p").to_string();
    let time_end = end.time().format("%-I:%M %p").to_string();
    let day_diff = (date - today).num_days();
    let date_part = if day_diff == 0 {
        "Today".to_string()
    } else if day_diff == 1 {
        "Tomorrow".to_string()
    } else if (1..7).contains(&day_diff) {
        date.format("%A").to_string()
    } else if date.year() == today.year() {
        date.format("%a, %b %-d").to_string()
    } else {
        date.format("%a, %b %-d, %Y").to_string()
    };
    format!(
        "{} at {} — {} {}",
        date_part, time_start, time_end, tz_label
    )
}

/// Format a raw date string (YYYY-MM-DD or from datetime) into a human-friendly
/// localized date label. Returns e.g. "Saturday, March 15, 2026" / "samedi 15 mars 2026".
pub(super) fn format_date_label(dt_str: &str, lang: &str) -> String {
    // Try parsing as full datetime first, then as date-only
    if let Some(ndt) = parse_booking_datetime(dt_str) {
        return crate::i18n::format_long_date(ndt.date(), lang);
    }
    if let Ok(d) = NaiveDate::parse_from_str(&dt_str[..10.min(dt_str.len())], "%Y-%m-%d") {
        return crate::i18n::format_long_date(d, lang);
    }
    dt_str.to_string()
}

/// Extract a human-friendly time (e.g. "2:30 PM") from a datetime string.
pub(super) fn format_time_from_dt(dt_str: &str) -> String {
    if let Some(ndt) = parse_booking_datetime(dt_str) {
        ndt.time().format("%-I:%M %p").to_string()
    } else if dt_str.len() >= 16 {
        dt_str[11..16].to_string()
    } else {
        "00:00".to_string()
    }
}

/// Extract HH:MM (24-hour) from a booking datetime for ICS generation.
/// format_time_from_dt returns 12-hour display format ("2:00 PM") which
/// convert_to_utc cannot parse. This function returns "14:00" format.
pub(super) fn extract_time_24h(dt_str: &str) -> String {
    if let Some(ndt) = parse_booking_datetime(dt_str) {
        ndt.time().format("%H:%M").to_string()
    } else if dt_str.len() >= 16 {
        dt_str[11..16].to_string()
    } else {
        "00:00".to_string()
    }
}

/// The pieces the bookings list lays out for one booking, in the host's
/// timezone: a calendar leaf (short month, day, short weekday), the time
/// range in 24-hour form, the zone label, the long date for screen readers,
/// and the UTC instant the page turns into a relative time ("in 2 days").
/// The guest's own time range is added when their zone differs. `None` when
/// the stored times cannot be read; the template then falls back to the
/// plain `start_at` label.
pub(super) fn dashboard_when(
    start_str: &str,
    end_str: &str,
    stored_tz: &str,
    host_tz: &str,
    guest_tz: &str,
    lang: &str,
) -> Option<minijinja::Value> {
    let stored = stored_tz.parse::<Tz>().unwrap_or(Tz::UTC);
    let host = host_tz.parse::<Tz>().unwrap_or(Tz::UTC);
    let guest = guest_tz.parse::<Tz>().unwrap_or(Tz::UTC);
    let start = crate::booking_time::local(start_str, stored, host)?;
    let end = crate::booking_time::local(end_str, stored, host)?;
    let start_utc = crate::booking_time::local(start_str, stored, Tz::UTC)?;
    let today = Utc::now().with_timezone(&host).date_naive();
    let date = start.date();
    let range = |s: NaiveDateTime, e: NaiveDateTime| {
        format!("{}\u{2013}{}", s.format("%H:%M"), e.format("%H:%M"))
    };
    let guest_range = if guest != host {
        let gs = crate::booking_time::local(start_str, stored, guest)?;
        let ge = crate::booking_time::local(end_str, stored, guest)?;
        format!("{} {}", range(gs, ge), tz_short_label(guest, gs))
    } else {
        String::new()
    };
    Some(context! {
        iso => format!("{}Z", start_utc.format("%Y-%m-%dT%H:%M:%S")),
        day => date.day(),
        month => crate::i18n::translate(lang, month_short_key(date.month()), None),
        weekday => crate::i18n::translate(lang, weekday_short_key(date.weekday()), None),
        long_date => crate::i18n::format_long_date(date, lang),
        time_range => range(start, end),
        tz_label => tz_short_label(host, start),
        guest_range => guest_range,
        is_today => date == today,
    })
}

fn weekday_short_key(d: chrono::Weekday) -> &'static str {
    use chrono::Weekday;
    match d {
        Weekday::Mon => "slots-weekday-mon",
        Weekday::Tue => "slots-weekday-tue",
        Weekday::Wed => "slots-weekday-wed",
        Weekday::Thu => "slots-weekday-thu",
        Weekday::Fri => "slots-weekday-fri",
        Weekday::Sat => "slots-weekday-sat",
        Weekday::Sun => "slots-weekday-sun",
    }
}

fn month_short_key(month: u32) -> &'static str {
    match month {
        1 => "common-month-short-1",
        2 => "common-month-short-2",
        3 => "common-month-short-3",
        4 => "common-month-short-4",
        5 => "common-month-short-5",
        6 => "common-month-short-6",
        7 => "common-month-short-7",
        8 => "common-month-short-8",
        9 => "common-month-short-9",
        10 => "common-month-short-10",
        11 => "common-month-short-11",
        _ => "common-month-short-12",
    }
}

/// A stored UTC timestamp ("2026-03-15 14:30:00" or "2026-03-15T14:30:00Z")
/// as RFC 3339, for the dashboard to render as a relative time. Empty when
/// the value is missing or unreadable.
pub(super) fn utc_iso(value: Option<&str>) -> String {
    value
        .and_then(crate::booking_time::naive)
        .map(|t| format!("{}Z", t.format("%Y-%m-%dT%H:%M:%S")))
        .unwrap_or_default()
}
