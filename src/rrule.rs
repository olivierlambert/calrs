use chrono::{Datelike, Duration, NaiveDate, NaiveDateTime, NaiveTime, Weekday};
use chrono_tz::Tz;

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

/// Most recurrence periods (days, weeks or months) walked for one window,
/// counted from the first period that can reach it, not from DTSTART
/// (#376). A year of a daily series is 367 of them.
const MAX_PERIODS: i64 = 20_000;

/// Expand a recurring event into occurrences within [window_start, window_end).
/// Returns (start, end) pairs for each occurrence.
///
/// The walk starts at the window, however long ago the series began: daily
/// and weekly series skip ahead arithmetically, counting the occurrences
/// they skip so `COUNT` still ends the series where it should, and monthly
/// ones skip ahead too unless they carry a `COUNT` (months hold a varying
/// number of occurrences, so those are counted month by month). Excluded
/// occurrences count toward `COUNT`, as RFC 5545 says.
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
    let duration = (event_end - event_start).max(Duration::zero());
    // The earliest start an occurrence still reaching the window can have.
    let reach = window_start
        .checked_sub_signed(duration)
        .unwrap_or(window_start);
    let (first_period, skipped) = skip_to(&rrule, event_start, reach);

    let mut results = Vec::new();
    let mut seen = skipped;
    'periods: for period in first_period..first_period.saturating_add(MAX_PERIODS) {
        let Some((first_day, days)) = period_dates(&rrule, event_start, period) else {
            break;
        };
        // Past the window or the series' end: so is every later period,
        // even when this one holds no occurrence (a 5th Monday, a 31st).
        let first = first_day.and_time(NaiveTime::MIN);
        if first >= window_end || rrule.until.is_some_and(|until| first > until) {
            break;
        }
        for day in days {
            let occ_start = day.and_time(event_start.time());
            if occ_start < event_start {
                continue;
            }
            if rrule.until.is_some_and(|until| occ_start > until) || occ_start >= window_end {
                break 'periods;
            }
            seen += 1;
            if rrule.count.is_some_and(|count| seen > i64::from(count)) {
                break 'periods;
            }
            let Some(occ_end) = occ_start.checked_add_signed(duration) else {
                break 'periods;
            };
            if occ_end > window_start && !is_excluded(occ_start, exdates) {
                results.push((occ_start, occ_end));
            }
        }
    }
    results
}

/// The first period worth walking for occurrences starting at `reach` or
/// later, and how many occurrences the periods before it hold. Every
/// occurrence of a skipped period starts before `reach`, so it cannot reach
/// the window: one period of margin is kept for the time of day.
fn skip_to(rrule: &RRule, event_start: NaiveDateTime, reach: NaiveDateTime) -> (i64, i64) {
    let interval = i64::from(rrule.interval);
    let periods_before = |units: i64| (units.div_euclid(interval) - 1).max(0);
    match rrule.freq {
        Freq::Daily => {
            let skip = periods_before((reach.date() - event_start.date()).num_days());
            (skip, skip)
        }
        Freq::Weekly => {
            let weeks = (week_start(reach.date()) - week_start(event_start.date())).num_days() / 7;
            let skip = periods_before(weeks);
            if skip == 0 {
                return (0, 0);
            }
            // The first week holds the days on or after DTSTART, every
            // later week all of them.
            let first = period_dates(rrule, event_start, 0)
                .map(|(_, days)| days)
                .unwrap_or_default()
                .iter()
                .filter(|d| d.and_time(event_start.time()) >= event_start)
                .count() as i64;
            let per_week = weekly_offsets(rrule, event_start).len() as i64;
            (skip, first + (skip - 1) * per_week)
        }
        Freq::Monthly if rrule.count.is_some() => (0, 0),
        Freq::Monthly => (
            periods_before(month_index(reach.date()) - month_index(event_start.date())),
            0,
        ),
    }
}

/// The first day of one period of the series and its dates, in order:
/// period 0 is the day, week or month of DTSTART, period n the one
/// `n * INTERVAL` units later. None once the calendar runs out, which ends
/// the walk.
fn period_dates(
    rrule: &RRule,
    event_start: NaiveDateTime,
    period: i64,
) -> Option<(NaiveDate, Vec<NaiveDate>)> {
    let units = period.checked_mul(i64::from(rrule.interval))?;
    let start = event_start.date();
    match rrule.freq {
        Freq::Daily => {
            let day = start.checked_add_signed(Duration::try_days(units)?)?;
            Some((day, vec![day]))
        }
        Freq::Weekly => {
            let week = week_start(start).checked_add_signed(Duration::try_weeks(units)?)?;
            let days = weekly_offsets(rrule, event_start)
                .into_iter()
                .map(|offset| week.checked_add_signed(Duration::days(offset)))
                .collect::<Option<Vec<_>>>()?;
            Some((week, days))
        }
        Freq::Monthly => {
            let index = month_index(start).checked_add(units)?;
            let year = i32::try_from(index.div_euclid(12)).ok()?;
            let month = index.rem_euclid(12) as u32 + 1;
            let first_day = NaiveDate::from_ymd_opt(year, month, 1)?;
            let mut days: Vec<NaiveDate> = if rrule.by_day.is_empty() {
                // Same day of month; none in a month too short for it.
                NaiveDate::from_ymd_opt(year, month, start.day())
                    .into_iter()
                    .collect()
            } else {
                rrule
                    .by_day
                    .iter()
                    .flat_map(|bd| match bd.nth {
                        Some(nth) => nth_weekday_of_month(year, month, bd.weekday, nth)
                            .into_iter()
                            .collect(),
                        // A bare weekday is every one of them in the month.
                        None => (1..=5)
                            .filter_map(|nth| nth_weekday_of_month(year, month, bd.weekday, nth))
                            .collect::<Vec<_>>(),
                    })
                    .collect()
            };
            days.sort();
            days.dedup();
            Some((first_day, days))
        }
    }
}

