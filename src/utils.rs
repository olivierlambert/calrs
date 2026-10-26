use std::io::{self, Write};

use chrono::{NaiveDate, NaiveDateTime};
use chrono_tz::Tz;

/// Parse an iCal datetime string into a `NaiveDateTime`.
///
/// Accepts the four shapes calrs sees in the wild:
/// - compact `YYYYMMDDTHHMMSS`
/// - ISO `YYYY-MM-DDTHH:MM:SS`
/// - date-only `YYYYMMDD` / `YYYY-MM-DD` (returns 00:00:00)
///
/// A trailing `Z` (UTC marker) is stripped before parsing — the caller is
/// expected to carry the timezone separately (e.g. via the `events.timezone`
/// column populated from `extract_vevent_tzid`). Keeping a single helper for
/// every busy-time path means the next Exchange format quirk is fixed in one
/// spot.
pub fn parse_ical_datetime(s: &str) -> Option<NaiveDateTime> {
    let s = s.strip_suffix('Z').unwrap_or(s);
    NaiveDateTime::parse_from_str(s, "%Y%m%dT%H%M%S")
        .ok()
        .or_else(|| NaiveDateTime::parse_from_str(s, "%Y%m%dT%H%M%S%.f").ok())
        .or_else(|| NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S").ok())
        .or_else(|| NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S%.f").ok())
        .or_else(|| {
            NaiveDate::parse_from_str(s, "%Y%m%d")
                .ok()
                .and_then(|d| d.and_hms_opt(0, 0, 0))
        })
        .or_else(|| {
            NaiveDate::parse_from_str(s, "%Y-%m-%d")
                .ok()
                .and_then(|d| d.and_hms_opt(0, 0, 0))
        })
}

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

/// Undo RFC 5545 §3.1 line folding: a CRLF (or a bare LF, which some
/// servers send) followed by one space or tab joins the next line to the
/// previous one. Servers fold long lines, and so does calrs since #267, so
/// every line-based reader goes through this before looking at fields.
pub fn unfold_ical(ical: &str) -> std::borrow::Cow<'_, str> {
    if !ical.contains("\n ") && !ical.contains("\n\t") {
        return std::borrow::Cow::Borrowed(ical);
    }
    std::borrow::Cow::Owned(
        ical.replace("\r\n ", "")
            .replace("\r\n\t", "")
            .replace("\n ", "")
            .replace("\n\t", ""),
    )
}

/// One iCal content line, `NAME;PARAM=VALUE;...:VALUE`, split per RFC 5545
/// §3.1. A parameter value may be quoted, and a quoted value may contain `:`
/// and `;` (Outlook writes `TZID="(UTC-05:00) Eastern Time"`), so the line
/// cannot be cut at the first `:`. Parameter values come back unquoted.
pub struct ContentLine<'a> {
    pub name: &'a str,
    params: Vec<(&'a str, &'a str)>,
    pub value: &'a str,
}

impl ContentLine<'_> {
    /// The value of a parameter, matched case-insensitively by name.
    pub fn param(&self, name: &str) -> Option<&str> {
        self.params
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, v)| *v)
    }
}

/// Split one unfolded content line. `None` when the line has no value
/// separator outside quotes (malformed, or an unterminated quote).
pub fn parse_content_line(line: &str) -> Option<ContentLine<'_>> {
    let name_end = line.find([';', ':'])?;
    let name = line[..name_end].trim();
    let mut params = Vec::new();
    let mut pos = name_end;
    while line.as_bytes()[pos] == b';' {
        let seg_start = pos + 1;
        let mut in_quotes = false;
        let mut seg_end = None;
        for (i, c) in line[seg_start..].char_indices() {
            match c {
                '"' => in_quotes = !in_quotes,
                ';' | ':' if !in_quotes => {
                    seg_end = Some(seg_start + i);
                    break;
                }
                _ => {}
            }
        }
        let seg_end = seg_end?;
        let segment = &line[seg_start..seg_end];
        if let Some((pname, pvalue)) = segment.split_once('=') {
            let pvalue = pvalue.trim();
            let pvalue = pvalue
                .strip_prefix('"')
                .and_then(|v| v.strip_suffix('"'))
                .unwrap_or(pvalue);
            params.push((pname.trim(), pvalue));
        }
        pos = seg_end;
    }
    Some(ContentLine {
        name,
        params,
        value: &line[pos + 1..],
    })
}

/// Extract a field value from a single VEVENT block.
pub fn extract_vevent_field(vevent: &str, field: &str) -> Option<String> {
    for line in unfold_ical(vevent).lines() {
        let Some(parsed) = parse_content_line(line) else {
            continue;
        };
        if parsed.name.eq_ignore_ascii_case(field) {
            let value = parsed.value.trim();
            if !value.is_empty() {
                return Some(value.to_string());
            }
        }
    }
    None
}

