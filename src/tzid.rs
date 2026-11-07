//! Turning the TZID an iCal event carries into a zone calrs can convert.
//!
//! Most clients write an IANA id. The rest, in the order they are tried:
//! - a Windows name, `Eastern Standard Time` (Outlook, Exchange);
//! - an IANA id behind a vendor prefix, `/mozilla.org/20050126_1/Europe/Paris`
//!   (older Thunderbird, Evolution, Citadel);
//! - a Windows display name naming a city, `(UTC+01:00) Amsterdam, Berlin`;
//! - at sync only, the event's own `VTIMEZONE`, matched against every IANA
//!   zone by its offsets (`Customized Time Zone`, `Eastern Time (US & Canada)`);
//! - last, the fixed offset a display name starts with, `(UTC-05:00)`, which is
//!   wrong across a DST change but closer than reading the time as host-local.

use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use chrono::{Datelike, Duration, NaiveDate, NaiveDateTime, Offset, TimeZone, Utc, Weekday};
use chrono_tz::{Tz, TZ_VARIANTS};

use crate::utils::{parse_content_line, parse_ical_datetime, unfold_ical};

/// Resolve a TZID by its name alone.
pub fn resolve(name: &str) -> Option<Tz> {
    let name = name.trim();
    name.parse::<Tz>()
        .ok()
        .or_else(|| crate::windows_zones::from_windows(name))
        .or_else(|| from_prefixed_path(name))
        .or_else(|| from_display_name(name))
}

/// [`resolve`], falling back to the fixed offset of a display name. Used
/// where the event's `VTIMEZONE` is no longer at hand (stored events).
pub fn resolve_or_offset(name: &str) -> Option<Tz> {
    resolve(name).or_else(|| fixed_offset(name))
}

/// The timezones of the VEVENTs of one resource (a CalDAV object, or a whole
/// ICS feed), whose `VTIMEZONE` blocks they refer to. The resource is
/// unfolded once, on the first TZID that needs it, and each (TZID, year)
/// match is remembered, so a feed of thousands of events with a custom TZID
/// is scanned once rather than once per event.
pub struct ResourceZones<'a> {
    resource: &'a str,
    unfolded: Option<Cow<'a, str>>,
    memo: HashMap<(String, i32), Option<Tz>>,
}

impl<'a> ResourceZones<'a> {
    pub fn new(resource: &'a str) -> Self {
        Self {
            resource,
            unfolded: None,
            memo: HashMap::new(),
        }
    }

    /// The timezone to store for a VEVENT's DTSTART. Returns the IANA id when
    /// the TZID resolves, the TZID unchanged when nothing does (read as
    /// floating time), and `None` for floating and all-day events.
    pub fn event_timezone(&mut self, vevent: &str) -> Option<String> {
        let tzid = crate::utils::extract_vevent_tzid(vevent, "DTSTART")?;
        if let Some(tz) = resolve(&tzid) {
            return Some(tz.name().to_string());
        }
        let year = crate::utils::extract_vevent_field(vevent, "DTSTART")
            .and_then(|v| parse_ical_datetime(&v))
            .map(|dt| dt.year())
            .unwrap_or_else(|| Utc::now().year());
        let key = (tzid, year);
        let matched = match self.memo.get(&key) {
            Some(hit) => *hit,
            None => {
                let resource = self.resource;
                let unfolded = self.unfolded.get_or_insert_with(|| unfold_ical(resource));
                let found = from_vtimezone(unfolded, &key.0, year);
                self.memo.insert(key.clone(), found);
                found
            }
        };
        matched
            .or_else(|| fixed_offset(&key.0))
            .map(|tz| tz.name().to_string())
            .or(Some(key.0))
    }
}

/// [`ResourceZones::event_timezone`] for a single VEVENT.
#[cfg(test)]
fn event_timezone(resource: &str, vevent: &str) -> Option<String> {
    ResourceZones::new(resource).event_timezone(vevent)
}

/// `/mozilla.org/20050126_1/Europe/Paris` → `Europe/Paris`: the longest
/// trailing path (up to three segments, `America/Argentina/Buenos_Aires`)
/// that is an IANA id.
fn from_prefixed_path(name: &str) -> Option<Tz> {
    if !name.contains('/') {
        return None;
    }
    let segments: Vec<&str> = name.split('/').filter(|s| !s.is_empty()).collect();
    (1..=segments.len().min(3))
        .rev()
        .find_map(|n| segments[segments.len() - n..].join("/").parse().ok())
}

