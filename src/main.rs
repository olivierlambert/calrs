// Domain model structs kept for documentation/future typed queries,
// RawEvent.href for future delta sync, cleanup_expired_sessions for future scheduled task.
#![allow(dead_code)]
// Complex tuple types from sqlx::query_as — will migrate to typed queries.
#![allow(clippy::type_complexity)]
// Slot computation functions have many parameters — will refactor into option structs.
#![allow(clippy::too_many_arguments)]

mod auth;
mod caldav;
mod commands;
mod crypto;
mod db;
mod email;
mod models;
mod rrule;
mod utils;
mod web;

use anyhow::Result;
use clap::{Parser, Subcommand};
use directories::ProjectDirs;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "calrs", about = "Fast, self-hostable scheduling", version)]
struct Cli {
    /// Custom data directory
    #[arg(long, env = "CALRS_DATA_DIR", global = true)]
    data_dir: Option<PathBuf>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Manage CalDAV sources
    Source {
        #[command(subcommand)]
        command: commands::source::SourceCommands,
    },
    /// Pull latest events from CalDAV
    Sync {
        /// Full re-sync (ignore sync tokens)
        #[arg(long)]
        full: bool,
    },
    /// View your calendar
    Calendar {
        #[command(subcommand)]
        command: CalendarCommands,
    },
    /// Manage bookable event types
    EventType {
        #[command(subcommand)]
        command: commands::event_type::EventTypeCommands,
    },
    /// Manage bookings
    Booking {
        #[command(subcommand)]
        command: commands::booking::BookingCommands,
    },
    /// Manage users
    User {
        #[command(subcommand)]
        command: commands::user::UserCommands,
    },
    /// Configure calrs settings (SMTP, auth, etc.)
    Config {
        #[command(subcommand)]
        command: commands::config::ConfigCommands,
    },
    /// Start the web booking server
    Serve {
        /// Port to listen on
        #[arg(long, default_value = "3000")]
        port: u16,
        /// Address to bind to (use 0.0.0.0 to listen on all interfaces)
        #[arg(long, default_value = "127.0.0.1")]
        host: std::net::IpAddr,
    },
}

#[derive(Subcommand)]
enum CalendarCommands {
    /// Show events
    Show {
        /// Start date (YYYY-MM-DD)
        #[arg(long)]
        from: Option<String>,
        /// End date (YYYY-MM-DD)
        #[arg(long)]
        to: Option<String>,
    },
}

fn get_data_dir(custom: Option<PathBuf>) -> Result<PathBuf> {
    if let Some(dir) = custom {
        return Ok(dir);
    }
    let proj = ProjectDirs::from("", "", "calrs")
        .ok_or_else(|| anyhow::anyhow!("Could not determine data directory"))?;
    Ok(proj.data_dir().to_path_buf())
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let data_dir = get_data_dir(cli.data_dir)?;
    let pool = db::connect(&data_dir).await?;
    db::migrate(&pool).await?;
    let secret_key = crypto::load_or_create_key(&data_dir)?;
    db::migrate_passwords(&pool, &secret_key).await?;

    match cli.command {
        Commands::Source { command } => commands::source::run(&pool, &secret_key, command).await?,
        Commands::Sync { full } => commands::sync::run(&pool, &secret_key, full).await?,
        Commands::Calendar { command } => match command {
            CalendarCommands::Show { from, to } => commands::calendar::run(&pool, from, to).await?,
        },
        Commands::EventType { command } => commands::event_type::run(&pool, command).await?,
        Commands::Booking { command } => {
            commands::booking::run(&pool, &secret_key, command).await?
        }
        Commands::User { command } => commands::user::run(&pool, command).await?,
        Commands::Config { command } => commands::config::run(&pool, &secret_key, command).await?,
        Commands::Serve { port, host } => {
            let router = web::create_router(pool, data_dir, secret_key);
            let addr = std::net::SocketAddr::from((host, port));
            println!("Booking page running at http://{}:{}", host, port);
            let listener = tokio::net::TcpListener::bind(addr).await?;
            axum::serve(listener, router).await?;
        }
    }

    Ok(())
}
