//! Smoke test for the EWS provider against a real Exchange server.
//! Run with credentials via env vars to avoid leaking them in shell history:
//!   EWS_URL=https://mail.example.com/EWS/Exchange.asmx \
//!   EWS_USER=alice@example.com \
//!   EWS_PASS=...                                       \
//!   cargo run --example ews_smoke

use anyhow::Result;
use calrs::ews::autodiscover;
use calrs::ews::EwsProvider;
use calrs::providers::CalendarProvider;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "calrs=debug,reqwest=info".into()),
        )
        .init();

    let url = std::env::var("EWS_URL").ok();
    let email = std::env::var("EWS_EMAIL").ok();
    let user = std::env::var("EWS_USER").expect("set EWS_USER");
    let pass = std::env::var("EWS_PASS").expect("set EWS_PASS");

    // --- 1. Resolve EWS endpoint -----------------------------------------
    let endpoint = match url {
        Some(u) => {
            println!("[1] Using URL from EWS_URL: {}", u);
            u
        }
        None => {
            let email = email.expect("set EWS_URL or EWS_EMAIL for autodiscover");
            println!("[1] Running autodiscover for {}", email);
            autodiscover::discover_ews_url(&email, &pass).await?
        }
    };
    println!("    endpoint = {}", endpoint);

    let provider = EwsProvider::new(&endpoint, &user, &pass);

    // --- 2. check_connection ---------------------------------------------
    print!("[2] check_connection()… ");
    match provider.check_connection().await {
        Ok(true) => println!("OK (calendar features advertised)"),
        Ok(false) => println!("connected, features uncertain"),
        Err(e) => {
            println!("FAILED: {:#}", e);
            return Err(e);
        }
    }

    // --- 3. list_calendars -----------------------------------------------
    println!("[3] list_calendars()…");
    let calendars = provider.list_calendars().await?;
    println!("    {} calendar(s) discovered", calendars.len());
    for c in &calendars {
        println!(
            "    - {} (id={}…)",
            c.display_name.as_deref().unwrap_or("(unnamed)"),
            &c.id[..c.id.len().min(40)]
        );
    }
    if calendars.is_empty() {
        println!("    (no calendars — stopping here)");
        return Ok(());
    }

    // --- 4. fetch_events_since (last 7 days) -----------------------------
    let target = &calendars[0];
    let since = (chrono::Utc::now() - chrono::Duration::days(7)).to_rfc3339();
    println!(
        "[4] fetch_events_since(target={}, since={})…",
        target.display_name.as_deref().unwrap_or(&target.id),
        since
    );
    let events = provider.fetch_events_since(&target.id, &since).await?;
    println!("    {} event(s) returned in the window", events.len());
    for ev in events.iter().take(3) {
        let preview = ev.ical.lines().take(8).collect::<Vec<_>>().join(" | ");
        println!("    - {}…", preview.chars().take(140).collect::<String>());
    }

    println!("\nSmoke test PASSED.");
    Ok(())
}
