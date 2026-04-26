use anyhow::{bail, Result};
use chrono::{Datelike, Duration, Local, NaiveDate, NaiveDateTime, NaiveTime};
use chrono_tz::Tz;
use clap::Subcommand;
use colored::Colorize;
use sqlx::SqlitePool;
use tabled::{Table, Tabled};
use uuid::Uuid;

use std::io::{self, Write};

use crate::utils::{convert_event_to_tz, prompt};

#[derive(Debug, Subcommand)]
pub enum BookingCommands {
    /// Book a slot on an event type
    Create {
        /// Event type slug
        slug: String,
        /// Date (YYYY-MM-DD)
        #[arg(long)]
        date: Option<String>,
        /// Start time (HH:MM)
        #[arg(long)]
        time: Option<String>,
        /// Guest name
        #[arg(long)]
        name: Option<String>,
        /// Guest email
        #[arg(long)]
        email: Option<String>,
        /// Guest timezone
        #[arg(long, default_value = "UTC")]
        timezone: String,
        /// Notes
        #[arg(long)]
        notes: Option<String>,
    },
    /// List bookings
    List {
        /// Show only upcoming bookings
        #[arg(long)]
        upcoming: bool,
    },
    /// Cancel a booking
    Cancel {
        /// Booking ID (prefix match)
        id: String,
    },
}

#[derive(Tabled)]
struct BookingRow {
    #[tabled(rename = "ID")]
    id: String,
    #[tabled(rename = "Guest")]
    guest: String,
    #[tabled(rename = "Event Type")]
    event_type: String,
    #[tabled(rename = "When")]
    when: String,
    #[tabled(rename = "Status")]
    status: String,
}

