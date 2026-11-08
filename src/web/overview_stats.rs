//! Dashboard overview: the masthead sentence, the day ruler, the week list,
//! the 12-week chart, the 30-day figures, the split by meeting type and the
//! "coming up" ledger.
//!
//! Every booking here is one the signed-in user takes part in, using the
//! same rule as the slot engine (`user_busy_bookings` in `booking_time.rs`)
//! minus its status filter: a booking assigned to them, an unassigned
//! booking on a non-collective event type they own, an unassigned collective
//! booking on a team they belong to and are not excluded from, plus a
//! booking they claimed as a watcher. A
//! round-robin booking assigned to another member does not count for the
//! event type's creator.
//!
//! calrs is the booking layer on top of the user's calendar, not a calendar:
//! the ruler shades the hours the user is open for bookings (the
//! availability rules and overrides of their enabled event types, merged),
//! draws calrs bookings with their title and guest, and shows events read
//! from synced calendars only as anonymous "unavailable" blocks.
//!
//! The SQL only narrows rows to a date window (two indexed range checks);
//! the bucketing happens in Rust because legacy rows store a wall-clock
//! time in the event type's zone while newer rows store UTC, and only
//! `booking_time::local` reads both correctly.

use super::*;

/// Number of weeks in the volume chart, the current week included.
pub(super) const WEEKS: usize = 12;

/// The "coming up" ledger: the next 14 days, at most 8 rows.
const AGENDA_DAYS: i64 = 14;
const AGENDA_LEN: usize = 8;

/// The ruler's default span, widened to fit anything outside it.
const RULER_FROM_HOUR: i64 = 7;
const RULER_TO_HOUR: i64 = 20;

/// Scope predicate, see the module documentation. Alias `b`. Binds the user
/// id four times. A team member excluded from the event type (weight 0, as
/// `hosts.rs` reads it) is not one of its hosts. An unassigned collective
/// booking is its roster's (members not excluded), its owner included only
/// when on that roster, as in `user_busy_bookings` (#375). One difference on
/// purpose: an unassigned round-robin booking (deleted member, legacy row) is
/// the owner's alone here, while busy time blocks every member for it, the
/// safe direction for availability.
///
/// The rule is otherwise the one of `user_busy_bookings`, written so that every branch
/// of the OR reads an index of `bookings` (assigned user, event type,
/// claimant) and SQLite unions them: the plain form tests `a.user_id` and
/// the team membership on the joined event type, which no index of
/// `bookings` can answer, so it scanned every booking of the instance. The
/// unary `+` keeps the planner from driving the event type branch from
/// `assigned_user_id IS NULL`, which matches most rows.
const HOST_SCOPE: &str = "(b.assigned_user_id = ?
    OR (+b.assigned_user_id IS NULL AND b.event_type_id IN (
          SELECT ot.id FROM event_types ot JOIN accounts oa ON oa.id = ot.account_id
          WHERE oa.user_id = ?
            AND NOT (ot.team_id IS NOT NULL AND ot.scheduling_mode = 'collective')
          UNION ALL
          SELECT tt.id FROM event_types tt JOIN team_members tm ON tm.team_id = tt.team_id
          WHERE tm.user_id = ? AND tt.scheduling_mode = 'collective'
            AND NOT EXISTS (SELECT 1 FROM event_type_member_weights etw
                WHERE etw.event_type_id = tt.id AND etw.user_id = tm.user_id AND etw.weight <= 0)))
    OR b.claimed_by_user_id = ?)";

/// Whether /dashboard/bookings lists the booking, so the overview links to
/// its row only then: that page lists the pending and upcoming confirmed
/// bookings of the event types the user owns (same predicate as its
/// queries). Binds the user id once.
const LISTED_ON_BOOKINGS_PAGE: &str = "(a.user_id = ? AND (b.status = 'pending'
    OR (b.status = 'confirmed' AND datetime(b.start_at) >= datetime('now'))))";

const FROM_SCOPED: &str = "FROM bookings b
     JOIN event_types et ON et.id = b.event_type_id
     LEFT JOIN accounts a ON a.id = et.account_id
     LEFT JOIN users u ON u.id = a.user_id";

const START_AT: &str = "CASE b.time_version WHEN 1 THEN b.start_at ELSE rtrim(b.start_at, 'Z') END";
const END_AT: &str = "CASE b.time_version WHEN 1 THEN b.end_at ELSE rtrim(b.end_at, 'Z') END";
const STORED_TZ: &str = "COALESCE(NULLIF(et.timezone, ''), u.timezone, 'UTC')";

/// One booking, its times already in the host's zone.
#[derive(Debug, Clone)]
pub(super) struct StatRow {
    pub start: NaiveDateTime,
    pub end: NaiveDateTime,
    pub created: Option<NaiveDateTime>,
    pub status: String,
    pub event_type_id: String,
    pub event_title: String,
}

/// Fetch the bookings the overview needs: those starting from 13 weeks ago
/// to 9 days ahead, and those created in the last 62 days. The bounds are
/// widened by two days so a row's zone can never push it out early; the
/// exact cut is made in [`compute`].
pub(super) async fn fetch_stat_rows(
    pool: &SqlitePool,
    user_id: &str,
    host_tz: Tz,
    now: chrono::DateTime<Utc>,
) -> Result<Vec<StatRow>, sqlx::Error> {
    let iso = "%Y-%m-%dT%H:%M:%S";
    let start_lo = (now - Duration::days(7 * (WEEKS as i64 + 1) + 2))
        .format(iso)
        .to_string();
    let start_hi = (now + Duration::days(9)).format(iso).to_string();
    let created_lo = (now - Duration::days(62))
        .format("%Y-%m-%d %H:%M:%S")
        .to_string();
    let sql = format!(
        "SELECT {START_AT}, {END_AT}, b.status, b.created_at, {STORED_TZ}, et.id, et.title
         {FROM_SCOPED}
         WHERE {HOST_SCOPE}
           AND ((b.start_at >= ? AND b.start_at < ?) OR b.created_at >= ?)"
    );
    let rows: Vec<(
        String,
        String,
        String,
        Option<String>,
        String,
        String,
        String,
    )> = sqlx::query_as(&sql)
        .bind(user_id)
        .bind(user_id)
        .bind(user_id)
        .bind(user_id)
        .bind(&start_lo)
        .bind(&start_hi)
        .bind(&created_lo)
        .fetch_all(pool)
        .await?;
    Ok(rows
        .into_iter()
        .filter_map(|(start, end, status, created, stored, et_id, title)| {
            let stored = stored.parse::<Tz>().unwrap_or(Tz::UTC);
            let start = crate::booking_time::local(&start, stored, host_tz)?;
            let end = crate::booking_time::local(&end, stored, host_tz)?;
            let created = created
                .as_deref()
                .and_then(parse_booking_datetime)
                .map(|utc| host_tz.from_utc_datetime(&utc).naive_local());
            Some(StatRow {
                start,
                end,
                created,
                status,
                event_type_id: et_id,
                event_title: title,
            })
        })
        .collect())
}

