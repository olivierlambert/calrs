use std::io::{self, Write};

use chrono::NaiveDateTime;
use chrono_tz::Tz;

/// Split an iCal blob into individual VEVENT blocks.
/// A single CalDAV resource can contain multiple VEVENTs when a recurring
/// event has modified instances (RECURRENCE-ID).
pub fn split_vevents(ical: &str) -> Vec<String> {
    let mut blocks = Vec::new();
    let mut search_from = 0;
    while let Some(start) = ical[search_from..].find("BEGIN:VEVENT") {
        let abs_start = search_from + start;
        if let Some(end) = ical[abs_start..].find("END:VEVENT") {
            let abs_end = abs_start + end + "END:VEVENT".len();
            blocks.push(ical[abs_start..abs_end].to_string());
            search_from = abs_end;
        } else {
            break;
        }
    }
    if blocks.is_empty() {
        blocks.push(ical.to_string());
    }
    blocks
}

/// Extract a field value from a single VEVENT block.
pub fn extract_vevent_field(vevent: &str, field: &str) -> Option<String> {
    for line in vevent.lines() {
        if line.starts_with(field) {
            if let Some(colon_pos) = line.find(':') {
                let value = line[colon_pos + 1..].trim().to_string();
                if !value.is_empty() {
                    return Some(value);
                }
            }
        }
    }
    None
}

/// Extract the TZID from a DTSTART or DTEND line in a VEVENT block.
///
/// - `DTSTART;TZID=Europe/Paris:20260310T100000` → `Some("Europe/Paris")`
/// - `DTSTART:20260310T100000Z` → `Some("UTC")`
/// - `DTSTART:20260310T100000` (no TZID, no Z) → `None` (floating/local)
/// - `DTSTART;VALUE=DATE:20260310` → `None` (all-day)
pub fn extract_vevent_tzid(vevent: &str, field: &str) -> Option<String> {
    for line in vevent.lines() {
        if !line.starts_with(field) {
            continue;
        }
        // Ensure we match the exact field, not a prefix (e.g. DTSTART vs DTSTART-EXTRA)
        let rest = &line[field.len()..];
        if rest.is_empty() {
            continue;
        }
        let first = rest.as_bytes()[0];
        if first != b';' && first != b':' {
            continue;
        }

        // Check for VALUE=DATE (all-day) — no timezone
        if rest.contains("VALUE=DATE") {
            return None;
        }

        // Check for TZID= parameter
        if let Some(tzid_pos) = rest.find("TZID=") {
            let after_tzid = &rest[tzid_pos + 5..];
            // TZID value ends at ':' or ';'
            let end = after_tzid.find(|c| c == ':' || c == ';').unwrap_or(after_tzid.len());
            let tz = after_tzid[..end].trim();
            if !tz.is_empty() {
                return Some(tz.to_string());
            }
        }

        // Check for trailing Z (UTC)
        if let Some(colon_pos) = rest.find(':') {
            let value = rest[colon_pos + 1..].trim();
            if value.ends_with('Z') {
                return Some("UTC".to_string());
            }
        }

        // No TZID, no Z → floating
        return None;
    }
    None
}

/// Convert a NaiveDateTime from the event's timezone to the target timezone.
///
/// - If `event_tz` is `Some` and a valid IANA timezone → convert
/// - If `None` (floating) → return as-is (backward-compatible)
/// - If IANA parse fails → return as-is (graceful degradation)
pub fn convert_event_to_tz(dt: NaiveDateTime, event_tz: Option<&str>, target_tz: Tz) -> NaiveDateTime {
    let etz: Tz = match event_tz {
        Some(tz_str) => match tz_str.parse::<Tz>() {
            Ok(tz) => tz,
            Err(_) => return dt,
        },
        None => return dt,
    };

    // Convert: event's local time → absolute instant → target TZ local time
    use chrono::TimeZone;
    match etz.from_local_datetime(&dt).earliest() {
        Some(zoned) => zoned.with_timezone(&target_tz).naive_local(),
        None => dt, // impossible time during DST transition
    }
}

pub fn prompt(label: &str) -> String {
    print!("{}: ", label);
    io::stdout().flush().unwrap();
    let mut input = String::new();
    io::stdin().read_line(&mut input).unwrap();
    input.trim().to_string()
}

