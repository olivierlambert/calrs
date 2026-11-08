//! Instance usage and health figures for the admin panel overview.
//!
//! Every figure is one aggregate query over the live tables, so the page
//! stays cheap with thousands of bookings and hundreds of users: no query
//! runs per row. `load()` takes `now` so tests can pin the clock.

use super::*;
use chrono::DateTime;
use serde::Serialize;

/// Weeks shown on the bookings chart.
pub(super) const CHART_WEEKS: i64 = 12;
/// Window of the "most booked" and "busiest hosts" rankings.
pub(super) const RANKING_DAYS: i64 = 90;
/// Rows kept in each ranking and health list.
const LIST_LIMIT: i64 = 5;
/// A source with no successful sync for this long is listed as stale.
const STALE_HOURS: i64 = 24;

#[derive(Debug, Default, Serialize)]
pub(super) struct UserStats {
    pub total: i64,
    pub enabled: i64,
    pub disabled: i64,
    pub admins: i64,
    pub local: i64,
    pub oidc: i64,
    /// Local accounts with TOTP enrolled.
    pub mfa_enrolled: i64,
    /// Accounts created in the last 30 days, and in the 30 days before.
    pub new_30d: i64,
    pub new_prev_30d: i64,
    /// Enabled accounts that signed in within the last 30 days.
    pub active_30d: i64,
}

#[derive(Debug, Default, Clone, Serialize)]
pub(super) struct BookingStats {
    pub total: i64,
    pub confirmed: i64,
    pub pending: i64,
    pub cancelled: i64,
    pub declined: i64,
    /// Bookings made (any status) in the last 30 days, and the 30 before.
    pub created_30d: i64,
    pub created_prev_30d: i64,
    /// Confirmed bookings starting from now on, and within 7 days.
    pub upcoming: i64,
    pub upcoming_7d: i64,
    /// Pending requests whose deadline falls within 24 hours.
    pub pending_expiring_24h: i64,
}

#[derive(Debug, Default, Clone, Serialize)]
pub(super) struct WeekBucket {
    /// First day of the week, YYYY-MM-DD.
    pub start: String,
    /// The week's first day written out in the reader's language.
    pub label: String,
    /// Axis label: the ISO week number ("S45" on the first labelled week,
    /// then the bare number) in locales that count weeks, "Nov 2" in `en`.
    pub axis_label: String,
    /// Labels are thinned to every other week, counted back from the
    /// current one so that it is always labelled.
    pub show_label: bool,
    /// The week we are in: the only bar drawn in the accent.
    pub is_current: bool,
    /// The tallest bar, labelled with its total like the current one.
    pub is_max: bool,
    pub confirmed: i64,
    pub pending: i64,
    /// Cancelled and declined together. Drawn as a count under the axis,
    /// not stacked on the bar.
    pub ended: i64,
    pub total: i64,
    /// Confirmed plus pending: what the bar stacks.
    pub active: i64,
    /// Bar height as a percentage of the chart's scale.
    pub height_pct: f64,
}

#[derive(Debug, Default, Clone, Serialize)]
pub(super) struct RankRow {
    pub name: String,
    /// Team name for a team meeting type, owner for a personal one, email
    /// for a host.
    pub detail: String,
    pub is_team: bool,
    pub count: i64,
    /// Bar length relative to the first row.
    pub pct: f64,
}

#[derive(Debug, Default, Clone, Serialize)]
pub(super) struct StatusSegment {
    pub key: &'static str,
    pub count: i64,
    pub pct: f64,
}

#[derive(Debug, Default, Serialize)]
pub(super) struct SourceStats {
    pub total: i64,
    pub enabled: i64,
    pub disabled: i64,
    pub failing: i64,
    /// Enabled, not failing, but no successful sync for `STALE_HOURS`.
    pub stale: i64,
    pub healthy: i64,
}

#[derive(Debug, Default, Serialize)]
pub(super) struct FailingSource {
    pub name: String,
    pub provider: String,
    pub owner_name: String,
    pub owner_email: String,
    pub error: String,
    /// ISO 8601 UTC, empty when unknown.
    pub failing_since: String,
    pub failure_count: i64,
}