/// `(UTC+01:00) Amsterdam, Berlin` → offset in seconds and the text after it.
fn display_offset(name: &str) -> Option<(i32, &str)> {
    let (inner, rest) = name.trim().strip_prefix('(')?.split_once(')')?;
    let inner = inner.trim();
    let offset = inner
        .strip_prefix("UTC")
        .or_else(|| inner.strip_prefix("GMT"))?
        .trim();
    if offset.is_empty() {
        return Some((0, rest.trim()));
    }
    let (sign, digits) = match offset.as_bytes()[0] {
        b'+' => (1, &offset[1..]),
        b'-' => (-1, &offset[1..]),
        _ => return None,
    };
    let (h, m) = digits.split_once(':').unwrap_or((digits, "0"));
    let secs = h.trim().parse::<i32>().ok()? * 3600 + m.trim().parse::<i32>().ok()? * 60;
    Some((sign * secs, rest.trim()))
}

/// The UTC offset of `tz` at a UTC instant, in seconds.
fn offset_at(tz: &Tz, utc: NaiveDateTime) -> i32 {
    tz.offset_from_utc_datetime(&utc).fix().local_minus_utc()
}

fn mid_month(year: i32, month: u32) -> NaiveDateTime {
    NaiveDate::from_ymd_opt(year, month, 15)
        .and_then(|d| d.and_hms_opt(12, 0, 0))
        .unwrap_or_default()
}

/// A display name whose city is an IANA zone with that standard offset:
/// `(UTC+01:00) Brussels, Copenhagen, Madrid, Paris` → `Europe/Brussels`.
fn from_display_name(name: &str) -> Option<Tz> {
    let (secs, cities) = display_offset(name)?;
    let year = Utc::now().year();
    cities.split(',').find_map(|city| {
        let city = city.trim().replace(' ', "_");
        if city.is_empty() {
            return None;
        }
        TZ_VARIANTS.iter().copied().find(|tz| {
            tz.name()
                .rsplit('/')
                .next()
                .is_some_and(|last| last.eq_ignore_ascii_case(&city))
                && (offset_at(tz, mid_month(year, 1)) == secs
                    || offset_at(tz, mid_month(year, 7)) == secs)
        })
    })
}

/// The fixed-offset zone of a display name, `(UTC-05:00) ...` → `Etc/GMT+5`.
/// `Etc` only has whole hours, so `(UTC+05:30)` gets nothing.
fn fixed_offset(name: &str) -> Option<Tz> {
    let (secs, _) = display_offset(name)?;
    if secs == 0 {
        return Some(Tz::UTC);
    }
    if secs % 3600 != 0 {
        return None;
    }
    // POSIX sign convention: Etc/GMT+5 is five hours *behind* UTC.
    format!("Etc/GMT{:+}", -secs / 3600).parse().ok()
}

/// One `STANDARD` or `DAYLIGHT` block of a `VTIMEZONE`.
#[derive(Default)]
struct Observance {
    dtstart: Option<NaiveDateTime>,
    offset_from: Option<i32>,
    offset_to: Option<i32>,
    rrule: Option<String>,
    rdates: Vec<NaiveDateTime>,
}

/// `+0100`, `-0500`, `+013000` → seconds.
fn parse_utc_offset(s: &str) -> Option<i32> {
    let s = s.trim();
    let (sign, digits) = match s.as_bytes().first()? {
        b'+' => (1, &s[1..]),
        b'-' => (-1, &s[1..]),
        _ => return None,
    };
    if digits.len() < 4 || !digits.is_ascii() {
        return None;
    }
    let h: i32 = digits[0..2].parse().ok()?;
    let m: i32 = digits[2..4].parse().ok()?;
    let sec: i32 = digits.get(4..6).map_or(Some(0), |x| x.parse().ok())?;
    Some(sign * (h * 3600 + m * 60 + sec))
}

fn parse_weekday(s: &str) -> Option<Weekday> {
    Some(match s {
        "MO" => Weekday::Mon,
        "TU" => Weekday::Tue,
        "WE" => Weekday::Wed,
        "TH" => Weekday::Thu,
        "FR" => Weekday::Fri,
        "SA" => Weekday::Sat,
        "SU" => Weekday::Sun,
        _ => return None,
    })
}

