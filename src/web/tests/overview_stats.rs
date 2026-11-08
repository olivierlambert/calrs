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

    // Created in the last 30 days: Oct 20, Nov 1, Oct 25, Nov 3, Nov 2.
    assert_eq!(s.booked30, 5);
    assert_eq!(s.booked_prev30, 1);
    // Held in the last 30 days: only the Nov 2 meeting (Nov 5 is ahead).
    assert_eq!((s.held30, s.minutes30), (1, 30));
    assert_eq!((s.held_prev30, s.minutes_prev30), (1, 45));
    assert_eq!(s.cancelled30, 1);
    // Notice of the bookings made in the last 30 days, start minus creation.
    assert_eq!(s.notice_count, 5);
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
    // Monday first: the same Sunday closes the previous week.
    let s = ov::compute(&rows, now, WeekStart::Mon);
    assert_eq!(s.weeks[ov::WEEKS - 2].confirmed, 1);
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
    let ctx = ov::stats_context(&s, "fr");
    let figures = ctx.get_attr("figures").unwrap();
    let fig = |i: usize, attr: &str| {
        figures
            .get_item(&minijinja::Value::from(i))
            .unwrap()
            .get_attr(attr)
            .unwrap()
            .to_string()
    };
    assert_eq!(fig(1, "value"), "1 h 30");
    assert_eq!(fig(1, "cmp"), "30 min les 30 jours précédents");
    // One of the two meetings due was cancelled, none the month before:
    // the rate, and a bar pair with the previous bar empty.
    assert_eq!(fig(2, "value"), "50\u{202f}%");
    assert_eq!(fig(2, "prev"), "0\u{202f}%");
    assert_eq!(fig(2, "cmp"), "0\u{202f}% les 30 jours précédents");
    assert_eq!(fig(2, "cur_pct"), "100.00");
    assert_eq!(fig(2, "prev_pct"), "0.00");
    // Booked in the last 30 days: two, against one the month before.
    assert_eq!(fig(0, "value"), "2");
    assert_eq!(fig(0, "prev_pct"), "50.00");
    assert!(ctx.get_attr("has_figures").unwrap().is_true());
    // Weeks are numbered in French: the first tick carries the prefix.
    let columns = ctx.get_attr("columns").unwrap();
    let ticks: Vec<String> = (0..ov::WEEKS)
        .map(|i| {
            columns
                .get_item(&minijinja::Value::from(i))
                .unwrap()
                .get_attr("tick")
                .unwrap()
                .to_string()
        })
        .filter(|t| !t.is_empty())
        .collect();
    assert_eq!(ticks.first().map(String::as_str), Some("S35"));
    assert_eq!(ticks.last().map(String::as_str), Some("45"));
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
    // Its unassigned booking below is a collective one: an unassigned
    // round-robin booking is no member's (#173).
    sqlx::query("UPDATE event_types SET scheduling_mode = 'collective' WHERE id = ?")
        .bind(&team_et)
        .execute(&pool)
        .await
        .unwrap();
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
    assert!(!body.contains("Busiest times"));
    assert!(body.contains("Coming up"));
    assert!(body.contains("Alice Guest"));
    assert!(!body.contains("Stranger Guest"));
    assert!(body.contains("Last 30 days"));
}

#[tokio::test]
async fn overview_page_for_a_new_account_shows_one_next_step_and_no_zeros() {
    // The seeded user has a meeting type with hours but no calendar and no
    // booking: the checklist leads, and no empty figure, chart or ledger
    // is drawn.
    let (app, _pool, session, _et_id) = setup_test_app().await;
    let body = body_string(
        app.oneshot(get_authed("/dashboard", &session))
            .await
            .unwrap(),
    )
    .await;
    assert!(body.contains("id=\"ov-start\""));
    assert!(body.contains("href=\"/dashboard/sources/new\""));
    let steps = body
        .split("id=\"ov-start\"")
        .nth(1)
        .and_then(|rest| rest.split("</ol>").next())
        .unwrap();
    assert_eq!(steps.matches("ui-btn--primary").count(), 1, "one next step");
    assert!(!body.contains("id=\"ov-30\""));
    assert!(!body.contains("id=\"ov-weekly-title\""));
    assert!(!body.contains("id=\"ov-types\""));
    assert!(!body.contains("id=\"ov-coming\""));
    // The week still shows the hours open for bookings.
    assert!(body.contains("class=\"wv-open\""));
}

