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
        (
            "027_calendar_sync_token",
            include_str!("../migrations/027_calendar_sync_token.sql"),
        ),
        (
            "028_company_link",
            include_str!("../migrations/028_company_link.sql"),
        ),
        (
            "029_scheduling_mode",
            include_str!("../migrations/029_scheduling_mode.sql"),
        ),
        (
            "030_member_weight",
            include_str!("../migrations/030_member_weight.sql"),
        ),
        (
            "031_fix_legacy_timezones",
            include_str!("../migrations/031_fix_legacy_timezones.sql"),
        ),
        (
            "032_event_type_member_weights",
            include_str!("../migrations/032_event_type_member_weights.sql"),
        ),
        (
            "033_group_profile",
            include_str!("../migrations/033_group_profile.sql"),
        ),
        ("034_teams", include_str!("../migrations/034_teams.sql")),
        (
            "035_drop_legacy_team_links",
            include_str!("../migrations/035_drop_legacy_team_links.sql"),
        ),
        (
            "036_default_calendar_view",
            include_str!("../migrations/036_default_calendar_view.sql"),
        ),
        (
            "037_booking_frequency_limits",
            include_str!("../migrations/037_booking_frequency_limits.sql"),
        ),
        (
            "038_first_slot_only",
            include_str!("../migrations/038_first_slot_only.sql"),
        ),
        (
            "039_allow_dynamic_group",
            include_str!("../migrations/039_allow_dynamic_group.sql"),
        ),
        (
            "040_user_availability",
            include_str!("../migrations/040_user_availability.sql"),
        ),
        (
            "041_last_full_sync",
            include_str!("../migrations/041_last_full_sync.sql"),
        ),
        (
            "042_event_transp",
            include_str!("../migrations/042_event_transp.sql"),
        ),
        (
            "043_event_type_watchers",
            include_str!("../migrations/043_event_type_watchers.sql"),
        ),
        (
            "044_booking_claim",
            include_str!("../migrations/044_booking_claim.sql"),
        ),
        (
            "045_slot_interval",
            include_str!("../migrations/045_slot_interval.sql"),
        ),
        ("046_ldap", include_str!("../migrations/046_ldap.sql")),
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

    // Migrate team links → event types + bookings (after migration 034)
    migrate_team_links_to_teams(pool).await?;

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

