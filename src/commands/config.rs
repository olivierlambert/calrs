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
    /// Configure LDAP authentication
    Ldap {
        /// LDAP server URL (ldap:// or ldaps://)
        #[arg(long)]
        server_url: Option<String>,
        /// TLS mode: ldaps, starttls, or plain
        #[arg(long)]
        tls_mode: Option<String>,
        /// Service bind DN (leave empty for anonymous)
        #[arg(long)]
        bind_dn: Option<String>,
        /// User search base DN
        #[arg(long)]
        user_search_base: Option<String>,
        /// User filter with {username} placeholder
        #[arg(long)]
        user_filter: Option<String>,
        /// Email attribute name (default: mail)
        #[arg(long)]
        email_attr: Option<String>,
        /// Name attribute name (default: cn)
        #[arg(long)]
        name_attr: Option<String>,
        /// Groups attribute (e.g. memberOf, leave empty to skip group sync)
        #[arg(long)]
        groups_attr: Option<String>,
        /// Enable or disable LDAP
        #[arg(long)]
        enabled: Option<bool>,
        /// Auto-register users on first LDAP login
        #[arg(long)]
        auto_register: Option<bool>,
    },
    /// Test the stored LDAP configuration (connect + service bind)
    LdapTest,
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

            let password_enc = crate::crypto::encrypt_password(key, &password)?;
            let id = Uuid::new_v4().to_string();

            // SMTP is a system-wide singleton: clear any prior row before inserting.
            sqlx::query("DELETE FROM smtp_config").execute(pool).await?;

            sqlx::query(
                "INSERT INTO smtp_config (id, host, port, username, password_enc, from_email, from_name)
                 VALUES (?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&id)
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
        ConfigCommands::Ldap {
            server_url,
            tls_mode,
            bind_dn,
            user_search_base,
            user_filter,
            email_attr,
            name_attr,
            groups_attr,
            enabled,
            auto_register,
        } => {
            let any_flag = server_url.is_some()
                || tls_mode.is_some()
                || bind_dn.is_some()
                || user_search_base.is_some()
                || user_filter.is_some()
                || email_attr.is_some()
                || name_attr.is_some()
                || groups_attr.is_some()
                || enabled.is_some()
                || auto_register.is_some();

            if !any_flag {
                // Interactive flow.
                let url = prompt("LDAP server URL (ldap:// or ldaps://)");
                let mode = {
                    let m = prompt("TLS mode [ldaps/starttls/plain] (default: starttls)");
                    if m.trim().is_empty() {
                        "starttls".to_string()
                    } else {
                        m
                    }
                };
                if !matches!(mode.as_str(), "ldaps" | "starttls" | "plain") {
                    anyhow::bail!("invalid TLS mode: {}", mode);
                }
                let b_dn = prompt("Bind DN (leave empty for anonymous)");
                let b_pw = if b_dn.trim().is_empty() {
                    String::new()
                } else {
                    rpassword::prompt_password("Bind password: ")?
                };
                let base = prompt("User search base (e.g. ou=users,dc=example,dc=com)");
                let filter = {
                    let f = prompt(
                        "User filter with {username} placeholder (default: (uid={username}))",
                    );
                    if f.trim().is_empty() {
                        "(uid={username})".to_string()
                    } else {
                        f
                    }
                };
                if !filter.contains("{username}") {
                    anyhow::bail!("user filter must contain the {{username}} placeholder");
                }
                let email_a = {
                    let a = prompt("Email attribute (default: mail)");
                    if a.trim().is_empty() {
                        "mail".to_string()
                    } else {
                        a
                    }
                };
                let name_a = {
                    let a = prompt("Name attribute (default: cn)");
                    if a.trim().is_empty() {
                        "cn".to_string()
                    } else {
                        a
                    }
                };
                let groups_a = {
                    let a = prompt("Groups attribute (leave empty to skip, typical: memberOf)");
                    if a.trim().is_empty() {
                        None
                    } else {
                        Some(a)
                    }
                };
                let auto_reg = prompt("Auto-register users on first LDAP login? (y/n)");

                let b_dn_opt = if b_dn.trim().is_empty() {
                    None
                } else {
                    Some(b_dn)
                };
                let b_pw_enc = if !b_pw.is_empty() {
                    Some(crate::crypto::encrypt_password(key, &b_pw)?)
                } else {
                    None
                };
                let base_opt = if base.trim().is_empty() {
                    None
                } else {
                    Some(base)
                };

                sqlx::query(
                    "UPDATE auth_config SET ldap_enabled = 1, ldap_server_url = ?, \
                     ldap_tls_mode = ?, ldap_bind_dn = ?, ldap_bind_password = ?, \
                     ldap_user_search_base = ?, ldap_user_filter = ?, \
                     ldap_email_attr = ?, ldap_name_attr = ?, ldap_groups_attr = ?, \
                     ldap_auto_register = ?, updated_at = datetime('now') \
                     WHERE id = 'singleton'",
                )
                .bind(&url)
                .bind(&mode)
                .bind(&b_dn_opt)
                .bind(&b_pw_enc)
                .bind(&base_opt)
                .bind(&filter)
                .bind(&email_a)
                .bind(&name_a)
                .bind(&groups_a)
                .bind(auto_reg.starts_with('y') || auto_reg.starts_with('Y'))
                .execute(pool)
                .await?;

                println!("{} LDAP configured and enabled", "✓".green());
            } else {
                if let Some(url) = server_url {
                    sqlx::query("UPDATE auth_config SET ldap_server_url = ?, updated_at = datetime('now') WHERE id = 'singleton'")
                        .bind(&url)
                        .execute(pool)
                        .await?;
                    println!("{} LDAP server URL set", "✓".green());
                }
                if let Some(m) = tls_mode {
                    if !matches!(m.as_str(), "ldaps" | "starttls" | "plain") {
                        anyhow::bail!("invalid TLS mode: {}", m);
                    }
                    sqlx::query("UPDATE auth_config SET ldap_tls_mode = ?, updated_at = datetime('now') WHERE id = 'singleton'")
                        .bind(&m)
                        .execute(pool)
                        .await?;
                    println!("{} LDAP TLS mode set to {}", "✓".green(), m);
                }
                if let Some(dn) = bind_dn {
                    let dn_opt = if dn.trim().is_empty() { None } else { Some(dn) };
                    sqlx::query("UPDATE auth_config SET ldap_bind_dn = ?, updated_at = datetime('now') WHERE id = 'singleton'")
                        .bind(&dn_opt)
                        .execute(pool)
                        .await?;
                    // If a bind DN is being set/changed, prompt for its password.
                    if dn_opt.is_some() {
                        let pw = rpassword::prompt_password(
                            "Bind password (leave blank to keep current): ",
                        )?;
                        if !pw.is_empty() {
                            let enc = crate::crypto::encrypt_password(key, &pw)?;
                            sqlx::query("UPDATE auth_config SET ldap_bind_password = ?, updated_at = datetime('now') WHERE id = 'singleton'")
                                .bind(&enc)
                                .execute(pool)
                                .await?;
                            println!("{} LDAP bind password updated", "✓".green());
                        }
                    } else {
                        // Clearing bind DN also clears the password.
                        sqlx::query("UPDATE auth_config SET ldap_bind_password = NULL, updated_at = datetime('now') WHERE id = 'singleton'")
                            .execute(pool)
                            .await?;
                    }
                    println!("{} LDAP bind DN set", "✓".green());
                }
                if let Some(b) = user_search_base {
                    let b_opt = if b.trim().is_empty() { None } else { Some(b) };
                    sqlx::query("UPDATE auth_config SET ldap_user_search_base = ?, updated_at = datetime('now') WHERE id = 'singleton'")
                        .bind(&b_opt)
                        .execute(pool)
                        .await?;
                    println!("{} LDAP user search base set", "✓".green());
                }
                if let Some(f) = user_filter {
                    if !f.contains("{username}") {
                        anyhow::bail!("user filter must contain the {{username}} placeholder");
                    }
                    sqlx::query("UPDATE auth_config SET ldap_user_filter = ?, updated_at = datetime('now') WHERE id = 'singleton'")
                        .bind(&f)
                        .execute(pool)
                        .await?;
                    println!("{} LDAP user filter set", "✓".green());
                }
                if let Some(a) = email_attr {
                    sqlx::query("UPDATE auth_config SET ldap_email_attr = ?, updated_at = datetime('now') WHERE id = 'singleton'")
                        .bind(&a)
                        .execute(pool)
                        .await?;
                    println!("{} LDAP email attribute set", "✓".green());
                }
                if let Some(a) = name_attr {
                    sqlx::query("UPDATE auth_config SET ldap_name_attr = ?, updated_at = datetime('now') WHERE id = 'singleton'")
                        .bind(&a)
                        .execute(pool)
                        .await?;
                    println!("{} LDAP name attribute set", "✓".green());
                }
                if let Some(a) = groups_attr {
                    let a_opt = if a.trim().is_empty() { None } else { Some(a) };
                    sqlx::query("UPDATE auth_config SET ldap_groups_attr = ?, updated_at = datetime('now') WHERE id = 'singleton'")
                        .bind(&a_opt)
                        .execute(pool)
                        .await?;
                    println!("{} LDAP groups attribute set", "✓".green());
                }
                if let Some(en) = enabled {
                    sqlx::query("UPDATE auth_config SET ldap_enabled = ?, updated_at = datetime('now') WHERE id = 'singleton'")
                        .bind(en)
                        .execute(pool)
                        .await?;
                    println!(
                        "{} LDAP {}",
                        "✓".green(),
                        if en { "enabled" } else { "disabled" }
                    );
                }
                if let Some(ar) = auto_register {
                    sqlx::query("UPDATE auth_config SET ldap_auto_register = ?, updated_at = datetime('now') WHERE id = 'singleton'")
                        .bind(ar)
                        .execute(pool)
                        .await?;
                    println!(
                        "{} LDAP auto-register {}",
                        "✓".green(),
                        if ar { "enabled" } else { "disabled" }
                    );
                }
            }
        }
        ConfigCommands::LdapTest => {
            let config = crate::auth::get_auth_config(pool).await?;
            if !config.ldap_enabled {
                println!(
                    "{} LDAP is disabled — run `calrs config ldap --enabled true` first",
                    "!".yellow()
                );
            }
            match crate::auth::ldap_test_connection(key, &config).await {
                Ok(_) => println!("{} LDAP connection OK", "✓".green()),
                Err(e) => {
                    println!("{} LDAP test failed: {}", "✗".red(), e);
                    std::process::exit(1);
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
}