#[tokio::test]
async fn overview_page_for_a_quiet_account_keeps_one_line_and_hides_empty_sections() {
    // Calendar connected, nothing booked yet: no checklist, no figures, no
    // chart, and "Coming up" says what to do in one line.
    let (app, pool, session, _et_id) = setup_test_app().await;
    let (acc,): (String,) = sqlx::query_as(
        "SELECT a.id FROM accounts a JOIN users u ON u.id = a.user_id WHERE u.email = 'test@example.com'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO caldav_sources (id, account_id, name, url, username) VALUES ('s', ?, 'S', 'https://dav.example/remote.php/dav', 'u')")
        .bind(&acc)
        .execute(&pool)
        .await
        .unwrap();
    let body = body_string(
        app.oneshot(get_authed("/dashboard", &session))
            .await
            .unwrap(),
    )
    .await;
    assert!(!body.contains("id=\"ov-start\""));
    assert!(!body.contains("id=\"ov-30\""));
    assert!(!body.contains("id=\"ov-weekly-title\""));
    assert!(body.contains("No meetings in the next 14 days"));
    assert!(body.contains("data-copy-path=\"/u/testuser\""));
    // With nothing on the right, the page is one column.
    assert!(body.contains("class=\"cols one\""));
    // One way out to the calendar, from the week's key.
    assert!(body.contains("Open your calendar"));
}

fn day_booking(id: &str, start: NaiveDateTime, minutes: i64, pending: bool) -> ov::DayBooking {
    ov::DayBooking {
        id: id.to_string(),
        start,
        end: start + Duration::minutes(minutes),
        pending,
        title: format!("Meeting {id}"),
        guest: "Ana Lopez".to_string(),
        extra_guests: 0,
        listed: true,
    }
}

#[test]
fn lanes_put_overlapping_bookings_side_by_side() {
    // 09:00-10:00 and 09:30-10:30 overlap, 10:30-11:00 starts a new cluster.
    let placed = ov::lanes(&[(540, 600), (570, 630), (630, 660)]);
    assert_eq!(placed, vec![(0, 2), (1, 2), (0, 1)]);
    // A third booking that fits after the first reuses its lane.
    let placed = ov::lanes(&[(540, 600), (570, 700), (600, 640)]);
    assert_eq!(placed, vec![(0, 2), (1, 2), (0, 2)]);
}