pub async fn run(pool: &SqlitePool, key: &[u8; 32], cmd: BookingCommands) -> Result<()> {
    match cmd {
        BookingCommands::Create {
            slug,
            date,
            time,
            name,
            email,
            timezone,
            notes,
        } => {
            // Look up event type
            let et: Option<(String, String, i32, i32, i32, i32)> = sqlx::query_as(
                "SELECT id, title, duration_min, buffer_before, buffer_after, min_notice_min
                 FROM event_types WHERE slug = ? AND enabled = 1",
            )
            .bind(&slug)
            .fetch_optional(pool)
            .await?;

            let (et_id, et_title, duration, buffer_before, buffer_after, min_notice) = match et {
                Some(e) => e,
                None => {
                    bail!("No active event type with slug '{}'", slug);
                }
            };

            // Get date and time (prompt if not provided)
            let date_str = date.unwrap_or_else(|| prompt("Date (YYYY-MM-DD)"));
            let time_str = time.unwrap_or_else(|| prompt("Start time (HH:MM)"));
            let guest_name = name.unwrap_or_else(|| prompt("Guest name"));
            let guest_email = email.unwrap_or_else(|| prompt("Guest email"));

            let date = NaiveDate::parse_from_str(&date_str, "%Y-%m-%d")?;
            let start_time = NaiveTime::parse_from_str(&time_str, "%H:%M")?;
            let slot_start = date.and_time(start_time);
            let slot_end = slot_start + Duration::minutes(duration as i64);

            // Validate: minimum notice
            let now = Local::now().naive_local();
            let min_start = now + Duration::minutes(min_notice as i64);
            if slot_start < min_start {
                bail!(
                    "Slot is too soon. Minimum notice is {} minutes (earliest: {})",
                    min_notice,
                    min_start.format("%Y-%m-%d %H:%M")
                );
            }

            // Validate: within availability rules
            let weekday = date.weekday().num_days_from_sunday() as i32;
            let rule_match: Option<(String,)> = sqlx::query_as(
                "SELECT id FROM availability_rules
                 WHERE event_type_id = ? AND day_of_week = ?
                   AND start_time <= ? AND end_time >= ?",
            )
            .bind(&et_id)
            .bind(weekday)
            .bind(start_time.format("%H:%M").to_string())
            .bind(slot_end.time().format("%H:%M").to_string())
            .fetch_optional(pool)
            .await?;

            if rule_match.is_none() {
                bail!(
                    "Slot {} {} – {} is outside availability windows",
                    date_str,
                    time_str,
                    slot_end.time().format("%H:%M")
                );
            }

            // Validate: no conflicts with existing events
            let buf_start = slot_start - Duration::minutes(buffer_before as i64);
            let buf_end = slot_end + Duration::minutes(buffer_after as i64);

            let host_tz: Tz = iana_time_zone::get_timezone()
                .ok()
                .and_then(|s| s.parse::<Tz>().ok())
                .unwrap_or(Tz::UTC);

            let conflicts: Vec<(String, String, Option<String>, Option<String>)> = sqlx::query_as(
                "SELECT e.start_at, e.end_at, e.summary, e.timezone FROM events e
                 JOIN calendars c ON c.id = e.calendar_id
                 WHERE c.is_busy = 1
                   AND (NOT EXISTS (SELECT 1 FROM event_type_calendars WHERE event_type_id = ?)
                        OR c.id IN (SELECT calendar_id FROM event_type_calendars WHERE event_type_id = ?))
                   AND (e.status IS NULL OR e.status != 'CANCELLED')
                   AND (e.transp IS NULL OR e.transp != 'TRANSPARENT')",
            )
            .bind(&et_id).bind(&et_id)
            .fetch_all(pool)
            .await?;

            for (bs, be, summary, event_tz) in &conflicts {
                let ev_start = parse_datetime(bs)
                    .map(|dt| convert_event_to_tz(dt, event_tz.as_deref(), host_tz));
                let ev_end = parse_datetime(be)
                    .map(|dt| convert_event_to_tz(dt, event_tz.as_deref(), host_tz));
                if let (Some(s), Some(e)) = (ev_start, ev_end) {
                    if s < buf_end && e > buf_start {
                        bail!(
                            "Conflict with '{}' ({} – {})",
                            summary.as_deref().unwrap_or("(no title)"),
                            s.format("%H:%M"),
                            e.format("%H:%M")
                        );
                    }
                }
            }

            // Validate: no conflicts with existing bookings
            let booking_conflicts: Vec<(String, String)> =
                sqlx::query_as("SELECT start_at, end_at FROM bookings WHERE status = 'confirmed'")
                    .fetch_all(pool)
                    .await?;

            for (bs, be) in &booking_conflicts {
                let bk_start = parse_datetime(bs);
                let bk_end = parse_datetime(be);
                if let (Some(s), Some(e)) = (bk_start, bk_end) {
                    if s < buf_end && e > buf_start {
                        bail!(
                            "Conflict with an existing booking at {} – {}",
                            s.format("%H:%M"),
                            e.format("%H:%M")
                        );
                    }
                }
            }

            // All good — create the booking
            let id = Uuid::new_v4().to_string();
            let uid = format!("{}@calrs", Uuid::new_v4());
            let cancel_token = Uuid::new_v4().to_string();
            let reschedule_token = Uuid::new_v4().to_string();
            let start_at = slot_start.format("%Y-%m-%dT%H:%M:%S").to_string();
            let end_at = slot_end.format("%Y-%m-%dT%H:%M:%S").to_string();

            sqlx::query(
                "INSERT INTO bookings (id, event_type_id, uid, guest_name, guest_email, guest_timezone, notes, start_at, end_at, cancel_token, reschedule_token)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&id)
            .bind(&et_id)
            .bind(&uid)
            .bind(&guest_name)
            .bind(&guest_email)
            .bind(&timezone)
            .bind(&notes)
            .bind(&start_at)
            .bind(&end_at)
            .bind(&cancel_token)
            .bind(&reschedule_token)
            .execute(pool)
            .await?;

            println!();
            println!("{} Booking confirmed!", "✓".green());
            println!("  {} {}", "Event:".bold(), et_title);
            println!(
                "  {} {} {} – {}",
                "When:".bold(),
                date_str,
                time_str,
                slot_end.time().format("%H:%M")
            );
            println!("  {} {} <{}>", "Guest:".bold(), guest_name, guest_email);
            println!("  {} {}", "ID:".bold(), &id[..8]);

            // Send email notifications if SMTP is configured
            if let Some(smtp_config) = crate::email::load_smtp_config(pool, key).await? {
                // Fetch host info
                let host: Option<(String, String)> = sqlx::query_as(
                    "SELECT u.name, COALESCE(u.booking_email, u.email) FROM users u JOIN accounts a ON a.user_id = u.id WHERE a.id = (SELECT account_id FROM event_types WHERE id = ?)",
                )
                .bind(&et_id)
                .fetch_optional(pool)
                .await?;

                if let Some((host_name, host_email)) = host {
                    let details = crate::email::BookingDetails {
                        event_title: et_title.clone(),
                        date: date_str.clone(),
                        start_time: time_str.clone(),
                        end_time: slot_end.time().format("%H:%M").to_string(),
                        guest_name: guest_name.clone(),
                        guest_email: guest_email.clone(),
                        guest_timezone: timezone.clone(),
                        host_name,
                        host_email,
                        uid: uid.clone(),
                        notes: notes.clone(),
                        location: None,
                        reminder_minutes: None,
                        additional_attendees: vec![],
                        ..Default::default()
                    };

                    print!(
                        "  {} Sending confirmation to {}… ",
                        "…".dimmed(),
                        guest_email
                    );
                    io::stdout().flush().unwrap();
                    match crate::email::send_guest_confirmation(&smtp_config, &details, None).await
                    {
                        Ok(_) => println!("{}", "sent".green()),
                        Err(e) => println!("{} {}", "failed:".red(), e),
                    }

                    print!(
                        "  {} Sending notification to {}… ",
                        "…".dimmed(),
                        details.host_email
                    );
                    io::stdout().flush().unwrap();
                    match crate::email::send_host_notification(&smtp_config, &details).await {
                        Ok(_) => println!("{}", "sent".green()),
                        Err(e) => println!("{} {}", "failed:".red(), e),
                    }
                }
            }
        }
        BookingCommands::List { upcoming } => {
            let query = if upcoming {
                "SELECT b.id, b.guest_name, b.guest_email, et.title, b.start_at, b.end_at, b.status
                 FROM bookings b
                 JOIN event_types et ON b.event_type_id = et.id
                 WHERE b.start_at >= datetime('now')
                 ORDER BY b.start_at"
            } else {
                "SELECT b.id, b.guest_name, b.guest_email, et.title, b.start_at, b.end_at, b.status
                 FROM bookings b
                 JOIN event_types et ON b.event_type_id = et.id
                 ORDER BY b.start_at DESC"
            };

            let bookings: Vec<(String, String, String, String, String, String, String)> =
                sqlx::query_as(query).fetch_all(pool).await?;

            if bookings.is_empty() {
                println!("No bookings found.");
                return Ok(());
            }

            let rows: Vec<BookingRow> = bookings
                .into_iter()
                .map(|(id, guest_name, guest_email, title, start, end, status)| {
                    let time = if start.contains('T') {
                        let date = &start[..10];
                        let start_time = &start[11..16];
                        let end_time = if end.len() > 16 { &end[11..16] } else { &end };
                        format!("{} {} – {}", date, start_time, end_time)
                    } else {
                        start
                    };

                    BookingRow {
                        id: id[..8].to_string(),
                        guest: format!("{} <{}>", guest_name, guest_email),
                        event_type: title,
                        when: time,
                        status,
                    }
                })
                .collect();

            println!("{}", Table::new(rows));
        }
        BookingCommands::Cancel { id } => {
            let booking: Option<(String, String, String, String, String, String, String, String)> = sqlx::query_as(
                "SELECT b.id, b.uid, b.guest_name, b.guest_email, b.start_at, b.end_at, et.title, COALESCE(b.guest_timezone, 'UTC')
                 FROM bookings b
                 JOIN event_types et ON et.id = b.event_type_id
                 WHERE b.id LIKE ? || '%' AND b.status = 'confirmed'",
            )
            .bind(&id)
            .fetch_optional(pool)
            .await?;

            match booking {
                Some((
                    full_id,
                    uid,
                    guest_name,
                    guest_email,
                    start_at,
                    end_at,
                    event_title,
                    guest_timezone,
                )) => {
                    let reason_input =
                        prompt("Reason for cancellation (optional, press Enter to skip)");
                    let reason = if reason_input.is_empty() {
                        None
                    } else {
                        Some(reason_input)
                    };

                    sqlx::query("UPDATE bookings SET status = 'cancelled' WHERE id = ?")
                        .bind(&full_id)
                        .execute(pool)
                        .await?;
                    println!("{} Booking {} cancelled.", "✓".green(), &full_id[..8]);

                    // Send cancellation emails
                    if let Some(smtp_config) = crate::email::load_smtp_config(pool, key).await? {
                        let host: Option<(String, String)> = sqlx::query_as(
                            "SELECT u.name, COALESCE(u.booking_email, u.email) FROM users u
                             JOIN accounts a ON a.user_id = u.id
                             JOIN event_types et ON et.account_id = a.id
                             JOIN bookings b ON b.event_type_id = et.id
                             WHERE b.id = ?",
                        )
                        .bind(&full_id)
                        .fetch_optional(pool)
                        .await?;

                        if let Some((host_name, host_email)) = host {
                            let date = if start_at.len() >= 10 {
                                &start_at[..10]
                            } else {
                                &start_at
                            };
                            let start_time = if start_at.len() >= 16 {
                                &start_at[11..16]
                            } else {
                                "00:00"
                            };
                            let end_time = if end_at.len() >= 16 {
                                &end_at[11..16]
                            } else {
                                "00:00"
                            };

                            let details = crate::email::CancellationDetails {
                                event_title,
                                date: date.to_string(),
                                start_time: start_time.to_string(),
                                end_time: end_time.to_string(),
                                guest_name: guest_name.clone(),
                                guest_email: guest_email.clone(),
                                guest_timezone: guest_timezone.clone(),
                                host_name,
                                host_email,
                                uid,
                                reason,
                                cancelled_by_host: true,
                                ..Default::default()
                            };

                            print!(
                                "  {} Sending cancellation to {}… ",
                                "…".dimmed(),
                                guest_email
                            );
                            io::stdout().flush().unwrap();
                            match crate::email::send_guest_cancellation(&smtp_config, &details)
                                .await
                            {
                                Ok(_) => println!("{}", "sent".green()),
                                Err(e) => println!("{} {}", "failed:".red(), e),
                            }

                            print!(
                                "  {} Sending cancellation to {}… ",
                                "…".dimmed(),
                                details.host_email
                            );
                            io::stdout().flush().unwrap();
                            match crate::email::send_host_cancellation(&smtp_config, &details).await
                            {
                                Ok(_) => println!("{}", "sent".green()),
                                Err(e) => println!("{} {}", "failed:".red(), e),
                            }
                        }
                    }
                }
                None => {
                    println!("{} No confirmed booking found matching '{}'", "✗".red(), id);
                }
            }
        }
    }

    Ok(())
}

/// Parse datetime from iCal formats: YYYYMMDD, YYYYMMDDTHHMMSS, YYYY-MM-DDTHH:MM:SS
fn parse_datetime(s: &str) -> Option<NaiveDateTime> {
    if let Ok(dt) = NaiveDateTime::parse_from_str(s, "%Y%m%dT%H%M%S") {
        return Some(dt);
    }
    if let Ok(dt) = NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S") {
        return Some(dt);
    }
    if let Ok(d) = NaiveDate::parse_from_str(s, "%Y%m%d") {
        return d.and_hms_opt(0, 0, 0);
    }
    if let Ok(d) = NaiveDate::parse_from_str(s, "%Y-%m-%d") {
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

    /// Seed a user, account, and event type. Returns (user_id, account_id, event_type_id).
    async fn seed_event_type(pool: &SqlitePool) -> (String, String, String) {
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

        let et_id = Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO event_types (id, account_id, title, slug, duration_min, buffer_before, buffer_after, min_notice_min, enabled)
             VALUES (?, ?, 'Test Meeting', 'test-meeting', 30, 0, 0, 0, 1)",
        )
        .bind(&et_id)
        .bind(&account_id)
        .execute(pool)
        .await
        .unwrap();

        // Add availability Mon-Fri 09:00-17:00
        for day in [1, 2, 3, 4, 5] {
            let rule_id = Uuid::new_v4().to_string();
            sqlx::query(
                "INSERT INTO availability_rules (id, event_type_id, day_of_week, start_time, end_time)
                 VALUES (?, ?, ?, '09:00', '17:00')",
            )
            .bind(&rule_id)
            .bind(&et_id)
            .bind(day)
            .execute(pool)
            .await
            .unwrap();
        }

        (user_id, account_id, et_id)
    }

    /// Insert a booking directly into the DB (bypasses interactive prompts and time checks)
    async fn insert_booking(
        pool: &SqlitePool,
        et_id: &str,
        guest_name: &str,
        guest_email: &str,
        start: &str,
        end: &str,
        status: &str,
    ) -> String {
        let id = Uuid::new_v4().to_string();
        let uid = format!("{}@calrs", Uuid::new_v4());
        let cancel_token = Uuid::new_v4().to_string();
        let reschedule_token = Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO bookings (id, event_type_id, uid, guest_name, guest_email, guest_timezone, start_at, end_at, status, cancel_token, reschedule_token)
             VALUES (?, ?, ?, ?, ?, 'UTC', ?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(et_id)
        .bind(&uid)
        .bind(guest_name)
        .bind(guest_email)
        .bind(start)
        .bind(end)
        .bind(status)
        .bind(&cancel_token)
        .bind(&reschedule_token)
        .execute(pool)
        .await
        .unwrap();
        id
    }

    #[tokio::test]
    async fn test_list_bookings_empty() {
        let pool = setup_db().await;
        let key = [0u8; 32];
        let result = run(&pool, &key, BookingCommands::List { upcoming: false }).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_list_bookings_with_data() {
        let pool = setup_db().await;
        let key = [0u8; 32];
        let (_user_id, _account_id, et_id) = seed_event_type(&pool).await;

        insert_booking(
            &pool,
            &et_id,
            "Alice Guest",
            "alice@guest.com",
            "2026-06-15T10:00:00",
            "2026-06-15T10:30:00",
            "confirmed",
        )
        .await;

        insert_booking(
            &pool,
            &et_id,
            "Bob Guest",
            "bob@guest.com",
            "2026-06-16T14:00:00",
            "2026-06-16T14:30:00",
            "confirmed",
        )
        .await;

        let result = run(&pool, &key, BookingCommands::List { upcoming: false }).await;
        assert!(result.is_ok());

        let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM bookings")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count.0, 2);
    }

    #[tokio::test]
    async fn test_list_bookings_upcoming_filter() {
        let pool = setup_db().await;
        let key = [0u8; 32];
        let (_user_id, _account_id, et_id) = seed_event_type(&pool).await;

        // Past booking
        insert_booking(
            &pool,
            &et_id,
            "Past Guest",
            "past@guest.com",
            "2020-01-01T10:00:00",
            "2020-01-01T10:30:00",
            "confirmed",
        )
        .await;

        // Future booking
        insert_booking(
            &pool,
            &et_id,
            "Future Guest",
            "future@guest.com",
            "2030-06-15T10:00:00",
            "2030-06-15T10:30:00",
            "confirmed",
        )
        .await;

        // List upcoming only should succeed (not error out)
        let result = run(&pool, &key, BookingCommands::List { upcoming: true }).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_cancel_nonexistent_booking() {
        let pool = setup_db().await;
        let key = [0u8; 32];
        // Cancelling a non-existent booking should succeed (prints "not found")
        let result = run(
            &pool,
            &key,
            BookingCommands::Cancel {
                id: "nonexistent".to_string(),
            },
        )
        .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_booking_status_lifecycle() {
        let pool = setup_db().await;
        let (_user_id, _account_id, et_id) = seed_event_type(&pool).await;

        let booking_id = insert_booking(
            &pool,
            &et_id,
            "Lifecycle Guest",
            "life@guest.com",
            "2026-06-15T10:00:00",
            "2026-06-15T10:30:00",
            "confirmed",
        )
        .await;

        // Verify initial status
        let status: (String,) = sqlx::query_as("SELECT status FROM bookings WHERE id = ?")
            .bind(&booking_id)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(status.0, "confirmed");

        // Cancel directly via SQL (since Cancel command requires interactive prompt)
        sqlx::query("UPDATE bookings SET status = 'cancelled' WHERE id = ?")
            .bind(&booking_id)
            .execute(&pool)
            .await
            .unwrap();

        let status: (String,) = sqlx::query_as("SELECT status FROM bookings WHERE id = ?")
            .bind(&booking_id)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(status.0, "cancelled");
    }

    #[tokio::test]
    async fn test_double_booking_prevention() {
        let pool = setup_db().await;
        let (_user_id, _account_id, et_id) = seed_event_type(&pool).await;

        // First booking succeeds
        insert_booking(
            &pool,
            &et_id,
            "Guest 1",
            "g1@test.com",
            "2026-06-15T10:00:00",
            "2026-06-15T10:30:00",
            "confirmed",
        )
        .await;

        // Second booking at the same time should fail (partial unique index)
        let id2 = Uuid::new_v4().to_string();
        let result = sqlx::query(
            "INSERT INTO bookings (id, event_type_id, uid, guest_name, guest_email, guest_timezone, start_at, end_at, status, cancel_token, reschedule_token)
             VALUES (?, ?, ?, 'Guest 2', 'g2@test.com', 'UTC', '2026-06-15T10:00:00', '2026-06-15T10:30:00', 'confirmed', ?, ?)",
        )
        .bind(&id2)
        .bind(&et_id)
        .bind(&format!("{}@calrs", Uuid::new_v4()))
        .bind(&Uuid::new_v4().to_string())
        .bind(&Uuid::new_v4().to_string())
        .execute(&pool)
        .await;

        assert!(
            result.is_err(),
            "Double booking at the same time/event_type should be prevented by unique index"
        );
    }

    #[tokio::test]
    async fn test_parse_datetime_formats() {
        assert_eq!(
            parse_datetime("20250315T100000"),
            Some(
                NaiveDateTime::parse_from_str("2025-03-15T10:00:00", "%Y-%m-%dT%H:%M:%S").unwrap()
            )
        );
        assert_eq!(
            parse_datetime("2025-03-15T10:00:00"),
            Some(
                NaiveDateTime::parse_from_str("2025-03-15T10:00:00", "%Y-%m-%dT%H:%M:%S").unwrap()
            )
        );
        assert!(parse_datetime("20250315").is_some());
        assert!(parse_datetime("2025-03-15").is_some());
        assert!(parse_datetime("garbage").is_none());
        assert!(parse_datetime("").is_none());
    }
}
