//! Admin overview: usage and health figures, attention items, last sign-in.

use super::*;
use crate::web::admin_stats::{iso_utc, load, nice_ceiling, week_axis_labels, week_starts};

fn at(y: i32, m: u32, d: u32, h: u32) -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(y, m, d, h, 0, 0).unwrap()
}

async fn add_user(
    pool: &SqlitePool,
    email: &str,
    role: &str,
    provider: &str,
    enabled: bool,
) -> String {
    let id = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO users (id, email, name, role, auth_provider, username, enabled, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, '2026-01-01 00:00:00')",
    )
    .bind(&id)
    .bind(email)
    .bind(email)
    .bind(role)
    .bind(provider)
    .bind(email.split('@').next().unwrap())
    .bind(enabled)
    .execute(pool)
    .await
    .unwrap();
    id
}

#[allow(clippy::too_many_arguments)]
async fn add_booking(
    pool: &SqlitePool,
    et_id: &str,
    status: &str,
    created_at: &str,
    start_at: &str,
    assigned: Option<&str>,
    pending_expires_at: Option<&str>,
    writeback_error: Option<&str>,
) {
    sqlx::query(
        "INSERT INTO bookings (id, event_type_id, uid, guest_name, guest_email, guest_timezone,
             start_at, end_at, status, cancel_token, reschedule_token, assigned_user_id, created_at,
             pending_expires_at, writeback_error)
         VALUES (?, ?, ?, 'G', 'g@example.com', 'UTC', ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(et_id)
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(start_at)
    .bind(start_at)
    .bind(status)
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(assigned)
    .bind(created_at)
    .bind(pending_expires_at)
    .bind(writeback_error)
    .execute(pool)
    .await
    .unwrap();
}

#[test]
fn nice_ceiling_keeps_an_even_round_top() {
    assert_eq!(nice_ceiling(0), 2);
    assert_eq!(nice_ceiling(1), 2);
    assert_eq!(nice_ceiling(3), 4);
    assert_eq!(nice_ceiling(9), 10);
    assert_eq!(nice_ceiling(11), 12);
    assert_eq!(nice_ceiling(55), 60);
    assert_eq!(nice_ceiling(61), 80);
    assert_eq!(nice_ceiling(81), 100);
    assert_eq!(nice_ceiling(101), 120);
    assert_eq!(nice_ceiling(1234), 1600);
    for max in 0..5000 {
        let top = nice_ceiling(max);
        assert!(top >= max && top % 2 == 0, "{max} -> {top}");
    }
}

#[test]
fn week_axis_labels_count_weeks_or_write_dates() {
    let starts = week_starts(
        NaiveDate::from_ymd_opt(2026, 11, 7).unwrap(),
        WeekStart::Mon,
    );
    let fr = week_axis_labels(&starts, "fr");
    // Every other week, counted back from the current one (ISO week 45).
    assert_eq!(fr.last().unwrap(), &("45".to_string(), true));
    assert!(!fr[0].1, "the oldest week is not labelled");
    assert_eq!(
        fr[1],
        ("S35".to_string(), true),
        "first label carries the prefix"
    );
    assert_eq!(fr[3], ("37".to_string(), true));
    let de = week_axis_labels(&starts, "de");
    assert_eq!(de[1].0, "KW 35");
    // English reads dates, not week numbers.
    let en = week_axis_labels(&starts, "en");
    assert_eq!(en.last().unwrap().0, "Nov 2");
    // A Sunday week takes the ISO number of the Monday inside it.
    let sun = week_starts(
        NaiveDate::from_ymd_opt(2026, 11, 7).unwrap(),
        WeekStart::Sun,
    );
    assert_eq!(week_axis_labels(&sun, "fr").last().unwrap().0, "45");
}

#[test]
fn weeks_follow_the_week_start() {
    // 2026-11-07 is a Saturday.
    let today = NaiveDate::from_ymd_opt(2026, 11, 7).unwrap();
    let monday = week_starts(today, WeekStart::Mon);
    assert_eq!(monday.len(), 12);
    assert_eq!(monday[11], NaiveDate::from_ymd_opt(2026, 11, 2).unwrap());
    assert_eq!(monday[0], NaiveDate::from_ymd_opt(2026, 8, 17).unwrap());
    let sunday = week_starts(today, WeekStart::Sun);
    assert_eq!(sunday[11], NaiveDate::from_ymd_opt(2026, 11, 1).unwrap());
}

#[test]
fn iso_utc_reads_both_stored_forms() {
    assert_eq!(iso_utc("2026-11-07 10:20:30"), "2026-11-07T10:20:30Z");
    assert_eq!(iso_utc("2026-11-07T10:20:30Z"), "2026-11-07T10:20:30Z");
    assert_eq!(iso_utc("garbage"), "");
}

/// Every figure on the overview, against a database whose answer is known.
#[tokio::test]
async fn stats_count_a_seeded_instance() {
    let pool = setup_test_db().await;
    // seed_test_data: one enabled local admin with a personal event type.
    let (owner, _account, et) = seed_test_data(&pool).await;
    let now = at(2026, 11, 7, 12);

    sqlx::query("UPDATE users SET created_at = '2026-01-01 00:00:00'")
        .execute(&pool)
        .await
        .unwrap();
    let member = add_user(&pool, "m@example.com", "user", "oidc", true).await;
    add_user(&pool, "off@example.com", "user", "local", false).await;
    // Created within the last 30 days, and in the 30 before.
    sqlx::query("UPDATE users SET created_at = '2026-11-01 08:00:00' WHERE id = ?")
        .bind(&member)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "UPDATE users SET created_at = '2026-09-20 08:00:00' WHERE email = 'off@example.com'",
    )
    .execute(&pool)
    .await
    .unwrap();
    // Signed in 3 days ago: active. The owner's sign-in is 40 days old.
    sqlx::query("UPDATE users SET last_login_at = '2026-11-04 09:00:00' WHERE id = ?")
        .bind(&member)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE users SET last_login_at = '2026-09-28 09:00:00' WHERE id = ?")
        .bind(&owner)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO user_mfa (user_id, secret_enc, last_step) VALUES (?, 'x', 0)")
        .bind(&owner)
        .execute(&pool)
        .await
        .unwrap();

    let team = uuid::Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO teams (id, name, slug) VALUES (?, 'Sales', 'sales')")
        .bind(&team)
        .execute(&pool)
        .await
        .unwrap();
    let account: String = sqlx::query_scalar("SELECT id FROM accounts WHERE user_id = ?")
        .bind(&owner)
        .fetch_one(&pool)
        .await
        .unwrap();
    let team_et = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO event_types (id, account_id, slug, title, duration_min, enabled, team_id)
         VALUES (?, ?, 'demo', 'Demo', 30, 0, ?)",
    )
    .bind(&team_et)
    .bind(&account)
    .bind(&team)
    .execute(&pool)
    .await
    .unwrap();

    // Last 30 days: 3 confirmed (one upcoming within 7 days, one later,
    // one past), 1 pending expiring in 5 hours, 1 cancelled.
    add_booking(
        &pool,
        &et,
        "confirmed",
        "2026-11-02 10:00:00",
        "2026-11-10T09:00:00Z",
        None,
        None,
        None,
    )
    .await;
    add_booking(
        &pool,
        &et,
        "confirmed",
        "2026-11-03 10:00:00",
        "2026-12-01T09:00:00Z",
        None,
        None,
        Some("HTTP 412"),
    )
    .await;
    add_booking(
        &pool,
        &team_et,
        "confirmed",
        "2026-10-20 10:00:00",
        "2026-10-25T09:00:00Z",
        Some(&member),
        None,
        None,
    )
    .await;
    add_booking(
        &pool,
        &et,
        "pending",
        "2026-11-06 10:00:00",
        "2026-11-09T09:00:00Z",
        None,
        Some("2026-11-07T17:00:00Z"),
        None,
    )
    .await;
    add_booking(
        &pool,
        &et,
        "cancelled",
        "2026-11-05 10:00:00",
        "2026-11-12T09:00:00Z",
        None,
        None,
        None,
    )
    .await;
    // Previous 30 days: 1 declined. Long ago (outside both windows and the
    // ranking): 1 confirmed.
    add_booking(
        &pool,
        &et,
        "declined",
        "2026-09-15 10:00:00",
        "2026-09-20T09:00:00Z",
        None,
        None,
        None,
    )
    .await;
    add_booking(
        &pool,
        &et,
        "confirmed",
        "2025-01-15 10:00:00",
        "2025-01-20T09:00:00Z",
        None,
        None,
        None,
    )
    .await;

    // Sources: one healthy, one failing, one stale, one disabled.
    for (name, enabled, synced, error) in [
        ("ok", 1, "2026-11-07 11:50:00", None),
        ("broken", 1, "2026-11-07 11:00:00", Some("401 Unauthorized")),
        ("stale", 1, "2026-11-01 11:00:00", None),
        ("off", 0, "2026-01-01 00:00:00", None),
    ] {
        sqlx::query(
            "INSERT INTO caldav_sources (id, account_id, name, url, username, password_enc, last_synced, enabled,
                 last_sync_error, sync_failing_since, sync_failure_count)
             VALUES (?, ?, ?, 'https://dav.example.com/', 'u', 'TOPSECRETBLOB', ?, ?, ?, '2026-11-07 05:00:00', 4)",
        )
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(&account)
        .bind(name)
        .bind(synced)
        .bind(enabled)
        .bind(error)
        .execute(&pool)
        .await
        .unwrap();
    }
    sqlx::query("INSERT INTO resources (id, name, feed_url, last_sync_error) VALUES (?, 'Lab', 'https://x.invalid/a.ics', 'HTTP 404')")
        .bind(uuid::Uuid::new_v4().to_string())
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO sms_usage (id, sent_at, event, provider, segments, cost, currency) VALUES (?, '2026-11-01 10:00:00', 'confirmed', 'twilio', 1, 0.05, 'EUR'), (?, '2026-08-01 10:00:00', 'confirmed', 'twilio', 1, 0.05, 'EUR')")
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(uuid::Uuid::new_v4().to_string())
        .execute(&pool)
        .await
        .unwrap();

    let s = load(&pool, now, WeekStart::Mon, "en").await;

    assert_eq!(s.users.total, 3);
    assert_eq!(s.users.enabled, 2);
    assert_eq!(s.users.disabled, 1);
    assert_eq!(s.users.admins, 1);
    assert_eq!(s.users.local, 2);
    assert_eq!(s.users.oidc, 1);
    assert_eq!(s.users.mfa_enrolled, 1);
    assert_eq!(s.users.new_30d, 1);
    assert_eq!(s.users.new_prev_30d, 1);
    assert_eq!(s.users.active_30d, 1);

    assert_eq!(s.bookings.total, 7);
    assert_eq!(s.bookings.confirmed, 4);
    assert_eq!(s.bookings.pending, 1);
    assert_eq!(s.bookings.cancelled, 1);
    assert_eq!(s.bookings.declined, 1);
    assert_eq!(s.bookings.created_30d, 5);
    assert_eq!(s.bookings.created_prev_30d, 1);
    assert_eq!(s.bookings.upcoming, 2);
    assert_eq!(s.bookings.upcoming_7d, 1);
    assert_eq!(s.bookings.pending_expiring_24h, 1);

    // Weekly buckets: Monday weeks ending with the week of 2026-11-02.
    assert_eq!(s.weekly.len(), 12);
    let last = s.weekly.last().unwrap();
    assert_eq!(last.start, "2026-11-02");
    assert_eq!((last.confirmed, last.pending, last.ended), (2, 1, 1));
    let oct19 = s.weekly.iter().find(|w| w.start == "2026-10-19").unwrap();
    assert_eq!(oct19.confirmed, 1);
    let sep14 = s.weekly.iter().find(|w| w.start == "2026-09-14").unwrap();
    assert_eq!(sep14.ended, 1);
    assert_eq!(s.weekly_total, 6, "the 2025 booking is outside the chart");
    // The scale counts what the bars stack: confirmed and pending, not
    // the cancelled count drawn under the axis.
    assert_eq!(last.active, 3);
    assert_eq!(s.weekly_scale, 4);
    assert_eq!(last.height_pct, 75.0);
    assert!(last.is_current && last.is_max);
    assert_eq!(s.weekly.iter().filter(|w| w.is_max).count(), 1);

    // Rankings: confirmed bookings made in the last 90 days.
    assert_eq!(s.top_event_types.len(), 2);
    assert_eq!(s.top_event_types[0].name, "Test Meeting");
    assert_eq!(s.top_event_types[0].count, 2);
    assert!(s.top_event_types[1].is_team);
    assert_eq!(s.top_event_types[1].detail, "Sales");
    assert_eq!(
        s.top_hosts.len(),
        2,
        "round-robin booking goes to its assigned member"
    );
    assert_eq!(s.top_hosts[0].count, 2);
    assert_eq!(s.top_hosts[1].detail, "m@example.com");

    assert_eq!(s.sources.total, 4);
    assert_eq!(s.sources.enabled, 3);
    assert_eq!(s.sources.disabled, 1);
    assert_eq!(s.sources.failing, 1);
    assert_eq!(s.sources.stale, 1);
    assert_eq!(s.sources.healthy, 1);
    assert_eq!(s.failing_sources.len(), 1);
    assert_eq!(s.failing_sources[0].error, "401 Unauthorized");
    assert_eq!(s.failing_sources[0].failing_since, "2026-11-07T05:00:00Z");
    assert_eq!(s.writeback_failure_count, 1);
    assert_eq!(s.writeback_failures[0].start, "2026-12-01T09:00:00Z");

    assert_eq!(s.resources_total, 1);
    assert_eq!(s.failing_resources.len(), 1);
    assert_eq!(s.teams, 1);
    assert_eq!(s.event_types_total, 2);
    assert_eq!(s.event_types_enabled, 1);
    assert_eq!(s.event_types_team, 1);
    assert_eq!(s.sms.sent_30d, 1);
    assert_eq!(s.sms.cost_30d, "0.05");
    assert_eq!(s.sms.currency, "EUR");
}