/// Extract the TZID from a DTSTART or DTEND line in a VEVENT block.
///
/// - `DTSTART;TZID=Europe/Paris:20260310T100000` → `Some("Europe/Paris")`
/// - `DTSTART;TZID="America/New_York":20260924T140000` → `Some("America/New_York")`
/// - `DTSTART;TZID=Eastern Standard Time:...` → `Some("America/New_York")` (Windows name)
/// - `DTSTART:20260310T100000Z` → `Some("UTC")`
/// - `DTSTART:20260310T100000` (no TZID, no Z) → `None` (floating/local)
/// - `DTSTART;VALUE=DATE:20260310` → `None` (all-day)
pub fn extract_vevent_tzid(vevent: &str, field: &str) -> Option<String> {
    for line in unfold_ical(vevent).lines() {
        let Some(parsed) = parse_content_line(line) else {
            continue;
        };
        if !parsed.name.eq_ignore_ascii_case(field) {
            continue;
        }

        // VALUE=DATE (all-day) carries no timezone. Matched whole, so the
        // explicit VALUE=DATE-TIME some clients write stays timed.
        if parsed
            .param("VALUE")
            .is_some_and(|v| v.trim().eq_ignore_ascii_case("DATE"))
        {
            return None;
        }

        if let Some(tz) = parsed.param("TZID").map(str::trim) {
            if !tz.is_empty() {
                return Some(normalize_tzid(tz));
            }
        }

        if parsed.value.trim().ends_with('Z') {
            return Some("UTC".to_string());
        }

        // No TZID, no Z → floating
        return None;
    }
    None
}

/// Store a TZID as the IANA id it stands for when its name resolves
/// (Windows name, vendor prefix, display name), and unchanged otherwise.
fn normalize_tzid(tz: &str) -> String {
    match crate::tzid::resolve(tz) {
        Some(z) => z.name().to_string(),
        None => tz.to_string(),
    }
}

/// Convert a NaiveDateTime from the event's timezone to the target timezone.
///
/// - If `event_tz` is `Some` and resolves (see `crate::tzid`) → convert
/// - If `None` (floating) → return as-is (backward-compatible)
/// - If the name is unknown → return as-is (graceful degradation)
pub fn convert_event_to_tz(
    dt: NaiveDateTime,
    event_tz: Option<&str>,
    target_tz: Tz,
) -> NaiveDateTime {
    let etz: Tz = match event_tz.and_then(crate::tzid::resolve_or_offset) {
        Some(tz) => tz,
        None => return dt,
    };

    // Convert: event's local time → absolute instant → target TZ local time
    use chrono::TimeZone;
    match etz.from_local_datetime(&dt).earliest() {
        Some(zoned) => zoned.with_timezone(&target_tz).naive_local(),
        None => dt, // impossible time during DST transition
    }
}

/// Render a Markdown string to safe inline HTML.
///
/// Only allows inline elements: links, bold, italic, strikethrough, inline code.
/// Block-level elements (headings, lists, images, code blocks) are stripped.
/// All HTML tags in the input are escaped by pulldown-cmark.
/// Links get `target="_blank"` and `rel="noopener noreferrer"` for safety.
pub fn render_inline_markdown(text: &str) -> String {
    use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};

    let parser = Parser::new_ext(text, Options::ENABLE_STRIKETHROUGH);

    // Filter to inline-only elements
    let filtered = parser.filter(|event| {
        !matches!(
            event,
            Event::Start(
                Tag::Heading { .. }
                    | Tag::BlockQuote(_)
                    | Tag::CodeBlock(_)
                    | Tag::Image { .. }
                    | Tag::List(_)
                    | Tag::Item
                    | Tag::Table(_)
                    | Tag::TableHead
                    | Tag::TableRow
                    | Tag::TableCell
                    | Tag::HtmlBlock
            ) | Event::End(
                TagEnd::Heading(_)
                    | TagEnd::BlockQuote(_)
                    | TagEnd::CodeBlock
                    | TagEnd::Image
                    | TagEnd::List(_)
                    | TagEnd::Item
                    | TagEnd::Table
                    | TagEnd::TableHead
                    | TagEnd::TableRow
                    | TagEnd::TableCell
                    | TagEnd::HtmlBlock
            ) | Event::Html(_)
                | Event::InlineHtml(_)
        )
    });

    let mut html = String::new();
    pulldown_cmark::html::push_html(&mut html, filtered);

    // Add target="_blank" and rel="noopener noreferrer" to links
    html = html.replace(
        "<a href=",
        "<a target=\"_blank\" rel=\"noopener noreferrer\" href=",
    );

    // Strip wrapping <p> tags to keep it inline
    let trimmed = html.trim();
    if trimmed.starts_with("<p>") && trimmed.ends_with("</p>") {
        // Check if there's only one <p> block
        let inner = &trimmed[3..trimmed.len() - 4];
        if !inner.contains("<p>") {
            return inner.to_string();
        }
    }

    // Multiple paragraphs: replace </p><p> with <br> for compact display
    html.trim()
        .replace("</p>\n<p>", "<br>")
        .trim_start_matches("<p>")
        .trim_end_matches("</p>")
        .to_string()
}

