use anyhow::Result;
use chrono::{Duration, Utc};
use clap::Subcommand;
use colored::Colorize;

use crate::caldav::CaldavClient;
use crate::utils::{extract_vevent_field, parse_ical_datetime, prompt_password, split_vevents};

#[derive(Subcommand)]
pub enum ResourceCommands {
    /// Probe a resource calendar URL (BlueMind ICS publish feed or CalDAV collection)
    Probe {
        /// Resource calendar URL (ICS publish URL or CalDAV collection URL)
        #[arg(long)]
        url: String,
        /// Username for authenticated CalDAV access (password prompted)
        #[arg(long)]
        username: Option<String>,
        /// Write test: PUT a temporary event, verify it exists, then delete it
        #[arg(long)]
        write_test: bool,
    },
}

pub async fn run(cmd: ResourceCommands) -> Result<()> {
    match cmd {
        ResourceCommands::Probe {
            url,
            username,
            write_test,
        } => probe(&url, username.as_deref(), write_test).await,
    }
}

async fn probe(url: &str, username: Option<&str>, write_test: bool) -> Result<()> {
    let password = match username {
        Some(_) => prompt_password("Password: "),
        None => String::new(),
    };
    let username = username.unwrap_or("");

    println!("Probing {}", url.bold());
    println!();

    // Step 1: plain GET. BlueMind "publish" URLs serve a raw ICS feed, which is
    // the cheapest possible read path (no auth, no WebDAV). A CalDAV collection
    // URL will typically answer with an error or HTML here, so falling through
    // to the CalDAV probe on anything that isn't a VCALENDAR body is safe.
    match fetch_ics_feed(url, username, &password).await {
        Ok(Some(body)) => {
            println!("{} ICS feed detected (publish URL)", "✓".green());
            report_ics_feed(&body);
            println!();
            println!(
                "{} An ICS publish feed is {}. It is enough for availability checks,",
                "ℹ".blue(),
                "read-only".bold()
            );
            println!("  but reserving the resource (write-back) needs its CalDAV URL.");
            if write_test {
                println!(
                    "{} --write-test skipped: cannot write to a publish feed",
                    "✗".red()
                );
            }
            return Ok(());
        }
        Ok(None) => {
            println!("{} Not an ICS feed, trying CalDAV…", "…".dimmed());
        }
        Err(e) => {
            println!("{} GET failed ({}), trying CalDAV…", "…".dimmed(), e);
        }
    }

    let client = CaldavClient::new(url, username, &password);

    match client.check_connection().await {
        Ok(true) => println!("{} Server advertises CalDAV support", "✓".green()),
        Ok(false) => println!(
            "{} Server reachable but does not advertise calendar-access",
            "!".yellow()
        ),
        Err(e) => println!("{} OPTIONS probe failed: {}", "✗".red(), e),
    }

    // Depth:1 PROPFIND on the URL. If it is the calendar collection itself, the
    // multistatus includes it and we learn its display name. If it is a
    // calendar home, we learn the calendars underneath (useful to discover the
    // exact collection href of a BlueMind resource).
    match client.list_calendars(url).await {
        Ok(cals) if !cals.is_empty() => {
            println!(
                "{} Found {} calendar collection(s):",
                "✓".green(),
                cals.len()
            );
            for cal in &cals {
                println!(
                    "    {} {}",
                    cal.display_name.as_deref().unwrap_or("(no name)").bold(),
                    cal.href.dimmed()
                );
            }
        }
        Ok(_) => println!(
            "{} PROPFIND ok but no calendar collection found at this URL",
            "!".yellow()
        ),
        Err(e) => println!("{} PROPFIND failed: {}", "✗".red(), e),
    }

    match client.fetch_events(url).await {
        Ok(events) => {
            println!(
                "{} REPORT ok: {} event resource(s)",
                "✓".green(),
                events.len()
            );
            let mut parsed: Vec<(chrono::NaiveDateTime, String)> = events
                .iter()
                .flat_map(|e| split_vevents(&e.ical_data))
                .filter_map(|v| {
                    let start = parse_ical_datetime(&extract_vevent_field(&v, "DTSTART")?)?;
                    let summary =
                        extract_vevent_field(&v, "SUMMARY").unwrap_or_else(|| "(no title)".into());
                    Some((start, summary))
                })
                .collect();
            parsed.sort();
            let now = Utc::now().naive_utc();
            for (start, summary) in parsed.iter().filter(|(s, _)| *s >= now).take(5) {
                println!("    {} {}", start.format("%Y-%m-%d %H:%M"), summary);
            }
        }
        Err(e) => println!("{} REPORT failed: {}", "✗".red(), e),
    }

    if write_test {
        println!();
        run_write_test(&client, url).await;
    }

    Ok(())
}

