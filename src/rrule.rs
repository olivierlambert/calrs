use chrono::{Datelike, Duration, NaiveDate, NaiveDateTime, NaiveTime, TimeZone, Weekday};
use chrono_tz::Tz;

/// A parsed RRULE.
struct RRule {
    freq: Freq,
    interval: u32,
    /// In the event's own wall-clock time: [`parse_rrule_in`] reads a UTC
    /// `UNTIL` into it.
    until: Option<NaiveDateTime>,
    count: Option<u32>,
    by_day: Vec<ByDay>,
    /// 1 to 12.
    by_month: Vec<u32>,
    /// 1 to 31, or -1 (the last day) to -31.
    by_month_day: Vec<i32>,
    /// Positions kept of each period's sorted candidates, 1 the first, -1
    /// the last.
    by_set_pos: Vec<i32>,
    week_start: Weekday,
}

#[derive(Clone, Copy)]
enum Freq {
    Daily,
    Weekly,
    Monthly,
    Yearly,
}

#[derive(Clone)]
struct ByDay {
    weekday: Weekday,
    nth: Option<i32>, // e.g. 2 for "2nd Monday", -1 for "last Friday"
}

/// A rule calrs cannot expand (a sub-daily `FREQ`, `BYHOUR`, `BYWEEKNO`,
/// no `FREQ` at all): what is known of where it ends, so the series can be
/// blocked as busy from DTSTART to there rather than read as free.
struct Unreadable {
    /// Every occurrence keeps DTSTART's time of day (the rule is daily or
    /// longer, and has no `BYHOUR`, `BYMINUTE` or `BYSECOND`), so only that
    /// time is blocked, each day; otherwise the whole time from DTSTART.
    time_kept: bool,
    until: Option<NaiveDateTime>,
    /// The latest an occurrence can start after DTSTART, when the rule has
    /// a `COUNT` and nothing that could thin its occurrences out.
    last_start: Option<Duration>,
}

/// Most recurrence periods (days, weeks, months or years) walked for one
/// window, counted from the first period that can reach it, not from
/// DTSTART (#376). A year of a daily series is 367 of them.
const MAX_PERIODS: i64 = 20_000;

/// Most periods walked from DTSTART to count a `COUNT` toward the window,
/// for the rules whose skipped occurrences cannot be counted arithmetically
/// (a daily series filtered by `BYDAY`, a monthly or yearly one with a
/// `COUNT`). 547 years of a daily series.
const MAX_COUNTED_PERIODS: i64 = 200_000;

/// Whether calrs can expand this RRULE. One it cannot is blocked as busy
/// over each window asked for (see [`expand_rrule`]).
pub fn is_supported(rrule_str: &str) -> bool {
    rrule_str.trim().is_empty() || parse_rrule(rrule_str).is_ok()
}

/// Log, once per event per sync, a recurring event calrs blocks rather
/// than expands: a rule it cannot read, or one whose `COUNT` it would have
/// to count over more than [`MAX_COUNTED_PERIODS`] periods to reach the
/// coming year. `dtstart` is the series' stored start; `container` the id
/// of the calendar or shared resource it was synced into.
pub fn warn_if_unexpandable(rrule_str: &str, dtstart: &str, uid: &str, container: &str) {
    if rrule_str.trim().is_empty() {
        return;
    }
    let Ok(rule) = parse_rrule(rrule_str) else {
        tracing::warn!(
            %uid,
            rrule = %rrule_str,
            %container,
            "recurrence rule calrs cannot expand: the event is blocked as busy from its start"
        );
        return;
    };
    let Some(start) = crate::utils::parse_ical_datetime(dtstart) else {
        return;
    };
    let horizon = chrono::Utc::now().naive_utc() + Duration::days(400);
    if skip_to(&rule, start, horizon).is_none()
        && periods_between(&rule, start, horizon) > MAX_COUNTED_PERIODS
    {
        tracing::warn!(
            %uid,
            rrule = %rrule_str,
            %container,
            "recurrence too long to count to today: the event is blocked as busy"
        );
    }
}

/// Expand a recurring event into occurrences within [window_start, window_end).
/// Returns (start, end) pairs for each occurrence.
///
/// The walk starts at the window, however long ago the series began. When
/// the occurrences skipped can be counted arithmetically (no `COUNT`, or a
/// daily or weekly series no `BY*` part thins out), the walk skips straight
/// to the window; otherwise it counts from DTSTART, up to
/// [`MAX_COUNTED_PERIODS`]. Excluded occurrences count toward `COUNT`, as
/// RFC 5545 says. A rule calrs cannot expand, or a walk that runs out of
/// periods before the window ends, blocks what it could not read as one
/// busy interval: it never reads as free.
pub fn expand_rrule(
    event_start: NaiveDateTime,
    event_end: NaiveDateTime,
    rrule_str: &str,
    exdates: &[NaiveDateTime],
    window_start: NaiveDateTime,
    window_end: NaiveDateTime,
) -> Vec<(NaiveDateTime, NaiveDateTime)> {
    expand_rrule_in(
        event_start,
        event_end,
        rrule_str,
        exdates,
        window_start,
        window_end,
        None,
    )
}

/// [`expand_rrule`] for an event whose wall-clock times are in `event_tz`
/// (None: floating), which a UTC `UNTIL` (`...Z`) is read into before it
/// is compared with the occurrences.
pub fn expand_rrule_in(
    event_start: NaiveDateTime,
    event_end: NaiveDateTime,
    rrule_str: &str,
    exdates: &[NaiveDateTime],
    window_start: NaiveDateTime,
    window_end: NaiveDateTime,
    event_tz: Option<Tz>,
) -> Vec<(NaiveDateTime, NaiveDateTime)> {
    if rrule_str.trim().is_empty() {
        return vec![];
    }
    let duration = (event_end - event_start).max(Duration::zero());
    let rrule = match parse_rrule_in(rrule_str, event_tz) {
        Ok(r) => r,
        Err(unreadable) => {
            return unreadable_block(event_start, duration, &unreadable, window_start, window_end)
        }
    };
    // The earliest start an occurrence still reaching the window can have.
    let reach = window_start
        .checked_sub_signed(duration)
        .unwrap_or(window_start);
    let (first_period, skipped, cap) = match skip_to(&rrule, event_start, reach) {
        Some((first, skipped)) => (first, skipped, MAX_PERIODS),
        None => (0, 0, MAX_COUNTED_PERIODS),
    };

    let mut results = Vec::new();
    // DTSTART is always the first instance (RFC 5545), and counts toward
    // COUNT, even when the rule itself would not produce it.
    let dtstart_extra = !period_dates(&rrule, event_start, 0)
        .is_some_and(|(_, days)| days.contains(&event_start.date()));
    if dtstart_extra
        && event_start < window_end
        && event_start + duration > window_start
        && !is_excluded(event_start, exdates)
    {
        results.push((event_start, event_start + duration));
    }
    let mut seen = skipped + i64::from(dtstart_extra);
    let mut period = first_period;
    let last = first_period.saturating_add(cap);
    'periods: while period < last {
        let Some((first_day, days)) = period_dates(&rrule, event_start, period) else {
            return results;
        };
        // Past the window or the series' end: so is every later period,
        // even when this one holds no occurrence (a 5th Monday, a 31st).
        let first = first_day.and_time(NaiveTime::MIN);
        if first >= window_end || rrule.until.is_some_and(|until| first > until) {
            return results;
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
        period += 1;
    }
    if period >= last {
        // Out of periods before the window or the series ended: what was
        // not walked is busy, not free. Sync warns about such a series
        // ([`warn_if_unexpandable`]), not each slot request.
        let from = period_dates(&rrule, event_start, period)
            .map(|(first_day, _)| first_day.and_time(NaiveTime::MIN).max(window_start))
            .unwrap_or(window_start);
        let mut until = window_end;
        if let Some(series_until) = rrule.until {
            until = until.min(
                series_until
                    .checked_add_signed(duration)
                    .unwrap_or(series_until),
            );
        }
        if from < until {
            results.push((from, until));
        }
    }
    results
}