#[test]
fn week_view_lays_out_the_week_on_one_hour_axis() {
    let paris: Tz = "Europe/Paris".parse().unwrap();
    // Open Monday to Friday, 09:00 to 12:00 and 14:00 to 17:00.
    let hours = vec![ov::EventTypeHours {
        tz: paris,
        rules: (1..=5)
            .flat_map(|d| [(d, hm(9, 0), hm(12, 0)), (d, hm(14, 0), hm(17, 0))])
            .collect(),
        overrides: vec![],
        bookable_from: None,
        bookable_until: None,
    }];
    let bookings = vec![
        day_booking("a", dt(2026, 11, 2, 9, 0), 60, false),
        day_booking("b", dt(2026, 11, 2, 9, 30), 30, false),
        day_booking("p", dt(2026, 11, 3, 15, 0), 45, true),
    ];
    let busy = vec![(dt(2026, 11, 4, 0, 0), dt(2026, 11, 5, 0, 0))];
    let now = wednesday_noon();
    let ctx = ov::week_context(
        "en",
        &hours,
        &bookings,
        &busy,
        &[],
        paris,
        now,
        WeekStart::Mon,
    )
    .expect("a week with open hours is drawn");
    let day = |i: usize| {
        ctx.get_attr("days")
            .unwrap()
            .get_item(&minijinja::Value::from(i))
            .unwrap()
    };
    let attr = |v: &minijinja::Value, k: &str| v.get_attr(k).unwrap().to_string();

    // The axis spans the open hours only: an all-day calendar event is
    // clipped to it rather than stretching the week to midnight.
    assert_eq!(ctx.get_attr("from_minute").unwrap().as_i64(), Some(9 * 60));
    assert_eq!(ctx.get_attr("span_minutes").unwrap().as_i64(), Some(8 * 60));
    let labels: Vec<String> = (0..5)
        .filter_map(|i| {
            ctx.get_attr("hours")
                .unwrap()
                .get_item(&minijinja::Value::from(i))
                .ok()
        })
        .filter(|h| !h.is_undefined())
        .map(|h| attr(&h, "label"))
        .collect();
    assert_eq!(labels, vec!["09", "11", "13", "15", "17"]);

    // Monday first; weekend closed; Wednesday is today, with the now line.
    assert_eq!(attr(&day(0), "weekday"), "Mon");
    assert!(day(5).get_attr("closed").unwrap().is_true());
    assert!(day(2).get_attr("is_today").unwrap().is_true());
    assert_eq!(attr(&day(2), "now_top"), "37.500");
    assert!(day(1).get_attr("now_top").unwrap().is_none());

    // Overlapping Monday bookings share the column; each links to itself.
    let monday = day(0).get_attr("blocks").unwrap();
    let block = |i: usize| monday.get_item(&minijinja::Value::from(i)).unwrap();
    assert_eq!(attr(&block(0), "width"), "50.000");
    assert_eq!(attr(&block(1), "left"), "50.000");
    assert_eq!(attr(&block(0), "id"), "a");
    assert!(block(0).get_attr("past").unwrap().is_true());
    assert_eq!(attr(&day(0), "booked"), "1 h 30 min");
    // The tooltip carries time, title, guest and status.
    let tuesday = day(1)
        .get_attr("blocks")
        .unwrap()
        .get_item(&minijinja::Value::from(0))
        .unwrap();
    assert_eq!(attr(&tuesday, "kind"), "p");
    assert_eq!(
        attr(&tuesday, "aria"),
        "15:00\u{2013}15:45, Meeting p, Ana Lopez, Waiting for a decision"
    );
    // Calendar time is anonymous: a time range and "unavailable", no title.
    let wednesday = day(2)
        .get_attr("busy")
        .unwrap()
        .get_item(&minijinja::Value::from(0))
        .unwrap();
    assert_eq!(
        attr(&wednesday, "tip"),
        "00:00\u{2013}24:00, Unavailable (your calendar)"
    );
    assert!(ctx.get_attr("has_pending").unwrap().is_true());
    assert_eq!(attr(&ctx, "total"), "2 h 15 min booked");

    // A Sunday week start puts Sunday first.
    let sunday_first = ov::week_context(
        "en",
        &hours,
        &bookings,
        &busy,
        &[],
        paris,
        now,
        WeekStart::Sun,
    )
    .unwrap();
    let first = sunday_first
        .get_attr("days")
        .unwrap()
        .get_item(&minijinja::Value::from(0))
        .unwrap();
    assert_eq!(attr(&first, "weekday"), "Sun");

    // Nothing open, booked or in the calendar: no week view at all.
    assert!(ov::week_context("en", &[], &[], &[], &[], paris, now, WeekStart::Mon).is_none());
}

#[test]
fn masthead_keeps_to_the_date_and_one_line() {
    let today = NaiveDate::from_ymd_opt(2026, 11, 7).unwrap();
    let quiet = ov::masthead_context("en", today, None);
    assert_eq!(
        quiet.get_attr("date").unwrap().to_string(),
        "Saturday, November 7"
    );
    assert!(quiet.get_attr("next").unwrap().is_none());
    let next = minijinja::context! {
        title => "Intro call",
        guest_name => "Jonas Weber",
        day => "2026-11-09",
        day_short => "Mon 9",
        time => "10:00",
        ongoing => false,
    };
    let mast = ov::masthead_context("en", today, Some(&next));
    assert_eq!(
        mast.get_attr("next").unwrap().to_string(),
        "Next: Intro call with Jonas Weber, Mon 9 at 10:00"
    );
}

fn hm(h: u32, m: u32) -> NaiveTime {
    NaiveTime::from_hms_opt(h, m, 0).unwrap()
}