/// GET the URL and return the body if it looks like an ICS feed.
async fn fetch_ics_feed(url: &str, username: &str, password: &str) -> Result<Option<String>> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()?;
    let mut req = client.get(url);
    if !username.is_empty() {
        req = req.basic_auth(username, Some(password));
    }
    let resp = req.send().await?;
    if !resp.status().is_success() {
        anyhow::bail!("HTTP {}", resp.status());
    }
    // BlueMind publish feeds answer text/calendar with an EMPTY body when the
    // calendar has no events, so the content-type is the reliable signal and
    // the VCALENDAR sniff is only a fallback for servers that mislabel.
    let is_calendar = resp
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .map(|ct| ct.contains("text/calendar"))
        .unwrap_or(false);
    let body = resp.text().await?;
    if is_calendar || body.contains("BEGIN:VCALENDAR") {
        Ok(Some(body))
    } else {
        Ok(None)
    }
}

fn report_ics_feed(body: &str) {
    if body.trim().is_empty() {
        println!("    Feed is valid but currently empty (no events published yet)");
        return;
    }
    if let Some(name) = body
        .lines()
        .find(|l| l.starts_with("X-WR-CALNAME"))
        .and_then(|l| l.split_once(':').map(|(_, v)| v.trim().to_string()))
    {
        println!("    Calendar name: {}", name.bold());
    } else {
        println!(
            "    {} No X-WR-CALNAME in feed, name must be set manually",
            "!".yellow()
        );
    }
    let vevents: Vec<String> = if body.contains("BEGIN:VEVENT") {
        split_vevents(body)
    } else {
        Vec::new()
    };
    println!("    {} VEVENT(s) in feed", vevents.len());
    let mut parsed: Vec<(chrono::NaiveDateTime, String)> = vevents
        .iter()
        .filter_map(|v| {
            let start = parse_ical_datetime(&extract_vevent_field(v, "DTSTART")?)?;
            let summary = extract_vevent_field(v, "SUMMARY").unwrap_or_else(|| "(no title)".into());
            Some((start, summary))
        })
        .collect();
    parsed.sort();
    let now = Utc::now().naive_utc();
    for (start, summary) in parsed.iter().filter(|(s, _)| *s >= now).take(5) {
        println!("    {} {}", start.format("%Y-%m-%d %H:%M"), summary);
    }
}

/// PUT a temporary event, verify it exists, delete it, verify it is gone.
/// The event is scheduled ~24h from now so it cannot collide with a live
/// booking, and its summary makes it obvious it is safe to remove by hand
/// if cleanup fails.
async fn run_write_test(client: &CaldavClient, url: &str) {
    let uid = format!("calrs-probe-{}", uuid::Uuid::new_v4());
    let start = Utc::now() + Duration::hours(24);
    let end = start + Duration::minutes(15);
    let ics = format!(
        "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//calrs//probe//EN\r\nBEGIN:VEVENT\r\nUID:{}\r\nDTSTAMP:{}\r\nDTSTART:{}\r\nDTEND:{}\r\nSUMMARY:calrs write probe (safe to delete)\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n",
        uid,
        Utc::now().format("%Y%m%dT%H%M%SZ"),
        start.format("%Y%m%dT%H%M%SZ"),
        end.format("%Y%m%dT%H%M%SZ"),
    );

    println!("Write test (temporary event, uid {})", uid.dimmed());

    match client.put_event(url, &uid, &ics).await {
        Ok(()) => println!("{} PUT accepted", "✓".green()),
        Err(e) => {
            println!("{} PUT failed: {}", "✗".red(), e);
            return;
        }
    }

    match client.event_exists(url, &uid).await {
        Ok(true) => println!("{} Event visible on server after PUT", "✓".green()),
        Ok(false) => println!(
            "{} Server accepted the PUT but the event is not there (silently dropped?)",
            "✗".red()
        ),
        Err(e) => println!("{} Could not verify event: {}", "!".yellow(), e),
    }

    match client.delete_event(url, &uid).await {
        Ok(()) => println!("{} DELETE accepted", "✓".green()),
        Err(e) => {
            println!("{} DELETE failed: {}", "✗".red(), e);
            println!(
                "{} Leftover test event {} must be removed manually!",
                "!".yellow(),
                uid.bold()
            );
            return;
        }
    }

    match client.event_exists(url, &uid).await {
        Ok(false) => println!(
            "{} Event gone after DELETE, write path fully works",
            "✓".green()
        ),
        Ok(true) => println!("{} Event still present after DELETE", "✗".red()),
        Err(e) => println!("{} Could not verify deletion: {}", "!".yellow(), e),
    }
}