#[derive(Debug, Default, Serialize)]
pub(super) struct WritebackFailure {
    pub title: String,
    pub host: String,
    /// The booking's start as an ISO 8601 UTC instant, rendered as a
    /// relative time. Empty for a legacy row (`time_version = 0`), whose
    /// stored start is a wall clock in its event type's zone, not UTC.
    pub start: String,
    /// A legacy row's start as stored, "YYYY-MM-DD HH:MM (zone)", shown as
    /// plain text. Empty for UTC rows.
    pub start_local: String,
    pub error: String,
    pub attempts: i64,
    /// No retry scheduled any more.
    pub exhausted: bool,
}

#[derive(Debug, Default, Serialize)]
pub(super) struct FailingResource {
    pub name: String,
    pub error: String,
}

#[derive(Debug, Default, Serialize)]
pub(super) struct SmsStats {
    pub sent_30d: i64,
    pub cost_30d: String,
    pub currency: String,
}

#[derive(Debug, Default, Serialize)]
pub(super) struct AdminStats {
    pub users: UserStats,
    pub bookings: BookingStats,
    pub weekly: Vec<WeekBucket>,
    /// Top of the chart's scale (confirmed plus pending), and its midpoint,
    /// for the gridlines.
    pub weekly_scale: i64,
    pub weekly_mid: i64,
    pub weekly_total: i64,
    pub status_segments: Vec<StatusSegment>,
    pub top_event_types: Vec<RankRow>,
    pub top_hosts: Vec<RankRow>,
    pub sources: SourceStats,
    pub failing_sources: Vec<FailingSource>,
    pub writeback_failure_count: i64,
    pub writeback_failures: Vec<WritebackFailure>,
    pub resources_total: i64,
    pub failing_resources: Vec<FailingResource>,
    pub teams: i64,
    pub groups: i64,
    pub event_types_total: i64,
    pub event_types_enabled: i64,
    pub event_types_team: i64,
    pub sms: SmsStats,
}

/// The booking-wide part of `AdminStats`, the one `FiguresCache` keeps.
#[derive(Debug, Default, Clone)]
pub(super) struct BookingFigures {
    bookings: BookingStats,
    weekly: Vec<WeekBucket>,
    weekly_scale: i64,
    weekly_mid: i64,
    weekly_total: i64,
    status_segments: Vec<StatusSegment>,
    top_event_types: Vec<RankRow>,
    top_hosts: Vec<RankRow>,
}

/// How long `FiguresCache` serves the booking-wide figures.
pub(super) const FIGURES_TTL: std::time::Duration = std::time::Duration::from_secs(60);
/// Figures cheaper than this to compute are not cached, so a small instance
/// always shows live numbers, exactly as before the cache existed.
pub(super) const FIGURES_CACHE_MIN_COST: std::time::Duration = std::time::Duration::from_millis(40);

struct CachedFigures {
    week_start: WeekStart,
    lang: String,
    computed_at: std::time::Instant,
    figures: BookingFigures,
}

/// The booking-wide admin figures of the last minute, shared by every
/// admin. Keyed by the reader's week start and language, which shape the
/// chart's weeks and labels; another key simply recomputes. Expires rather
/// than being invalidated: a booking made meanwhile shows up within a
/// minute, and the page says how old the figures are while it is cached.
pub(crate) struct FiguresCache {
    ttl: std::time::Duration,
    min_cost: std::time::Duration,
    slot: std::sync::Mutex<Option<CachedFigures>>,
}

impl Default for FiguresCache {
    fn default() -> Self {
        Self::new(FIGURES_TTL, FIGURES_CACHE_MIN_COST)
    }
}

impl FiguresCache {
    pub(super) fn new(ttl: std::time::Duration, min_cost: std::time::Duration) -> Self {
        Self {
            ttl,
            min_cost,
            slot: std::sync::Mutex::new(None),
        }
    }

