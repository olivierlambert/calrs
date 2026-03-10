use anyhow::Result;
use chrono::{Local, NaiveDate};
use colored::Colorize;
use sqlx::SqlitePool;
use tabled::{Table, Tabled};

#[derive(Tabled)]
struct EventRow {
    #[tabled(rename = "Date")]
    date: String,
    #[tabled(rename = "Time")]
    time: String,
    #[tabled(rename = "Summary")]
    summary: String,
    #[tabled(rename = "Calendar")]
    calendar: String,
}

pub async fn run(pool: &SqlitePool, from: Option<String>, to: Option<String>) -> Result<()> {
    let from_date = from
        .map(|s| NaiveDate::parse_from_str(&s, "%Y-%m-%d"))
        .transpose()?
        .unwrap_or_else(|| Local::now().date_naive());

    let to_date = to
        .map(|s| NaiveDate::parse_from_str(&s, "%Y-%m-%d"))
        .transpose()?
        .unwrap_or_else(|| from_date + chrono::Duration::days(14));

    // Support both YYYYMMDD (all-day) and YYYY-MM-DDTHH:MM:SS formats
    let from_compact = from_date.format("%Y%m%d").to_string();
    let to_compact = to_date.format("%Y%m%d").to_string();
    let from_iso = from_date.format("%Y-%m-%d").to_string();
    let to_iso = to_date.format("%Y-%m-%d").to_string();

    let events: Vec<(String, String, Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT e.start_at, e.end_at, e.summary, c.display_name
         FROM events e
         JOIN calendars c ON e.calendar_id = c.id
         WHERE (e.start_at >= ? AND e.start_at <= ?)
            OR (e.start_at >= ? AND e.start_at <= ?)
         ORDER BY e.start_at",
    )
    .bind(&from_compact)
    .bind(&to_compact)
    .bind(&from_iso)
    .bind(format!("{}T23:59:59", to_iso))
    .fetch_all(pool)
    .await?;

    if events.is_empty() {
        println!("No events from {} to {}.", from_iso, to_iso);
        return Ok(());
    }

    let rows: Vec<EventRow> = events
        .into_iter()
        .map(|(start_at, end_at, summary, cal_name)| {
            let (date, time) = if start_at.contains('T') {
                let parts: Vec<&str> = start_at.splitn(2, 'T').collect();
                (
                    format_date(parts[0]),
                    format!(
                        "{} – {}",
                        format_time(parts.get(1).unwrap_or(&"")),
                        format_time(&extract_time(&end_at))
                    ),
                )
            } else {
                (format_date(&start_at), "all-day".to_string())
            };

            EventRow {
                date,
                time,
                summary: summary.unwrap_or_else(|| "(no title)".to_string()),
                calendar: cal_name.unwrap_or_else(|| "—".to_string()),
            }
        })
        .collect();

    println!(
        "{} events from {} to {}:\n",
        rows.len().to_string().bold(),
        from_iso,
        to_iso
    );
    println!("{}", Table::new(rows));

    Ok(())
}

fn extract_time(dt: &str) -> String {
    if let Some(pos) = dt.find('T') {
        dt[pos + 1..].to_string()
    } else {
        dt.to_string()
    }
}

/// Format YYYYMMDD → YYYY-MM-DD, pass through if already has dashes
fn format_date(d: &str) -> String {
    if d.len() == 8 && !d.contains('-') {
        format!("{}-{}-{}", &d[..4], &d[4..6], &d[6..8])
    } else {
        d.to_string()
    }
}

/// Format HHMMSS → HH:MM, pass through if already has colons
fn format_time(t: &str) -> String {
    let t = t.trim_end_matches('Z');
    if t.len() >= 6 && !t.contains(':') {
        format!("{}:{}", &t[..2], &t[2..4])
    } else if t.len() >= 5 {
        t[..5].to_string()
    } else {
        t.to_string()
    }
}
