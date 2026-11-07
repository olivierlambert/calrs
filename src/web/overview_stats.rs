//! Dashboard overview analytics: the KPI strip, weekly volume, busiest
//! hours, the split by meeting type and the "next up" agenda.
//!
//! Every figure here is about the meetings the signed-in user takes part
//! in, using the same rule as the slot engine (`user_busy_bookings` in
//! `booking_time.rs`) minus its status filter: a booking assigned to them,
//! an unassigned booking on an event type they own, an unassigned
//! collective booking on a team they belong to, plus a booking they
//! claimed as a watcher. A round-robin booking assigned to another member
//! does not count for the event type's creator.
//!
//! The SQL only narrows rows to a date window (two indexed range checks);
//! the bucketing happens in Rust because legacy rows store a wall-clock
//! time in the event type's zone while newer rows store UTC, and only
//! `booking_time::local` reads both correctly.

use super::*;

/// Number of weeks in the volume chart and the KPI sparklines, the
/// current week included.
pub(super) const WEEKS: usize = 12;

/// How many upcoming meetings the agenda lists.
const AGENDA_LEN: usize = 5;

/// Scope predicate, see the module documentation. Aliases `b`, `et` and
/// `a` (the event type's account, LEFT JOINed: team event types may have
/// none). Binds the user id four times.
const HOST_SCOPE: &str = "(b.assigned_user_id = ?
    OR (b.assigned_user_id IS NULL AND a.user_id = ?)
    OR (b.assigned_user_id IS NULL AND et.team_id IS NOT NULL AND EXISTS (
          SELECT 1 FROM team_members tm WHERE tm.team_id = et.team_id AND tm.user_id = ?))
    OR b.claimed_by_user_id = ?)";

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
    /// Bookings made (created) this week, any status.
    pub booked: i64,
    /// Minutes of confirmed meetings starting this week.
    pub minutes: i64,
}