    /// The cached figures and their age in seconds, if fresh for this key.
    fn get(&self, week_start: WeekStart, lang: &str) -> Option<(BookingFigures, u64)> {
        let slot = self.slot.lock().unwrap_or_else(|e| e.into_inner());
        let cached = slot.as_ref()?;
        let age = cached.computed_at.elapsed();
        (cached.week_start == week_start && cached.lang == lang && age < self.ttl)
            .then(|| (cached.figures.clone(), age.as_secs()))
    }

    /// Keep `figures`, computed from `started` on, if they took long
    /// enough to be worth keeping. Their age counts from `started`.
    fn store(
        &self,
        week_start: WeekStart,
        lang: &str,
        figures: &BookingFigures,
        started: std::time::Instant,
    ) {
        let mut slot = self.slot.lock().unwrap_or_else(|e| e.into_inner());
        *slot = (started.elapsed() >= self.min_cost).then(|| CachedFigures {
            week_start,
            lang: lang.to_string(),
            computed_at: started,
            figures: figures.clone(),
        });
    }
}

fn sql_time(t: DateTime<Utc>) -> String {
    t.format("%Y-%m-%d %H:%M:%S").to_string()
}

/// A plain-text lower bound for a stored timestamp at or after `t`, so a
/// `datetime(col) >= datetime(t)` filter can first be narrowed by an index
/// range on `col`. Stored values begin with their date (`datetime('now')`,
/// ISO 8601 with or without `Z`), and one day of slack covers a value
/// written with a UTC offset, whose date can be a day behind its instant.
/// The `datetime()` test then makes the exact cut, as before.
fn day_floor(t: DateTime<Utc>) -> String {
    (t - Duration::days(1)).format("%Y-%m-%d").to_string()
}

/// `YYYY-MM-DD HH:MM:SS` (SQLite `datetime('now')`) to ISO 8601 UTC, for
/// the relative times the page computes client-side. Empty when unparsable.
pub(super) fn iso_utc(stored: &str) -> String {
    let trimmed = stored.trim().trim_end_matches('Z');
    ["%Y-%m-%d %H:%M:%S", "%Y-%m-%dT%H:%M:%S"]
        .iter()
        .find_map(|fmt| NaiveDateTime::parse_from_str(trimmed, fmt).ok())
        .map(|dt| dt.format("%Y-%m-%dT%H:%M:%SZ").to_string())
        .unwrap_or_default()
}

/// A round top for the chart's scale, at least `max` and even, so the
/// midpoint gridline is a whole number too: 2, 4, 6, 8, 10, 12, 16, 20,
/// 30, 40, 50, 60, 80, 100, 120 and so on.
pub(super) fn nice_ceiling(max: i64) -> i64 {
    const STEPS: [i64; 9] = [10, 12, 16, 20, 30, 40, 50, 60, 80];
    for small in [2, 4, 6, 8] {
        if max <= small {
            return small;
        }
    }
    let mut factor = 1;
    loop {
        for step in STEPS {
            if step * factor >= max {
                return step * factor;
            }
        }
        factor *= 10;
    }
}

fn shorten(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    let mut out: String = text.chars().take(max_chars).collect();
    out.push('\u{2026}');
    out
}

fn pct(part: i64, whole: i64) -> f64 {
    if whole <= 0 {
        0.0
    } else {
        ((part as f64 / whole as f64) * 1000.0).round() / 10.0
    }
}

/// Week buckets for the last `CHART_WEEKS` weeks, the current week last,
/// each starting on `week_start`.
pub(super) fn week_starts(today: NaiveDate, week_start: WeekStart) -> Vec<NaiveDate> {
    let current = today - Duration::days(week_start.offset(today) as i64);
    (0..CHART_WEEKS)
        .rev()
        .map(|i| current - Duration::weeks(i))
        .collect()
}

/// Whether the locale counts weeks by ISO number. Every shipped locale
/// does except English, which reads "Nov 2" style dates instead.
fn uses_week_numbers(lang: &str) -> bool {
    !lang.starts_with("en")
}

