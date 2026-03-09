use anyhow::Result;
use clap::Subcommand;
use colored::Colorize;
use sqlx::SqlitePool;
use tabled::{Table, Tabled};
use uuid::Uuid;

use std::io::{self, Write};

use crate::auth;
use crate::models::User;

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
}

fn prompt(label: &str) -> String {
    print!("{}: ", label);
    io::stdout().flush().unwrap();
    let mut input = String::new();
    io::stdin().read_line(&mut input).unwrap();
    input.trim().to_string()
}

fn prompt_password(label: &str) -> String {
    print!("{}: ", label);
    io::stdout().flush().unwrap();
    // TODO: use rpassword for hidden input
    let mut input = String::new();
    io::stdin().read_line(&mut input).unwrap();
    input.trim().to_string()
}

pub async fn run(pool: &SqlitePool, cmd: UserCommands) -> Result<()> {
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
            let existing_account: Option<(String,)> = sqlx::query_as(
                "SELECT id FROM accounts WHERE email = ?",
            )
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
            let result = sqlx::query("UPDATE users SET enabled = 0, updated_at = datetime('now') WHERE email = ?")
                .bind(&email)
                .execute(pool)
                .await?;
            if result.rows_affected() == 0 {
                println!("{} User not found: {}", "✗".red(), email);
            } else {
                // Also invalidate their sessions
                sqlx::query("DELETE FROM sessions WHERE user_id = (SELECT id FROM users WHERE email = ?)")
                    .bind(&email)
                    .execute(pool)
                    .await?;
                println!("{} User disabled: {}", "✓".green(), email);
            }
        }
        UserCommands::Enable { email } => {
            let result = sqlx::query("UPDATE users SET enabled = 1, updated_at = datetime('now') WHERE email = ?")
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
            let result = sqlx::query("UPDATE users SET role = 'admin', updated_at = datetime('now') WHERE email = ?")
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

            let result = sqlx::query("UPDATE users SET role = 'user', updated_at = datetime('now') WHERE email = ?")
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
            sqlx::query("UPDATE users SET password_hash = ?, updated_at = datetime('now') WHERE email = ?")
                .bind(&password_hash)
                .bind(&email)
                .execute(pool)
                .await?;

            println!("{} Password updated for {}", "✓".green(), email);
        }
    }

    Ok(())
}
