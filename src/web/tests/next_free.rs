//! The landing pages' "next free" hints (`next_free.rs`) and the team
//! page's per-type roster.

use super::*;

/// A GET from `ip` (as the trusted proxy reports it).
fn get_from(uri: &str, ip: &str) -> axum::http::Request<Body> {
    axum::http::Request::builder()
        .uri(uri)
        .header("x-forwarded-for", ip)
        .body(Body::empty())
        .unwrap()
}

async fn next_free_json(app: &Router, uri: &str) -> (u16, serde_json::Value) {
    let response = app.clone().oneshot(get(uri)).await.unwrap();
    let status = response.status().as_u16();
    let body = body_string(response).await;
    (
        status,
        serde_json::from_str(&body).unwrap_or(serde_json::Value::Null),
    )
}

#[tokio::test]
async fn public_type_answers_its_first_free_start_and_free_days() {
    let (app, _, _, _) = setup_test_app().await;
    let response = app
        .clone()
        .oneshot(get("/u/testuser/test-meeting/next-free"))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    assert_eq!(response.headers()["content-type"], "application/json");
    assert_eq!(response.headers()["cache-control"], "private, max-age=60");
    let json: serde_json::Value = serde_json::from_str(&body_string(response).await).unwrap();
    let start = json["start"]
        .as_str()
        .expect("a start, Mon-Fri 09-17 is open");
    let start = chrono::DateTime::parse_from_rfc3339(start).unwrap();
    assert!(start > Utc::now(), "the hint is never in the past");
    assert!(start < Utc::now() + Duration::days(8));
    // The open days of the coming week, in order: every weekday from
    // tomorrow on, and today too while a slot is left.
    let today = Utc::now().date_naive();
    let days: Vec<NaiveDate> = json["days"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| NaiveDate::parse_from_str(d.as_str().unwrap(), "%Y-%m-%d").unwrap())
        .collect();
    let weekdays: Vec<NaiveDate> = (1..7)
        .map(|i| today + Duration::days(i))
        .filter(|d| d.weekday().number_from_monday() <= 5)
        .collect();
    let mut sorted = days.clone();
    sorted.sort();
    assert_eq!(days, sorted);
    for day in &weekdays {
        assert!(days.contains(day), "{day} missing from {days:?}");
    }
    for day in &days {
        assert!(*day == today || weekdays.contains(day), "{day} is not open");
    }
}

#[tokio::test]
async fn days_follow_the_guest_timezone() {
    let (app, _, _, _) = setup_test_app().await;
    // Kiritimati is UTC+14: a Monday 09:00 UTC slot is Monday 23:00 there,
    // so whatever the day, every listed date is a guest-local date in range.
    let (status, json) = next_free_json(
        &app,
        "/u/testuser/test-meeting/next-free?tz=Pacific/Kiritimati",
    )
    .await;
    assert_eq!(status, 200);
    let tz: Tz = "Pacific/Kiritimati".parse().unwrap();
    let today = Utc::now().with_timezone(&tz).date_naive();
    for day in json["days"].as_array().unwrap() {
        let day = NaiveDate::parse_from_str(day.as_str().unwrap(), "%Y-%m-%d").unwrap();
        assert!(day >= today && day < today + Duration::days(7), "{day}");
    }
    // An unknown zone falls back to the host's, it is not an error.
    let (status, _) = next_free_json(&app, "/u/testuser/test-meeting/next-free?tz=Mars/Base").await;
    assert_eq!(status, 200);
}

