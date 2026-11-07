//! Dashboard overview analytics: bucketing, scoping and the rendered page.

use super::*;
use crate::web::overview_stats::{self as ov, StatRow};

fn row(start: NaiveDateTime, minutes: i64, status: &str, created: NaiveDateTime) -> StatRow {
    StatRow {
        start,
        end: start + Duration::minutes(minutes),
        created: Some(created),
        status: status.to_string(),
        event_type_id: "et-a".to_string(),
        event_title: "Intro".to_string(),
    }
}

// Wednesday 2026-11-04, 12:00.
fn wednesday_noon() -> NaiveDateTime {
    dt(2026, 11, 4, 12, 0)
}

#[test]
fn compute_buckets_weeks_deltas_and_rates() {
    let now = wednesday_noon();
    let rows = vec![
        // Two confirmed meetings this week, one in the past, one ahead.
        row(
            dt(2026, 11, 2, 9, 0),
            30,
            "confirmed",
            dt(2026, 10, 20, 8, 0),
        ),
        row(
            dt(2026, 11, 5, 14, 0),
            60,
            "confirmed",
            dt(2026, 11, 1, 8, 0),
        ),
        // Cancelled and pending this week.
        row(
            dt(2026, 11, 3, 10, 0),
            30,
            "cancelled",
            dt(2026, 10, 25, 8, 0),
        ),
        row(dt(2026, 11, 6, 10, 0), 30, "pending", dt(2026, 11, 3, 8, 0)),
        // Declined counts with cancelled in the chart, not in the rate.
        row(
            dt(2026, 11, 3, 11, 0),
            30,
            "declined",
            dt(2026, 11, 2, 8, 0),
        ),
        // Previous 30-day window: one held, created then too.
        row(
            dt(2026, 9, 20, 9, 0),
            45,
            "confirmed",
            dt(2026, 9, 15, 8, 0),
        ),
        // Outside every window.
        row(dt(2026, 1, 5, 9, 0), 30, "confirmed", dt(2026, 1, 1, 8, 0)),
    ];
    let s = ov::compute(&rows, now, WeekStart::Mon);

    assert_eq!(s.weeks.len(), ov::WEEKS);
    let current = s.weeks.last().unwrap();
    assert_eq!(current.start, NaiveDate::from_ymd_opt(2026, 11, 2));
    assert_eq!(
        (current.confirmed, current.pending, current.cancelled),
        (2, 1, 2)
    );
    assert_eq!(current.minutes, 90);
    // Created this week: the pending (Nov 3) and the declined (Nov 2).
    assert_eq!(current.booked, 2);

    // Created in the last 30 days: Oct 20, Nov 1, Oct 25, Nov 3, Nov 2.
    assert_eq!(s.booked30, 5);
    assert_eq!(s.booked_prev30, 1);
    // Held in the last 30 days: only the Nov 2 meeting (Nov 5 is ahead).
    assert_eq!((s.held30, s.minutes30), (1, 30));
    assert_eq!((s.held_prev30, s.minutes_prev30), (1, 45));
    assert_eq!(s.cancelled30, 1);
    // Next 7 days: the Nov 5 confirmed meeting only.
    assert_eq!((s.next7_count, s.next7_minutes), (1, 60));
    // Heatmap, Monday-first rows: Monday 09:00, Thursday 14:00 and the
    // Sunday Sep 20 meeting, still inside the 12-week window.
    assert_eq!(s.heat[0][9], 1);
    assert_eq!(s.heat[3][14], 1);
    assert_eq!(s.heat[6][9], 1);
    assert_eq!(s.heat.iter().flatten().sum::<i64>(), 3);
    assert_eq!(s.by_type, vec![("Intro".to_string(), 3)]);
    // January is outside the chart; everything else starts inside it.
    assert_eq!(s.chart_total, 6);
}

#[test]
fn compute_follows_the_week_start() {
    let now = wednesday_noon();
    let rows = vec![row(dt(2026, 11, 1, 9, 0), 30, "confirmed", now)];
    // Sunday first: Sunday Nov 1 opens the current week, row 0.
    let s = ov::compute(&rows, now, WeekStart::Sun);
    assert_eq!(
        s.weeks.last().unwrap().start,
        NaiveDate::from_ymd_opt(2026, 11, 1)
    );
    assert_eq!(s.weeks.last().unwrap().confirmed, 1);
    assert_eq!(s.heat[0][9], 1);
    // Monday first: the same Sunday closes the previous week, row 6.
    let s = ov::compute(&rows, now, WeekStart::Mon);
    assert_eq!(s.weeks[ov::WEEKS - 2].confirmed, 1);
    assert_eq!(s.heat[6][9], 1);
}