pub fn prompt(label: &str) -> String {
    print!("{}: ", label);
    io::stdout().flush().unwrap();
    let mut input = String::new();
    io::stdin().read_line(&mut input).unwrap();
    input.trim().to_string()
}

pub fn prompt_password(label: &str) -> String {
    rpassword::prompt_password(format!("{}: ", label)).unwrap_or_default()
}

#[derive(Debug, PartialEq, Eq)]
struct IcalDuration {
    days: u64,
    seconds: i64,
}

/// Return explicit DTEND, or derive it from DTSTART + DURATION.
///
/// Supports:
/// P1W, P2D, P1DT2H30M, PT7H, PT30M, PT45S.
///
/// Calendar days are applied before elapsed hours/minutes/seconds, in
/// `zone`, the event's resolved timezone (`None` for floating and UTC
/// times). Existing DTEND values are returned unchanged. An all-day event
/// with neither lasts one day (RFC 5545 §3.6.1).
pub fn extract_vevent_end(vevent: &str, zone: Option<Tz>) -> Option<String> {
    if let Some(end) = extract_vevent_field(vevent, "DTEND") {
        return Some(end);
    }

    let start = extract_vevent_field(vevent, "DTSTART")?;
    let start_dt = parse_ical_datetime(&start)?;
    let timed = start.contains('T');
    let iso = start.contains('-');

    let raw_duration = match extract_vevent_field(vevent, "DURATION") {
        Some(raw) => raw,
        None if !timed => "P1D".to_string(),
        None => return None,
    };
    let duration = parse_ical_duration(&raw_duration)?;

    // DATE-valued events only permit day/week durations.
    // Checking the syntax also rejects P1DT0H on an all-day event.
    if !timed {
        if raw_duration.contains('T') {
            return None;
        }

        let end_date = start_dt
            .date()
            .checked_add_days(chrono::Days::new(duration.days))?;

        let format = if iso { "%Y-%m-%d" } else { "%Y%m%d" };
        return Some(end_date.format(format).to_string());
    }

    // Nominal calendar-day addition preserves the local wall time.
    let after_days = start_dt.checked_add_days(chrono::Days::new(duration.days))?;

    let elapsed = chrono::Duration::try_seconds(duration.seconds)?;

    // A `Z` start is UTC whatever TZID sits beside it.
    let zone = zone.filter(|_| !start.ends_with('Z'));
    let end_dt = match zone {
        Some(tz) => {
            use chrono::{Offset, TimeZone};

            // Earliest selects the first occurrence of an ambiguous local
            // time. A time skipped by a spring-forward change takes the
            // offset from before the change (RFC 5545 §3.3.5).
            let zoned = match tz.from_local_datetime(&after_days).earliest() {
                Some(zoned) => zoned,
                None => {
                    let before = tz
                        .from_local_datetime(&(after_days - chrono::Duration::hours(3)))
                        .earliest()?
                        .offset()
                        .fix();
                    tz.from_utc_datetime(&(after_days - before))
                }
            };

            // Elapsed-time addition respects any DST transition.
            zoned.checked_add_signed(elapsed)?.naive_local()
        }
        None => {
            // Floating time: no timezone conversion is possible.
            after_days.checked_add_signed(elapsed)?
        }
    };

    let format = if iso {
        "%Y-%m-%dT%H:%M:%S"
    } else {
        "%Y%m%dT%H%M%S"
    };

    let mut end = end_dt.format(format).to_string();
    if start.ends_with('Z') {
        end.push('Z');
    }

    Some(end)
}

/// Parse a positive RFC 5545 duration.
///
/// Weeks must appear alone. Days may be followed by a time component.
/// Negative, zero, malformed, and overflowing durations are rejected.
fn parse_ical_duration(value: &str) -> Option<IcalDuration> {
    let value = value.strip_prefix('+').unwrap_or(value);
    let body = value.strip_prefix('P')?;

    // Week form cannot be combined with days or time.
    if let Some(weeks) = body.strip_suffix('W') {
        let days = parse_duration_number(weeks)?.checked_mul(7)?;
        return (days > 0).then_some(IcalDuration { days, seconds: 0 });
    }

    let (date_part, time_part) = match body.split_once('T') {
        Some((date, time)) => (date, Some(time)),
        None => (body, None),
    };

    let days = if date_part.is_empty() {
        0
    } else {
        parse_duration_number(date_part.strip_suffix('D')?)?
    };

    let seconds = match time_part {
        Some(time) => parse_duration_time(time)?,
        None => 0,
    };

    if days == 0 && seconds == 0 {
        return None;
    }

    Some(IcalDuration { days, seconds })
}