#[tokio::test]
async fn only_types_the_profile_lists_are_answered() {
    let (app, pool, _, et_id) = setup_test_app().await;
    let not_found = |uri: &'static str| {
        let app = app.clone();
        async move {
            let response = app.oneshot(get(uri)).await.unwrap();
            assert_eq!(response.status(), 404, "{uri}");
        }
    };
    not_found("/u/testuser/nope/next-free").await;
    not_found("/u/nobody/test-meeting/next-free").await;

    for visibility in ["internal", "private"] {
        sqlx::query("UPDATE event_types SET visibility = ? WHERE id = ?")
            .bind(visibility)
            .bind(&et_id)
            .execute(&pool)
            .await
            .unwrap();
        // Not even with an event-type invite: the landing page never lists it.
        not_found("/u/testuser/test-meeting/next-free").await;
    }
    sqlx::query("UPDATE event_types SET visibility = 'public', enabled = 0 WHERE id = ?")
        .bind(&et_id)
        .execute(&pool)
        .await
        .unwrap();
    not_found("/u/testuser/test-meeting/next-free").await;

    sqlx::query("UPDATE event_types SET enabled = 1 WHERE id = ?")
        .bind(&et_id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE users SET enabled = 0 WHERE username = 'testuser'")
        .execute(&pool)
        .await
        .unwrap();
    not_found("/u/testuser/test-meeting/next-free").await;

    sqlx::query("UPDATE users SET enabled = 1 WHERE username = 'testuser'")
        .execute(&pool)
        .await
        .unwrap();
    // Past its last bookable day, the type leaves the profile and the hint.
    let yesterday = (Utc::now() - Duration::days(2))
        .format("%Y-%m-%d")
        .to_string();
    sqlx::query("UPDATE event_types SET bookable_until = ? WHERE id = ?")
        .bind(&yesterday)
        .bind(&et_id)
        .execute(&pool)
        .await
        .unwrap();
    not_found("/u/testuser/test-meeting/next-free").await;
}

#[tokio::test]
async fn a_team_type_is_not_answered_on_the_user_route() {
    let (app, pool, _, _) = setup_test_app().await;
    seed_team_event_type(&pool).await;
    let response = app
        .oneshot(get("/u/testuser/team-sync/next-free"))
        .await
        .unwrap();
    assert_eq!(response.status(), 404);
}

#[tokio::test]
async fn search_stops_at_sixty_days_and_respects_the_booking_window() {
    let (app, pool, _, et_id) = setup_test_app().await;
    // Opening in 70 days: past the search, so nothing free (null, not 404).
    let from = (Utc::now() + Duration::days(70))
        .format("%Y-%m-%d")
        .to_string();
    sqlx::query("UPDATE event_types SET bookable_from = ? WHERE id = ?")
        .bind(&from)
        .bind(&et_id)
        .execute(&pool)
        .await
        .unwrap();
    let (status, json) = next_free_json(&app, "/u/testuser/test-meeting/next-free").await;
    assert_eq!(status, 200);
    assert!(json["start"].is_null(), "{json}");
    assert_eq!(json["days"], serde_json::json!([]));

    // Opening in 40 days: found in the last span, still no free day this week.
    let from = Utc::now().date_naive() + Duration::days(40);
    sqlx::query("UPDATE event_types SET bookable_from = ? WHERE id = ?")
        .bind(from.format("%Y-%m-%d").to_string())
        .bind(&et_id)
        .execute(&pool)
        .await
        .unwrap();
    let (_, json) = next_free_json(&app, "/u/testuser/test-meeting/next-free").await;
    let start = chrono::DateTime::parse_from_rfc3339(json["start"].as_str().unwrap()).unwrap();
    assert!(start.date_naive() >= from, "{start}");
    assert_eq!(json["days"], serde_json::json!([]));

    // A two-day horizon: the hint never looks past it.
    sqlx::query("UPDATE event_types SET bookable_from = NULL WHERE id = ?")
        .bind(&et_id)
        .execute(&pool)
        .await
        .unwrap();
    set_horizon(&pool, &et_id, 2).await;
    let (_, json) = next_free_json(&app, "/u/testuser/test-meeting/next-free").await;
    if let Some(start) = json["start"].as_str() {
        let start = chrono::DateTime::parse_from_rfc3339(start).unwrap();
        assert!(start < Utc::now() + Duration::days(3), "{start}");
    }
    assert!(json["days"].as_array().unwrap().len() <= 3);
}