/// The days of the week a weekly series falls on, as offsets from Monday,
/// in order: `BYDAY`, else DTSTART's weekday.
fn weekly_offsets(rrule: &RRule, event_start: NaiveDateTime) -> Vec<i64> {
    let mut offsets: Vec<i64> = if rrule.by_day.is_empty() {
        vec![weekday_offset(event_start.weekday())]
    } else {
        rrule
            .by_day
            .iter()
            .map(|bd| weekday_offset(bd.weekday))
            .collect()
    };
    offsets.sort();
    offsets.dedup();
    offsets
}

/// Months since year 0, so two months subtract to the months between them.
fn month_index(d: NaiveDate) -> i64 {
    i64::from(d.year()) * 12 + i64::from(d.month0())
}

/// Occurrences of one stored recurring event (start and end as stored, its
/// RRULE, its raw iCal for EXDATE and RECURRENCE-ID, its timezone)
/// overlapping `[window_start, window_end)`, given and returned as
/// wall-clock times in `target`. The series is expanded in its own zone,
/// which can be a day away from `target`, so the expansion window is
/// widened by two days and the result clipped after conversion. The one
/// expansion the slot engine, resources, troubleshoot and the CLI share.
#[allow(clippy::too_many_arguments)]
pub fn occurrences_in(
    start: &str,
    end: &str,
    rrule: &str,
    raw_ical: Option<&str>,
    event_tz: Option<&str>,
    window_start: NaiveDateTime,
    window_end: NaiveDateTime,
    target: Tz,
) -> Vec<(NaiveDateTime, NaiveDateTime)> {
    let (Some(ev_start), Some(ev_end)) = (
        crate::utils::parse_ical_datetime(start),
        crate::utils::parse_ical_datetime(end),
    ) else {
        return vec![];
    };
    let exdates = raw_ical
        .map(|raw| extract_exdates(raw, event_tz))
        .unwrap_or_default();
    let slack = Duration::days(2);
    expand_rrule(
        ev_start,
        ev_end,
        rrule,
        &exdates,
        window_start
            .checked_sub_signed(slack)
            .unwrap_or(window_start),
        window_end.checked_add_signed(slack).unwrap_or(window_end),
    )
    .into_iter()
    .map(|(s, e)| {
        (
            crate::utils::convert_event_to_tz(s, event_tz, target),
            crate::utils::convert_event_to_tz(e, event_tz, target),
        )
    })
    .filter(|(s, e)| *s < window_end && *e > window_start)
    .collect()
}

/// Parse EXDATE values and RECURRENCE-ID overrides from raw iCal.
/// Returns NaiveDateTimes that should be excluded from RRULE expansion.
/// Scans ALL VEVENTs in the resource: EXDATEs from the first (recurring) VEVENT,
/// and RECURRENCE-ID values from any override VEVENTs (modified instances).
///
/// Occurrences are expanded in the event's own timezone (`event_tz`, as stored
/// in `events.timezone`), so a value written in UTC (`...Z`) or under another
/// TZID is converted to it first; otherwise `EXDATE:20260308T233000Z` would
/// miss the 00:30 Paris occurrence it cancels.
pub fn extract_exdates(raw_ical: &str, event_tz: Option<&str>) -> Vec<NaiveDateTime> {
    let event_tz = event_tz.and_then(crate::tzid::resolve_or_offset);
    let mut exdates = Vec::new();
    // A long EXDATE list is folded over several lines.
    let unfolded = crate::utils::unfold_ical(raw_ical);
    let raw_ical: &str = &unfolded;

    // Extract EXDATEs from the first VEVENT (the one with the RRULE)
    if let Some(vevent_start) = raw_ical.find("BEGIN:VEVENT") {
        let vevent_end = raw_ical[vevent_start..]
            .find("END:VEVENT")
            .map(|i| vevent_start + i)
            .unwrap_or(raw_ical.len());
        let vevent = &raw_ical[vevent_start..vevent_end];

        for line in vevent.lines() {
            let Some(parsed) = crate::utils::parse_content_line(line.trim()) else {
                continue;
            };
            if parsed.name.eq_ignore_ascii_case("EXDATE") {
                for val in parsed.value.split(',') {
                    if let Some(dt) = parse_exdate(val.trim(), parsed.param("TZID"), event_tz) {
                        exdates.push(dt);
                    }
                }
            }
        }
    }

    // Also extract RECURRENCE-ID from any override VEVENTs in the same resource.
    // These represent modified instances — the original occurrence should be excluded
    // from RRULE expansion (the modified instance is stored/displayed separately).
    for line in raw_ical.lines() {
        let Some(parsed) = crate::utils::parse_content_line(line.trim()) else {
            continue;
        };
        if parsed.name.eq_ignore_ascii_case("RECURRENCE-ID") {
            if let Some(dt) = parse_exdate(parsed.value.trim(), parsed.param("TZID"), event_tz) {
                exdates.push(dt);
            }
        }
    }

    exdates
}