/// Create event types and migrate bookings for team links that were converted to teams.
/// The SQL migration (034) creates the team rows and members, but team links need
/// event_type rows (they didn't have any) and their bookings need to move to `bookings`.
async fn migrate_team_links_to_teams(pool: &SqlitePool) -> Result<()> {
    // Check if team_links table exists (might not if fresh install)
    let has_tl: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='team_links'",
    )
    .fetch_one(pool)
    .await?;
    if has_tl.0 == 0 {
        return Ok(());
    }

    // Check if teams table exists (migration 034 must have run)
    let has_teams: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='teams'")
            .fetch_one(pool)
            .await?;
    if has_teams.0 == 0 {
        return Ok(());
    }

    // Fix teams with NULL slugs (from old migration that didn't generate slugs)
    let null_slug_teams: Vec<(String, String)> =
        sqlx::query_as("SELECT id, name FROM teams WHERE slug IS NULL")
            .fetch_all(pool)
            .await?;
    for (team_id, team_name) in &null_slug_teams {
        let slug = team_name
            .to_lowercase()
            .chars()
            .map(|c| {
                if c.is_alphanumeric() || c == '-' {
                    c
                } else {
                    '-'
                }
            })
            .collect::<String>()
            .trim_matches('-')
            .to_string();
        let slug = if slug.is_empty() {
            format!("team-{}", &team_id[..8.min(team_id.len())])
        } else {
            slug
        };
        // Use a unique suffix if slug already taken
        let existing: Option<(String,)> =
            sqlx::query_as("SELECT id FROM teams WHERE slug = ? AND id != ?")
                .bind(&slug)
                .bind(team_id)
                .fetch_optional(pool)
                .await?;
        let final_slug = if existing.is_some() {
            format!("{}-{}", slug, &team_id[..8.min(team_id.len())])
        } else {
            slug
        };
        sqlx::query("UPDATE teams SET slug = ? WHERE id = ?")
            .bind(&final_slug)
            .bind(team_id)
            .execute(pool)
            .await?;
        tracing::info!(team_id = %team_id, slug = %final_slug, "generated slug for team with NULL slug");
    }

    // Find team links that don't yet have a corresponding event type on the team
    #[allow(clippy::type_complexity)]
    let links: Vec<(
        String,
        String,
        i32,
        i32,
        i32,
        i32,
        String,
        String,
        Option<String>,
        String,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<i32>,
    )> = sqlx::query_as(
        "SELECT tl.id, tl.title, tl.duration_min, tl.buffer_before, tl.buffer_after, \
         tl.min_notice_min, tl.availability_start, tl.availability_end, \
         tl.availability_windows, tl.availability_days, \
         tl.location_type, tl.location_value, tl.description, tl.reminder_minutes \
         FROM team_links tl \
         WHERE EXISTS (SELECT 1 FROM teams t WHERE t.id = tl.id) \
         AND NOT EXISTS (SELECT 1 FROM event_types et WHERE et.team_id = tl.id)",
    )
    .fetch_all(pool)
    .await?;

    if links.is_empty() {
        return Ok(());
    }

    let mut migrated = 0u32;
    for (
        tl_id,
        title,
        duration,
        buf_before,
        buf_after,
        min_notice,
        avail_start,
        avail_end,
        avail_windows,
        avail_days,
        loc_type,
        loc_value,
        description,
        reminder_min,
    ) in &links
    {
        // Find the account of the team creator (needed for event_type.account_id)
        let creator: Option<(String,)> = sqlx::query_as(
            "SELECT a.id FROM accounts a \
             JOIN teams t ON a.user_id = t.created_by \
             WHERE t.id = ?",
        )
        .bind(tl_id)
        .fetch_optional(pool)
        .await?;

        let account_id = match creator {
            Some((aid,)) => aid,
            None => continue, // Skip if creator has no account
        };

        // Create event type for the team
        let et_id = uuid::Uuid::new_v4().to_string();
        let slug = title
            .to_lowercase()
            .chars()
            .map(|c| if c.is_alphanumeric() { c } else { '-' })
            .collect::<String>()
            .trim_matches('-')
            .to_string();
        let slug = if slug.is_empty() {
            "meeting".to_string()
        } else {
            slug
        };

        sqlx::query(
            "INSERT INTO event_types (id, account_id, slug, title, description, duration_min, \
             buffer_before, buffer_after, min_notice_min, enabled, requires_confirmation, \
             location_type, location_value, team_id, scheduling_mode, reminder_minutes, \
             visibility, created_by_user_id) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, 1, 0, ?, ?, ?, 'round_robin', ?, 'private', \
             (SELECT created_by FROM teams WHERE id = ?))",
        )
        .bind(&et_id)
        .bind(&account_id)
        .bind(&slug)
        .bind(title)
        .bind(description)
        .bind(duration)
        .bind(buf_before)
        .bind(buf_after)
        .bind(min_notice)
        .bind(loc_type)
        .bind(loc_value)
        .bind(tl_id)
        .bind(reminder_min)
        .bind(tl_id)
        .execute(pool)
        .await?;

        // Create availability rules from the team link's inline config
        let days: Vec<i32> = avail_days
            .split(',')
            .filter_map(|d| d.trim().parse().ok())
            .collect();

        if let Some(windows) = avail_windows {
            // Multiple windows: "09:00-12:00,13:00-17:00"
            for window in windows.split(',') {
                let parts: Vec<&str> = window.trim().split('-').collect();
                if parts.len() == 2 {
                    for day in &days {
                        let rule_id = uuid::Uuid::new_v4().to_string();
                        sqlx::query(
                            "INSERT INTO availability_rules (id, event_type_id, day_of_week, start_time, end_time) \
                             VALUES (?, ?, ?, ?, ?)",
                        )
                        .bind(&rule_id)
                        .bind(&et_id)
                        .bind(day)
                        .bind(parts[0].trim())
                        .bind(parts[1].trim())
                        .execute(pool)
                        .await?;
                    }
                }
            }
        } else {
            // Single window from start/end
            for day in &days {
                let rule_id = uuid::Uuid::new_v4().to_string();
                sqlx::query(
                    "INSERT INTO availability_rules (id, event_type_id, day_of_week, start_time, end_time) \
                     VALUES (?, ?, ?, ?, ?)",
                )
                .bind(&rule_id)
                .bind(&et_id)
                .bind(day)
                .bind(avail_start)
                .bind(avail_end)
                .execute(pool)
                .await?;
            }
        }

        // Migrate team_link_bookings → bookings
        let tlb: Vec<(
            String,
            String,
            String,
            String,
            Option<String>,
            Option<String>,
            String,
            String,
            String,
            String,
            Option<String>,
            String,
        )> = sqlx::query_as(
            "SELECT id, uid, guest_name, guest_email, guest_timezone, notes, \
                 start_at, end_at, status, cancel_token, reminder_sent_at, created_at \
                 FROM team_link_bookings WHERE team_link_id = ?",
        )
        .bind(tl_id)
        .fetch_all(pool)
        .await?;

        for (
            bid,
            uid,
            gname,
            gemail,
            gtz,
            notes,
            start,
            end,
            status,
            cancel_tok,
            reminder_sent,
            created,
        ) in &tlb
        {
            let reschedule_token = uuid::Uuid::new_v4().to_string();
            sqlx::query(
                "INSERT OR IGNORE INTO bookings (id, event_type_id, uid, guest_name, guest_email, \
                 guest_timezone, notes, start_at, end_at, status, cancel_token, reschedule_token, \
                 reminder_sent_at, created_at) \
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(bid)
            .bind(&et_id)
            .bind(uid)
            .bind(gname)
            .bind(gemail)
            .bind(gtz)
            .bind(notes)
            .bind(start)
            .bind(end)
            .bind(status)
            .bind(cancel_tok)
            .bind(&reschedule_token)
            .bind(reminder_sent)
            .bind(created)
            .execute(pool)
            .await?;
        }

        migrated += 1;
    }

    if migrated > 0 {
        tracing::info!(count = migrated, "migrated team links to team event types");
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
            "booking_invites",
            "booking_attendees",
            "event_type_watchers",
            "booking_claim_tokens",
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
        assert_eq!(count.0, 46, "All 46 migrations should be tracked");
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
        assert_eq!(count.0, 46, "Still 46 migrations after second run");
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