#[tokio::test]
async fn team_types_follow_the_team_page_rules() {
    let (app, pool, session, _) = setup_test_app().await;
    let (team_id, et_id) = seed_team_event_type(&pool).await;
    let uri = "/team/horizon-team/team-sync/next-free";

    let (status, json) = next_free_json(&app, uri).await;
    assert_eq!(status, 200);
    assert!(json["start"].is_string(), "{json}");

    // A private team: only with its invite token.
    sqlx::query("UPDATE teams SET visibility = 'private', invite_token = 'tok-123' WHERE id = ?")
        .bind(&team_id)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(app.clone().oneshot(get(uri)).await.unwrap().status(), 404);
    let wrong = format!("{uri}?invite=tok-999");
    assert_eq!(
        app.clone().oneshot(get(&wrong)).await.unwrap().status(),
        404
    );
    let (status, _) = next_free_json(&app, &format!("{uri}?invite=tok-123")).await;
    assert_eq!(status, 200);

    // An internal type: hidden from visitors, even with the team invite,
    // and shown to a member, as the team page shows it.
    sqlx::query("UPDATE event_types SET visibility = 'internal' WHERE id = ?")
        .bind(&et_id)
        .execute(&pool)
        .await
        .unwrap();
    let with_invite = format!("{uri}?invite=tok-123");
    assert_eq!(
        app.clone()
            .oneshot(get(&with_invite))
            .await
            .unwrap()
            .status(),
        404
    );
    let response = app
        .clone()
        .oneshot(get_authed(uri, &session))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);

    // A signed-in non-member is a visitor.
    let (_, outsider) =
        insert_test_user_with_session(&pool, "Out Sider", "out@example.com", "outsider").await;
    let response = app
        .clone()
        .oneshot(get_authed(uri, &outsider))
        .await
        .unwrap();
    assert_eq!(response.status(), 404);

    sqlx::query("UPDATE event_types SET visibility = 'public', enabled = 0 WHERE id = ?")
        .bind(&et_id)
        .execute(&pool)
        .await
        .unwrap();
    let response = app
        .clone()
        .oneshot(get_authed(uri, &session))
        .await
        .unwrap();
    assert_eq!(response.status(), 404);
}

#[tokio::test]
async fn next_free_is_rate_limited_per_address() {
    let (app, _, _, _) = setup_test_app().await;
    let uri = "/u/testuser/test-meeting/next-free";
    for _ in 0..60 {
        let response = app
            .clone()
            .oneshot(get_from(uri, "203.0.113.7"))
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
    }
    let response = app
        .clone()
        .oneshot(get_from(uri, "203.0.113.7"))
        .await
        .unwrap();
    assert_eq!(response.status(), 429);
    assert_eq!(body_string(response).await, r#"{"start":null,"days":[]}"#);
    // Another address keeps its own budget.
    let response = app
        .clone()
        .oneshot(get_from(uri, "203.0.113.8"))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
}

#[tokio::test]
async fn landing_rows_point_at_their_next_free_endpoint() {
    let (app, pool, _, _) = setup_test_app().await;
    seed_team_event_type(&pool).await;
    let profile = body_string(app.clone().oneshot(get("/u/testuser")).await.unwrap()).await;
    assert!(profile.contains(r#"data-next="/u/testuser/test-meeting/next-free""#));
    let team = body_string(
        app.clone()
            .oneshot(get("/team/horizon-team"))
            .await
            .unwrap(),
    )
    .await;
    assert!(team.contains(r#"data-next="/team/horizon-team/team-sync/next-free""#));
}

/// The team page names who a guest meets: "with one of" for round robin,
/// "with everyone" for collective, over the type's own roster (a member
/// excluded by a zero weight is left out), and first names under the faces.
#[tokio::test]
async fn team_page_shows_each_type_roster_and_first_names() {
    let (app, pool, _, _) = setup_test_app().await;
    let (team_id, et_id) = seed_team_event_type(&pool).await;
    let (other_id, _) =
        insert_test_user_with_session(&pool, "Zoe Quartz", "zoe@example.com", "zoe").await;
    sqlx::query("INSERT INTO team_members (team_id, user_id, role, source) VALUES (?, ?, 'member', 'direct')")
        .bind(&team_id)
        .bind(&other_id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO event_type_member_weights (event_type_id, user_id, weight) VALUES (?, ?, 0)",
    )
    .bind(&et_id)
    .bind(&other_id)
    .execute(&pool)
    .await
    .unwrap();

    let page = body_string(
        app.clone()
            .oneshot(get("/team/horizon-team"))
            .await
            .unwrap(),
    )
    .await;
    assert!(
        page.contains(r#"<span class="lp-member-name">Zoe</span>"#),
        "first name under the face"
    );
    let with = page
        .split(r#"<div class="lp-with">"#)
        .nth(1)
        .expect("a roster line");
    let with = with.split("</div>").next().unwrap();
    assert!(with.contains("with one of"), "{with}");
    assert!(with.contains(r#"data-face="Test User""#), "{with}");
    assert!(
        !with.contains("Zoe"),
        "an excluded member is not in the roster: {with}"
    );

    sqlx::query("UPDATE event_types SET scheduling_mode = 'collective' WHERE id = ?")
        .bind(&et_id)
        .execute(&pool)
        .await
        .unwrap();
    let page = body_string(app.oneshot(get("/team/horizon-team")).await.unwrap()).await;
    assert!(page.contains("with everyone"));
}
