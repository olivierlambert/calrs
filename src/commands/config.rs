use anyhow::Result;
use clap::Subcommand;
use colored::Colorize;
use sqlx::SqlitePool;
use uuid::Uuid;

use std::io::{self, Write};

#[derive(Debug, Subcommand)]
pub enum ConfigCommands {
    /// Configure SMTP for email notifications
    Smtp {
        /// SMTP server host
        #[arg(long)]
        host: Option<String>,
        /// SMTP port (default: 587)
        #[arg(long, default_value = "587")]
        port: u16,
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

fn prompt(label: &str) -> String {
    print!("{}: ", label);
    io::stdout().flush().unwrap();
    let mut input = String::new();
    io::stdin().read_line(&mut input).unwrap();
    input.trim().to_string()
}

pub async fn run(pool: &SqlitePool, cmd: ConfigCommands) -> Result<()> {
    match cmd {
        ConfigCommands::Smtp {
            host,
            port,
            username,
            from_email,
            from_name,
        } => {
            let account: (String,) =
                sqlx::query_as("SELECT id FROM accounts LIMIT 1")
                    .fetch_optional(pool)
                    .await?
                    .ok_or_else(|| anyhow::anyhow!("No account found. Run `calrs init` first."))?;

            let host = host.unwrap_or_else(|| prompt("SMTP host"));
            let username = username.unwrap_or_else(|| prompt("SMTP username"));
            let password = prompt("SMTP password");
            let from_email = from_email.unwrap_or_else(|| prompt("From email"));
            let from_name = from_name.or_else(|| {
                let name = prompt("From name (optional, press Enter to skip)");
                if name.is_empty() { None } else { Some(name) }
            });

            let password_hex = hex::encode(password.as_bytes());
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
            .bind(&password_hex)
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
                    println!("  From:     {} <{}>", from_name.as_deref().unwrap_or(""), from_email);
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
                if auth_config.registration_enabled { "enabled" } else { "disabled" }
            );
            match &auth_config.allowed_email_domains {
                Some(d) if !d.is_empty() => println!("  Allowed domains: {}", d),
                _ => println!("  Allowed domains: {}", "(any)".dimmed()),
            }

            println!();
            println!("{}:", "OIDC".bold());
            if auth_config.oidc_enabled {
                println!("  Enabled:       {}", "✓".green());
                println!("  Issuer:        {}", auth_config.oidc_issuer_url.as_deref().unwrap_or("(not set)"));
                println!("  Client ID:     {}", auth_config.oidc_client_id.as_deref().unwrap_or("(not set)"));
                println!("  Client secret: {}", if auth_config.oidc_client_secret.is_some() { "****" } else { "(not set)" });
                println!("  Auto-register: {}", if auth_config.oidc_auto_register { "yes" } else { "no" });
            } else {
                println!("  Enabled:       {}", "✗".dimmed());
                println!("  {}", "Run `calrs config oidc` to set up SSO.".dimmed());
            }
        }
        ConfigCommands::SmtpTest { to } => {
            let smtp_config = crate::email::load_smtp_config(pool).await?;
            match smtp_config {
                Some(config) => {
                    println!("{} Sending test email to {}…", "…".dimmed(), to);
                    crate::email::send_test_email(&config, &to).await?;
                    println!("{} Test email sent!", "✓".green());
                }
                None => {
                    println!("{} No SMTP configured. Run `calrs config smtp` first.", "✗".red());
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
                    if config.registration_enabled { "enabled" } else { "disabled" }
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
                let issuer = prompt("OIDC issuer URL (e.g. https://keycloak.example.com/realms/myrealm)");
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
