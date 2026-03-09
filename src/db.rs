use anyhow::Result;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;
use std::path::Path;
use std::str::FromStr;

pub async fn connect(data_dir: &Path) -> Result<SqlitePool> {
    std::fs::create_dir_all(data_dir)?;
    let db_path = data_dir.join("calrs.db");
    let db_url = format!("sqlite:{}?mode=rwc", db_path.display());

    let options = SqliteConnectOptions::from_str(&db_url)?
        .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
        .foreign_keys(true);

    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(options)
        .await?;

    Ok(pool)
}

pub async fn migrate(pool: &SqlitePool) -> Result<()> {
    // Create migration tracking table
    sqlx::raw_sql(
        "CREATE TABLE IF NOT EXISTS _migrations (
            name TEXT PRIMARY KEY,
            applied_at TEXT NOT NULL DEFAULT (datetime('now'))
        )",
    )
    .execute(pool)
    .await?;

    let migrations: &[(&str, &str)] = &[
        ("001_initial", include_str!("../migrations/001_initial.sql")),
        ("002_auth", include_str!("../migrations/002_auth.sql")),
        ("003_username", include_str!("../migrations/003_username.sql")),
    ];

    for (name, sql) in migrations {
        let applied: Option<(String,)> =
            sqlx::query_as("SELECT name FROM _migrations WHERE name = ?")
                .bind(name)
                .fetch_optional(&*pool)
                .await?;

        if applied.is_none() {
            sqlx::raw_sql(sql).execute(&*pool).await?;
            sqlx::query("INSERT INTO _migrations (name) VALUES (?)")
                .bind(name)
                .execute(&*pool)
                .await?;
        }
    }

    // Migrate orphaned accounts (pre-auth) → create users and link them
    migrate_orphaned_accounts(pool).await?;

    // Generate usernames for users that don't have one
    generate_missing_usernames(pool).await?;

    Ok(())
}

/// For each account without a linked user, create a user (admin, local provider,
/// no password — must be set via `calrs user create` or web registration) and link it.
async fn migrate_orphaned_accounts(pool: &SqlitePool) -> Result<()> {
    let orphans: Vec<(String, String, String, String)> = sqlx::query_as(
        "SELECT id, name, email, timezone FROM accounts WHERE user_id IS NULL",
    )
    .fetch_all(pool)
    .await?;

    for (account_id, name, email, timezone) in orphans {
        // Check if a user with this email already exists (e.g. created via CLI)
        let existing: Option<(String,)> =
            sqlx::query_as("SELECT id FROM users WHERE email = ?")
                .bind(&email)
                .fetch_optional(&*pool)
                .await?;

        let user_id = if let Some((uid,)) = existing {
            uid
        } else {
            let uid = uuid::Uuid::new_v4().to_string();
            // First user (or only pre-existing account) gets admin
            let has_users: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM users")
                .fetch_one(&*pool)
                .await?;
            let role = if has_users.0 == 0 { "admin" } else { "user" };

            sqlx::query(
                "INSERT INTO users (id, email, name, timezone, role, auth_provider) VALUES (?, ?, ?, ?, ?, 'local')",
            )
            .bind(&uid)
            .bind(&email)
            .bind(&name)
            .bind(&timezone)
            .bind(role)
            .execute(&*pool)
            .await?;
            uid
        };

        sqlx::query("UPDATE accounts SET user_id = ? WHERE id = ?")
            .bind(&user_id)
            .bind(&account_id)
            .execute(&*pool)
            .await?;
    }

    Ok(())
}

/// Generate a username from email (local part, lowercased, dots replaced with dashes).
/// If it collides, append a number.
async fn generate_missing_usernames(pool: &SqlitePool) -> Result<()> {
    let users: Vec<(String, String)> = sqlx::query_as(
        "SELECT id, email FROM users WHERE username IS NULL",
    )
    .fetch_all(pool)
    .await?;

    for (user_id, email) in users {
        let local_part = email.split('@').next().unwrap_or("user");
        let base = local_part
            .to_lowercase()
            .replace('.', "-")
            .chars()
            .filter(|c| c.is_alphanumeric() || *c == '-')
            .collect::<String>();
        let base = if base.is_empty() { "user".to_string() } else { base };

        let mut candidate = base.clone();
        let mut suffix = 1u32;
        loop {
            let taken: Option<(String,)> =
                sqlx::query_as("SELECT id FROM users WHERE username = ?")
                    .bind(&candidate)
                    .fetch_optional(&*pool)
                    .await?;
            if taken.is_none() {
                break;
            }
            candidate = format!("{}-{}", base, suffix);
            suffix += 1;
        }

        sqlx::query("UPDATE users SET username = ? WHERE id = ?")
            .bind(&candidate)
            .bind(&user_id)
            .execute(&*pool)
            .await?;
    }

    Ok(())
}