/// The ISO week a bucket belongs to: that of its Monday, whichever day the
/// chart's weeks start on.
fn iso_week_of(start: NaiveDate) -> u32 {
    (0..7)
        .map(|d| start + Duration::days(d))
        .find(|d| d.weekday() == chrono::Weekday::Mon)
        .unwrap_or(start)
        .iso_week()
        .week()
}

/// Axis labels for the chart's weeks, with whether each is shown. Every
/// other week is labelled, counting back from the current week; the first
/// shown label carries the locale's week prefix ("S34", then "36", "38").
pub(super) fn week_axis_labels(starts: &[NaiveDate], lang: &str) -> Vec<(String, bool)> {
    let n = starts.len();
    let mut prefixed = false;
    starts
        .iter()
        .enumerate()
        .map(|(i, start)| {
            let shown = (n - 1 - i).is_multiple_of(2);
            let label = if uses_week_numbers(lang) {
                let week = iso_week_of(*start);
                if shown && !prefixed {
                    prefixed = true;
                    let mut args = fluent_bundle::FluentArgs::new();
                    args.set("week", week as i64);
                    crate::i18n::translate(lang, "admin-chart-week-short", Some(&args))
                } else {
                    week.to_string()
                }
            } else {
                let month =
                    crate::i18n::translate(lang, &format!("common-month-{}", start.month()), None);
                let short: String = month.chars().take(3).collect();
                format!("{short} {}", start.day())
            };
            (label, shown)
        })
        .collect()
}

