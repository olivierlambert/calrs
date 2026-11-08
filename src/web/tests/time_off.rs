//! Time off (#46): the dashboard page, who may see and change a period, and
//! every slot path a period blocks.

use super::*;
use crate::web::overview_stats as ov;

async fn add_off(
    pool: &SqlitePool,
    user_id: &str,
    start: NaiveDateTime,
    end: NaiveDateTime,
    label: Option<&str>,
) -> String {
    crate::time_off::add(pool, user_id, start.and_utc(), end.and_utc(), "UTC", label)
        .await
        .unwrap()
        .expect("under the cap")
}

async fn test_user_id(pool: &SqlitePool) -> String {
    sqlx::query_scalar("SELECT id FROM users WHERE username = 'testuser'")
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn time_off_count(pool: &SqlitePool) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM user_time_off")
        .fetch_one(pool)
        .await
        .unwrap()
}

fn monday() -> NaiveDate {
    NaiveDate::parse_from_str(&far_future_monday(), "%Y-%m-%d").unwrap()
}

async fn booking_count(pool: &SqlitePool, et_id: &str) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM bookings WHERE event_type_id = ?")
        .bind(et_id)
        .fetch_one(pool)
        .await
        .unwrap()
}

#[tokio::test]
async fn the_page_lists_only_the_users_own_periods() {
    let (app, pool, session, _) = setup_test_app().await;
    let me = test_user_id(&pool).await;
    let (bob, _) = insert_test_user_with_session(&pool, "Bob", "bob@test.com", "bob").await;
    let day = monday().and_time(NaiveTime::MIN);
    add_off(
        &pool,
        &me,
        day,
        day + Duration::days(3),
        Some("Ski in Zermatt"),
    )
    .await;
    add_off(
        &pool,
        &bob,
        day,
        day + Duration::days(1),
        Some("Bob private"),
    )
    .await;
    // Ended: not listed.
    let past = (Utc::now() - Duration::days(3)).naive_utc();
    add_off(
        &pool,
        &me,
        past,
        past + Duration::hours(2),
        Some("Long gone"),
    )
    .await;

    let response = app
        .oneshot(get_authed("/dashboard/time-off", &session))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = body_string(response).await;
    assert!(body.contains("Ski in Zermatt"), "own period listed");
    assert!(
        !body.contains("Bob private"),
        "someone else's period leaked"
    );
    assert!(!body.contains("Long gone"), "an ended period is hidden");
    assert!(body.contains("action=\"/dashboard/time-off\""));
    assert!(body.contains("are kept"), "the page says bookings are kept");
    let range = format!(
        "{} to {}",
        crate::i18n::format_date(monday(), "en"),
        crate::i18n::format_date(monday() + Duration::days(2), "en")
    );
    assert!(body.contains(&range), "whole days shown inclusive: {range}");
}

#[tokio::test]
async fn adding_needs_csrf_and_valid_input() {
    let (app, pool, session, _) = setup_test_app().await;
    let me = test_user_id(&pool).await;
    let start = monday();
    let end = start + Duration::days(2);
    let fields = format!("start_date={start}&end_date={end}&label=Ski");

    // A token that does not match the cookie is refused.
    let response = app
        .clone()
        .oneshot(post_form(
            "/dashboard/time-off",
            &session,
            "right",
            &format!("_csrf=wrong&{fields}"),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), 403);
    assert_eq!(time_off_count(&pool).await, 0);

    let response = app
        .clone()
        .oneshot(post_form(
            "/dashboard/time-off",
            &session,
            "tok",
            &format!("_csrf=tok&{fields}"),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), 303);
    assert_eq!(
        response.headers().get("location").unwrap(),
        "/dashboard/time-off?saved=added"
    );
    let stored: (String, String, String, Option<String>) =
        sqlx::query_as("SELECT user_id, start_at, end_at, label FROM user_time_off")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(stored.0, me);
    assert_eq!(stored.1, format!("{start}T00:00:00Z"));
    assert_eq!(stored.2, format!("{}T00:00:00Z", end + Duration::days(1)));
    assert_eq!(stored.3.as_deref(), Some("Ski"));

    // Refused input renders the page again with the reason and what was typed.
    for (body, message) in [
        (
            format!("_csrf=tok&start_date={end}&end_date={start}"),
            "The end must be after the start.",
        ),
        (
            "_csrf=tok&start_date=2020-01-01&end_date=2020-01-02".to_string(),
            "This period has already ended.",
        ),
        (
            format!("_csrf=tok&start_date={start}&label={}", "x".repeat(201)),
            "The note can be at most 200 characters.",
        ),
        (
            "_csrf=tok&start_date=tomorrow".to_string(),
            "Enter the dates as YYYY-MM-DD",
        ),
    ] {
        let response = app
            .clone()
            .oneshot(post_form("/dashboard/time-off", &session, "tok", &body))
            .await
            .unwrap();
        assert_eq!(response.status(), 200, "{body}");
        let page = body_string(response).await;
        assert!(page.contains(message), "{message} for {body}");
    }
    assert_eq!(time_off_count(&pool).await, 1);
}