#[test]
fn open_minutes_merges_types_and_follows_overrides_and_zones() {
    let monday = NaiveDate::from_ymd_opt(2026, 11, 2).unwrap();
    let paris: Tz = "Europe/Paris".parse().unwrap();
    let a = ov::EventTypeHours {
        tz: paris,
        rules: vec![(1, hm(9, 0), hm(12, 0)), (1, hm(14, 0), hm(17, 0))],
        overrides: vec![],
        bookable_from: None,
        bookable_until: None,
    };
    // Overlaps the first window of `a`: the union is one band.
    let b = ov::EventTypeHours {
        tz: paris,
        rules: vec![(1, hm(11, 0), hm(13, 0))],
        overrides: vec![],
        bookable_from: None,
        bookable_until: None,
    };
    let open = ov::open_minutes(&[a.clone(), b.clone()], monday, paris);
    assert_eq!(open, vec![(540, 780), (840, 1020)]);

    // A blocked override closes the day for that type only.
    let mut blocked = a.clone();
    blocked.overrides = vec![(monday, None, true)];
    assert_eq!(
        ov::open_minutes(&[blocked, b.clone()], monday, paris),
        vec![(660, 780)]
    );
    // Custom hours replace the weekly rules.
    let mut custom = a.clone();
    custom.overrides = vec![(monday, Some((hm(8, 0), hm(9, 0))), false)];
    assert_eq!(ov::open_minutes(&[custom], monday, paris), vec![(480, 540)]);
    // Outside the bookable range: closed.
    let mut ended = a.clone();
    ended.bookable_until = NaiveDate::from_ymd_opt(2026, 11, 1);
    assert!(ov::open_minutes(&[ended], monday, paris).is_empty());
    // Read from New York, Paris 09:00 is 03:00 (UTC+1 vs UTC-5 in November).
    let ny: Tz = "America/New_York".parse().unwrap();
    assert_eq!(ov::open_minutes(&[a], monday, ny)[0], (180, 360));
}

#[test]
fn ruler_widens_to_fit_and_keeps_calendar_events_anonymous() {
    let blocks = vec![
        ov::RulerBlock {
            start: 6 * 60,
            end: 6 * 60 + 30,
            kind: "c",
            id: "b1".into(),
            listed: true,
            title: "Early call".into(),
            guest: "Ana".into(),
        },
        ov::RulerBlock {
            start: 10 * 60,
            end: 11 * 60,
            kind: "u",
            id: String::new(),
            listed: false,
            title: String::new(),
            guest: String::new(),
        },
    ];
    let ctx = ov::ruler_context("en", &[(540, 1080)], &blocks, 9 * 60).unwrap();
    // 06:00 is before the default 07:00 start: the scale starts at 06.
    assert_eq!(ctx.get_attr("from_minute").unwrap().as_i64(), Some(360));
    let rendered = ctx.get_attr("blocks").unwrap().to_string();
    assert!(rendered.contains("Early call"));
    let labels = ctx.get_attr("labels").unwrap();
    assert_eq!(labels.len(), Some(1), "only bookings get a label");
    // Nothing open and nothing booked: no ruler at all.
    assert!(ov::ruler_context("en", &[], &[], 600).is_none());
}