/// What a series whose rule calrs cannot expand blocks, from DTSTART up to
/// the rule's `UNTIL` or the last start its `COUNT` allows: when every
/// occurrence keeps DTSTART's time of day, that time on every day;
/// otherwise all the time from DTSTART to the window's end.
fn unreadable_block(
    event_start: NaiveDateTime,
    duration: Duration,
    unreadable: &Unreadable,
    window_start: NaiveDateTime,
    window_end: NaiveDateTime,
) -> Vec<(NaiveDateTime, NaiveDateTime)> {
    let last_start = [
        unreadable.until,
        unreadable
            .last_start
            .and_then(|span| event_start.checked_add_signed(span)),
    ]
    .into_iter()
    .flatten()
    .min();
    if unreadable.time_kept {
        let mut busy = Vec::new();
        let reach = window_start
            .checked_sub_signed(duration)
            .unwrap_or(window_start);
        let mut day = event_start.date().max(reach.date());
        for _ in 0..MAX_PERIODS {
            let start = day.and_time(event_start.time());
            if start >= window_end || last_start.is_some_and(|last| start > last) {
                return busy;
            }
            let Some(end) = start.checked_add_signed(duration) else {
                return busy;
            };
            if start >= event_start && end > window_start {
                busy.push((start, end));
            }
            let Some(next) = day.succ_opt() else {
                return busy;
            };
            day = next;
        }
        // A window longer than the cap: the rest of it is busy.
        let from = day.and_time(NaiveTime::MIN).max(window_start);
        if from < window_end {
            busy.push((from, window_end));
        }
        return busy;
    }
    let from = event_start.max(window_start);
    let to = last_start.map_or(window_end, |last| {
        window_end.min(last.checked_add_signed(duration).unwrap_or(last))
    });
    if from < to {
        vec![(from, to)]
    } else {
        vec![]
    }
}

/// Whether a rule's `BY*` parts can drop some of its periods' candidates,
/// so the occurrences before the window cannot be counted arithmetically.
fn thins_out(rrule: &RRule) -> bool {
    match rrule.freq {
        Freq::Daily => {
            !rrule.by_day.is_empty()
                || !rrule.by_month.is_empty()
                || !rrule.by_month_day.is_empty()
                || !rrule.by_set_pos.is_empty()
        }
        Freq::Weekly => {
            !rrule.by_month.is_empty()
                || !rrule.by_month_day.is_empty()
                || !rrule.by_set_pos.is_empty()
        }
        Freq::Monthly | Freq::Yearly => true,
    }
}

/// The first period worth walking for occurrences starting at `reach` or
/// later, and how many occurrences the periods before it hold; None when
/// those cannot be counted without walking them (a `COUNT` on a rule that
/// [`thins_out`]). Every occurrence of a skipped period starts before
/// `reach`, so it cannot reach the window: one period of margin is kept for
/// the time of day.
fn skip_to(rrule: &RRule, event_start: NaiveDateTime, reach: NaiveDateTime) -> Option<(i64, i64)> {
    let skip = (periods_between(rrule, event_start, reach) - 1).max(0);
    if rrule.count.is_none() || skip == 0 {
        return Some((skip, 0));
    }
    if thins_out(rrule) {
        return None;
    }
    match rrule.freq {
        Freq::Daily => Some((skip, skip)),
        Freq::Weekly => {
            // The first week holds the days on or after DTSTART, every
            // later week all of them.
            let first = period_dates(rrule, event_start, 0)
                .map(|(_, days)| days)
                .unwrap_or_default()
                .iter()
                .filter(|d| d.and_time(event_start.time()) >= event_start)
                .count() as i64;
            let per_week = weekly_offsets(rrule, event_start).len() as i64;
            Some((skip, first + (skip - 1) * per_week))
        }
        Freq::Monthly | Freq::Yearly => None,
    }
}

/// Whole periods (of `INTERVAL` units) from DTSTART's to `reach`'s.
fn periods_between(rrule: &RRule, event_start: NaiveDateTime, reach: NaiveDateTime) -> i64 {
    let start = event_start.date();
    let units = match rrule.freq {
        Freq::Daily => (reach.date() - start).num_days(),
        Freq::Weekly => {
            let wkst = rrule.week_start;
            (week_start_on(reach.date(), wkst) - week_start_on(start, wkst)).num_days() / 7
        }
        Freq::Monthly => month_index(reach.date()) - month_index(start),
        Freq::Yearly => i64::from(reach.year()) - i64::from(start.year()),
    };
    units.div_euclid(i64::from(rrule.interval))
}

/// The first day of one period of the series and its dates, in order:
/// period 0 is the day, week, month or year of DTSTART, period n the one
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
            let keep = in_months(rrule, day)
                && (rrule.by_month_day.is_empty()
                    || rrule
                        .by_month_day
                        .iter()
                        .any(|&md| month_day(day.year(), day.month(), md) == Some(day)))
                && (rrule.by_day.is_empty() || on_listed_weekday(rrule, day));
            let days = if keep { vec![day] } else { vec![] };
            Some((day, set_positions(rrule, days)))
        }
        Freq::Weekly => {
            let week = week_start_on(start, rrule.week_start)
                .checked_add_signed(Duration::try_weeks(units)?)?;
            let days = weekly_offsets(rrule, event_start)
                .into_iter()
                .map(|offset| week.checked_add_signed(Duration::days(offset)))
                .collect::<Option<Vec<_>>>()?;
            let days = days
                .into_iter()
                .filter(|&d| in_months(rrule, d) && on_listed_month_day(rrule, d))
                .collect();
            Some((week, set_positions(rrule, days)))
        }
        Freq::Monthly => {
            let index = month_index(start).checked_add(units)?;
            let year = i32::try_from(index.div_euclid(12)).ok()?;
            let month = index.rem_euclid(12) as u32 + 1;
            let first_day = NaiveDate::from_ymd_opt(year, month, 1)?;
            let days = if rrule.by_month.is_empty() || rrule.by_month.contains(&month) {
                month_dates(rrule, start, year, month)
            } else {
                vec![]
            };
            Some((first_day, set_positions(rrule, days)))
        }
        Freq::Yearly => {
            let year = i32::try_from(i64::from(start.year()).checked_add(units)?).ok()?;
            let first_day = NaiveDate::from_ymd_opt(year, 1, 1)?;
            let mut days: Vec<NaiveDate> = if !rrule.by_month.is_empty() {
                rrule
                    .by_month
                    .iter()
                    .flat_map(|&month| month_dates(rrule, start, year, month))
                    .collect()
            } else if !rrule.by_month_day.is_empty() {
                // BYDAY limits the days of the year, counted in the year.
                let weekdays = (!rrule.by_day.is_empty()).then(|| year_weekdays(rrule, year));
                (1..=12)
                    .flat_map(|month| {
                        rrule
                            .by_month_day
                            .iter()
                            .filter_map(move |&md| month_day(year, month, md))
                    })
                    .filter(|d| weekdays.as_ref().is_none_or(|w| w.contains(d)))
                    .collect()
            } else if !rrule.by_day.is_empty() {
                year_weekdays(rrule, year)
            } else {
                // DTSTART's day; none in a year without it (29 February).
                NaiveDate::from_ymd_opt(year, start.month(), start.day())
                    .into_iter()
                    .collect()
            };
            days.sort();
            days.dedup();
            Some((first_day, set_positions(rrule, days)))
        }
    }
}

