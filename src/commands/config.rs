use anyhow::{Context, Result};
use clap::Subcommand;
use colored::Colorize;
use sqlx::SqlitePool;
use uuid::Uuid;

use crate::utils::prompt;

#[derive(Debug, Subcommand)]
pub enum ConfigCommands {
    /// Configure SMTP for email notifications
    Smtp {
        /// SMTP server host
        #[arg(long)]
        host: Option<String>,
        /// SMTP port (default: 587)
        #[arg(long)]
        port: Option<u16>,
        /// SMTP username
        #[arg(long)]
        username: Option<String>,
        /// From email address
        #[arg(long)]
        from_email: Option<String>,
        /// From display name
        #[arg(long)]
        from_name: Option<String>,
    },
    /// Show current configuration
    Show,
    /// Send a test email
    SmtpTest {
        /// Email address to send test to
        to: String,
    },
    /// Configure authentication settings
    Auth {
        /// Enable or disable registration
        #[arg(long)]
        registration: Option<bool>,
        /// Comma-separated list of allowed email domains (empty to allow all)
        #[arg(long)]
        allowed_domains: Option<String>,
    },
    /// Dump all configuration as JSON
    Dump {
        /// Pretty-print the JSON output
        #[arg(long)]
        pretty: bool,
    },
    /// Configure OIDC (OpenID Connect) for SSO login
    Oidc {
        /// OIDC issuer URL (e.g. https://keycloak.example.com/realms/myrealm)
        #[arg(long)]
        issuer_url: Option<String>,
        /// OIDC client ID
        #[arg(long)]
        client_id: Option<String>,
        /// OIDC client secret
        #[arg(long)]
        client_secret: Option<String>,
        /// Enable or disable OIDC
        #[arg(long)]
        enabled: Option<bool>,
        /// Auto-register users on first OIDC login
        #[arg(long)]
        auto_register: Option<bool>,
    },
}

// ── Dump-specific structs (no secret fields) ──

#[derive(serde::Serialize, sqlx::FromRow)]
#[serde(rename_all = "snake_case")]
struct AuthConfigDump {
    registration_enabled: bool,
    allowed_email_domains: Option<String>,
    oidc_enabled: bool,
    oidc_issuer_url: Option<String>,
    oidc_client_id: Option<String>,
    oidc_auto_register: bool,
    created_at: String,
    updated_at: String,
}

#[derive(serde::Serialize, sqlx::FromRow)]
#[serde(rename_all = "snake_case")]
struct SmtpConfigDump {
    id: String,
    host: String,
    port: i32,
    username: String,
    from_email: String,
    from_name: Option<String>,
    enabled: bool,
}

#[derive(serde::Serialize, sqlx::FromRow)]
#[serde(rename_all = "snake_case")]
struct UserDump {
    id: String,
    email: String,
    name: String,
    timezone: String,
    role: String,
    auth_provider: String,
    oidc_subject: Option<String>,
    enabled: bool,
    created_at: String,
    username: Option<String>,
    booking_email: Option<String>,
    title: Option<String>,
    bio: Option<String>,
    allow_dynamic_group: bool,
    language: Option<String>,
}

#[derive(serde::Serialize, sqlx::FromRow)]
#[serde(rename_all = "snake_case")]
struct CaldavSourceDump {
    id: String,
    account_id: String,
    name: String,
    url: String,
    username: String,
    last_synced: Option<String>,
    sync_token: Option<String>,
    enabled: bool,
    created_at: String,
}

#[derive(serde::Serialize, sqlx::FromRow)]
#[serde(rename_all = "snake_case")]
struct EventTypeDump {
    id: String,
    account_id: String,
    slug: String,
    title: String,
    duration_min: i32,
    location_type: String,
    location_value: Option<String>,
    buffer_before: i32,
    buffer_after: i32,
    min_notice_min: i32,
    enabled: bool,
    requires_confirmation: bool,
    reminder_minutes: Option<i32>,
    max_additional_guests: Option<i32>,
    scheduling_mode: String,
    first_slot_only: bool,
    default_calendar_view: String,
    visibility: String,
    created_at: String,
    description: Option<String>,
}

