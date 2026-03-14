use anyhow::Result;
use colored::Colorize;
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
        (
            "003_username",
            include_str!("../migrations/003_username.sql"),
        ),
        ("004_oidc", include_str!("../migrations/004_oidc.sql")),
        (
            "005_requires_confirmation",
            include_str!("../migrations/005_requires_confirmation.sql"),
        ),
        (
            "006_group_event_types",
            include_str!("../migrations/006_group_event_types.sql"),
        ),
        (
            "007_caldav_write",
            include_str!("../migrations/007_caldav_write.sql"),
        ),
        (
            "008_recurrence_id",
            include_str!("../migrations/008_recurrence_id.sql"),
        ),
        (
            "009_uid_recurrence_unique",
            include_str!("../migrations/009_uid_recurrence_unique.sql"),
        ),
        (
            "010_confirm_token",
            include_str!("../migrations/010_confirm_token.sql"),
        ),
        (
            "011_event_type_calendars",
            include_str!("../migrations/011_event_type_calendars.sql"),
        ),
        (
            "012_reminders",
            include_str!("../migrations/012_reminders.sql"),
        ),
        (
            "013_booking_email",
            include_str!("../migrations/013_booking_email.sql"),
        ),
        (
            "014_team_links",
            include_str!("../migrations/014_team_links.sql"),
        ),
        (
            "015_user_profile",
            include_str!("../migrations/015_user_profile.sql"),
        ),
        (
            "016_booking_unique",
            include_str!("../migrations/016_booking_unique.sql"),
        ),
        (
            "017_events_per_calendar",
            include_str!("../migrations/017_events_per_calendar.sql"),
        ),
        (
            "018_private_invites",
            include_str!("../migrations/018_private_invites.sql"),
        ),
        (
            "019_team_link_reusable",
            include_str!("../migrations/019_team_link_reusable.sql"),
        ),
        (
            "020_booking_attendees",
            include_str!("../migrations/020_booking_attendees.sql"),
        ),
        (
            "021_accent_color",
            include_str!("../migrations/021_accent_color.sql"),
        ),
        ("022_theme", include_str!("../migrations/022_theme.sql")),
        (
            "023_team_link_windows",
            include_str!("../migrations/023_team_link_windows.sql"),
        ),
        (
            "024_team_link_features",
            include_str!("../migrations/024_team_link_features.sql"),
        ),
        (
            "025_reschedule_by_host",
            include_str!("../migrations/025_reschedule_by_host.sql"),
        ),
        (
            "026_visibility",
            include_str!("../migrations/026_visibility.sql"),
        ),
    ];

    let mut applied_count = 0u32;
    for (name, sql) in migrations {
        let applied: Option<(String,)> =
            sqlx::query_as("SELECT name FROM _migrations WHERE name = ?")
                .bind(name)
                .fetch_optional(pool)
                .await?;

        if applied.is_none() {
            sqlx::raw_sql(sql).execute(pool).await?;
            sqlx::query("INSERT INTO _migrations (name) VALUES (?)")
                .bind(name)
                .execute(pool)
                .await?;
            tracing::info!(migration = %name, "database migration applied");
            applied_count += 1;
        }
    }

    if applied_count == 0 {
        tracing::debug!("database migrations up to date");
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
    let orphans: Vec<(String, String, String, String)> =
        sqlx::query_as("SELECT id, name, email, timezone FROM accounts WHERE user_id IS NULL")
            .fetch_all(pool)
            .await?;

    for (account_id, name, email, timezone) in orphans {
        // Check if a user with this email already exists (e.g. created via CLI)
        let existing: Option<(String,)> = sqlx::query_as("SELECT id FROM users WHERE email = ?")
            .bind(&email)
            .fetch_optional(pool)
            .await?;

        let user_id = if let Some((uid,)) = existing {
            uid
        } else {
            let uid = uuid::Uuid::new_v4().to_string();
            // First user (or only pre-existing account) gets admin
            let has_users: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM users")
                .fetch_one(pool)
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
            .execute(pool)
            .await?;
            uid
        };

        sqlx::query("UPDATE accounts SET user_id = ? WHERE id = ?")
            .bind(&user_id)
            .bind(&account_id)
            .execute(pool)
            .await?;
    }

    Ok(())
}

