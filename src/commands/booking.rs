use anyhow::{bail, Result};
use chrono::{Datelike, Duration, Local, NaiveDate, NaiveDateTime, NaiveTime};
use clap::Subcommand;
use colored::Colorize;
use sqlx::SqlitePool;
use tabled::{Table, Tabled};
use uuid::Uuid;

use std::io::{self, Write};

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

fn prompt(label: &str) -> String {
    print!("{}: ", label);
    io::stdout().flush().unwrap();
    let mut input = String::new();
    io::stdin().read_line(&mut input).unwrap();
    input.trim().to_string()
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

pub async fn run(pool: &SqlitePool, cmd: BookingCommands) -> Result<()> {
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

            let conflicts: Vec<(String, String, Option<String>)> = sqlx::query_as(
                "SELECT start_at, end_at, summary FROM events",
            )
            .fetch_all(pool)
            .await?;

            for (bs, be, summary) in &conflicts {
                let ev_start = parse_datetime(bs);
                let ev_end = parse_datetime(be);
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
            let booking_conflicts: Vec<(String, String)> = sqlx::query_as(
                "SELECT start_at, end_at FROM bookings WHERE status = 'confirmed'",
            )
            .fetch_all(pool)
            .await?;

            for (bs, be) in &booking_conflicts {
                let bk_start = parse_datetime(bs);
                let bk_end = parse_datetime(be);
                if let (Some(s), Some(e)) = (bk_start, bk_end) {
                    if s < buf_end && e > buf_start {
                        bail!("Conflict with an existing booking at {} – {}", s.format("%H:%M"), e.format("%H:%M"));
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
            println!("  {} {} {} – {}", "When:".bold(), date_str, time_str, slot_end.time().format("%H:%M"));
            println!("  {} {} <{}>", "Guest:".bold(), guest_name, guest_email);
            println!("  {} {}", "ID:".bold(), &id[..8]);

            // Send email notifications if SMTP is configured
            if let Some(smtp_config) = crate::email::load_smtp_config(pool).await? {
                // Fetch host info
                let host: Option<(String, String)> = sqlx::query_as(
                    "SELECT name, email FROM accounts WHERE id = (SELECT account_id FROM event_types WHERE id = ?)",
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
                    };

                    print!("  {} Sending confirmation to {}… ", "…".dimmed(), guest_email);
                    io::stdout().flush().unwrap();
                    match crate::email::send_guest_confirmation(&smtp_config, &details).await {
                        Ok(_) => println!("{}", "sent".green()),
                        Err(e) => println!("{} {}", "failed:".red(), e),
                    }

                    print!("  {} Sending notification to {}… ", "…".dimmed(), details.host_email);
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
                .map(
                    |(id, guest_name, guest_email, title, start, end, status)| {
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
                    },
                )
                .collect();

            println!("{}", Table::new(rows));
        }
        BookingCommands::Cancel { id } => {
            let booking: Option<(String, String, String, String, String, String, String)> = sqlx::query_as(
                "SELECT b.id, b.uid, b.guest_name, b.guest_email, b.start_at, b.end_at, et.title
                 FROM bookings b
                 JOIN event_types et ON et.id = b.event_type_id
                 WHERE b.id LIKE ? || '%' AND b.status = 'confirmed'",
            )
            .bind(&id)
            .fetch_optional(pool)
            .await?;

            match booking {
                Some((full_id, uid, guest_name, guest_email, start_at, end_at, event_title)) => {
                    let reason_input = prompt("Reason for cancellation (optional, press Enter to skip)");
                    let reason = if reason_input.is_empty() { None } else { Some(reason_input) };

                    sqlx::query("UPDATE bookings SET status = 'cancelled' WHERE id = ?")
                        .bind(&full_id)
                        .execute(pool)
                        .await?;
                    println!("{} Booking {} cancelled.", "✓".green(), &full_id[..8]);

                    // Send cancellation emails
                    if let Some(smtp_config) = crate::email::load_smtp_config(pool).await? {
                        let host: Option<(String, String)> = sqlx::query_as(
                            "SELECT a.name, a.email FROM accounts a
                             JOIN event_types et ON et.account_id = a.id
                             JOIN bookings b ON b.event_type_id = et.id
                             WHERE b.id = ?",
                        )
                        .bind(&full_id)
                        .fetch_optional(pool)
                        .await?;

                        if let Some((host_name, host_email)) = host {
                            let date = if start_at.len() >= 10 { &start_at[..10] } else { &start_at };
                            let start_time = if start_at.len() >= 16 { &start_at[11..16] } else { "00:00" };
                            let end_time = if end_at.len() >= 16 { &end_at[11..16] } else { "00:00" };

                            let details = crate::email::CancellationDetails {
                                event_title,
                                date: date.to_string(),
                                start_time: start_time.to_string(),
                                end_time: end_time.to_string(),
                                guest_name: guest_name.clone(),
                                guest_email: guest_email.clone(),
                                host_name,
                                host_email,
                                uid,
                                reason,
                            };

                            print!("  {} Sending cancellation to {}… ", "…".dimmed(), guest_email);
                            io::stdout().flush().unwrap();
                            match crate::email::send_guest_cancellation(&smtp_config, &details).await {
                                Ok(_) => println!("{}", "sent".green()),
                                Err(e) => println!("{} {}", "failed:".red(), e),
                            }

                            print!("  {} Sending cancellation to {}… ", "…".dimmed(), details.host_email);
                            io::stdout().flush().unwrap();
                            match crate::email::send_host_cancellation(&smtp_config, &details).await {
                                Ok(_) => println!("{}", "sent".green()),
                                Err(e) => println!("{} {}", "failed:".red(), e),
                            }
                        }
                    }
                }
                None => {
                    println!(
                        "{} No confirmed booking found matching '{}'",
                        "✗".red(),
                        id
                    );
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
        return Some(d.and_hms_opt(0, 0, 0)?);
    }
    if let Ok(d) = NaiveDate::parse_from_str(s, "%Y-%m-%d") {
        return Some(d.and_hms_opt(0, 0, 0)?);
    }
    None
}
