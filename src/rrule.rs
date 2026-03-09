use chrono::{Datelike, Duration, NaiveDate, NaiveDateTime, Weekday};

/// A parsed RRULE.
struct RRule {
    freq: Freq,
    interval: u32,
    until: Option<NaiveDateTime>,
    count: Option<u32>,
    by_day: Vec<ByDay>,
}

enum Freq {
    Daily,
    Weekly,
    Monthly,
}

#[derive(Clone)]
struct ByDay {
    weekday: Weekday,
    nth: Option<i32>, // e.g. 2 for "2nd Monday", -1 for "last Friday"
}

/// Expand a recurring event into occurrences within [window_start, window_end).
/// Returns (start, end) pairs for each occurrence.
pub fn expand_rrule(
    event_start: NaiveDateTime,
    event_end: NaiveDateTime,
    rrule_str: &str,
    exdates: &[NaiveDateTime],
    window_start: NaiveDateTime,
    window_end: NaiveDateTime,
) -> Vec<(NaiveDateTime, NaiveDateTime)> {
    let rrule = match parse_rrule(rrule_str) {
        Some(r) => r,
        None => return vec![],
    };

    let event_duration = event_end - event_start;
    let mut results = Vec::new();
    let max_iter = 2000; // safety cap
    let mut iter_count = 0;

    match rrule.freq {
        Freq::Daily => {
            let mut current_date = event_start.date();
            let mut count_total = 0u32;
            loop {
                if iter_count >= max_iter { break; }
                iter_count += 1;

                let occ_start = current_date.and_time(event_start.time());
                let occ_end = occ_start + event_duration;

                if let Some(until) = rrule.until {
                    if occ_start > until { break; }
                }
                if occ_start >= window_end { break; }

                if !is_excluded(occ_start, exdates) {
                    if let Some(count) = rrule.count {
                        count_total += 1;
                        if count_total > count { break; }
                    }

                    if occ_end > window_start {
                        results.push((occ_start, occ_end));
                    }
                }

                current_date = current_date + Duration::days(rrule.interval as i64);
            }
        }
        Freq::Weekly => {
            // Determine which weekdays to use
            let weekdays: Vec<Weekday> = if rrule.by_day.is_empty() {
                vec![event_start.weekday()]
            } else {
                rrule.by_day.iter().map(|bd| bd.weekday).collect()
            };

            let mut count_total = 0u32;
            // Start from the week of the event, iterate by interval weeks
            let event_week_start = week_start(event_start.date());
            let mut current_week = event_week_start;

            loop {
                if iter_count >= max_iter { break; }

                for &wd in &weekdays {
                    iter_count += 1;
                    let day = current_week + Duration::days(weekday_offset(wd) as i64);
                    let occ_start = day.and_time(event_start.time());
                    let occ_end = occ_start + event_duration;

                    if occ_start < event_start { continue; }
                    if let Some(until) = rrule.until {
                        if occ_start > until { return results; }
                    }
                    if occ_start >= window_end { return results; }

                    if let Some(count) = rrule.count {
                        count_total += 1;
                        if count_total > count { return results; }
                    }

                    if occ_end > window_start && !is_excluded(occ_start, exdates) {
                        results.push((occ_start, occ_end));
                    }
                }

                current_week = current_week + Duration::weeks(rrule.interval as i64);
            }
        }
        Freq::Monthly => {
            let mut year = event_start.year();
            let mut month = event_start.month();
            let mut count_total = 0u32;

            loop {
                if iter_count >= max_iter { break; }
                iter_count += 1;

                let occurrences_this_month: Vec<NaiveDate> = if rrule.by_day.is_empty() {
                    // Same day of month
                    match NaiveDate::from_ymd_opt(year, month, event_start.day()) {
                        Some(d) => vec![d],
                        None => vec![], // e.g. Feb 30
                    }
                } else {
                    rrule.by_day.iter().filter_map(|bd| {
                        nth_weekday_of_month(year, month, bd.weekday, bd.nth.unwrap_or(1))
                    }).collect()
                };

                for day in occurrences_this_month {
                    let occ_start = day.and_time(event_start.time());
                    let occ_end = occ_start + event_duration;

                    if occ_start < event_start { continue; }
                    if let Some(until) = rrule.until {
                        if occ_start > until { return results; }
                    }
                    if occ_start >= window_end { return results; }

                    if let Some(count) = rrule.count {
                        count_total += 1;
                        if count_total > count { return results; }
                    }

                    if occ_end > window_start && !is_excluded(occ_start, exdates) {
                        results.push((occ_start, occ_end));
                    }
                }

                // Advance by interval months
                month += rrule.interval;
                while month > 12 {
                    month -= 12;
                    year += 1;
                }
            }
        }
    }

    results
}

