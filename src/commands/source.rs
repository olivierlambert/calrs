use anyhow::{bail, Result};
use clap::Subcommand;
use colored::Colorize;
use sqlx::SqlitePool;
use tabled::{Table, Tabled};
use uuid::Uuid;

use crate::caldav::CaldavClient;

use std::io::{self, Write};

use crate::utils::prompt;

#[derive(Debug, Subcommand)]
pub enum SourceCommands {
    /// Connect a CalDAV calendar
    Add {
        /// CalDAV server URL
        #[arg(long)]
        url: Option<String>,
        /// Username
        #[arg(long)]
        username: Option<String>,
        /// Display name for this source
        #[arg(long)]
        name: Option<String>,
        /// Skip the connection test
        #[arg(long)]
        no_test: bool,
    },
    /// List connected sources
    List,
    /// Remove a source
    Remove {
        /// Source ID
        id: String,
    },
    /// Test a CalDAV connection
    Test {
        /// Source ID
        id: String,
    },
    /// Update a source's connection details
    Update {
        /// Source ID (or unique prefix)
        id: String,
        /// New display name
        #[arg(long)]
        name: Option<String>,
        /// New CalDAV URL
        #[arg(long)]
        url: Option<String>,
        /// New username
        #[arg(long)]
        username: Option<String>,
        /// Prompt for a new password (use this for scripted password rotation)
        #[arg(long)]
        password: bool,
    },
}

#[derive(Tabled)]
struct SourceRow {
    #[tabled(rename = "ID")]
    id: String,
    #[tabled(rename = "Name")]
    name: String,
    #[tabled(rename = "URL")]
    url: String,
    #[tabled(rename = "Username")]
    username: String,
    #[tabled(rename = "Last Synced")]
    last_synced: String,
}

