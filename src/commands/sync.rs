use anyhow::Result;
use colored::Colorize;
use sqlx::SqlitePool;
use uuid::Uuid;

use crate::caldav::CaldavClient;

pub async fn run(pool: &SqlitePool, _full: bool) -> Result<()> {
    let sources: Vec<(String, String, String, String, String)> = sqlx::query_as(
        "SELECT id, name, url, username, password_enc FROM caldav_sources WHERE enabled = 1",
    )
    .fetch_all(pool)
    .await?;

    if sources.is_empty() {
        println!("No sources configured. Add one with `calrs source add`.");
        return Ok(());
    }

    for (source_id, name, url, username, password_hex) in &sources {
        println!("{} Syncing '{}'…", "…".dimmed(), name);

        let password_bytes = hex::decode(password_hex)?;
        let password = String::from_utf8(password_bytes)?;

        let client = CaldavClient::new(url, username, &password);

        // Discover principal → calendar-home-set → calendars
        let principal = match client.discover_principal().await {
            Ok(p) => p,
            Err(e) => {
                println!("  {} Could not discover principal: {}", "✗".red(), e);
                continue;
            }
        };

        let calendar_home = match client.discover_calendar_home(&principal).await {
            Ok(h) => h,
            Err(e) => {
                println!("  {} Could not discover calendar home: {}", "✗".red(), e);
                continue;
            }
        };

        let calendars = match client.list_calendars(&calendar_home).await {
            Ok(c) => c,
            Err(e) => {
                println!("  {} Could not list calendars: {}", "✗".red(), e);
                continue;
            }
        };

        println!("  Found {} calendar(s)", calendars.len());

        for cal_info in &calendars {
            // Upsert calendar
            let cal_id: String = match sqlx::query_scalar::<_, String>(
                "SELECT id FROM calendars WHERE source_id = ? AND href = ?",
            )
            .bind(source_id)
            .bind(&cal_info.href)
            .fetch_optional(pool)
            .await?
            {
                Some(id) => id,
                None => {
                    let id = Uuid::new_v4().to_string();
                    sqlx::query(
                        "INSERT INTO calendars (id, source_id, href, display_name, color, ctag) VALUES (?, ?, ?, ?, ?, ?)",
                    )
                    .bind(&id)
                    .bind(source_id)
                    .bind(&cal_info.href)
                    .bind(&cal_info.display_name)
                    .bind(&cal_info.color)
                    .bind(&cal_info.ctag)
                    .execute(pool)
                    .await?;
                    id
                }
            };

            let display = cal_info
                .display_name
                .as_deref()
                .unwrap_or(&cal_info.href);

            // Fetch events
            match client.fetch_events(&cal_info.href).await {
                Ok(raw_events) => {
                    let mut count = 0;
                    for raw in &raw_events {
                        let uid = extract_ical_field(&raw.ical_data, "UID")
                            .unwrap_or_else(|| Uuid::new_v4().to_string());
                        let summary = extract_ical_field(&raw.ical_data, "SUMMARY");
                        let start_at = extract_ical_field(&raw.ical_data, "DTSTART")
                            .unwrap_or_default();
                        let end_at = extract_ical_field(&raw.ical_data, "DTEND")
                            .unwrap_or_default();
                        let location = extract_ical_field(&raw.ical_data, "LOCATION");
                        let description = extract_ical_field(&raw.ical_data, "DESCRIPTION");
                        let status = extract_ical_field(&raw.ical_data, "STATUS");
                        let rrule = extract_ical_field(&raw.ical_data, "RRULE");

                        let event_id = Uuid::new_v4().to_string();

                        sqlx::query(
                            "INSERT INTO events (id, calendar_id, uid, summary, start_at, end_at, location, description, status, rrule, raw_ical)
                             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                             ON CONFLICT(uid) DO UPDATE SET
                               summary = excluded.summary,
                               start_at = excluded.start_at,
                               end_at = excluded.end_at,
                               location = excluded.location,
                               description = excluded.description,
                               status = excluded.status,
                               rrule = excluded.rrule,
                               raw_ical = excluded.raw_ical,
                               synced_at = datetime('now')",
                        )
                        .bind(&event_id)
                        .bind(&cal_id)
                        .bind(&uid)
                        .bind(&summary)
                        .bind(&start_at)
                        .bind(&end_at)
                        .bind(&location)
                        .bind(&description)
                        .bind(&status)
                        .bind(&rrule)
                        .bind(&raw.ical_data)
                        .execute(pool)
                        .await?;

                        count += 1;
                    }
                    println!(
                        "  {} {} — {} event(s) synced",
                        "✓".green(),
                        display,
                        count
                    );
                }
                Err(e) => {
                    println!("  {} {} — failed: {}", "✗".red(), display, e);
                }
            }
        }

        // Update last_synced
        sqlx::query("UPDATE caldav_sources SET last_synced = datetime('now') WHERE id = ?")
            .bind(source_id)
            .execute(pool)
            .await?;
    }

    println!("{} Sync complete.", "✓".green());
    Ok(())
}

/// Extract a field from the VEVENT block only (ignores VTIMEZONE etc.)
fn extract_ical_field(ical: &str, field: &str) -> Option<String> {
    // Find the VEVENT block to avoid matching fields in VTIMEZONE etc.
    let vevent_start = ical.find("BEGIN:VEVENT")?;
    let vevent_end = ical[vevent_start..].find("END:VEVENT")
        .map(|i| vevent_start + i)
        .unwrap_or(ical.len());
    let vevent = &ical[vevent_start..vevent_end];

    for line in vevent.lines() {
        if line.starts_with(field) {
            if let Some(colon_pos) = line.find(':') {
                let value = line[colon_pos + 1..].trim().to_string();
                if !value.is_empty() {
                    return Some(value);
                }
            }
        }
    }
    None
}
