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

#[derive(Debug, Default, Serialize)]
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

/// A change against the previous period, ready for the template.
#[derive(Debug, Default, Serialize, PartialEq, Eq)]
pub(super) struct Delta {
    /// "up", "down", "flat", or "new" when the previous period was empty.
    pub direction: &'static str,
    /// Absolute percentage, rounded; 0 for "flat" and "new".
    pub pct: i64,
}

pub(super) fn delta(current: i64, previous: i64) -> Delta {
    if current == previous {
        return Delta {
            direction: "flat",
            pct: 0,
        };
    }
    if previous == 0 {
        return Delta {
            direction: "new",
            pct: 0,
        };
    }
    let pct = (((current - previous) as f64 / previous as f64) * 100.0).round() as i64;
    Delta {
        direction: if current > previous { "up" } else { "down" },
        pct: pct.abs(),
    }
}

#[derive(Debug, Default, Serialize)]
pub(super) struct WeekBucket {
    /// First day of the week, YYYY-MM-DD.
    pub start: String,
    /// The week's first day written out in the reader's language.
    pub label: String,
    /// Day of month for the axis, and the month name on the first week of
    /// the chart and on each week that starts a new month.
    pub axis_day: u32,
    pub axis_month: String,
    pub confirmed: i64,
    pub pending: i64,
    /// Cancelled and declined together.
    pub ended: i64,
    pub total: i64,
    /// Bar height as a percentage of the chart's scale.
    pub height_pct: f64,
}

#[derive(Debug, Default, Serialize)]
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

#[derive(Debug, Default, Serialize)]
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
    /// The booking's start as stored, YYYY-MM-DD HH:MM (UTC for new rows).
    pub start: String,
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
    pub bookings_delta: Delta,
    pub users_delta: Delta,
    pub weekly: Vec<WeekBucket>,
    /// Top of the chart's scale, and its midpoint, for the gridlines.
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

