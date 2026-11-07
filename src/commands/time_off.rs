//! `calrs user time-off`: a user's time off from the command line (#46), the
//! same periods as the dashboard's Time off page (`crate::time_off`).

use anyhow::{bail, Result};
use chrono::Utc;
use chrono_tz::Tz;
use clap::Subcommand;
use colored::Colorize;
use sqlx::SqlitePool;
use tabled::{Table, Tabled};

#[derive(Debug, Subcommand)]
pub enum TimeOffCommands {
    /// Add a period the user takes no bookings, on every event type they host
    Add {
        /// User email
        email: String,
        /// First day, YYYY-MM-DD, or a start time as YYYY-MM-DDTHH:MM
        #[arg(long)]
        from: String,
        /// Last day (included), YYYY-MM-DD, or an end time as
        /// YYYY-MM-DDTHH:MM. Defaults to the first day.
        #[arg(long)]
        to: Option<String>,
        /// Private note, shown only to the user
        #[arg(long)]
        label: Option<String>,
        /// Zone the dates are in (defaults to the user's profile time zone)
        #[arg(long)]
        timezone: Option<String>,
    },
    /// List the user's time off that has not ended yet
    List {
        /// User email
        email: String,
    },
    /// Delete one period by its id (from `list`)
    Remove {
        /// User email
        email: String,
        /// Period id
        id: String,
    },
}

/// "2026-12-21" or "2026-12-21T14:00" (a space works too) into a date and a
/// time, the time empty when not given.
fn split_when(value: &str) -> (&str, &str) {
    match value.trim().split_once(['T', ' ']) {
        Some((date, time)) => (date, time),
        None => (value.trim(), ""),
    }
}

async fn user_id_and_tz(pool: &SqlitePool, email: &str) -> Result<(String, String)> {
    let row: Option<(String, String)> =
        sqlx::query_as("SELECT id, timezone FROM users WHERE email = ?")
            .bind(email)
            .fetch_optional(pool)
            .await?;
    match row {
        Some(row) => Ok(row),
        None => bail!("No user with email {email}"),
    }
}

pub async fn run(pool: &SqlitePool, cmd: TimeOffCommands) -> Result<()> {
    match cmd {
        TimeOffCommands::Add {
            email,
            from,
            to,
            label,
            timezone,
        } => {
            let (user_id, profile_tz) = user_id_and_tz(pool, &email).await?;
            let tz_name = timezone.unwrap_or(profile_tz);
            let Ok(tz) = tz_name.parse::<Tz>() else {
                bail!("Unknown time zone '{tz_name}'");
            };
            let (start_date, start_time) = split_when(&from);
            let (end_date, end_time) = to.as_deref().map(split_when).unwrap_or(("", ""));
            let label = crate::time_off::normalize_label(label.as_deref().unwrap_or(""))
                .map_err(|e| anyhow::anyhow!(e.message()))?;
            let (start, end) = crate::time_off::parse_period(
                start_date,
                start_time,
                end_date,
                end_time,
                tz,
                Utc::now(),
            )
            .map_err(|e| anyhow::anyhow!(e.message()))?;
            let id = crate::time_off::add(pool, &user_id, start, end, tz.name(), label.as_deref())
                .await?;
            println!("{} Time off added for {} ({})", "✓".green(), email, id);
            println!(
                "{}",
                "Bookings already in this period are kept. Cancel them with `calrs booking cancel` if needed."
                    .dimmed()
            );
        }
        TimeOffCommands::List { email } => {
            let (user_id, _) = user_id_and_tz(pool, &email).await?;
            let periods = crate::time_off::upcoming(pool, &user_id, Utc::now()).await?;
            if periods.is_empty() {
                println!("No time off planned for {email}.");
                return Ok(());
            }
            #[derive(Tabled)]
            struct Row {
                id: String,
                from: String,
                to: String,
                timezone: String,
                note: String,
            }
            let rows: Vec<Row> = periods
                .iter()
                .map(|p| Row {
                    id: p.id.clone(),
                    from: p.local_start().format("%Y-%m-%d %H:%M").to_string(),
                    to: p.local_end().format("%Y-%m-%d %H:%M").to_string(),
                    timezone: p.timezone.clone(),
                    note: p.label.clone().unwrap_or_default(),
                })
                .collect();
            println!("{}", Table::new(rows));
            println!("{}", "End times are exclusive.".dimmed());
        }
        TimeOffCommands::Remove { email, id } => {
            let (user_id, _) = user_id_and_tz(pool, &email).await?;
            if crate::time_off::delete(pool, &user_id, &id).await? {
                println!("{} Time off removed", "✓".green());
            } else {
                bail!("No time off {id} for {email}");
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates_and_times_split() {
        assert_eq!(split_when("2026-12-21"), ("2026-12-21", ""));
        assert_eq!(split_when("2026-12-21T14:00"), ("2026-12-21", "14:00"));
        assert_eq!(split_when(" 2026-12-21 14:00 "), ("2026-12-21", "14:00"));
    }
}