/// `BYSETPOS`: the positions kept of one period's sorted candidates.
fn set_positions(rrule: &RRule, days: Vec<NaiveDate>) -> Vec<NaiveDate> {
    if rrule.by_set_pos.is_empty() {
        return days;
    }
    let len = days.len() as i64;
    let mut kept: Vec<NaiveDate> = rrule
        .by_set_pos
        .iter()
        .filter_map(|&pos| {
            let index = if pos > 0 {
                i64::from(pos) - 1
            } else {
                len + i64::from(pos)
            };
            usize::try_from(index)
                .ok()
                .and_then(|i| days.get(i))
                .copied()
        })
        .collect();
    kept.sort();
    kept.dedup();
    kept
}

fn on_listed_month_day(rrule: &RRule, day: NaiveDate) -> bool {
    rrule.by_month_day.is_empty()
        || rrule
            .by_month_day
            .iter()
            .any(|&md| month_day(day.year(), day.month(), md) == Some(day))
}

/// The dates of one month of a monthly series, or of a yearly one in a
/// month it covers, in order: `BYMONTHDAY` (only those on a `BYDAY`
/// weekday, when both are given), else `BYDAY`, else DTSTART's day of the
/// month (none in a month too short for it).
fn month_dates(rrule: &RRule, start: NaiveDate, year: i32, month: u32) -> Vec<NaiveDate> {
    let weekdays = || -> Vec<NaiveDate> {
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
    let mut days: Vec<NaiveDate> = if !rrule.by_month_day.is_empty() {
        // BYDAY limits them, a numbered one to its own day of the month.
        let listed = (!rrule.by_day.is_empty()).then(weekdays);
        rrule
            .by_month_day
            .iter()
            .filter_map(|&md| month_day(year, month, md))
            .filter(|d| listed.as_ref().is_none_or(|w| w.contains(d)))
            .collect()
    } else if !rrule.by_day.is_empty() {
        weekdays()
    } else {
        NaiveDate::from_ymd_opt(year, month, start.day())
            .into_iter()
            .collect()
    };
    days.sort();
    days.dedup();
    days
}

/// `BYDAY` across a whole year (a yearly rule without `BYMONTH`): a bare
/// weekday is every one of them, `20MO` the 20th Monday, `-1FR` the last
/// Friday of the year.
fn year_weekdays(rrule: &RRule, year: i32) -> Vec<NaiveDate> {
    let (Some(jan1), Some(next)) = (
        NaiveDate::from_ymd_opt(year, 1, 1),
        NaiveDate::from_ymd_opt(year + 1, 1, 1),
    ) else {
        return vec![];
    };
    let mut days = Vec::new();
    for bd in &rrule.by_day {
        let all: Vec<NaiveDate> = jan1
            .iter_days()
            .take_while(|&d| d < next)
            .filter(|d| d.weekday() == bd.weekday)
            .collect();
        match bd.nth {
            None => days.extend(all),
            Some(nth) if nth > 0 => days.extend(all.get(nth as usize - 1)),
            Some(nth) => days.extend(
                all.len()
                    .checked_sub(nth.unsigned_abs() as usize)
                    .and_then(|i| all.get(i)),
            ),
        }
    }
    days
}

/// A `BYMONTHDAY` value in one month: 1 is the first, -1 the last day.
fn month_day(year: i32, month: u32, md: i32) -> Option<NaiveDate> {
    if md > 0 {
        return NaiveDate::from_ymd_opt(year, month, md as u32);
    }
    let first = NaiveDate::from_ymd_opt(year, month, 1)?;
    let next = if month == 12 {
        NaiveDate::from_ymd_opt(year + 1, 1, 1)?
    } else {
        NaiveDate::from_ymd_opt(year, month + 1, 1)?
    };
    let day = next.checked_add_signed(Duration::days(md.into()))?;
    (day >= first).then_some(day)
}

fn in_months(rrule: &RRule, day: NaiveDate) -> bool {
    rrule.by_month.is_empty() || rrule.by_month.contains(&day.month())
}

fn on_listed_weekday(rrule: &RRule, day: NaiveDate) -> bool {
    rrule.by_day.iter().any(|bd| bd.weekday == day.weekday())
}

/// The days of the week a weekly series falls on, as offsets from its week
/// start (`WKST`, Monday by default), in order: `BYDAY`, else DTSTART's
/// weekday.
fn weekly_offsets(rrule: &RRule, event_start: NaiveDateTime) -> Vec<i64> {
    let offset = |wd: Weekday| days_from(wd, rrule.week_start);
    let mut offsets: Vec<i64> = if rrule.by_day.is_empty() {
        vec![offset(event_start.weekday())]
    } else {
        rrule.by_day.iter().map(|bd| offset(bd.weekday)).collect()
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
/// RRULE, empty for a series of RDATEs alone, its raw iCal for EXDATE,
/// RDATE and RECURRENCE-ID, its timezone)
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
    let (from, to) = (
        window_start
            .checked_sub_signed(slack)
            .unwrap_or(window_start),
        window_end.checked_add_signed(slack).unwrap_or(window_end),
    );
    let duration = (ev_end - ev_start).max(Duration::zero());
    let mut occurrences = if rrule.trim().is_empty() {
        // A series of RDATEs alone: DTSTART is its first instance.
        vec![(ev_start, ev_start + duration)]
            .into_iter()
            .filter(|&(s, e)| s < to && e > from && !is_excluded(s, &exdates))
            .collect()
    } else {
        expand_rrule_in(
            ev_start,
            ev_end,
            rrule,
            &exdates,
            from,
            to,
            // A floating event (or one whose zone does not resolve) is read
            // in `target`'s time, so a UTC UNTIL is too.
            Some(
                event_tz
                    .and_then(crate::tzid::resolve_or_offset)
                    .unwrap_or(target),
            ),
        )
    };
    // Extra dates join the set before EXDATE (and moved occurrences, whose
    // RECURRENCE-ID is among `exdates`) take theirs out; they never count
    // toward COUNT.
    for rdate in raw_ical
        .map(|raw| extract_rdates(raw, event_tz))
        .unwrap_or_default()
    {
        let (s, length) = match rdate {
            RDate::Day(day) => (day.and_time(ev_start.time()), duration),
            RDate::At(s, length) => (s, length.unwrap_or(duration)),
        };
        let Some(e) = s.checked_add_signed(length) else {
            continue;
        };
        if s < to && e > from && !is_excluded(s, &exdates) && !occurrences.contains(&(s, e)) {
            occurrences.push((s, e));
        }
    }
    occurrences.sort();
    occurrences
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

/// An extra date a series' `RDATE` adds (#376).
#[derive(Debug, PartialEq)]
pub enum RDate {
    /// A `VALUE=DATE` day: the occurrence starts at DTSTART's time of day.
    Day(NaiveDate),
    /// A start in the event's timezone, with its own length when the value
    /// is a `PERIOD` (`start/end` or `start/duration`).
    At(NaiveDateTime, Option<Duration>),
}

/// Whether a VEVENT (unfolded) carries an `RDATE`.
pub fn has_rdate(vevent: &str) -> bool {
    vevent.lines().any(|line| {
        crate::utils::parse_content_line(line.trim())
            .is_some_and(|parsed| parsed.name.eq_ignore_ascii_case("RDATE"))
    })
}

/// The `RDATE` values of a series, in the event's timezone (`event_tz`, as
/// stored in `events.timezone`): from the resource's first VEVENT that is
/// not a moved occurrence. A UTC or TZID value is converted to the event's
/// zone, a floating one is taken as already in it, as for EXDATE.
pub fn extract_rdates(raw_ical: &str, event_tz: Option<&str>) -> Vec<RDate> {
    let event_tz = event_tz.and_then(crate::tzid::resolve_or_offset);
    let unfolded = crate::utils::unfold_ical(raw_ical);
    let mut rdates = Vec::new();
    let Some(series) = crate::utils::split_vevents(&unfolded)
        .into_iter()
        .find(|vevent| {
            !vevent.lines().any(|line| {
                crate::utils::parse_content_line(line.trim())
                    .is_some_and(|p| p.name.eq_ignore_ascii_case("RECURRENCE-ID"))
            })
        })
    else {
        return rdates;
    };
    for line in series.lines() {
        let Some(parsed) = crate::utils::parse_content_line(line.trim()) else {
            continue;
        };
        if !parsed.name.eq_ignore_ascii_case("RDATE") {
            continue;
        }
        let kind = parsed.param("VALUE").map(|v| v.to_ascii_uppercase());
        let tzid = parsed.param("TZID");
        for value in parsed.value.split(',').map(str::trim) {
            let rdate = match kind.as_deref() {
                Some("PERIOD") => value.split_once('/').and_then(|(from, to)| {
                    let start = parse_exdate(from, tzid, event_tz)?;
                    let length = if to.trim_start_matches('+').starts_with('P') {
                        crate::utils::ical_duration(to)?
                    } else {
                        parse_exdate(to, tzid, event_tz)? - start
                    };
                    Some(RDate::At(start, Some(length.max(Duration::zero()))))
                }),
                _ if value.len() == 8 && value.bytes().all(|b| b.is_ascii_digit()) => {
                    NaiveDate::parse_from_str(value, "%Y%m%d")
                        .ok()
                        .map(RDate::Day)
                }
                _ => parse_exdate(value, tzid, event_tz).map(|dt| RDate::At(dt, None)),
            };
            rdates.extend(rdate);
        }
    }
    rdates
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

/// Parse an RRULE, or say what is known of a rule calrs cannot expand: a
/// `FREQ` other than daily to yearly (or none), a part that adds
/// occurrences calrs would not generate (`BYHOUR`, `BYMINUTE`, `BYSECOND`,
/// `BYYEARDAY`, `BYWEEKNO`), or a `BY*` value it cannot read. Parts that
/// only remove occurrences (`BYSETPOS`) are ignored, which over-blocks
/// rather than under-blocks.
fn parse_rrule(s: &str) -> Result<RRule, Unreadable> {
    parse_rrule_in(s, None)
}

/// [`parse_rrule`], reading a UTC `UNTIL` into `event_tz` (RFC 5545: an
/// `UNTIL` with a `Z` is an instant, one without is in DTSTART's own
/// time). A floating event keeps the value as written.
fn parse_rrule_in(s: &str, event_tz: Option<Tz>) -> Result<RRule, Unreadable> {
    let s = s.trim().to_ascii_uppercase();
    let mut freq = None;
    let mut sub_daily = None;
    let mut readable = true;
    let mut thinned = false;
    let mut interval = 1u32;
    let mut until = None;
    let mut count = None;
    let mut by_day = Vec::new();
    let mut by_month = Vec::new();
    let mut by_month_day = Vec::new();
    let mut by_set_pos = Vec::new();
    let mut week_start = Weekday::Mon;
    let mut time_kept = true;

    for part in s.split(';') {
        if part.trim().is_empty() {
            continue;
        }
        let Some((name, val)) = part.split_once('=') else {
            readable = false;
            continue;
        };
        match name.trim() {
            "FREQ" => {
                freq = match val {
                    "DAILY" => Some(Freq::Daily),
                    "WEEKLY" => Some(Freq::Weekly),
                    "MONTHLY" => Some(Freq::Monthly),
                    "YEARLY" => Some(Freq::Yearly),
                    _ => None,
                };
                sub_daily = match val {
                    "HOURLY" => Some(Duration::hours(1)),
                    "MINUTELY" => Some(Duration::minutes(1)),
                    "SECONDLY" => Some(Duration::seconds(1)),
                    _ => None,
                };
                if freq.is_none() {
                    time_kept = false;
                }
            }
            "INTERVAL" => interval = val.parse().ok().filter(|&i| i > 0).unwrap_or(1),
            "UNTIL" => {
                let v = val.strip_suffix('Z').unwrap_or(val);
                until = NaiveDateTime::parse_from_str(v, "%Y%m%dT%H%M%S")
                    .ok()
                    .map(|t| match event_tz {
                        Some(tz) if val.ends_with('Z') => tz.from_utc_datetime(&t).naive_local(),
                        _ => t,
                    })
                    .or_else(|| {
                        NaiveDate::parse_from_str(v, "%Y%m%d")
                            .ok()
                            .and_then(|d| d.and_hms_opt(23, 59, 59))
                    });
            }
            "COUNT" => count = val.parse().ok(),
            "BYDAY" => {
                thinned = true;
                for day_str in val.split(',') {
                    match parse_byday(day_str.trim()) {
                        Some(bd) => by_day.push(bd),
                        None => readable = false,
                    }
                }
            }
            "BYMONTH" => {
                thinned = true;
                for m in val.split(',') {
                    match m.trim().parse::<u32>() {
                        Ok(m) if (1..=12).contains(&m) => by_month.push(m),
                        _ => readable = false,
                    }
                }
            }
            "BYMONTHDAY" => {
                thinned = true;
                for md in val.split(',') {
                    match md.trim().parse::<i32>() {
                        Ok(md) if md != 0 && (-31..=31).contains(&md) => by_month_day.push(md),
                        _ => readable = false,
                    }
                }
            }
            "WKST" => match parse_weekday(val.trim()) {
                Some(wd) => week_start = wd,
                None => readable = false,
            },
            "BYSETPOS" => {
                thinned = true;
                for pos in val.split(',') {
                    match pos.trim().trim_start_matches('+').parse::<i32>() {
                        Ok(pos) if pos != 0 && (-366..=366).contains(&pos) => by_set_pos.push(pos),
                        _ => readable = false,
                    }
                }
            }
            "BYHOUR" | "BYMINUTE" | "BYSECOND" => {
                thinned = true;
                readable = false;
                time_kept = false;
            }
            // BYYEARDAY, BYWEEKNO, RSCALE, SKIP and anything else calrs
            // does not implement: never expanded as if it were absent.
            _ => {
                thinned = true;
                readable = false;
            }
        }
    }
    if freq.is_none() {
        time_kept = false;
    }
    // A numbered BYDAY only means something in a month or a year, and a
    // month holds at most five of a weekday: anything else is unreadable.
    let max_nth = match freq {
        Some(Freq::Monthly) => 5,
        Some(Freq::Yearly) if !by_month.is_empty() => 5,
        Some(Freq::Yearly) => 53,
        _ => 0,
    };
    if by_day
        .iter()
        .any(|bd| bd.nth.is_some_and(|n| n.abs() > max_nth))
    {
        readable = false;
    }

    match freq {
        Some(freq) if readable => Ok(RRule {
            freq,
            interval,
            until,
            count,
            by_day,
            by_month,
            by_month_day,
            by_set_pos,
            week_start,
        }),
        _ => Err(Unreadable {
            time_kept,
            until,
            // Without `BY*` parts, a sub-daily series' last start is
            // (COUNT - 1) * INTERVAL units after DTSTART.
            last_start: match (sub_daily, count, thinned) {
                (Some(unit), Some(count), false) => {
                    i32::try_from(i64::from(count.saturating_sub(1)) * i64::from(interval))
                        .ok()
                        .and_then(|n| unit.checked_mul(n))
                }
                _ => None,
            },
        }),
    }
}

fn parse_byday(s: &str) -> Option<ByDay> {
    // "MO", "TU", "2MO", "-1FR", "+3TH", etc.
    if !s.is_ascii() {
        return None;
    }
    let (nth_part, day_part) = s.split_at(s.len().checked_sub(2)?);
    let nth = if nth_part.is_empty() {
        None
    } else {
        let n: i32 = nth_part.trim_start_matches('+').parse().ok()?;
        // A month holds at most five of a weekday, a year 53.
        if n == 0 || !(-53..=53).contains(&n) {
            return None;
        }
        Some(n)
    };
    Some(ByDay {
        weekday: parse_weekday(day_part)?,
        nth,
    })
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

fn is_excluded(dt: NaiveDateTime, exdates: &[NaiveDateTime]) -> bool {
    exdates.iter().any(|ex| {
        // Match by date + time, or just by date for all-day events
        *ex == dt || ex.date() == dt.date()
    })
}

/// Monday-based week start for a given date.
#[cfg(test)]
fn week_start(d: NaiveDate) -> NaiveDate {
    week_start_on(d, Weekday::Mon)
}

/// The start of the week holding `d`, weeks starting on `wkst`.
fn week_start_on(d: NaiveDate, wkst: Weekday) -> NaiveDate {
    d - Duration::days(days_from(d.weekday(), wkst))
}

/// Days from `from` forward to `wd` (0 to 6).
fn days_from(wd: Weekday, from: Weekday) -> i64 {
    (i64::from(wd.num_days_from_monday()) - i64::from(from.num_days_from_monday())).rem_euclid(7)
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
    fn test_yearly_rrule_starts_on_dtstart() {
        let start = dt(2026, 3, 1, 9, 0);
        let end = dt(2026, 3, 1, 10, 0);
        let results = expand_rrule(start, end, "FREQ=YEARLY", &[], start, end);
        assert_eq!(results, vec![(start, end)]);
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
        let rrule = parse_rrule(rrule_str).ok().unwrap();
        let duration = event_end - event_start;
        let mut results = Vec::new();
        // DTSTART first, counted, when the rule would not produce it.
        let extra = !period_dates(&rrule, event_start, 0)
            .unwrap()
            .1
            .contains(&event_start.date());
        if extra
            && event_start < window_end
            && event_end > window_start
            && !is_excluded(event_start, exdates)
        {
            results.push((event_start, event_end));
        }
        let mut seen = i64::from(extra);
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
            "FREQ=MONTHLY;BYMONTHDAY=1,15,-1;COUNT=150",
            "FREQ=MONTHLY;BYDAY=FR;BYMONTHDAY=13",
            "FREQ=DAILY;BYDAY=MO,TU,WE,TH,FR",
            "FREQ=DAILY;INTERVAL=2;BYDAY=SA,SU;COUNT=300",
            "FREQ=DAILY;BYMONTH=12,1;COUNT=700",
            "FREQ=WEEKLY;INTERVAL=2;BYDAY=SU,TU;WKST=SU;COUNT=400",
            "FREQ=WEEKLY;BYDAY=MO;BYMONTH=11;COUNT=20",
            "FREQ=YEARLY",
            "FREQ=YEARLY;COUNT=7",
            "FREQ=YEARLY;BYMONTH=3,10;BYDAY=-1SU",
            "FREQ=YEARLY;BYMONTHDAY=-1;COUNT=40",
            "FREQ=YEARLY;BYDAY=20MO",
            "FREQ=MONTHLY;BYDAY=MO,TU,WE,TH,FR;BYSETPOS=-1;COUNT=40",
            "FREQ=MONTHLY;BYDAY=MO,TU,WE,TH,FR;BYSETPOS=1,-1",
            "FREQ=WEEKLY;BYDAY=MO,WE,FR;BYSETPOS=2;COUNT=90",
            "FREQ=YEARLY;BYMONTH=6,12;BYDAY=SU;BYSETPOS=-1;COUNT=9",
            "FREQ=WEEKLY;BYDAY=FR;BYMONTHDAY=13;COUNT=5",
            "FREQ=MONTHLY;BYDAY=1MO;BYMONTHDAY=1,2,3;COUNT=12",
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
        // What the cap left unread is one busy block to the window's end.
        assert_eq!(results.len() as i64, MAX_PERIODS + 1);
        let block = results.last().unwrap();
        assert_eq!(block.0, dt(2026, 1, 1, 0, 0) + Duration::days(MAX_PERIODS));
        assert_eq!(block.1, dt(2526, 1, 1, 0, 0));
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
        // A position no month has is unreadable: its time is busy every
        // day, not free, and nothing overflows.
        let busy = expand_rrule(
            dt(2026, 1, 1, 9, 0),
            dt(2026, 1, 1, 10, 0),
            "FREQ=MONTHLY;BYDAY=2147483647MO",
            &[],
            dt(2026, 1, 1, 0, 0),
            dt(2027, 1, 1, 0, 0),
        );
        assert_eq!(busy.len(), 365);
        assert_eq!(busy[0], (dt(2026, 1, 1, 9, 0), dt(2026, 1, 1, 10, 0)));
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

    #[test]
    fn an_excluded_occurrence_still_counts_toward_count() {
        // RFC 5545: COUNT applies to the generated instances, EXDATE is
        // removed afterwards, so the last occurrence stays on day 5.
        let start = dt(2026, 11, 1, 9, 0);
        let results = expand_rrule(
            start,
            start + Duration::hours(1),
            "FREQ=DAILY;COUNT=5",
            &[dt(2026, 11, 3, 9, 0)],
            dt(2026, 10, 1, 0, 0),
            dt(2027, 1, 1, 0, 0),
        );
        assert_eq!(
            dates(&results),
            vec![
                day(2026, 11, 1),
                day(2026, 11, 2),
                day(2026, 11, 4),
                day(2026, 11, 5)
            ]
        );
    }

    #[test]
    fn yearly_series_are_expanded() {
        let window = (dt(2026, 1, 1, 0, 0), dt(2029, 1, 1, 0, 0));
        // A birthday since 1980.
        let results = expand_rrule(
            dt(1980, 6, 12, 0, 0),
            dt(1980, 6, 13, 0, 0),
            "FREQ=YEARLY",
            &[],
            window.0,
            window.1,
        );
        assert_eq!(
            dates(&results),
            vec![day(2026, 6, 12), day(2027, 6, 12), day(2028, 6, 12)]
        );
        // 29 February only in leap years.
        let results = expand_rrule(
            dt(2000, 2, 29, 10, 0),
            dt(2000, 2, 29, 11, 0),
            "FREQ=YEARLY",
            &[],
            window.0,
            window.1,
        );
        assert_eq!(dates(&results), vec![day(2028, 2, 29)]);
        // The last Sunday of March since 1990, with one year cancelled.
        let results = expand_rrule(
            dt(1990, 3, 25, 9, 0),
            dt(1990, 3, 25, 10, 0),
            "FREQ=YEARLY;BYMONTH=3;BYDAY=-1SU",
            &[dt(2027, 3, 28, 9, 0)],
            window.0,
            window.1,
        );
        assert_eq!(dates(&results), vec![day(2026, 3, 29), day(2028, 3, 26)]);
        // COUNT is counted from DTSTART: eight years from 2020 end in 2027.
        let results = expand_rrule(
            dt(2020, 5, 1, 9, 0),
            dt(2020, 5, 1, 10, 0),
            "FREQ=YEARLY;COUNT=8",
            &[],
            window.0,
            window.1,
        );
        assert_eq!(dates(&results), vec![day(2026, 5, 1), day(2027, 5, 1)]);
    }

    #[test]
    fn daily_byday_keeps_only_its_days() {
        let results = expand_rrule(
            dt(2019, 1, 7, 9, 0),
            dt(2019, 1, 7, 9, 15),
            "FREQ=DAILY;BYDAY=MO,TU,WE,TH,FR",
            &[],
            dt(2026, 11, 13, 0, 0),
            dt(2026, 11, 17, 0, 0),
        );
        assert_eq!(dates(&results), vec![day(2026, 11, 13), day(2026, 11, 16)]);
        // With a COUNT, the days before the window are counted one by one:
        // weekdays from Monday 2 November 2026, the 7th is 10 November.
        let results = expand_rrule(
            dt(2026, 11, 2, 9, 0),
            dt(2026, 11, 2, 9, 15),
            "FREQ=DAILY;BYDAY=MO,TU,WE,TH,FR;COUNT=7",
            &[],
            dt(2026, 11, 6, 0, 0),
            dt(2026, 12, 1, 0, 0),
        );
        assert_eq!(
            dates(&results),
            vec![day(2026, 11, 6), day(2026, 11, 9), day(2026, 11, 10)]
        );
    }

    #[test]
    fn monthly_month_days() {
        let results = expand_rrule(
            dt(2001, 1, 1, 9, 0),
            dt(2001, 1, 1, 10, 0),
            "FREQ=MONTHLY;BYMONTHDAY=1,15,-1",
            &[],
            dt(2026, 2, 1, 0, 0),
            dt(2026, 3, 1, 0, 0),
        );
        assert_eq!(
            dates(&results),
            vec![day(2026, 2, 1), day(2026, 2, 15), day(2026, 2, 28)]
        );
        // Friday the 13th.
        let results = expand_rrule(
            dt(2001, 4, 13, 9, 0),
            dt(2001, 4, 13, 10, 0),
            "FREQ=MONTHLY;BYDAY=FR;BYMONTHDAY=13",
            &[],
            dt(2026, 1, 1, 0, 0),
            dt(2027, 1, 1, 0, 0),
        );
        assert_eq!(
            dates(&results),
            vec![day(2026, 2, 13), day(2026, 3, 13), day(2026, 11, 13)]
        );
    }

    #[test]
    fn week_start_changes_which_weeks_an_interval_keeps() {
        // RFC 5545's own example: the two lists differ only by WKST.
        let start = dt(1997, 8, 5, 9, 0);
        let window = (dt(1997, 8, 1, 0, 0), dt(1997, 10, 1, 0, 0));
        let expand = |rule| {
            dates(&expand_rrule(
                start,
                start + Duration::hours(1),
                rule,
                &[],
                window.0,
                window.1,
            ))
        };
        assert_eq!(
            expand("FREQ=WEEKLY;INTERVAL=2;COUNT=4;BYDAY=TU,SU;WKST=MO"),
            vec![
                day(1997, 8, 5),
                day(1997, 8, 10),
                day(1997, 8, 19),
                day(1997, 8, 24)
            ]
        );
        assert_eq!(
            expand("FREQ=WEEKLY;INTERVAL=2;COUNT=4;BYDAY=TU,SU;WKST=SU"),
            vec![
                day(1997, 8, 5),
                day(1997, 8, 17),
                day(1997, 8, 19),
                day(1997, 8, 31)
            ]
        );
    }

    #[test]
    fn a_rule_calrs_cannot_expand_blocks_from_dtstart_within_the_window() {
        let start = dt(2026, 11, 10, 9, 0);
        let end = dt(2026, 11, 10, 9, 30);
        let window = (dt(2026, 11, 9, 0, 0), dt(2026, 11, 12, 0, 0));
        for rule in [
            "FREQ=HOURLY",
            "FREQ=MINUTELY;INTERVAL=15",
            "FREQ=DAILY;BYHOUR=9,14",
            "FREQ=WEEKLY;BYHOUR=9;COUNT=4",
            "FREQ=FORTNIGHTLY",
            "INTERVAL=2",
        ] {
            assert!(!is_supported(rule), "{rule}");
            assert_eq!(
                expand_rrule(start, end, rule, &[], window.0, window.1),
                vec![(start, window.1)],
                "{rule}: busy from DTSTART, not before it"
            );
        }
        // UNTIL bounds it: the last start plus the event's length.
        assert_eq!(
            expand_rrule(
                start,
                end,
                "FREQ=HOURLY;UNTIL=20261110T170000",
                &[],
                window.0,
                window.1
            ),
            vec![(start, dt(2026, 11, 10, 17, 30))]
        );
        // So does a COUNT on a plain sub-daily rule: three starts, two
        // hours apart, the last at 13:00.
        assert_eq!(
            expand_rrule(
                start,
                end,
                "FREQ=HOURLY;INTERVAL=2;COUNT=3",
                &[],
                window.0,
                window.1
            ),
            vec![(start, dt(2026, 11, 10, 13, 30))]
        );
        // Nothing before DTSTART, nothing after the series' end.
        assert!(expand_rrule(
            start,
            end,
            "FREQ=HOURLY",
            &[],
            dt(2026, 11, 1, 0, 0),
            dt(2026, 11, 2, 0, 0)
        )
        .is_empty());
        assert!(expand_rrule(
            start,
            end,
            "FREQ=HOURLY;UNTIL=20261110T170000",
            &[],
            dt(2026, 11, 11, 0, 0),
            dt(2026, 11, 12, 0, 0)
        )
        .is_empty());
        // Any other part calrs does not implement keeps DTSTART's time of
        // day: that time is blocked, every day from DTSTART, no more.
        for rule in [
            "FREQ=YEARLY;BYYEARDAY=1;COUNT=3",
            "FREQ=YEARLY;BYWEEKNO=20",
            "FREQ=WEEKLY;BYDAY=XX",
            "FREQ=MONTHLY;RSCALE=HEBREW",
            "FREQ=MONTHLY;BYMONTHDAY=31;SKIP=BACKWARD",
            "FREQ=DAILY;BYDAY=1MO",
            "FREQ=MONTHLY;BYDAY=6MO",
            "FREQ=WEEKLY;X-NAME=1",
        ] {
            assert!(!is_supported(rule), "{rule}");
            assert_eq!(
                expand_rrule(start, end, rule, &[], window.0, window.1),
                vec![
                    (start, end),
                    (dt(2026, 11, 11, 9, 0), dt(2026, 11, 11, 9, 30)),
                ],
                "{rule}: DTSTART's time each day from DTSTART"
            );
        }
        for rule in [
            "",
            "FREQ=DAILY",
            "freq=weekly;byday=mo",
            "FREQ=YEARLY;BYSETPOS=-1;BYDAY=MO",
        ] {
            assert!(is_supported(rule), "{rule}");
        }
    }

    #[test]
    fn rule_names_and_values_are_read_whatever_their_case() {
        let start = dt(2026, 11, 9, 9, 0);
        let results = expand_rrule(
            start,
            start + Duration::hours(1),
            "freq=daily;Count=2",
            &[],
            dt(2026, 11, 1, 0, 0),
            dt(2026, 12, 1, 0, 0),
        );
        assert_eq!(dates(&results), vec![day(2026, 11, 9), day(2026, 11, 10)]);
    }

    #[test]
    fn a_utc_until_is_read_in_the_events_zone() {
        // 00:00Z on 10 November is 09:00 in Tokyo: that day's 09:00
        // occurrence is the last, not one past the end.
        let tokyo: Tz = "Asia/Tokyo".parse().unwrap();
        let busy = occurrences_in(
            "20261101T090000",
            "20261101T093000",
            "FREQ=DAILY;UNTIL=20261110T000000Z",
            None,
            Some("Asia/Tokyo"),
            dt(2026, 11, 9, 0, 0),
            dt(2026, 11, 13, 0, 0),
            tokyo,
        );
        assert_eq!(
            busy,
            vec![
                (dt(2026, 11, 9, 9, 0), dt(2026, 11, 9, 9, 30)),
                (dt(2026, 11, 10, 9, 0), dt(2026, 11, 10, 9, 30)),
            ]
        );
        // West of UTC the same instant is the evening before: the 10th is
        // past the end.
        let ny: Tz = "America/New_York".parse().unwrap();
        let busy = occurrences_in(
            "20261101T090000",
            "20261101T093000",
            "FREQ=DAILY;UNTIL=20261110T000000Z",
            None,
            Some("America/New_York"),
            dt(2026, 11, 9, 0, 0),
            dt(2026, 11, 13, 0, 0),
            ny,
        );
        assert_eq!(busy, vec![(dt(2026, 11, 9, 9, 0), dt(2026, 11, 9, 9, 30))]);
        // An UNTIL without Z is in the event's own time.
        let busy = occurrences_in(
            "20261101T090000",
            "20261101T093000",
            "FREQ=DAILY;UNTIL=20261110T090000",
            None,
            Some("America/New_York"),
            dt(2026, 11, 9, 0, 0),
            dt(2026, 11, 13, 0, 0),
            ny,
        );
        assert_eq!(busy.len(), 2);
    }

    #[test]
    fn week_start_groups_sunday_with_the_following_days() {
        // Sunday 1 November 2026, every other week, weeks starting Sunday.
        let start = dt(2026, 11, 1, 10, 0);
        let rule = "FREQ=WEEKLY;INTERVAL=2;WKST=SU;BYDAY=SU,TU";
        let results = expand_rrule(
            start,
            start + Duration::hours(1),
            rule,
            &[],
            dt(2026, 11, 1, 0, 0),
            dt(2026, 11, 20, 0, 0),
        );
        assert_eq!(
            dates(&results),
            vec![
                day(2026, 11, 1),
                day(2026, 11, 3),
                day(2026, 11, 15),
                day(2026, 11, 17)
            ]
        );
        // The same series six years on, skipped ahead to the window.
        let start = dt(2020, 11, 1, 10, 0);
        let results = expand_rrule(
            start,
            start + Duration::hours(1),
            rule,
            &[],
            dt(2026, 11, 1, 0, 0),
            dt(2026, 11, 20, 0, 0),
        );
        assert_eq!(
            results,
            walk_from_dtstart(
                start,
                start + Duration::hours(1),
                rule,
                &[],
                dt(2026, 11, 1, 0, 0),
                dt(2026, 11, 20, 0, 0)
            )
        );
        assert!(!results.is_empty());
    }

    // --- RDATE ---

    /// A weekly series from Monday 2 November 2026 at 10:00 Paris, two
    /// occurrences, with every RDATE value form.
    const RDATE_SERIES: &str = "BEGIN:VCALENDAR\r\nBEGIN:VEVENT\r\nUID:r\r\n\
        DTSTART;TZID=Europe/Paris:20261102T100000\r\n\
        DTEND;TZID=Europe/Paris:20261102T103000\r\n\
        RRULE:FREQ=WEEKLY;COUNT=2\r\n\
        RDATE:20261110T090000Z\r\n\
        RDATE;TZID=America/New_York:20261111T040000\r\n\
        RDATE:20261112T150000\r\n\
        RDATE;VALUE=DATE:20261113\r\n\
        RDATE;VALUE=PERIOD:20261114T080000Z/20261114T100000Z,20261115T080000Z/PT3H\r\n\
        RDATE:20261116T090000Z,20261117T090000Z\r\n\
        EXDATE:20261116T090000Z\r\n\
        END:VEVENT\r\nBEGIN:VEVENT\r\nUID:r\r\n\
        RECURRENCE-ID;TZID=Europe/Paris:20261117T100000\r\n\
        DTSTART;TZID=Europe/Paris:20261117T170000\r\n\
        END:VEVENT\r\nEND:VCALENDAR\r\n";

    #[test]
    fn rdate_values_are_read_in_every_form() {
        let rdates = extract_rdates(RDATE_SERIES, Some("Europe/Paris"));
        assert_eq!(
            rdates,
            vec![
                // UTC, converted to Paris.
                RDate::At(dt(2026, 11, 10, 10, 0), None),
                // Under a TZID: 04:00 in New York is 10:00 in Paris.
                RDate::At(dt(2026, 11, 11, 10, 0), None),
                // Floating: already in the event's time.
                RDate::At(dt(2026, 11, 12, 15, 0), None),
                // A date: at DTSTART's time of day when expanded.
                RDate::Day(day(2026, 11, 13)),
                // Periods, with an end or a duration.
                RDate::At(dt(2026, 11, 14, 9, 0), Some(Duration::hours(2))),
                RDate::At(dt(2026, 11, 15, 9, 0), Some(Duration::hours(3))),
                RDate::At(dt(2026, 11, 16, 10, 0), None),
                RDate::At(dt(2026, 11, 17, 10, 0), None),
            ]
        );
    }

    #[test]
    fn rdates_join_the_series_before_exdate_and_overrides() {
        let paris: Tz = "Europe/Paris".parse().unwrap();
        let busy = occurrences_in(
            "20261102T100000",
            "20261102T103000",
            "FREQ=WEEKLY;COUNT=2",
            Some(RDATE_SERIES),
            Some("Europe/Paris"),
            dt(2026, 11, 9, 0, 0),
            dt(2026, 11, 20, 0, 0),
            paris,
        );
        assert_eq!(
            busy,
            vec![
                // The second and last RRULE occurrence: the RDATEs do not
                // use up the COUNT.
                (dt(2026, 11, 9, 10, 0), dt(2026, 11, 9, 10, 30)),
                (dt(2026, 11, 10, 10, 0), dt(2026, 11, 10, 10, 30)),
                (dt(2026, 11, 11, 10, 0), dt(2026, 11, 11, 10, 30)),
                (dt(2026, 11, 12, 15, 0), dt(2026, 11, 12, 15, 30)),
                (dt(2026, 11, 13, 10, 0), dt(2026, 11, 13, 10, 30)),
                (dt(2026, 11, 14, 9, 0), dt(2026, 11, 14, 11, 0)),
                (dt(2026, 11, 15, 9, 0), dt(2026, 11, 15, 12, 0)),
                // 16th: removed by EXDATE; 17th: moved, its row blocks 17:00.
            ]
        );
    }

    #[test]
    fn a_series_of_rdates_alone_starts_on_dtstart() {
        let ical = "BEGIN:VEVENT\r\nDTSTART:20261109T140000\r\n\
            RDATE:20261111T140000,20261125T140000\r\nEND:VEVENT\r\n";
        let busy = occurrences_in(
            "20261109T140000",
            "20261109T150000",
            "",
            Some(ical),
            None,
            dt(2026, 11, 9, 0, 0),
            dt(2026, 11, 20, 0, 0),
            Tz::UTC,
        );
        assert_eq!(
            busy,
            vec![
                (dt(2026, 11, 9, 14, 0), dt(2026, 11, 9, 15, 0)),
                (dt(2026, 11, 11, 14, 0), dt(2026, 11, 11, 15, 0)),
            ]
        );
    }

    // --- second review of #394 ---

    #[test]
    fn bysetpos_keeps_the_positions_of_each_period() {
        // The last weekday of the month, twelve times, as Thunderbird and
        // Apple write it, from Friday 30 January 2026: the COUNT reaches
        // 31 December, and February's other weekdays do not use it up.
        let start = dt(2026, 1, 30, 9, 0);
        let rule = "FREQ=MONTHLY;BYDAY=MO,TU,WE,TH,FR;BYSETPOS=-1;COUNT=12";
        let expand = |from, to| {
            dates(&expand_rrule(
                start,
                start + Duration::hours(1),
                rule,
                &[],
                from,
                to,
            ))
        };
        assert_eq!(
            expand(dt(2026, 2, 1, 0, 0), dt(2026, 5, 1, 0, 0)),
            vec![day(2026, 2, 27), day(2026, 3, 31), day(2026, 4, 30)]
        );
        assert_eq!(
            expand(dt(2026, 12, 1, 0, 0), dt(2027, 3, 1, 0, 0)),
            vec![day(2026, 12, 31)]
        );
        // Weekly: the second of Monday, Wednesday, Friday is Wednesday.
        let results = expand_rrule(
            dt(2026, 11, 2, 9, 0),
            dt(2026, 11, 2, 10, 0),
            "FREQ=WEEKLY;BYDAY=MO,WE,FR;BYSETPOS=2",
            &[],
            dt(2026, 11, 9, 0, 0),
            dt(2026, 11, 16, 0, 0),
        );
        assert_eq!(dates(&results), vec![day(2026, 11, 11)]);
    }

    #[test]
    fn month_days_and_numbered_weekdays_limit_each_other() {
        // The first Monday, only when it falls on the 1st to the 3rd:
        // November 2026 (the 2nd), February and March 2027 (the 1st).
        let results = expand_rrule(
            dt(2026, 11, 2, 9, 0),
            dt(2026, 11, 2, 10, 0),
            "FREQ=MONTHLY;BYDAY=1MO;BYMONTHDAY=1,2,3",
            &[],
            dt(2026, 11, 1, 0, 0),
            dt(2027, 4, 1, 0, 0),
        );
        assert_eq!(
            dates(&results),
            vec![day(2026, 11, 2), day(2027, 2, 1), day(2027, 3, 1)]
        );
        // Weekly with BYMONTHDAY: Friday the 13th, counted.
        let results = expand_rrule(
            dt(2026, 2, 13, 9, 0),
            dt(2026, 2, 13, 10, 0),
            "FREQ=WEEKLY;BYDAY=FR;BYMONTHDAY=13;COUNT=3",
            &[],
            dt(2026, 1, 1, 0, 0),
            dt(2028, 1, 1, 0, 0),
        );
        assert_eq!(
            dates(&results),
            vec![day(2026, 2, 13), day(2026, 3, 13), day(2026, 11, 13)]
        );
    }

    #[test]
    fn a_utc_until_of_a_floating_event_is_read_in_the_hosts_zone() {
        // A floating 09:00 is 09:00 in Tokyo for a Tokyo host, and the
        // UNTIL instant 00:00Z on the 10th is 09:00 there.
        let tokyo: Tz = "Asia/Tokyo".parse().unwrap();
        let busy = occurrences_in(
            "20261101T090000",
            "20261101T093000",
            "FREQ=DAILY;UNTIL=20261110T000000Z",
            None,
            None,
            dt(2026, 11, 9, 0, 0),
            dt(2026, 11, 13, 0, 0),
            tokyo,
        );
        assert_eq!(dates(&busy), vec![day(2026, 11, 9), day(2026, 11, 10)]);
    }

    #[test]
    fn dtstart_is_always_the_first_instance() {
        // Sunday 8 November, a Monday rule: the Sunday is the first of two.
        let start = dt(2026, 11, 8, 9, 0);
        let results = expand_rrule(
            start,
            start + Duration::hours(1),
            "FREQ=WEEKLY;BYDAY=MO;COUNT=2",
            &[],
            dt(2026, 11, 1, 0, 0),
            dt(2026, 12, 1, 0, 0),
        );
        assert_eq!(dates(&results), vec![day(2026, 11, 8), day(2026, 11, 9)]);
        // Unless an EXDATE cancels it; it still counts.
        let results = expand_rrule(
            start,
            start + Duration::hours(1),
            "FREQ=WEEKLY;BYDAY=MO;COUNT=2",
            &[start],
            dt(2026, 11, 1, 0, 0),
            dt(2026, 12, 1, 0, 0),
        );
        assert_eq!(dates(&results), vec![day(2026, 11, 9)]);
    }
}