pub async fn run(pool: &SqlitePool, key: &[u8; 32], cmd: SourceCommands) -> Result<()> {
    match cmd {
        SourceCommands::Add {
            url,
            username,
            name,
            no_test,
        } => {
            let account: (String,) = sqlx::query_as("SELECT id FROM accounts LIMIT 1")
                .fetch_optional(pool)
                .await?
                .ok_or_else(|| anyhow::anyhow!("No account found. Run `calrs init` first."))?;

            let url = url.unwrap_or_else(|| prompt("CalDAV URL"));
            let username = username.unwrap_or_else(|| prompt("Username"));
            let name = name.unwrap_or_else(|| prompt("Display name"));
            let password = rpassword::prompt_password("Password: ").unwrap_or_default();

            // Test connection
            if !no_test {
                print!("{} Testing connection… ", "…".dimmed());
                io::stdout().flush().unwrap();

                let client = CaldavClient::new(&url, &username, &password);
                match client.check_connection().await {
                    Ok(true) => println!("{}", "CalDAV supported".green()),
                    Ok(false) => {
                        println!(
                            "{}",
                            "No CalDAV support detected (missing calendar-access in DAV header)"
                                .yellow()
                        );
                        println!("Continuing anyway…");
                    }
                    Err(e) => {
                        println!("{} {}", "✗".red(), e);
                        bail!("Connection failed: {}", e);
                    }
                }
            }

            let id = Uuid::new_v4().to_string();
            let password_enc = crate::crypto::encrypt_password(key, &password)?;

            sqlx::query(
                "INSERT INTO caldav_sources (id, account_id, name, url, username, password_enc) VALUES (?, ?, ?, ?, ?, ?)",
            )
            .bind(&id)
            .bind(&account.0)
            .bind(&name)
            .bind(&url)
            .bind(&username)
            .bind(&password_enc)
            .execute(pool)
            .await?;

            println!("{} Source '{}' added (id: {})", "✓".green(), name, &id[..8]);
        }
        SourceCommands::List => {
            let sources: Vec<(String, String, String, String, Option<String>)> = sqlx::query_as(
                "SELECT id, name, url, username, last_synced FROM caldav_sources ORDER BY created_at",
            )
            .fetch_all(pool)
            .await?;

            if sources.is_empty() {
                println!("No sources configured. Add one with `calrs source add`.");
                return Ok(());
            }

            let rows: Vec<SourceRow> = sources
                .into_iter()
                .map(|(id, name, url, username, last_synced)| SourceRow {
                    id: id[..8].to_string(),
                    name,
                    url,
                    username,
                    last_synced: last_synced.unwrap_or_else(|| "never".to_string()),
                })
                .collect();

            println!("{}", Table::new(rows));
        }
        SourceCommands::Remove { id } => {
            let full_id: Option<(String,)> =
                sqlx::query_as("SELECT id FROM caldav_sources WHERE id LIKE ? || '%'")
                    .bind(&id)
                    .fetch_optional(pool)
                    .await?;

            match full_id {
                Some((full_id,)) => {
                    // CASCADE handles events and calendars
                    sqlx::query("DELETE FROM caldav_sources WHERE id = ?")
                        .bind(&full_id)
                        .execute(pool)
                        .await?;
                    println!("{} Source removed.", "✓".green());
                }
                None => {
                    println!("{} No source found matching '{}'", "✗".red(), id);
                }
            }
        }
        SourceCommands::Update {
            id,
            name,
            url,
            username,
            password,
        } => {
            let existing: Option<(String, String, String, String, String)> = sqlx::query_as(
                "SELECT id, name, url, username, password_enc FROM caldav_sources WHERE id LIKE ? || '%'",
            )
            .bind(&id)
            .fetch_optional(pool)
            .await?;

            let (full_id, current_name, current_url, current_username, current_password_enc) =
                match existing {
                    Some(t) => t,
                    None => {
                        println!("{} No source found matching '{}'", "✗".red(), id);
                        return Ok(());
                    }
                };

            let url_or_username_changed = url.is_some() || username.is_some();
            let new_name = name.unwrap_or(current_name);
            let new_url = url.unwrap_or(current_url);
            let new_username = username.unwrap_or(current_username);

            if password {
                let new_pw = rpassword::prompt_password("New password: ").unwrap_or_default();
                if new_pw.is_empty() {
                    bail!("Password is required when --password is set");
                }
                let new_enc = crate::crypto::encrypt_password(key, &new_pw)?;
                sqlx::query(
                    "UPDATE caldav_sources SET name = ?, url = ?, username = ?, password_enc = ? WHERE id = ?",
                )
                .bind(&new_name)
                .bind(&new_url)
                .bind(&new_username)
                .bind(&new_enc)
                .bind(&full_id)
                .execute(pool)
                .await?;
            } else {
                let _ = current_password_enc;
                sqlx::query(
                    "UPDATE caldav_sources SET name = ?, url = ?, username = ? WHERE id = ?",
                )
                .bind(&new_name)
                .bind(&new_url)
                .bind(&new_username)
                .bind(&full_id)
                .execute(pool)
                .await?;
            }

            println!("{} Source updated: {}", "✓".green(), new_name);

            if url_or_username_changed {
                println!(
                    "{}",
                    "  URL or username changed — run `calrs sync` to refresh the calendar list."
                        .dimmed()
                );
            }
        }
        SourceCommands::Test { id } => {
            let source: Option<(String, String, String, String)> = sqlx::query_as(
                "SELECT url, username, password_enc, name FROM caldav_sources WHERE id LIKE ? || '%'",
            )
            .bind(&id)
            .fetch_optional(pool)
            .await?;

            match source {
                Some((url, username, password_enc, name)) => {
                    let password = crate::crypto::decrypt_password(key, &password_enc)?;

                    println!("Testing source '{}'…", name);
                    let client = CaldavClient::new(&url, &username, &password);
                    match client.check_connection().await {
                        Ok(true) => println!("{} Connection OK — CalDAV supported", "✓".green()),
                        Ok(false) => println!("{} Connected but CalDAV not detected", "⚠".yellow()),
                        Err(e) => println!("{} Connection failed: {}", "✗".red(), e),
                    }
                }
                None => {
                    println!("{} No source found matching '{}'", "✗".red(), id);
                }
            }
        }
    }

    Ok(())
}
