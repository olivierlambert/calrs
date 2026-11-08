use anyhow::Result;
use clap::Subcommand;
use colored::Colorize;
use sqlx::SqlitePool;
use std::path::Path;
use tabled::{Table, Tabled};
use uuid::Uuid;

use crate::auth;
use crate::models::User;
use crate::utils::{prompt, read_secret};

#[derive(Debug, Subcommand)]
pub enum UserCommands {
    /// Create a new user
    Create {
        /// User's email
        #[arg(long)]
        email: Option<String>,
        /// User's name
        #[arg(long)]
        name: Option<String>,
        /// Grant admin role
        #[arg(long)]
        admin: bool,
        /// Read the password from stdin instead of the terminal (for scripts).
        /// Needs --email and --name, since stdin then holds the password.
        #[arg(long)]
        password_stdin: bool,
    },
    /// Reset local MFA and revoke sessions (server-operator recovery)
    ResetMfa { email: String },
    /// List all users
    List,
    /// Disable a user
    Disable {
        /// User email
        email: String,
    },
    /// Enable a user
    Enable {
        /// User email
        email: String,
    },
    /// Promote a user to admin
    Promote {
        /// User email
        email: String,
    },
    /// Demote an admin to regular user
    Demote {
        /// User email
        email: String,
    },
    /// Set or reset a user's password
    SetPassword {
        /// User email
        email: String,
        /// Read the new password from stdin instead of the terminal (for scripts)
        #[arg(long)]
        password_stdin: bool,
    },
    /// Confirm that a user owns their email address, so calrs may give them a
    /// managed Exchange mailbox (global EWS impersonation). Accounts an admin
    /// creates are trusted already; self-registered ones never are until an
    /// admin runs this.
    TrustEmail {
        /// User email (any letter case)
        email: String,
    },
    /// Manage a user's time off: periods they take no bookings, on every
    /// event type they host
    TimeOff {
        #[command(subcommand)]
        command: crate::commands::time_off::TimeOffCommands,
    },
    /// Permanently delete a user and all data uniquely owned by them
    Delete {
        /// User email
        email: String,
        /// Skip the interactive confirmation prompt
        #[arg(long, short = 'y')]
        yes: bool,
    },
}

/// Provision a user's managed Exchange source and say what happened.
async fn provision_and_report(
    pool: &SqlitePool,
    cfg: &crate::ews::global::EwsGlobalConfig,
    user_id: &str,
) {
    use crate::ews::global::Provisioned;
    match crate::ews::global::provision_managed_ews_source_for_user(pool, cfg, user_id).await {
        Ok(Provisioned::Created) => println!(
            "  {} Auto-provisioned managed Exchange source.",
            "✓".green()
        ),
        Ok(Provisioned::Conflict) => println!(
            "  {} No managed Exchange source: another user's source already reads this mailbox.",
            "⚠".yellow()
        ),
        Ok(Provisioned::InvalidAddress) => println!(
            "  {} No managed Exchange source: the address names no mailbox.",
            "⚠".yellow()
        ),
        Ok(_) => {}
        Err(e) => println!(
            "  {} Failed to provision managed Exchange source: {}",
            "⚠".yellow(),
            e
        ),
    }
}