fn parse_duration_number(value: &str) -> Option<u64> {
    if value.is_empty() || !value.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }

    value.parse().ok()
}

/// Parse H/M/S components, requiring strictly increasing unit order.
fn parse_duration_time(value: &str) -> Option<i64> {
    let mut total = 0_i64;
    let mut number = 0_i64;
    let mut has_digits = false;
    let mut previous_rank = 0;

    for ch in value.chars() {
        if ch.is_ascii_digit() {
            number = number
                .checked_mul(10)?
                .checked_add(i64::from(ch.to_digit(10)?))?;
            has_digits = true;
            continue;
        }

        let (rank, multiplier) = match ch {
            'H' => (1, 3600_i64),
            'M' => (2, 60_i64),
            'S' => (3, 1_i64),
            _ => return None,
        };

        if !has_digits || rank <= previous_rank {
            return None;
        }

        total = total.checked_add(number.checked_mul(multiplier)?)?;
        previous_rank = rank;
        number = 0;
        has_digits = false;
    }

    if previous_rank == 0 || has_digits {
        return None;
    }

    Some(total)
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
        assert_eq!(
            extract_vevent_field(vevent, "UID"),
            Some("test-uid-123".to_string())
        );
        assert_eq!(
            extract_vevent_field(vevent, "SUMMARY"),
            Some("Team meeting".to_string())
        );
    }

    #[test]
    fn extract_field_with_params() {
        // DTSTART has timezone parameters before the colon
        let vevent = "BEGIN:VEVENT\nDTSTART;TZID=Europe/Paris:20260310T100000\nEND:VEVENT";
        assert_eq!(
            extract_vevent_field(vevent, "DTSTART"),
            Some("20260310T100000".to_string())
        );
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
        assert_eq!(
            extract_vevent_field(vevent, "DTSTART"),
            Some("20260310T100000".to_string())
        );
    }

    // --- extract_vevent_tzid ---

    #[test]
    fn tzid_with_explicit_timezone() {
        let vevent = "BEGIN:VEVENT\nDTSTART;TZID=Europe/Paris:20260310T100000\nEND:VEVENT";
        assert_eq!(
            extract_vevent_tzid(vevent, "DTSTART"),
            Some("Europe/Paris".to_string())
        );
    }

    #[test]
    fn tzid_utc_suffix() {
        let vevent = "BEGIN:VEVENT\nDTSTART:20260310T100000Z\nEND:VEVENT";
        assert_eq!(
            extract_vevent_tzid(vevent, "DTSTART"),
            Some("UTC".to_string())
        );
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
        assert_eq!(
            extract_vevent_tzid(vevent, "DTSTART"),
            Some("America/New_York".to_string())
        );
        assert_eq!(
            extract_vevent_tzid(vevent, "DTEND"),
            Some("America/New_York".to_string())
        );
    }

    #[test]
    fn tzid_quoted_value() {
        // RFC 5545 allows quoted parameter values; Outlook and DavMail always quote TZID.
        let vevent = "BEGIN:VEVENT\nDTSTART;TZID=\"America/New_York\":20260924T140000\nEND:VEVENT";
        assert_eq!(
            extract_vevent_tzid(vevent, "DTSTART"),
            Some("America/New_York".to_string())
        );
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

    /// #283: an explicit VALUE=DATE-TIME is a timed event, not an all-day one.
    #[test]
    fn tzid_value_date_time_is_not_all_day() {
        let vevent = "BEGIN:VEVENT\nDTSTART;VALUE=DATE-TIME;TZID=America/New_York:20260924T140000\nEND:VEVENT";
        assert_eq!(
            extract_vevent_tzid(vevent, "DTSTART"),
            Some("America/New_York".to_string())
        );
        let utc = "BEGIN:VEVENT\nDTSTART;VALUE=DATE-TIME:20260924T140000Z\nEND:VEVENT";
        assert_eq!(extract_vevent_tzid(utc, "DTSTART"), Some("UTC".to_string()));
    }

    /// #283: a quoted TZID holding ':' and ';' does not cut the line short.
    #[test]
    fn quoted_param_with_colon_and_semicolon() {
        let vevent =
            "BEGIN:VEVENT\nDTSTART;TZID=\"(UTC-05:00) Eastern; US\":20260924T140000\nEND:VEVENT";
        assert_eq!(
            extract_vevent_tzid(vevent, "DTSTART"),
            Some("(UTC-05:00) Eastern; US".to_string())
        );
        assert_eq!(
            extract_vevent_field(vevent, "DTSTART"),
            Some("20260924T140000".to_string())
        );
    }

    /// #283: Outlook's Windows zone names are stored as the IANA id.
    #[test]
    fn tzid_windows_name_maps_to_iana() {
        let vevent = "BEGIN:VEVENT\nDTSTART;TZID=Eastern Standard Time:20260924T140000\nEND:VEVENT";
        assert_eq!(
            extract_vevent_tzid(vevent, "DTSTART"),
            Some("America/New_York".to_string())
        );
        let quoted =
            "BEGIN:VEVENT\nDTSTART;TZID=\"W. Europe Standard Time\":20260924T140000\nEND:VEVENT";
        assert_eq!(
            extract_vevent_tzid(quoted, "DTSTART"),
            Some("Europe/Berlin".to_string())
        );
    }

    /// An unknown name is kept as written, so it still degrades to floating.
    #[test]
    fn tzid_unknown_name_kept() {
        let vevent = "BEGIN:VEVENT\nDTSTART;TZID=Customized Time Zone:20260924T140000\nEND:VEVENT";
        assert_eq!(
            extract_vevent_tzid(vevent, "DTSTART"),
            Some("Customized Time Zone".to_string())
        );
    }

    // --- parse_content_line ---

    #[test]
    fn content_line_splits_name_params_value() {
        let line =
            parse_content_line("DTSTART;VALUE=DATE-TIME;tzid=\"A:B\":20260924T140000").unwrap();
        assert_eq!(line.name, "DTSTART");
        assert_eq!(line.param("VALUE"), Some("DATE-TIME"));
        assert_eq!(line.param("TZID"), Some("A:B"));
        assert_eq!(line.value, "20260924T140000");
    }

    #[test]
    fn content_line_value_keeps_its_separators() {
        let line = parse_content_line("RRULE:FREQ=WEEKLY;UNTIL=20261231T000000Z").unwrap();
        assert_eq!(line.name, "RRULE");
        assert_eq!(line.param("FREQ"), None);
        assert_eq!(line.value, "FREQ=WEEKLY;UNTIL=20261231T000000Z");
    }

    #[test]
    fn content_line_malformed() {
        assert!(parse_content_line("END").is_none());
        assert!(parse_content_line("DTSTART;TZID=\"unterminated:2026").is_none());
    }

    // --- convert_event_to_tz ---

    #[test]
    fn convert_ny_to_paris() {
        use chrono::NaiveDate;
        // 10:00 in New York (EDT, UTC-4) = 16:00 in Paris (CEST, UTC+2) in summer
        let dt = NaiveDate::from_ymd_opt(2026, 7, 15)
            .unwrap()
            .and_hms_opt(10, 0, 0)
            .unwrap();
        let result = convert_event_to_tz(
            dt,
            Some("America/New_York"),
            "Europe/Paris".parse::<Tz>().unwrap(),
        );
        assert_eq!(result.hour(), 16);
    }

    #[test]
    fn convert_utc_to_paris() {
        use chrono::NaiveDate;
        // 10:00 UTC = 12:00 Paris (CEST, UTC+2) in summer
        let dt = NaiveDate::from_ymd_opt(2026, 7, 15)
            .unwrap()
            .and_hms_opt(10, 0, 0)
            .unwrap();
        let result = convert_event_to_tz(dt, Some("UTC"), "Europe/Paris".parse::<Tz>().unwrap());
        assert_eq!(result.hour(), 12);
    }

    #[test]
    fn convert_floating_unchanged() {
        use chrono::NaiveDate;
        let dt = NaiveDate::from_ymd_opt(2026, 7, 15)
            .unwrap()
            .and_hms_opt(10, 0, 0)
            .unwrap();
        let result = convert_event_to_tz(dt, None, "Europe/Paris".parse::<Tz>().unwrap());
        assert_eq!(result, dt); // unchanged
    }

    #[test]
    fn convert_invalid_tz_unchanged() {
        use chrono::NaiveDate;
        let dt = NaiveDate::from_ymd_opt(2026, 7, 15)
            .unwrap()
            .and_hms_opt(10, 0, 0)
            .unwrap();
        let result = convert_event_to_tz(
            dt,
            Some("Invalid/Zone"),
            "Europe/Paris".parse::<Tz>().unwrap(),
        );
        assert_eq!(result, dt); // unchanged
    }

    // --- tzdata (vendor/chrono-tz) ---

    /// tzdata 2026a/b: British Columbia stays on -07 from 2026-11-01. Pins
    /// the vendored data; chrono-tz 0.8.6's own 2024a would give -08.
    #[test]
    fn tzdata_vancouver_permanent_minus_seven() {
        use chrono::{NaiveDate, Offset, TimeZone};
        let at = |y, m, d| {
            let utc = NaiveDate::from_ymd_opt(y, m, d)
                .unwrap()
                .and_hms_opt(12, 0, 0)
                .unwrap();
            chrono_tz::America::Vancouver
                .offset_from_utc_datetime(&utc)
                .fix()
                .local_minus_utc()
                / 3600
        };
        assert_eq!(at(2026, 1, 15), -8);
        assert_eq!(at(2026, 7, 15), -7);
        assert_eq!(at(2026, 11, 15), -7);
        assert_eq!(at(2027, 1, 15), -7);
        // Neighbours keep their DST.
        let la = |m| {
            let utc = NaiveDate::from_ymd_opt(2026, m, 15)
                .unwrap()
                .and_hms_opt(12, 0, 0)
                .unwrap();
            chrono_tz::America::Los_Angeles
                .offset_from_utc_datetime(&utc)
                .fix()
                .local_minus_utc()
                / 3600
        };
        assert_eq!(la(11), -8);
    }

    /// The vendored data is in rearguard form: numeric abbreviations are
    /// spelled out, not the main form's literal `%z`.
    #[test]
    fn tzdata_numeric_abbreviations_are_spelled_out() {
        use chrono::{NaiveDate, TimeZone};
        use chrono_tz::OffsetName;
        let utc = NaiveDate::from_ymd_opt(2026, 7, 15)
            .unwrap()
            .and_hms_opt(12, 0, 0)
            .unwrap();
        let dubai = chrono_tz::Asia::Dubai.from_utc_datetime(&utc);
        assert_eq!(dubai.offset().abbreviation(), "+04");
    }

    /// #283: an event stored with a Windows name before this fix still converts.
    #[test]
    fn convert_windows_name() {
        use chrono::NaiveDate;
        let dt = NaiveDate::from_ymd_opt(2026, 7, 15)
            .unwrap()
            .and_hms_opt(10, 0, 0)
            .unwrap();
        let result = convert_event_to_tz(
            dt,
            Some("Eastern Standard Time"),
            "Europe/Paris".parse::<Tz>().unwrap(),
        );
        assert_eq!(result.hour(), 16);
    }

    // --- render_inline_markdown ---

    #[test]
    fn bio_plain_text() {
        let result = render_inline_markdown("Hello world");
        assert_eq!(result, "Hello world");
    }

    #[test]
    fn bio_link() {
        let result = render_inline_markdown("[My site](https://example.com)");
        assert!(result.contains("href=\"https://example.com\""));
        assert!(result.contains("target=\"_blank\""));
        assert!(result.contains("rel=\"noopener noreferrer\""));
        assert!(result.contains("My site"));
    }

    #[test]
    fn bio_bold_italic() {
        let result = render_inline_markdown("**bold** and *italic*");
        assert!(result.contains("<strong>bold</strong>"));
        assert!(result.contains("<em>italic</em>"));
    }

    #[test]
    fn bio_strips_headings() {
        let result = render_inline_markdown("# Heading\nSome text");
        assert!(!result.contains("<h1>"));
        assert!(result.contains("Some text"));
    }

    #[test]
    fn bio_strips_images() {
        let result = render_inline_markdown("![alt](https://evil.com/img.png)");
        assert!(!result.contains("<img"));
        assert!(!result.contains("evil.com"));
    }

    #[test]
    fn bio_strips_html_tags() {
        let result = render_inline_markdown("<script>alert('xss')</script>");
        assert!(!result.contains("<script>"));
    }

    #[test]
    fn bio_inline_code() {
        let result = render_inline_markdown("Use `cargo build`");
        assert!(result.contains("<code>cargo build</code>"));
    }

    #[test]
    fn bio_multiple_paragraphs() {
        let result = render_inline_markdown("Line 1\n\nLine 2");
        assert!(result.contains("Line 1"));
        assert!(result.contains("Line 2"));
        assert!(result.contains("<br>"));
    }

    // --- parse_ical_datetime ---

    fn pdt(y: i32, m: u32, d: u32, h: u32, mi: u32) -> NaiveDateTime {
        chrono::NaiveDate::from_ymd_opt(y, m, d)
            .unwrap()
            .and_hms_opt(h, mi, 0)
            .unwrap()
    }

    #[test]
    fn parse_ical_compact_format() {
        assert_eq!(
            parse_ical_datetime("20260310T140000"),
            Some(pdt(2026, 3, 10, 14, 0))
        );
    }

    /// EWS serializes instants with fractional seconds (`20230207T170000.000Z`
    /// on the wire, which EWS-side formatting keeps). `validate_provider_snapshot`
    /// treats an unparseable datetime as a malformed snapshot and aborts the
    /// sync, so the shared parser has to accept the fraction.
    #[test]
    fn parse_ical_accepts_fractional_seconds() {
        assert_eq!(
            parse_ical_datetime("20230207T170000.000Z"),
            Some(pdt(2023, 2, 7, 17, 0))
        );
        assert_eq!(
            parse_ical_datetime("2023-02-07T17:00:00.123Z"),
            Some(
                chrono::NaiveDate::from_ymd_opt(2023, 2, 7)
                    .unwrap()
                    .and_hms_nano_opt(17, 0, 0, 123_000_000)
                    .unwrap()
            )
        );
    }

    #[test]
    fn parse_ical_iso_format() {
        assert_eq!(
            parse_ical_datetime("2026-03-10T14:00:00"),
            Some(pdt(2026, 3, 10, 14, 0))
        );
    }

    /// EWS UTC events arrive as `YYYYMMDDTHHMMSSZ`. Before consolidation each
    /// busy-time path had its own parser and the `Z` suffix made chrono reject
    /// the value, silently dropping the event. Pair with the DST round-trip
    /// tests below to guard the parse + tz-conversion combo.
    #[test]
    fn parse_ical_compact_with_z_suffix() {
        assert_eq!(
            parse_ical_datetime("20260310T140000Z"),
            Some(pdt(2026, 3, 10, 14, 0))
        );
    }

    #[test]
    fn parse_ical_iso_with_z_suffix() {
        assert_eq!(
            parse_ical_datetime("2026-06-06T08:10:00Z"),
            Some(pdt(2026, 6, 6, 8, 10))
        );
    }

    #[test]
    fn parse_ical_allday_compact() {
        assert_eq!(
            parse_ical_datetime("20260310"),
            Some(pdt(2026, 3, 10, 0, 0))
        );
    }

    #[test]
    fn parse_ical_allday_dashed() {
        assert_eq!(
            parse_ical_datetime("2026-03-10"),
            Some(pdt(2026, 3, 10, 0, 0))
        );
    }

    #[test]
    fn parse_ical_invalid() {
        assert_eq!(parse_ical_datetime("not-a-date"), None);
        assert_eq!(parse_ical_datetime(""), None);
        assert_eq!(parse_ical_datetime("2026"), None);
    }

    /// Summer round-trip: 08:10 UTC → 10:10 Europe/Paris (CEST, UTC+2).
    /// Together with the winter variant below this anchors the end-to-end
    /// path from Z-suffixed EWS string to host-local time.
    #[test]
    fn ews_utc_event_converts_to_paris_summer() {
        let host_tz: Tz = "Europe/Paris".parse().unwrap();
        let dt = parse_ical_datetime("20260606T081000Z").unwrap();
        let converted = convert_event_to_tz(dt, Some("UTC"), host_tz);
        assert_eq!(converted, pdt(2026, 6, 6, 10, 10));
    }

    /// Winter round-trip: 08:10 UTC → 09:10 Europe/Paris (CET, UTC+1).
    #[test]
    fn ews_utc_event_converts_to_paris_winter() {
        let host_tz: Tz = "Europe/Paris".parse().unwrap();
        let dt = parse_ical_datetime("20260112T081000Z").unwrap();
        let converted = convert_event_to_tz(dt, Some("UTC"), host_tz);
        assert_eq!(converted, pdt(2026, 1, 12, 9, 10));
    }

    #[test]
    fn convert_winter_time() {
        use chrono::NaiveDate;
        // 10:00 in New York (EST, UTC-5) = 16:00 in Paris (CET, UTC+1) in winter
        let dt = NaiveDate::from_ymd_opt(2026, 1, 15)
            .unwrap()
            .and_hms_opt(10, 0, 0)
            .unwrap();
        let result = convert_event_to_tz(
            dt,
            Some("America/New_York"),
            "Europe/Paris".parse::<Tz>().unwrap(),
        );
        assert_eq!(result.hour(), 16);
    }

    /// #267: calrs folds what it writes, and servers fold what they serve.
    #[test]
    fn extract_field_reads_folded_lines() {
        let vevent = "BEGIN:VEVENT\r\nUID:abc\r\nSUMMARY:A long\r\n  title\r\nDESCRIPTION:one\n\ttwo\r\nDTSTART;TZID=Europe/\r\n Paris:20260310T100000\r\nEND:VEVENT";
        assert_eq!(
            extract_vevent_field(vevent, "SUMMARY").as_deref(),
            Some("A long title")
        );
        assert_eq!(
            extract_vevent_field(vevent, "DESCRIPTION").as_deref(),
            Some("onetwo")
        );
        assert_eq!(
            extract_vevent_tzid(vevent, "DTSTART").as_deref(),
            Some("Europe/Paris")
        );
        assert!(matches!(
            unfold_ical("UID:abc\r\nEND:VEVENT"),
            std::borrow::Cow::Borrowed(_)
        ));
    }
}