/// One EXDATE or RECURRENCE-ID value, in the event's timezone. A value in
/// UTC or under a TZID that resolves is converted; one that is floating, or
/// whose TZID does not resolve (the same custom name as the DTSTART, whose
/// zone was only found through its `VTIMEZONE`), is taken as already being
/// in the event's timezone, as is everything when the event is floating.
fn parse_exdate(s: &str, value_tz: Option<&str>, event_tz: Option<Tz>) -> Option<NaiveDateTime> {
    let dt = crate::utils::parse_ical_datetime(s)?;
    let source = if s.ends_with('Z') {
        Some("UTC")
    } else {
        value_tz
    };
    match (source, event_tz) {
        (Some(source), Some(event_tz)) if crate::tzid::resolve_or_offset(source).is_some() => Some(
            crate::utils::convert_event_to_tz(dt, Some(source), event_tz),
        ),
        _ => Some(dt),
    }
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
            interval = val.parse().ok().filter(|&i| i > 0).unwrap_or(1);
        } else if let Some(val) = part.strip_prefix("UNTIL=") {
            let v = val.strip_suffix('Z').unwrap_or(val);
            until = NaiveDateTime::parse_from_str(v, "%Y%m%dT%H%M%S")
                .ok()
                .or_else(|| {
                    NaiveDate::parse_from_str(v, "%Y%m%d")
                        .ok()
                        .and_then(|d| d.and_hms_opt(23, 59, 59))
                });
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

    Some(RRule {
        freq: freq?,
        interval,
        until,
        count,
        by_day,
    })
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
    // A month holds at most five of any weekday.
    if !(-5..=5).contains(&nth) {
        return None;
    }
    if nth > 0 {
        // Find first occurrence of weekday in month
        let first = NaiveDate::from_ymd_opt(year, month, 1)?;
        let first_wd = first.weekday();
        let diff = (weekday.num_days_from_monday() as i64 - first_wd.num_days_from_monday() as i64
            + 7)
            % 7;
        let target = first + Duration::days(diff + (nth as i64 - 1) * 7);
        if target.month() == month {
            Some(target)
        } else {
            None
        }
    } else if nth < 0 {
        // Counting back from the end of the month: -1 is the last.
        let next_month = if month == 12 {
            NaiveDate::from_ymd_opt(year + 1, 1, 1)?
        } else {
            NaiveDate::from_ymd_opt(year, month + 1, 1)?
        };
        let last_day = next_month - Duration::days(1);
        let last_wd = last_day.weekday();
        let diff =
            (last_wd.num_days_from_monday() as i64 - weekday.num_days_from_monday() as i64 + 7) % 7;
        let target = last_day - Duration::days(diff + (-(nth as i64) - 1) * 7);
        (target.month() == month).then_some(target)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dt(y: i32, m: u32, d: u32, h: u32, min: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(y, m, d)
            .unwrap()
            .and_hms_opt(h, min, 0)
            .unwrap()
    }

    #[test]
    fn test_weekly_monday() {
        let start = dt(2026, 2, 23, 16, 0);
        let end = dt(2026, 2, 23, 18, 0);
        let window_start = dt(2026, 3, 23, 0, 0);
        let window_end = dt(2026, 3, 24, 0, 0);

        let results = expand_rrule(
            start,
            end,
            "FREQ=WEEKLY;INTERVAL=1;BYDAY=MO",
            &[],
            window_start,
            window_end,
        );
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

        let results = expand_rrule(
            start,
            end,
            "FREQ=WEEKLY;UNTIL=20260209T100000;BYDAY=MO",
            &[],
            window_start,
            window_end,
        );
        assert_eq!(results.len(), 2); // Feb 2 and Feb 9
        assert_eq!(
            results[0].0.date(),
            NaiveDate::from_ymd_opt(2026, 2, 2).unwrap()
        );
        assert_eq!(
            results[1].0.date(),
            NaiveDate::from_ymd_opt(2026, 2, 9).unwrap()
        );
    }

    #[test]
    fn test_monthly_2nd_monday() {
        let start = dt(2026, 1, 12, 16, 0);
        let end = dt(2026, 1, 12, 17, 0);
        let window_start = dt(2026, 3, 1, 0, 0);
        let window_end = dt(2026, 4, 1, 0, 0);

        let results = expand_rrule(
            start,
            end,
            "FREQ=MONTHLY;INTERVAL=1;BYDAY=2MO",
            &[],
            window_start,
            window_end,
        );
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

        let results = expand_rrule(
            start,
            end,
            "FREQ=WEEKLY;BYDAY=MO",
            &exdates,
            window_start,
            window_end,
        );
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
        let exdates = extract_exdates(ical, None);
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

        let results = expand_rrule(
            start,
            end,
            "FREQ=WEEKLY;COUNT=3;BYDAY=MO",
            &[],
            window_start,
            window_end,
        );
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
        let exdates = extract_exdates(ical, None);
        // The RECURRENCE-ID should be treated as an exclusion
        assert_eq!(exdates.len(), 1);
        assert_eq!(exdates[0], dt(2026, 3, 9, 10, 0));

        // The original March 9 occurrence should be excluded from expansion
        let start = dt(2026, 3, 2, 10, 0);
        let end = dt(2026, 3, 2, 11, 0);
        let window_start = dt(2026, 3, 1, 0, 0);
        let window_end = dt(2026, 3, 31, 0, 0);
        let results = expand_rrule(
            start,
            end,
            "FREQ=WEEKLY;BYDAY=MO",
            &exdates,
            window_start,
            window_end,
        );
        let dates: Vec<_> = results.iter().map(|(s, _)| s.date()).collect();
        assert!(dates.contains(&NaiveDate::from_ymd_opt(2026, 3, 2).unwrap()));
        assert!(!dates.contains(&NaiveDate::from_ymd_opt(2026, 3, 9).unwrap())); // excluded by RECURRENCE-ID
        assert!(dates.contains(&NaiveDate::from_ymd_opt(2026, 3, 16).unwrap()));
    }

    #[test]
    fn test_weekly_multi_byday_devops() {
        // Real case: DevOps tools daily, WEEKLY on MO,TU,TH,FR
        // Starts 2025-10-20 (Monday), UNTIL 2026-03-15
        let start = dt(2025, 10, 20, 10, 30);
        let end = dt(2025, 10, 20, 11, 0);
        let window_start = dt(2026, 3, 10, 0, 0);
        let window_end = dt(2026, 3, 10, 23, 59);

        let results = expand_rrule(
            start,
            end,
            "FREQ=WEEKLY;UNTIL=20260315T093000;INTERVAL=1;BYDAY=MO,TU,TH,FR",
            &[],
            window_start,
            window_end,
        );
        // March 10 is a Tuesday — should have one occurrence at 10:30
        assert!(
            !results.is_empty(),
            "Expected occurrence on Tuesday March 10"
        );
        assert_eq!(results[0].0, dt(2026, 3, 10, 10, 30));
        assert_eq!(results[0].1, dt(2026, 3, 10, 11, 0));
    }

    #[test]
    fn test_daily_count_pre_window() {
        // Daily event with COUNT=3 starting before the window
        let start = dt(2026, 3, 1, 9, 0);
        let end = dt(2026, 3, 1, 10, 0);
        let window_start = dt(2026, 3, 3, 0, 0); // window starts after 2 occurrences
        let window_end = dt(2026, 3, 10, 0, 0);

        let results = expand_rrule(
            start,
            end,
            "FREQ=DAILY;COUNT=3",
            &[],
            window_start,
            window_end,
        );
        // COUNT=3: Mar 1, Mar 2, Mar 3 — only Mar 3 is in window
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].0, dt(2026, 3, 3, 9, 0));
    }

    // --- Additional edge case tests ---

    #[test]
    fn test_daily_interval_2() {
        let start = dt(2026, 3, 1, 9, 0);
        let end = dt(2026, 3, 1, 10, 0);
        let window_start = dt(2026, 3, 1, 0, 0);
        let window_end = dt(2026, 3, 8, 0, 0);

        let results = expand_rrule(
            start,
            end,
            "FREQ=DAILY;INTERVAL=2",
            &[],
            window_start,
            window_end,
        );
        // Mar 1, 3, 5, 7
        assert_eq!(results.len(), 4);
        assert_eq!(
            results[0].0.date(),
            NaiveDate::from_ymd_opt(2026, 3, 1).unwrap()
        );
        assert_eq!(
            results[1].0.date(),
            NaiveDate::from_ymd_opt(2026, 3, 3).unwrap()
        );
        assert_eq!(
            results[2].0.date(),
            NaiveDate::from_ymd_opt(2026, 3, 5).unwrap()
        );
        assert_eq!(
            results[3].0.date(),
            NaiveDate::from_ymd_opt(2026, 3, 7).unwrap()
        );
    }

    #[test]
    fn test_weekly_interval_2() {
        let start = dt(2026, 3, 2, 10, 0); // Monday
        let end = dt(2026, 3, 2, 11, 0);
        let window_start = dt(2026, 3, 1, 0, 0);
        let window_end = dt(2026, 4, 1, 0, 0);

        let results = expand_rrule(
            start,
            end,
            "FREQ=WEEKLY;INTERVAL=2;BYDAY=MO",
            &[],
            window_start,
            window_end,
        );
        // Mar 2, Mar 16, Mar 30
        assert_eq!(results.len(), 3);
        assert_eq!(
            results[0].0.date(),
            NaiveDate::from_ymd_opt(2026, 3, 2).unwrap()
        );
        assert_eq!(
            results[1].0.date(),
            NaiveDate::from_ymd_opt(2026, 3, 16).unwrap()
        );
        assert_eq!(
            results[2].0.date(),
            NaiveDate::from_ymd_opt(2026, 3, 30).unwrap()
        );
    }

    #[test]
    fn test_monthly_last_friday() {
        let start = dt(2026, 1, 30, 15, 0); // Last Friday of Jan
        let end = dt(2026, 1, 30, 16, 0);
        let window_start = dt(2026, 2, 1, 0, 0);
        let window_end = dt(2026, 5, 1, 0, 0);

        let results = expand_rrule(
            start,
            end,
            "FREQ=MONTHLY;BYDAY=-1FR",
            &[],
            window_start,
            window_end,
        );
        // Last Friday: Feb 27, Mar 27, Apr 24
        assert_eq!(results.len(), 3);
        assert_eq!(
            results[0].0.date(),
            NaiveDate::from_ymd_opt(2026, 2, 27).unwrap()
        );
        assert_eq!(
            results[1].0.date(),
            NaiveDate::from_ymd_opt(2026, 3, 27).unwrap()
        );
        assert_eq!(
            results[2].0.date(),
            NaiveDate::from_ymd_opt(2026, 4, 24).unwrap()
        );
    }

    #[test]
    fn test_monthly_same_day() {
        // Monthly on the 15th
        let start = dt(2026, 1, 15, 9, 0);
        let end = dt(2026, 1, 15, 10, 0);
        let window_start = dt(2026, 3, 1, 0, 0);
        let window_end = dt(2026, 6, 1, 0, 0);

        let results = expand_rrule(
            start,
            end,
            "FREQ=MONTHLY;INTERVAL=1",
            &[],
            window_start,
            window_end,
        );
        assert_eq!(results.len(), 3); // Mar 15, Apr 15, May 15
        assert_eq!(
            results[0].0.date(),
            NaiveDate::from_ymd_opt(2026, 3, 15).unwrap()
        );
        assert_eq!(
            results[1].0.date(),
            NaiveDate::from_ymd_opt(2026, 4, 15).unwrap()
        );
        assert_eq!(
            results[2].0.date(),
            NaiveDate::from_ymd_opt(2026, 5, 15).unwrap()
        );
    }

    #[test]
    fn test_monthly_feb_30_skipped() {
        // Monthly on the 31st — Feb has no 31st
        let start = dt(2026, 1, 31, 9, 0);
        let end = dt(2026, 1, 31, 10, 0);
        let window_start = dt(2026, 2, 1, 0, 0);
        let window_end = dt(2026, 4, 1, 0, 0);

        let results = expand_rrule(
            start,
            end,
            "FREQ=MONTHLY;INTERVAL=1",
            &[],
            window_start,
            window_end,
        );
        // Feb 31 doesn't exist → skipped; Mar 31 exists
        assert_eq!(results.len(), 1);
        assert_eq!(
            results[0].0.date(),
            NaiveDate::from_ymd_opt(2026, 3, 31).unwrap()
        );
    }

    #[test]
    fn test_invalid_rrule() {
        let start = dt(2026, 3, 1, 9, 0);
        let end = dt(2026, 3, 1, 10, 0);
        let results = expand_rrule(start, end, "FREQ=YEARLY", &[], start, end);
        assert!(results.is_empty()); // YEARLY not supported
    }

    #[test]
    fn test_empty_rrule() {
        let start = dt(2026, 3, 1, 9, 0);
        let end = dt(2026, 3, 1, 10, 0);
        let results = expand_rrule(start, end, "", &[], start, end);
        assert!(results.is_empty());
    }

    // --- nth_weekday_of_month ---

    #[test]
    fn test_first_monday_march_2026() {
        assert_eq!(
            nth_weekday_of_month(2026, 3, Weekday::Mon, 1),
            Some(NaiveDate::from_ymd_opt(2026, 3, 2).unwrap())
        );
    }

    #[test]
    fn test_second_monday_march_2026() {
        assert_eq!(
            nth_weekday_of_month(2026, 3, Weekday::Mon, 2),
            Some(NaiveDate::from_ymd_opt(2026, 3, 9).unwrap())
        );
    }

    #[test]
    fn test_last_friday_march_2026() {
        assert_eq!(
            nth_weekday_of_month(2026, 3, Weekday::Fri, -1),
            Some(NaiveDate::from_ymd_opt(2026, 3, 27).unwrap())
        );
    }

    #[test]
    fn test_fifth_monday_march_2026_none() {
        // March 2026 has 5 Mondays (2,9,16,23,30) so 5th exists
        assert_eq!(
            nth_weekday_of_month(2026, 3, Weekday::Mon, 5),
            Some(NaiveDate::from_ymd_opt(2026, 3, 30).unwrap())
        );
        // But 6th doesn't
        assert_eq!(nth_weekday_of_month(2026, 3, Weekday::Mon, 6), None);
    }

    // --- extract_exdates with multiple formats ---

    #[test]
    fn test_extract_exdates_with_timezone() {
        let ical = "BEGIN:VEVENT\nDTSTART:20260302T100000\nRRULE:FREQ=WEEKLY\nEXDATE;TZID=Europe/Paris:20260309T100000\nEND:VEVENT";
        let exdates = extract_exdates(ical, None);
        assert_eq!(exdates.len(), 1);
        assert_eq!(exdates[0], dt(2026, 3, 9, 10, 0));
    }

    /// #283: a quoted TZID holding ':' used to make the date unreadable.
    #[test]
    fn test_extract_exdates_quoted_tzid_with_colon() {
        let ical = "BEGIN:VEVENT\nDTSTART:20260302T100000\nRRULE:FREQ=WEEKLY\nEXDATE;TZID=\"(UTC+01:00) Paris\":20260309T100000\nEND:VEVENT\nBEGIN:VEVENT\nRECURRENCE-ID;TZID=\"(UTC+01:00) Paris\":20260316T100000\nEND:VEVENT";
        let exdates = extract_exdates(ical, None);
        assert_eq!(exdates, vec![dt(2026, 3, 9, 10, 0), dt(2026, 3, 16, 10, 0)]);
    }

    /// An EXDATE in UTC cancels the occurrence it names in the event's own
    /// timezone: 23:30Z on the 8th is 00:30 on the 9th in Paris.
    #[test]
    fn test_extract_exdates_utc_converted_to_event_tz() {
        let ical = "BEGIN:VEVENT\nDTSTART;TZID=Europe/Paris:20260302T003000\nRRULE:FREQ=WEEKLY\nEXDATE:20260308T233000Z\nEND:VEVENT";
        assert_eq!(
            extract_exdates(ical, Some("Europe/Paris")),
            vec![dt(2026, 3, 9, 0, 30)]
        );
        let occurrences = expand_rrule(
            dt(2026, 3, 2, 0, 30),
            dt(2026, 3, 2, 1, 0),
            "FREQ=WEEKLY",
            &extract_exdates(ical, Some("Europe/Paris")),
            dt(2026, 3, 1, 0, 0),
            dt(2026, 3, 20, 0, 0),
        );
        let starts: Vec<_> = occurrences.iter().map(|o| o.0).collect();
        assert_eq!(starts, vec![dt(2026, 3, 2, 0, 30), dt(2026, 3, 16, 0, 30)]);
    }

    /// A RECURRENCE-ID under another TZID, or a Windows name, is converted too.
    #[test]
    fn test_extract_exdates_other_tzid_converted() {
        let ical = "BEGIN:VEVENT\nDTSTART;TZID=Europe/Paris:20260302T160000\nRRULE:FREQ=WEEKLY\nEND:VEVENT\nBEGIN:VEVENT\nRECURRENCE-ID;TZID=\"Eastern Standard Time\":20260309T110000\nEND:VEVENT";
        assert_eq!(
            extract_exdates(ical, Some("Europe/Paris")),
            vec![dt(2026, 3, 9, 16, 0)]
        );
    }

    /// The DTSTART's own custom TZID does not resolve by name (it was found
    /// through the VTIMEZONE at sync), so its exceptions stay as written; so do
    /// every exception of a floating event.
    #[test]
    fn test_extract_exdates_unresolved_or_floating_kept() {
        let custom = "BEGIN:VEVENT\nDTSTART;TZID=Customized Time Zone:20260302T100000\nRRULE:FREQ=WEEKLY\nEXDATE;TZID=Customized Time Zone:20260309T100000\nEND:VEVENT";
        assert_eq!(
            extract_exdates(custom, Some("America/New_York")),
            vec![dt(2026, 3, 9, 10, 0)]
        );
        let floating = "BEGIN:VEVENT\nDTSTART:20260302T100000\nRRULE:FREQ=WEEKLY\nEXDATE:20260309T100000Z\nEND:VEVENT";
        assert_eq!(extract_exdates(floating, None), vec![dt(2026, 3, 9, 10, 0)]);
    }

    #[test]
    fn test_extract_exdates_comma_separated() {
        let ical =
            "BEGIN:VEVENT\nEXDATE:20260309T100000,20260316T100000,20260323T100000\nEND:VEVENT";
        let exdates = extract_exdates(ical, None);
        assert_eq!(exdates.len(), 3);
    }

    /// #267: a long EXDATE list arrives folded over several lines.
    #[test]
    fn test_extract_exdates_folded() {
        let ical = "BEGIN:VEVENT\r\nEXDATE:20260309T100000,2026031\r\n 6T100000,20260323T100000\r\nEND:VEVENT";
        let exdates = extract_exdates(ical, None);
        assert_eq!(exdates.len(), 3);
        assert_eq!(exdates[1], dt(2026, 3, 16, 10, 0));
    }

    // --- week_start ---

    #[test]
    fn test_week_start_monday() {
        let monday = NaiveDate::from_ymd_opt(2026, 3, 9).unwrap(); // Monday
        assert_eq!(week_start(monday), monday);
    }

    #[test]
    fn test_week_start_sunday() {
        let sunday = NaiveDate::from_ymd_opt(2026, 3, 15).unwrap(); // Sunday
        assert_eq!(
            week_start(sunday),
            NaiveDate::from_ymd_opt(2026, 3, 9).unwrap()
        );
    }

    #[test]
    fn test_week_start_wednesday() {
        let wed = NaiveDate::from_ymd_opt(2026, 3, 11).unwrap(); // Wednesday
        assert_eq!(
            week_start(wed),
            NaiveDate::from_ymd_opt(2026, 3, 9).unwrap()
        );
    }

    // --- #376: series started long before the window ---

    /// The same walk without skipping ahead and without any cap: every
    /// period from DTSTART on, to check that skipping changes nothing.
    fn walk_from_dtstart(
        event_start: NaiveDateTime,
        event_end: NaiveDateTime,
        rrule_str: &str,
        exdates: &[NaiveDateTime],
        window_start: NaiveDateTime,
        window_end: NaiveDateTime,
    ) -> Vec<(NaiveDateTime, NaiveDateTime)> {
        let rrule = parse_rrule(rrule_str).unwrap();
        let duration = event_end - event_start;
        let mut results = Vec::new();
        let mut seen = 0i64;
        for period in 0.. {
            for day in period_dates(&rrule, event_start, period).unwrap().1 {
                let occ_start = day.and_time(event_start.time());
                if occ_start < event_start {
                    continue;
                }
                if rrule.until.is_some_and(|u| occ_start > u) || occ_start >= window_end {
                    return results;
                }
                seen += 1;
                if rrule.count.is_some_and(|c| seen > i64::from(c)) {
                    return results;
                }
                if occ_start + duration > window_start && !is_excluded(occ_start, exdates) {
                    results.push((occ_start, occ_start + duration));
                }
            }
        }
        unreachable!()
    }

    fn dates(results: &[(NaiveDateTime, NaiveDateTime)]) -> Vec<NaiveDate> {
        results.iter().map(|(s, _)| s.date()).collect()
    }

    fn day(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    #[test]
    fn daily_series_started_years_ago_still_blocks_today() {
        // Daily stand-up since 2019, one occurrence cancelled and one moved
        // in the week asked for. Walking from DTSTART with the old cap of
        // 2000 occurrences stopped in June 2024.
        let ical = "BEGIN:VCALENDAR\n\
            BEGIN:VEVENT\n\
            UID:standup\n\
            DTSTART:20190107T091500\n\
            DTEND:20190107T093000\n\
            RRULE:FREQ=DAILY\n\
            EXDATE:20261110T091500\n\
            END:VEVENT\n\
            BEGIN:VEVENT\n\
            UID:standup\n\
            RECURRENCE-ID:20261112T091500\n\
            DTSTART:20261112T140000\n\
            DTEND:20261112T141500\n\
            END:VEVENT\n\
            END:VCALENDAR";
        let exdates = extract_exdates(ical, None);
        let results = expand_rrule(
            dt(2019, 1, 7, 9, 15),
            dt(2019, 1, 7, 9, 30),
            "FREQ=DAILY",
            &exdates,
            dt(2026, 11, 9, 0, 0),
            dt(2026, 11, 16, 0, 0),
        );
        assert_eq!(
            dates(&results),
            vec![
                day(2026, 11, 9),
                day(2026, 11, 11),
                day(2026, 11, 13),
                day(2026, 11, 14),
                day(2026, 11, 15),
            ]
        );
        assert_eq!(results[0], (dt(2026, 11, 9, 9, 15), dt(2026, 11, 9, 9, 30)));
    }

    #[test]
    fn weekly_series_started_years_ago_still_blocks_today() {
        // Every weekday since 2018 (five occurrences a week, so the old cap
        // was spent by 2025), and a Monday-only series since 1990.
        let ical = "BEGIN:VEVENT\n\
            DTSTART:20180101T100000\n\
            RRULE:FREQ=WEEKLY;BYDAY=MO,TU,WE,TH,FR\n\
            EXDATE:20261111T100000\n\
            END:VEVENT\n\
            BEGIN:VEVENT\n\
            RECURRENCE-ID:20261113T100000\n\
            DTSTART:20261113T160000\n\
            END:VEVENT";
        let exdates = extract_exdates(ical, None);
        let results = expand_rrule(
            dt(2018, 1, 1, 10, 0),
            dt(2018, 1, 1, 11, 0),
            "FREQ=WEEKLY;BYDAY=MO,TU,WE,TH,FR",
            &exdates,
            dt(2026, 11, 9, 0, 0),
            dt(2026, 11, 16, 0, 0),
        );
        assert_eq!(
            dates(&results),
            vec![day(2026, 11, 9), day(2026, 11, 10), day(2026, 11, 12)]
        );

        let results = expand_rrule(
            dt(1990, 1, 1, 8, 0),
            dt(1990, 1, 1, 9, 0),
            "FREQ=WEEKLY",
            &[],
            dt(2026, 11, 1, 0, 0),
            dt(2026, 12, 1, 0, 0),
        );
        assert_eq!(
            dates(&results),
            vec![
                day(2026, 11, 2),
                day(2026, 11, 9),
                day(2026, 11, 16),
                day(2026, 11, 23),
                day(2026, 11, 30),
            ]
        );
    }

    #[test]
    fn monthly_series_started_long_ago_still_blocks_today() {
        let results = expand_rrule(
            dt(1970, 1, 13, 18, 0),
            dt(1970, 1, 13, 19, 0),
            "FREQ=MONTHLY;BYDAY=2TU",
            &[],
            dt(2026, 11, 1, 0, 0),
            dt(2027, 1, 1, 0, 0),
        );
        assert_eq!(dates(&results), vec![day(2026, 11, 10), day(2026, 12, 8)]);
    }

    #[test]
    fn count_still_ends_a_series_skipped_ahead() {
        // The series' last occurrence is 11 November 2026: COUNT is the
        // number of days from DTSTART to it, inclusive.
        let start = dt(2019, 1, 7, 9, 0);
        let count = (day(2026, 11, 11) - start.date()).num_days() + 1;
        let rule = format!("FREQ=DAILY;COUNT={count}");
        let window = (dt(2026, 11, 9, 0, 0), dt(2026, 11, 16, 0, 0));
        let results = expand_rrule(
            start,
            start + Duration::hours(1),
            &rule,
            &[],
            window.0,
            window.1,
        );
        assert_eq!(
            dates(&results),
            vec![day(2026, 11, 9), day(2026, 11, 10), day(2026, 11, 11)]
        );
        // A cancelled occurrence still counts: it does not push the end out.
        let results = expand_rrule(
            start,
            start + Duration::hours(1),
            &rule,
            &[dt(2026, 11, 10, 9, 0), dt(2020, 2, 3, 9, 0)],
            window.0,
            window.1,
        );
        assert_eq!(dates(&results), vec![day(2026, 11, 9), day(2026, 11, 11)]);
    }

    #[test]
    fn until_still_ends_a_series_skipped_ahead() {
        let results = expand_rrule(
            dt(2018, 1, 1, 10, 0),
            dt(2018, 1, 1, 11, 0),
            "FREQ=WEEKLY;BYDAY=MO,WE,FR;UNTIL=20261111T100000",
            &[],
            dt(2026, 11, 9, 0, 0),
            dt(2026, 11, 16, 0, 0),
        );
        assert_eq!(dates(&results), vec![day(2026, 11, 9), day(2026, 11, 11)]);
        let results = expand_rrule(
            dt(2018, 1, 1, 10, 0),
            dt(2018, 1, 1, 11, 0),
            "FREQ=DAILY;UNTIL=20240101",
            &[],
            dt(2026, 11, 9, 0, 0),
            dt(2026, 11, 16, 0, 0),
        );
        assert!(results.is_empty());
    }

    #[test]
    fn skipping_ahead_matches_walking_from_dtstart() {
        let starts = [
            dt(2019, 1, 7, 9, 0),
            dt(2019, 1, 9, 23, 30),
            dt(2020, 2, 29, 0, 0),
            dt(2021, 1, 31, 12, 0),
        ];
        let rules = [
            "FREQ=DAILY",
            "FREQ=DAILY;INTERVAL=3",
            "FREQ=DAILY;INTERVAL=10;COUNT=250",
            "FREQ=DAILY;COUNT=2861",
            "FREQ=WEEKLY",
            "FREQ=WEEKLY;INTERVAL=2;BYDAY=TU,TH",
            "FREQ=WEEKLY;BYDAY=FR,MO,WE;COUNT=1200",
            "FREQ=WEEKLY;INTERVAL=3;BYDAY=SU,SA;COUNT=301",
            "FREQ=WEEKLY;BYDAY=MO;UNTIL=20261118T000000",
            "FREQ=MONTHLY",
            "FREQ=MONTHLY;INTERVAL=5",
            "FREQ=MONTHLY;BYDAY=-1FR,1MO;COUNT=166",
            "FREQ=MONTHLY;INTERVAL=2;BYDAY=WE",
            "FREQ=MONTHLY;BYDAY=5TH",
        ];
        let windows = [
            (dt(2026, 11, 9, 0, 0), dt(2026, 11, 16, 0, 0)),
            (dt(2026, 11, 10, 9, 30), dt(2026, 11, 10, 10, 0)),
            (dt(2026, 1, 1, 0, 0), dt(2027, 1, 1, 0, 0)),
            (dt(2019, 1, 1, 0, 0), dt(2019, 3, 1, 0, 0)),
        ];
        let exdates = [dt(2026, 11, 11, 9, 0), dt(2026, 3, 6, 23, 30)];
        for start in starts {
            for rule in rules {
                for (ws, we) in windows {
                    for duration in [Duration::minutes(45), Duration::hours(30)] {
                        let end = start + duration;
                        assert_eq!(
                            expand_rrule(start, end, rule, &exdates, ws, we),
                            walk_from_dtstart(start, end, rule, &exdates, ws, we),
                            "{rule} from {start}, {duration} long, in {ws} to {we}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn work_is_bounded_by_the_window() {
        // A series from year 1 is expanded at the window, not walked to it.
        let results = expand_rrule(
            dt(1, 1, 1, 9, 0),
            dt(1, 1, 1, 10, 0),
            "FREQ=DAILY",
            &[],
            dt(2026, 11, 9, 0, 0),
            dt(2026, 11, 10, 0, 0),
        );
        assert_eq!(
            results,
            vec![(dt(2026, 11, 9, 9, 0), dt(2026, 11, 9, 10, 0))]
        );
        // A window of centuries stops at the per-window cap.
        let results = expand_rrule(
            dt(2026, 1, 1, 9, 0),
            dt(2026, 1, 1, 10, 0),
            "FREQ=DAILY",
            &[],
            dt(2026, 1, 1, 0, 0),
            dt(2526, 1, 1, 0, 0),
        );
        assert_eq!(results.len() as i64, MAX_PERIODS);
        // Absurd values end the walk instead of overflowing.
        assert!(
            expand_rrule(
                dt(2026, 1, 1, 9, 0),
                dt(2026, 1, 1, 10, 0),
                "FREQ=DAILY;INTERVAL=4294967295",
                &[],
                dt(2026, 1, 1, 0, 0),
                NaiveDateTime::MAX,
            )
            .len()
                == 1
        );
        assert!(expand_rrule(
            dt(2026, 1, 1, 9, 0),
            dt(2026, 1, 1, 10, 0),
            "FREQ=MONTHLY;BYDAY=2147483647MO",
            &[],
            dt(2026, 1, 1, 0, 0),
            dt(2027, 1, 1, 0, 0),
        )
        .is_empty());
    }

    #[test]
    fn weekly_days_are_walked_in_calendar_order() {
        // BYDAY lists Friday first: a window ending on Wednesday must still
        // get its Monday.
        let results = expand_rrule(
            dt(2026, 1, 5, 10, 0),
            dt(2026, 1, 5, 11, 0),
            "FREQ=WEEKLY;BYDAY=FR,MO",
            &[],
            dt(2026, 11, 9, 0, 0),
            dt(2026, 11, 11, 0, 0),
        );
        assert_eq!(dates(&results), vec![day(2026, 11, 9)]);
    }

    #[test]
    fn monthly_bare_weekday_is_every_one_of_them() {
        let results = expand_rrule(
            dt(2026, 1, 5, 10, 0),
            dt(2026, 1, 5, 11, 0),
            "FREQ=MONTHLY;BYDAY=MO",
            &[],
            dt(2026, 11, 1, 0, 0),
            dt(2026, 12, 1, 0, 0),
        );
        assert_eq!(
            dates(&results),
            vec![
                day(2026, 11, 2),
                day(2026, 11, 9),
                day(2026, 11, 16),
                day(2026, 11, 23),
                day(2026, 11, 30),
            ]
        );
        // -2 is the second to last.
        let results = expand_rrule(
            dt(2026, 1, 5, 10, 0),
            dt(2026, 1, 5, 11, 0),
            "FREQ=MONTHLY;BYDAY=-2MO",
            &[],
            dt(2026, 11, 1, 0, 0),
            dt(2026, 12, 1, 0, 0),
        );
        assert_eq!(dates(&results), vec![day(2026, 11, 23)]);
    }

    #[test]
    fn occurrences_are_read_in_the_hosts_day() {
        // 20:00 in New York is 02:00 the next day in Paris: the Paris day
        // of 10 November holds the occurrence of 9 November, not of the
        // 10th, which the expansion window in New York terms used to give.
        let paris: Tz = "Europe/Paris".parse().unwrap();
        let busy = occurrences_in(
            "20190107T200000",
            "20190107T203000",
            "FREQ=DAILY",
            None,
            Some("America/New_York"),
            dt(2026, 11, 10, 0, 0),
            dt(2026, 11, 11, 0, 0),
            paris,
        );
        assert_eq!(
            busy,
            vec![(dt(2026, 11, 10, 2, 0), dt(2026, 11, 10, 2, 30))]
        );
        // EXDATE in UTC cancels the same occurrence.
        let ical = "BEGIN:VEVENT\nDTSTART;TZID=America/New_York:20190107T200000\nEXDATE:20261110T010000Z\nEND:VEVENT";
        let busy = occurrences_in(
            "20190107T200000",
            "20190107T203000",
            "FREQ=DAILY",
            Some(ical),
            Some("America/New_York"),
            dt(2026, 11, 10, 0, 0),
            dt(2026, 11, 11, 0, 0),
            paris,
        );
        assert!(busy.is_empty(), "{busy:?}");
    }
}