/// The `n`th `weekday` of a month, counting from the end when `n` is negative.
fn nth_weekday(year: i32, month: u32, weekday: Weekday, n: i32) -> Option<NaiveDate> {
    if n > 0 {
        return NaiveDate::from_weekday_of_month_opt(year, month, weekday, u8::try_from(n).ok()?);
    }
    let next = if month == 12 {
        NaiveDate::from_ymd_opt(year + 1, 1, 1)?
    } else {
        NaiveDate::from_ymd_opt(year, month + 1, 1)?
    };
    let last = next - Duration::days(1);
    let back = (7 + last.weekday().num_days_from_monday() - weekday.num_days_from_monday()) % 7;
    let date = last - Duration::days(i64::from(back) + 7 * i64::from(-n - 1));
    (date.month() == month).then_some(date)
}

/// The local date a yearly `VTIMEZONE` rule falls on in `year`.
fn yearly_rule_date(rrule: &str, dtstart: NaiveDateTime, year: i32) -> Option<NaiveDate> {
    let mut freq = None;
    let mut month = dtstart.month();
    let mut byday = None;
    let mut bymonthday: Vec<u32> = Vec::new();
    let mut until = None;
    for part in rrule.split(';') {
        let (k, v) = part.split_once('=')?;
        match k.trim().to_ascii_uppercase().as_str() {
            "FREQ" => freq = Some(v.trim().to_ascii_uppercase()),
            "BYMONTH" => month = v.split(',').next()?.trim().parse().ok()?,
            "BYDAY" => byday = Some(v.split(',').next()?.trim().to_ascii_uppercase()),
            "BYMONTHDAY" => {
                bymonthday = v.split(',').filter_map(|d| d.trim().parse().ok()).collect()
            }
            "UNTIL" => until = parse_ical_datetime(v.trim()),
            _ => {}
        }
    }
    if freq.as_deref() != Some("YEARLY") || year < dtstart.year() {
        return None;
    }
    let date = match byday {
        Some(day) => {
            let split = day.len().checked_sub(2)?;
            let weekday = parse_weekday(&day[split..])?;
            let ordinal = &day[..split];
            if ordinal.is_empty() {
                // `BYMONTHDAY=8,9,...,14;BYDAY=SU`: the listed day that is a Sunday.
                bymonthday.iter().find_map(|&d| {
                    NaiveDate::from_ymd_opt(year, month, d).filter(|x| x.weekday() == weekday)
                })?
            } else {
                nth_weekday(year, month, weekday, ordinal.parse().ok()?)?
            }
        }
        None => {
            NaiveDate::from_ymd_opt(year, month, *bymonthday.first().unwrap_or(&dtstart.day()))?
        }
    };
    if until.is_some_and(|u| date.and_time(dtstart.time()) > u) {
        return None;
    }
    Some(date)
}

/// The UTC instants at which a `VTIMEZONE` changes offset, with the offset
/// before and after each, sorted. Yearly rules are expanded for `year - 1`
/// and `year`, which is enough to know the offset at any point of `year`.
fn vtimezone_transitions(block: &str, year: i32) -> Vec<(NaiveDateTime, i32, i32)> {
    let mut observances = Vec::new();
    let mut current: Option<Observance> = None;
    for line in block.lines() {
        let line = line.trim();
        match line {
            "BEGIN:STANDARD" | "BEGIN:DAYLIGHT" => current = Some(Observance::default()),
            "END:STANDARD" | "END:DAYLIGHT" => observances.extend(current.take()),
            _ => {
                let (Some(obs), Some(parsed)) = (current.as_mut(), parse_content_line(line)) else {
                    continue;
                };
                let value = parsed.value.trim();
                match parsed.name.to_ascii_uppercase().as_str() {
                    "DTSTART" => obs.dtstart = parse_ical_datetime(value),
                    "TZOFFSETFROM" => obs.offset_from = parse_utc_offset(value),
                    "TZOFFSETTO" => obs.offset_to = parse_utc_offset(value),
                    "RRULE" => obs.rrule = Some(value.to_string()),
                    "RDATE" => obs.rdates.extend(
                        value
                            .split(',')
                            .filter_map(|v| parse_ical_datetime(v.trim())),
                    ),
                    _ => {}
                }
            }
        }
    }

    let mut transitions = Vec::new();
    for obs in &observances {
        let (Some(dtstart), Some(to)) = (obs.dtstart, obs.offset_to) else {
            continue;
        };
        let from = obs.offset_from.unwrap_or(to);
        let mut locals = vec![dtstart];
        locals.extend(&obs.rdates);
        if let Some(rrule) = &obs.rrule {
            locals.extend(
                [year - 1, year]
                    .into_iter()
                    .filter_map(|y| yearly_rule_date(rrule, dtstart, y))
                    .map(|d| d.and_time(dtstart.time())),
            );
        }
        // A local transition time is written in the offset in force before it.
        transitions.extend(
            locals
                .into_iter()
                .map(|local| (local - Duration::seconds(i64::from(from)), from, to)),
        );
    }
    transitions.sort_by_key(|t| t.0);
    transitions.dedup_by_key(|t| t.0);
    transitions
}