#[derive(serde::Serialize, sqlx::FromRow)]
#[serde(rename_all = "snake_case")]
struct TeamDump {
    id: String,
    name: String,
    slug: Option<String>,
    description: Option<String>,
    avatar_path: Option<String>,
    visibility: String,
    created_at: String,
}

#[derive(serde::Serialize, sqlx::FromRow)]
#[serde(rename_all = "snake_case")]
struct GroupDump {
    id: String,
    name: String,
    source: String,
    oidc_id: Option<String>,
    created_at: String,
    slug: Option<String>,
    description: Option<String>,
    avatar_path: Option<String>,
}

/// Build the full dump output as a JSON value, querying each config section.
async fn build_dump_output(pool: &SqlitePool) -> Result<serde_json::Value> {
    // Auth config — SELECT only non-secret columns (omit oidc_client_secret)
    let auth_json = sqlx::query_as::<_, AuthConfigDump>(
        "SELECT registration_enabled, allowed_email_domains, oidc_enabled, \
         oidc_issuer_url, oidc_client_id, oidc_auto_register, created_at, updated_at \
         FROM auth_config WHERE id = 'singleton'",
    )
    .fetch_one(pool)
    .await?;
    let auth_json = serde_json::to_value(&auth_json)?;

    // SMTP config — skip password_enc (secret)
    let smtp_json: serde_json::Value =
        match sqlx::query_as::<_, SmtpConfigDump>(
            "SELECT id, host, port, username, from_email, from_name, enabled \
             FROM smtp_config LIMIT 1",
        )
        .fetch_optional(pool)
        .await?
        {
            Some(row) => serde_json::to_value(&row)?,
            None => serde_json::Value::Null,
        };

    // Users — only enabled users; skip password_hash (secret)
    let users: Vec<UserDump> = sqlx::query_as::<_, UserDump>(
        "SELECT id, email, name, timezone, role, auth_provider, \
         oidc_subject, enabled, created_at, username, booking_email, \
         title, bio, allow_dynamic_group, language \
         FROM users WHERE enabled = 1 ORDER BY email",
    )
    .fetch_all(pool)
    .await?;
    let users_json = serde_json::to_value(&users)?;

    // CalDAV sources — skip password_enc (secret)
    let caldav_sources: Vec<CaldavSourceDump> = sqlx::query_as::<_, CaldavSourceDump>(
        "SELECT id, account_id, name, url, username, \
         last_synced, sync_token, enabled, created_at \
         FROM caldav_sources ORDER BY name",
    )
    .fetch_all(pool)
    .await?;
    let caldav_sources_json = serde_json::to_value(&caldav_sources)?;

    // Event types — selected config fields, no operational data
    let event_types: Vec<EventTypeDump> = sqlx::query_as::<_, EventTypeDump>(
        "SELECT id, account_id, slug, title, duration_min, location_type, \
         location_value, buffer_before, buffer_after, min_notice_min, enabled, \
         requires_confirmation, reminder_minutes, max_additional_guests, \
         scheduling_mode, first_slot_only, default_calendar_view, visibility, \
         created_at, description \
         FROM event_types ORDER BY title",
    )
    .fetch_all(pool)
    .await?;
    let event_types_json = serde_json::to_value(&event_types)?;

    // Teams
    let teams: Vec<TeamDump> = sqlx::query_as::<_, TeamDump>(
        "SELECT id, name, slug, description, avatar_path, \
         visibility, created_at \
         FROM teams ORDER BY name",
    )
    .fetch_all(pool)
    .await?;
    let teams_json = serde_json::to_value(&teams)?;

    // Groups (legacy — kept for OIDC identity sync)
    let groups: Vec<GroupDump> = sqlx::query_as::<_, GroupDump>(
        "SELECT id, name, source, oidc_id, created_at, \
         slug, description, avatar_path \
         FROM groups ORDER BY name",
    )
    .fetch_all(pool)
    .await?;
    let groups_json = serde_json::to_value(&groups)?;

    Ok(serde_json::json!({
        "auth": auth_json,
        "smtp": smtp_json,
        "users": users_json,
        "caldav_sources": caldav_sources_json,
        "event_types": event_types_json,
        "teams": teams_json,
        "groups": groups_json,
    }))
}