#[test]
fn stats_context_formats_in_the_reader_language() {
    let now = wednesday_noon();
    let rows = vec![
        row(
            dt(2026, 11, 2, 9, 0),
            90,
            "confirmed",
            dt(2026, 11, 1, 8, 0),
        ),
        row(
            dt(2026, 11, 3, 9, 0),
            30,
            "cancelled",
            dt(2026, 11, 1, 8, 0),
        ),
        row(
            dt(2026, 9, 25, 9, 0),
            30,
            "confirmed",
            dt(2026, 9, 20, 8, 0),
        ),
    ];
    let s = ov::compute(&rows, now, WeekStart::Mon);
    let ctx = ov::stats_context(&s, "fr", WeekStart::Mon);
    let kpis = ctx.get_attr("kpis").unwrap();
    let time = kpis.get_item(&minijinja::Value::from(2)).unwrap();
    assert_eq!(time.get_attr("value").unwrap().to_string(), "1 h 30");
    let rate = kpis.get_item(&minijinja::Value::from(3)).unwrap();
    assert_eq!(rate.get_attr("value").unwrap().to_string(), "50\u{202f}%");
    // From 0 % to 50 %: an increase in cancellations is bad news.
    let delta = rate.get_attr("delta").unwrap();
    assert_eq!(delta.get_attr("tone").unwrap().to_string(), "bad");
    assert_eq!(delta.get_attr("text").unwrap().to_string(), "+50 pts");
    assert_eq!(ov::duration_label("en", 45), "45 min");
    assert_eq!(ov::duration_label("de", 120), "2 Std.");
}

