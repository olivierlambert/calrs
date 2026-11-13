use anyhow::{bail, Result};
use clap::Subcommand;
use colored::Colorize;
use sqlx::SqlitePool;
use tabled::{Table, Tabled};
use uuid::Uuid;

use crate::providers::{build_provider, factory};

use std::io::{self, Write};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::utils::{id_prefix_condition, pick_id_by_prefix, prompt, read_secret};

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
        /// Read the password from stdin instead of the terminal (for scripts).
        /// Needs --username, --name and --url (or --email with --provider ews),
        /// since stdin then holds the password.
        #[arg(long)]
        password_stdin: bool,
        /// CalDAV only: list the source's calendars now, none of them
        /// synced, so you pick them with `calrs source calendars` before the
        /// first sync fetches anything.
        #[arg(long)]
        pick_calendars: bool,
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
        /// Read the new password from stdin instead of the terminal (for scripts)
        #[arg(long, conflicts_with = "password")]
        password_stdin: bool,
    },
    /// List a source's calendars and choose which ones are synced
    Calendars {
        /// Source ID (or unique prefix)
        id: String,
        /// Sync this calendar (ID prefix, href or display name; repeatable)
        #[arg(long)]
        enable: Vec<String>,
        /// Stop syncing this calendar and drop its cached events (ID prefix,
        /// href or display name; repeatable)
        #[arg(long)]
        disable: Vec<String>,
        /// CalDAV only: read the calendar list from the server again,
        /// without fetching any event
        #[arg(long)]
        discover: bool,
        /// Whether calendars the source lists later start synced (true, the
        /// default) or not (false)
        #[arg(long, value_name = "BOOL")]
        sync_new: Option<bool>,
    },
}

#[derive(Tabled)]
struct CalendarRow {
    #[tabled(rename = "ID")]
    id: String,
    #[tabled(rename = "Name")]
    name: String,
    #[tabled(rename = "Synced")]
    synced: String,
    #[tabled(rename = "Events")]
    events: i64,
    #[tabled(rename = "Href")]
    href: String,
}

/// (id, href, display name, sync_enabled, cached event count)
type CalendarListing = (String, String, Option<String>, bool, i64);

/// The calendar of `calendars` that `wanted` names: an id prefix, the exact
/// href, or the display name (any case).
fn find_calendar<'a>(calendars: &'a [CalendarListing], wanted: &str) -> Result<&'a str> {
    let matches: Vec<&str> = calendars
        .iter()
        .filter(|(id, href, name, _, _)| {
            id.starts_with(wanted)
                || href == wanted
                || name
                    .as_deref()
                    .is_some_and(|n| n.eq_ignore_ascii_case(wanted))
        })
        .map(|(id, _, _, _, _)| id.as_str())
        .collect();
    match matches.as_slice() {
        [one] => Ok(one),
        [] => bail!("no calendar of this source matches '{}'", wanted),
        _ => bail!("'{}' matches several calendars, use its ID", wanted),
    }
}