pub async fn run(
    pool: &SqlitePool,
    data_dir: &Path,
    secret_key: &[u8; 32],
    cmd: UserCommands,
) -> Result<()> {
    match cmd {
        UserCommands::TimeOff { command } => {
            crate::commands::time_off::run(pool, command).await?;
        }
        UserCommands::ResetMfa { email } => {
            crate::mfa::reset(pool, &email).await?;
            println!("MFA reset for {email}; all sessions revoked.");
        }
        UserCommands::Create {
            email,
            name,
            admin,
            password_stdin,
        } => {
            if password_stdin && (email.is_none() || name.is_none()) {
                anyhow::bail!(
                    "--password-stdin needs --email and --name: stdin holds the password"
                );
            }
            let email = email.unwrap_or_else(|| prompt("Email"));
            let name = name.unwrap_or_else(|| prompt("Name"));
            let password = read_secret("Password", password_stdin)?;

            if password.len() < 12 {
                anyhow::bail!("Password must be at least 12 characters");
            }

            let password_hash = auth::hash_password(&password)?;
            let user_id = Uuid::new_v4().to_string();
            let username = auth::generate_username(pool, &email).await?;

            let mut tx = pool.begin().await?;
            let role = auth::create_local_user(
                &mut *tx,
                &user_id,
                &email,
                &name,
                &password_hash,
                &username,
                admin,
            )
            .await?;
            auth::create_user_account(&mut tx, &user_id, &name, &email).await?;
            // An admin typed this address, so it is trusted: global EWS
            // impersonation may open the mailbox it names. Set with the row,
            // so the account never exists untrusted.
            sqlx::query("UPDATE users SET email_trusted = 1 WHERE id = ?")
                .bind(&user_id)
                .execute(&mut *tx)
                .await?;
            tx.commit().await?;

            println!(
                "{} User created: {} <{}> (role: {})",
                "✓".green(),
                name,
                email,
                role
            );

            if role == "admin" && !admin {
                println!(
                    "{}",
                    "  First user, automatically granted admin role.".dimmed()
                );
            }

            // If the admin has enabled global EWS impersonation with
            // auto-provision, give the new user a managed Exchange source.
            if let Some(cfg) = crate::ews::global::load_ews_global_config(pool, secret_key).await {
                if cfg.auto_provision {
                    provision_and_report(pool, &cfg, &user_id).await;
                }
            }
        }
        UserCommands::TrustEmail { email } => {
            let matches: Vec<(String, String)> =
                sqlx::query_as("SELECT id, email FROM users WHERE lower(email) = lower(?)")
                    .bind(email.trim())
                    .fetch_all(pool)
                    .await?;
            let (user_id, stored) = match matches.as_slice() {
                [] => anyhow::bail!("No user found with email '{}'", email),
                [one] => one.clone(),
                _ => anyhow::bail!(
                    "Several users have '{}' in some letter case; give the exact address",
                    email
                ),
            };
            // Exactly the stored address just shown, so a change in between
            // is not confirmed unseen.
            if !crate::ews::global::trust_email(pool, &user_id, &stored).await? {
                anyhow::bail!(
                    "The address of '{}' changed meanwhile; run the command again",
                    email
                );
            }
            println!("{} Address confirmed: {}", "✓".green(), stored);
            if let Some(cfg) = crate::ews::global::load_ews_global_config(pool, secret_key).await {
                provision_and_report(pool, &cfg, &user_id).await;
            }
        }
        UserCommands::List => {
            let users: Vec<User> = sqlx::query_as("SELECT * FROM users ORDER BY created_at")
                .fetch_all(pool)
                .await?;

            if users.is_empty() {
                println!("No users. Create one with `calrs user create`.");
                return Ok(());
            }

            #[derive(Tabled)]
            struct UserRow {
                email: String,
                name: String,
                role: String,
                provider: String,
                enabled: String,
                created: String,
            }

            let rows: Vec<UserRow> = users
                .iter()
                .map(|u| UserRow {
                    email: u.email.clone(),
                    name: u.name.clone(),
                    role: u.role.clone(),
                    provider: u.auth_provider.clone(),
                    enabled: if u.enabled {
                        "✓".to_string()
                    } else {
                        "✗".to_string()
                    },
                    created: u.created_at.clone(),
                })
                .collect();

            println!("{}", Table::new(rows));
        }
        UserCommands::Disable { email } => {
            let id = user_id_by_email(pool, &email).await?;
            match auth::set_user_enabled(pool, &id, false).await? {
                auth::AdminChange::NotFound => {
                    println!("{} User not found: {}", "✗".red(), email);
                }
                auth::AdminChange::LastAdmin => {
                    println!(
                        "{} Cannot disable the last enabled admin. Promote or enable another admin first.",
                        "✗".red()
                    );
                }
                auth::AdminChange::Changed => {
                    println!("{} User disabled: {}", "✓".green(), email);
                }
            }
        }
        UserCommands::Enable { email } => {
            let result = sqlx::query(
                "UPDATE users SET enabled = 1, updated_at = datetime('now') WHERE email = ?",
            )
            .bind(&email)
            .execute(pool)
            .await?;
            if result.rows_affected() == 0 {
                println!("{} User not found: {}", "✗".red(), email);
            } else {
                println!("{} User enabled: {}", "✓".green(), email);
            }
        }
        UserCommands::Promote { email } => {
            let result = sqlx::query(
                "UPDATE users SET role = 'admin', updated_at = datetime('now') WHERE email = ?",
            )
            .bind(&email)
            .execute(pool)
            .await?;
            if result.rows_affected() == 0 {
                println!("{} User not found: {}", "✗".red(), email);
            } else {
                println!("{} User promoted to admin: {}", "✓".green(), email);
            }
        }
        UserCommands::Demote { email } => {
            // Refuses to demote the last enabled admin, atomically.
            let id = user_id_by_email(pool, &email).await?;
            match auth::set_user_admin(pool, &id, false).await? {
                auth::AdminChange::NotFound => {
                    println!("{} User not found: {}", "✗".red(), email);
                }
                auth::AdminChange::LastAdmin => {
                    println!(
                        "{} Cannot demote the last admin. Promote another user first.",
                        "✗".red()
                    );
                }
                auth::AdminChange::Changed => {
                    println!("{} User demoted to user: {}", "✓".green(), email);
                }
            }
        }
        UserCommands::SetPassword {
            email,
            password_stdin,
        } => {
            let existing: Option<(String,)> =
                sqlx::query_as("SELECT id FROM users WHERE email = ?")
                    .bind(&email)
                    .fetch_optional(pool)
                    .await?;

            if existing.is_none() {
                anyhow::bail!("User not found: {}", email);
            }

            let password = read_secret("New password", password_stdin)?;
            if password.len() < 12 {
                anyhow::bail!("Password must be at least 12 characters");
            }

            let password_hash = auth::hash_password(&password)?;
            // A reset password usually means the old one leaked: sign out
            // every session it opened, in the same transaction.
            let mut tx = pool.begin().await?;
            sqlx::query(
                "UPDATE users SET password_hash = ?, updated_at = datetime('now') WHERE email = ?",
            )
            .bind(&password_hash)
            .bind(&email)
            .execute(&mut *tx)
            .await?;
            if let Some((id,)) = &existing {
                crate::mfa::revoke_access(&mut tx, id).await?;
            }
            tx.commit().await?;

            println!(
                "{} Password updated for {}; all sessions signed out.",
                "✓".green(),
                email
            );
        }
        UserCommands::Delete { email, yes } => {
            let user: Option<(String, String, String, String)> =
                sqlx::query_as("SELECT id, name, role, auth_provider FROM users WHERE email = ?")
                    .bind(&email)
                    .fetch_optional(pool)
                    .await?;
            let (target_id, target_name, _role, auth_provider) = match user {
                Some(u) => u,
                None => {
                    println!("{} User not found: {}", "✗".red(), email);
                    return Ok(());
                }
            };

            if !yes {
                println!("{} About to permanently delete:", "⚠".yellow());
                println!("    {} <{}>", target_name, email);
                println!(
                    "{}",
                    "  This removes their user record, scheduling account, calendar sources,"
                        .dimmed()
                );
                println!(
                    "{}",
                    "  event types, and all data uniquely owned by them.".dimmed()
                );
                if auth_provider == "oidc" {
                    println!(
                        "{}",
                        "  This is an OIDC/SSO user; if auto-register is enabled they will be"
                            .dimmed()
                    );
                    println!("{}", "  re-created on their next login.".dimmed());
                }
                let confirm = prompt("Type 'delete' to confirm");
                if confirm.trim() != "delete" {
                    println!("{} Aborted.", "✗".red());
                    return Ok(());
                }
            }

            let avatars_dir = data_dir.join("avatars");
            match auth::delete_user(pool, &target_id, None, Some(&avatars_dir)).await {
                Ok(()) => {
                    println!("{} User deleted: {}", "✓".green(), email);
                }
                Err(e) => {
                    println!("{} {}", "✗".red(), e);
                }
            }
        }
    }

    Ok(())
}