#[cfg(test)]
mod duration_tests {
    use super::*;

    /// The zone the sync passes: the DTSTART TZID, resolved.
    fn end_of(event: &str) -> Option<String> {
        let zone = extract_vevent_tzid(event, "DTSTART").and_then(|t| crate::tzid::resolve(&t));
        extract_vevent_end(event, zone)
    }

    #[test]
    fn derives_end_from_duration() {
        let cases = [
            // Fastmail reproduction.
            (
                "DTSTART;TZID=Europe/Paris:20261027T161500",
                "PT7H",
                "20261027T231500",
            ),
            // Week duration.
            (
                "DTSTART;TZID=Europe/Paris:20261027T161500",
                "P1W",
                "20261103T161500",
            ),
            // All-day exclusive end.
            ("DTSTART;VALUE=DATE:20261027", "P2D", "20261029"),
            // Mixed duration crossing midnight, preserving UTC.
            ("DTSTART:20261027T230000Z", "P1DT2H30M", "20261029T013000Z"),
        ];

        for (start, duration, expected) in cases {
            let event = format!("BEGIN:VEVENT\n{start}\nDURATION:{duration}\nEND:VEVENT");

            assert_eq!(
                end_of(&event),
                Some(expected.to_string()),
                "{start}, {duration}",
            );
        }
    }