#[tokio::test]
async fn stats_on_an_empty_instance() {
    let pool = setup_test_db().await;
    let s = load(&pool, at(2026, 11, 7, 12), WeekStart::Mon, "fr").await;
    assert_eq!(s.users.total, 0);
    assert_eq!(s.bookings.total, 0);
    assert_eq!(s.weekly_total, 0);
    assert_eq!(s.weekly.len(), 12);
    assert!(s.top_hosts.is_empty());
    assert!(
        s.weekly.iter().all(|w| !w.is_max),
        "an empty chart labels no bar as the maximum"
    );
    assert_eq!(
        s.weekly[1].axis_label, "S35",
        "week prefix in the reader's language"
    );
}

#[tokio::test]
async fn signing_in_stamps_last_login() {
    let pool = setup_test_db().await;
    let (user, _, _) = seed_test_data(&pool).await;
    let before: Option<String> = sqlx::query_scalar("SELECT last_login_at FROM users WHERE id = ?")
        .bind(&user)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(before.is_none());
    let session = crate::auth::create_session(&pool, &user).await.unwrap();
    // Signing out deletes the session, never the stamp.
    crate::auth::delete_session(&pool, &session.id)
        .await
        .unwrap();
    let after: Option<String> = sqlx::query_scalar("SELECT last_login_at FROM users WHERE id = ?")
        .bind(&user)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(after.is_some(), "a sign-in records last_login_at");
}