#[tokio::test]
async fn calendar_busy_leaves_out_written_back_bookings() {
    let pool = setup_test_db().await;
    let me = insert_role_user(&pool, "me@example.com", "user").await;
    let et = insert_personal_et(&pool, &me, "call").await;
    let (acc,): (String,) = sqlx::query_as("SELECT id FROM accounts WHERE user_id = ?")
        .bind(&me)
        .fetch_one(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO caldav_sources (id, account_id, name, url, username) VALUES ('s', ?, 'S', 'https://dav.example', 'u')")
        .bind(&acc)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO calendars (id, source_id, href, is_busy) VALUES ('c', 's', '/c/', 1)")
        .execute(&pool)
        .await
        .unwrap();
    let day = NaiveDate::from_ymd_opt(2026, 11, 2).unwrap();
    for (uid, start, end) in [
        ("lunch", "20261102T120000", "20261102T130000"),
        ("copy", "20261102T150000", "20261102T153000"),
    ] {
        sqlx::query(
            "INSERT INTO events (id, calendar_id, uid, start_at, end_at) VALUES (?, 'c', ?, ?, ?)",
        )
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(uid)
        .bind(start)
        .bind(end)
        .execute(&pool)
        .await
        .unwrap();
    }
    sqlx::query("INSERT INTO bookings (id, event_type_id, uid, guest_name, guest_email, guest_timezone, start_at, end_at, status, cancel_token, reschedule_token) VALUES ('b', ?, 'copy', 'G', 'g@e.com', 'UTC', '2026-11-02T15:00:00', '2026-11-02T15:30:00', 'confirmed', 'x', 'y')")
        .bind(&et)
        .execute(&pool)
        .await
        .unwrap();
    let from = day.and_time(NaiveTime::MIN);
    let busy = ov::fetch_calendar_busy(&pool, &me, Tz::UTC, from, from + Duration::days(1)).await;
    assert_eq!(busy, vec![(dt(2026, 11, 2, 12, 0), dt(2026, 11, 2, 13, 0))]);
}

fn ruler_block(start: i64, end: i64, id: &str) -> ov::RulerBlock {
    ov::RulerBlock {
        start,
        end,
        kind: "c",
        id: id.into(),
        listed: true,
        title: format!("Meeting {id}"),
        guest: "Ana".into(),
    }
}

/// Every label's room as (left, max_width), in percent of the track.
fn label_rooms(ctx: &minijinja::Value) -> Vec<(f64, f64)> {
    let labels = ctx.get_attr("labels").unwrap();
    (0..labels.len().unwrap())
        .map(|i| {
            let l = labels.get_item_by_index(i).unwrap();
            let num = |k: &str| l.get_attr(k).unwrap().to_string().parse::<f64>().unwrap();
            (num("left"), num("max_width"))
        })
        .collect()
}

#[test]
fn ruler_labels_near_the_end_of_the_span_never_panic() {
    // 19:40 to 20:00 on the default 07:00 to 20:00 span: the label starts
    // past 96% of the track, which used to panic in f64::clamp.
    let open = [(9 * 60, 17 * 60)];
    let late = [ruler_block(19 * 60 + 40, 20 * 60, "late")];
    let ctx = ov::ruler_context("en", &open, &late, 12 * 60).unwrap();
    assert_eq!(
        ctx.get_attr("span_minutes").unwrap().as_i64(),
        Some(13 * 60)
    );
    // One ending at the very end of the span, and one at midnight, with a
    // label lane shared by several bookings.
    let blocks = [
        ruler_block(19 * 60, 20 * 60, "a"),
        ruler_block(19 * 60 + 50, 20 * 60, "b"),
        ruler_block(23 * 60 + 50, 24 * 60, "c"),
    ];
    for blocks in [&late[..], &blocks[..]] {
        let ctx = ov::ruler_context("en", &open, blocks, 12 * 60).unwrap();
        for (left, room) in label_rooms(&ctx) {
            assert!(room >= 0.0, "room {room} at {left}");
            assert!(left + room <= 100.0 + 1e-6, "room {room} at {left}");
        }
    }
}

/// A collective team event type with `me` and `other` as members, `other`
/// excluded from it (weight 0), open Monday to Sunday 09:00 to 17:00.
async fn collective_with_excluded_member(pool: &SqlitePool, me: &str, other: &str) -> String {
    let (_, et) = insert_team_with_et(
        pool,
        "duo",
        "duo-call",
        &[(me, "member"), (other, "member")],
    )
    .await;
    sqlx::query("UPDATE event_types SET scheduling_mode = 'collective' WHERE id = ?")
        .bind(&et)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO event_type_member_weights (event_type_id, user_id, weight) VALUES (?, ?, 0)",
    )
    .bind(&et)
    .bind(other)
    .execute(pool)
    .await
    .unwrap();
    for day in 0..7 {
        sqlx::query("INSERT INTO availability_rules (id, event_type_id, day_of_week, start_time, end_time) VALUES (?, ?, ?, '09:00', '17:00')")
            .bind(uuid::Uuid::new_v4().to_string())
            .bind(&et)
            .bind(day)
            .execute(pool)
            .await
            .unwrap();
    }
    et
}

#[tokio::test]
async fn members_excluded_from_a_collective_type_see_neither_its_bookings_nor_its_hours() {
    let pool = setup_test_db().await;
    let me = insert_role_user(&pool, "me@example.com", "user").await;
    let other = insert_role_user(&pool, "other@example.com", "user").await;
    let et = collective_with_excluded_member(&pool, &me, &other).await;
    let start = Utc::now() + Duration::days(1);
    insert_stat_booking(&pool, &et, "Collective", start, "confirmed", None, None).await;

    let (from, to) = (
        (start - Duration::days(1)).naive_utc(),
        (start + Duration::days(1)).naive_utc(),
    );
    for (user, expected) in [(&me, 1), (&other, 0)] {
        let rows = ov::fetch_stat_rows(&pool, user, Tz::UTC, Utc::now())
            .await
            .unwrap();
        assert_eq!(rows.len(), expected, "stat rows for {user}");
        let day = ov::fetch_day_bookings(&pool, user, Tz::UTC, from, to).await;
        assert_eq!(day.len(), expected, "ruler bookings for {user}");
        let agenda = ov::agenda(&pool, user, Tz::UTC, Utc::now(), "en").await;
        assert_eq!(agenda.len(), expected, "agenda for {user}");
        assert_eq!(ov::has_any_booking(&pool, user).await, expected == 1);
        let hours = ov::load_event_type_hours(&pool, user).await;
        assert_eq!(hours.len(), expected, "open hours for {user}");
    }
}