/// Whether the user has ever had a booking in scope, for the checklist.
pub(super) async fn has_any_booking(pool: &SqlitePool, user_id: &str) -> bool {
    let sql = format!("SELECT EXISTS (SELECT 1 {FROM_SCOPED} WHERE {HOST_SCOPE})");
    sqlx::query_scalar::<_, bool>(&sql)
        .bind(user_id)
        .bind(user_id)
        .bind(user_id)
        .bind(user_id)
        .fetch_one(pool)
        .await
        .unwrap_or(false)
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct WeekBucket {
    pub start: Option<NaiveDate>,
    /// Meetings starting this week, by status.
    pub confirmed: i64,
    pub pending: i64,
    pub cancelled: i64,
    /// Minutes of confirmed meetings starting this week.
    pub minutes: i64,
}

#[derive(Debug, Clone, Default)]
pub(super) struct OverviewStats {
    pub booked30: i64,
    pub booked_prev30: i64,
    pub minutes30: i64,
    pub minutes_prev30: i64,
    pub held30: i64,
    pub cancelled30: i64,
    pub held_prev30: i64,
    pub cancelled_prev30: i64,
    /// Sum and count of the notice (creation to start) of the bookings made
    /// in the last 30 days, in minutes.
    pub notice_minutes: i64,
    pub notice_count: i64,
    /// The same for the bookings made in the 30 days before.
    pub notice_prev_minutes: i64,
    pub notice_prev_count: i64,
    pub weeks: Vec<WeekBucket>,
    /// Confirmed meetings in the chart window per event type, most first.
    pub by_type: Vec<(String, i64)>,
    /// Bookings of any status starting in the chart window.
    pub chart_total: i64,
}

fn minutes(row: &StatRow) -> i64 {
    (row.end - row.start).num_minutes().max(0)
}

fn in_range(t: NaiveDateTime, from: NaiveDateTime, to: NaiveDateTime) -> bool {
    t >= from && t < to
}

/// Bucket the rows. `now` is the host's wall clock.
pub(super) fn compute(
    rows: &[StatRow],
    now: NaiveDateTime,
    week_start: WeekStart,
) -> OverviewStats {
    let today = now.date();
    let this_week = today - Duration::days(week_start.offset(today).into());
    let first_week = this_week - Duration::weeks(WEEKS as i64 - 1);
    let chart_from = first_week.and_time(NaiveTime::MIN);
    let chart_to = (this_week + Duration::weeks(1)).and_time(NaiveTime::MIN);
    let d30 = now - Duration::days(30);
    let d60 = now - Duration::days(60);

    let mut s = OverviewStats {
        weeks: (0..WEEKS)
            .map(|i| WeekBucket {
                start: Some(first_week + Duration::weeks(i as i64)),
                ..Default::default()
            })
            .collect(),
        ..Default::default()
    };
    let week_of = |t: NaiveDateTime| -> Option<usize> {
        if !in_range(t, chart_from, chart_to) {
            return None;
        }
        Some(((t.date() - first_week).num_days() / 7) as usize)
    };
    let mut by_type: HashMap<&str, (&str, i64)> = HashMap::new();

    for row in rows {
        let confirmed = row.status == "confirmed";
        let cancelled = row.status == "cancelled";
        if let Some(w) = week_of(row.start) {
            let bucket = &mut s.weeks[w];
            match row.status.as_str() {
                "confirmed" => {
                    bucket.confirmed += 1;
                    bucket.minutes += minutes(row);
                }
                "pending" => bucket.pending += 1,
                "cancelled" | "declined" => bucket.cancelled += 1,
                _ => {}
            }
            s.chart_total += 1;
            if confirmed {
                by_type
                    .entry(&row.event_type_id)
                    .or_insert((&row.event_title, 0))
                    .1 += 1;
            }
        }
        if let Some(created) = row.created {
            if in_range(created, d30, now) {
                s.booked30 += 1;
                s.notice_minutes += (row.start - created).num_minutes().max(0);
                s.notice_count += 1;
            } else if in_range(created, d60, d30) {
                s.booked_prev30 += 1;
                s.notice_prev_minutes += (row.start - created).num_minutes().max(0);
                s.notice_prev_count += 1;
            }
        }
        if in_range(row.start, d30, now) {
            if confirmed {
                s.held30 += 1;
                s.minutes30 += minutes(row);
            } else if cancelled {
                s.cancelled30 += 1;
            }
        } else if in_range(row.start, d60, d30) {
            if confirmed {
                s.held_prev30 += 1;
                s.minutes_prev30 += minutes(row);
            } else if cancelled {
                s.cancelled_prev30 += 1;
            }
        }
    }

    let mut types: Vec<(String, i64)> = by_type
        .into_values()
        .map(|(title, n)| (title.to_string(), n))
        .collect();
    types.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    s.by_type = types;
    s
}

/// Cancelled share of the meetings that were due in a period, in percent.
fn cancel_rate(cancelled: i64, held: i64) -> Option<i64> {
    let total = cancelled + held;
    (total > 0).then(|| ((cancelled as f64) * 100.0 / (total as f64)).round() as i64)
}

// --- Presentation ---

fn t(lang: &str, key: &str) -> String {
    crate::i18n::translate(lang, key, None)
}

fn tr1(lang: &str, key: &str, name: &str, n: i64) -> String {
    let mut args = fluent_bundle::FluentArgs::new();
    args.set(name, crate::i18n::number(n as f64));
    crate::i18n::translate(lang, key, Some(&args))
}

/// "2 h 30" in the reader's language ("45 min", "2 h").
pub(super) fn duration_label(lang: &str, total_minutes: i64) -> String {
    let total = total_minutes.abs();
    let (h, m) = (total / 60, total % 60);
    let mut args = fluent_bundle::FluentArgs::new();
    args.set("hours", crate::i18n::number(h as f64));
    args.set("minutes", crate::i18n::number(m as f64));
    match (h, m) {
        (0, _) => crate::i18n::translate(lang, "overview-duration-m", Some(&args)),
        (_, 0) => crate::i18n::translate(lang, "overview-duration-h", Some(&args)),
        _ => crate::i18n::translate(lang, "overview-duration-hm", Some(&args)),
    }
}

fn percent_label(lang: &str, value: i64) -> String {
    tr1(lang, "overview-percent", "value", value)
}

/// Time of day on the 24-hour clock: the dashboard is a timetable, and the
/// bookings list reads the same way.
pub(super) fn time_label(t: NaiveDateTime) -> String {
    t.format("%H:%M").to_string()
}

fn clock(minutes: i64) -> String {
    let m = minutes.clamp(0, 24 * 60);
    format!("{:02}:{:02}", m / 60, m % 60)
}

/// "09:00 to 18:00", several windows joined with commas.
fn windows_label(lang: &str, windows: &[(i64, i64)]) -> String {
    windows
        .iter()
        .map(|(s, e)| {
            let mut args = fluent_bundle::FluentArgs::new();
            args.set("from", clock(*s));
            args.set("to", clock(*e));
            crate::i18n::translate(lang, "overview-time-range", Some(&args))
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn weekday_short(lang: &str, d: chrono::Weekday) -> String {
    let key = match d {
        chrono::Weekday::Mon => "slots-weekday-mon",
        chrono::Weekday::Tue => "slots-weekday-tue",
        chrono::Weekday::Wed => "slots-weekday-wed",
        chrono::Weekday::Thu => "slots-weekday-thu",
        chrono::Weekday::Fri => "slots-weekday-fri",
        chrono::Weekday::Sat => "slots-weekday-sat",
        chrono::Weekday::Sun => "slots-weekday-sun",
    };
    t(lang, key)
}

fn weekday_long(lang: &str, d: chrono::Weekday) -> String {
    let key = match d {
        chrono::Weekday::Mon => "common-weekday-long-mon",
        chrono::Weekday::Tue => "common-weekday-long-tue",
        chrono::Weekday::Wed => "common-weekday-long-wed",
        chrono::Weekday::Thu => "common-weekday-long-thu",
        chrono::Weekday::Fri => "common-weekday-long-fri",
        chrono::Weekday::Sat => "common-weekday-long-sat",
        chrono::Weekday::Sun => "common-weekday-long-sun",
    };
    t(lang, key)
}

/// Upper-case the first letter, for a headline in a language that writes
/// weekdays in lower case ("samedi 7 novembre").
fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

/// "Today", "Tomorrow", else "Thu 12": a day relative to today.
fn relative_day(lang: &str, date: NaiveDate, today: NaiveDate) -> String {
    if date == today {
        t(lang, "overview-agenda-today")
    } else if date == today + Duration::days(1) {
        t(lang, "overview-agenda-tomorrow")
    } else {
        format!("{} {}", weekday_short(lang, date.weekday()), date.day())
    }
}

/// ISO week number of the week starting `start` (any week start: the
/// middle of the week always falls in the right ISO week).
fn iso_week(start: NaiveDate) -> u32 {
    (start + Duration::days(3)).iso_week().week()
}

/// The x-axis label of a week: its ISO number in the locales that count
/// weeks ("45"), the date it starts on in English ("Nov 2").
fn week_tick(lang: &str, start: NaiveDate, first: bool) -> String {
    if lang == "en" {
        crate::i18n::format_short_date(start, lang)
    } else if first {
        tr1(lang, "overview-week-short", "week", iso_week(start) as i64)
    } else {
        iso_week(start).to_string()
    }
}

/// The top of the y axis: the peak rounded up to an even multiple of a
/// round step (2, 10, 20, 100...), so the midline is a whole number and
/// the tallest bar fills most of the plot.
fn nice_max(v: i64) -> i64 {
    if v <= 2 {
        return 2;
    }
    let step = if v <= 10 {
        1
    } else if v <= 50 {
        5
    } else if v <= 100 {
        10
    } else {
        10_i64.pow((v as f64).log10().floor() as u32 - 1) * 5
    };
    let unit = 2 * step;
    (v + unit - 1) / unit * unit
}

fn pct(v: i64, of: i64) -> String {
    if of <= 0 {
        return "0".into();
    }
    format!("{:.2}", v as f64 * 100.0 / of as f64)
}

/// The figures, chart and meeting-type table, already formatted in `lang`.
pub(super) fn stats_context(s: &OverviewStats, lang: &str) -> minijinja::Value {
    // Last 30 days: four figures, each with a bar pair against the 30
    // days before (this period over the previous one, both scaled to the
    // larger). Good or bad is for the reader to judge.
    let before = |value: &str| {
        let mut args = fluent_bundle::FluentArgs::new();
        args.set("value", value.to_string());
        crate::i18n::translate(lang, "overview-fig-before", Some(&args))
    };
    let notice_label = |sum: i64, count: i64| {
        if count == 0 {
            return "\u{2013}".to_string();
        }
        let avg = sum / count;
        if avg < 48 * 60 {
            duration_label(lang, avg)
        } else {
            tr1(
                lang,
                "overview-fig-days",
                "count",
                (avg as f64 / 1440.0).round() as i64,
            )
        }
    };
    let rate = cancel_rate(s.cancelled30, s.held30);
    let prev_rate = cancel_rate(s.cancelled_prev30, s.held_prev30);
    let rate_label = |r: Option<i64>| r.map_or("\u{2013}".to_string(), |r| percent_label(lang, r));
    let avg = |sum: i64, count: i64| if count > 0 { sum / count } else { 0 };
    let figure = |label: &str, value: String, prev: String, cur_n: i64, prev_n: i64| {
        let top = cur_n.max(prev_n);
        context! {
            label => t(lang, label),
            cmp => before(&prev),
            value => value,
            prev => prev,
            cur_pct => pct(cur_n, top),
            prev_pct => pct(prev_n, top),
        }
    };
    let figures = vec![
        figure(
            "overview-fig-booked",
            s.booked30.to_string(),
            s.booked_prev30.to_string(),
            s.booked30,
            s.booked_prev30,
        ),
        figure(
            "overview-fig-time",
            duration_label(lang, s.minutes30),
            duration_label(lang, s.minutes_prev30),
            s.minutes30,
            s.minutes_prev30,
        ),
        figure(
            "overview-fig-cancelled",
            rate_label(rate),
            rate_label(prev_rate),
            rate.unwrap_or(0),
            prev_rate.unwrap_or(0),
        ),
        figure(
            "overview-fig-notice",
            notice_label(s.notice_minutes, s.notice_count),
            notice_label(s.notice_prev_minutes, s.notice_prev_count),
            avg(s.notice_minutes, s.notice_count),
            avg(s.notice_prev_minutes, s.notice_prev_count),
        ),
    ];
    // Nothing booked, held or cancelled in 60 days: no row of zeros.
    let has_figures = s.booked30
        + s.booked_prev30
        + s.held30
        + s.held_prev30
        + s.cancelled30
        + s.cancelled_prev30
        > 0;

    // Weekly columns: confirmed and pending stacked, cancelled counted
    // under the axis.
    let peak = s
        .weeks
        .iter()
        .map(|w| w.confirmed + w.pending)
        .max()
        .unwrap_or(0);
    let peak_index = s
        .weeks
        .iter()
        .rposition(|w| w.confirmed + w.pending == peak && peak > 0);
    let axis_max = nice_max(peak);
    let last = s.weeks.len().saturating_sub(1);
    let tick_every = if lang == "en" { 3 } else { 2 };
    let first_tick = (0..=last).find(|i| (last - i).is_multiple_of(tick_every));
    let columns: Vec<minijinja::Value> = s
        .weeks
        .iter()
        .enumerate()
        .map(|(i, w)| {
            let date = w.start.unwrap_or_default();
            let title = if i == last {
                t(lang, "overview-chart-this-week")
            } else {
                let mut args = fluent_bundle::FluentArgs::new();
                args.set("date", crate::i18n::format_short_date(date, lang));
                args.set("week", crate::i18n::number(iso_week(date) as f64));
                crate::i18n::translate(lang, "overview-chart-week-of", Some(&args))
            };
            let total = w.confirmed + w.pending;
            context! {
                title => title,
                tick => if (last - i).is_multiple_of(tick_every) { week_tick(lang, date, Some(i) == first_tick) } else { String::new() },
                current => i == last,
                total => total,
                label_total => total > 0 && (i == last || Some(i) == peak_index),
                confirmed => w.confirmed,
                pending => w.pending,
                cancelled => w.cancelled,
                total_pct => pct(total, axis_max),
            }
        })
        .collect();

    // Meeting types: the five busiest, the rest folded into one row.
    let mut types: Vec<(String, i64)> = s.by_type.iter().take(5).cloned().collect();
    let rest: i64 = s.by_type.iter().skip(5).map(|(_, n)| n).sum();
    if rest > 0 {
        types.push((t(lang, "overview-types-other"), rest));
    }
    let type_max = types.iter().map(|(_, n)| *n).max().unwrap_or(0);
    let type_rows: Vec<minijinja::Value> = types
        .iter()
        .map(|(title, n)| {
            context! {
                title => title,
                count => n,
                width => pct(*n, type_max),
            }
        })
        .collect();

    context! {
        figures => figures,
        has_figures => has_figures,
        columns => columns,
        axis_max => axis_max,
        axis_mid => axis_max / 2,
        has_chart_data => s.chart_total > 0,
        type_rows => type_rows,
    }
}

// --- Bookings of a few days, for the ruler and the week list ---

/// A booking in scope, its times in the viewer's zone.
#[derive(Debug, Clone)]
pub(super) struct DayBooking {
    pub id: String,
    pub start: NaiveDateTime,
    pub end: NaiveDateTime,
    pub pending: bool,
    pub title: String,
    pub guest: String,
    pub extra_guests: i64,
    /// Listed on /dashboard/bookings, so the overview may link to its row.
    pub listed: bool,
}

/// Confirmed and pending bookings in scope overlapping `[from, to)` (the
/// viewer's wall clock).
pub(super) async fn fetch_day_bookings(
    pool: &SqlitePool,
    user_id: &str,
    host_tz: Tz,
    from: NaiveDateTime,
    to: NaiveDateTime,
) -> Vec<DayBooking> {
    let iso = "%Y-%m-%dT%H:%M:%S";
    // Two days of slack either side: the stored value may be another zone's
    // wall clock; the exact cut is made after conversion.
    let lo = (from - Duration::days(2)).format(iso).to_string();
    let hi = (to + Duration::days(2)).format(iso).to_string();
    // `+b.status`: matching the partial idx_bookings_no_overlap would make
    // the planner read it, which cannot test `end_at` without the row.
    let sql = format!(
        "SELECT b.id, {START_AT}, {END_AT}, {STORED_TZ}, b.status, et.title, b.guest_name,
                (SELECT COUNT(*) FROM booking_attendees ba WHERE ba.booking_id = b.id),
                {LISTED_ON_BOOKINGS_PAGE}
         {FROM_SCOPED}
         WHERE {HOST_SCOPE} AND +b.status IN ('confirmed', 'pending')
           AND b.start_at < ? AND b.end_at > ?
         ORDER BY b.start_at"
    );
    #[allow(clippy::type_complexity)]
    let rows: Vec<(
        String,
        String,
        String,
        String,
        String,
        String,
        String,
        i64,
        bool,
    )> = sqlx::query_as(&sql)
        .bind(user_id)
        .bind(user_id)
        .bind(user_id)
        .bind(user_id)
        .bind(user_id)
        .bind(&hi)
        .bind(&lo)
        .fetch_all(pool)
        .await
        .unwrap_or_default();
    rows.into_iter()
        .filter_map(
            |(id, start, end, stored, status, title, guest, extra, listed)| {
                let stored = stored.parse::<Tz>().unwrap_or(Tz::UTC);
                let start = crate::booking_time::local(&start, stored, host_tz)?;
                let end = crate::booking_time::local(&end, stored, host_tz)?;
                (start < to && end > from).then_some(DayBooking {
                    id,
                    start,
                    end,
                    pending: status == "pending",
                    title,
                    guest,
                    extra_guests: extra,
                    listed,
                })
            },
        )
        .collect()
}

/// Busy times read from the user's synced calendars (the calendars marked
/// busy), in the viewer's zone. Copies of calrs bookings written back to
/// the calendar are left out by uid: the ruler draws those as bookings.
/// Only an active booking in scope counts: the leftover copy of a cancelled
/// or declined booking still blocks the time, so it still shows as busy.
/// Only the times are read: the overview never shows what an event is.
pub(super) async fn fetch_calendar_busy(
    pool: &SqlitePool,
    user_id: &str,
    host_tz: Tz,
    from: NaiveDateTime,
    to: NaiveDateTime,
) -> Vec<(NaiveDateTime, NaiveDateTime)> {
    let lo = (from - Duration::days(1))
        .format("%Y%m%dT%H%M%S")
        .to_string();
    let hi = (to + Duration::days(1)).format("%Y%m%dT%H%M%S").to_string();
    // The subquery's own `b`, `et` and `a` shadow the outer `a`. Binds the
    // user id five times.
    let scope = format!(
        "FROM events e
         JOIN calendars c ON c.id = e.calendar_id
         JOIN caldav_sources cs ON cs.id = c.source_id
         JOIN accounts a ON a.id = cs.account_id
         WHERE a.user_id = ? AND c.is_busy = 1 AND c.sync_enabled = 1 AND cs.enabled = 1
           AND (e.status IS NULL OR e.status != 'CANCELLED')
           AND (e.transp IS NULL OR e.transp != 'TRANSPARENT')
           AND NOT EXISTS (SELECT 1 {FROM_SCOPED} WHERE b.uid = e.uid
               AND b.status IN ('confirmed', 'pending') AND {HOST_SCOPE})"
    );
    let single: Vec<(String, String, Option<String>)> = sqlx::query_as(&format!(
        "SELECT e.start_at, e.end_at, e.timezone {scope}
           AND (e.rrule IS NULL OR e.rrule = '') AND e.rdate = 0
           AND e.start_at <= ? AND e.end_at >= ?"
    ))
    .bind(user_id)
    .bind(user_id)
    .bind(user_id)
    .bind(user_id)
    .bind(user_id)
    .bind(&hi)
    .bind(&lo)
    .fetch_all(pool)
    .await
    .unwrap_or_default();
    let mut busy: Vec<(NaiveDateTime, NaiveDateTime)> = single
        .iter()
        .filter_map(|(s, e, tz)| {
            let start = convert_event_to_tz(parse_ical_datetime(s)?, tz.as_deref(), host_tz);
            let end = convert_event_to_tz(parse_ical_datetime(e)?, tz.as_deref(), host_tz);
            Some((start, end))
        })
        .collect();
    let recurring: Vec<(String, String, String, Option<String>, Option<String>)> =
        sqlx::query_as(&format!(
            "SELECT e.start_at, e.end_at, COALESCE(e.rrule, ''), e.raw_ical, e.timezone {scope}
               AND ((e.rrule IS NOT NULL AND e.rrule != '') OR e.rdate = 1) AND e.start_at <= ?"
        ))
        .bind(user_id)
        .bind(user_id)
        .bind(user_id)
        .bind(user_id)
        .bind(user_id)
        .bind(&hi)
        .fetch_all(pool)
        .await
        .unwrap_or_default();
    busy.extend(expand_recurring_into_busy(&recurring, from, to, host_tz));
    busy.retain(|(s, e)| *s < to && *e > from && e > s);
    busy
}

// --- Bookable hours ---

/// One enabled event type's availability, as stored.
#[derive(Debug, Clone)]
pub(super) struct EventTypeHours {
    /// The zone the rules are written in.
    pub tz: Tz,
    /// (day_of_week 0 = Sunday, start, end)
    pub rules: Vec<(u32, NaiveTime, NaiveTime)>,
    /// (date, custom window or None, blocked)
    pub overrides: Vec<(NaiveDate, Option<(NaiveTime, NaiveTime)>, bool)>,
    pub bookable_from: Option<NaiveDate>,
    pub bookable_until: Option<NaiveDate>,
}

fn parse_hm(s: &str) -> Option<NaiveTime> {
    NaiveTime::parse_from_str(s, "%H:%M").ok()
}

/// The availability of every enabled event type the user takes bookings
/// for: their own, and those of the teams they belong to unless they are
/// excluded from it (weight 0).
pub(super) async fn load_event_type_hours(pool: &SqlitePool, user_id: &str) -> Vec<EventTypeHours> {
    let types: Vec<(String, String, Option<String>, Option<String>)> = sqlx::query_as(&format!(
        "SELECT et.id, {STORED_TZ}, et.bookable_from, et.bookable_until
         FROM event_types et
         LEFT JOIN accounts a ON a.id = et.account_id
         LEFT JOIN users u ON u.id = a.user_id
         WHERE et.enabled = 1
           AND ((et.team_id IS NULL AND a.user_id = ?)
                OR (et.team_id IS NOT NULL AND EXISTS (
                      SELECT 1 FROM team_members tm WHERE tm.team_id = et.team_id AND tm.user_id = ?
                        AND NOT EXISTS (SELECT 1 FROM event_type_member_weights etw
                            WHERE etw.event_type_id = et.id AND etw.user_id = tm.user_id
                              AND etw.weight <= 0))))"
    ))
    .bind(user_id)
    .bind(user_id)
    .fetch_all(pool)
    .await
    .unwrap_or_default();
    let mut out = Vec::with_capacity(types.len());
    for (id, tz, from, until) in types {
        let rules: Vec<(i32, String, String)> = sqlx::query_as(
            "SELECT day_of_week, start_time, end_time FROM availability_rules WHERE event_type_id = ?",
        )
        .bind(&id)
        .fetch_all(pool)
        .await
        .unwrap_or_default();
        let overrides: Vec<(String, Option<String>, Option<String>, i32)> = sqlx::query_as(
            "SELECT date, start_time, end_time, is_blocked FROM availability_overrides WHERE event_type_id = ?",
        )
        .bind(&id)
        .fetch_all(pool)
        .await
        .unwrap_or_default();
        let date = |s: Option<String>| {
            s.and_then(|d| NaiveDate::parse_from_str(d.trim(), "%Y-%m-%d").ok())
        };
        out.push(EventTypeHours {
            tz: tz.parse().unwrap_or(Tz::UTC),
            rules: rules
                .iter()
                .filter_map(|(d, s, e)| Some((u32::try_from(*d).ok()?, parse_hm(s)?, parse_hm(e)?)))
                .collect(),
            overrides: overrides
                .iter()
                .filter_map(|(d, s, e, blocked)| {
                    let day = NaiveDate::parse_from_str(d, "%Y-%m-%d").ok()?;
                    let window = match (
                        s.as_deref().and_then(parse_hm),
                        e.as_deref().and_then(parse_hm),
                    ) {
                        (Some(s), Some(e)) => Some((s, e)),
                        _ => None,
                    };
                    Some((day, window, *blocked != 0))
                })
                .collect(),
            bookable_from: date(from),
            bookable_until: date(until),
        });
    }
    out
}

/// Merge overlapping or touching intervals.
fn merge(mut v: Vec<(i64, i64)>) -> Vec<(i64, i64)> {
    v.retain(|(s, e)| e > s);
    v.sort();
    let mut out: Vec<(i64, i64)> = Vec::with_capacity(v.len());
    for (s, e) in v {
        match out.last_mut() {
            Some(last) if s <= last.1 => last.1 = last.1.max(e),
            _ => out.push((s, e)),
        }
    }
    out
}

/// The hours `day` is open for bookings, in minutes from the viewer's
/// midnight: the union, over the event types, of the windows their rules
/// and overrides give (the same precedence as `compute_slots_from_rules`:
/// a blocked override closes the day, custom hours replace the weekly
/// rules). Rules are written in the event type's zone, so the event type's
/// days either side are converted too: a window there can land on `day`
/// here.
pub(super) fn open_minutes(
    hours: &[EventTypeHours],
    day: NaiveDate,
    viewer: Tz,
) -> Vec<(i64, i64)> {
    let midnight = day.and_time(NaiveTime::MIN);
    let mut windows = Vec::new();
    for et in hours {
        for offset in -1..=1 {
            let date = day + Duration::days(offset);
            if et.bookable_from.is_some_and(|f| date < f)
                || et.bookable_until.is_some_and(|u| date > u)
            {
                continue;
            }
            let todays: Vec<_> = et.overrides.iter().filter(|o| o.0 == date).collect();
            let local: Vec<(NaiveTime, NaiveTime)> = if todays.iter().any(|o| o.2) {
                continue;
            } else if !todays.is_empty() {
                todays.iter().filter_map(|o| o.1).collect()
            } else {
                let weekday = date.weekday().num_days_from_sunday();
                et.rules
                    .iter()
                    .filter(|r| r.0 == weekday)
                    .map(|r| (r.1, r.2))
                    .collect()
            };
            for (s, e) in local {
                let to_viewer = |t: NaiveTime| {
                    et.tz
                        .from_local_datetime(&date.and_time(t))
                        .earliest()
                        .map(|z| z.with_timezone(&viewer).naive_local())
                };
                let (Some(s), Some(e)) = (to_viewer(s), to_viewer(e)) else {
                    continue;
                };
                let s = (s - midnight).num_minutes().clamp(0, 1440);
                let e = (e - midnight).num_minutes().clamp(0, 1440);
                windows.push((s, e));
            }
        }
    }
    merge(windows)
}

/// Take time off (#46) out of a day's open hours, both in minutes from the
/// viewer's midnight: the user takes no bookings then, so the day is not
/// open. A day off entirely comes back empty, and reads as closed.
pub(super) fn without_time_off(
    open: Vec<(i64, i64)>,
    day: NaiveDate,
    time_off: &[(NaiveDateTime, NaiveDateTime)],
) -> Vec<(i64, i64)> {
    let mut out = open;
    for (s, e) in time_off {
        let (cs, ce) = (minute_of(day, *s), minute_of(day, *e));
        if ce <= cs {
            continue;
        }
        out = out
            .into_iter()
            .flat_map(|(os, oe)| {
                if cs >= oe || ce <= os {
                    vec![(os, oe)]
                } else {
                    [(os, cs), (ce, oe)]
                        .into_iter()
                        .filter(|(a, b)| b > a)
                        .collect()
                }
            })
            .collect();
    }
    out
}

// --- The day ruler ---

/// A block on the ruler, in minutes from the viewer's midnight.
#[derive(Debug, Clone)]
pub(super) struct RulerBlock {
    pub start: i64,
    pub end: i64,
    /// "c" confirmed, "p" waiting for a decision, "u" unavailable.
    pub kind: &'static str,
    /// The booking's id, empty for unavailable time.
    pub id: String,
    /// The booking has a row on /dashboard/bookings to link to.
    pub listed: bool,
    pub title: String,
    pub guest: String,
}

/// Lay the day out on an hour scale. `None` when there is nothing to draw
/// (closed, no booking, nothing in the calendar).
pub(super) fn ruler_context(
    lang: &str,
    open: &[(i64, i64)],
    blocks: &[RulerBlock],
    now_minute: i64,
) -> Option<minijinja::Value> {
    if open.is_empty() && blocks.is_empty() {
        return None;
    }
    let lo = open
        .iter()
        .map(|w| w.0)
        .chain(blocks.iter().map(|b| b.start))
        .min()
        .unwrap_or(RULER_FROM_HOUR * 60);
    let hi = open
        .iter()
        .map(|w| w.1)
        .chain(blocks.iter().map(|b| b.end))
        .max()
        .unwrap_or(RULER_TO_HOUR * 60);
    let h0 = (lo.max(0) / 60).min(RULER_FROM_HOUR);
    let h1 = ((hi.min(1440) + 59) / 60).clamp(RULER_TO_HOUR, 24);
    let (from, span) = (h0 * 60, (h1 - h0) * 60);
    let at = |m: i64| (m.clamp(from, from + span) - from) as f64 * 100.0 / span as f64;
    let fmt = |v: f64| format!("{v:.3}");

    let ticks: Vec<minijinja::Value> = (h0 * 2..=h1 * 2)
        .map(|half| {
            let minute = half * 30;
            context! {
                left => fmt(at(minute)),
                half => half % 2 == 1,
                label => if half % 2 == 0 { format!("{:02}", half / 2) } else { String::new() },
                // Every other hour label is dropped on narrow screens.
                odd_hour => (half / 2) % 2 == 1,
            }
        })
        .collect();
    let bands: Vec<minijinja::Value> = open
        .iter()
        .map(|(s, e)| context! { left => fmt(at(*s)), width => fmt(at(*e) - at(*s)) })
        .collect();

    let mut sorted: Vec<&RulerBlock> = blocks.iter().collect();
    sorted.sort_by_key(|b| (b.start, b.kind == "u"));
    // Labels go on two lanes under the track: a label moves to the second
    // lane when the first is still taken at its start.
    const LABEL_WIDTH: f64 = 10.0;
    let mut lane_end = [f64::MIN, f64::MIN];
    let mut items = Vec::new();
    let mut labels: Vec<(usize, f64, minijinja::Value)> = Vec::new();
    for b in &sorted {
        let left = at(b.start);
        let width = (at(b.end) - left).max(0.4);
        let range = format!("{}\u{2013}{}", clock(b.start), clock(b.end));
        let status = match b.kind {
            "c" => t(lang, "overview-ruler-booked"),
            "p" => t(lang, "overview-ruler-pending"),
            _ => t(lang, "overview-ruler-unavailable"),
        };
        let title = if b.kind == "u" {
            format!("{range}, {status}")
        } else {
            format!("{range}, {}, {}, {status}", b.title, b.guest)
        };
        items.push(context! {
            kind => b.kind,
            id => b.id,
            listed => b.listed,
            left => fmt(left),
            width => fmt(width),
            time => clock(b.start),
            // A block under half an hour is too narrow for its time; the
            // label lane under the track carries it.
            show_time => b.end - b.start >= 30,
            past => b.end <= now_minute,
            title => title,
        });
        if b.kind != "u" {
            let lane = if left >= lane_end[0] { 0 } else { 1 };
            lane_end[lane] = left + LABEL_WIDTH;
            labels.push((
                lane,
                left,
                context! {
                    title => b.title,
                    guest => b.guest,
                    past => b.end <= now_minute,
                },
            ));
        }
    }
    // Each label may run until the next label on its lane starts.
    let label_ctx: Vec<minijinja::Value> = labels
        .iter()
        .enumerate()
        .map(|(i, (lane, left, v))| {
            let next = labels[i + 1..]
                .iter()
                .find(|(l, _, _)| l == lane)
                .map(|(_, x, _)| *x)
                .unwrap_or(100.0);
            // At least 4% when there is room for it, never past the end
            // of the track; a label starting past 96% gets what is left.
            let max = (100.0 - left).max(0.0);
            let room = (next - left - 0.6).max(4.0).min(max);
            context! {
                lane => lane,
                left => fmt(*left),
                max_width => fmt(room),
                title => v.get_attr("title").unwrap_or_default(),
                guest => v.get_attr("guest").unwrap_or_default(),
                past => v.get_attr("past").unwrap_or_default(),
            }
        })
        .collect();
    let two_lanes = labels.iter().any(|(l, _, _)| *l == 1);

    let mut args = fluent_bundle::FluentArgs::new();
    args.set("from", clock(from));
    args.set("to", clock(from + span));
    Some(context! {
        aria => crate::i18n::translate(lang, "overview-ruler-aria", Some(&args)),
        ticks => ticks,
        bands => bands,
        blocks => items,
        labels => label_ctx,
        two_lanes => two_lanes,
        has_labels => !labels.is_empty(),
        from_minute => from,
        span_minutes => span,
        now_left => (now_minute >= from && now_minute <= from + span).then(|| fmt(at(now_minute))),
        now_label => clock(now_minute),
    })
}

/// Minutes from `day`'s midnight to `t`, clamped to the day.
fn minute_of(day: NaiveDate, t: NaiveDateTime) -> i64 {
    (t - day.and_time(NaiveTime::MIN))
        .num_minutes()
        .clamp(0, 1440)
}

/// The blocks of one day: bookings with their title and guest, calendar
/// busy times merged into anonymous blocks.
pub(super) fn day_blocks(
    lang: &str,
    day: NaiveDate,
    bookings: &[DayBooking],
    busy: &[(NaiveDateTime, NaiveDateTime)],
) -> Vec<RulerBlock> {
    let (from, to) = (
        day.and_time(NaiveTime::MIN),
        (day + Duration::days(1)).and_time(NaiveTime::MIN),
    );
    let mut blocks: Vec<RulerBlock> = bookings
        .iter()
        .filter(|b| b.start < to && b.end > from)
        .map(|b| RulerBlock {
            start: minute_of(day, b.start),
            end: minute_of(day, b.end),
            kind: if b.pending { "p" } else { "c" },
            id: b.id.clone(),
            listed: b.listed,
            title: b.title.clone(),
            guest: guest_label(lang, &b.guest, b.extra_guests),
        })
        .collect();
    let unavailable = merge(
        busy.iter()
            .filter(|(s, e)| *s < to && *e > from)
            .map(|(s, e)| (minute_of(day, *s), minute_of(day, *e)))
            .collect(),
    );
    blocks.extend(unavailable.into_iter().map(|(start, end)| RulerBlock {
        start,
        end,
        kind: "u",
        id: String::new(),
        listed: false,
        title: String::new(),
        guest: String::new(),
    }));
    blocks
}

/// "Anna Keller" or "Anna Keller, 2 more".
fn guest_label(lang: &str, guest: &str, extra: i64) -> String {
    if extra > 0 {
        let mut args = fluent_bundle::FluentArgs::new();
        args.set("name", guest.to_string());
        args.set("count", crate::i18n::number(extra as f64));
        crate::i18n::translate(lang, "overview-guest-and-more", Some(&args))
    } else {
        guest.to_string()
    }
}

// --- The week view ---

/// Height of one hour in the week view, in pixels, before clamping.
const WEEK_HOUR_PX: i64 = 44;

/// Side-by-side lanes for overlapping bookings in one day column. `spans`
/// is sorted by start; returns (lane, lanes in its overlap cluster).
pub(super) fn lanes(spans: &[(i64, i64)]) -> Vec<(usize, usize)> {
    let mut out = vec![(0, 1); spans.len()];
    let mut i = 0;
    while i < spans.len() {
        let mut lane_ends: Vec<i64> = Vec::new();
        let mut cluster_end = spans[i].1;
        let mut j = i;
        while j < spans.len() && (j == i || spans[j].0 < cluster_end) {
            let (start, end) = spans[j];
            let lane = match lane_ends.iter().position(|&e| e <= start) {
                Some(l) => {
                    lane_ends[l] = end;
                    l
                }
                None => {
                    lane_ends.push(end);
                    lane_ends.len() - 1
                }
            };
            out[j].0 = lane;
            cluster_end = cluster_end.max(end);
            j += 1;
        }
        for o in &mut out[i..j] {
            o.1 = lane_ends.len();
        }
        i = j;
    }
    out
}

/// The current week as seven day columns on a shared hour axis: the hours
/// open for bookings shaded, bookings as blocks (linking to the booking),
/// calendar busy time as anonymous blocks, today marked with the now line,
/// and the booked time under each day. Days follow the user's week start.
/// `None` when the week has nothing to draw: no open hours, no booking and
/// nothing in the calendar.
#[allow(clippy::too_many_arguments)]
pub(super) fn week_context(
    lang: &str,
    hours: &[EventTypeHours],
    bookings: &[DayBooking],
    busy: &[(NaiveDateTime, NaiveDateTime)],
    time_off: &[(NaiveDateTime, NaiveDateTime)],
    viewer: Tz,
    now: NaiveDateTime,
    week_start: WeekStart,
) -> Option<minijinja::Value> {
    let today = now.date();
    let now_minute = minute_of(today, now);
    let first = today - Duration::days(week_start.offset(today).into());
    let days: Vec<(NaiveDate, Vec<(i64, i64)>, Vec<RulerBlock>)> = (0..7)
        .map(|i| {
            let day = first + Duration::days(i);
            (
                day,
                without_time_off(open_minutes(hours, day, viewer), day, time_off),
                day_blocks(lang, day, bookings, busy),
            )
        })
        .collect();

    // The axis spans the open hours and the bookings; calendar busy time
    // (an all-day event, a night flight) is clipped to it, not widening it.
    let spans = || {
        days.iter().flat_map(|(_, open, blocks)| {
            open.iter().copied().chain(
                blocks
                    .iter()
                    .filter(|b| b.kind != "u")
                    .map(|b| (b.start, b.end)),
            )
        })
    };
    let has_busy = days.iter().any(|(_, _, b)| b.iter().any(|b| b.kind == "u"));
    let (lo, hi) = match (spans().map(|s| s.0).min(), spans().map(|s| s.1).max()) {
        (Some(lo), Some(hi)) => (lo, hi),
        _ if has_busy => (8 * 60, 18 * 60),
        _ => return None,
    };
    let mut h0 = lo.clamp(0, 1440) / 60;
    let mut h1 = ((hi.clamp(0, 1440) + 59) / 60).max(h0 + 1);
    // At least six hours, so a single short window still reads as a day.
    while h1 - h0 < 6 {
        if h1 < 24 {
            h1 += 1;
        }
        if h1 - h0 < 6 && h0 > 0 {
            h0 -= 1;
        }
    }
    let (from, to) = (h0 * 60, h1 * 60);
    let span = to - from;
    let height = ((h1 - h0) * WEEK_HOUR_PX).clamp(220, 480);
    let px_per_minute = height as f64 / span as f64;
    let at = |m: i64| (m.clamp(from, to) - from) as f64 * 100.0 / span as f64;
    let fmt = |v: f64| format!("{v:.3}");

    let step = if h1 - h0 > 12 { 3 } else { 2 };
    let hour_marks: Vec<minijinja::Value> = (h0..=h1)
        .filter(|h| (h - h0) % step == 0)
        .map(|h| context! { top => fmt(at(h * 60)), label => format!("{h:02}") })
        .collect();

    let status = |kind: &str| match kind {
        "c" => t(lang, "overview-ruler-booked"),
        "p" => t(lang, "overview-ruler-pending"),
        _ => t(lang, "overview-ruler-unavailable"),
    };
    let mut booked_total = 0;
    let mut has_pending = false;
    let columns: Vec<minijinja::Value> = days
        .iter()
        .enumerate()
        .map(|(index, (day, open, blocks))| {
            let past_day = *day < today;
            let is_past = |end: i64| past_day || (*day == today && end <= now_minute);
            let bands: Vec<minijinja::Value> = open
                .iter()
                .filter(|(s, e)| *e > from && *s < to)
                .map(|(s, e)| context! { top => fmt(at(*s)), height => fmt(at(*e) - at(*s)) })
                .collect();
            let unavailable: Vec<minijinja::Value> = blocks
                .iter()
                .filter(|b| b.kind == "u" && b.end > from && b.start < to)
                .map(|b| {
                    context! {
                        top => fmt(at(b.start)),
                        height => fmt(at(b.end) - at(b.start)),
                        tip => format!("{}\u{2013}{}, {}", clock(b.start), clock(b.end), status("u")),
                    }
                })
                .collect();
            let mut booked: Vec<&RulerBlock> = blocks.iter().filter(|b| b.kind != "u").collect();
            booked.sort_by_key(|b| (b.start, b.end));
            let minutes: i64 = booked.iter().map(|b| b.end - b.start).sum();
            booked_total += minutes;
            has_pending |= booked.iter().any(|b| b.kind == "p");
            let placed = lanes(&booked.iter().map(|b| (b.start, b.end)).collect::<Vec<_>>());
            let items: Vec<minijinja::Value> = booked
                .iter()
                .zip(placed)
                .map(|(b, (lane, of))| {
                    let top = at(b.start);
                    let pixels = (b.end.min(to) - b.start.max(from)) as f64 * px_per_minute;
                    let range = format!("{}\u{2013}{}", clock(b.start), clock(b.end));
                    context! {
                        kind => b.kind,
                        id => b.id,
                        listed => b.listed,
                        top => fmt(top),
                        height => fmt(at(b.end) - top),
                        left => fmt(lane as f64 * 100.0 / of as f64),
                        width => fmt(100.0 / of as f64),
                        // Two lines (time, title) when the block is tall
                        // enough, one line, or none: the tooltip has it all.
                        lines => if pixels >= 32.0 { 2 } else if pixels >= 17.0 { 1 } else { 0 },
                        time => clock(b.start),
                        range => range.clone(),
                        title => b.title,
                        guest => b.guest,
                        status => status(b.kind),
                        aria => format!("{range}, {}, {}, {}", b.title, b.guest, status(b.kind)),
                        past => is_past(b.end),
                        // Tooltips open towards the middle of the grid.
                        tip_left => index >= 4,
                        tip_up => top > 55.0,
                    }
                })
                .collect();
            let mut title = crate::i18n::format_day_date(*day, lang);
            if !open.is_empty() {
                title = format!("{title}, {}", windows_label(lang, open));
            }
            context! {
                weekday => weekday_short(lang, day.weekday()),
                day => day.day(),
                title => capitalize(&title),
                is_today => *day == today,
                is_past => past_day,
                closed => open.is_empty(),
                bands => bands,
                busy => unavailable,
                blocks => items,
                now_top => (*day == today && now_minute >= from && now_minute <= to)
                    .then(|| fmt(at(now_minute))),
                booked => if minutes > 0 { duration_label(lang, minutes) } else { String::new() },
            }
        })
        .collect();
    let mut args = fluent_bundle::FluentArgs::new();
    args.set("booked", duration_label(lang, booked_total));
    Some(context! {
        days => columns,
        hours => hour_marks,
        height => height,
        from_minute => from,
        span_minutes => span,
        has_pending => has_pending,
        has_unavailable => has_busy,
        total => (booked_total > 0)
            .then(|| crate::i18n::translate(lang, "overview-week-total", Some(&args))),
    })
}

// --- Masthead ---

/// The date headline, the week number and one line: the meeting going on,
/// or the next one. The ruler and the week view show the rest.
pub(super) fn masthead_context(
    lang: &str,
    today: NaiveDate,
    next: Option<&minijinja::Value>,
) -> minijinja::Value {
    let next_text = next.map(|a| {
        let attr = |k: &str| a.get_attr(k).map(|v| v.to_string()).unwrap_or_default();
        let mut args = fluent_bundle::FluentArgs::new();
        args.set("title", attr("title"));
        args.set("guest", attr("guest_name"));
        if a.get_attr("ongoing").is_ok_and(|v| v.is_true()) {
            args.set("time", attr("end_time"));
            crate::i18n::translate(lang, "overview-mast-now", Some(&args))
        } else if attr("day") == today.to_string() {
            args.set("time", attr("time"));
            crate::i18n::translate(lang, "overview-mast-next", Some(&args))
        } else {
            args.set("time", attr("time"));
            args.set("day", attr("day_short"));
            crate::i18n::translate(lang, "overview-mast-next-day", Some(&args))
        }
    });
    context! {
        date => capitalize(&crate::i18n::format_day_date(today, lang)),
        week => tr1(lang, "overview-week-number", "week", today.iso_week().week() as i64),
        next => next_text,
    }
}

// --- Coming up ---

/// The next confirmed meetings in scope over the coming 14 days, ongoing
/// ones included.
pub(super) async fn agenda(
    pool: &SqlitePool,
    user_id: &str,
    host_tz: Tz,
    now: chrono::DateTime<Utc>,
    lang: &str,
) -> Vec<minijinja::Value> {
    let iso = "%Y-%m-%dT%H:%M:%S";
    let lo = (now - Duration::days(2)).format(iso).to_string();
    let hi = (now + Duration::days(AGENDA_DAYS + 2))
        .format(iso)
        .to_string();
    let sql = format!(
        "SELECT {START_AT}, {END_AT}, {STORED_TZ}, et.title, b.guest_name, b.guest_email,
                COALESCE(NULLIF(b.meeting_url, ''), et.location_value, ''),
                (SELECT COUNT(*) FROM booking_attendees ba WHERE ba.booking_id = b.id),
                b.id, b.host_notes IS NOT NULL
         {FROM_SCOPED}
         WHERE {HOST_SCOPE} AND b.status = 'confirmed' AND b.end_at >= ? AND b.start_at < ?
         ORDER BY b.start_at
         LIMIT 40"
    );
    let rows: Vec<(
        String,
        String,
        String,
        String,
        String,
        String,
        String,
        i64,
        String,
        bool,
    )> = sqlx::query_as(&sql)
        .bind(user_id)
        .bind(user_id)
        .bind(user_id)
        .bind(user_id)
        .bind(&lo)
        .bind(&hi)
        .fetch_all(pool)
        .await
        .unwrap_or_default();

    let now_local = now.with_timezone(&host_tz).naive_local();
    let today = now_local.date();
    let horizon = (today + Duration::days(AGENDA_DAYS)).and_time(NaiveTime::MIN);
    let mut items: Vec<(NaiveDateTime, minijinja::Value)> = rows
        .into_iter()
        .filter_map(
            |(start, end, stored, title, guest, email, location, extra, id, has_host_notes)| {
                let stored = stored.parse::<Tz>().unwrap_or(Tz::UTC);
                let s = crate::booking_time::local(&start, stored, host_tz)?;
                let e = crate::booking_time::local(&end, stored, host_tz)?;
                if e <= now_local || s >= horizon {
                    return None;
                }
                let s_utc = crate::booking_time::local(&start, stored, Tz::UTC)?;
                let e_utc = crate::booking_time::local(&end, stored, Tz::UTC)?;
                let lower = location.trim().to_ascii_lowercase();
                let join = if lower.starts_with("https://") || lower.starts_with("http://") {
                    location.trim().to_string()
                } else {
                    String::new()
                };
                let day = s.date();
                let day_label = if day == today {
                    t(lang, "overview-agenda-today")
                } else if day == today + Duration::days(1) {
                    t(lang, "overview-agenda-tomorrow")
                } else {
                    weekday_long(lang, day.weekday())
                };
                Some((
                    s,
                    context! {
                        id => id,
                        title => title,
                        guest_name => guest,
                        guest_label => guest_label(lang, &guest, extra),
                        guest_email => email,
                        extra_guests => extra,
                        day => day.to_string(),
                        day_label => day_label,
                        day_short => relative_day(lang, day, today),
                        day_date => crate::i18n::format_short_date(day, lang),
                        is_today => day == today,
                        time => time_label(s),
                        end_time => time_label(e),
                        duration => duration_label(lang, (e - s).num_minutes()),
                        start_utc => s_utc.format("%Y-%m-%dT%H:%M:%SZ").to_string(),
                        end_utc => e_utc.format("%Y-%m-%dT%H:%M:%SZ").to_string(),
                        ongoing => s <= now_local,
                        join_url => join,
                        // Every host in scope may read the booking's host
                        // notes (`booking_notes.rs`), so the row links to them.
                        has_host_notes => has_host_notes,
                    },
                ))
            },
        )
        .collect();
    items.sort_by_key(|(s, _)| *s);
    items.into_iter().take(AGENDA_LEN).map(|(_, v)| v).collect()
}

/// A pending request as a timetable row: when, until, and when it expires
/// (red under a day). Rows sort by expiry, then start.
pub(super) fn pending_row(
    lang: &str,
    start: NaiveDateTime,
    end: NaiveDateTime,
    expires: Option<NaiveDateTime>,
    now: NaiveDateTime,
) -> minijinja::Value {
    let today = now.date();
    let mut args = fluent_bundle::FluentArgs::new();
    args.set("time", time_label(end));
    let until = crate::i18n::translate(lang, "overview-until", Some(&args));
    let expiry = expires.map(|x| {
        let when = if x.date() == today {
            time_label(x)
        } else {
            format!(
                "{} {} {}",
                weekday_short(lang, x.date().weekday()),
                x.date().day(),
                time_label(x)
            )
        };
        let mut args = fluent_bundle::FluentArgs::new();
        args.set("when", when);
        crate::i18n::translate(lang, "overview-expires", Some(&args))
    });
    context! {
        day_time => format!("{} {}", relative_day(lang, start.date(), today), time_label(start)),
        until => until,
        expires => expiry,
        expires_soon => expires.is_some_and(|x| x - now < Duration::hours(24)),
    }
}
