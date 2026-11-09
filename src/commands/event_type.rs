use anyhow::Result;
use chrono::{Datelike, Duration, NaiveDate, NaiveDateTime, NaiveTime, TimeZone, Utc};
use clap::Subcommand;
use colored::Colorize;
use sqlx::SqlitePool;
use tabled::{Table, Tabled};
use uuid::Uuid;

use crate::booking_window::{parse_optional_date, today_in, BookingWindow};
use crate::utils::{convert_event_to_tz, parse_ical_datetime};
use std::ops::Range;

#[derive(Debug, Subcommand)]
pub enum EventTypeCommands {
    /// Create a new bookable meeting type
    Create {
        /// Title
        #[arg(long)]
        title: String,
        /// URL slug
        #[arg(long)]
        slug: String,
        /// Duration in minutes
        #[arg(long)]
        duration: i32,
        /// Description
        #[arg(long)]
        description: Option<String>,
        /// Buffer before (minutes)
        #[arg(long, default_value = "0")]
        buffer_before: i32,
        /// Buffer after (minutes)
        #[arg(long, default_value = "0")]
        buffer_after: i32,
        /// First day an appointment may take place on (YYYY-MM-DD, inclusive)
        #[arg(long, value_parser = parse_cli_date)]
        bookable_from: Option<NaiveDate>,
        /// Last day an appointment may take place on (YYYY-MM-DD, inclusive)
        #[arg(long, value_parser = parse_cli_date)]
        bookable_until: Option<NaiveDate>,
    },
    /// List event types
    List,
    /// Show available slots for an event type
    Slots {
        /// Event type slug
        slug: String,
        /// Number of days to show, from the first bookable day (clamped to the
        /// booking horizon and the bookable date range)
        #[arg(long, default_value = "7")]
        days: i32,
    },
}

fn parse_cli_date(s: &str) -> Result<NaiveDate, String> {
    NaiveDate::parse_from_str(s, "%Y-%m-%d").map_err(|_| format!("'{s}' is not a YYYY-MM-DD date"))
}

/// The days `slots` prints, as offsets from today: `days` days starting at the
/// first bookable day, cut at the last one. A horizon of N days leaves days
/// 0..=N bookable; a bookable date range moves the start forward and caps the
/// end. The range is empty when nothing is bookable.
fn slot_day_offsets(days: i32, today: NaiveDate, window: &BookingWindow) -> Range<i32> {
    let (first, last) = window.effective_date_bounds(today);
    let start = (first - today).num_days().min(i32::MAX as i64) as i32;
    let last_offset =
        ((last - today).num_days() + 1).clamp(i32::MIN as i64, i32::MAX as i64) as i32;
    let end = start.saturating_add(days.max(0)).min(last_offset);
    start..end.max(start)
}

/// The `list` column for a bookable date range, with its state when it is
/// not currently running.
fn format_bookable(window: &BookingWindow, today: NaiveDate) -> String {
    let range = match (window.bookable_from, window.bookable_until) {
        (Some(from), Some(until)) => format!("{from} – {until}"),
        (Some(from), None) => format!("from {from}"),
        (None, Some(until)) => format!("until {until}"),
        (None, None) => return String::new(),
    };
    if window.is_expired(today) {
        format!("{range} (expired)")
    } else if window.bookable_from.is_some_and(|from| today < from) {
        format!("{range} (upcoming)")
    } else {
        range
    }
}

#[derive(Tabled)]
struct EventTypeRow {
    #[tabled(rename = "Slug")]
    slug: String,
    #[tabled(rename = "Title")]
    title: String,
    #[tabled(rename = "Duration")]
    duration: String,
    #[tabled(rename = "Active")]
    active: String,
    #[tabled(rename = "Bookable")]
    bookable: String,
}