#[tokio::test]
async fn overview_links_only_the_bookings_the_bookings_page_lists() {
    let (app, pool, session, et_id) = setup_test_app().await;
    let (me,): (String,) = sqlx::query_as("SELECT user_id FROM sessions WHERE id = ?")
        .bind(&session)
        .fetch_one(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE users SET timezone = 'UTC' WHERE id = ?")
        .bind(&me)
        .execute(&pool)
        .await
        .unwrap();
    let other = insert_role_user(&pool, "other@example.com", "user").await;
    let team_et = collective_with_excluded_member(&pool, &me, &other).await;
    // Both on today's ruler: started ten minutes ago, or at midnight.
    let now = Utc::now();
    let midnight = now.date_naive().and_time(NaiveTime::MIN).and_utc();
    let start = (now - Duration::minutes(10)).max(midnight);
    insert_stat_booking(&pool, &et_id, "Own Pending", start, "pending", None, None).await;
    insert_stat_booking(&pool, &team_et, "Team Call", start, "confirmed", None, None).await;
    let id_of = |guest: &'static str| {
        let pool = pool.clone();
        async move {
            let (id,): (String,) = sqlx::query_as("SELECT id FROM bookings WHERE guest_name = ?")
                .bind(guest)
                .fetch_one(&pool)
                .await
                .unwrap();
            id
        }
    };
    let (own, team) = (id_of("Own Pending").await, id_of("Team Call").await);

    let (from, to) = (
        now.naive_utc() - Duration::days(1),
        now.naive_utc() + Duration::days(1),
    );
    let day = ov::fetch_day_bookings(&pool, &me, Tz::UTC, from, to).await;
    let listed = |id: &str| day.iter().find(|b| b.id == id).map(|b| b.listed);
    assert_eq!(
        listed(&own),
        Some(true),
        "own pending: on the bookings page"
    );
    assert_eq!(listed(&team), Some(false), "collective: not on that page");

    let body = body_string(
        app.clone()
            .oneshot(get_authed("/dashboard", &session))
            .await
            .unwrap(),
    )
    .await;
    assert!(body.contains(&format!("href=\"/dashboard/bookings#b-{own}\"")));
    assert!(
        !body.contains(&format!("#b-{team}")),
        "no link to a row that is not there"
    );
    // The unlinked block keeps its tooltip.
    assert!(body.contains(", T, Team Call, Booked\""));

    let bookings = body_string(
        app.oneshot(get_authed("/dashboard/bookings", &session))
            .await
            .unwrap(),
    )
    .await;
    assert!(bookings.contains(&format!("id=\"b-{own}\"")));
    assert!(!bookings.contains(&format!("id=\"b-{team}\"")));
}

