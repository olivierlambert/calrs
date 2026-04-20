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
            let account: (String,) = sqlx::query_as("SELECT id FROM accounts LIMIT 1")
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
                    active: if enabled {
                        "✓".to_string()
                    } else {
                        "✗".to_string()
                    },
                })
                .collect();

            println!("{}", Table::new(rows));
        }
        EventTypeCommands::Slots { slug, days } => {
            let et: Option<(String, i32, i32, i32, i32, Option<i32>)> = sqlx::query_as(
                "SELECT id, duration_min, buffer_before, buffer_after, min_notice_min, slot_interval_min
                 FROM event_types WHERE slug = ? AND enabled = 1",
            )
            .bind(&slug)
            .fetch_optional(pool)
            .await?;

            let (et_id, duration, buffer_before, buffer_after, min_notice, slot_interval) = match et
            {
                Some(e) => e,
                None => {
                    println!("{} No active event type with slug '{}'", "✗".red(), slug);
                    return Ok(());
                }
            };
            let interval = slot_interval.filter(|v| *v > 0).unwrap_or(duration);

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
                "SELECT e.start_at, e.end_at, e.timezone FROM events e
                 JOIN calendars c ON c.id = e.calendar_id
                 WHERE c.is_busy = 1
                   AND (NOT EXISTS (SELECT 1 FROM event_type_calendars WHERE event_type_id = ?)
                        OR c.id IN (SELECT calendar_id FROM event_type_calendars WHERE event_type_id = ?))
                   AND (e.rrule IS NULL OR e.rrule = '')
                   AND (e.status IS NULL OR e.status != 'CANCELLED')
                   AND (e.transp IS NULL OR e.transp != 'TRANSPARENT')
                   AND ((e.start_at <= ? AND e.end_at >= ?) OR (e.start_at <= ? AND e.end_at >= ?))",
            )
            .bind(&et_id).bind(&et_id)
            .bind(&end_compact)
            .bind(&now_compact)
            .bind(&end_iso)
            .bind(&now_iso)
            .fetch_all(pool)
            .await?;

            let mut busy_events: Vec<(String, String)> = events
                .iter()
                .filter_map(|(s, e, tz)| {
                    let start = convert_event_to_tz(parse_datetime(s)?, tz.as_deref(), host_tz);
                    let end = convert_event_to_tz(parse_datetime(e)?, tz.as_deref(), host_tz);
                    Some((
                        start.format("%Y-%m-%dT%H:%M:%S").to_string(),
                        end.format("%Y-%m-%dT%H:%M:%S").to_string(),
                    ))
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
                "SELECT e.start_at, e.end_at, e.rrule, e.raw_ical, e.timezone FROM events e
                 JOIN calendars c ON c.id = e.calendar_id
                 WHERE c.is_busy = 1
                   AND (NOT EXISTS (SELECT 1 FROM event_type_calendars WHERE event_type_id = ?)
                        OR c.id IN (SELECT calendar_id FROM event_type_calendars WHERE event_type_id = ?))
                   AND (e.status IS NULL OR e.status != 'CANCELLED')
                   AND (e.transp IS NULL OR e.transp != 'TRANSPARENT')
                   AND e.rrule IS NOT NULL AND e.rrule != '' AND (e.start_at <= ? OR e.start_at <= ?)",
            )
            .bind(&et_id).bind(&et_id)
            .bind(&end_iso)
            .bind(&end_compact_rrule)
            .fetch_all(pool)
            .await
            .unwrap_or_default();

            let window_end_dt = end_date.and_hms_opt(23, 59, 59).unwrap_or(now);
            for (s, e, rrule_str, raw_ical, event_tz) in &recurring {
                if let (Some(ev_start), Some(ev_end)) = (parse_datetime(s), parse_datetime(e)) {
                    let exdates = raw_ical
                        .as_deref()
                        .map(crate::rrule::extract_exdates)
                        .unwrap_or_default();
                    let occurrences = crate::rrule::expand_rrule(
                        ev_start,
                        ev_end,
                        rrule_str,
                        &exdates,
                        now,
                        window_end_dt,
                    );
                    for (os, oe) in occurrences {
                        let cs = convert_event_to_tz(os, event_tz.as_deref(), host_tz);
                        let ce = convert_event_to_tz(oe, event_tz.as_deref(), host_tz);
                        busy_events.push((
                            cs.format("%Y-%m-%dT%H:%M:%S").to_string(),
                            ce.format("%Y-%m-%dT%H:%M:%S").to_string(),
                        ));
                    }
                }
            }

            println!("Available slots for {} ({}min):\n", slug.bold(), duration);

            let slot_duration = Duration::minutes(duration as i64);
            let slot_step = Duration::minutes(interval.max(1) as i64);

            for day_offset in 0..days {
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

                    let mut cursor = window_start;
                    while cursor + slot_duration <= window_end {
                        let slot_start = date.and_time(cursor);
                        let slot_end = slot_start + slot_duration;

                        // Check minimum notice
                        if slot_start < min_start {
                            cursor += slot_step;
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
        return d.and_hms_opt(0, 0, 0);
    }
    // YYYY-MM-DD
    if let Ok(d) = chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d") {
        return d.and_hms_opt(0, 0, 0);
    }
    None
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
            },
        )
        .await;
        assert!(result.is_err(), "Duplicate slug should fail");
    }

    #[tokio::test]
    async fn test_parse_datetime_formats() {
        // iCal compact
        assert!(parse_datetime("20250315T100000").is_some());
        // ISO format
        assert!(parse_datetime("2025-03-15T10:00:00").is_some());
        // All-day compact
        assert!(parse_datetime("20250315").is_some());
        // All-day ISO
        assert!(parse_datetime("2025-03-15").is_some());
        // Invalid
        assert!(parse_datetime("not-a-date").is_none());
    }
}