/// Bookings that make the host of `et_id` busy around a window, as raw
/// (start_at, end_at) strings for `booking_time::busy_range`. The host is the
/// event type owner, and the rule is the web slot engine's
/// (`booking_time::user_busy_bookings`): their own bookings, bookings assigned
/// to them on other people's team event types, unassigned collective team
/// bookings, bookings they claimed as a watcher, and pending requests whose
/// event type holds the slot. Bookings
/// of other users on the same instance do not count. The window bounds are
/// ISO local times and are widened by two days on each side, like the web
/// path, because stored times may be UTC.
pub(crate) async fn host_busy_bookings(
    pool: &SqlitePool,
    et_id: &str,
    window_start_iso: &str,
    window_end_iso: &str,
) -> Result<Vec<(String, String)>> {
    let Some(host) = event_type_host(pool, et_id).await? else {
        return Ok(Vec::new());
    };
    let rows = sqlx::query_as(&format!(
        "SELECT CASE b.time_version WHEN 1 THEN b.start_at ELSE rtrim(b.start_at, 'Z') END AS start_at, CASE b.time_version WHEN 1 THEN b.end_at ELSE rtrim(b.end_at, 'Z') END AS end_at FROM bookings b
         JOIN event_types et ON et.id = b.event_type_id
         JOIN accounts a ON a.id = et.account_id
         WHERE {}
           AND b.start_at <= strftime('%Y-%m-%dT%H:%M:%S', ?, '+2 days') AND b.end_at >= strftime('%Y-%m-%dT%H:%M:%S', ?, '-2 days')",
        crate::booking_time::user_busy_bookings(true)
    ))
    .bind(&host)
    .bind(&host)
    .bind(&host)
    .bind(&host)
    .bind(&host)
    .bind(window_end_iso)
    .bind(window_start_iso)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// The user who hosts `et_id` on the CLI paths: the owner of its account.
pub(crate) async fn event_type_host(pool: &SqlitePool, et_id: &str) -> Result<Option<String>> {
    Ok(sqlx::query_scalar(
        "SELECT a.user_id FROM event_types et JOIN accounts a ON a.id = et.account_id WHERE et.id = ?",
    )
    .bind(et_id)
    .fetch_optional(pool)
    .await?
    .flatten())
}

/// The users whose time off (#46) gates `et_id` on the CLI paths, the way
/// the web slot engine resolves them: every eligible member of a
/// collective team event type (enabled, not excluded by a zero weight),
/// otherwise the owner. The CLI checks no member's calendar, so time off is
/// the only per-member check it makes.
async fn time_off_hosts(pool: &SqlitePool, et_id: &str) -> Result<Option<Vec<String>>> {
    let row: Option<(Option<String>, String, Option<String>)> = sqlx::query_as(
        "SELECT et.team_id, et.scheduling_mode, a.user_id FROM event_types et \
         JOIN accounts a ON a.id = et.account_id WHERE et.id = ?",
    )
    .bind(et_id)
    .fetch_optional(pool)
    .await?;
    let Some((team_id, mode, owner)) = row else {
        return Ok(Some(Vec::new()));
    };
    match team_id {
        Some(team_id) if mode == "collective" => {
            let members: Vec<String> = sqlx::query_scalar(
                "SELECT u.id FROM users u JOIN team_members tm ON tm.user_id = u.id \
                 LEFT JOIN event_type_member_weights etw ON etw.user_id = u.id AND etw.event_type_id = ? \
                 WHERE tm.team_id = ? AND u.enabled = 1 AND COALESCE(etw.weight, 1) > 0",
            )
            .bind(et_id)
            .bind(&team_id)
            .fetch_all(pool)
            .await?;
            // An empty collective roster offers nothing, as on the web.
            Ok((!members.is_empty()).then_some(members))
        }
        _ => Ok(Some(owner.into_iter().collect())),
    }
}

/// Time off (#46) around a window, as busy intervals in `host_tz`, for
/// every user `et_id` needs (see [`time_off_hosts`]), the way the web slot
/// engine adds it. A collective event type with nobody eligible is busy
/// over the whole window. A failed read is an error, not a free host.
pub(crate) async fn host_time_off(
    pool: &SqlitePool,
    et_id: &str,
    window_start: NaiveDateTime,
    window_end: NaiveDateTime,
    host_tz: chrono_tz::Tz,
) -> Result<Vec<(NaiveDateTime, NaiveDateTime)>> {
    let Some(users) = time_off_hosts(pool, et_id).await? else {
        return Ok(vec![(window_start, window_end)]);
    };
    let mut busy = Vec::new();
    for user in users {
        // The CLI reports a failed read instead of failing closed silently.
        let off =
            crate::time_off::try_busy_intervals(pool, &user, window_start, window_end, host_tz)
                .await
                .map_err(|e| anyhow::anyhow!("could not read the time off of {user}: {e}"))?;
        busy.extend(off);
    }
    Ok(busy)
}

/// The parts of a window no Microsoft calendar of `et_id`'s hosts was read
/// for (see [`crate::commands::sync::microsoft_gap_intervals`]), the way the
/// web slot engine treats them: on a collective team event type any eligible
/// member's gap blocks; on a round-robin one a time is blocked only where
/// every eligible member has one (anyone else could take it); otherwise the
/// owner's.
pub(crate) async fn host_microsoft_gaps(
    pool: &SqlitePool,
    et_id: &str,
    window_start: NaiveDateTime,
    window_end: NaiveDateTime,
    host_tz: chrono_tz::Tz,
) -> Result<
    Vec<(
        NaiveDateTime,
        NaiveDateTime,
        crate::commands::sync::CoverageGap,
    )>,
> {
    let row: Option<(Option<String>, String, Option<String>)> = sqlx::query_as(
        "SELECT et.team_id, et.scheduling_mode, a.user_id FROM event_types et \
         JOIN accounts a ON a.id = et.account_id WHERE et.id = ?",
    )
    .bind(et_id)
    .fetch_optional(pool)
    .await?;
    let Some((team_id, mode, owner)) = row else {
        return Ok(Vec::new());
    };
    let gaps_of = |user: String| async move {
        crate::commands::sync::microsoft_gap_intervals(
            pool,
            &user,
            window_start,
            window_end,
            host_tz,
        )
        .await
    };
    let Some(team_id) = team_id else {
        return Ok(match owner {
            Some(owner) => gaps_of(owner).await,
            None => Vec::new(),
        });
    };
    let members: Vec<String> = sqlx::query_scalar(
        "SELECT u.id FROM users u JOIN team_members tm ON tm.user_id = u.id \
         LEFT JOIN event_type_member_weights etw ON etw.user_id = u.id AND etw.event_type_id = ? \
         WHERE tm.team_id = ? AND u.enabled = 1 AND COALESCE(etw.weight, 1) > 0",
    )
    .bind(et_id)
    .bind(&team_id)
    .fetch_all(pool)
    .await?;
    let mut per_member = Vec::with_capacity(members.len());
    for member in members {
        per_member.push(gaps_of(member).await);
    }
    if mode == "collective" {
        return Ok(per_member.into_iter().flatten().collect());
    }
    // Round robin: each member's gap runs from some start to the window's
    // end, so the times nobody can vouch for start at the latest of them.
    if per_member.is_empty() || per_member.iter().any(Vec::is_empty) {
        return Ok(Vec::new());
    }
    Ok(per_member
        .into_iter()
        .flatten()
        .max_by_key(|(start, _, _)| *start)
        .into_iter()
        .collect())
}

/// Fetch non-recurring, busy (non-cancelled, OPAQUE) events for `et_id` whose
/// stored time range overlaps `[window_start, window_end]`. Only the event
/// type owner's calendars count (and, when the event type selects calendars,
/// only those), like `fetch_busy_times_for_user_ex` on the web side.
///
/// Events are stored in compact iCal form ("YYYYMMDDTHHMMSS" for timed,
/// "YYYYMMDD" for all-day), so both overlap bounds are formatted compact *with
/// a time component*. A date-only upper bound ("YYYYMMDD") is a bug: a timed
/// event sorts lexically after its own date prefix ("20260618T100000" >
/// "20260618"), so a window ending on the event's own date would silently miss
/// it. Returns raw (start_at, end_at, timezone) rows for tz conversion by the
/// caller. UTC Google status events use a two-day SQL margin so host-local
/// slots aren't missed before the caller converts the returned busy ranges.
async fn fetch_nonrecurring_busy_events(
    pool: &SqlitePool,
    et_id: &str,
    window_start: NaiveDateTime,
    window_end: NaiveDateTime,
) -> Result<Vec<(String, String, Option<String>)>> {
    let end_compact = window_end.format("%Y%m%dT%H%M%S").to_string();
    let start_compact = window_start.format("%Y%m%dT%H%M%S").to_string();
    let rows = sqlx::query_as(
        "SELECT e.start_at, e.end_at, e.timezone FROM events e
         JOIN calendars c ON c.id = e.calendar_id
         JOIN caldav_sources cs ON cs.id = c.source_id
         JOIN accounts a ON a.id = cs.account_id
         WHERE a.user_id = (SELECT ha.user_id FROM event_types het
                            JOIN accounts ha ON ha.id = het.account_id WHERE het.id = ?)
           AND c.sync_enabled = 1
           AND ((c.is_busy = 1 AND NOT EXISTS (SELECT 1 FROM event_type_calendars etc JOIN calendars sc ON sc.id = etc.calendar_id WHERE etc.event_type_id = ? AND sc.sync_enabled = 1))
                OR c.id IN (SELECT calendar_id FROM event_type_calendars WHERE event_type_id = ?))
           AND (e.rrule IS NULL OR e.rrule = '') AND e.rdate = 0
           AND (e.status IS NULL OR e.status != 'CANCELLED')
           AND (e.transp IS NULL OR e.transp != 'TRANSPARENT')
           AND ((e.start_at <= ? AND e.end_at >= ?)
                OR (e.google_out_of_office = 1
                    AND e.start_at <= strftime('%Y%m%dT%H%M%S', ?, '+2 days')
                    AND e.end_at >= strftime('%Y%m%dT%H%M%S', ?, '-2 days')))",
    )
    .bind(et_id)
    .bind(et_id)
    .bind(et_id)
    .bind(&end_compact)
    .bind(&start_compact)
    .bind(window_end.format("%Y-%m-%dT%H:%M:%S").to_string())
    .bind(window_start.format("%Y-%m-%dT%H:%M:%S").to_string())
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub async fn run(pool: &SqlitePool, cmd: EventTypeCommands) -> Result<()> {
    match cmd {
        EventTypeCommands::Create {
            title,
            slug,
            duration,
            description,
            buffer_before,
            buffer_after,
            bookable_from,
            bookable_until,
        } => {
            if let (Some(from), Some(until)) = (bookable_from, bookable_until) {
                if from > until {
                    anyhow::bail!("--bookable-until must be on or after --bookable-from");
                }
            }
            // /u/{username}/next-free is the landing page's hint endpoint.
            if slug == "next-free" {
                anyhow::bail!("The slug 'next-free' is reserved, pick another one");
            }
            let account: (String,) = sqlx::query_as("SELECT id FROM accounts LIMIT 1")
                .fetch_optional(pool)
                .await?
                .ok_or_else(|| anyhow::anyhow!("No account found. Run `calrs init` first."))?;

            let id = Uuid::new_v4().to_string();

            sqlx::query(
                "INSERT INTO event_types (id, account_id, title, slug, description, duration_min, buffer_before, buffer_after, bookable_from, bookable_until)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&id)
            .bind(&account.0)
            .bind(&title)
            .bind(&slug)
            .bind(&description)
            .bind(duration)
            .bind(buffer_before)
            .bind(buffer_after)
            .bind(bookable_from)
            .bind(bookable_until)
            .execute(pool)
            .await?;

            // Add default availability: Mon-Fri 09:00-17:00
            for day in [1, 2, 3, 4, 5] {
                let rule_id = Uuid::new_v4().to_string();
                sqlx::query(
                    "INSERT INTO availability_rules (id, event_type_id, day_of_week, start_time, end_time)
                     VALUES (?, ?, ?, '09:00', '17:00')",
                )
                .bind(&rule_id)
                .bind(&id)
                .bind(day)
                .execute(pool)
                .await?;
            }

            println!(
                "{} Event type '{}' created (slug: {}, {}min)",
                "✓".green(),
                title,
                slug,
                duration
            );
            println!(
                "{}",
                "Default availability: Mon–Fri 09:00–17:00. View slots with `calrs event-type slots`."
                    .dimmed()
            );
        }
        EventTypeCommands::List => {
            let types: Vec<(
                String,
                String,
                String,
                i32,
                bool,
                Option<String>,
                Option<String>,
            )> = sqlx::query_as(
                "SELECT id, slug, title, duration_min, enabled, bookable_from, bookable_until
                     FROM event_types ORDER BY created_at",
            )
            .fetch_all(pool)
            .await?;

            if types.is_empty() {
                println!("No event types. Create one with `calrs event-type create`.");
                return Ok(());
            }

            let mut rows: Vec<EventTypeRow> = Vec::with_capacity(types.len());
            for (id, slug, title, duration, enabled, from, until) in types {
                let window = BookingWindow {
                    bookable_from: parse_optional_date(from.as_deref()),
                    bookable_until: parse_optional_date(until.as_deref()),
                    ..BookingWindow::default()
                };
                let bookable = if window.has_date_range() {
                    let today = today_in(crate::booking_time::event_timezone(pool, &id).await?);
                    format_bookable(&window, today)
                } else {
                    String::new()
                };
                rows.push(EventTypeRow {
                    slug,
                    title,
                    duration: format!("{}min", duration),
                    active: if enabled {
                        "✓".to_string()
                    } else {
                        "✗".to_string()
                    },
                    bookable,
                });
            }

            println!("{}", Table::new(rows));
        }
        EventTypeCommands::Slots { slug, days } => {
            #[allow(clippy::type_complexity)]
            let et: Option<(String, i32, i32, i32, i32, Option<i32>, Option<i32>, Option<String>, Option<String>)> = sqlx::query_as(
                "SELECT id, duration_min, buffer_before, buffer_after, min_notice_min, slot_interval_min, booking_horizon_days, bookable_from, bookable_until
                 FROM event_types WHERE slug = ? AND enabled = 1",
            )
            .bind(&slug)
            .fetch_optional(pool)
            .await?;

            let (
                et_id,
                duration,
                buffer_before,
                buffer_after,
                min_notice,
                slot_interval,
                booking_horizon,
                bookable_from,
                bookable_until,
            ) = match et {
                Some(e) => e,
                None => {
                    println!("{} No active event type with slug '{}'", "✗".red(), slug);
                    return Ok(());
                }
            };
            let interval = slot_interval.filter(|v| *v > 0).unwrap_or(duration);

            let window = BookingWindow {
                horizon_days: booking_horizon,
                bookable_from: parse_optional_date(bookable_from.as_deref()),
                bookable_until: parse_optional_date(bookable_until.as_deref()),
            };

            let rules: Vec<(i32, String, String)> = sqlx::query_as(
                "SELECT day_of_week, start_time, end_time FROM availability_rules WHERE event_type_id = ?",
            )
            .bind(&et_id)
            .fetch_all(pool)
            .await?;

            let overrides: Vec<(String, Option<String>, Option<String>, i32)> = sqlx::query_as(
                "SELECT date, start_time, end_time, is_blocked FROM availability_overrides WHERE event_type_id = ? ORDER BY date, start_time",
            )
            .bind(&et_id)
            .fetch_all(pool)
            .await
            .unwrap_or_default();

            // Get busy events for the period
            let host_tz = crate::booking_time::event_timezone(pool, &et_id).await?;
            let now = chrono::Utc::now().with_timezone(&host_tz).naive_local();
            // Show the same window the booking page offers: start at the
            // first bookable day, stop at the horizon or the end of the range.
            let offsets = slot_day_offsets(days, now.date(), &window);
            if offsets.is_empty() {
                // Nothing is bookable (an expired range, say): skip the busy
                // and resource lookups, which would otherwise run up to a
                // distant `bookable_from`.
                println!("Available slots for {} ({}min):\n", slug.bold(), duration);
                return Ok(());
            }
            let days = offsets.end;
            let min_start = Utc::now() + Duration::minutes(min_notice as i64);
            let end_date = now.date() + Duration::days(days as i64);
            let end_iso = end_date.format("%Y-%m-%dT23:59:59").to_string();
            let now_iso = now.format("%Y-%m-%dT%H:%M:%S").to_string();
            let window_end_dt = end_date.and_hms_opt(23, 59, 59).unwrap_or(now);

            // Non-recurring events (with timezone for conversion)
            let events = fetch_nonrecurring_busy_events(pool, &et_id, now, window_end_dt).await?;

            let mut busy_events: Vec<(String, String)> = events
                .iter()
                .filter_map(|(s, e, tz)| {
                    let start =
                        convert_event_to_tz(parse_ical_datetime(s)?, tz.as_deref(), host_tz);
                    let end = convert_event_to_tz(parse_ical_datetime(e)?, tz.as_deref(), host_tz);
                    Some((
                        start.format("%Y-%m-%dT%H:%M:%S").to_string(),
                        end.format("%Y-%m-%dT%H:%M:%S").to_string(),
                    ))
                })
                .collect();

            // The host's bookings, converted by `busy_range` at the
            // comparison below (UTC rows) or kept as wall clock (legacy rows).
            let booking_busy = host_busy_bookings(pool, &et_id, &now_iso, &end_iso)
                .await
                .unwrap_or_default();

            busy_events.extend(booking_busy);

            // Expand recurring events
            let end_compact_rrule = end_date.format("%Y%m%dT235959").to_string();
            let recurring: Vec<(String, String, String, Option<String>, Option<String>)> = sqlx::query_as(
                "SELECT e.start_at, e.end_at, COALESCE(e.rrule, ''), e.raw_ical, e.timezone FROM events e
                 JOIN calendars c ON c.id = e.calendar_id
                 JOIN caldav_sources cs ON cs.id = c.source_id
                 JOIN accounts a ON a.id = cs.account_id
                 WHERE a.user_id = (SELECT ha.user_id FROM event_types het
                                    JOIN accounts ha ON ha.id = het.account_id WHERE het.id = ?)
                   AND c.sync_enabled = 1
                   AND ((c.is_busy = 1 AND NOT EXISTS (SELECT 1 FROM event_type_calendars etc JOIN calendars sc ON sc.id = etc.calendar_id WHERE etc.event_type_id = ? AND sc.sync_enabled = 1))
                        OR c.id IN (SELECT calendar_id FROM event_type_calendars WHERE event_type_id = ?))
                   AND (e.status IS NULL OR e.status != 'CANCELLED')
                   AND (e.transp IS NULL OR e.transp != 'TRANSPARENT')
                   AND ((e.rrule IS NOT NULL AND e.rrule != '') OR e.rdate = 1) AND (e.start_at <= ? OR e.start_at <= ?)",
            )
            .bind(&et_id).bind(&et_id).bind(&et_id)
            .bind(&end_iso)
            .bind(&end_compact_rrule)
            .fetch_all(pool)
            .await
            .unwrap_or_default();

            for (s, e, rrule_str, raw_ical, event_tz) in &recurring {
                for (cs, ce) in crate::rrule::occurrences_in(
                    s,
                    e,
                    rrule_str,
                    raw_ical.as_deref(),
                    event_tz.as_deref(),
                    now,
                    window_end_dt,
                    host_tz,
                ) {
                    busy_events.push((
                        cs.format("%Y-%m-%dT%H:%M:%S").to_string(),
                        ce.format("%Y-%m-%dT%H:%M:%S").to_string(),
                    ));
                }
            }

            // Times no Microsoft calendar of the hosts was read for, as on
            // the web slot pages: `booking create` refuses them too.
            for (s, e, _) in host_microsoft_gaps(pool, &et_id, now, window_end_dt, host_tz).await? {
                busy_events.push((
                    s.format("%Y-%m-%dT%H:%M:%S").to_string(),
                    e.format("%Y-%m-%dT%H:%M:%S").to_string(),
                ));
            }

            // The host's time off, as on the web slot pages (#46).
            for (s, e) in host_time_off(pool, &et_id, now, window_end_dt, host_tz).await? {
                busy_events.push((
                    s.format("%Y-%m-%dT%H:%M:%S").to_string(),
                    e.format("%Y-%m-%dT%H:%M:%S").to_string(),
                ));
            }

            // Required shared resources: same blocking intervals as the web
            // slot pages (syncs stale feeds, merges per the scheduling mode).
            let resource_busy = crate::resources::blocking_intervals_for_event_type(
                pool,
                &et_id,
                now,
                window_end_dt,
                host_tz,
                None,
            )
            .await;
            for (s, e) in resource_busy {
                busy_events.push((
                    s.format("%Y-%m-%dT%H:%M:%S").to_string(),
                    e.format("%Y-%m-%dT%H:%M:%S").to_string(),
                ));
            }

            println!("Available slots for {} ({}min):\n", slug.bold(), duration);

            let slot_duration = Duration::minutes(duration as i64);
            let slot_step = Duration::minutes(interval.max(1) as i64);

            for day_offset in offsets {
                let date = now.date() + Duration::days(day_offset as i64);
                let date_str = date.format("%Y-%m-%d").to_string();

                // Check availability overrides for this date
                let day_overrides: Vec<&(String, Option<String>, Option<String>, i32)> = overrides
                    .iter()
                    .filter(|(d, _, _, _)| *d == date_str)
                    .collect();

                if day_overrides.iter().any(|(_, _, _, blocked)| *blocked != 0) {
                    continue;
                }

                let windows: Vec<(String, String)> = if !day_overrides.is_empty() {
                    day_overrides
                        .iter()
                        .filter_map(|(_, s, e, _)| match (s, e) {
                            (Some(start), Some(end)) => Some((start.clone(), end.clone())),
                            _ => None,
                        })
                        .collect()
                } else {
                    let weekday = date.weekday().num_days_from_sunday() as i32;
                    rules
                        .iter()
                        .filter(|(d, _, _)| *d == weekday)
                        .map(|(_, s, e)| (s.clone(), e.clone()))
                        .collect()
                };

                if windows.is_empty() {
                    continue;
                }

                let mut slots = Vec::new();

                for (start_str, end_str) in &windows {
                    let window_start = NaiveTime::parse_from_str(start_str, "%H:%M")?;
                    let window_end = NaiveTime::parse_from_str(end_str, "%H:%M")?;

                    let mut cursor = date.and_time(window_start);
                    while cursor < date.and_time(window_end) {
                        let Some(start) = host_tz.from_local_datetime(&cursor).single() else {
                            cursor += slot_step;
                            continue;
                        };
                        let end =
                            (start.with_timezone(&Utc) + slot_duration).with_timezone(&host_tz);
                        let (slot_start, slot_end) = crate::booking_time::busy_range(
                            &start.to_rfc3339(),
                            &end.to_rfc3339(),
                            host_tz,
                        )
                        .unwrap();

                        // Check minimum notice
                        if start.with_timezone(&Utc) < min_start
                            || slot_start < date.and_time(window_start)
                            || slot_end > date.and_time(window_end)
                        {
                            cursor += slot_step;
                            continue;
                        }

                        // Check conflicts with busy events
                        let buf_start = slot_start - Duration::minutes(buffer_before as i64);
                        let buf_end = slot_end + Duration::minutes(buffer_after as i64);

                        let has_conflict =
                            busy_events.iter().any(
                                |(bs, be)| match crate::booking_time::busy_range(bs, be, host_tz) {
                                    Some((s, e)) => s < buf_end && e > buf_start,
                                    _ => false,
                                },
                            );

                        if !has_conflict {
                            slots.push(format!(
                                "  {} – {}",
                                cursor.format("%H:%M"),
                                end.format("%H:%M")
                            ));
                        }

                        cursor += slot_step;
                    }
                }

                if !slots.is_empty() {
                    slots.sort();
                    let day_name = date.format("%a %Y-%m-%d").to_string();
                    println!("{}:", day_name.bold());
                    for slot in &slots {
                        println!("{}", slot.green());
                    }
                    println!();
                }
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
    use std::str::FromStr;

    async fn setup_db() -> SqlitePool {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(
                SqliteConnectOptions::from_str("sqlite::memory:")
                    .unwrap()
                    .foreign_keys(true),
            )
            .await
            .unwrap();
        crate::db::migrate(&pool).await.unwrap();
        pool
    }

    /// Seed a user + account so event_type create can find an account
    async fn seed_account(pool: &SqlitePool) -> (String, String) {
        let user_id = Uuid::new_v4().to_string();
        let username = crate::auth::generate_username(pool, "host@test.com")
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO users (id, email, name, timezone, role, auth_provider, username, enabled)
             VALUES (?, 'host@test.com', 'Host', 'UTC', 'admin', 'local', ?, 1)",
        )
        .bind(&user_id)
        .bind(&username)
        .execute(pool)
        .await
        .unwrap();

        let account_id = Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO accounts (id, name, email, timezone, user_id) VALUES (?, 'Host', 'host@test.com', 'UTC', ?)",
        )
        .bind(&account_id)
        .bind(&user_id)
        .execute(pool)
        .await
        .unwrap();

        (user_id, account_id)
    }

    #[tokio::test]
    async fn test_create_event_type() {
        let pool = setup_db().await;
        seed_account(&pool).await;

        let result = run(
            &pool,
            EventTypeCommands::Create {
                title: "30min Call".to_string(),
                slug: "intro".to_string(),
                duration: 30,
                description: Some("A quick intro call".to_string()),
                buffer_before: 5,
                buffer_after: 5,
                bookable_from: None,
                bookable_until: None,
            },
        )
        .await;
        assert!(result.is_ok());

        // Verify event type was created
        let et: (String, String, i32, i32, i32) = sqlx::query_as(
            "SELECT title, slug, duration_min, buffer_before, buffer_after FROM event_types WHERE slug = 'intro'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(et.0, "30min Call");
        assert_eq!(et.1, "intro");
        assert_eq!(et.2, 30);
        assert_eq!(et.3, 5);
        assert_eq!(et.4, 5);
    }

    #[tokio::test]
    async fn test_create_event_type_default_availability() {
        let pool = setup_db().await;
        seed_account(&pool).await;

        run(
            &pool,
            EventTypeCommands::Create {
                title: "Meeting".to_string(),
                slug: "meeting".to_string(),
                duration: 60,
                description: None,
                buffer_before: 0,
                buffer_after: 0,
                bookable_from: None,
                bookable_until: None,
            },
        )
        .await
        .unwrap();

        // Should have 5 availability rules (Mon-Fri)
        let rules: Vec<(i32, String, String)> = sqlx::query_as(
            "SELECT day_of_week, start_time, end_time FROM availability_rules
             WHERE event_type_id = (SELECT id FROM event_types WHERE slug = 'meeting')
             ORDER BY day_of_week",
        )
        .fetch_all(&pool)
        .await
        .unwrap();

        assert_eq!(rules.len(), 5, "Should have Mon-Fri availability rules");
        // Days 1-5 = Mon-Fri
        for (i, (day, start, end)) in rules.iter().enumerate() {
            assert_eq!(*day, (i + 1) as i32);
            assert_eq!(start, "09:00");
            assert_eq!(end, "17:00");
        }
    }

    #[tokio::test]
    async fn test_create_event_type_no_account() {
        let pool = setup_db().await;
        // No account seeded — should fail
        let result = run(
            &pool,
            EventTypeCommands::Create {
                title: "Test".to_string(),
                slug: "test".to_string(),
                duration: 30,
                description: None,
                buffer_before: 0,
                buffer_after: 0,
                bookable_from: None,
                bookable_until: None,
            },
        )
        .await;
        assert!(result.is_err(), "Should fail without an account");
    }

    #[tokio::test]
    async fn test_list_event_types_empty() {
        let pool = setup_db().await;
        let result = run(&pool, EventTypeCommands::List).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_list_event_types() {
        let pool = setup_db().await;
        seed_account(&pool).await;

        // Create two event types
        run(
            &pool,
            EventTypeCommands::Create {
                title: "Quick Chat".to_string(),
                slug: "quick".to_string(),
                duration: 15,
                description: None,
                buffer_before: 0,
                buffer_after: 0,
                bookable_from: None,
                bookable_until: None,
            },
        )
        .await
        .unwrap();

        run(
            &pool,
            EventTypeCommands::Create {
                title: "Deep Dive".to_string(),
                slug: "deep".to_string(),
                duration: 60,
                description: None,
                buffer_before: 0,
                buffer_after: 0,
                bookable_from: None,
                bookable_until: None,
            },
        )
        .await
        .unwrap();

        let result = run(&pool, EventTypeCommands::List).await;
        assert!(result.is_ok());

        let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM event_types")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count.0, 2);
    }

    #[tokio::test]
    async fn test_create_duplicate_slug_fails() {
        let pool = setup_db().await;
        seed_account(&pool).await;

        run(
            &pool,
            EventTypeCommands::Create {
                title: "First".to_string(),
                slug: "same-slug".to_string(),
                duration: 30,
                description: None,
                buffer_before: 0,
                buffer_after: 0,
                bookable_from: None,
                bookable_until: None,
            },
        )
        .await
        .unwrap();

        // Second with same slug should fail (UNIQUE constraint)
        let result = run(
            &pool,
            EventTypeCommands::Create {
                title: "Second".to_string(),
                slug: "same-slug".to_string(),
                duration: 30,
                description: None,
                buffer_before: 0,
                buffer_after: 0,
                bookable_from: None,
                bookable_until: None,
            },
        )
        .await;
        assert!(result.is_err(), "Duplicate slug should fail");
    }

    #[tokio::test]
    async fn slots_busy_query_detects_same_day_compact_event() {
        // Regression: the CLI slots busy query must detect a timed event stored
        // in compact iCal form ("YYYYMMDDTHHMMSS") on the SAME day as the
        // window's end. A date-only upper bound ("YYYYMMDD") sorts before
        // "...T100000" and silently dropped the event, so the slot looked free.
        let pool = setup_db().await;
        let (_user_id, account_id) = seed_account(&pool).await;

        let et_id = Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO event_types (id, account_id, slug, title, duration_min) VALUES (?, ?, 'demo', 'Demo', 60)")
            .bind(&et_id).bind(&account_id).execute(&pool).await.unwrap();
        let source_id = Uuid::new_v4().to_string();
        let cal_id = Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO caldav_sources (id, account_id, name, url, username) VALUES (?, ?, 'S', 'https://x/dav', 'u')")
            .bind(&source_id).bind(&account_id).execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO calendars (id, source_id, href, display_name, is_busy) VALUES (?, ?, '/c/', 'C', 1)")
            .bind(&cal_id).bind(&source_id).execute(&pool).await.unwrap();
        let ev_id = Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO events (id, calendar_id, uid, summary, start_at, end_at, all_day, timezone, status, transp) VALUES (?, ?, 'u1', 'Busy', '20260618T100000', '20260618T113000', 0, 'Europe/Paris', 'CONFIRMED', 'OPAQUE')")
            .bind(&ev_id).bind(&cal_id).execute(&pool).await.unwrap();

        // Same-day window: end bound formatted from the event's own date.
        let day = chrono::NaiveDate::from_ymd_opt(2026, 6, 18).unwrap();
        let busy = fetch_nonrecurring_busy_events(
            &pool,
            &et_id,
            day.and_hms_opt(0, 0, 0).unwrap(),
            day.and_hms_opt(23, 59, 59).unwrap(),
        )
        .await
        .unwrap();

        assert_eq!(busy.len(), 1, "same-day compact event must be detected");
        assert_eq!(busy[0].0, "20260618T100000");

        // Guard: TRANSPARENT (free) events are still excluded.
        sqlx::query("UPDATE events SET transp = 'TRANSPARENT' WHERE id = ?")
            .bind(&ev_id)
            .execute(&pool)
            .await
            .unwrap();
        let busy = fetch_nonrecurring_busy_events(
            &pool,
            &et_id,
            day.and_hms_opt(0, 0, 0).unwrap(),
            day.and_hms_opt(23, 59, 59).unwrap(),
        )
        .await
        .unwrap();
        assert!(busy.is_empty(), "TRANSPARENT events must not block");
    }

    /// Another user's busy calendar must not block this host's slots: the
    /// query used to read every busy calendar on the instance.
    #[tokio::test]
    async fn slots_busy_query_ignores_other_users_calendars() {
        let pool = setup_db().await;
        let (_user_id, account_id) = seed_account(&pool).await;
        sqlx::raw_sql(
            "INSERT INTO users (id, email, name, role, auth_provider, username, enabled)
                 VALUES ('other', 'other@test.com', 'Other', 'user', 'local', 'other', 1);
             INSERT INTO accounts (id, name, email, timezone, user_id)
                 VALUES ('acc-other', 'Other', 'other@test.com', 'UTC', 'other');
             INSERT INTO caldav_sources (id, account_id, name, url, username)
                 VALUES ('src-other', 'acc-other', 'S', 'https://x/dav', 'u');
             INSERT INTO calendars (id, source_id, href, display_name, is_busy)
                 VALUES ('cal-other', 'src-other', '/c/', 'C', 1);
             INSERT INTO events (id, calendar_id, uid, summary, start_at, end_at, all_day, timezone, status, transp)
                 VALUES ('ev-other', 'cal-other', 'u-other', 'Busy', '20300318T100000', '20300318T110000', 0, 'UTC', 'CONFIRMED', 'OPAQUE');",
        )
        .execute(&pool)
        .await
        .unwrap();
        let et_id = Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO event_types (id, account_id, slug, title, duration_min) VALUES (?, ?, 'demo', 'Demo', 60)")
            .bind(&et_id)
            .bind(&account_id)
            .execute(&pool)
            .await
            .unwrap();

        let day = chrono::NaiveDate::from_ymd_opt(2030, 3, 18).unwrap();
        let busy = fetch_nonrecurring_busy_events(
            &pool,
            &et_id,
            day.and_hms_opt(0, 0, 0).unwrap(),
            day.and_hms_opt(23, 59, 59).unwrap(),
        )
        .await
        .unwrap();
        assert!(busy.is_empty(), "{busy:?}");
    }

    // --- Rolling booking horizon (migration 063) ---

    /// How many days `slots` prints for a horizon-only window.
    fn clamp_days_to_horizon(days: i32, horizon: Option<i32>) -> i32 {
        let window = BookingWindow {
            horizon_days: horizon,
            ..BookingWindow::default()
        };
        slot_day_offsets(days, today(), &window).len() as i32
    }

    fn today() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 11, 2).unwrap()
    }

    fn date(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    #[test]
    fn clamp_days_to_horizon_leaves_request_alone_when_unlimited() {
        assert_eq!(clamp_days_to_horizon(7, None), 7);
        assert_eq!(clamp_days_to_horizon(365, None), 365);
    }

    #[test]
    fn clamp_days_to_horizon_caps_at_horizon_plus_one() {
        // A horizon of N leaves days 0..=N bookable, so the window is N + 1.
        assert_eq!(clamp_days_to_horizon(7, Some(2)), 3);
        assert_eq!(clamp_days_to_horizon(7, Some(0)), 1);
        assert_eq!(clamp_days_to_horizon(30, Some(13)), 14);
    }

    #[test]
    fn clamp_days_to_horizon_keeps_smaller_requests() {
        // A request narrower than the horizon is already inside it.
        assert_eq!(clamp_days_to_horizon(3, Some(30)), 3);
        assert_eq!(clamp_days_to_horizon(1, Some(0)), 1);
    }

    #[tokio::test]
    async fn slots_command_reads_horizon_from_the_event_type() {
        let pool = setup_db().await;
        let (_, account_id) = seed_account(&pool).await;
        let et_id = Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO event_types (id, account_id, slug, title, duration_min, enabled, booking_horizon_days) \
             VALUES (?, ?, 'capped', 'Capped', 30, 1, 5)",
        )
        .bind(&et_id)
        .bind(&account_id)
        .execute(&pool)
        .await
        .unwrap();

        let horizon: Option<i32> =
            sqlx::query_scalar("SELECT booking_horizon_days FROM event_types WHERE id = ?")
                .bind(&et_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(horizon, Some(5));
        assert_eq!(clamp_days_to_horizon(30, horizon), 6);
    }

    /// The CLI used to count every booking on the instance as busy for every
    /// host. It now follows the web rule: only what makes this host busy.
    /// A collective team event type needs every eligible member, so any
    /// member's time off blocks it on the CLI too (#46).
    #[tokio::test]
    async fn host_time_off_covers_the_collective_roster() {
        let pool = setup_db().await;
        sqlx::raw_sql(
            "INSERT INTO users (id, email, name, role, auth_provider, username, enabled) VALUES
                 ('owner', 'owner@x', 'Owner', 'admin', 'local', 'owner', 1),
                 ('m1', 'm1@x', 'M1', 'user', 'local', 'm1', 1),
                 ('m2', 'm2@x', 'M2', 'user', 'local', 'm2', 1);
             INSERT INTO accounts (id, name, email, timezone, user_id) VALUES
                 ('acc-owner', 'Owner', 'owner@x', 'UTC', 'owner');
             INSERT INTO teams (id, name, slug, visibility) VALUES ('t', 'T', 't', 'public');
             INSERT INTO team_members (team_id, user_id, role, source) VALUES
                 ('t', 'm1', 'member', 'direct'), ('t', 'm2', 'member', 'direct');
             INSERT INTO event_types (id, account_id, team_id, slug, title, duration_min, scheduling_mode)
                 VALUES ('et-team', 'acc-owner', 't', 'demo', 'Demo', 30, 'collective');
             INSERT INTO user_time_off (id, user_id, start_at, end_at, timezone) VALUES
                 ('off-m1', 'm1', '2030-03-18T09:00:00Z', '2030-03-18T10:00:00Z', 'UTC');",
        )
        .execute(&pool)
        .await
        .unwrap();
        let at = |h| {
            NaiveDate::from_ymd_opt(2030, 3, 18)
                .unwrap()
                .and_hms_opt(h, 0, 0)
                .unwrap()
        };
        let utc = chrono_tz::Tz::UTC;
        let busy = || host_time_off(&pool, "et-team", at(0), at(23), utc);
        assert_eq!(busy().await.unwrap(), vec![(at(9), at(10))]);

        // A member excluded by a zero weight does not count.
        sqlx::query("INSERT INTO event_type_member_weights (event_type_id, user_id, weight) VALUES ('et-team', 'm1', 0)")
            .execute(&pool)
            .await
            .unwrap();
        assert!(busy().await.unwrap().is_empty());

        // Nobody eligible: nothing can be booked.
        sqlx::query("UPDATE users SET enabled = 0 WHERE id = 'm2'")
            .execute(&pool)
            .await
            .unwrap();
        assert_eq!(busy().await.unwrap(), vec![(at(0), at(23))]);
    }

    /// Microsoft gaps follow the team: a collective event type is blocked by
    /// any member's unread calendar, a round-robin one only when every
    /// member has one, from the latest of their starts.
    #[tokio::test]
    async fn microsoft_gaps_follow_the_scheduling_mode() {
        let pool = setup_db().await;
        sqlx::raw_sql(
            "INSERT INTO users (id, email, name, role, auth_provider, username, enabled) VALUES
                 ('owner', 'owner@x', 'Owner', 'admin', 'local', 'owner', 1),
                 ('m1', 'm1@x', 'M1', 'user', 'local', 'm1', 1),
                 ('m2', 'm2@x', 'M2', 'user', 'local', 'm2', 1);
             INSERT INTO accounts (id, name, email, timezone, user_id) VALUES
                 ('acc-owner', 'Owner', 'owner@x', 'UTC', 'owner'),
                 ('acc-m1', 'M1', 'm1@x', 'UTC', 'm1'),
                 ('acc-m2', 'M2', 'm2@x', 'UTC', 'm2');
             INSERT INTO teams (id, name, slug, visibility) VALUES ('t', 'T', 't', 'public');
             INSERT INTO team_members (team_id, user_id, role, source) VALUES
                 ('t', 'm1', 'member', 'direct'), ('t', 'm2', 'member', 'direct');
             INSERT INTO event_types (id, account_id, team_id, slug, title, duration_min, scheduling_mode)
                 VALUES ('et-team', 'acc-owner', 't', 'demo', 'Demo', 30, 'collective');
             INSERT INTO caldav_sources (id, account_id, name, url, username, provider_type) VALUES
                 ('ms1', 'acc-m1', 'M', 'https://graph.microsoft.com/v1.0', 'u', 'microsoft');",
        )
        .execute(&pool)
        .await
        .unwrap();
        let day = NaiveDate::from_ymd_opt(2030, 3, 18).unwrap();
        let (start, end) = (
            day.and_hms_opt(0, 0, 0).unwrap(),
            day.and_hms_opt(23, 0, 0).unwrap(),
        );
        let utc = chrono_tz::Tz::UTC;
        let gaps = || host_microsoft_gaps(&pool, "et-team", start, end, utc);
        let spans = |g: Vec<(
            NaiveDateTime,
            NaiveDateTime,
            crate::commands::sync::CoverageGap,
        )>| { g.into_iter().map(|(s, e, _)| (s, e)).collect::<Vec<_>>() };
        // Collective: m1's unread calendar blocks the team.
        assert_eq!(spans(gaps().await.unwrap()), [(start, end)]);
        // Round robin: m2 can take it.
        sqlx::query("UPDATE event_types SET scheduling_mode = 'round_robin' WHERE id = 'et-team'")
            .execute(&pool)
            .await
            .unwrap();
        assert!(gaps().await.unwrap().is_empty());
        sqlx::query("INSERT INTO caldav_sources (id, account_id, name, url, username, provider_type) VALUES ('ms2', 'acc-m2', 'M', 'https://graph.microsoft.com/v1.0', 'u', 'microsoft')")
            .execute(&pool)
            .await
            .unwrap();
        assert_eq!(spans(gaps().await.unwrap()), [(start, end)]);
    }

    /// `event-type slots` adds the host's time off (#46), and only theirs.
    #[tokio::test]
    async fn host_time_off_counts_only_the_host() {
        let pool = setup_db().await;
        sqlx::raw_sql(
            "INSERT INTO users (id, email, name, role, auth_provider, username, enabled) VALUES
                 ('host', 'host@x', 'Host', 'admin', 'local', 'host', 1),
                 ('other', 'other@x', 'Other', 'user', 'local', 'other', 1);
             INSERT INTO accounts (id, name, email, timezone, user_id) VALUES
                 ('acc-host', 'Host', 'host@x', 'UTC', 'host'),
                 ('acc-other', 'Other', 'other@x', 'UTC', 'other');
             INSERT INTO event_types (id, account_id, slug, title, duration_min) VALUES
                 ('et-host', 'acc-host', 'mine', 'Mine', 30),
                 ('et-other', 'acc-other', 'theirs', 'Theirs', 30);
             INSERT INTO user_time_off (id, user_id, start_at, end_at, timezone) VALUES
                 ('off-host', 'host', '2030-03-18T09:00:00Z', '2030-03-18T12:00:00Z', 'UTC'),
                 ('off-other', 'other', '2030-03-18T13:00:00Z', '2030-03-18T15:00:00Z', 'UTC');",
        )
        .execute(&pool)
        .await
        .unwrap();
        let at = |h| {
            NaiveDate::from_ymd_opt(2030, 3, 18)
                .unwrap()
                .and_hms_opt(h, 0, 0)
                .unwrap()
        };
        let paris = chrono_tz::Europe::Paris;
        let busy = host_time_off(&pool, "et-host", at(0), at(23), paris)
            .await
            .unwrap();
        // In the host's zone: Paris is UTC+1 in March.
        assert_eq!(busy, vec![(at(10), at(13))]);
        assert!(host_time_off(&pool, "missing", at(0), at(23), paris)
            .await
            .unwrap()
            .is_empty());
    }

    #[tokio::test]
    async fn host_busy_bookings_only_counts_the_host() {
        let pool = setup_db().await;
        sqlx::raw_sql(
            "INSERT INTO users (id, email, name, role, auth_provider, username, enabled) VALUES
                 ('host', 'host@x', 'Host', 'admin', 'local', 'host', 1),
                 ('other', 'other@x', 'Other', 'user', 'local', 'other', 1);
             INSERT INTO accounts (id, name, email, timezone, user_id) VALUES
                 ('acc-host', 'Host', 'host@x', 'UTC', 'host'),
                 ('acc-other', 'Other', 'other@x', 'UTC', 'other');
             INSERT INTO event_types (id, account_id, slug, title, duration_min) VALUES
                 ('et-host', 'acc-host', 'mine', 'Mine', 30),
                 ('et-other', 'acc-other', 'theirs', 'Theirs', 30);
             INSERT INTO bookings (id, event_type_id, uid, guest_name, guest_email, guest_timezone, start_at, end_at, status, cancel_token, reschedule_token, assigned_user_id) VALUES
                 ('own', 'et-host', 'u1', 'G', 'g@x', 'UTC', '2030-03-18T09:00:00', '2030-03-18T09:30:00', 'confirmed', 'c1', 'r1', NULL),
                 ('assigned-to-host', 'et-other', 'u2', 'G', 'g@x', 'UTC', '2030-03-18T10:00:00', '2030-03-18T10:30:00', 'confirmed', 'c2', 'r2', 'host'),
                 ('someone-elses', 'et-other', 'u3', 'G', 'g@x', 'UTC', '2030-03-18T11:00:00', '2030-03-18T11:30:00', 'confirmed', 'c3', 'r3', NULL),
                 ('own-but-assigned-away', 'et-host', 'u4', 'G', 'g@x', 'UTC', '2030-03-18T12:00:00', '2030-03-18T12:30:00', 'confirmed', 'c4', 'r4', 'other'),
                 ('held', 'et-host', 'u5', 'G', 'g@x', 'UTC', '2030-03-18T13:00:00', '2030-03-18T13:30:00', 'pending', 'c5', 'r5', NULL);",
        )
        .execute(&pool)
        .await
        .unwrap();

        let mut starts: Vec<String> = host_busy_bookings(
            &pool,
            "et-host",
            "2030-03-18T00:00:00",
            "2030-03-18T23:59:59",
        )
        .await
        .unwrap()
        .into_iter()
        .map(|(s, _)| s)
        .collect();
        starts.sort();
        assert_eq!(
            starts,
            vec![
                "2030-03-18T09:00:00",
                "2030-03-18T10:00:00",
                "2030-03-18T13:00:00"
            ]
        );
    }

    // --- Bookable date range (migration 076) ---

    #[test]
    fn slot_day_offsets_without_window_starts_today() {
        assert_eq!(
            slot_day_offsets(7, today(), &BookingWindow::default()),
            0..7
        );
    }

    #[test]
    fn slot_day_offsets_start_at_a_future_range_and_stop_at_its_end() {
        // Today is 2 Nov; the fair runs 10–12 Nov (inclusive).
        let window = BookingWindow {
            bookable_from: Some(date("2026-11-10")),
            bookable_until: Some(date("2026-11-12")),
            ..BookingWindow::default()
        };
        assert_eq!(slot_day_offsets(7, today(), &window), 8..11);
    }

    #[test]
    fn slot_day_offsets_are_empty_once_the_range_has_ended() {
        let window = BookingWindow {
            bookable_until: Some(date("2026-11-01")),
            ..BookingWindow::default()
        };
        assert!(slot_day_offsets(7, today(), &window).is_empty());
    }

    #[test]
    fn slot_day_offsets_intersect_range_and_horizon() {
        // A 9-day horizon reaches 11 Nov, so the 12th is cut off.
        let window = BookingWindow {
            horizon_days: Some(9),
            bookable_from: Some(date("2026-11-10")),
            bookable_until: Some(date("2026-11-12")),
        };
        assert_eq!(slot_day_offsets(7, today(), &window), 8..10);
    }

    #[test]
    fn format_bookable_marks_upcoming_and_expired_ranges() {
        let window = BookingWindow {
            bookable_from: Some(date("2026-11-10")),
            bookable_until: Some(date("2026-11-12")),
            ..BookingWindow::default()
        };
        assert_eq!(
            format_bookable(&window, today()),
            "2026-11-10 – 2026-11-12 (upcoming)"
        );
        assert_eq!(
            format_bookable(&window, date("2026-11-11")),
            "2026-11-10 – 2026-11-12"
        );
        assert_eq!(
            format_bookable(&window, date("2026-11-13")),
            "2026-11-10 – 2026-11-12 (expired)"
        );
        let open_ended = BookingWindow {
            bookable_from: Some(date("2027-01-05")),
            ..BookingWindow::default()
        };
        assert_eq!(
            format_bookable(&open_ended, today()),
            "from 2027-01-05 (upcoming)"
        );
        let until_only = BookingWindow {
            bookable_until: Some(date("2026-11-20")),
            ..BookingWindow::default()
        };
        assert_eq!(format_bookable(&until_only, today()), "until 2026-11-20");
    }

    #[tokio::test]
    async fn create_stores_the_bookable_range() {
        let pool = setup_db().await;
        seed_account(&pool).await;
        run(
            &pool,
            EventTypeCommands::Create {
                title: "Fair".into(),
                slug: "fair".into(),
                duration: 30,
                description: None,
                buffer_before: 0,
                buffer_after: 0,
                bookable_from: Some(date("2026-11-10")),
                bookable_until: Some(date("2026-11-12")),
            },
        )
        .await
        .unwrap();
        let stored: (Option<String>, Option<String>) = sqlx::query_as(
            "SELECT bookable_from, bookable_until FROM event_types WHERE slug = 'fair'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            stored,
            (
                Some("2026-11-10".to_string()),
                Some("2026-11-12".to_string())
            )
        );
    }

    #[tokio::test]
    async fn create_rejects_an_end_before_the_start() {
        let pool = setup_db().await;
        seed_account(&pool).await;
        let result = run(
            &pool,
            EventTypeCommands::Create {
                title: "Backwards".into(),
                slug: "backwards".into(),
                duration: 30,
                description: None,
                buffer_before: 0,
                buffer_after: 0,
                bookable_from: Some(date("2026-11-12")),
                bookable_until: Some(date("2026-11-10")),
            },
        )
        .await;
        assert!(result.is_err());
        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM event_types WHERE slug = 'backwards'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(count, 0, "an invalid range must not be stored");
    }
}