/// Give a user an account with a unique email, so `insert_personal_et`
/// (whose own fallback account email is fixed) works for several users.
async fn give_account(pool: &SqlitePool, user_id: &str) {
    sqlx::query(
        "INSERT INTO accounts (id, name, email, timezone, user_id) VALUES (?, 'A', ?, 'UTC', ?)",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(format!("{user_id}@accounts.test"))
    .bind(user_id)
    .execute(pool)
    .await
    .unwrap();
}

/// Insert a booking with explicit times (UTC instants) and optional
/// assignment / claim.
async fn insert_stat_booking(
    pool: &SqlitePool,
    et_id: &str,
    guest: &str,
    start: chrono::DateTime<Utc>,
    status: &str,
    assigned: Option<&str>,
    claimed: Option<&str>,
) {
    let end = start + Duration::minutes(30);
    sqlx::query("INSERT INTO bookings (id, event_type_id, uid, guest_name, guest_email, guest_timezone, start_at, end_at, time_version, status, cancel_token, reschedule_token, assigned_user_id, claimed_by_user_id) VALUES (?, ?, ?, ?, 'g@e.com', 'UTC', ?, ?, 1, ?, ?, ?, ?, ?)")
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(et_id)
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(guest)
        .bind(start.format("%Y-%m-%dT%H:%M:%SZ").to_string())
        .bind(end.format("%Y-%m-%dT%H:%M:%SZ").to_string())
        .bind(status)
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(assigned)
        .bind(claimed)
        .execute(pool)
        .await
        .unwrap();
}

#[tokio::test]
async fn stats_count_only_the_meetings_the_user_hosts() {
    let pool = setup_test_db().await;
    let me = insert_role_user(&pool, "me@example.com", "user").await;
    let other = insert_role_user(&pool, "other@example.com", "user").await;
    give_account(&pool, &me).await;
    give_account(&pool, &other).await;
    let mine = insert_personal_et(&pool, &me, "mine").await;
    let theirs = insert_personal_et(&pool, &other, "theirs").await;
    let (_, team_et) = insert_team_with_et(
        &pool,
        "crew",
        "crew-call",
        &[(&me, "member"), (&other, "member")],
    )
    .await;
    // A team event type created under my own account: a round-robin
    // booking assigned to the other member is theirs, not mine.
    let my_team_et = insert_personal_et(&pool, &me, "my-rr").await;
    let (team_id,): (String,) = sqlx::query_as("SELECT id FROM teams WHERE slug = 'crew'")
        .fetch_one(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE event_types SET team_id = ? WHERE id = ?")
        .bind(&team_id)
        .bind(&my_team_et)
        .execute(&pool)
        .await
        .unwrap();

    let soon = Utc::now() + Duration::days(2);
    insert_stat_booking(&pool, &mine, "Mine", soon, "confirmed", None, None).await;
    insert_stat_booking(&pool, &theirs, "Theirs", soon, "confirmed", None, None).await;
    insert_stat_booking(
        &pool,
        &team_et,
        "Assigned",
        soon,
        "confirmed",
        Some(&me),
        None,
    )
    .await;
    insert_stat_booking(
        &pool,
        &team_et,
        "Collective",
        soon + Duration::hours(1),
        "confirmed",
        None,
        None,
    )
    .await;
    insert_stat_booking(
        &pool,
        &my_team_et,
        "RrOther",
        soon,
        "confirmed",
        Some(&other),
        None,
    )
    .await;
    insert_stat_booking(
        &pool,
        &theirs,
        "Claimed",
        soon + Duration::hours(2),
        "confirmed",
        None,
        Some(&me),
    )
    .await;

    let rows = ov::fetch_stat_rows(&pool, &me, Tz::UTC, Utc::now())
        .await
        .unwrap();
    let mut titles: Vec<_> = rows.iter().map(|r| r.event_type_id.clone()).collect();
    titles.sort();
    let mut expected = vec![
        mine.clone(),
        team_et.clone(),
        team_et.clone(),
        theirs.clone(),
    ];
    expected.sort();
    assert_eq!(titles, expected, "mine, assigned, collective, claimed");

    // The other user sees their own booking, the collective one and the
    // round-robin booking assigned to them, not mine.
    let rows = ov::fetch_stat_rows(&pool, &other, Tz::UTC, Utc::now())
        .await
        .unwrap();
    assert_eq!(
        rows.len(),
        4,
        "theirs, claimed (on their type), collective, rr"
    );

    let agenda = ov::agenda(&pool, &me, Tz::UTC, Utc::now(), "en").await;
    let guests: Vec<String> = agenda
        .iter()
        .map(|a| a.get_attr("guest_name").unwrap().to_string())
        .collect();
    assert_eq!(guests.len(), 4);
    for g in ["Mine", "Assigned", "Collective", "Claimed"] {
        assert!(guests.contains(&g.to_string()), "{g} in {guests:?}");
    }
    assert!(ov::has_any_booking(&pool, &me).await);
    let lonely = insert_role_user(&pool, "lonely@example.com", "user").await;
    assert!(!ov::has_any_booking(&pool, &lonely).await);
}

#[tokio::test]
async fn agenda_keeps_ongoing_drops_past_and_only_links_web_urls() {
    let pool = setup_test_db().await;
    let me = insert_role_user(&pool, "me@example.com", "user").await;
    let et = insert_personal_et(&pool, &me, "call").await;
    sqlx::query("UPDATE event_types SET location_value = 'javascript:alert(1)' WHERE id = ?")
        .bind(&et)
        .execute(&pool)
        .await
        .unwrap();
    let now = Utc::now();
    insert_stat_booking(
        &pool,
        &et,
        "Past",
        now - Duration::hours(3),
        "confirmed",
        None,
        None,
    )
    .await;
    insert_stat_booking(
        &pool,
        &et,
        "Live",
        now - Duration::minutes(10),
        "confirmed",
        None,
        None,
    )
    .await;
    insert_stat_booking(
        &pool,
        &et,
        "Later",
        now + Duration::hours(5),
        "confirmed",
        None,
        None,
    )
    .await;
    insert_stat_booking(
        &pool,
        &et,
        "Gone",
        now + Duration::hours(6),
        "cancelled",
        None,
        None,
    )
    .await;
    sqlx::query(
        "UPDATE bookings SET meeting_url = 'https://meet.example/x' WHERE guest_name = 'Later'",
    )
    .execute(&pool)
    .await
    .unwrap();

    let agenda = ov::agenda(&pool, &me, Tz::UTC, now, "en").await;
    let names: Vec<String> = agenda
        .iter()
        .map(|a| a.get_attr("guest_name").unwrap().to_string())
        .collect();
    assert_eq!(names, vec!["Live", "Later"]);
    assert!(agenda[0].get_attr("ongoing").unwrap().is_true());
    assert_eq!(agenda[0].get_attr("join_url").unwrap().to_string(), "");
    assert_eq!(
        agenda[1].get_attr("join_url").unwrap().to_string(),
        "https://meet.example/x"
    );
}

#[tokio::test]
async fn overview_page_renders_the_analytics_for_the_signed_in_user_only() {
    let (app, pool, session, et_id) = setup_test_app().await;
    let other = insert_role_user(&pool, "other@example.com", "user").await;
    give_account(&pool, &other).await;
    let theirs = insert_personal_et(&pool, &other, "theirs").await;
    let now = Utc::now();
    insert_stat_booking(
        &pool,
        &et_id,
        "Alice Guest",
        now + Duration::hours(2),
        "confirmed",
        None,
        None,
    )
    .await;
    insert_stat_booking(
        &pool,
        &et_id,
        "Old Guest",
        now - Duration::days(3),
        "cancelled",
        None,
        None,
    )
    .await;
    insert_stat_booking(
        &pool,
        &theirs,
        "Stranger Guest",
        now + Duration::hours(1),
        "confirmed",
        None,
        None,
    )
    .await;

    let body = body_string(
        app.oneshot(get_authed("/dashboard", &session))
            .await
            .unwrap(),
    )
    .await;
    assert!(body.contains("Meetings per week"));
    assert!(body.contains("Busiest times"));
    assert!(body.contains("Next up"));
    assert!(body.contains("Alice Guest"));
    assert!(!body.contains("Stranger Guest"));
    assert!(body.contains("Cancellation rate, last 30 days"));
}

#[tokio::test]
async fn overview_page_without_bookings_shows_the_empty_state() {
    let (app, _pool, session, _et_id) = setup_test_app().await;
    let body = body_string(
        app.oneshot(get_authed("/dashboard", &session))
            .await
            .unwrap(),
    )
    .await;
    assert!(body.contains("Your activity will show up here"));
    assert!(body.contains("No upcoming meetings"));
    assert!(!body.contains("Busiest times"));
}