#[tokio::test]
async fn a_user_cannot_delete_someone_elses_period() {
    let (app, pool, session, _) = setup_test_app().await;
    let (bob, bob_session) =
        insert_test_user_with_session(&pool, "Bob", "bob@test.com", "bob").await;
    let day = monday().and_time(NaiveTime::MIN);
    let id = add_off(&pool, &bob, day, day + Duration::days(1), None).await;
    let uri = format!("/dashboard/time-off/{id}/delete");

    let response = app
        .clone()
        .oneshot(post_form(&uri, &session, "tok", "_csrf=tok"))
        .await
        .unwrap();
    assert_eq!(response.status(), 303);
    assert_eq!(
        time_off_count(&pool).await,
        1,
        "another user's period stays"
    );

    // Without a CSRF token the owner cannot delete it either.
    let response = app
        .clone()
        .oneshot(post_form(&uri, &bob_session, "tok", ""))
        .await
        .unwrap();
    assert_eq!(response.status(), 403);
    assert_eq!(time_off_count(&pool).await, 1);

    let response = app
        .oneshot(post_form(&uri, &bob_session, "tok", "_csrf=tok"))
        .await
        .unwrap();
    assert_eq!(
        response.headers().get("location").unwrap(),
        "/dashboard/time-off?saved=deleted"
    );
    assert_eq!(time_off_count(&pool).await, 0);
}

#[tokio::test]
async fn busy_times_include_time_off() {
    let pool = setup_test_db().await;
    let (user, _, et_id) = seed_test_data(&pool).await;
    let day = monday();
    add_off(
        &pool,
        &user,
        day.and_hms_opt(12, 0, 0).unwrap(),
        day.and_hms_opt(14, 0, 0).unwrap(),
        None,
    )
    .await;
    let paris = chrono_tz::Europe::Paris;
    let busy = fetch_busy_times_for_user(
        &pool,
        &user,
        day.and_time(NaiveTime::MIN),
        day.and_hms_opt(23, 59, 59).unwrap(),
        paris,
        Some(&et_id),
    )
    .await;
    // Stored in UTC, read back in the caller's zone.
    let offset = paris
        .from_utc_datetime(&day.and_hms_opt(12, 0, 0).unwrap())
        .naive_local()
        - day.and_hms_opt(12, 0, 0).unwrap();
    assert!(busy.contains(&(
        day.and_hms_opt(12, 0, 0).unwrap() + offset,
        day.and_hms_opt(14, 0, 0).unwrap() + offset
    )));
}

#[tokio::test]
async fn time_off_refuses_the_booking_without_saying_why() {
    let (app, pool, _, et_id) = setup_test_app().await;
    let me = test_user_id(&pool).await;
    let day = monday().and_time(NaiveTime::MIN);
    let id = add_off(&pool, &me, day, day + Duration::days(1), Some("Dentist")).await;

    let csrf = "csrf-off";
    let uri = "/u/testuser/test-meeting/book";
    let response = app
        .clone()
        .oneshot(post_form_unauthed(uri, csrf, &book_body(csrf)))
        .await
        .unwrap();
    let body = body_string(response).await;
    assert_eq!(booking_count(&pool, &et_id).await, 0, "slot is off");
    assert!(!body.contains("Dentist"), "the guest never sees the note");

    // The slot page renders without the note either.
    let response = app
        .clone()
        .oneshot(get(&format!(
            "/u/testuser/test-meeting?month={}",
            &far_future_monday()[..7]
        )))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    assert!(!body_string(response).await.contains("Dentist"));

    // Back from time off: the same slot is bookable.
    crate::time_off::delete(&pool, &me, &id).await.unwrap();
    app.oneshot(post_form_unauthed(uri, csrf, &book_body(csrf)))
        .await
        .unwrap();
    assert_eq!(booking_count(&pool, &et_id).await, 1);
}