/// The id of the user with this email, or an empty string when there is
/// none (the guarded updates then report `NotFound`).
async fn user_id_by_email(pool: &SqlitePool, email: &str) -> Result<String> {
    let id: Option<String> = sqlx::query_scalar("SELECT id FROM users WHERE email = ?")
        .bind(email)
        .fetch_optional(pool)
        .await?;
    Ok(id.unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
    use std::str::FromStr;

    async fn setup_db() -> SqlitePool {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
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

    /// Insert a user directly (bypasses interactive prompts in UserCommands::Create)
    async fn insert_user(pool: &SqlitePool, email: &str, name: &str, role: &str) -> String {
        let user_id = Uuid::new_v4().to_string();
        let password_hash = crate::auth::hash_password("testpass123").unwrap();
        let username = crate::auth::generate_username(pool, email).await.unwrap();
        sqlx::query(
            "INSERT INTO users (id, email, name, timezone, password_hash, role, auth_provider, username, enabled)
             VALUES (?, ?, ?, 'UTC', ?, ?, 'local', ?, 1)",
        )
        .bind(&user_id)
        .bind(email)
        .bind(name)
        .bind(&password_hash)
        .bind(role)
        .bind(&username)
        .execute(pool)
        .await
        .unwrap();

        // Create a linked account
        let account_id = Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO accounts (id, name, email, timezone, user_id) VALUES (?, ?, ?, 'UTC', ?)",
        )
        .bind(&account_id)
        .bind(name)
        .bind(email)
        .bind(&user_id)
        .execute(pool)
        .await
        .unwrap();

        user_id
    }

    /// Wrapper around `auth::create_local_user` that handles the bookkeeping
    /// the production handler also handles (UUID, password hash, username).
    /// Calls the same helper the `Create` command uses, so the atomic CASE
    /// in the production code path is what's actually under test.
    async fn create_user_returning_role(pool: &SqlitePool, email: &str, admin: bool) -> String {
        let user_id = Uuid::new_v4().to_string();
        let password_hash = crate::auth::hash_password("testpass123").unwrap();
        let username = crate::auth::generate_username(pool, email).await.unwrap();
        crate::auth::create_local_user(
            pool,
            &user_id,
            email,
            "Test",
            &password_hash,
            &username,
            admin,
        )
        .await
        .unwrap()
    }

    #[tokio::test]
    async fn first_user_create_gets_admin_role_atomically() {
        let pool = setup_db().await;
        let role = create_user_returning_role(&pool, "first@test.com", false).await;
        assert_eq!(role, "admin");
    }

    #[tokio::test]
    async fn subsequent_user_create_gets_user_role() {
        let pool = setup_db().await;
        insert_user(&pool, "first@test.com", "First", "admin").await;
        let role = create_user_returning_role(&pool, "second@test.com", false).await;
        assert_eq!(role, "user");
    }

    #[tokio::test]
    async fn explicit_admin_flag_overrides_user_role() {
        // `--admin` flag set + existing users: the operator's choice wins.
        let pool = setup_db().await;
        insert_user(&pool, "first@test.com", "First", "admin").await;
        let role = create_user_returning_role(&pool, "second@test.com", true).await;
        assert_eq!(role, "admin");
    }

    #[tokio::test]
    async fn test_list_users_empty() {
        let pool = setup_db().await;
        let result = run(&pool, &std::env::temp_dir(), &[0u8; 32], UserCommands::List).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_list_users_with_data() {
        let pool = setup_db().await;
        insert_user(&pool, "alice@test.com", "Alice", "admin").await;
        insert_user(&pool, "bob@test.com", "Bob", "user").await;

        let result = run(&pool, &std::env::temp_dir(), &[0u8; 32], UserCommands::List).await;
        assert!(result.is_ok());

        let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM users")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count.0, 2);
    }

    #[tokio::test]
    async fn test_promote_user() {
        let pool = setup_db().await;
        insert_user(&pool, "alice@test.com", "Alice", "admin").await;
        insert_user(&pool, "bob@test.com", "Bob", "user").await;

        let result = run(
            &pool,
            &std::env::temp_dir(),
            &[0u8; 32],
            UserCommands::Promote {
                email: "bob@test.com".to_string(),
            },
        )
        .await;
        assert!(result.is_ok());

        let role: (String,) = sqlx::query_as("SELECT role FROM users WHERE email = 'bob@test.com'")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(role.0, "admin");
    }

    #[tokio::test]
    async fn test_demote_user() {
        let pool = setup_db().await;
        insert_user(&pool, "alice@test.com", "Alice", "admin").await;
        insert_user(&pool, "bob@test.com", "Bob", "admin").await;

        let result = run(
            &pool,
            &std::env::temp_dir(),
            &[0u8; 32],
            UserCommands::Demote {
                email: "bob@test.com".to_string(),
            },
        )
        .await;
        assert!(result.is_ok());

        let role: (String,) = sqlx::query_as("SELECT role FROM users WHERE email = 'bob@test.com'")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(role.0, "user");
    }

    #[tokio::test]
    async fn test_demote_last_admin_prevented() {
        let pool = setup_db().await;
        insert_user(&pool, "alice@test.com", "Alice", "admin").await;

        // Attempting to demote the only admin should not change the role
        let result = run(
            &pool,
            &std::env::temp_dir(),
            &[0u8; 32],
            UserCommands::Demote {
                email: "alice@test.com".to_string(),
            },
        )
        .await;
        assert!(result.is_ok());

        let role: (String,) =
            sqlx::query_as("SELECT role FROM users WHERE email = 'alice@test.com'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(role.0, "admin", "Last admin should not be demoted");
    }

    #[tokio::test]
    async fn test_disable_user() {
        let pool = setup_db().await;
        insert_user(&pool, "bob@test.com", "Bob", "user").await;

        let result = run(
            &pool,
            &std::env::temp_dir(),
            &[0u8; 32],
            UserCommands::Disable {
                email: "bob@test.com".to_string(),
            },
        )
        .await;
        assert!(result.is_ok());

        let enabled: (bool,) =
            sqlx::query_as("SELECT enabled FROM users WHERE email = 'bob@test.com'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert!(!enabled.0, "User should be disabled");
    }

    #[tokio::test]
    async fn test_enable_user() {
        let pool = setup_db().await;
        insert_user(&pool, "bob@test.com", "Bob", "user").await;

        // Disable first
        run(
            &pool,
            &std::env::temp_dir(),
            &[0u8; 32],
            UserCommands::Disable {
                email: "bob@test.com".to_string(),
            },
        )
        .await
        .unwrap();

        // Then re-enable
        let result = run(
            &pool,
            &std::env::temp_dir(),
            &[0u8; 32],
            UserCommands::Enable {
                email: "bob@test.com".to_string(),
            },
        )
        .await;
        assert!(result.is_ok());

        let enabled: (bool,) =
            sqlx::query_as("SELECT enabled FROM users WHERE email = 'bob@test.com'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert!(enabled.0, "User should be enabled");
    }

    #[tokio::test]
    async fn test_disable_nonexistent_user() {
        let pool = setup_db().await;
        // Should succeed (no error), just print "not found"
        let result = run(
            &pool,
            &std::env::temp_dir(),
            &[0u8; 32],
            UserCommands::Disable {
                email: "nobody@test.com".to_string(),
            },
        )
        .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_promote_nonexistent_user() {
        let pool = setup_db().await;
        let result = run(
            &pool,
            &std::env::temp_dir(),
            &[0u8; 32],
            UserCommands::Promote {
                email: "nobody@test.com".to_string(),
            },
        )
        .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_delete_user_with_yes_flag() {
        let pool = setup_db().await;
        // Two admins so the LastAdmin guard doesn't fire on Bob.
        insert_user(&pool, "alice@test.com", "Alice", "admin").await;
        insert_user(&pool, "bob@test.com", "Bob", "admin").await;

        let result = run(
            &pool,
            &std::env::temp_dir(),
            &[0u8; 32],
            UserCommands::Delete {
                email: "bob@test.com".to_string(),
                yes: true,
            },
        )
        .await;
        assert!(result.is_ok());

        let bob_count: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM users WHERE email = 'bob@test.com'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(bob_count.0, 0, "Bob should be gone");
    }

    #[tokio::test]
    async fn test_delete_nonexistent_user() {
        let pool = setup_db().await;
        // Should not error; just print "not found".
        let result = run(
            &pool,
            &std::env::temp_dir(),
            &[0u8; 32],
            UserCommands::Delete {
                email: "ghost@test.com".to_string(),
                yes: true,
            },
        )
        .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_disable_invalidates_sessions() {
        let pool = setup_db().await;
        let user_id = insert_user(&pool, "bob@test.com", "Bob", "user").await;

        // Create a session for Bob
        let session_id = Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO sessions (id, user_id, expires_at) VALUES (?, ?, datetime('now', '+30 days'))",
        )
        .bind(&session_id)
        .bind(&user_id)
        .execute(&pool)
        .await
        .unwrap();

        // Disable Bob
        run(
            &pool,
            &std::env::temp_dir(),
            &[0u8; 32],
            UserCommands::Disable {
                email: "bob@test.com".to_string(),
            },
        )
        .await
        .unwrap();

        // Session should be deleted
        let session_count: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM sessions WHERE user_id = ?")
                .bind(&user_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(session_count.0, 0, "Sessions should be deleted on disable");
    }

    /// `calrs user trust-email` is the admin's confirmation that a person owns
    /// their address: it finds the user in any letter case, and refuses an
    /// unknown one with an error (a non-zero exit status).
    #[tokio::test]
    async fn trust_email_confirms_a_known_user_and_refuses_an_unknown_one() {
        let pool = setup_db().await;
        let id = insert_user(&pool, "bob@test.com", "Bob", "user").await;
        let trusted = |pool: SqlitePool, id: String| async move {
            sqlx::query_scalar::<_, bool>("SELECT email_trusted FROM users WHERE id = ?")
                .bind(&id)
                .fetch_one(&pool)
                .await
                .unwrap()
        };
        assert!(!trusted(pool.clone(), id.clone()).await);

        let unknown = run(
            &pool,
            &std::env::temp_dir(),
            &[0u8; 32],
            UserCommands::TrustEmail {
                email: "nobody@test.com".to_string(),
            },
        )
        .await;
        assert!(unknown.is_err());

        run(
            &pool,
            &std::env::temp_dir(),
            &[0u8; 32],
            UserCommands::TrustEmail {
                email: "Bob@Test.com".to_string(),
            },
        )
        .await
        .unwrap();
        assert!(trusted(pool.clone(), id.clone()).await);
    }
}