async fn list_source_calendars(pool: &SqlitePool, source_id: &str) -> Result<Vec<CalendarListing>> {
    Ok(sqlx::query_as(
        "SELECT c.id, c.href, c.display_name, c.sync_enabled,
                (SELECT COUNT(*) FROM events e WHERE e.calendar_id = c.id)
         FROM calendars c WHERE c.source_id = ?
         ORDER BY c.display_name, c.href",
    )
    .bind(source_id)
    .fetch_all(pool)
    .await?)
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

/// Resolves a source ID prefix to one full ID, for `remove`, `update`,
/// `test` and `calendars` (see [`pick_id_by_prefix`]).
async fn resolve_source_id(pool: &SqlitePool, prefix: &str) -> Result<String> {
    let sql = format!(
        "SELECT id FROM caldav_sources WHERE {}",
        id_prefix_condition("id")
    );
    let ids: Vec<String> = sqlx::query_scalar(&sql)
        .bind(prefix)
        .bind(prefix)
        .fetch_all(pool)
        .await?;
    pick_id_by_prefix(prefix, ids, "source")
}

pub async fn run(pool: &SqlitePool, key: &[u8; 32], cmd: SourceCommands) -> Result<()> {
    // Honour the DB-stored private-host allowlist during URL validation (the
    // env var, if set, still takes precedence — see `crate::settings`).
    crate::settings::load_from_db(pool).await;

    match cmd {
        SourceCommands::Add {
            url,
            username,
            name,
            provider,
            email,
            no_test,
            password_stdin,
            pick_calendars,
        } => {
            let provider = provider.trim().to_ascii_lowercase();
            if provider != factory::kinds::CALDAV && provider != factory::kinds::EWS {
                bail!("unknown provider '{}'. Use 'caldav' or 'ews'.", provider);
            }
            if pick_calendars && provider != factory::kinds::CALDAV {
                bail!("--pick-calendars works with CalDAV sources only");
            }
            let url_known = url.is_some() || (provider == factory::kinds::EWS && email.is_some());
            if password_stdin && (username.is_none() || name.is_none() || !url_known) {
                bail!(
                    "--password-stdin needs --username, --name and --url (or --email with \
                     --provider ews): stdin holds the password"
                );
            }

            let account: (String,) = sqlx::query_as("SELECT id FROM accounts LIMIT 1")
                .fetch_optional(pool)
                .await?
                .ok_or_else(|| anyhow::anyhow!("No account found. Run `calrs init` first."))?;

            let username = username.unwrap_or_else(|| prompt("Username"));
            let name = name.unwrap_or_else(|| prompt("Display name"));
            let password = read_secret("Password", password_stdin)?;
            if password.is_empty() {
                // An unset variable in `printf '%s' "$PW" | ... --password-stdin`
                // would otherwise store an empty secret.
                bail!("Password is required");
            }

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
                                if password_stdin {
                                    bail!("Autodiscover failed: {e}. Pass --url.");
                                }
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
                "INSERT INTO caldav_sources (id, account_id, name, url, username, password_enc, provider_type, sync_new_calendars) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&id)
            .bind(&account.0)
            .bind(&name)
            .bind(&url)
            .bind(&username)
            .bind(&password_enc)
            .bind(&provider)
            .bind(!pick_calendars)
            .execute(pool)
            .await?;

            println!(
                "{} Source '{}' ({}) added (id: {})",
                "✓".green(),
                name,
                factory::label(&provider),
                &id[..8]
            );

            if pick_calendars {
                let client = crate::caldav::CaldavClient::new(&url, &username, &password);
                let found =
                    crate::commands::sync::discover_new_caldav_source(pool, key, &client, &id)
                    .await?;
                println!(
                    "{} {} calendar(s) listed, none synced yet. Pick them with:",
                    "✓".green(),
                    found
                );
                println!(
                    "  calrs source calendars {} --enable <calendar>", &id[..8]
                );
            }
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
            let full_id = resolve_source_id(pool, &id).await?;
            // CASCADE handles events and calendars
            sqlx::query("DELETE FROM caldav_sources WHERE id = ?")
                .bind(&full_id)
                .execute(pool)
                .await?;
            println!("{} Source removed.", "✓".green());
        }
        SourceCommands::Update {
            id,
            name,
            url,
            username,
            password,
            password_stdin,
        } => {
            let (full_id, current_name, current_url, current_username, auth_type): (
                String,
                String,
                String,
                String,
                String,
            ) = sqlx::query_as(
                "SELECT id, name, url, username, auth_type FROM caldav_sources WHERE id = ?",
            )
            .bind(resolve_source_id(pool, &id).await?)
            .fetch_one(pool)
            .await?;
            if auth_type == "oauth2"
                && (url.is_some() || username.is_some() || password || password_stdin)
            {
                bail!("Reconnect OAuth sources from your dashboard to change their account or credentials");
            }

            if let Some(url) = url.as_deref() {
                crate::caldav::validate_caldav_url(url)?;
            }

            let url_or_username_changed = url.is_some() || username.is_some();
            let previous_url = current_url.clone();
            let previous_username = current_username.clone();
            let new_name = name.unwrap_or(current_name);
            let new_url = url.unwrap_or(current_url);
            let new_username = username.unwrap_or(current_username);

            if password || password_stdin {
                let new_pw = read_secret("New password", password_stdin)?;
                if new_pw.is_empty() {
                    bail!("Password is required when --password or --password-stdin is set");
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

            // An EWS source pointed at another account or server reads
            // another mailbox, whose folder ids are not the old one's: what
            // it cached goes now rather than at the next sync.
            let provider_type: String =
                sqlx::query_scalar("SELECT provider_type FROM caldav_sources WHERE id = ?")
                    .bind(&full_id)
                    .fetch_one(pool)
                    .await?;
            if provider_type == factory::kinds::EWS
                && (!previous_username
                    .trim()
                    .eq_ignore_ascii_case(new_username.trim())
                    || crate::resources::url_origin(&previous_url)
                        != crate::resources::url_origin(&new_url))
            {
                crate::ews::global::forget_mailbox(pool, &full_id).await?;
            }

            if url_or_username_changed {
                println!(
                    "{}",
                    "  URL or username changed — run `calrs sync` to refresh the calendar list."
                        .dimmed()
                );
            }
        }
        SourceCommands::Calendars {
            id,
            enable,
            disable,
            discover,
            sync_new,
        } => {
            let source_id = resolve_source_id(pool, &id).await?;
            #[allow(clippy::type_complexity)]
            let (source_name, url, username, password_enc, auth_type, access_token_enc, token_expires_at, provider_type):
                (String, String, String, Option<String>, String, Option<String>, Option<String>, String) =
                sqlx::query_as(
                    "SELECT name, url, username, password_enc, auth_type, access_token_enc, \
                     token_expires_at, provider_type FROM caldav_sources WHERE id = ?",
                )
                .bind(&source_id)
                .fetch_one(pool)
                .await?;

            if let Some(sync_new) = sync_new {
                crate::commands::sync::set_sync_new_calendars(pool, &source_id, sync_new).await?;
            }

            if discover {
                if provider_type != factory::kinds::CALDAV {
                    bail!("--discover works with CalDAV sources only; run `calrs sync` instead");
                }
                let client = crate::oauth2_caldav::build_client_for_source(
                    pool, key, &source_id, &url, &auth_type, &username,
                    password_enc.as_deref(), access_token_enc.as_deref(), token_expires_at.as_deref(),
                )
                .await?;
                crate::commands::sync::discover_caldav_calendars(pool, &client, &source_id).await?;
            }

            let calendars = list_source_calendars(pool, &source_id).await?;
            if calendars.is_empty() {
                println!(
                    "No calendars discovered for '{}' yet. Run `calrs source calendars {} --discover` \
                     to list them without syncing, or `calrs sync`.",
                    source_name,
                    &source_id[..8.min(source_id.len())]
                );
                return Ok(());
            }

            // Resolve every name first, so a typo changes nothing.
            let mut changes: Vec<(String, bool)> = Vec::new();
            for wanted in &enable {
                changes.push((find_calendar(&calendars, wanted)?.to_string(), true));
            }
            for wanted in &disable {
                changes.push((find_calendar(&calendars, wanted)?.to_string(), false));
            }
            for (calendar_id, enabled) in &changes {
                crate::commands::sync::set_calendar_sync_enabled(
                    pool,
                    &source_id,
                    calendar_id,
                    *enabled,
                )
                .await?;
            }

            let calendars = if changes.is_empty() {
                calendars
            } else {
                list_source_calendars(pool, &source_id).await?
            };
            let rows: Vec<CalendarRow> = calendars
                .into_iter()
                .map(|(id, href, name, sync_enabled, events)| CalendarRow {
                    id: id[..8.min(id.len())].to_string(),
                    name: name.unwrap_or_default(),
                    synced: if sync_enabled { "yes" } else { "no" }.to_string(),
                    events,
                    href,
                })
                .collect();
            println!("{}", Table::new(rows));
            let syncs_new: bool =
                sqlx::query_scalar("SELECT sync_new_calendars FROM caldav_sources WHERE id = ?")
                    .bind(&source_id)
                    .fetch_one(pool)
                    .await?;
            println!("  Calendars found later: {}", if syncs_new { "synced" } else { "not synced" });
            if changes.iter().any(|(_, enabled)| *enabled) {
                println!(
                    "{}",
                    "  Run `calrs sync` to fetch the calendars just enabled.".dimmed()
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
                 FROM caldav_sources WHERE id = ?",
            )
            .bind(resolve_source_id(pool, &id).await?)
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

                    // OAuth providers use their own token lifecycle; basic-auth
                    // sources are constructed by the protocol factory.
                    let client: Box<dyn crate::providers::CalendarProvider> =
                        if provider_type == factory::kinds::MICROSOFT {
                            crate::microsoft_oauth::build_provider_for_source(pool, key, &source_id)
                                .await?
                        } else if auth_type == "oauth2" {
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
                    bail!("No source found matching '{}'", id);
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

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
    use std::str::FromStr;

    /// A source ID prefix matching two sources is refused instead of
    /// acting on whichever one the database returns first.
    #[tokio::test]
    async fn calendars_refuses_an_ambiguous_source_prefix() {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(SqliteConnectOptions::from_str("sqlite::memory:").unwrap())
            .await
            .unwrap();
        crate::db::migrate(&pool).await.unwrap();
        sqlx::query("INSERT INTO accounts (id, name, email, timezone) VALUES ('acc', 'A', 'a@example.com', 'UTC')")
            .execute(&pool).await.unwrap();
        for id in ["ab12-one", "ab34-two"] {
            sqlx::query("INSERT INTO caldav_sources (id, account_id, name, url, username) VALUES (?, 'acc', ?, 'https://dav.example.com/', 'u')")
                .bind(id).bind(id).execute(&pool).await.unwrap();
        }

        let err = run(
            &pool,
            &[0u8; 32],
            SourceCommands::Calendars {
                id: "ab".to_string(),
                enable: vec![],
                disable: vec![],
                discover: false,
                sync_new: None,
            },
        )
        .await
        .unwrap_err();
        assert!(err.to_string().contains("matches 2 sources"), "{err}");

        // Remove, update and test refuse the same prefix, and remove deletes
        // nothing.
        for cmd in [
            SourceCommands::Remove { id: "ab".into() },
            SourceCommands::Update {
                id: "ab".into(),
                name: Some("renamed".into()),
                url: None,
                username: None,
                password: false,
                password_stdin: false,
            },
            SourceCommands::Test { id: "ab".into() },
        ] {
            let err = run(&pool, &[0u8; 32], cmd).await.unwrap_err();
            assert!(err.to_string().contains("matches 2 sources"), "{err}");
        }
        let names: Vec<String> = sqlx::query_scalar("SELECT name FROM caldav_sources ORDER BY id")
            .fetch_all(&pool)
            .await
            .unwrap();
        assert_eq!(names, ["ab12-one", "ab34-two"]);

        // Wildcards and an empty ID are not patterns, and unknown IDs fail.
        for id in ["%", "_b12", "", "zz"] {
            let err = run(&pool, &[0u8; 32], SourceCommands::Remove { id: id.into() })
                .await
                .unwrap_err();
            assert!(
                err.to_string().contains("No source found")
                    || err.to_string().contains("give a source ID"),
                "{id:?}: {err}"
            );
        }

        // A unique prefix still works.
        run(
            &pool,
            &[0u8; 32],
            SourceCommands::Remove { id: "ab12".into() },
        )
        .await
        .unwrap();
        let left: Vec<String> = sqlx::query_scalar("SELECT id FROM caldav_sources")
            .fetch_all(&pool)
            .await
            .unwrap();
        assert_eq!(left, ["ab34-two"]);
    }
}