#[tokio::test]
async fn attention_lists_what_an_operator_must_act_on() {
    let mut stats = crate::web::admin_stats::AdminStats::default();
    let clear = attention_items(&AttentionInputs {
        stats: &stats,
        smtp_configured: true,
        smtp_enabled: true,
        smtp_error: false,
        sms_configured: false,
        captcha_configured: false,
        base_url_set: true,
    });
    assert!(clear.is_empty(), "{clear:?}");

    stats.sources.failing = 2;
    stats.sources.stale = 1;
    stats.writeback_failure_count = 3;
    stats.bookings.pending_expiring_24h = 1;
    stats
        .failing_resources
        .push(crate::web::admin_stats::FailingResource::default());
    let items = attention_items(&AttentionInputs {
        stats: &stats,
        smtp_configured: false,
        smtp_enabled: false,
        smtp_error: false,
        sms_configured: true,
        captcha_configured: false,
        base_url_set: false,
    });
    let kinds: Vec<&str> = items.iter().map(|i| i.kind).collect();
    assert_eq!(
        kinds,
        [
            "smtp-missing",
            "sources-failing",
            "writeback",
            "sms-captcha",
            "resources-failing",
            "pending-expiring",
            "sources-stale",
            "base-url"
        ]
    );
    assert!(items[..4].iter().all(|i| i.severity == "critical"));
    assert_eq!(items[1].count, 2);

    // Every kind has its sentence in the template: an unmapped kind would
    // render an empty row.
    let template = std::fs::read_to_string("templates/admin.html").unwrap();
    for kind in kinds.iter().chain(["smtp-error", "smtp-disabled"].iter()) {
        assert!(
            template.contains(&format!("kind == \"{kind}\"")),
            "admin.html has no sentence for {kind}"
        );
    }
}

