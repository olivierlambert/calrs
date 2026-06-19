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

// Note: oidc_client_secret, captcha_secret, google_oauth2_client_secret and
// meeting_webhook_secret are deliberately excluded (secrets).
#[derive(serde::Serialize, sqlx::FromRow)]
#[serde(rename_all = "snake_case")]
struct AuthConfigDump {
    registration_enabled: bool,
    allowed_email_domains: Option<String>,
    oidc_enabled: bool,
    oidc_issuer_url: Option<String>,
    oidc_client_id: Option<String>,
    oidc_auto_register: bool,
    accent_color: String,
    theme: String,
    custom_accent: Option<String>,
    custom_accent_hover: Option<String>,
    custom_bg: Option<String>,
    custom_surface: Option<String>,
    custom_text: Option<String>,
    company_link: Option<String>,
    captcha_instance_url: Option<String>,
    captcha_site_key: Option<String>,
    captcha_widget_url: Option<String>,
    google_oauth2_client_id: Option<String>,
    jitsi_base_url: Option<String>,
    jitsi_pattern: Option<String>,
    jitsi_display_name: Option<String>,
    meeting_webhook_url: Option<String>,
    meeting_webhook_auth_mode: Option<String>,
    meeting_webhook_display_name: Option<String>,
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
    tls_mode: String,
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

// Note: password_enc, access_token_enc and refresh_token_enc are deliberately
// excluded (secrets). last_synced / sync_token / last_full_sync /
// token_expires_at are excluded too — they are operational sync state, not
// configuration, and must not be transplanted by a future `config import`
// onto a fresh deployment.
#[derive(serde::Serialize, sqlx::FromRow)]
#[serde(rename_all = "snake_case")]
struct CaldavSourceDump {
    id: String,
    account_id: String,
    name: String,
    url: String,
    username: String,
    provider_type: String,
    auth_type: String,
    oauth2_provider: Option<String>,
    write_calendar_href: Option<String>,
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
    slot_interval_min: Option<i32>,
    timezone: Option<String>,
    cancel_notice_min: Option<i32>,
    reschedule_notice_min: Option<i32>,
    meeting_pattern_override: Option<String>,
    team_id: Option<String>,
    group_id: Option<String>,
    created_by_user_id: Option<String>,
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

#[derive(serde::Serialize, sqlx::FromRow)]
#[serde(rename_all = "snake_case")]
struct AccountDump {
    id: String,
    user_id: Option<String>,
    name: String,
    email: String,
    timezone: String,
    created_at: String,
}

#[derive(serde::Serialize, sqlx::FromRow)]
#[serde(rename_all = "snake_case")]
struct AvailabilityRuleDump {
    id: String,
    event_type_id: String,
    day_of_week: i32,
    start_time: String,
    end_time: String,
}

#[derive(serde::Serialize, sqlx::FromRow)]
#[serde(rename_all = "snake_case")]
struct AvailabilityOverrideDump {
    id: String,
    event_type_id: String,
    date: String,
    start_time: Option<String>,
    end_time: Option<String>,
    is_blocked: bool,
}

#[derive(serde::Serialize, sqlx::FromRow)]
#[serde(rename_all = "snake_case")]
struct UserAvailabilityRuleDump {
    id: String,
    user_id: String,
    day_of_week: i32,
    start_time: String,
    end_time: String,
}

#[derive(serde::Serialize, sqlx::FromRow)]
#[serde(rename_all = "snake_case")]
struct EventTypeCalendarDump {
    event_type_id: String,
    calendar_id: String,
}

#[derive(serde::Serialize, sqlx::FromRow)]
#[serde(rename_all = "snake_case")]
struct EventTypeMemberWeightDump {
    event_type_id: String,
    user_id: String,
    weight: i32,
}

#[derive(serde::Serialize, sqlx::FromRow)]
#[serde(rename_all = "snake_case")]
struct EventTypeWatcherDump {
    event_type_id: String,
    team_id: String,
}

#[derive(serde::Serialize, sqlx::FromRow)]
#[serde(rename_all = "snake_case")]
struct TeamMemberDump {
    team_id: String,
    user_id: String,
    role: String,
    source: String,
}

#[derive(serde::Serialize, sqlx::FromRow)]
#[serde(rename_all = "snake_case")]
struct TeamGroupDump {
    team_id: String,
    group_id: String,
}

#[derive(serde::Serialize, sqlx::FromRow)]
#[serde(rename_all = "snake_case")]
struct UserGroupDump {
    user_id: String,
    group_id: String,
    weight: i32,
}

#[derive(serde::Serialize, sqlx::FromRow)]
#[serde(rename_all = "snake_case")]
struct BookingFrequencyLimitDump {
    id: String,
    event_type_id: String,
    max_bookings: i32,
    period: String,
    per_member: bool,
}

/// Fetch all rows of a dump struct and serialize them as a JSON array.
async fn dump_table<T>(pool: &SqlitePool, sql: &str) -> Result<serde_json::Value>
where
    T: for<'r> sqlx::FromRow<'r, sqlx::sqlite::SqliteRow> + serde::Serialize + Send + Unpin,
{
    let rows: Vec<T> = sqlx::query_as::<_, T>(sql).fetch_all(pool).await?;
    Ok(serde_json::to_value(&rows)?)
}

/// Build the full dump output as a JSON value, querying each config section.
async fn build_dump_output(pool: &SqlitePool) -> Result<serde_json::Value> {
    // Auth config — SELECT only non-secret columns (omit oidc_client_secret)
    let auth_json = sqlx::query_as::<_, AuthConfigDump>(
        "SELECT registration_enabled, allowed_email_domains, oidc_enabled, \
         oidc_issuer_url, oidc_client_id, oidc_auto_register, \
         accent_color, theme, custom_accent, custom_accent_hover, custom_bg, \
         custom_surface, custom_text, company_link, \
         captcha_instance_url, captcha_site_key, captcha_widget_url, \
         google_oauth2_client_id, \
         jitsi_base_url, jitsi_pattern, jitsi_display_name, \
         meeting_webhook_url, meeting_webhook_auth_mode, meeting_webhook_display_name, \
         created_at, updated_at \
         FROM auth_config WHERE id = 'singleton'",
    )
    .fetch_one(pool)
    .await?;
    let auth_json = serde_json::to_value(&auth_json)?;

    // SMTP config — skip password_enc (secret)
    let smtp_json: serde_json::Value = match sqlx::query_as::<_, SmtpConfigDump>(
        "SELECT id, host, port, username, from_email, from_name, tls_mode, enabled \
         FROM smtp_config LIMIT 1",
    )
    .fetch_optional(pool)
    .await?
    {
        Some(row) => serde_json::to_value(&row)?,
        None => serde_json::Value::Null,
    };

    // Accounts — scheduling profiles linked to users
    let accounts_json = dump_table::<AccountDump>(
        pool,
        "SELECT id, user_id, name, email, timezone, created_at \
         FROM accounts ORDER BY email",
    )
    .await?;

    // Users — all users including disabled ones (the `enabled` flag is part of
    // the configuration); skip password_hash (secret)
    let users_json = dump_table::<UserDump>(
        pool,
        "SELECT id, email, name, timezone, role, auth_provider, \
         oidc_subject, enabled, created_at, username, booking_email, \
         title, bio, allow_dynamic_group, language \
         FROM users ORDER BY email",
    )
    .await?;

    // Per-user default working hours
    let user_availability_rules_json = dump_table::<UserAvailabilityRuleDump>(
        pool,
        "SELECT id, user_id, day_of_week, start_time, end_time \
         FROM user_availability_rules ORDER BY user_id, day_of_week, start_time",
    )
    .await?;

    // OIDC group membership + round-robin weight
    let user_groups_json = dump_table::<UserGroupDump>(
        pool,
        "SELECT user_id, group_id, weight FROM user_groups ORDER BY group_id, user_id",
    )
    .await?;

    // CalDAV/EWS sources — skip credentials (secrets) and sync state (operational)
    let caldav_sources_json = dump_table::<CaldavSourceDump>(
        pool,
        "SELECT id, account_id, name, url, username, \
         provider_type, auth_type, oauth2_provider, \
         write_calendar_href, enabled, created_at \
         FROM caldav_sources ORDER BY name",
    )
    .await?;

    // Event types — selected config fields, no operational data
    let event_types_json = dump_table::<EventTypeDump>(
        pool,
        "SELECT id, account_id, slug, title, duration_min, location_type, \
         location_value, buffer_before, buffer_after, min_notice_min, enabled, \
         requires_confirmation, reminder_minutes, max_additional_guests, \
         scheduling_mode, first_slot_only, default_calendar_view, visibility, \
         slot_interval_min, timezone, cancel_notice_min, reschedule_notice_min, \
         meeting_pattern_override, team_id, group_id, created_by_user_id, \
         created_at, description \
         FROM event_types ORDER BY title",
    )
    .await?;

    // Weekly availability windows per event type
    let availability_rules_json = dump_table::<AvailabilityRuleDump>(
        pool,
        "SELECT id, event_type_id, day_of_week, start_time, end_time \
         FROM availability_rules ORDER BY event_type_id, day_of_week, start_time",
    )
    .await?;

    // Date-specific exceptions per event type
    let availability_overrides_json = dump_table::<AvailabilityOverrideDump>(
        pool,
        "SELECT id, event_type_id, date, start_time, end_time, is_blocked \
         FROM availability_overrides ORDER BY event_type_id, date",
    )
    .await?;

    // Per-event-type calendar selection
    let event_type_calendars_json = dump_table::<EventTypeCalendarDump>(
        pool,
        "SELECT event_type_id, calendar_id \
         FROM event_type_calendars ORDER BY event_type_id, calendar_id",
    )
    .await?;

    // Per-event-type round-robin member weights
    let event_type_member_weights_json = dump_table::<EventTypeMemberWeightDump>(
        pool,
        "SELECT event_type_id, user_id, weight \
         FROM event_type_member_weights ORDER BY event_type_id, user_id",
    )
    .await?;

    // Teams watching an event type for booking claims
    let event_type_watchers_json = dump_table::<EventTypeWatcherDump>(
        pool,
        "SELECT event_type_id, team_id \
         FROM event_type_watchers ORDER BY event_type_id, team_id",
    )
    .await?;

    // Per-event-type booking caps
    let booking_frequency_limits_json = dump_table::<BookingFrequencyLimitDump>(
        pool,
        "SELECT id, event_type_id, max_bookings, period, per_member \
         FROM booking_frequency_limits ORDER BY event_type_id",
    )
    .await?;

    // Teams — skip invite_token (secret: grants access to private teams)
    let teams_json = dump_table::<TeamDump>(
        pool,
        "SELECT id, name, slug, description, avatar_path, \
         visibility, created_at \
         FROM teams ORDER BY name",
    )
    .await?;

    // Team composition (direct members and OIDC-synced members)
    let team_members_json = dump_table::<TeamMemberDump>(
        pool,
        "SELECT team_id, user_id, role, source FROM team_members ORDER BY team_id, user_id",
    )
    .await?;

    // OIDC group linkage for automatic member sync
    let team_groups_json = dump_table::<TeamGroupDump>(
        pool,
        "SELECT team_id, group_id FROM team_groups ORDER BY team_id, group_id",
    )
    .await?;

    // Groups (legacy — kept for OIDC identity sync)
    let groups_json = dump_table::<GroupDump>(
        pool,
        "SELECT id, name, source, oidc_id, created_at, \
         slug, description, avatar_path \
         FROM groups ORDER BY name",
    )
    .await?;

    Ok(serde_json::json!({
        "schema_version": 1,
        "auth": auth_json,
        "smtp": smtp_json,
        "accounts": accounts_json,
        "users": users_json,
        "user_availability_rules": user_availability_rules_json,
        "user_groups": user_groups_json,
        "caldav_sources": caldav_sources_json,
        "event_types": event_types_json,
        "availability_rules": availability_rules_json,
        "availability_overrides": availability_overrides_json,
        "event_type_calendars": event_type_calendars_json,
        "event_type_member_weights": event_type_member_weights_json,
        "event_type_watchers": event_type_watchers_json,
        "booking_frequency_limits": booking_frequency_limits_json,
        "teams": teams_json,
        "team_members": team_members_json,
        "team_groups": team_groups_json,
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
            // Go through `load_smtp_status` so the env block (CALRS_SMTP_*) is
            // reflected here too — a direct SELECT would be blind to it and
            // misleadingly report "No SMTP configured" when env is in use.
            match crate::email::load_smtp_status(pool).await? {
                Some(status) => {
                    println!("{}:", "SMTP".bold());
                    print!("  Host:     {}:{}", status.host, status.port);
                    if status.from_env {
                        println!(" {}", "(via environment)".dimmed());
                    } else {
                        println!();
                    }
                    println!("  Username: {}", status.username);
                    if let Some(from_name) = status.from_name.as_deref() {
                        println!("  From:     {} <{}>", from_name, status.from_email);
                    } else {
                        println!("  From:     {}", status.from_email);
                    }
                    println!("  TLS mode: {}", status.tls_mode);
                    println!("  Enabled:  {}", if status.enabled { "✓" } else { "✗" });
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
        assert_eq!(output["schema_version"], 1);
        assert!(output["auth"]["registration_enabled"].as_bool().unwrap());
        // Secrets must never appear in the auth section
        let auth = output["auth"].as_object().unwrap();
        for secret in [
            "oidc_client_secret",
            "captcha_secret",
            "google_oauth2_client_secret",
            "meeting_webhook_secret",
        ] {
            assert!(
                !auth.contains_key(secret),
                "auth section must not contain '{}'",
                secret
            );
        }
        assert!(output["smtp"].is_null());
        for section in [
            "accounts",
            "users",
            "user_availability_rules",
            "user_groups",
            "caldav_sources",
            "event_types",
            "availability_rules",
            "availability_overrides",
            "event_type_calendars",
            "event_type_member_weights",
            "event_type_watchers",
            "booking_frequency_limits",
            "teams",
            "team_members",
            "team_groups",
            "groups",
        ] {
            assert!(
                output[section].as_array().unwrap().is_empty(),
                "section '{}' should be an empty array",
                section
            );
        }
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
        sqlx::query("INSERT INTO accounts (id, name, email) VALUES (?, 'Alice', 'alice@test.com')")
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
        sqlx::query("INSERT INTO groups (id, name, source) VALUES (?, 'Engineering', 'local')")
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

        // A disabled user is still configuration — it should appear, flagged disabled
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
        let users = output["users"].as_array().unwrap();
        assert_eq!(users.len(), 2, "disabled user should appear in the dump");
        let bob = users
            .iter()
            .find(|u| u["email"] == "bob@test.com")
            .expect("bob should be in the dump");
        assert!(!bob["enabled"].as_bool().unwrap());
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
        let source = sources[0].as_object().unwrap();
        assert!(!source.contains_key("password_enc"));
        // Operational sync state is excluded too
        assert!(!source.contains_key("last_synced"));
        assert!(!source.contains_key("sync_token"));
    }

    #[tokio::test]
    async fn config_dump_relational_sections() {
        let pool = setup_db().await;

        let user_id = Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO users (id, email, name, role, auth_provider) \
             VALUES (?, 'host@test.com', 'Host', 'user', 'local')",
        )
        .bind(&user_id)
        .execute(&pool)
        .await
        .unwrap();
        let account_id = Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO accounts (id, name, email, user_id) VALUES (?, 'Host', 'host@test.com', ?)",
        )
        .bind(&account_id)
        .bind(&user_id)
        .execute(&pool)
        .await
        .unwrap();
        let et_id = Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO event_types (id, account_id, slug, title, duration_min) \
             VALUES (?, ?, 'demo', 'Demo', 45)",
        )
        .bind(&et_id)
        .bind(&account_id)
        .execute(&pool)
        .await
        .unwrap();

        // Weekly rule, date override, per-user hours, frequency limit
        sqlx::query(
            "INSERT INTO availability_rules (id, event_type_id, day_of_week, start_time, end_time) \
             VALUES (?, ?, 1, '09:00', '17:00')",
        )
        .bind(Uuid::new_v4().to_string())
        .bind(&et_id)
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO availability_overrides (id, event_type_id, date, is_blocked) \
             VALUES (?, ?, '2026-12-25', 1)",
        )
        .bind(Uuid::new_v4().to_string())
        .bind(&et_id)
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO user_availability_rules (id, user_id, day_of_week, start_time, end_time) \
             VALUES (?, ?, 2, '10:00', '16:00')",
        )
        .bind(Uuid::new_v4().to_string())
        .bind(&user_id)
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO booking_frequency_limits (id, event_type_id, max_bookings, period) \
             VALUES (?, ?, 3, 'day')",
        )
        .bind(Uuid::new_v4().to_string())
        .bind(&et_id)
        .execute(&pool)
        .await
        .unwrap();

        let output = build_dump_output(&pool).await.unwrap();
        assert_eq!(output["accounts"].as_array().unwrap().len(), 1);
        assert_eq!(output["availability_rules"].as_array().unwrap().len(), 1);
        assert_eq!(output["availability_rules"][0]["day_of_week"], 1);
        assert_eq!(
            output["availability_overrides"].as_array().unwrap().len(),
            1
        );
        assert!(output["availability_overrides"][0]["is_blocked"]
            .as_bool()
            .unwrap());
        assert_eq!(
            output["user_availability_rules"].as_array().unwrap().len(),
            1
        );
        assert_eq!(
            output["booking_frequency_limits"].as_array().unwrap().len(),
            1
        );
        assert_eq!(output["booking_frequency_limits"][0]["period"], "day");
    }
}