    #[test]
    fn distinguishes_calendar_days_from_elapsed_hours_across_dst() {
        for (duration, expected) in [("P1D", "20261025T120000"), ("PT24H", "20261025T110000")] {
            let event = format!(
                "BEGIN:VEVENT\n\
                 DTSTART;TZID=Europe/Paris:20261024T120000\n\
                 DURATION:{duration}\n\
                 END:VEVENT"
            );

            assert_eq!(end_of(&event), Some(expected.to_string()), "{duration}",);
        }
    }

    #[test]
    fn preserves_explicit_dtend() {
        let event = "\
BEGIN:VEVENT
DTSTART:20261027T161500Z
DTEND:20261027T171500Z
END:VEVENT";

        assert_eq!(end_of(event), Some("20261027T171500Z".to_string()),);
    }

    #[test]
    fn rejects_invalid_durations() {
        for duration in [
            "P",
            "PT",
            "P1DT",
            "-PT1H",
            "PT0S",
            "P1WT2H",
            "PT1M2H",
            "PT1H2H",
            "P18446744073709551615W",
        ] {
            assert_eq!(parse_ical_duration(duration), None, "{duration}");
        }

        // All-day events must use day/week durations.
        let event = "\
BEGIN:VEVENT
DTSTART;VALUE=DATE:20261027
DURATION:PT24H
END:VEVENT";

        assert_eq!(end_of(event), None);
    }