#[tokio::test]
async fn calendar_busy_keeps_the_copy_of_a_cancelled_booking() {
    let pool = setup_test_db().await;
    let me = insert_role_user(&pool, "me@example.com", "user").await;
    let other = insert_role_user(&pool, "other@example.com", "user").await;
    give_account(&pool, &other).await;
    let et = insert_personal_et(&pool, &me, "call").await;
    let theirs = insert_personal_et(&pool, &other, "theirs").await;
    let (acc,): (String,) = sqlx::query_as("SELECT id FROM accounts WHERE user_id = ?")
        .bind(&me)
        .fetch_one(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO caldav_sources (id, account_id, name, url, username) VALUES ('s', ?, 'S', 'https://dav.example', 'u')")
        .bind(&acc)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO calendars (id, source_id, href, is_busy) VALUES ('c', 's', '/c/', 1)")
        .execute(&pool)
        .await
        .unwrap();
    // A live copy (dropped), a cancelled booking's leftover copy and the
    // copy of a booking someone else hosts (both kept as busy).
    for (uid, start, end, status, owner) in [
        (
            "live",
            "20261102T090000",
            "20261102T093000",
            "confirmed",
            &et,
        ),
        (
            "gone",
            "20261102T120000",
            "20261102T123000",
            "cancelled",
            &et,
        ),
        (
            "foreign",
            "20261102T150000",
            "20261102T153000",
            "confirmed",
            &theirs,
        ),
    ] {
        sqlx::query(
            "INSERT INTO events (id, calendar_id, uid, start_at, end_at) VALUES (?, 'c', ?, ?, ?)",
        )
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(uid)
        .bind(start)
        .bind(end)
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query("INSERT INTO bookings (id, event_type_id, uid, guest_name, guest_email, guest_timezone, start_at, end_at, status, cancel_token, reschedule_token) VALUES (?, ?, ?, 'G', 'g@e.com', 'UTC', '2026-11-02T09:00:00', '2026-11-02T09:30:00', ?, ?, ?)")
            .bind(uuid::Uuid::new_v4().to_string())
            .bind(owner)
            .bind(uid)
            .bind(status)
            .bind(uuid::Uuid::new_v4().to_string())
            .bind(uuid::Uuid::new_v4().to_string())
            .execute(&pool)
            .await
            .unwrap();
    }
    let from = dt(2026, 11, 2, 0, 0);
    let busy = ov::fetch_calendar_busy(&pool, &me, Tz::UTC, from, from + Duration::days(1)).await;
    assert_eq!(
        busy,
        vec![
            (dt(2026, 11, 2, 12, 0), dt(2026, 11, 2, 12, 30)),
            (dt(2026, 11, 2, 15, 0), dt(2026, 11, 2, 15, 30)),
        ]
    );
}

/// A calendar the owner stopped syncing (#324) never shows as busy, like in
/// the slot engine, even if events written by a sync in flight remain.
#[tokio::test]
async fn calendar_busy_skips_calendars_that_are_not_synced() {
    let pool = setup_test_db().await;
    let me = insert_role_user(&pool, "me@example.com", "user").await;
    insert_personal_et(&pool, &me, "call").await;
    let (acc,): (String,) = sqlx::query_as("SELECT id FROM accounts WHERE user_id = ?")
        .bind(&me)
        .fetch_one(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO caldav_sources (id, account_id, name, url, username) VALUES ('s', ?, 'S', 'https://dav.example', 'u')")
        .bind(&acc)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO calendars (id, source_id, href, is_busy, sync_enabled) VALUES ('on', 's', '/on/', 1, 1), ('off', 's', '/off/', 1, 0)")
        .execute(&pool)
        .await
        .unwrap();
    for (cal, start, end) in [
        ("on", "20261102T090000", "20261102T093000"),
        ("off", "20261102T120000", "20261102T123000"),
    ] {
        sqlx::query(
            "INSERT INTO events (id, calendar_id, uid, start_at, end_at) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(cal)
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(start)
        .bind(end)
        .execute(&pool)
        .await
        .unwrap();
    }
    let from = dt(2026, 11, 2, 0, 0);
    let busy = ov::fetch_calendar_busy(&pool, &me, Tz::UTC, from, from + Duration::days(1)).await;
    assert_eq!(busy, vec![(dt(2026, 11, 2, 9, 0), dt(2026, 11, 2, 9, 30))]);
}

#[tokio::test]
async fn overview_counts_every_pending_request_like_the_sidebar() {
    let (app, pool, session, et_id) = setup_test_app().await;
    insert_stat_booking(
        &pool,
        &et_id,
        "Readable",
        Utc::now() + Duration::days(2),
        "pending",
        None,
        None,
    )
    .await;
    // A legacy row whose stored time cannot be read.
    sqlx::query("INSERT INTO bookings (id, event_type_id, uid, guest_name, guest_email, guest_timezone, start_at, end_at, status, cancel_token, reschedule_token) VALUES ('bad', ?, 'bad', 'Unreadable', 'g@e.com', 'UTC', 'not a date', 'not a date', 'pending', 'bx', 'by')")
        .bind(&et_id)
        .execute(&pool)
        .await
        .unwrap();
    let body = body_string(
        app.oneshot(get_authed("/dashboard", &session))
            .await
            .unwrap(),
    )
    .await;
    assert!(body.contains("Readable"));
    assert!(
        body.contains("<span class=\"num\">2</span></h2>"),
        "both pending rows counted"
    );
}