/// The figures that read every booking of a window: totals, the weekly
/// chart and the 90-day rankings. On a large instance they are most of the
/// page's cost, so `load_cached` keeps them for a minute.
async fn booking_figures(
    pool: &SqlitePool,
    now: DateTime<Utc>,
    week_start: WeekStart,
    lang: &str,
) -> BookingFigures {
    let t_now = sql_time(now);
    let t_30 = sql_time(now - Duration::days(30));
    let t_60 = sql_time(now - Duration::days(60));
    let t_7d_ahead = sql_time(now + Duration::days(7));
    let t_24h_ahead = sql_time(now + Duration::hours(24));
    let t_rank = sql_time(now - Duration::days(RANKING_DAYS));
    let mut stats = BookingFigures::default();

    // --- Bookings ---
    // `start_at` is compared as UTC. Legacy rows (`time_version = 0`) store
    // a wall clock in the event type's zone, so their upcoming counts can
    // be off by at most that zone's offset around the cut: close enough
    // for an overview figure, not worth a per-row conversion.
    // Four reads rather than one pass computing every figure per row: each
    // windowed figure is narrowed by an index first (see `day_floor`), so
    // `datetime()` runs on the rows of the window, not on every booking.
    let (total, confirmed, pending, cancelled, declined): (i64, i64, i64, i64, i64) =
        sqlx::query_as(
            "SELECT COUNT(*),
                    COALESCE(SUM(status = 'confirmed'), 0),
                    COALESCE(SUM(status = 'pending'), 0),
                    COALESCE(SUM(status = 'cancelled'), 0),
                    COALESCE(SUM(status = 'declined'), 0)
             FROM bookings",
        )
        .fetch_one(pool)
        .await
        .unwrap_or_default();
    let (created_30d, created_prev_30d): (i64, i64) = sqlx::query_as(
        "SELECT COALESCE(SUM(datetime(created_at) >= datetime(?)), 0),
                COALESCE(SUM(datetime(created_at) >= datetime(?) AND datetime(created_at) < datetime(?)), 0)
         FROM bookings WHERE created_at >= ?",
    )
    .bind(&t_30)
    .bind(&t_60)
    .bind(&t_30)
    .bind(day_floor(now - Duration::days(60)))
    .fetch_one(pool)
    .await
    .unwrap_or_default();
    let (upcoming, upcoming_7d): (i64, i64) = sqlx::query_as(
        "SELECT COALESCE(SUM(datetime(start_at) >= datetime(?)), 0),
                COALESCE(SUM(datetime(start_at) >= datetime(?) AND datetime(start_at) < datetime(?)), 0)
         FROM bookings WHERE status = 'confirmed' AND start_at >= ?",
    )
    .bind(&t_now)
    .bind(&t_now)
    .bind(&t_7d_ahead)
    .bind(day_floor(now))
    .fetch_one(pool)
    .await
    .unwrap_or_default();
    let pending_expiring_24h: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM bookings
         WHERE status = 'pending' AND pending_expires_at IS NOT NULL
           AND datetime(pending_expires_at) <= datetime(?)",
    )
    .bind(&t_24h_ahead)
    .fetch_one(pool)
    .await
    .unwrap_or(0);
    stats.bookings = BookingStats {
        total,
        confirmed,
        pending,
        cancelled,
        declined,
        created_30d,
        created_prev_30d,
        upcoming,
        upcoming_7d,
        pending_expiring_24h,
    };
    let b = &stats.bookings;
    stats.status_segments = [
        ("confirmed", b.confirmed),
        ("pending", b.pending),
        ("cancelled", b.cancelled),
        ("declined", b.declined),
    ]
    .into_iter()
    .map(|(key, count)| StatusSegment {
        key,
        count,
        pct: pct(count, b.total),
    })
    .collect();

    // --- Bookings per week, by the day they were made ---
    let starts = week_starts(now.date_naive(), week_start);
    let first = starts[0];
    let per_day: Vec<(String, String, i64)> = sqlx::query_as(
        "SELECT date(created_at) AS day, status, COUNT(*) FROM bookings
         WHERE created_at >= ? AND date(created_at) >= ? GROUP BY day, status",
    )
    .bind(day_floor(first.and_time(NaiveTime::MIN).and_utc()))
    .bind(first.format("%Y-%m-%d").to_string())
    .fetch_all(pool)
    .await
    .unwrap_or_default();
    let labels = week_axis_labels(&starts, lang);
    let mut weekly: Vec<WeekBucket> = starts
        .iter()
        .zip(labels)
        .enumerate()
        .map(|(i, (start, (axis_label, show_label)))| WeekBucket {
            start: start.format("%Y-%m-%d").to_string(),
            label: crate::i18n::format_date(*start, lang),
            axis_label,
            show_label,
            is_current: i + 1 == starts.len(),
            ..Default::default()
        })
        .collect();
    for (day, status, count) in per_day {
        let Ok(date) = NaiveDate::parse_from_str(&day, "%Y-%m-%d") else {
            continue;
        };
        let Some(idx) = starts.iter().rposition(|s| *s <= date) else {
            continue;
        };
        let bucket = &mut weekly[idx];
        match status.as_str() {
            "confirmed" => bucket.confirmed += count,
            "pending" => bucket.pending += count,
            _ => bucket.ended += count,
        }
        bucket.total += count;
    }
    for w in &mut weekly {
        w.active = w.confirmed + w.pending;
    }
    let max = weekly.iter().map(|w| w.active).max().unwrap_or(0);
    let scale = nice_ceiling(max);
    // Only the first tallest bar gets the label, and none on an empty chart.
    let max_idx = weekly.iter().position(|w| max > 0 && w.active == max);
    for (i, w) in weekly.iter_mut().enumerate() {
        w.height_pct = pct(w.active, scale);
        w.is_max = Some(i) == max_idx;
    }
    stats.weekly_total = weekly.iter().map(|w| w.total).sum();
    stats.weekly_scale = scale;
    stats.weekly_mid = scale / 2;
    stats.weekly = weekly;

    // --- Rankings over the last 90 days, confirmed bookings only ---
    // Counted per event type first, then joined: the joins run once per
    // event type instead of once per booking. `+status` keeps the planner on
    // the creation date index: grouping by event type would otherwise lead
    // it to walk every booking in event type order.
    let rows: Vec<(String, Option<String>, bool, i64)> = sqlx::query_as(
        "SELECT et.title, COALESCE(t.name, u.name), et.team_id IS NOT NULL, c.n
         FROM (SELECT event_type_id, COUNT(*) AS n FROM bookings
               WHERE +status = 'confirmed' AND created_at >= ?
                 AND datetime(created_at) >= datetime(?)
               GROUP BY event_type_id) c
         JOIN event_types et ON et.id = c.event_type_id
         LEFT JOIN teams t ON t.id = et.team_id
         LEFT JOIN accounts a ON a.id = et.account_id
         LEFT JOIN users u ON u.id = a.user_id
         ORDER BY c.n DESC, et.title LIMIT ?",
    )
    .bind(day_floor(now - Duration::days(RANKING_DAYS)))
    .bind(&t_rank)
    .bind(LIST_LIMIT)
    .fetch_all(pool)
    .await
    .unwrap_or_default();
    let top = rows.first().map(|r| r.3).unwrap_or(0);
    stats.top_event_types = rows
        .into_iter()
        .map(|(name, detail, is_team, count)| RankRow {
            name,
            detail: detail.unwrap_or_default(),
            is_team,
            count,
            pct: pct(count, top),
        })
        .collect();

    // The host is the assigned member for round-robin bookings, the event
    // type's owner otherwise (personal and collective). Counted per event
    // type and assigned member first, as above.
    let rows: Vec<(String, String, i64)> = sqlx::query_as(
        "SELECT u.name, u.email, SUM(c.n) AS n
         FROM (SELECT event_type_id, assigned_user_id, COUNT(*) AS n FROM bookings
               WHERE +status = 'confirmed' AND created_at >= ?
                 AND datetime(created_at) >= datetime(?)
               GROUP BY event_type_id, assigned_user_id) c
         JOIN event_types et ON et.id = c.event_type_id
         LEFT JOIN accounts a ON a.id = et.account_id
         JOIN users u ON u.id = COALESCE(c.assigned_user_id, a.user_id)
         GROUP BY u.id ORDER BY n DESC, u.name LIMIT ?",
    )
    .bind(day_floor(now - Duration::days(RANKING_DAYS)))
    .bind(&t_rank)
    .bind(LIST_LIMIT)
    .fetch_all(pool)
    .await
    .unwrap_or_default();
    let top = rows.first().map(|r| r.2).unwrap_or(0);
    stats.top_hosts = rows
        .into_iter()
        .map(|(name, email, count)| RankRow {
            name,
            detail: email,
            is_team: false,
            count,
            pct: pct(count, top),
        })
        .collect();

    stats
}