/// Generate a username from email (local part, lowercased, dots replaced with dashes).
/// If it collides, append a number.
async fn generate_missing_usernames(pool: &SqlitePool) -> Result<()> {
    let users: Vec<(String, String)> =
        sqlx::query_as("SELECT id, email FROM users WHERE username IS NULL")
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
        let base = if base.is_empty() {
            "user".to_string()
        } else {
            base
        };

        let mut candidate = base.clone();
        let mut suffix = 1u32;
        loop {
            let taken: Option<(String,)> =
                sqlx::query_as("SELECT id FROM users WHERE username = ?")
                    .bind(&candidate)
                    .fetch_optional(pool)
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
            .execute(pool)
            .await?;
    }

    Ok(())
}

/// Migrate legacy hex-encoded passwords to AES-256-GCM encrypted format.
pub async fn migrate_passwords(pool: &SqlitePool, key: &[u8; 32]) -> Result<()> {
    let mut migrated = 0u32;

    // Migrate caldav_sources.password_enc
    let sources: Vec<(String, String)> = sqlx::query_as(
        "SELECT id, password_enc FROM caldav_sources WHERE password_enc IS NOT NULL",
    )
    .fetch_all(pool)
    .await?;

    for (id, stored) in &sources {
        if let Ok(Some(encrypted)) = crate::crypto::migrate_legacy(key, stored) {
            sqlx::query("UPDATE caldav_sources SET password_enc = ? WHERE id = ?")
                .bind(&encrypted)
                .bind(id)
                .execute(pool)
                .await?;
            migrated += 1;
        }
    }

    // Migrate smtp_config.password_enc
    let smtp_rows: Vec<(String, String)> =
        sqlx::query_as("SELECT id, password_enc FROM smtp_config WHERE password_enc IS NOT NULL")
            .fetch_all(pool)
            .await?;

    for (id, stored) in &smtp_rows {
        if let Ok(Some(encrypted)) = crate::crypto::migrate_legacy(key, stored) {
            sqlx::query("UPDATE smtp_config SET password_enc = ? WHERE id = ?")
                .bind(&encrypted)
                .bind(id)
                .execute(pool)
                .await?;
            migrated += 1;
        }
    }

    if migrated > 0 {
        println!(
            "{} Migrated {} credential(s) to encrypted storage.",
            "✓".green(),
            migrated
        );
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
    use std::str::FromStr;

    async fn memory_pool() -> SqlitePool {
        SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(
                SqliteConnectOptions::from_str("sqlite::memory:")
                    .unwrap()
                    .foreign_keys(true),
            )
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn migrate_creates_all_tables() {
        let pool = memory_pool().await;
        migrate(&pool).await.unwrap();

        let expected_tables = [
            "accounts",
            "caldav_sources",
            "calendars",
            "events",
            "event_types",
            "availability_rules",
            "availability_overrides",
            "bookings",
            "users",
            "sessions",
            "auth_config",
            "smtp_config",
            "groups",
            "user_groups",
            "event_type_calendars",
            "team_links",
            "team_link_members",
            "team_link_bookings",
            "booking_invites",
            "booking_attendees",
        ];

        for table in &expected_tables {
            let exists: (i64,) = sqlx::query_as(&format!(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='{}'",
                table
            ))
            .fetch_one(&pool)
            .await
            .unwrap();
            assert_eq!(
                exists.0, 1,
                "Table '{}' should exist after migration",
                table
            );
        }
    }

    #[tokio::test]
    async fn migrate_tracks_applied_migrations() {
        let pool = memory_pool().await;
        migrate(&pool).await.unwrap();

        let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM _migrations")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count.0, 26, "All 26 migrations should be tracked");
    }

    #[tokio::test]
    async fn migrate_is_idempotent() {
        let pool = memory_pool().await;
        migrate(&pool).await.unwrap();
        // Running again should not fail or double-apply
        migrate(&pool).await.unwrap();

        let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM _migrations")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count.0, 26, "Still 26 migrations after second run");
    }

    #[tokio::test]
    async fn migrate_migration_count_matches_files() {
        // This test catches the "forgot to register migration" bug.
        // Count .sql files in migrations/ dir
        let migration_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations");
        let sql_files: Vec<_> = std::fs::read_dir(&migration_dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().map_or(false, |ext| ext == "sql"))
            .collect();

        let pool = memory_pool().await;
        migrate(&pool).await.unwrap();

        let registered: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM _migrations")
            .fetch_one(&pool)
            .await
            .unwrap();

        assert_eq!(
            sql_files.len() as i64,
            registered.0,
            "Number of .sql files ({}) must match registered migrations ({}). Did you forget to register a migration in db.rs?",
            sql_files.len(),
            registered.0,
        );
    }

    #[tokio::test]
    async fn migrate_foreign_keys_work() {
        let pool = memory_pool().await;
        migrate(&pool).await.unwrap();

        // Insert a user and account
        let user_id = uuid::Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO users (id, email, name, role, auth_provider, username, enabled) VALUES (?, 'fk@test.com', 'FK Test', 'user', 'local', 'fktest', 1)")
            .bind(&user_id)
            .execute(&pool).await.unwrap();

        let account_id = uuid::Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO accounts (id, name, email, timezone, user_id) VALUES (?, 'FK Test', 'fk@test.com', 'UTC', ?)")
            .bind(&account_id)
            .bind(&user_id)
            .execute(&pool).await.unwrap();

        // Insert a caldav source referencing the account
        let source_id = uuid::Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO caldav_sources (id, account_id, name, url, username, enabled) VALUES (?, ?, 'Test', 'https://example.com', 'user', 1)")
            .bind(&source_id)
            .bind(&account_id)
            .execute(&pool).await.unwrap();

        // Deleting the account should cascade-delete the source
        sqlx::query("DELETE FROM accounts WHERE id = ?")
            .bind(&account_id)
            .execute(&pool)
            .await
            .unwrap();

        let source_exists: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM caldav_sources WHERE id = ?")
                .bind(&source_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(
            source_exists.0, 0,
            "Source should be cascade-deleted with account"
        );
    }

    #[tokio::test]
    async fn migrate_orphaned_accounts_links_to_user() {
        let pool = memory_pool().await;

        // Run migrations first to get schema
        migrate(&pool).await.unwrap();

        // Insert an orphaned account (user_id = NULL)
        let account_id = uuid::Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO accounts (id, name, email, timezone, user_id) VALUES (?, 'Orphan', 'orphan@test.com', 'UTC', NULL)")
            .bind(&account_id)
            .execute(&pool).await.unwrap();

        // Run migration again → should create user and link
        migrate(&pool).await.unwrap();

        // Account should now have a user_id
        let linked: Option<(String,)> =
            sqlx::query_as("SELECT user_id FROM accounts WHERE id = ? AND user_id IS NOT NULL")
                .bind(&account_id)
                .fetch_optional(&pool)
                .await
                .unwrap();
        assert!(
            linked.is_some(),
            "Orphaned account should be linked to a user"
        );
    }

    #[tokio::test]
    async fn generate_usernames_from_email() {
        let pool = memory_pool().await;
        migrate(&pool).await.unwrap();

        let user_id = uuid::Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO users (id, email, name, role, auth_provider, username, enabled) VALUES (?, 'john.doe@example.com', 'John', 'user', 'local', NULL, 1)")
            .bind(&user_id)
            .execute(&pool).await.unwrap();

        generate_missing_usernames(&pool).await.unwrap();

        let username: Option<(String,)> = sqlx::query_as("SELECT username FROM users WHERE id = ?")
            .bind(&user_id)
            .fetch_optional(&pool)
            .await
            .unwrap();
        assert_eq!(
            username.unwrap().0,
            "john-doe",
            "john.doe@example.com → john-doe"
        );
    }
}