/// The offset a `VTIMEZONE` puts in force at a UTC instant.
fn vtimezone_offset(transitions: &[(NaiveDateTime, i32, i32)], utc: NaiveDateTime) -> Option<i32> {
    match transitions.iter().rev().find(|t| t.0 <= utc) {
        Some(t) => Some(t.2),
        None => transitions.first().map(|t| t.1),
    }
}

/// The `VTIMEZONE` block of `resource` whose TZID is `tzid`.
fn find_vtimezone<'a>(resource: &'a str, tzid: &str) -> Option<&'a str> {
    let mut from = 0;
    while let Some(start) = resource[from..].find("BEGIN:VTIMEZONE") {
        let start = from + start;
        let end = resource[start..]
            .find("END:VTIMEZONE")
            .map_or(resource.len(), |e| start + e);
        let block = &resource[start..end];
        let matches = block.lines().filter_map(parse_content_line).any(|l| {
            l.name.eq_ignore_ascii_case("TZID") && l.value.trim().trim_matches('"') == tzid
        });
        if matches {
            return Some(block);
        }
        from = end;
    }
    None
}

/// IANA areas, to prefer `Europe/Berlin` over `Etc/GMT-1` or `CET`.
fn in_area(tz: &Tz) -> bool {
    const AREAS: &[&str] = &[
        "Africa",
        "America",
        "Antarctica",
        "Asia",
        "Atlantic",
        "Australia",
        "Europe",
        "Indian",
        "Pacific",
    ];
    tz.name()
        .split_once('/')
        .is_some_and(|(area, _)| AREAS.contains(&area))
}

