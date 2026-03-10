use anyhow::Result;
use chrono::{Datelike, Duration, Local, NaiveDateTime, NaiveTime};
use chrono_tz::Tz;
use clap::Subcommand;
use colored::Colorize;
use sqlx::SqlitePool;
use tabled::{Table, Tabled};
use uuid::Uuid;

use crate::utils::convert_event_to_tz;

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
    },
    /// List event types
    List,
    /// Show available slots for an event type
    Slots {
        /// Event type slug
        slug: String,
        /// Number of days to show
        #[arg(long, default_value = "7")]
        days: i32,
    },
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
        } => {
            let account: (String,) =
                sqlx::query_as("SELECT id FROM accounts LIMIT 1")
                    .fetch_optional(pool)
                    .await?
                    .ok_or_else(|| anyhow::anyhow!("No account found. Run `calrs init` first."))?;

            let id = Uuid::new_v4().to_string();

            sqlx::query(
                "INSERT INTO event_types (id, account_id, title, slug, description, duration_min, buffer_before, buffer_after)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&id)
            .bind(&account.0)
            .bind(&title)
            .bind(&slug)
            .bind(&description)
            .bind(duration)
            .bind(buffer_before)
            .bind(buffer_after)
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
            let types: Vec<(String, String, i32, bool)> = sqlx::query_as(
                "SELECT slug, title, duration_min, enabled FROM event_types ORDER BY created_at",
            )
            .fetch_all(pool)
            .await?;

            if types.is_empty() {
                println!("No event types. Create one with `calrs event-type create`.");
                return Ok(());
            }

            let rows: Vec<EventTypeRow> = types
                .into_iter()
                .map(|(slug, title, duration, enabled)| EventTypeRow {
                    slug,
                    title,
                    duration: format!("{}min", duration),
                    active: if enabled { "✓".to_string() } else { "✗".to_string() },
                })
                .collect();

            println!("{}", Table::new(rows));
        }
        EventTypeCommands::Slots { slug, days } => {
            let et: Option<(String, i32, i32, i32, i32)> = sqlx::query_as(
                "SELECT id, duration_min, buffer_before, buffer_after, min_notice_min
                 FROM event_types WHERE slug = ? AND enabled = 1",
            )
            .bind(&slug)
            .fetch_optional(pool)
            .await?;

            let (et_id, duration, buffer_before, buffer_after, min_notice) = match et {
                Some(e) => e,
                None => {
                    println!("{} No active event type with slug '{}'", "✗".red(), slug);
                    return Ok(());
                }
            };

            let rules: Vec<(i32, String, String)> = sqlx::query_as(
                "SELECT day_of_week, start_time, end_time FROM availability_rules WHERE event_type_id = ?",
            )
            .bind(&et_id)
            .fetch_all(pool)
            .await?;

            // Get busy events for the period
            let now = Local::now().naive_local();
            let min_start = now + Duration::minutes(min_notice as i64);
            let end_date = now.date() + Duration::days(days as i64);

            let host_tz: Tz = iana_time_zone::get_timezone()
                .ok()
                .and_then(|s| s.parse::<Tz>().ok())
                .unwrap_or(Tz::UTC);

            // Fetch all events in range (both YYYYMMDD and ISO formats)
            let end_compact = end_date.format("%Y%m%d").to_string();
            let now_compact = now.format("%Y%m%dT%H%M%S").to_string();
            let end_iso = end_date.format("%Y-%m-%dT23:59:59").to_string();
            let now_iso = now.format("%Y-%m-%dT%H:%M:%S").to_string();

            // Non-recurring events (with timezone for conversion)
            let events: Vec<(String, String, Option<String>)> = sqlx::query_as(
                "SELECT start_at, end_at, timezone FROM events
                 WHERE (rrule IS NULL OR rrule = '')
                   AND (status IS NULL OR status != 'CANCELLED')
                   AND ((start_at <= ? AND end_at >= ?) OR (start_at <= ? AND end_at >= ?))",
            )
            .bind(&end_compact)
            .bind(&now_compact)
            .bind(&end_iso)
            .bind(&now_iso)
            .fetch_all(pool)
            .await?;

            let mut busy_events: Vec<(String, String)> = events.iter()
                .filter_map(|(s, e, tz)| {
                    let start = convert_event_to_tz(parse_datetime(s)?, tz.as_deref(), host_tz);
                    let end = convert_event_to_tz(parse_datetime(e)?, tz.as_deref(), host_tz);
                    Some((start.format("%Y-%m-%dT%H:%M:%S").to_string(), end.format("%Y-%m-%dT%H:%M:%S").to_string()))
                })
                .collect();

            // Bookings (already in host-local time, no conversion needed)
            let booking_busy: Vec<(String, String)> = sqlx::query_as(
                "SELECT start_at, end_at FROM bookings
                 WHERE status = 'confirmed'
                   AND start_at <= ? AND end_at >= ?",
            )
            .bind(&end_iso)
            .bind(&now_iso)
            .fetch_all(pool)
            .await
            .unwrap_or_default();

            busy_events.extend(booking_busy);

            // Expand recurring events
            let end_compact_rrule = end_date.format("%Y%m%dT235959").to_string();
            let recurring: Vec<(String, String, String, Option<String>, Option<String>)> = sqlx::query_as(
                "SELECT start_at, end_at, rrule, raw_ical, timezone FROM events
                 WHERE rrule IS NOT NULL AND rrule != ''
                   AND (status IS NULL OR status != 'CANCELLED')
                   AND (start_at <= ? OR start_at <= ?)",
            )
            .bind(&end_iso)
            .bind(&end_compact_rrule)
            .fetch_all(pool)
            .await
            .unwrap_or_default();

            let window_end_dt = end_date.and_hms_opt(23, 59, 59).unwrap_or(now);
            for (s, e, rrule_str, raw_ical, event_tz) in &recurring {
                if let (Some(ev_start), Some(ev_end)) = (parse_datetime(s), parse_datetime(e)) {
                    let exdates = raw_ical.as_deref().map(crate::rrule::extract_exdates).unwrap_or_default();
                    let occurrences = crate::rrule::expand_rrule(ev_start, ev_end, rrule_str, &exdates, now, window_end_dt);
                    for (os, oe) in occurrences {
                        let cs = convert_event_to_tz(os, event_tz.as_deref(), host_tz);
                        let ce = convert_event_to_tz(oe, event_tz.as_deref(), host_tz);
                        busy_events.push((cs.format("%Y-%m-%dT%H:%M:%S").to_string(), ce.format("%Y-%m-%dT%H:%M:%S").to_string()));
                    }
                }
            }

            println!(
                "Available slots for {} ({}min):\n",
                slug.bold(),
                duration
            );

            let slot_duration = Duration::minutes(duration as i64);

            for day_offset in 0..days {
                let date = now.date() + Duration::days(day_offset as i64);
                let weekday = date.weekday().num_days_from_sunday() as i32;

                let day_rules: Vec<&(i32, String, String)> = rules
                    .iter()
                    .filter(|(d, _, _)| *d == weekday)
                    .collect();

                if day_rules.is_empty() {
                    continue;
                }

                let mut slots = Vec::new();

                for (_, start_str, end_str) in &day_rules {
                    let window_start = NaiveTime::parse_from_str(start_str, "%H:%M")?;
                    let window_end = NaiveTime::parse_from_str(end_str, "%H:%M")?;

                    let mut cursor = window_start;
                    while cursor + slot_duration <= window_end {
                        let slot_start = date.and_time(cursor);
                        let slot_end = slot_start + slot_duration;

                        // Check minimum notice
                        if slot_start < min_start {
                            cursor = cursor + Duration::minutes(duration as i64);
                            continue;
                        }

                        // Check conflicts with busy events
                        let buf_start = slot_start - Duration::minutes(buffer_before as i64);
                        let buf_end = slot_end + Duration::minutes(buffer_after as i64);

                        let has_conflict = busy_events.iter().any(|(bs, be)| {
                            let ev_start = parse_datetime(bs);
                            let ev_end = parse_datetime(be);
                            match (ev_start, ev_end) {
                                (Some(s), Some(e)) => s < buf_end && e > buf_start,
                                _ => false,
                            }
                        });

                        if !has_conflict {
                            slots.push(format!(
                                "  {} – {}",
                                cursor.format("%H:%M"),
                                (cursor + slot_duration).format("%H:%M")
                            ));
                        }

                        cursor = cursor + Duration::minutes(duration as i64);
                    }
                }

                if !slots.is_empty() {
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

/// Parse datetime from iCal formats: YYYYMMDD, YYYYMMDDTHHMMSS, YYYY-MM-DDTHH:MM:SS
fn parse_datetime(s: &str) -> Option<NaiveDateTime> {
    // YYYYMMDDTHHMMSS
    if let Ok(dt) = NaiveDateTime::parse_from_str(s, "%Y%m%dT%H%M%S") {
        return Some(dt);
    }
    // YYYY-MM-DDTHH:MM:SS
    if let Ok(dt) = NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S") {
        return Some(dt);
    }
    // YYYYMMDD (all-day → start of day)
    if let Ok(d) = chrono::NaiveDate::parse_from_str(s, "%Y%m%d") {
        return Some(d.and_hms_opt(0, 0, 0)?);
    }
    // YYYY-MM-DD
    if let Ok(d) = chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d") {
        return Some(d.and_hms_opt(0, 0, 0)?);
    }
    None
}
