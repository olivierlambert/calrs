use anyhow::{bail, Result};
use clap::Subcommand;
use colored::Colorize;
use sqlx::SqlitePool;
use tabled::{Table, Tabled};
use uuid::Uuid;

use crate::providers::{build_provider, factory};

use std::io::{self, Write};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::utils::prompt;

#[derive(Debug, Subcommand)]
pub enum SourceCommands {
    /// Connect a calendar source (CalDAV or Exchange/EWS)
    Add {
        /// Source URL. CalDAV: discovery root. EWS: `Exchange.asmx` endpoint
        /// (auto-discovered when omitted with `--provider ews`).
        #[arg(long)]
        url: Option<String>,
        /// Username
        #[arg(long)]
        username: Option<String>,
        /// Display name for this source
        #[arg(long)]
        name: Option<String>,
        /// Provider type: `caldav` (default) or `ews`.
        #[arg(long, default_value = "caldav")]
        provider: String,
        /// For EWS: email used for Autodiscover when --url is not supplied.
        #[arg(long)]
        email: Option<String>,
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
    /// Test a connection
    Test {
        /// Source ID
        id: String,
    },
    /// Connect a Google Calendar via OAuth2
    AddGoogle {
        /// Display name for this source
        #[arg(long)]
        name: Option<String>,
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
    #[tabled(rename = "Type")]
    provider: String,
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
            provider,
            email,
            no_test,
        } => {
            let provider = provider.trim().to_ascii_lowercase();
            if provider != factory::kinds::CALDAV && provider != factory::kinds::EWS {
                bail!("unknown provider '{}'. Use 'caldav' or 'ews'.", provider);
            }

            let account: (String,) = sqlx::query_as("SELECT id FROM accounts LIMIT 1")
                .fetch_optional(pool)
                .await?
                .ok_or_else(|| anyhow::anyhow!("No account found. Run `calrs init` first."))?;

            let username = username.unwrap_or_else(|| prompt("Username"));
            let name = name.unwrap_or_else(|| prompt("Display name"));
            let password = rpassword::prompt_password("Password: ").unwrap_or_default();

            // Resolve URL: explicit flag wins; otherwise EWS gets a chance to
            // autodiscover from the email; CalDAV always asks the user.
            let url = match url {
                Some(u) => u,
                None => match provider.as_str() {
                    factory::kinds::EWS => {
                        let email_for_disco = email
                            .clone()
                            .unwrap_or_else(|| prompt("Email (for Autodiscover)"));
                        print!(
                            "{} Discovering EWS endpoint via Autodiscover… ",
                            "…".dimmed()
                        );
                        io::stdout().flush().ok();
                        match crate::ews::autodiscover::discover_ews_url(
                            &email_for_disco,
                            &password,
                        )
                        .await
                        {
                            Ok(u) => {
                                println!("{}", u.green());
                                u
                            }
                            Err(e) => {
                                println!("{}", "failed".red());
                                println!(
                                    "  {} Autodiscover failed: {}. Falling back to manual entry.",
                                    "!".yellow(),
                                    e
                                );
                                prompt("EWS Exchange.asmx URL")
                            }
                        }
                    }
                    _ => prompt("CalDAV URL"),
                },
            };

            // Validate URL (HTTPS, no SSRF target).
            factory::validate_url(&provider, &url)?;

            // Test connection unless skipped
            if !no_test {
                print!("{} Testing connection… ", "…".dimmed());
                io::stdout().flush().unwrap();

                let client = build_provider(&provider, &url, &username, &password)?;
                match client.check_connection().await {
                    Ok(true) => println!("{}", "OK".green()),
                    Ok(false) => {
                        println!(
                            "{}",
                            "Connected, but provider features not explicitly advertised".yellow()
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
                "INSERT INTO caldav_sources (id, account_id, name, url, username, password_enc, provider_type) VALUES (?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&id)
            .bind(&account.0)
            .bind(&name)
            .bind(&url)
            .bind(&username)
            .bind(&password_enc)
            .bind(&provider)
            .execute(pool)
            .await?;

            println!(
                "{} Source '{}' ({}) added (id: {})",
                "✓".green(),
                name,
                factory::label(&provider),
                &id[..8]
            );
        }
        SourceCommands::List => {
            let sources: Vec<(String, String, String, String, Option<String>, String)> =
                sqlx::query_as(
                    "SELECT id, name, url, username, last_synced, provider_type
                     FROM caldav_sources ORDER BY created_at",
                )
                .fetch_all(pool)
                .await?;

            if sources.is_empty() {
                println!("No sources configured. Add one with `calrs source add`.");
                return Ok(());
            }

            let rows: Vec<SourceRow> = sources
                .into_iter()
                .map(
                    |(id, name, url, username, last_synced, provider_type)| SourceRow {
                        id: id[..8].to_string(),
                        name,
                        provider: factory::label(&provider_type).to_string(),
                        url,
                        username,
                        last_synced: last_synced.unwrap_or_else(|| "never".to_string()),
                    },
                )
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
            let existing: Option<(String, String, String, String)> = sqlx::query_as(
                "SELECT id, name, url, username FROM caldav_sources WHERE id LIKE ? || '%'",
            )
            .bind(&id)
            .fetch_optional(pool)
            .await?;

            let (full_id, current_name, current_url, current_username) = match existing {
                Some(t) => t,
                None => {
                    println!("{} No source found matching '{}'", "✗".red(), id);
                    return Ok(());
                }
            };

            if let Some(url) = url.as_deref() {
                crate::caldav::validate_caldav_url(url)?;
            }

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
            let source: Option<(
                String,
                String,
                String,
                String,
                Option<String>,
                String,
                Option<String>,
                Option<String>,
                String,
            )> = sqlx::query_as(
                "SELECT id, url, username, name, password_enc, auth_type, \
                 access_token_enc, token_expires_at, provider_type \
                 FROM caldav_sources WHERE id LIKE ? || '%'",
            )
            .bind(&id)
            .fetch_optional(pool)
            .await?;

            match source {
                Some((
                    source_id,
                    url,
                    username,
                    name,
                    password_enc,
                    auth_type,
                    access_token_enc,
                    token_expires_at,
                    provider_type,
                )) => {
                    println!(
                        "Testing source '{}' ({})…",
                        name,
                        factory::label(&provider_type)
                    );

                    // OAuth2 sources are CalDAV-only (Google). Basic-auth
                    // sources may be CalDAV or EWS; let the provider factory
                    // pick the right back-end.
                    let client: Box<dyn crate::providers::CalendarProvider> =
                        if auth_type == "oauth2" {
                            let caldav = crate::oauth2_caldav::build_client_for_source(
                                pool,
                                key,
                                &source_id,
                                &url,
                                &auth_type,
                                &username,
                                password_enc.as_deref(),
                                access_token_enc.as_deref(),
                                token_expires_at.as_deref(),
                            )
                            .await?;
                            Box::new(crate::providers::caldav::CaldavProvider::from_client(
                                caldav,
                            ))
                        } else {
                            let enc = password_enc.as_deref().ok_or_else(|| {
                                anyhow::anyhow!("Basic auth source missing password")
                            })?;
                            let password = crate::crypto::decrypt_password(key, enc)?;
                            build_provider(&provider_type, &url, &username, &password)?
                        };
                    match client.check_connection().await {
                        Ok(true) => println!("{} Connection OK", "✓".green()),
                        Ok(false) => println!("{} Connected, partial detection", "⚠".yellow()),
                        Err(e) => println!("{} Connection failed: {}", "✗".red(), e),
                    }
                }
                None => {
                    println!("{} No source found matching '{}'", "✗".red(), id);
                }
            }
        }
        SourceCommands::AddGoogle { name } => {
            let account: (String,) = sqlx::query_as("SELECT id FROM accounts LIMIT 1")
                .fetch_optional(pool)
                .await?
                .ok_or_else(|| anyhow::anyhow!("No account found. Run `calrs init` first."))?;

            let (client_id, client_secret): (Option<String>, Option<String>) = sqlx::query_as(
                "SELECT google_oauth2_client_id, google_oauth2_client_secret FROM auth_config LIMIT 1",
            )
            .fetch_optional(pool)
            .await?
            .unwrap_or((None, None));

            let client_id = client_id
                .filter(|s| !s.is_empty())
                .ok_or_else(|| anyhow::anyhow!("Google OAuth2 not configured. Set credentials via `calrs config` or the admin panel."))?;
            let client_secret_enc = client_secret
                .filter(|s| !s.is_empty())
                .ok_or_else(|| anyhow::anyhow!("Google OAuth2 client secret not configured."))?;
            // Stored client_secret is encrypted at rest (see crypto::encrypt_value).
            let client_secret =
                crate::crypto::decrypt_value(key, &client_secret_enc).map_err(|e| {
                    anyhow::anyhow!("Google OAuth2 client secret decryption failed: {}", e)
                })?;

            let name = name.unwrap_or_else(|| prompt("Display name"));

            // Bind a temporary TCP listener on a random port for the OAuth2 callback
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
            let port = listener.local_addr()?.port();
            let redirect_uri = format!("http://localhost:{port}/callback");

            let state = Uuid::new_v4().to_string();
            let auth_url =
                crate::oauth2_caldav::build_google_auth_url(&client_id, &redirect_uri, &state);

            println!("\nOpen this URL in your browser to authorize calrs:\n");
            println!("  {}\n", auth_url);

            // Try to open browser automatically
            if open::that(&auth_url).is_err() {
                println!("(Could not open browser automatically. Please copy the URL above.)");
            }

            println!("{} Waiting for authorization…", "…".dimmed());

            // Accept one connection and read the HTTP request
            let (mut stream, _) = listener.accept().await?;
            let mut buf = [0u8; 4096];
            let n = stream.read(&mut buf).await?;
            let request = String::from_utf8_lossy(&buf[..n]);

            // Parse the GET request line to extract query parameters
            let path = request
                .lines()
                .next()
                .and_then(|line| line.split_whitespace().nth(1))
                .unwrap_or("");

            // Send a response to the browser
            let response_body = "<html><body><h2>Authorization complete!</h2><p>You can close this tab and return to the terminal.</p></body></html>";
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                response_body.len(),
                response_body
            );
            let _ = stream.write_all(response.as_bytes()).await;
            drop(stream);

            // Extract code and state from query string
            let query = path.split('?').nth(1).unwrap_or("");
            let params: std::collections::HashMap<&str, &str> = query
                .split('&')
                .filter_map(|pair| {
                    let mut parts = pair.splitn(2, '=');
                    Some((parts.next()?, parts.next()?))
                })
                .collect();

            if let Some(error) = params.get("error") {
                bail!("Authorization failed: {}", error);
            }

            let callback_state = params.get("state").unwrap_or(&"");
            if *callback_state != state {
                bail!("CSRF state mismatch, possible security issue. Please try again.");
            }

            let code = params
                .get("code")
                .ok_or_else(|| anyhow::anyhow!("No authorization code received"))?;

            print!("{} Exchanging code for tokens… ", "…".dimmed());
            io::stdout().flush().unwrap();

            let (access_token, refresh_token, expires_in) =
                crate::oauth2_caldav::exchange_google_code(
                    &client_id,
                    &client_secret,
                    code,
                    &redirect_uri,
                )
                .await?;
            println!("{}", "OK".green());

            // Fetch the Google account email; it identifies the principal in the CalDAV URL.
            let username = crate::oauth2_caldav::fetch_google_email(&access_token).await?;
            let caldav_url = crate::oauth2_caldav::google_caldav_url_for_email(&username);

            // Test CalDAV connection (PROPFIND requires the per-user URL)
            print!("{} Testing CalDAV connection… ", "…".dimmed());
            io::stdout().flush().unwrap();

            let client = crate::caldav::CaldavClient::with_bearer(&caldav_url, &access_token);
            match client.check_connection().await {
                Ok(true) => println!("{}", "CalDAV supported".green()),
                Ok(false) => {
                    println!(
                        "{}",
                        "Connected but CalDAV not detected in DAV header".yellow()
                    );
                    println!("Continuing anyway…");
                }
                Err(e) => {
                    println!("{} {}", "✗".red(), e);
                    bail!("CalDAV connection failed: {}", e);
                }
            }

            // Encrypt and store tokens
            let access_token_enc = crate::crypto::encrypt_password(key, &access_token)?;
            let refresh_token_enc = crate::crypto::encrypt_password(key, &refresh_token)?;
            let expires_at =
                (chrono::Utc::now() + chrono::Duration::seconds(expires_in)).to_rfc3339();

            let id = Uuid::new_v4().to_string();

            sqlx::query(
                "INSERT INTO caldav_sources (id, account_id, name, url, username, password_enc, auth_type, oauth2_provider, access_token_enc, refresh_token_enc, token_expires_at) VALUES (?, ?, ?, ?, ?, ?, 'oauth2', 'google', ?, ?, ?)",
            )
            .bind(&id)
            .bind(&account.0)
            .bind(&name)
            .bind(&caldav_url)
            .bind(&username)
            .bind(None::<String>)
            .bind(&access_token_enc)
            .bind(&refresh_token_enc)
            .bind(&expires_at)
            .execute(pool)
            .await?;

            println!(
                "{} Google Calendar source '{}' added (id: {}, user: {})",
                "✓".green(),
                name,
                &id[..8],
                username
            );
            println!("Run `calrs sync` to fetch your calendars.");
        }
    }

    Ok(())
}
