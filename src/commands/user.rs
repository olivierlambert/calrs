use anyhow::Result;
use clap::Subcommand;
use colored::Colorize;
use sqlx::SqlitePool;
use std::path::Path;
use tabled::{Table, Tabled};
use uuid::Uuid;

use crate::auth;
use crate::models::User;
use crate::utils::{prompt, prompt_password};

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
    },
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

pub async fn run(pool: &SqlitePool, data_dir: &Path, cmd: UserCommands) -> Result<()> {
    match cmd {
        UserCommands::Create { email, name, admin } => {
            let email = email.unwrap_or_else(|| prompt("Email"));
            let name = name.unwrap_or_else(|| prompt("Name"));
            let password = prompt_password("Password");

            if password.len() < 8 {
                anyhow::bail!("Password must be at least 8 characters");
            }

            // First user is always admin
            let is_first = !auth::has_any_users(pool).await?;
            let role = if admin || is_first { "admin" } else { "user" };

            let password_hash = auth::hash_password(&password)?;
            let user_id = Uuid::new_v4().to_string();
            let username = auth::generate_username(pool, &email).await?;

            sqlx::query(
                "INSERT INTO users (id, email, name, timezone, password_hash, role, auth_provider, username) VALUES (?, ?, ?, 'UTC', ?, ?, 'local', ?)",
            )
            .bind(&user_id)
            .bind(&email)
            .bind(&name)
            .bind(&password_hash)
            .bind(role)
            .bind(&username)
            .execute(pool)
            .await?;

            // Link to existing account (e.g. from old `calrs init`) or create a new one
            let existing_account: Option<(String,)> =
                sqlx::query_as("SELECT id FROM accounts WHERE email = ?")
                    .bind(&email)
                    .fetch_optional(pool)
                    .await?;

            if let Some((account_id,)) = existing_account {
                // Link the existing account to this user
                sqlx::query("UPDATE accounts SET user_id = ?, name = ? WHERE id = ?")
                    .bind(&user_id)
                    .bind(&name)
                    .bind(&account_id)
                    .execute(pool)
                    .await?;
            } else {
                let account_id = Uuid::new_v4().to_string();
                sqlx::query(
                    "INSERT INTO accounts (id, name, email, timezone, user_id) VALUES (?, ?, ?, 'UTC', ?)",
                )
                .bind(&account_id)
                .bind(&name)
                .bind(&email)
                .bind(&user_id)
                .execute(pool)
                .await?;
            }

            println!(
                "{} User created: {} <{}> (role: {})",
                "✓".green(),
                name,
                email,
                role
            );

            if is_first {
                println!(
                    "{}",
                    "  First user — automatically granted admin role.".dimmed()
                );
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
            let result = sqlx::query(
                "UPDATE users SET enabled = 0, updated_at = datetime('now') WHERE email = ?",
            )
            .bind(&email)
            .execute(pool)
            .await?;
            if result.rows_affected() == 0 {
                println!("{} User not found: {}", "✗".red(), email);
            } else {
                // Also invalidate their sessions
                sqlx::query(
                    "DELETE FROM sessions WHERE user_id = (SELECT id FROM users WHERE email = ?)",
                )
                .bind(&email)
                .execute(pool)
                .await?;
                println!("{} User disabled: {}", "✓".green(), email);
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
            // Prevent demoting the last admin
            let admin_count: (i64,) =
                sqlx::query_as("SELECT COUNT(*) FROM users WHERE role = 'admin' AND enabled = 1")
                    .fetch_one(pool)
                    .await?;

            let is_admin: Option<(String,)> =
                sqlx::query_as("SELECT role FROM users WHERE email = ? AND role = 'admin'")
                    .bind(&email)
                    .fetch_optional(pool)
                    .await?;

            if is_admin.is_some() && admin_count.0 <= 1 {
                println!(
                    "{} Cannot demote the last admin. Promote another user first.",
                    "✗".red()
                );
                return Ok(());
            }

            let result = sqlx::query(
                "UPDATE users SET role = 'user', updated_at = datetime('now') WHERE email = ?",
            )
            .bind(&email)
            .execute(pool)
            .await?;
            if result.rows_affected() == 0 {
                println!("{} User not found: {}", "✗".red(), email);
            } else {
                println!("{} User demoted to user: {}", "✓".green(), email);
            }
        }
        UserCommands::SetPassword { email } => {
            let existing: Option<(String,)> =
                sqlx::query_as("SELECT id FROM users WHERE email = ?")
                    .bind(&email)
                    .fetch_optional(pool)
                    .await?;

            if existing.is_none() {
                println!("{} User not found: {}", "✗".red(), email);
                return Ok(());
            }

            let password = prompt_password("New password");
            if password.len() < 8 {
                anyhow::bail!("Password must be at least 8 characters");
            }

            let password_hash = auth::hash_password(&password)?;
            sqlx::query(
                "UPDATE users SET password_hash = ?, updated_at = datetime('now') WHERE email = ?",
            )
            .bind(&password_hash)
            .bind(&email)
            .execute(pool)
            .await?;

            println!("{} Password updated for {}", "✓".green(), email);
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

    #[tokio::test]
    async fn test_list_users_empty() {
        let pool = setup_db().await;
        let result = run(&pool, &std::env::temp_dir(), UserCommands::List).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_list_users_with_data() {
        let pool = setup_db().await;
        insert_user(&pool, "alice@test.com", "Alice", "admin").await;
        insert_user(&pool, "bob@test.com", "Bob", "user").await;

        let result = run(&pool, &std::env::temp_dir(), UserCommands::List).await;
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
}