#[derive(Debug, Clone, Default)]
pub(super) struct OverviewStats {
    pub next7_count: i64,
    pub next7_minutes: i64,
    pub booked30: i64,
    pub booked_prev30: i64,
    pub minutes30: i64,
    pub minutes_prev30: i64,
    pub held30: i64,
    pub cancelled30: i64,
    pub held_prev30: i64,
    pub cancelled_prev30: i64,
    pub weeks: Vec<WeekBucket>,
    /// Confirmed meetings in the chart window, by display row (0 is the
    /// first day of the user's week) and hour of day.
    pub heat: [[i64; 24]; 7],
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
    let d7 = now + Duration::days(7);

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
                let day = week_start.offset(row.start.date()) as usize;
                s.heat[day][row.start.hour() as usize] += 1;
                by_type
                    .entry(&row.event_type_id)
                    .or_insert((&row.event_title, 0))
                    .1 += 1;
            }
        }
        if let Some(created) = row.created {
            if let Some(w) = week_of(created) {
                s.weeks[w].booked += 1;
            }
            if in_range(created, d30, now) {
                s.booked30 += 1;
            } else if in_range(created, d60, d30) {
                s.booked_prev30 += 1;
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
        if confirmed && in_range(row.start, now, d7) {
            s.next7_count += 1;
            s.next7_minutes += minutes(row);
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

/// "2 h 30 min" in the reader's language.
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

/// Time of day: 12-hour clock for English, 24-hour everywhere else.
pub(super) fn time_label(lang: &str, t: NaiveDateTime) -> String {
    if lang == "en" {
        t.format("%-I:%M %p").to_string()
    } else {
        t.format("%H:%M").to_string()
    }
}

fn day_month_label(lang: &str, date: NaiveDate) -> String {
    let mut args = fluent_bundle::FluentArgs::new();
    args.set("day", crate::i18n::number(date.day() as f64));
    args.set("month", crate::i18n::number(date.month() as f64));
    crate::i18n::translate(lang, "overview-day-month", Some(&args))
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

/// A delta badge: direction, whether that direction is good news, and the
/// signed text. `None` when there is nothing to compare against.
fn delta(text: String, diff: i64, up_is_good: bool) -> minijinja::Value {
    let dir = match diff.signum() {
        1 => "up",
        -1 => "down",
        _ => "flat",
    };
    let tone = match (diff.signum(), up_is_good) {
        (0, _) => "neutral",
        (1, true) | (-1, false) => "good",
        _ => "bad",
    };
    context! { text => text, dir => dir, tone => tone }
}

fn signed(diff: i64, body: String) -> String {
    match diff.signum() {
        1 => format!("+{body}"),
        -1 => format!("\u{2212}{body}"),
        _ => body,
    }
}

/// Sparkline geometry in a 100 x 30 box: the polyline points and where the
/// last value sits, in percent, for the end dot.
fn sparkline(values: &[i64]) -> minijinja::Value {
    let max = values.iter().copied().max().unwrap_or(0).max(1) as f64;
    let n = values.len().max(2) - 1;
    let point = |i: usize, v: i64| {
        let x = i as f64 * 100.0 / n as f64;
        let y = 28.0 - (v as f64 / max) * 26.0;
        (x, y)
    };
    let points = values
        .iter()
        .enumerate()
        .map(|(i, v)| {
            let (x, y) = point(i, *v);
            format!("{x:.1},{y:.1}")
        })
        .collect::<Vec<_>>()
        .join(" ");
    let (lx, ly) = point(values.len().saturating_sub(1), *values.last().unwrap_or(&0));
    context! {
        points => points,
        area => format!("0,30 {points} 100,30"),
        dot_x => format!("{lx:.1}"),
        dot_y => format!("{:.1}", ly * 100.0 / 30.0),
    }
}

/// Smallest of 1, 2 or 5 times a power of ten that is at least `v`.
fn nice_max(v: i64) -> i64 {
    if v <= 2 {
        return 2;
    }
    let mut base = 1;
    loop {
        for m in [1, 2, 5] {
            if m * base >= v {
                return m * base;
            }
        }
        base *= 10;
    }
}

fn pct(v: i64, of: i64) -> String {
    if of <= 0 {
        return "0".into();
    }
    format!("{:.2}", v as f64 * 100.0 / of as f64)
}

/// Everything the template needs, already formatted in `lang`.
pub(super) fn stats_context(
    s: &OverviewStats,
    lang: &str,
    week_start: WeekStart,
) -> minijinja::Value {
    let weekly = |f: fn(&WeekBucket) -> i64| s.weeks.iter().map(f).collect::<Vec<_>>();

    // KPI strip.
    let next7 = context! {
        label => t(lang, "overview-kpi-next7"),
        value => s.next7_count.to_string(),
        caption => {
            let mut args = fluent_bundle::FluentArgs::new();
            args.set("duration", duration_label(lang, s.next7_minutes));
            crate::i18n::translate(lang, "overview-kpi-next7-caption", Some(&args))
        },
        spark => sparkline(&weekly(|w| w.confirmed)),
        spark_label => t(lang, "overview-kpi-spark-meetings"),
        href => "/dashboard/bookings",
    };
    let booked_diff = s.booked30 - s.booked_prev30;
    let booked = context! {
        label => t(lang, "overview-kpi-booked"),
        value => s.booked30.to_string(),
        delta => delta(signed(booked_diff, booked_diff.abs().to_string()), booked_diff, true),
        caption => t(lang, "overview-delta-caption"),
        spark => sparkline(&weekly(|w| w.booked)),
        spark_label => t(lang, "overview-kpi-spark-booked"),
        href => "/dashboard/bookings",
    };
    let minutes_diff = s.minutes30 - s.minutes_prev30;
    let time_tile = context! {
        label => t(lang, "overview-kpi-hours"),
        value => duration_label(lang, s.minutes30),
        delta => delta(signed(minutes_diff, duration_label(lang, minutes_diff)), minutes_diff, true),
        caption => t(lang, "overview-delta-caption"),
        spark => sparkline(&weekly(|w| w.minutes)),
        spark_label => t(lang, "overview-kpi-spark-hours"),
        href => "/dashboard/bookings",
    };
    let rate = cancel_rate(s.cancelled30, s.held30);
    let prev_rate = cancel_rate(s.cancelled_prev30, s.held_prev30);
    let cancel = match rate {
        Some(r) => {
            let caption = {
                let mut args = fluent_bundle::FluentArgs::new();
                args.set("cancelled", crate::i18n::number(s.cancelled30 as f64));
                args.set(
                    "total",
                    crate::i18n::number((s.cancelled30 + s.held30) as f64),
                );
                crate::i18n::translate(lang, "overview-kpi-cancel-caption", Some(&args))
            };
            let d = prev_rate.map(|p| {
                let diff = r - p;
                delta(
                    signed(
                        diff,
                        tr1(lang, "overview-delta-points", "value", diff.abs()),
                    ),
                    diff,
                    false,
                )
            });
            context! {
                label => t(lang, "overview-kpi-cancel"),
                value => percent_label(lang, r),
                delta => d,
                caption => caption,
                href => "/dashboard/bookings",
            }
        }
        None => context! {
            label => t(lang, "overview-kpi-cancel"),
            value => "\u{2013}",
            caption => t(lang, "overview-kpi-cancel-empty"),
            href => "/dashboard/bookings",
        },
    };

    // Weekly volume chart.
    let peak = s
        .weeks
        .iter()
        .map(|w| w.confirmed + w.pending + w.cancelled)
        .max()
        .unwrap_or(0);
    let axis_max = nice_max(peak);
    let last = s.weeks.len().saturating_sub(1);
    let columns: Vec<minijinja::Value> = s
        .weeks
        .iter()
        .enumerate()
        .map(|(i, w)| {
            let date = w.start.unwrap_or_default();
            let date_label = day_month_label(lang, date);
            let title = if i == last {
                t(lang, "overview-chart-this-week")
            } else {
                let mut args = fluent_bundle::FluentArgs::new();
                args.set("date", date_label.clone());
                crate::i18n::translate(lang, "overview-chart-week-of", Some(&args))
            };
            let total = w.confirmed + w.pending + w.cancelled;
            context! {
                title => title,
                tick => if i == last { t(lang, "overview-chart-this-week") } else { date_label },
                show_tick => i == last || (i % 3 == 0 && i + 3 < last),
                current => i == last,
                total => total,
                confirmed => w.confirmed,
                pending => w.pending,
                cancelled => w.cancelled,
                total_pct => pct(total, axis_max),
            }
        })
        .collect();

    // Busiest times: hours with any meeting, padded to a working day.
    let used: Vec<usize> = (0..24)
        .filter(|h| s.heat.iter().any(|row| row[*h] > 0))
        .collect();
    let first_hour = used.first().copied().unwrap_or(9).min(8);
    let last_hour = used.last().copied().unwrap_or(16).max(17);
    let heat_max = s.heat.iter().flatten().copied().max().unwrap_or(0);
    let anchor = NaiveDate::from_ymd_opt(2024, 1, 7).unwrap_or_default(); // a Sunday
    let heat_rows: Vec<minijinja::Value> = (0..7)
        .map(|row| {
            let day =
                (anchor + Duration::days((week_start.index() as i64 + row as i64) % 7)).weekday();
            let long = weekday_long(lang, day);
            let cells: Vec<minijinja::Value> = (first_hour..=last_hour)
                .map(|h| {
                    let n = s.heat[row][h];
                    let level = if n == 0 || heat_max == 0 {
                        0
                    } else {
                        ((n * 4 + heat_max - 1) / heat_max).clamp(1, 4)
                    };
                    let mut args = fluent_bundle::FluentArgs::new();
                    args.set("day", long.clone());
                    args.set("hour", format!("{h:02}:00"));
                    args.set("count", crate::i18n::number(n as f64));
                    context! {
                        level => level,
                        tip => crate::i18n::translate(lang, "overview-heat-tip", Some(&args)),
                    }
                })
                .collect();
            context! { label => weekday_short(lang, day), long => long, cells => cells }
        })
        .collect();
    let hours: Vec<minijinja::Value> = (first_hour..=last_hour)
        .map(|h| context! { label => format!("{h}"), show => h % 2 == 0 })
        .collect();

    // Meeting types: the five busiest, the rest folded into one row.
    let mut types: Vec<(String, i64)> = s.by_type.iter().take(5).cloned().collect();
    let rest: i64 = s.by_type.iter().skip(5).map(|(_, n)| n).sum();
    if rest > 0 {
        types.push((t(lang, "overview-types-other"), rest));
    }
    let type_total: i64 = types.iter().map(|(_, n)| n).sum();
    let type_max = types.iter().map(|(_, n)| *n).max().unwrap_or(0);
    let type_rows: Vec<minijinja::Value> = types
        .iter()
        .map(|(title, n)| {
            let share = if type_total > 0 {
                (*n as f64 * 100.0 / type_total as f64).round() as i64
            } else {
                0
            };
            context! {
                title => title,
                count => n,
                width => pct(*n, type_max),
                share => percent_label(lang, share),
            }
        })
        .collect();

    context! {
        kpis => vec![next7, booked, time_tile, cancel],
        columns => columns,
        axis_max => axis_max,
        axis_mid => axis_max / 2,
        has_chart_data => s.chart_total > 0,
        heat_rows => heat_rows,
        heat_hours => hours,
        has_heat => heat_max > 0,
        type_rows => type_rows,
    }
}

// --- Agenda ---

/// The next confirmed meetings in scope, ongoing ones included.
pub(super) async fn agenda(
    pool: &SqlitePool,
    user_id: &str,
    host_tz: Tz,
    now: chrono::DateTime<Utc>,
    lang: &str,
) -> Vec<minijinja::Value> {
    let lo = (now - Duration::days(2))
        .format("%Y-%m-%dT%H:%M:%S")
        .to_string();
    let sql = format!(
        "SELECT {START_AT}, {END_AT}, {STORED_TZ}, et.title, b.guest_name, b.guest_email,
                COALESCE(NULLIF(b.meeting_url, ''), et.location_value, ''),
                (SELECT COUNT(*) FROM booking_attendees ba WHERE ba.booking_id = b.id)
         {FROM_SCOPED}
         WHERE {HOST_SCOPE} AND b.status = 'confirmed' AND b.end_at >= ?
         ORDER BY b.start_at
         LIMIT 20"
    );
    let rows: Vec<(String, String, String, String, String, String, String, i64)> =
        sqlx::query_as(&sql)
            .bind(user_id)
            .bind(user_id)
            .bind(user_id)
            .bind(user_id)
            .bind(&lo)
            .fetch_all(pool)
            .await
            .unwrap_or_default();

    let now_local = now.with_timezone(&host_tz).naive_local();
    let today = now_local.date();
    let mut items: Vec<(NaiveDateTime, minijinja::Value)> = rows
        .into_iter()
        .filter_map(|(start, end, stored, title, guest, email, location, extra)| {
            let stored = stored.parse::<Tz>().unwrap_or(Tz::UTC);
            let s = crate::booking_time::local(&start, stored, host_tz)?;
            let e = crate::booking_time::local(&end, stored, host_tz)?;
            if e <= now_local {
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
                crate::i18n::format_long_date(day, lang)
            };
            Some((
                s,
                context! {
                    title => title,
                    guest_name => guest,
                    guest_email => email,
                    extra_guests => extra,
                    day => day.to_string(),
                    day_label => day_label,
                    time => time_label(lang, s),
                    range => format!("{} \u{2013} {}", time_label(lang, s), time_label(lang, e)),
                    duration => duration_label(lang, (e - s).num_minutes()),
                    start_utc => s_utc.format("%Y-%m-%dT%H:%M:%SZ").to_string(),
                    end_utc => e_utc.format("%Y-%m-%dT%H:%M:%SZ").to_string(),
                    ongoing => s <= now_local,
                    join_url => join,
                },
            ))
        })
        .collect();
    items.sort_by_key(|(s, _)| *s);
    items.into_iter().take(AGENDA_LEN).map(|(_, v)| v).collect()
}