pub fn prompt_password(label: &str) -> String {
    print!("{}: ", label);
    io::stdout().flush().unwrap();
    // TODO: use rpassword for hidden input
    let mut input = String::new();
    io::stdin().read_line(&mut input).unwrap();
    input.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Timelike;

    // --- split_vevents ---

    #[test]
    fn split_single_vevent() {
        let ical = "BEGIN:VCALENDAR\nBEGIN:VEVENT\nUID:abc\nEND:VEVENT\nEND:VCALENDAR";
        let blocks = split_vevents(ical);
        assert_eq!(blocks.len(), 1);
        assert!(blocks[0].starts_with("BEGIN:VEVENT"));
        assert!(blocks[0].ends_with("END:VEVENT"));
    }

    #[test]
    fn split_multiple_vevents() {
        let ical = "\
BEGIN:VCALENDAR\n\
BEGIN:VEVENT\n\
UID:abc\n\
RRULE:FREQ=WEEKLY\n\
END:VEVENT\n\
BEGIN:VEVENT\n\
UID:abc\n\
RECURRENCE-ID:20260309T100000\n\
END:VEVENT\n\
END:VCALENDAR";
        let blocks = split_vevents(ical);
        assert_eq!(blocks.len(), 2);
        assert!(blocks[0].contains("RRULE"));
        assert!(blocks[1].contains("RECURRENCE-ID"));
    }

    #[test]
    fn split_no_vevent_returns_whole() {
        let ical = "BEGIN:VCALENDAR\nEND:VCALENDAR";
        let blocks = split_vevents(ical);
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0], ical);
    }

    #[test]
    fn split_missing_end_vevent() {
        let ical = "BEGIN:VCALENDAR\nBEGIN:VEVENT\nUID:abc\n";
        let blocks = split_vevents(ical);
        // No END:VEVENT → falls back to returning whole string
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0], ical);
    }

    // --- extract_vevent_field ---

    #[test]
    fn extract_existing_field() {
        let vevent = "BEGIN:VEVENT\nUID:test-uid-123\nSUMMARY:Team meeting\nEND:VEVENT";
        assert_eq!(extract_vevent_field(vevent, "UID"), Some("test-uid-123".to_string()));
        assert_eq!(extract_vevent_field(vevent, "SUMMARY"), Some("Team meeting".to_string()));
    }

    #[test]
    fn extract_field_with_params() {
        // DTSTART has timezone parameters before the colon
        let vevent = "BEGIN:VEVENT\nDTSTART;TZID=Europe/Paris:20260310T100000\nEND:VEVENT";
        assert_eq!(extract_vevent_field(vevent, "DTSTART"), Some("20260310T100000".to_string()));
    }

    #[test]
    fn extract_nonexistent_field() {
        let vevent = "BEGIN:VEVENT\nUID:abc\nEND:VEVENT";
        assert_eq!(extract_vevent_field(vevent, "SUMMARY"), None);
    }

    #[test]
    fn extract_empty_value() {
        let vevent = "BEGIN:VEVENT\nSUMMARY:\nEND:VEVENT";
        assert_eq!(extract_vevent_field(vevent, "SUMMARY"), None);
    }

    #[test]
    fn extract_does_not_match_substring() {
        // DTSTART should not match DTSTART-EXTRA or other prefixed fields
        let vevent = "BEGIN:VEVENT\nDTSTART:20260310T100000\nDTSTART-EXTRA:ignored\nEND:VEVENT";
        assert_eq!(extract_vevent_field(vevent, "DTSTART"), Some("20260310T100000".to_string()));
    }

    // --- extract_vevent_tzid ---

    #[test]
    fn tzid_with_explicit_timezone() {
        let vevent = "BEGIN:VEVENT\nDTSTART;TZID=Europe/Paris:20260310T100000\nEND:VEVENT";
        assert_eq!(extract_vevent_tzid(vevent, "DTSTART"), Some("Europe/Paris".to_string()));
    }

    #[test]
    fn tzid_utc_suffix() {
        let vevent = "BEGIN:VEVENT\nDTSTART:20260310T100000Z\nEND:VEVENT";
        assert_eq!(extract_vevent_tzid(vevent, "DTSTART"), Some("UTC".to_string()));
    }

    #[test]
    fn tzid_floating_no_tz() {
        let vevent = "BEGIN:VEVENT\nDTSTART:20260310T100000\nEND:VEVENT";
        assert_eq!(extract_vevent_tzid(vevent, "DTSTART"), None);
    }

    #[test]
    fn tzid_all_day_value_date() {
        let vevent = "BEGIN:VEVENT\nDTSTART;VALUE=DATE:20260310\nEND:VEVENT";
        assert_eq!(extract_vevent_tzid(vevent, "DTSTART"), None);
    }

    #[test]
    fn tzid_america_new_york() {
        let vevent = "BEGIN:VEVENT\nDTSTART;TZID=America/New_York:20260310T100000\nDTEND;TZID=America/New_York:20260310T110000\nEND:VEVENT";
        assert_eq!(extract_vevent_tzid(vevent, "DTSTART"), Some("America/New_York".to_string()));
        assert_eq!(extract_vevent_tzid(vevent, "DTEND"), Some("America/New_York".to_string()));
    }

    #[test]
    fn tzid_no_matching_field() {
        let vevent = "BEGIN:VEVENT\nDTEND;TZID=UTC:20260310T100000\nEND:VEVENT";
        assert_eq!(extract_vevent_tzid(vevent, "DTSTART"), None);
    }

    #[test]
    fn tzid_does_not_match_prefix() {
        let vevent = "BEGIN:VEVENT\nDTSTART-EXTRA;TZID=Europe/Paris:20260310T100000\nDTSTART:20260310T100000\nEND:VEVENT";
        assert_eq!(extract_vevent_tzid(vevent, "DTSTART"), None); // floating, not the -EXTRA line
    }

    // --- convert_event_to_tz ---

    #[test]
    fn convert_ny_to_paris() {
        use chrono::NaiveDate;
        // 10:00 in New York (EDT, UTC-4) = 16:00 in Paris (CEST, UTC+2) in summer
        let dt = NaiveDate::from_ymd_opt(2026, 7, 15).unwrap().and_hms_opt(10, 0, 0).unwrap();
        let result = convert_event_to_tz(dt, Some("America/New_York"), "Europe/Paris".parse::<Tz>().unwrap());
        assert_eq!(result.hour(), 16);
    }

    #[test]
    fn convert_utc_to_paris() {
        use chrono::NaiveDate;
        // 10:00 UTC = 12:00 Paris (CEST, UTC+2) in summer
        let dt = NaiveDate::from_ymd_opt(2026, 7, 15).unwrap().and_hms_opt(10, 0, 0).unwrap();
        let result = convert_event_to_tz(dt, Some("UTC"), "Europe/Paris".parse::<Tz>().unwrap());
        assert_eq!(result.hour(), 12);
    }

    #[test]
    fn convert_floating_unchanged() {
        use chrono::NaiveDate;
        let dt = NaiveDate::from_ymd_opt(2026, 7, 15).unwrap().and_hms_opt(10, 0, 0).unwrap();
        let result = convert_event_to_tz(dt, None, "Europe/Paris".parse::<Tz>().unwrap());
        assert_eq!(result, dt); // unchanged
    }

    #[test]
    fn convert_invalid_tz_unchanged() {
        use chrono::NaiveDate;
        let dt = NaiveDate::from_ymd_opt(2026, 7, 15).unwrap().and_hms_opt(10, 0, 0).unwrap();
        let result = convert_event_to_tz(dt, Some("Invalid/Zone"), "Europe/Paris".parse::<Tz>().unwrap());
        assert_eq!(result, dt); // unchanged
    }

    #[test]
    fn convert_winter_time() {
        use chrono::NaiveDate;
        // 10:00 in New York (EST, UTC-5) = 16:00 in Paris (CET, UTC+1) in winter
        let dt = NaiveDate::from_ymd_opt(2026, 1, 15).unwrap().and_hms_opt(10, 0, 0).unwrap();
        let result = convert_event_to_tz(dt, Some("America/New_York"), "Europe/Paris".parse::<Tz>().unwrap());
        assert_eq!(result.hour(), 16);
    }
}