    #[test]
    fn duration_follows_a_windows_tzid() {
        // P1D keeps the wall time; PT24H crosses the Paris fall-back change.
        for (duration, expected) in [("P1D", "20261025T120000"), ("PT24H", "20261025T110000")] {
            let event = format!(
                "BEGIN:VEVENT\n\
                 DTSTART;TZID=\"W. Europe Standard Time\":20261024T120000\n\
                 DURATION:{duration}\n\
                 END:VEVENT"
            );
            assert_eq!(end_of(&event), Some(expected.to_string()), "{duration}");
        }
    }

    #[test]
    fn duration_from_a_time_skipped_by_spring_forward() {
        // 02:30 does not exist in Paris on 2026-03-29: it is read with the
        // offset from before the change (+01:00), so 01:30 UTC, and an hour
        // later is 02:30 UTC, 04:30 local.
        let event = "\
BEGIN:VEVENT
DTSTART;TZID=Europe/Paris:20260329T023000
DURATION:PT1H
END:VEVENT";
        assert_eq!(end_of(event), Some("20260329T043000".to_string()));
    }

    #[test]
    fn all_day_without_end_or_duration_lasts_one_day() {
        let event = "BEGIN:VEVENT\nDTSTART;VALUE=DATE:20261231\nEND:VEVENT";
        assert_eq!(end_of(event), Some("20270101".to_string()));
    }

    #[test]
    fn utc_start_ignores_a_tzid_beside_it() {
        // PT24H from a UTC start is 24 elapsed hours, even if a Paris TZID
        // (which crosses its fall-back change here) is also written.
        let event = "\
BEGIN:VEVENT
DTSTART;TZID=Europe/Paris:20261024T120000Z
DURATION:PT24H
END:VEVENT";
        assert_eq!(end_of(event), Some("20261025T120000Z".to_string()));
    }

    #[test]
    fn timed_without_end_or_duration_has_no_end() {
        let event = "BEGIN:VEVENT\nDTSTART:20261027T161500Z\nEND:VEVENT";
        assert_eq!(end_of(event), None);
    }
}
