use anyhow::Result;
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

pub async fn run(pool: &SqlitePool, key: &[u8; 32], cmd: ConfigCommands) -> Result<()> {
    match cmd {
        ConfigCommands::Smtp {
            host,
            port,
            username,
            from_email,
            from_name,
        } => {
            let account: (String,) = sqlx::query_as("SELECT id FROM accounts LIMIT 1")
                .fetch_optional(pool)
                .await?
                .ok_or_else(|| anyhow::anyhow!("No account found. Run `calrs init` first."))?;

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

            let password_enc = crate::crypto::encrypt_password(key, &password)?;
            let id = Uuid::new_v4().to_string();

            // Upsert (one config per account)
            sqlx::query("DELETE FROM smtp_config WHERE account_id = ?")
                .bind(&account.0)
                .execute(pool)
                .await?;

            sqlx::query(
                "INSERT INTO smtp_config (id, account_id, host, port, username, password_enc, from_email, from_name)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&id)
            .bind(&account.0)
            .bind(&host)
            .bind(port as i32)
            .bind(&username)
            .bind(&password_enc)
            .bind(&from_email)
            .bind(&from_name)
            .execute(pool)
            .await?;

            println!("{} SMTP configured ({}:{})", "✓".green(), host, port);
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

                sqlx::query(
                    "UPDATE auth_config SET oidc_enabled = 1, oidc_issuer_url = ?, oidc_client_id = ?, oidc_client_secret = ?, oidc_auto_register = ?, updated_at = datetime('now') WHERE id = 'singleton'",
                )
                .bind(&issuer)
                .bind(&cid)
                .bind(&csecret)
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
                    sqlx::query("UPDATE auth_config SET oidc_client_secret = ?, updated_at = datetime('now') WHERE id = 'singleton'")
                        .bind(&cs)
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

        // Seed account and SMTP config
        let user_id = Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO users (id, email, name, role, auth_provider, username, enabled) VALUES (?, 'test@test.com', 'Test', 'admin', 'local', 'test', 1)")
            .bind(&user_id).execute(&pool).await.unwrap();
        let account_id = Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO accounts (id, name, email, timezone, user_id) VALUES (?, 'Test', 'test@test.com', 'UTC', ?)")
            .bind(&account_id).bind(&user_id).execute(&pool).await.unwrap();
        let smtp_id = Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO smtp_config (id, account_id, host, port, username, password_enc, from_email, from_name, enabled) VALUES (?, ?, 'smtp.test.com', 587, 'user', 'enc', 'noreply@test.com', 'Test', 1)")
            .bind(&smtp_id).bind(&account_id).execute(&pool).await.unwrap();

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
}