/// Parse EXDATE values and RECURRENCE-ID overrides from raw iCal.
/// Returns NaiveDateTimes that should be excluded from RRULE expansion.
/// Scans ALL VEVENTs in the resource: EXDATEs from the first (recurring) VEVENT,
/// and RECURRENCE-ID values from any override VEVENTs (modified instances).
pub fn extract_exdates(raw_ical: &str) -> Vec<NaiveDateTime> {
    let mut exdates = Vec::new();

    // Extract EXDATEs from the first VEVENT (the one with the RRULE)
    if let Some(vevent_start) = raw_ical.find("BEGIN:VEVENT") {
        let vevent_end = raw_ical[vevent_start..]
            .find("END:VEVENT")
            .map(|i| vevent_start + i)
            .unwrap_or(raw_ical.len());
        let vevent = &raw_ical[vevent_start..vevent_end];

        for line in vevent.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("EXDATE") {
                if let Some(colon) = trimmed.find(':') {
                    let values = &trimmed[colon + 1..];
                    for val in values.split(',') {
                        if let Some(dt) = parse_exdate(val.trim()) {
                            exdates.push(dt);
                        }
                    }
                }
            }
        }
    }

    // Also extract RECURRENCE-ID from any override VEVENTs in the same resource.
    // These represent modified instances — the original occurrence should be excluded
    // from RRULE expansion (the modified instance is stored/displayed separately).
    for line in raw_ical.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("RECURRENCE-ID") {
            if let Some(colon) = trimmed.find(':') {
                let val = trimmed[colon + 1..].trim();
                if let Some(dt) = parse_exdate(val) {
                    exdates.push(dt);
                }
            }
        }
    }

    exdates
}

fn parse_exdate(s: &str) -> Option<NaiveDateTime> {
    parse_ical_datetime(s)
}