pub async fn run(pool: &SqlitePool, key: &[u8; 32], cmd: ConfigCommands) -> Result<()> {
    match cmd {
        ConfigCommands::Smtp {
            host,
            port,
            username,
            from_email,
            from_name,
        } => {
            let host = host.unwrap_or_else(|| prompt("SMTP host"));
            let port = port.unwrap_or_else(|| {
                let p = prompt("SMTP port (default 587)");
                if p.is_empty() {
                    587
                } else {
                    p.parse().unwrap_or(587)
                }
            });
            let username = username.unwrap_or_else(|| prompt("SMTP username"));
            let password = prompt("SMTP password");
            let from_email = from_email.unwrap_or_else(|| prompt("From email"));
            let from_name = from_name.or_else(|| {
                let name = prompt("From name (optional, press Enter to skip)");
                if name.is_empty() {
                    None
                } else {
                    Some(name)
                }
            });
            let tls_mode = {
                let raw = prompt("TLS mode (starttls/tls, default starttls)");
                let normalized = raw.trim().to_ascii_lowercase();
                match normalized.as_str() {
                    "" | "starttls" => "starttls",
                    "tls" => "tls",
                    other => {
                        anyhow::bail!(
                            "Invalid TLS mode '{}'. Use 'starttls' (default, port 587) or 'tls' (implicit TLS, port 465).",
                            other
                        );
                    }
                }
            };

            let password_enc = crate::crypto::encrypt_password(key, &password)?;
            let id = Uuid::new_v4().to_string();

            // SMTP is a system-wide singleton: clear any prior row before inserting.
            sqlx::query("DELETE FROM smtp_config").execute(pool).await?;

            sqlx::query(
                "INSERT INTO smtp_config (id, host, port, username, password_enc, from_email, from_name, tls_mode)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&id)
            .bind(&host)
            .bind(port as i32)
            .bind(&username)
            .bind(&password_enc)
            .bind(&from_email)
            .bind(&from_name)
            .bind(tls_mode)
            .execute(pool)
            .await?;

            println!(
                "{} SMTP configured ({}:{}, {})",
                "✓".green(),
                host,
                port,
                tls_mode
            );
        }
        ConfigCommands::Show => {
            let smtp: Option<(String, i32, String, String, Option<String>, bool)> = sqlx::query_as(
                "SELECT host, port, username, from_email, from_name, enabled FROM smtp_config LIMIT 1",
            )
            .fetch_optional(pool)
            .await?;

            match smtp {
                Some((host, port, username, from_email, from_name, enabled)) => {
                    println!("{}:", "SMTP".bold());
                    println!("  Host:     {}:{}", host, port);
                    println!("  Username: {}", username);
                    println!(
                        "  From:     {} <{}>",
                        from_name.as_deref().unwrap_or(""),
                        from_email
                    );
                    println!("  Enabled:  {}", if enabled { "✓" } else { "✗" });
                }
                None => {
                    println!("No SMTP configured. Run `calrs config smtp` to set it up.");
                }
            }

            println!();
            let auth_config = crate::auth::get_auth_config(pool).await?;
            println!("{}:", "Authentication".bold());
            println!(
                "  Registration:  {}",
                if auth_config.registration_enabled {
                    "enabled"
                } else {
                    "disabled"
                }
            );
            match &auth_config.allowed_email_domains {
                Some(d) if !d.is_empty() => println!("  Allowed domains: {}", d),
                _ => println!("  Allowed domains: {}", "(any)".dimmed()),
            }

            println!();
            println!("{}:", "OIDC".bold());
            if auth_config.oidc_enabled {
                println!("  Enabled:       {}", "✓".green());
                println!(
                    "  Issuer:        {}",
                    auth_config
                        .oidc_issuer_url
                        .as_deref()
                        .unwrap_or("(not set)")
                );
                println!(
                    "  Client ID:     {}",
                    auth_config.oidc_client_id.as_deref().unwrap_or("(not set)")
                );
                println!(
                    "  Client secret: {}",
                    if auth_config.oidc_client_secret.is_some() {
                        "****"
                    } else {
                        "(not set)"
                    }
                );
                println!(
                    "  Auto-register: {}",
                    if auth_config.oidc_auto_register {
                        "yes"
                    } else {
                        "no"
                    }
                );
            } else {
                println!("  Enabled:       {}", "✗".dimmed());
                println!("  {}", "Run `calrs config oidc` to set up SSO.".dimmed());
            }
        }
        ConfigCommands::SmtpTest { to } => {
            let smtp_config = crate::email::load_smtp_config(pool, key).await?;
            match smtp_config {
                Some(config) => {
                    println!("{} Sending test email to {}…", "…".dimmed(), to);
                    crate::email::send_test_email(&config, &to).await?;
                    println!("{} Test email sent!", "✓".green());
                }
                None => {
                    println!(
                        "{} No SMTP configured. Run `calrs config smtp` first.",
                        "✗".red()
                    );
                }
            }
        }
        ConfigCommands::Auth {
            registration,
            allowed_domains,
        } => {
            if registration.is_none() && allowed_domains.is_none() {
                // Show current auth config
                let config = crate::auth::get_auth_config(pool).await?;
                println!("{}:", "Authentication".bold());
                println!(
                    "  Registration: {}",
                    if config.registration_enabled {
                        "enabled"
                    } else {
                        "disabled"
                    }
                );
                match &config.allowed_email_domains {
                    Some(domains) if !domains.is_empty() => {
                        println!("  Allowed domains: {}", domains);
                    }
                    _ => {
                        println!("  Allowed domains: {}", "(any)".dimmed());
                    }
                }
            } else {
                if let Some(reg) = registration {
                    sqlx::query(
                        "UPDATE auth_config SET registration_enabled = ?, updated_at = datetime('now') WHERE id = 'singleton'",
                    )
                    .bind(reg)
                    .execute(pool)
                    .await?;
                    println!(
                        "{} Registration {}",
                        "✓".green(),
                        if reg { "enabled" } else { "disabled" }
                    );
                }
                if let Some(domains) = allowed_domains {
                    let domains = if domains.is_empty() || domains == "any" {
                        None
                    } else {
                        Some(domains)
                    };
                    sqlx::query(
                        "UPDATE auth_config SET allowed_email_domains = ?, updated_at = datetime('now') WHERE id = 'singleton'",
                    )
                    .bind(&domains)
                    .execute(pool)
                    .await?;
                    match &domains {
                        Some(d) => println!("{} Allowed domains set to: {}", "✓".green(), d),
                        None => println!("{} Allowed domain restriction removed", "✓".green()),
                    }
                }
            }
        }
        ConfigCommands::Dump { pretty } => {
            let output = build_dump_output(pool).await?;
            let json = if pretty {
                serde_json::to_string_pretty(&output)
            } else {
                serde_json::to_string(&output)
            }
            .context("Failed to serialize dump output")?;
            println!("{json}");
        }
        ConfigCommands::Oidc {
            issuer_url,
            client_id,
            client_secret,
            enabled,
            auto_register,
        } => {
            // If no flags, prompt interactively
            if issuer_url.is_none()
                && client_id.is_none()
                && client_secret.is_none()
                && enabled.is_none()
                && auto_register.is_none()
            {
                let issuer =
                    prompt("OIDC issuer URL (e.g. https://keycloak.example.com/realms/myrealm)");
                let cid = prompt("Client ID");
                let csecret = prompt("Client secret");
                let auto_reg = prompt("Auto-register users on first login? (y/n)");

                let encrypted_secret = crate::crypto::encrypt_value(key, &csecret)?;
                sqlx::query(
                    "UPDATE auth_config SET oidc_enabled = 1, oidc_issuer_url = ?, oidc_client_id = ?, oidc_client_secret = ?, oidc_auto_register = ?, updated_at = datetime('now') WHERE id = 'singleton'",
                )
                .bind(&issuer)
                .bind(&cid)
                .bind(&encrypted_secret)
                .bind(auto_reg.starts_with('y') || auto_reg.starts_with('Y'))
                .execute(pool)
                .await?;

                println!("{} OIDC configured and enabled", "✓".green());
                println!("  Issuer:    {}", issuer);
                println!("  Client ID: {}", cid);
            } else {
                if let Some(url) = issuer_url {
                    sqlx::query("UPDATE auth_config SET oidc_issuer_url = ?, updated_at = datetime('now') WHERE id = 'singleton'")
                        .bind(&url)
                        .execute(pool)
                        .await?;
                    println!("{} OIDC issuer URL set", "✓".green());
                }
                if let Some(cid) = client_id {
                    sqlx::query("UPDATE auth_config SET oidc_client_id = ?, updated_at = datetime('now') WHERE id = 'singleton'")
                        .bind(&cid)
                        .execute(pool)
                        .await?;
                    println!("{} OIDC client ID set", "✓".green());
                }
                if let Some(cs) = client_secret {
                    let encrypted_secret = crate::crypto::encrypt_value(key, &cs)?;
                    sqlx::query("UPDATE auth_config SET oidc_client_secret = ?, updated_at = datetime('now') WHERE id = 'singleton'")
                        .bind(&encrypted_secret)
                        .execute(pool)
                        .await?;
                    println!("{} OIDC client secret set", "✓".green());
                }
                if let Some(en) = enabled {
                    sqlx::query("UPDATE auth_config SET oidc_enabled = ?, updated_at = datetime('now') WHERE id = 'singleton'")
                        .bind(en)
                        .execute(pool)
                        .await?;
                    println!(
                        "{} OIDC {}",
                        "✓".green(),
                        if en { "enabled" } else { "disabled" }
                    );
                }
                if let Some(ar) = auto_register {
                    sqlx::query("UPDATE auth_config SET oidc_auto_register = ?, updated_at = datetime('now') WHERE id = 'singleton'")
                        .bind(ar)
                        .execute(pool)
                        .await?;
                    println!(
                        "{} OIDC auto-register {}",
                        "✓".green(),
                        if ar { "enabled" } else { "disabled" }
                    );
                }
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
    use std::str::FromStr;

    async fn setup_db() -> SqlitePool {
        let pool = SqlitePoolOptions::new()
            .max_connections(2)
            .connect_with(
                SqliteConnectOptions::from_str("sqlite::memory:")
                    .unwrap()
                    .foreign_keys(true),
            )
            .await
            .unwrap();
        crate::db::migrate(&pool).await.unwrap();
        pool
    }

    #[tokio::test]
    async fn config_show_no_smtp() {
        let pool = setup_db().await;
        let key = [0u8; 32];
        let result = run(&pool, &key, ConfigCommands::Show).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn config_show_with_smtp() {
        let pool = setup_db().await;
        let key = [0u8; 32];

        let smtp_id = Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO smtp_config (id, host, port, username, password_enc, from_email, from_name, enabled) VALUES (?, 'smtp.test.com', 587, 'user', 'enc', 'noreply@test.com', 'Test', 1)")
            .bind(&smtp_id).execute(&pool).await.unwrap();

        let result = run(&pool, &key, ConfigCommands::Show).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn config_auth_show_defaults() {
        let pool = setup_db().await;
        let key = [0u8; 32];
        // No flags — shows current config
        let result = run(
            &pool,
            &key,
            ConfigCommands::Auth {
                registration: None,
                allowed_domains: None,
            },
        )
        .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn config_auth_disable_registration() {
        let pool = setup_db().await;
        let key = [0u8; 32];
        let result = run(
            &pool,
            &key,
            ConfigCommands::Auth {
                registration: Some(false),
                allowed_domains: None,
            },
        )
        .await;
        assert!(result.is_ok());

        let enabled: bool = sqlx::query_scalar(
            "SELECT registration_enabled FROM auth_config WHERE id = 'singleton'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(!enabled);
    }

    #[tokio::test]
    async fn config_auth_set_allowed_domains() {
        let pool = setup_db().await;
        let key = [0u8; 32];
        let result = run(
            &pool,
            &key,
            ConfigCommands::Auth {
                registration: None,
                allowed_domains: Some("example.com,test.org".to_string()),
            },
        )
        .await;
        assert!(result.is_ok());

        let domains: Option<String> = sqlx::query_scalar(
            "SELECT allowed_email_domains FROM auth_config WHERE id = 'singleton'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(domains.unwrap(), "example.com,test.org");
    }

    #[tokio::test]
    async fn config_auth_clear_domains_with_any() {
        let pool = setup_db().await;
        let key = [0u8; 32];
        // Set domains first
        let _ = run(
            &pool,
            &key,
            ConfigCommands::Auth {
                registration: None,
                allowed_domains: Some("example.com".to_string()),
            },
        )
        .await;
        // Clear with "any"
        let result = run(
            &pool,
            &key,
            ConfigCommands::Auth {
                registration: None,
                allowed_domains: Some("any".to_string()),
            },
        )
        .await;
        assert!(result.is_ok());

        let domains: Option<String> = sqlx::query_scalar(
            "SELECT allowed_email_domains FROM auth_config WHERE id = 'singleton'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(domains.is_none());
    }

    #[tokio::test]
    async fn config_oidc_set_fields() {
        let pool = setup_db().await;
        let key = [0u8; 32];
        let result = run(
            &pool,
            &key,
            ConfigCommands::Oidc {
                issuer_url: Some("https://auth.example.com/realms/test".to_string()),
                client_id: Some("calrs-app".to_string()),
                client_secret: Some("super-secret".to_string()),
                enabled: Some(true),
                auto_register: Some(true),
            },
        )
        .await;
        assert!(result.is_ok());

        let (issuer, cid, enabled, auto_reg): (Option<String>, Option<String>, bool, bool) =
            sqlx::query_as(
                "SELECT oidc_issuer_url, oidc_client_id, oidc_enabled, oidc_auto_register FROM auth_config WHERE id = 'singleton'",
            )
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(issuer.unwrap(), "https://auth.example.com/realms/test");
        assert_eq!(cid.unwrap(), "calrs-app");
        assert!(enabled);
        assert!(auto_reg);
    }

    #[tokio::test]
    async fn config_oidc_disable() {
        let pool = setup_db().await;
        let key = [0u8; 32];
        // Enable first
        let _ = run(
            &pool,
            &key,
            ConfigCommands::Oidc {
                issuer_url: None,
                client_id: None,
                client_secret: None,
                enabled: Some(true),
                auto_register: None,
            },
        )
        .await;
        // Then disable
        let result = run(
            &pool,
            &key,
            ConfigCommands::Oidc {
                issuer_url: None,
                client_id: None,
                client_secret: None,
                enabled: Some(false),
                auto_register: None,
            },
        )
        .await;
        assert!(result.is_ok());

        let enabled: bool =
            sqlx::query_scalar("SELECT oidc_enabled FROM auth_config WHERE id = 'singleton'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert!(!enabled);
    }

    #[tokio::test]
    async fn config_smtp_test_no_smtp() {
        let pool = setup_db().await;
        let key = [0u8; 32];
        // No SMTP configured — should print error, not crash
        let result = run(
            &pool,
            &key,
            ConfigCommands::SmtpTest {
                to: "test@example.com".to_string(),
            },
        )
        .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn config_dump_default() {
        let pool = setup_db().await;
        let output = build_dump_output(&pool).await.unwrap();
        assert!(output["auth"]["registration_enabled"].as_bool().unwrap());
        assert!(output["smtp"].is_null());
        assert!(output["users"].as_array().unwrap().is_empty());
        assert!(output["caldav_sources"].as_array().unwrap().is_empty());
        assert!(output["event_types"].as_array().unwrap().is_empty());
        assert!(output["teams"].as_array().unwrap().is_empty());
        assert!(output["groups"].as_array().unwrap().is_empty());
    }

    #[tokio::test]
    async fn config_dump_pretty() {
        let pool = setup_db().await;
        let key = [0u8; 32];
        let result = run(&pool, &key, ConfigCommands::Dump { pretty: true }).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn config_dump_with_smtp() {
        let pool = setup_db().await;

        let smtp_id = Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO smtp_config (id, host, port, username, password_enc, from_email, from_name, enabled) \
             VALUES (?, 'smtp.test.com', 587, 'user', 'enc', 'noreply@test.com', 'Test', 1)",
        )
        .bind(&smtp_id)
        .execute(&pool)
        .await
        .unwrap();

        let output = build_dump_output(&pool).await.unwrap();
        let smtp = output["smtp"].as_object().unwrap();
        assert_eq!(smtp["host"], "smtp.test.com");
        assert_eq!(smtp["port"], 587);
        assert_eq!(smtp["username"], "user");
        assert_eq!(smtp["from_email"], "noreply@test.com");
        assert_eq!(smtp["from_name"], "Test");
        assert!(smtp["enabled"].as_bool().unwrap());
        // Verify secret is NOT in output
        assert!(!smtp.contains_key("password_enc"));
    }

    #[tokio::test]
    async fn config_dump_with_users_and_event_types() {
        let pool = setup_db().await;

        // Insert a user
        let user_id = Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO users (id, email, name, password_hash, role, auth_provider) \
             VALUES (?, 'alice@test.com', 'Alice', 'hash', 'admin', 'local')",
        )
        .bind(&user_id)
        .execute(&pool)
        .await
        .unwrap();

        // Insert an account (required by event_types FK)
        let account_id = Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO accounts (id, name, email) VALUES (?, 'Alice', 'alice@test.com')",
        )
        .bind(&account_id)
        .execute(&pool)
        .await
        .unwrap();

        // Insert an event type
        let et_id = Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO event_types (id, account_id, slug, title, duration_min, visibility) \
             VALUES (?, ?, 'intro', 'Intro Call', 30, 'public')",
        )
        .bind(&et_id)
        .bind(&account_id)
        .execute(&pool)
        .await
        .unwrap();

        // Insert a team
        let team_id = Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO teams (id, name, slug, visibility) VALUES (?, 'My Team', 'my-team', 'public')",
        )
        .bind(&team_id)
        .execute(&pool)
        .await
        .unwrap();

        // Insert a group
        let group_id = Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO groups (id, name, source) VALUES (?, 'Engineering', 'local')",
        )
        .bind(&group_id)
        .execute(&pool)
        .await
        .unwrap();

        let output = build_dump_output(&pool).await.unwrap();
        assert_eq!(output["users"].as_array().unwrap().len(), 1);
        assert_eq!(output["users"][0]["email"], "alice@test.com");
        assert!(!output["users"][0]
            .as_object()
            .unwrap()
            .contains_key("password_hash"));
        assert_eq!(output["event_types"].as_array().unwrap().len(), 1);
        assert_eq!(output["event_types"][0]["title"], "Intro Call");
        assert_eq!(output["teams"].as_array().unwrap().len(), 1);
        assert_eq!(output["groups"].as_array().unwrap().len(), 1);

        // Test with a disabled user — should not appear
        let disabled_id = Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO users (id, email, name, password_hash, role, auth_provider, enabled) \
             VALUES (?, 'bob@test.com', 'Bob', 'hash', 'user', 'local', 0)",
        )
        .bind(&disabled_id)
        .execute(&pool)
        .await
        .unwrap();

        let output = build_dump_output(&pool).await.unwrap();
        assert_eq!(
            output["users"].as_array().unwrap().len(),
            1,
            "disabled user should not appear"
        );
    }

    #[tokio::test]
    async fn config_dump_caldav_no_secrets() {
        let pool = setup_db().await;

        let user_id = Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO users (id, email, name, role, auth_provider, enabled) \
             VALUES (?, 'test@test.com', 'Test', 'user', 'local', 1)",
        )
        .bind(&user_id)
        .execute(&pool)
        .await
        .unwrap();
        let account_id = Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO accounts (id, name, email, user_id) VALUES (?, 'Test', 'test@test.com', ?)",
        )
        .bind(&account_id)
        .bind(&user_id)
        .execute(&pool)
        .await
        .unwrap();
        let source_id = Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO caldav_sources (id, account_id, name, url, username, password_enc) \
             VALUES (?, ?, 'Test', 'https://caldav.test', 'user', 'secret-encrypted')",
        )
        .bind(&source_id)
        .bind(&account_id)
        .execute(&pool)
        .await
        .unwrap();

        let output = build_dump_output(&pool).await.unwrap();
        let sources = output["caldav_sources"].as_array().unwrap();
        assert_eq!(sources.len(), 1);
        assert!(!sources[0]
            .as_object()
            .unwrap()
            .contains_key("password_enc"));
    }
}