#[tokio::test]
async fn existing_bookings_inside_a_new_period_are_kept() {
    let (app, pool, session, et_id) = setup_test_app().await;
    let start = monday().and_hms_opt(10, 0, 0).unwrap();
    insert_booking(&pool, &et_id, None, start, "confirmed").await;
    let day = monday();
    app.oneshot(post_form(
        "/dashboard/time-off",
        &session,
        "tok",
        &format!("_csrf=tok&start_date={day}"),
    ))
    .await
    .unwrap();
    assert_eq!(time_off_count(&pool).await, 1);
    let status: String = sqlx::query_scalar("SELECT status FROM bookings WHERE event_type_id = ?")
        .bind(&et_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(status, "confirmed");
}

#[tokio::test]
async fn round_robin_skips_the_member_off_and_collective_loses_the_slot() {
    let pool = setup_test_db().await;
    let alice = insert_role_user(&pool, "alice@test.com", "user").await;
    let bob = insert_role_user(&pool, "bob@test.com", "user").await;
    let (team_id, et_id) = insert_team_with_et(
        &pool,
        "sales",
        "demo",
        &[(&alice, "member"), (&bob, "member")],
    )
    .await;
    let day = monday();
    let slot_start = day.and_hms_opt(10, 0, 0).unwrap();
    let slot_end = slot_start + Duration::minutes(30);
    let utc = chrono_tz::Tz::UTC;
    add_off(
        &pool,
        &alice,
        day.and_time(NaiveTime::MIN),
        day.and_time(NaiveTime::MIN) + Duration::days(1),
        None,
    )
    .await;

    let picked = pick_group_member(
        &pool, &team_id, &et_id, slot_start, slot_end, slot_start, 0, 0, utc,
    )
    .await
    .unwrap()
    .map(|(id, _, _)| id);
    assert_eq!(picked.as_deref(), Some(bob.as_str()), "Bob takes it");

    let window = (
        day.and_time(NaiveTime::MIN),
        day.and_hms_opt(23, 59, 59).unwrap(),
    );
    let rr = team_busy_source(&pool, &et_id, &team_id, false, window.0, window.1, utc).await;
    assert!(busy_source_is_free(&rr, slot_start, slot_end));
    let collective = team_busy_source(&pool, &et_id, &team_id, true, window.0, window.1, utc).await;
    assert!(!busy_source_is_free(&collective, slot_start, slot_end));

    add_off(&pool, &bob, slot_start, slot_end, None).await;
    assert!(pick_group_member(
        &pool, &team_id, &et_id, slot_start, slot_end, slot_start, 0, 0, utc
    )
    .await
    .unwrap()
    .is_none());
}

#[tokio::test]
async fn approval_recheck_refuses_a_host_on_time_off() {
    let pool = setup_test_db().await;
    let (user, _, et_id) = seed_test_data(&pool).await;
    let start = monday().and_hms_opt(10, 0, 0).unwrap();
    let (start_at, end_at) = (
        start.format("%Y-%m-%dT%H:%M:%S").to_string(),
        (start + Duration::minutes(30))
            .format("%Y-%m-%dT%H:%M:%S")
            .to_string(),
    );
    let (booking_id, _, _) =
        insert_test_booking(&pool, &et_id, &start_at, &end_at, "pending").await;
    let hosts = RescheduleHosts {
        user_ids: vec![user.clone()],
        team_id: None,
        collective: false,
        participants: Vec::new(),
    };
    assert!(approval_hosts_free(
        &pool,
        &hosts,
        &booking_id,
        "uid",
        &et_id,
        &start_at,
        &end_at
    )
    .await
    .unwrap());
    add_off(&pool, &user, start, start + Duration::hours(1), None).await;
    assert!(!approval_hosts_free(
        &pool,
        &hosts,
        &booking_id,
        "uid",
        &et_id,
        &start_at,
        &end_at
    )
    .await
    .unwrap());
}

#[tokio::test]
async fn troubleshoot_labels_time_off_with_the_users_note() {
    let (app, pool, session, _) = setup_test_app().await;
    let me = test_user_id(&pool).await;
    let day = monday();
    add_off(
        &pool,
        &me,
        day.and_hms_opt(12, 0, 0).unwrap(),
        day.and_hms_opt(14, 0, 0).unwrap(),
        Some("Lunch with Sam"),
    )
    .await;
    // A booking kept inside the period is labelled as the booking.
    let et_id: String =
        sqlx::query_scalar("SELECT id FROM event_types WHERE slug = 'test-meeting'")
            .fetch_one(&pool)
            .await
            .unwrap();
    insert_booking(
        &pool,
        &et_id,
        None,
        day.and_hms_opt(13, 0, 0).unwrap(),
        "confirmed",
    )
    .await;
    let response = app
        .oneshot(get_authed(
            &format!("/dashboard/troubleshoot?date={day}&event_type=test-meeting"),
            &session,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = body_string(response).await;
    assert!(body.contains("ts-time_off"), "time off block drawn");
    assert!(body.contains("12:00&ndash;13:00: Time off"), "{body}");
    assert!(
        body.contains("13:00&ndash;13:30: G"),
        "the booking wins its slot"
    );
    assert!(body.contains("13:30&ndash;14:00: Time off"));
    assert!(body.contains("Lunch with Sam"));
}

#[tokio::test]
async fn the_overview_renders_with_time_off_today() {
    let (app, pool, session, _) = setup_test_app().await;
    let me = test_user_id(&pool).await;
    let now = Utc::now().naive_utc();
    add_off(
        &pool,
        &me,
        now - Duration::hours(1),
        now + Duration::hours(1),
        None,
    )
    .await;
    let response = app
        .oneshot(get_authed("/dashboard", &session))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
}

#[test]
fn time_off_closes_open_hours() {
    let day = monday();
    let at = |h, m| day.and_hms_opt(h, m, 0).unwrap();
    let open = vec![(9 * 60, 12 * 60), (14 * 60, 18 * 60)];
    // Lunch extended to 15:00, and the evening off from 17:00.
    let off = vec![(at(11, 0), at(15, 0)), (at(17, 0), at(23, 0))];
    assert_eq!(
        ov::without_time_off(open.clone(), day, &off),
        vec![(9 * 60, 11 * 60), (15 * 60, 17 * 60)]
    );
    // A whole day off closes it; time off on other days changes nothing.
    let whole = vec![(day.and_time(NaiveTime::MIN), at(0, 0) + Duration::days(1))];
    assert!(ov::without_time_off(open.clone(), day, &whole).is_empty());
    let elsewhere = vec![(at(9, 0) + Duration::days(2), at(18, 0) + Duration::days(2))];
    assert_eq!(ov::without_time_off(open.clone(), day, &elsewhere), open);
}

#[test]
fn periods_read_as_typed() {
    let period = |start: &str, end: &str| crate::time_off::Period {
        id: String::new(),
        start: chrono::DateTime::parse_from_rfc3339(start)
            .unwrap()
            .with_timezone(&Utc),
        end: chrono::DateTime::parse_from_rfc3339(end)
            .unwrap()
            .with_timezone(&Utc),
        tz: chrono_tz::Europe::Paris,
        timezone: "Europe/Paris".into(),
        label: None,
    };
    // Whole days: the exclusive midnight end is shown as the day before.
    let days = period("2026-12-20T23:00:00Z", "2026-12-24T23:00:00Z");
    assert_eq!(
        period_range_label(&days, "en"),
        "December 21, 2026 to December 24, 2026"
    );
    assert_eq!(
        period_range_label(&days, "fr"),
        "Du 21 décembre 2026 au 24 décembre 2026"
    );
    let one = period("2026-12-20T23:00:00Z", "2026-12-21T23:00:00Z");
    assert_eq!(period_range_label(&one, "en"), "December 21, 2026");
    let afternoon = period("2026-12-21T13:00:00Z", "2026-12-21T16:30:00Z");
    assert_eq!(
        period_range_label(&afternoon, "en"),
        "December 21, 2026, 14:00 to 17:30"
    );
    // A start time alone runs to the end of that day, an end time alone
    // from its start.
    let from = period("2026-12-21T13:00:00Z", "2026-12-21T23:00:00Z");
    assert_eq!(
        period_range_label(&from, "en"),
        "December 21, 2026, from 14:00"
    );
    assert_eq!(
        period_range_label(&from, "fr"),
        "21 décembre 2026, à partir de 14:00"
    );
    let until = period("2026-12-20T23:00:00Z", "2026-12-21T11:00:00Z");
    assert_eq!(
        period_range_label(&until, "en"),
        "December 21, 2026, until 12:00"
    );
    let from_to_day = period("2026-12-21T13:00:00Z", "2026-12-23T23:00:00Z");
    assert_eq!(
        period_range_label(&from_to_day, "en"),
        "December 21, 2026, 14:00 to December 23, 2026"
    );
    let across = period("2026-12-21T13:00:00Z", "2026-12-23T11:00:00Z");
    assert_eq!(
        period_range_label(&across, "en"),
        "December 21, 2026, 14:00 to December 23, 2026, 12:00"
    );
}

/// In Santiago 6 September 2026 starts at 01:00 (the clock change is at
/// midnight): a whole day there is still shown as a date, not as times.
#[test]
fn whole_days_read_as_dates_across_a_midnight_clock_change() {
    let santiago = chrono_tz::America::Santiago;
    let (start, end) = crate::time_off::parse_period(
        "2026-09-06",
        "",
        "",
        "",
        santiago,
        chrono::DateTime::parse_from_rfc3339("2026-08-01T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc),
    )
    .unwrap();
    let p = crate::time_off::Period {
        id: String::new(),
        start,
        end,
        tz: santiago,
        timezone: "America/Santiago".into(),
        label: None,
    };
    assert_eq!(period_range_label(&p, "en"), "September 6, 2026");
}

#[tokio::test]
async fn an_impersonating_admin_reads_without_notes_and_cannot_change() {
    let (app, pool, admin_session, _) = setup_test_app().await;
    let (bob, _) = insert_test_user_with_session(&pool, "Bob", "bob@test.com", "bob").await;
    let day = monday().and_time(NaiveTime::MIN);
    let id = add_off(
        &pool,
        &bob,
        day,
        day + Duration::days(1),
        Some("Bob private"),
    )
    .await;
    let as_bob = |method: &str, uri: &str, body: &str| {
        axum::http::Request::builder()
            .method(method)
            .uri(uri)
            .header(
                "cookie",
                format!(
                    "__Host-calrs_session={admin_session}; __Host-calrs_impersonate={bob}; __Host-calrs_csrf=tok"
                ),
            )
            .header("content-type", "application/x-www-form-urlencoded")
            .body(Body::from(body.to_string()))
            .unwrap()
    };

    let response = app
        .clone()
        .oneshot(as_bob("GET", "/dashboard/time-off", ""))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let page = body_string(response).await;
    assert!(page.contains("You are viewing this page as Bob"));
    assert!(!page.contains("Bob private"), "the note stays the user's");
    assert!(
        !page.contains("action=\"/dashboard/time-off\""),
        "no add form"
    );
    assert!(!page.contains("/delete"), "no delete button");

    let start = monday();
    for (uri, body) in [
        (
            "/dashboard/time-off".to_string(),
            format!("_csrf=tok&start_date={start}"),
        ),
        (
            format!("/dashboard/time-off/{id}/delete"),
            "_csrf=tok".to_string(),
        ),
    ] {
        let response = app
            .clone()
            .oneshot(as_bob("POST", &uri, &body))
            .await
            .unwrap();
        assert_eq!(response.status(), 403, "{uri}");
    }
    assert_eq!(time_off_count(&pool).await, 1);
    let label: Option<String> = sqlx::query_scalar("SELECT label FROM user_time_off")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(label.as_deref(), Some("Bob private"));
}

#[tokio::test]
async fn an_ongoing_absence_can_be_entered() {
    let (app, _, session, _) = setup_test_app().await;
    let page = body_string(
        app.oneshot(get_authed("/dashboard/time-off", &session))
            .await
            .unwrap(),
    )
    .await;
    let start = page
        .split("id=\"to-start-date\"")
        .nth(1)
        .and_then(|rest| rest.split('>').next())
        .unwrap();
    assert!(
        !start.contains("min="),
        "a start in the past is allowed: {start}"
    );
}

/// If the time off cannot be read, the user counts as busy: a booking,
/// reschedule or approval is refused rather than made during time off.
#[tokio::test]
async fn an_unreadable_time_off_table_fails_closed() {
    let pool = setup_test_db().await;
    let (user, _, et_id) = seed_test_data(&pool).await;
    sqlx::query("ALTER TABLE user_time_off RENAME TO user_time_off_gone")
        .execute(&pool)
        .await
        .unwrap();
    let day = monday();
    let slot = day.and_hms_opt(10, 0, 0).unwrap();
    let busy = fetch_busy_times_for_user(
        &pool,
        &user,
        day.and_time(NaiveTime::MIN),
        day.and_hms_opt(23, 59, 59).unwrap(),
        chrono_tz::Tz::UTC,
        Some(&et_id),
    )
    .await;
    assert!(has_conflict(&busy, slot, slot + Duration::minutes(30)));
}

#[tokio::test]
async fn adjacent_periods_keep_their_own_notes_in_troubleshoot() {
    let (app, pool, session, _) = setup_test_app().await;
    let me = test_user_id(&pool).await;
    let day = monday();
    let at = |h| day.and_hms_opt(h, 0, 0).unwrap();
    add_off(&pool, &me, at(10), at(11), Some("Dentist")).await;
    add_off(&pool, &me, at(11), at(12), Some("Lunch")).await;
    let body = body_string(
        app.oneshot(get_authed(
            &format!("/dashboard/troubleshoot?date={day}&event_type=test-meeting"),
            &session,
        ))
        .await
        .unwrap(),
    )
    .await;
    assert!(
        body.contains("10:00&ndash;11:00: Time off (Dentist)"),
        "{body}"
    );
    assert!(body.contains("11:00&ndash;12:00: Time off (Lunch)"));
}

/// A dynamic group booking (`/u/a+b/slug`) remembers its co-host, and a
/// reschedule or an approval checks their time off too, not only the owner's.
#[tokio::test]
async fn dynamic_group_reschedule_and_approval_check_every_participant() {
    let (app, pool, _, et_id) = setup_test_app().await;
    let second = uuid::Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO users (id, email, name, role, auth_provider, username, enabled) VALUES (?, 'second@example.com', 'Second User', 'user', 'local', 'seconduser', 1)")
        .bind(&second)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO accounts (id, name, email, timezone, user_id) VALUES (?, 'Second User', 'second@example.com', 'UTC', ?)")
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(&second)
        .execute(&pool)
        .await
        .unwrap();
    let csrf = "csrf-dyn-off";
    app.oneshot(post_form_unauthed(
        "/u/testuser+seconduser/test-meeting/book",
        csrf,
        &book_body(csrf),
    ))
    .await
    .unwrap();
    let (booking_id, uid, owner): (String, String, String) = sqlx::query_as(
        "SELECT b.id, b.uid, a.user_id FROM bookings b JOIN event_types et ON et.id = b.event_type_id \
         JOIN accounts a ON a.id = et.account_id WHERE b.event_type_id = ?",
    )
    .bind(&et_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    let hosts = reschedule_hosts(&pool, &uid, &owner).await.unwrap();
    assert_eq!(hosts.participants, vec![second.clone()]);

    // The co-host is busy during their own group booking (10:00 Monday).
    let day = monday();
    let booked = day.and_hms_opt(10, 0, 0).unwrap();
    let co_host_busy = fetch_busy_times_for_user(
        &pool,
        &second,
        day.and_time(NaiveTime::MIN),
        day.and_hms_opt(23, 59, 59).unwrap(),
        chrono_tz::Tz::UTC,
        None,
    )
    .await;
    assert!(has_conflict(
        &co_host_busy,
        booked,
        booked + Duration::minutes(30)
    ));

    // Move it to 14:00 the same day, when the co-host is off.
    let new_start = day.and_hms_opt(14, 0, 0).unwrap();
    let new_end = new_start + Duration::minutes(30);
    let window = (
        day.and_time(NaiveTime::MIN),
        day.and_hms_opt(23, 59, 59).unwrap(),
    );
    let utc = chrono_tz::Tz::UTC;
    let free = |busy: &BusySource| busy_source_is_free(busy, new_start, new_end);
    let busy = reschedule_busy_source(
        &pool,
        &hosts,
        &et_id,
        window.0,
        window.1,
        utc,
        &booking_id,
        &uid,
        true,
    )
    .await
    .unwrap();
    assert!(free(&busy), "both free before the time off");
    add_off(&pool, &second, new_start, new_end, None).await;
    let busy = reschedule_busy_source(
        &pool,
        &hosts,
        &et_id,
        window.0,
        window.1,
        utc,
        &booking_id,
        &uid,
        true,
    )
    .await
    .unwrap();
    assert!(!free(&busy), "the co-host is off at the new time");

    let (start_at, end_at) = (
        format!("{}Z", new_start.format("%Y-%m-%dT%H:%M:%S")),
        format!("{}Z", new_end.format("%Y-%m-%dT%H:%M:%S")),
    );
    assert!(
        !approval_hosts_free(&pool, &hosts, &booking_id, &uid, &et_id, &start_at, &end_at)
            .await
            .unwrap()
    );
}