/// Parse an iCal datetime string (compact or ISO format, with optional trailing Z).
pub fn parse_ical_datetime(s: &str) -> Option<NaiveDateTime> {
    let s = s.strip_suffix('Z').unwrap_or(s);
    NaiveDateTime::parse_from_str(s, "%Y%m%dT%H%M%S")
        .ok()
        .or_else(|| NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S").ok())
        .or_else(|| NaiveDate::parse_from_str(s, "%Y%m%d").ok()
            .and_then(|d| d.and_hms_opt(0, 0, 0)))
}

fn parse_rrule(s: &str) -> Option<RRule> {
    let mut freq = None;
    let mut interval = 1u32;
    let mut until = None;
    let mut count = None;
    let mut by_day = Vec::new();

    for part in s.split(';') {
        if let Some(val) = part.strip_prefix("FREQ=") {
            freq = match val {
                "DAILY" => Some(Freq::Daily),
                "WEEKLY" => Some(Freq::Weekly),
                "MONTHLY" => Some(Freq::Monthly),
                _ => None,
            };
        } else if let Some(val) = part.strip_prefix("INTERVAL=") {
            interval = val.parse().unwrap_or(1);
        } else if let Some(val) = part.strip_prefix("UNTIL=") {
            let v = val.strip_suffix('Z').unwrap_or(val);
            until = NaiveDateTime::parse_from_str(v, "%Y%m%dT%H%M%S")
                .ok()
                .or_else(|| NaiveDate::parse_from_str(v, "%Y%m%d").ok()
                    .and_then(|d| d.and_hms_opt(23, 59, 59)));
        } else if let Some(val) = part.strip_prefix("COUNT=") {
            count = val.parse().ok();
        } else if let Some(val) = part.strip_prefix("BYDAY=") {
            for day_str in val.split(',') {
                if let Some(bd) = parse_byday(day_str.trim()) {
                    by_day.push(bd);
                }
            }
        }
    }

    Some(RRule { freq: freq?, interval, until, count, by_day })
}

fn parse_byday(s: &str) -> Option<ByDay> {
    // "MO", "TU", "2MO", "-1FR", etc.
    let (nth, day_part) = if s.len() > 2 {
        let day_part = &s[s.len() - 2..];
        let nth_part = &s[..s.len() - 2];
        (nth_part.parse::<i32>().ok(), day_part)
    } else {
        (None, s)
    };

    let weekday = match day_part {
        "MO" => Weekday::Mon,
        "TU" => Weekday::Tue,
        "WE" => Weekday::Wed,
        "TH" => Weekday::Thu,
        "FR" => Weekday::Fri,
        "SA" => Weekday::Sat,
        "SU" => Weekday::Sun,
        _ => return None,
    };

    Some(ByDay { weekday, nth })
}

fn is_excluded(dt: NaiveDateTime, exdates: &[NaiveDateTime]) -> bool {
    exdates.iter().any(|ex| {
        // Match by date + time, or just by date for all-day events
        *ex == dt || ex.date() == dt.date()
    })
}

/// Monday-based week start for a given date.
fn week_start(d: NaiveDate) -> NaiveDate {
    let days_since_monday = d.weekday().num_days_from_monday() as i64;
    d - Duration::days(days_since_monday)
}

/// Offset from Monday (0=Mon, 1=Tue, ..., 6=Sun).
fn weekday_offset(wd: Weekday) -> i64 {
    wd.num_days_from_monday() as i64
}

/// Find the Nth weekday of a month (e.g., 2nd Monday, 3rd Wednesday).
/// nth=1 is first, nth=-1 is last.
fn nth_weekday_of_month(year: i32, month: u32, weekday: Weekday, nth: i32) -> Option<NaiveDate> {
    if nth > 0 {
        // Find first occurrence of weekday in month
        let first = NaiveDate::from_ymd_opt(year, month, 1)?;
        let first_wd = first.weekday();
        let diff = (weekday.num_days_from_monday() as i64 - first_wd.num_days_from_monday() as i64 + 7) % 7;
        let target = first + Duration::days(diff + (nth as i64 - 1) * 7);
        if target.month() == month { Some(target) } else { None }
    } else if nth == -1 {
        // Last occurrence: start from end of month
        let next_month = if month == 12 {
            NaiveDate::from_ymd_opt(year + 1, 1, 1)?
        } else {
            NaiveDate::from_ymd_opt(year, month + 1, 1)?
        };
        let last_day = next_month - Duration::days(1);
        let last_wd = last_day.weekday();
        let diff = (last_wd.num_days_from_monday() as i64 - weekday.num_days_from_monday() as i64 + 7) % 7;
        Some(last_day - Duration::days(diff))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dt(y: i32, m: u32, d: u32, h: u32, min: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
            .and_hms_opt(h, min, 0).unwrap()
    }

    #[test]
    fn test_weekly_monday() {
        let start = dt(2026, 2, 23, 16, 0);
        let end = dt(2026, 2, 23, 18, 0);
        let window_start = dt(2026, 3, 23, 0, 0);
        let window_end = dt(2026, 3, 24, 0, 0);

        let results = expand_rrule(start, end, "FREQ=WEEKLY;INTERVAL=1;BYDAY=MO", &[], window_start, window_end);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].0, dt(2026, 3, 23, 16, 0));
        assert_eq!(results[0].1, dt(2026, 3, 23, 18, 0));
    }

    #[test]
    fn test_weekly_with_until() {
        let start = dt(2026, 1, 5, 10, 0);
        let end = dt(2026, 1, 5, 11, 0);
        let window_start = dt(2026, 2, 1, 0, 0);
        let window_end = dt(2026, 3, 1, 0, 0);

        let results = expand_rrule(start, end, "FREQ=WEEKLY;UNTIL=20260209T100000;BYDAY=MO", &[], window_start, window_end);
        assert_eq!(results.len(), 2); // Feb 2 and Feb 9
        assert_eq!(results[0].0.date(), NaiveDate::from_ymd_opt(2026, 2, 2).unwrap());
        assert_eq!(results[1].0.date(), NaiveDate::from_ymd_opt(2026, 2, 9).unwrap());
    }

    #[test]
    fn test_monthly_2nd_monday() {
        let start = dt(2026, 1, 12, 16, 0);
        let end = dt(2026, 1, 12, 17, 0);
        let window_start = dt(2026, 3, 1, 0, 0);
        let window_end = dt(2026, 4, 1, 0, 0);

        let results = expand_rrule(start, end, "FREQ=MONTHLY;INTERVAL=1;BYDAY=2MO", &[], window_start, window_end);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].0, dt(2026, 3, 9, 16, 0)); // 2nd Monday of March 2026
    }

    #[test]
    fn test_exdate_exclusion() {
        let start = dt(2026, 3, 2, 10, 0);
        let end = dt(2026, 3, 2, 11, 0);
        let window_start = dt(2026, 3, 1, 0, 0);
        let window_end = dt(2026, 3, 31, 0, 0);
        let exdates = vec![dt(2026, 3, 9, 10, 0)]; // exclude March 9

        let results = expand_rrule(start, end, "FREQ=WEEKLY;BYDAY=MO", &exdates, window_start, window_end);
        let dates: Vec<_> = results.iter().map(|(s, _)| s.date()).collect();
        assert!(dates.contains(&NaiveDate::from_ymd_opt(2026, 3, 2).unwrap()));
        assert!(!dates.contains(&NaiveDate::from_ymd_opt(2026, 3, 9).unwrap())); // excluded
        assert!(dates.contains(&NaiveDate::from_ymd_opt(2026, 3, 16).unwrap()));
        assert!(dates.contains(&NaiveDate::from_ymd_opt(2026, 3, 23).unwrap()));
        assert!(dates.contains(&NaiveDate::from_ymd_opt(2026, 3, 30).unwrap()));
    }

    #[test]
    fn test_extract_exdates() {
        let ical = "BEGIN:VEVENT\nDTSTART:20260302T100000\nRRULE:FREQ=WEEKLY;BYDAY=MO\nEXDATE:20260309T100000\nEXDATE:20260316T100000\nEND:VEVENT";
        let exdates = extract_exdates(ical);
        assert_eq!(exdates.len(), 2);
        assert_eq!(exdates[0], dt(2026, 3, 9, 10, 0));
        assert_eq!(exdates[1], dt(2026, 3, 16, 10, 0));
    }

    #[test]
    fn test_weekly_count() {
        let start = dt(2026, 3, 2, 9, 0);
        let end = dt(2026, 3, 2, 10, 0);
        let window_start = dt(2026, 3, 1, 0, 0);
        let window_end = dt(2026, 12, 31, 0, 0);

        let results = expand_rrule(start, end, "FREQ=WEEKLY;COUNT=3;BYDAY=MO", &[], window_start, window_end);
        assert_eq!(results.len(), 3);
    }

    #[test]
    fn test_recurrence_id_exclusion() {
        // A recurring event with a modified instance (RECURRENCE-ID)
        let ical = "BEGIN:VCALENDAR\n\
            BEGIN:VEVENT\n\
            UID:abc\n\
            DTSTART:20260302T100000\n\
            DTEND:20260302T110000\n\
            RRULE:FREQ=WEEKLY;BYDAY=MO\n\
            END:VEVENT\n\
            BEGIN:VEVENT\n\
            UID:abc\n\
            RECURRENCE-ID:20260309T100000\n\
            DTSTART:20260309T140000\n\
            DTEND:20260309T150000\n\
            END:VEVENT\n\
            END:VCALENDAR";
        let exdates = extract_exdates(ical);
        // The RECURRENCE-ID should be treated as an exclusion
        assert_eq!(exdates.len(), 1);
        assert_eq!(exdates[0], dt(2026, 3, 9, 10, 0));

        // The original March 9 occurrence should be excluded from expansion
        let start = dt(2026, 3, 2, 10, 0);
        let end = dt(2026, 3, 2, 11, 0);
        let window_start = dt(2026, 3, 1, 0, 0);
        let window_end = dt(2026, 3, 31, 0, 0);
        let results = expand_rrule(start, end, "FREQ=WEEKLY;BYDAY=MO", &exdates, window_start, window_end);
        let dates: Vec<_> = results.iter().map(|(s, _)| s.date()).collect();
        assert!(dates.contains(&NaiveDate::from_ymd_opt(2026, 3, 2).unwrap()));
        assert!(!dates.contains(&NaiveDate::from_ymd_opt(2026, 3, 9).unwrap())); // excluded by RECURRENCE-ID
        assert!(dates.contains(&NaiveDate::from_ymd_opt(2026, 3, 16).unwrap()));
    }

    #[test]
    fn test_daily_count_pre_window() {
        // Daily event with COUNT=3 starting before the window
        let start = dt(2026, 3, 1, 9, 0);
        let end = dt(2026, 3, 1, 10, 0);
        let window_start = dt(2026, 3, 3, 0, 0); // window starts after 2 occurrences
        let window_end = dt(2026, 3, 10, 0, 0);

        let results = expand_rrule(start, end, "FREQ=DAILY;COUNT=3", &[], window_start, window_end);
        // COUNT=3: Mar 1, Mar 2, Mar 3 — only Mar 3 is in window
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].0, dt(2026, 3, 3, 9, 0));
    }
}