fn sql_time(t: DateTime<Utc>) -> String {
    t.format("%Y-%m-%d %H:%M:%S").to_string()
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

pub(super) async fn load(
    pool: &SqlitePool,
    now: DateTime<Utc>,
    week_start: WeekStart,
    lang: &str,
) -> AdminStats {
    let t_now = sql_time(now);
    let t_30 = sql_time(now - Duration::days(30));
    let t_60 = sql_time(now - Duration::days(60));
    let t_7d_ahead = sql_time(now + Duration::days(7));
    let t_24h_ahead = sql_time(now + Duration::hours(24));
    let t_rank = sql_time(now - Duration::days(RANKING_DAYS));
    let t_stale = sql_time(now - Duration::hours(STALE_HOURS));

    let mut stats = AdminStats::default();

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
    stats.users_delta = delta(stats.users.new_30d, stats.users.new_prev_30d);

    // --- Bookings ---
    let row: (i64, i64, i64, i64, i64, i64, i64, i64, i64, i64) = sqlx::query_as(
        "SELECT COUNT(*),
                COALESCE(SUM(status = 'confirmed'), 0),
                COALESCE(SUM(status = 'pending'), 0),
                COALESCE(SUM(status = 'cancelled'), 0),
                COALESCE(SUM(status = 'declined'), 0),
                COALESCE(SUM(datetime(created_at) >= datetime(?)), 0),
                COALESCE(SUM(datetime(created_at) >= datetime(?) AND datetime(created_at) < datetime(?)), 0),
                COALESCE(SUM(status = 'confirmed' AND datetime(start_at) >= datetime(?)), 0),
                COALESCE(SUM(status = 'confirmed' AND datetime(start_at) >= datetime(?)
                    AND datetime(start_at) < datetime(?)), 0),
                COALESCE(SUM(status = 'pending' AND pending_expires_at IS NOT NULL
                    AND datetime(pending_expires_at) <= datetime(?)), 0)
         FROM bookings",
    )
    .bind(&t_30)
    .bind(&t_60)
    .bind(&t_30)
    .bind(&t_now)
    .bind(&t_now)
    .bind(&t_7d_ahead)
    .bind(&t_24h_ahead)
    .fetch_one(pool)
    .await
    .unwrap_or_default();
    stats.bookings = BookingStats {
        total: row.0,
        confirmed: row.1,
        pending: row.2,
        cancelled: row.3,
        declined: row.4,
        created_30d: row.5,
        created_prev_30d: row.6,
        upcoming: row.7,
        upcoming_7d: row.8,
        pending_expiring_24h: row.9,
    };
    stats.bookings_delta = delta(stats.bookings.created_30d, stats.bookings.created_prev_30d);
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
         WHERE date(created_at) >= ? GROUP BY day, status",
    )
    .bind(first.format("%Y-%m-%d").to_string())
    .fetch_all(pool)
    .await
    .unwrap_or_default();
    let mut weekly: Vec<WeekBucket> = Vec::with_capacity(starts.len());
    let mut prev_month = 0;
    for start in &starts {
        let month = start.month();
        weekly.push(WeekBucket {
            start: start.format("%Y-%m-%d").to_string(),
            label: crate::i18n::format_date(*start, lang),
            axis_day: start.day(),
            axis_month: if month != prev_month {
                crate::i18n::translate(lang, &format!("common-month-{month}"), None)
            } else {
                String::new()
            },
            ..Default::default()
        });
        prev_month = month;
    }
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
    let max = weekly.iter().map(|w| w.total).max().unwrap_or(0);
    let scale = nice_ceiling(max);
    for w in &mut weekly {
        w.height_pct = pct(w.total, scale);
    }
    stats.weekly_total = weekly.iter().map(|w| w.total).sum();
    stats.weekly_scale = scale;
    stats.weekly_mid = scale / 2;
    stats.weekly = weekly;

    // --- Rankings over the last 90 days, confirmed bookings only ---
    let rows: Vec<(String, Option<String>, bool, i64)> = sqlx::query_as(
        "SELECT et.title, COALESCE(t.name, u.name), et.team_id IS NOT NULL, COUNT(*) AS n
         FROM bookings b
         JOIN event_types et ON et.id = b.event_type_id
         LEFT JOIN teams t ON t.id = et.team_id
         LEFT JOIN accounts a ON a.id = et.account_id
         LEFT JOIN users u ON u.id = a.user_id
         WHERE b.status = 'confirmed' AND datetime(b.created_at) >= datetime(?)
         GROUP BY et.id ORDER BY n DESC, et.title LIMIT ?",
    )
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
    // type's owner otherwise (personal and collective).
    let rows: Vec<(String, String, i64)> = sqlx::query_as(
        "SELECT u.name, u.email, COUNT(*) AS n
         FROM bookings b
         JOIN event_types et ON et.id = b.event_type_id
         LEFT JOIN accounts a ON a.id = et.account_id
         JOIN users u ON u.id = COALESCE(b.assigned_user_id, a.user_id)
         WHERE b.status = 'confirmed' AND datetime(b.created_at) >= datetime(?)
         GROUP BY u.id ORDER BY n DESC, u.name LIMIT ?",
    )
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
    stats.writeback_failure_count = sqlx::query_scalar(
        "SELECT COUNT(*) FROM bookings
         WHERE status = 'confirmed' AND writeback_error IS NOT NULL
           AND datetime(start_at) >= datetime(?)",
    )
    .bind(&t_now)
    .fetch_one(pool)
    .await
    .unwrap_or(0);
    if stats.writeback_failure_count > 0 {
        let rows: Vec<(String, Option<String>, String, String, i64, bool)> = sqlx::query_as(
            "SELECT et.title, COALESCE(hu.name, ou.name), b.start_at, b.writeback_error,
                    b.writeback_attempts, b.writeback_next_attempt_at IS NULL
             FROM bookings b
             JOIN event_types et ON et.id = b.event_type_id
             LEFT JOIN accounts a ON a.id = et.account_id
             LEFT JOIN users ou ON ou.id = a.user_id
             LEFT JOIN users hu ON hu.id = b.assigned_user_id
             WHERE b.status = 'confirmed' AND b.writeback_error IS NOT NULL
               AND datetime(b.start_at) >= datetime(?)
             ORDER BY datetime(b.start_at) LIMIT ?",
        )
        .bind(&t_now)
        .bind(LIST_LIMIT * 2)
        .fetch_all(pool)
        .await
        .unwrap_or_default();
        stats.writeback_failures = rows
            .into_iter()
            .map(
                |(title, host, start, error, attempts, exhausted)| WritebackFailure {
                    title,
                    host: host.unwrap_or_default(),
                    start: start
                        .trim_end_matches('Z')
                        .replacen('T', " ", 1)
                        .chars()
                        .take(16)
                        .collect(),
                    error: shorten(&error, 240),
                    attempts,
                    exhausted,
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