/// The IANA zone whose offsets in `year` are the ones the resource's
/// `VTIMEZONE` for `tzid` describes. Several usually match (every EU zone
/// shares one rule), and they convert that year's times identically; the
/// pick prefers a zone whose city the TZID names, then the first zone CLDR
/// maps a Windows name to, then any zone in an IANA area.
/// `resource` must already be unfolded.
fn from_vtimezone(resource: &str, tzid: &str, year: i32) -> Option<Tz> {
    let block = find_vtimezone(resource, tzid)?;

    static CACHE: OnceLock<Mutex<HashMap<(String, i32), Option<Tz>>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let key = (block.to_string(), year);
    if let Some(hit) = cache.lock().ok().and_then(|c| c.get(&key).copied()) {
        return hit;
    }

    let transitions = vtimezone_transitions(block, year);
    let found = (|| {
        let mut samples = vec![mid_month(year, 1), mid_month(year, 7)];
        for (utc, _, _) in transitions.iter().filter(|t| t.0.year() == year) {
            samples.push(*utc - Duration::hours(2));
            samples.push(*utc + Duration::hours(2));
        }
        let expected: Vec<i32> = samples
            .iter()
            .map(|s| vtimezone_offset(&transitions, *s))
            .collect::<Option<_>>()?;
        let candidates: Vec<Tz> = TZ_VARIANTS
            .iter()
            .copied()
            .filter(|tz| {
                samples
                    .iter()
                    .zip(&expected)
                    .all(|(s, e)| offset_at(tz, *s) == *e)
            })
            .collect();
        // Area zones only: `US/Eastern` would match "Eastern Time" by name.
        let named = |tz: &&Tz| {
            in_area(tz)
                && tz.name().rsplit('/').next().is_some_and(|city| {
                    city.len() >= 4
                        && tzid
                            .to_lowercase()
                            .contains(&city.replace('_', " ").to_lowercase())
                })
        };
        candidates
            .iter()
            .find(named)
            .or_else(|| {
                crate::windows_zones::default_zones()
                    .find_map(|zone| candidates.iter().find(|tz| tz.name() == zone))
            })
            .or_else(|| candidates.iter().find(|tz| in_area(tz)))
            .or_else(|| candidates.first())
            .copied()
    })();

    if let Ok(mut c) = cache.lock() {
        if c.len() >= 256 {
            c.clear();
        }
        c.insert(key, found);
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefixed_paths() {
        assert_eq!(
            resolve("/mozilla.org/20050126_1/Europe/Paris"),
            Some(chrono_tz::Europe::Paris)
        );
        assert_eq!(
            resolve("/freeassociation.sourceforge.net/Tzfile/America/Argentina/Buenos_Aires"),
            Some(chrono_tz::America::Argentina::Buenos_Aires)
        );
        assert_eq!(resolve("/custom/Nowhere"), None);
    }

    #[test]
    fn display_names_with_a_city() {
        assert_eq!(
            resolve("(UTC+01:00) Amsterdam, Berlin, Bern, Rome, Stockholm, Vienna"),
            Some(chrono_tz::Europe::Amsterdam)
        );
        assert_eq!(
            resolve("(UTC+05:30) Chennai, Kolkata, Mumbai, New Delhi"),
            Some(chrono_tz::Asia::Kolkata)
        );
        // A city with the wrong offset is not taken.
        assert_eq!(resolve("(UTC+03:00) Paris"), None);
    }

    #[test]
    fn display_names_without_a_city_fall_back_to_the_offset() {
        let name = "(UTC-05:00) Eastern Time (US & Canada)";
        assert_eq!(resolve(name), None);
        assert_eq!(resolve_or_offset(name), Some(chrono_tz::Etc::GMTPlus5));
        assert_eq!(
            resolve_or_offset("(UTC) Coordinated Universal Time"),
            Some(Tz::UTC)
        );
        assert_eq!(resolve_or_offset("(UTC+05:45) Kathmandu-ish"), None);
    }

    #[test]
    fn negative_nth_weekday() {
        assert_eq!(
            nth_weekday(2026, 3, Weekday::Sun, -1),
            NaiveDate::from_ymd_opt(2026, 3, 29)
        );
        assert_eq!(
            nth_weekday(2026, 10, Weekday::Sun, -1),
            NaiveDate::from_ymd_opt(2026, 10, 25)
        );
    }

    const OUTLOOK_EASTERN: &str = "BEGIN:VCALENDAR\r\n\
BEGIN:VTIMEZONE\r\n\
TZID:Customized Time Zone\r\n\
BEGIN:STANDARD\r\n\
DTSTART:16010101T020000\r\n\
TZOFFSETFROM:-0400\r\n\
TZOFFSETTO:-0500\r\n\
RRULE:FREQ=YEARLY;BYDAY=1SU;BYMONTH=11\r\n\
END:STANDARD\r\n\
BEGIN:DAYLIGHT\r\n\
DTSTART:16010101T020000\r\n\
TZOFFSETFROM:-0500\r\n\
TZOFFSETTO:-0400\r\n\
RRULE:FREQ=YEARLY;BYDAY=2SU;BYMONTH=3\r\n\
END:DAYLIGHT\r\n\
END:VTIMEZONE\r\n\
BEGIN:VEVENT\r\n\
UID:x\r\n\
DTSTART;TZID=\"Customized Time Zone\":20260924T140000\r\n\
END:VEVENT\r\n\
END:VCALENDAR\r\n";

    #[test]
    fn custom_vtimezone_matches_us_eastern() {
        let vevent = crate::utils::split_vevents(OUTLOOK_EASTERN).remove(0);
        assert_eq!(
            event_timezone(OUTLOOK_EASTERN, &vevent).as_deref(),
            Some("America/New_York")
        );
    }

    /// "Eastern" names the `US/Eastern` link, which must not win over the
    /// CLDR zone.
    #[test]
    fn display_name_without_city_uses_the_vtimezone() {
        let ics = OUTLOOK_EASTERN.replace(
            "Customized Time Zone",
            "(UTC-05:00) Eastern Time (US & Canada)",
        );
        let vevent = crate::utils::split_vevents(&ics).remove(0);
        assert_eq!(
            event_timezone(&ics, &vevent).as_deref(),
            Some("America/New_York")
        );
    }

    #[test]
    fn display_name_vtimezone_prefers_the_named_city() {
        let ics = "BEGIN:VCALENDAR\n\
BEGIN:VTIMEZONE\n\
TZID:(UTC+01:00) Brussels, Copenhagen, Madrid, Paris (custom)\n\
BEGIN:STANDARD\n\
DTSTART:16010101T030000\n\
TZOFFSETFROM:+0200\n\
TZOFFSETTO:+0100\n\
RRULE:FREQ=YEARLY;BYDAY=-1SU;BYMONTH=10\n\
END:STANDARD\n\
BEGIN:DAYLIGHT\n\
DTSTART:16010101T020000\n\
TZOFFSETFROM:+0100\n\
TZOFFSETTO:+0200\n\
RRULE:FREQ=YEARLY;BYDAY=-1SU;BYMONTH=3\n\
END:DAYLIGHT\n\
END:VTIMEZONE\n\
BEGIN:VEVENT\n\
DTSTART;TZID=\"(UTC+01:00) Brussels, Copenhagen, Madrid, Paris (custom)\":20260715T100000\n\
END:VEVENT\n\
END:VCALENDAR\n";
        let vevent = crate::utils::split_vevents(ics).remove(0);
        // The name resolves by city before the VTIMEZONE is consulted.
        assert_eq!(
            event_timezone(ics, &vevent).as_deref(),
            Some("Europe/Brussels")
        );
        // Without the city shortcut, the VTIMEZONE match still lands on a
        // zone the name mentions.
        assert_eq!(
            from_vtimezone(
                ics,
                "(UTC+01:00) Brussels, Copenhagen, Madrid, Paris (custom)",
                2026
            )
            .map(|tz| tz.name()),
            Some("Europe/Brussels")
        );
    }

    /// One resolver per feed: the folded resource is unfolded once and the
    /// match is reused for every event sharing the TZID.
    #[test]
    fn resource_zones_reuses_matches_across_events() {
        let feed = OUTLOOK_EASTERN
            .replace("TZID:Customized Time Zone\r\n", "TZID:Customized Time\r\n  Zone\r\n")
            .replace(
                "END:VEVENT\r\n",
                "END:VEVENT\r\nBEGIN:VEVENT\r\nUID:y\r\nDTSTART;TZID=\"Customized Time Zone\":20260110T090000\r\nEND:VEVENT\r\n",
            );
        let mut zones = ResourceZones::new(&feed);
        for vevent in crate::utils::split_vevents(&feed) {
            let fields = crate::utils::unfold_ical(&vevent);
            assert_eq!(
                zones.event_timezone(&fields).as_deref(),
                Some("America/New_York")
            );
        }
        assert_eq!(zones.memo.len(), 1);
    }

    #[test]
    fn vtimezone_without_dst() {
        let ics = "BEGIN:VCALENDAR\n\
BEGIN:VTIMEZONE\n\
TZID:My Zone\n\
BEGIN:STANDARD\n\
DTSTART:16010101T000000\n\
TZOFFSETFROM:+0900\n\
TZOFFSETTO:+0900\n\
END:STANDARD\n\
END:VTIMEZONE\n\
BEGIN:VEVENT\n\
DTSTART;TZID=My Zone:20260715T100000\n\
END:VEVENT\n\
END:VCALENDAR\n";
        let vevent = crate::utils::split_vevents(ics).remove(0);
        let tz: Tz = event_timezone(ics, &vevent).unwrap().parse().unwrap();
        assert_eq!(offset_at(&tz, mid_month(2026, 1)), 9 * 3600);
        assert_eq!(offset_at(&tz, mid_month(2026, 7)), 9 * 3600);
    }

    #[test]
    fn unknown_tzid_without_vtimezone_is_kept() {
        let vevent = "BEGIN:VEVENT\nDTSTART;TZID=Customized Time Zone:20260924T140000\nEND:VEVENT";
        assert_eq!(
            event_timezone(vevent, vevent).as_deref(),
            Some("Customized Time Zone")
        );
    }

    #[test]
    fn floating_and_all_day_stay_none() {
        let floating = "BEGIN:VEVENT\nDTSTART:20260924T140000\nEND:VEVENT";
        assert_eq!(event_timezone(floating, floating), None);
        let all_day = "BEGIN:VEVENT\nDTSTART;VALUE=DATE:20260924\nEND:VEVENT";
        assert_eq!(event_timezone(all_day, all_day), None);
    }
}
