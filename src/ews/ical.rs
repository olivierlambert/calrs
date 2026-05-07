//! Convert EWS calendar items into iCalendar text.
//!
//! When sync responses ship MIME content we can extract the iCal block
//! verbatim (see [`super::parse::extract_vcalendar`]). For lighter `FindItem`
//! results we synthesise a minimal VEVENT from the structured fields. The
//! goal is feature parity with the CalDAV path: calrs only needs UID, start,
//! end, summary, location, status, and the all-day flag to compute slot
//! availability.

use super::parse::EwsCalendarItem;

/// Synthesise a minimal `BEGIN:VCALENDAR…END:VCALENDAR` block from a
/// `FindItem` result. Useful when the caller doesn't want a follow-up
/// `GetItem` round trip.
pub fn synth_vcalendar(item: &EwsCalendarItem) -> Option<String> {
    let uid = item
        .uid
        .clone()
        .or_else(|| Some(item.item_id.clone()))
        .filter(|s| !s.is_empty())?;
    let start = item.start.clone()?;
    let end = item.end.clone()?;

    let dtstart = format_dt(&start, item.is_all_day);
    let dtend = format_dt(&end, item.is_all_day);

    let summary = item
        .subject
        .as_deref()
        .map(escape_ical_text)
        .unwrap_or_default();
    let location = item
        .location
        .as_deref()
        .map(escape_ical_text)
        .unwrap_or_default();

    // calrs availability checks treat TRANSPARENT events as non-blocking. EWS
    // exposes that intent via LegacyFreeBusyStatus.
    let transp = match item.free_busy_status.as_deref() {
        Some("Free") | Some("Tentative") => "TRANSPARENT",
        _ => "OPAQUE",
    };

    let status = if item.is_cancelled {
        "CANCELLED"
    } else {
        "CONFIRMED"
    };

    let mut buf = String::new();
    buf.push_str("BEGIN:VCALENDAR\r\n");
    buf.push_str("VERSION:2.0\r\n");
    buf.push_str("PRODID:-//calrs//ews-bridge//EN\r\n");
    buf.push_str("BEGIN:VEVENT\r\n");
    buf.push_str(&format!("UID:{uid}\r\n"));
    buf.push_str(&format!("DTSTART{dtstart}\r\n"));
    buf.push_str(&format!("DTEND{dtend}\r\n"));
    if !summary.is_empty() {
        buf.push_str(&format!("SUMMARY:{summary}\r\n"));
    }
    if !location.is_empty() {
        buf.push_str(&format!("LOCATION:{location}\r\n"));
    }
    buf.push_str(&format!("TRANSP:{transp}\r\n"));
    buf.push_str(&format!("STATUS:{status}\r\n"));
    buf.push_str("END:VEVENT\r\n");
    buf.push_str("END:VCALENDAR\r\n");
    Some(buf)
}

/// Format an EWS datetime (`2026-05-06T09:00:00Z` or
/// `2026-05-08T00:00:00`) as the iCal property value, including the right
/// VALUE/TZID hint. EWS stores naive-local-with-timezone or UTC; the
/// generated iCal mirrors the source semantics.
fn format_dt(value: &str, all_day: bool) -> String {
    if all_day {
        // All-day events use VALUE=DATE with YYYYMMDD.
        let date = value.chars().take(10).collect::<String>().replace('-', "");
        return format!(";VALUE=DATE:{}", date);
    }
    // EWS UTC is YYYY-MM-DDTHH:MM:SSZ → iCal UTC YYYYMMDDTHHMMSSZ.
    let stripped = value.replace(['-', ':'], "");
    let stripped = stripped.trim().to_string();
    if stripped.ends_with('Z') {
        format!(":{}", stripped)
    } else {
        // Local-only datetime; let downstream parsers treat as floating.
        format!(":{}", stripped)
    }
}

/// Escape a string for embedding in an iCal TEXT property. Per RFC 5545:
/// backslash, comma, semicolon and newlines need escaping.
fn escape_ical_text(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for c in value.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            ',' => out.push_str("\\,"),
            ';' => out.push_str("\\;"),
            '\n' => out.push_str("\\n"),
            '\r' => {}
            _ => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(start: &str, end: &str, all_day: bool) -> EwsCalendarItem {
        EwsCalendarItem {
            item_id: "ID".to_string(),
            change_key: None,
            uid: Some("uid-1".to_string()),
            subject: Some("Hello, world".to_string()),
            start: Some(start.to_string()),
            end: Some(end.to_string()),
            location: Some("Room 1".to_string()),
            is_all_day: all_day,
            is_cancelled: false,
            free_busy_status: Some("Busy".to_string()),
            has_recurrence: false,
        }
    }

    #[test]
    fn synth_basic_event() {
        let it = item("2026-05-06T09:00:00Z", "2026-05-06T09:30:00Z", false);
        let ics = synth_vcalendar(&it).unwrap();
        assert!(ics.contains("UID:uid-1"));
        assert!(ics.contains("DTSTART:20260506T090000Z"));
        assert!(ics.contains("DTEND:20260506T093000Z"));
        // RFC 5545 escaping: comma must be backslash-escaped
        assert!(ics.contains("SUMMARY:Hello\\, world"));
        assert!(ics.contains("LOCATION:Room 1"));
        assert!(ics.contains("TRANSP:OPAQUE"));
        assert!(ics.contains("STATUS:CONFIRMED"));
    }

    #[test]
    fn synth_all_day() {
        let it = item("2026-05-08T00:00:00", "2026-05-09T00:00:00", true);
        let ics = synth_vcalendar(&it).unwrap();
        assert!(ics.contains("DTSTART;VALUE=DATE:20260508"));
        assert!(ics.contains("DTEND;VALUE=DATE:20260509"));
    }

    #[test]
    fn synth_marks_free_as_transparent() {
        let mut it = item("2026-05-06T09:00:00Z", "2026-05-06T09:30:00Z", false);
        it.free_busy_status = Some("Free".to_string());
        let ics = synth_vcalendar(&it).unwrap();
        assert!(ics.contains("TRANSP:TRANSPARENT"));
    }

    #[test]
    fn synth_marks_cancelled_status() {
        let mut it = item("2026-05-06T09:00:00Z", "2026-05-06T09:30:00Z", false);
        it.is_cancelled = true;
        let ics = synth_vcalendar(&it).unwrap();
        assert!(ics.contains("STATUS:CANCELLED"));
    }

    #[test]
    fn synth_uses_item_id_when_uid_missing() {
        let mut it = item("2026-05-06T09:00:00Z", "2026-05-06T09:30:00Z", false);
        it.uid = None;
        let ics = synth_vcalendar(&it).unwrap();
        assert!(ics.contains("UID:ID"));
    }
}