pub(super) async fn load(
    pool: &SqlitePool,
    now: DateTime<Utc>,
    week_start: WeekStart,
    lang: &str,
) -> AdminStats {
    let figures = booking_figures(pool, now, week_start, lang).await;
    live_stats(pool, now, figures).await
}

/// The figures with the booking-wide figures taken from `cache` when it
/// holds fresh ones, and the age in seconds of those it served.
pub(super) async fn load_cached(
    pool: &SqlitePool,
    cache: &FiguresCache,
    now: DateTime<Utc>,
    week_start: WeekStart,
    lang: &str,
) -> (AdminStats, Option<u64>) {
    let (figures, age) = match cache.get(week_start, lang) {
        Some((figures, age)) => (figures, Some(age)),
        None => {
            let started = std::time::Instant::now();
            let figures = booking_figures(pool, now, week_start, lang).await;
            cache.store(week_start, lang, &figures, started);
            (figures, None)
        }
    };
    (live_stats(pool, now, figures).await, age)
}

/// Everything except the booking-wide figures: cheap, and what the
/// attention list and the Users tab counts read, so always live.
async fn live_stats(pool: &SqlitePool, now: DateTime<Utc>, figures: BookingFigures) -> AdminStats {
    let t_now = sql_time(now);
    let t_30 = sql_time(now - Duration::days(30));
    let t_60 = sql_time(now - Duration::days(60));
    let t_stale = sql_time(now - Duration::hours(STALE_HOURS));

    let mut stats = AdminStats {
        bookings: figures.bookings,
        weekly: figures.weekly,
        weekly_scale: figures.weekly_scale,
        weekly_mid: figures.weekly_mid,
        weekly_total: figures.weekly_total,
        status_segments: figures.status_segments,
        top_event_types: figures.top_event_types,
        top_hosts: figures.top_hosts,
        ..AdminStats::default()
    };

    // --- Users ---
    let row: (i64, i64, i64, i64, i64, i64, i64, i64, i64) = sqlx::query_as(
        "SELECT COUNT(*),
                COALESCE(SUM(enabled = 1), 0),
                COALESCE(SUM(role = 'admin'), 0),
                COALESCE(SUM(auth_provider = 'local'), 0),
                COALESCE(SUM(auth_provider = 'oidc'), 0),
                COALESCE(SUM(auth_provider = 'local'
                    AND EXISTS (SELECT 1 FROM user_mfa m WHERE m.user_id = users.id)), 0),
                COALESCE(SUM(datetime(created_at) >= datetime(?)), 0),
                COALESCE(SUM(datetime(created_at) >= datetime(?) AND datetime(created_at) < datetime(?)), 0),
                COALESCE(SUM(enabled = 1 AND last_login_at IS NOT NULL
                    AND datetime(last_login_at) >= datetime(?)), 0)
         FROM users",
    )
    .bind(&t_30)
    .bind(&t_60)
    .bind(&t_30)
    .bind(&t_30)
    .fetch_one(pool)
    .await
    .unwrap_or_default();
    stats.users = UserStats {
        total: row.0,
        enabled: row.1,
        disabled: row.0 - row.1,
        admins: row.2,
        local: row.3,
        oidc: row.4,
        mfa_enrolled: row.5,
        new_30d: row.6,
        new_prev_30d: row.7,
        active_30d: row.8,
    };

    // --- Calendar sources ---
    let row: (i64, i64, i64, i64) = sqlx::query_as(
        "SELECT COUNT(*),
                COALESCE(SUM(enabled = 1), 0),
                COALESCE(SUM(enabled = 1 AND last_sync_error IS NOT NULL), 0),
                COALESCE(SUM(enabled = 1 AND last_sync_error IS NULL
                    AND (last_synced IS NULL OR datetime(last_synced) < datetime(?))), 0)
         FROM caldav_sources",
    )
    .bind(&t_stale)
    .fetch_one(pool)
    .await
    .unwrap_or_default();
    stats.sources = SourceStats {
        total: row.0,
        enabled: row.1,
        disabled: row.0 - row.1,
        failing: row.2,
        stale: row.3,
        healthy: row.1 - row.2 - row.3,
    };
    let rows: Vec<(
        String,
        String,
        Option<String>,
        Option<String>,
        String,
        Option<String>,
        i64,
    )> = sqlx::query_as(
        "SELECT cs.name, cs.provider_type, u.name, u.email, cs.last_sync_error,
                    cs.sync_failing_since, cs.sync_failure_count
             FROM caldav_sources cs
             JOIN accounts a ON a.id = cs.account_id
             LEFT JOIN users u ON u.id = a.user_id
             WHERE cs.enabled = 1 AND cs.last_sync_error IS NOT NULL
             ORDER BY cs.sync_failing_since, cs.name LIMIT ?",
    )
    .bind(LIST_LIMIT * 2)
    .fetch_all(pool)
    .await
    .unwrap_or_default();
    stats.failing_sources = rows
        .into_iter()
        .map(
            |(name, provider, owner_name, owner_email, error, since, count)| FailingSource {
                name,
                provider,
                owner_name: owner_name.unwrap_or_default(),
                owner_email: owner_email.unwrap_or_default(),
                error: shorten(&error, 240),
                failing_since: since.as_deref().map(iso_utc).unwrap_or_default(),
                failure_count: count,
            },
        )
        .collect();

    // --- Write-back failures on upcoming confirmed bookings ---
    // Counted against UTC like the booking figures above (same caveat for
    // legacy rows); each listed row is rendered according to its version.
    stats.writeback_failure_count = sqlx::query_scalar(
        "SELECT COUNT(*) FROM bookings
         WHERE status = 'confirmed' AND writeback_error IS NOT NULL
           AND start_at >= ? AND datetime(start_at) >= datetime(?)",
    )
    .bind(day_floor(now))
    .bind(&t_now)
    .fetch_one(pool)
    .await
    .unwrap_or(0);
    if stats.writeback_failure_count > 0 {
        #[allow(clippy::type_complexity)]
        let rows: Vec<(
            String,
            Option<String>,
            String,
            String,
            i64,
            bool,
            i64,
            String,
        )> = sqlx::query_as(
            "SELECT et.title, COALESCE(hu.name, ou.name), b.start_at, b.writeback_error,
                    b.writeback_attempts, b.writeback_next_attempt_at IS NULL, b.time_version,
                    COALESCE(NULLIF(et.timezone, ''), ou.timezone, 'UTC')
             FROM bookings b
             JOIN event_types et ON et.id = b.event_type_id
             LEFT JOIN accounts a ON a.id = et.account_id
             LEFT JOIN users ou ON ou.id = a.user_id
             LEFT JOIN users hu ON hu.id = b.assigned_user_id
             WHERE b.status = 'confirmed' AND b.writeback_error IS NOT NULL
               AND b.start_at >= ? AND datetime(b.start_at) >= datetime(?)
             ORDER BY datetime(b.start_at) LIMIT ?",
        )
        .bind(day_floor(now))
        .bind(&t_now)
        .bind(LIST_LIMIT * 2)
        .fetch_all(pool)
        .await
        .unwrap_or_default();
        stats.writeback_failures = rows
            .into_iter()
            .map(
                |(title, host, start, error, attempts, exhausted, version, zone)| {
                    let stored: String = start.trim_end_matches('Z').chars().take(19).collect();
                    let (start, start_local) = if version == 1 {
                        // ISO UTC instant, rendered as a localized relative time.
                        (format!("{stored}Z"), String::new())
                    } else {
                        let wall: String = stored.replacen('T', " ", 1).chars().take(16).collect();
                        (String::new(), format!("{wall} ({zone})"))
                    };
                    WritebackFailure {
                        title,
                        host: host.unwrap_or_default(),
                        start,
                        start_local,
                        error: shorten(&error, 240),
                        attempts,
                        exhausted,
                    }
                },
            )
            .collect();
    }

    // --- Shared resources ---
    stats.resources_total = sqlx::query_scalar("SELECT COUNT(*) FROM resources")
        .fetch_one(pool)
        .await
        .unwrap_or(0);
    let rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT name, last_sync_error FROM resources
         WHERE last_sync_error IS NOT NULL ORDER BY name",
    )
    .fetch_all(pool)
    .await
    .unwrap_or_default();
    stats.failing_resources = rows
        .into_iter()
        .map(|(name, error)| FailingResource {
            name,
            error: shorten(&error, 240),
        })
        .collect();

    // --- Inventory ---
    stats.teams = sqlx::query_scalar("SELECT COUNT(*) FROM teams")
        .fetch_one(pool)
        .await
        .unwrap_or(0);
    stats.groups = sqlx::query_scalar("SELECT COUNT(*) FROM groups")
        .fetch_one(pool)
        .await
        .unwrap_or(0);
    let row: (i64, i64, i64) = sqlx::query_as(
        "SELECT COUNT(*), COALESCE(SUM(enabled = 1), 0), COALESCE(SUM(team_id IS NOT NULL), 0)
         FROM event_types",
    )
    .fetch_one(pool)
    .await
    .unwrap_or_default();
    stats.event_types_total = row.0;
    stats.event_types_enabled = row.1;
    stats.event_types_team = row.2;

    // --- SMS, last 30 days ---
    let row: (i64, Option<f64>, Option<String>) = sqlx::query_as(
        "SELECT COUNT(*), SUM(cost), MAX(currency) FROM sms_usage
         WHERE datetime(sent_at) >= datetime(?)",
    )
    .bind(&t_30)
    .fetch_one(pool)
    .await
    .unwrap_or_default();
    stats.sms = SmsStats {
        sent_30d: row.0,
        cost_30d: row.1.map(|c| format!("{c:.2}")).unwrap_or_default(),
        currency: row.2.unwrap_or_default(),
    };

    stats
}