#[tokio::test]
async fn overview_renders_for_admins_without_secrets() {
    let (app, pool, session, et) = setup_test_app().await;
    let account: String = sqlx::query_scalar("SELECT id FROM accounts LIMIT 1")
        .fetch_one(&pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO caldav_sources (id, account_id, name, url, username, password_enc, enabled, last_sync_error, sync_failure_count)
         VALUES (?, ?, 'Work calendar', 'https://dav.example.com/', 'u', 'TOPSECRETBLOB', 1, 'PROPFIND returned 401', 5)",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(&account)
    .execute(&pool)
    .await
    .unwrap();
    insert_booking(&pool, &et, None, dt(2030, 1, 7, 10, 0), "confirmed").await;

    let response = app
        .clone()
        .oneshot(get_authed("/dashboard/admin", &session))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = body_string(response).await;
    assert!(body.contains("Needs attention"));
    assert!(body.contains("Bookings per week"));
    assert!(
        body.contains("PROPFIND returned 401"),
        "failing source error is listed"
    );
    assert!(body.contains("Work calendar"));
    assert!(
        !body.contains("TOPSECRETBLOB"),
        "credentials never reach the page"
    );
    // Every section is still rendered server-side, so forms work without JS.
    for action in [
        "/dashboard/admin/auth",
        "/dashboard/admin/oidc",
        "/dashboard/admin/smtp",
        "/dashboard/admin/sms/policy",
        "/dashboard/admin/captcha",
        "/dashboard/admin/resources",
        "/dashboard/admin/accent",
        "/dashboard/admin/general",
    ] {
        assert!(
            body.contains(&format!("action=\"{action}\"")),
            "{action} form missing"
        );
    }

    // A member gets no figures.
    let (_, member_session) =
        insert_test_user_with_session(&pool, "Member", "member@example.com", "member").await;
    let response = app
        .oneshot(get_authed("/dashboard/admin", &member_session))
        .await
        .unwrap();
    assert_eq!(response.status(), 403);
    let body = body_string(response).await;
    assert!(!body.contains("Bookings per week"));
    assert!(!body.contains("PROPFIND returned 401"));
}
