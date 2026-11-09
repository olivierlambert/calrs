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

async fn json_of(response: axum::http::Response<Body>) -> serde_json::Value {
    serde_json::from_str(&body_string(response).await).unwrap_or(serde_json::Value::Null)
}

async fn next_free_json(app: &Router, uri: &str) -> (u16, serde_json::Value) {
    let response = app.clone().oneshot(get(uri)).await.unwrap();
    let status = response.status().as_u16();
    (status, json_of(response).await)
}

/// The slugs a page's answer covers.
fn slugs(json: &serde_json::Value) -> Vec<String> {
    json.as_object()
        .map(|o| o.keys().cloned().collect())
        .unwrap_or_default()
}

/// A personal public event type of the test user, Mon-Fri 09-17.
async fn add_personal_type(pool: &SqlitePool, slug: &str) -> String {
    let user_id: String = sqlx::query_scalar("SELECT id FROM users WHERE username = 'testuser'")
        .fetch_one(pool)
        .await
        .unwrap();
    let et_id = insert_personal_et(pool, &user_id, slug).await;
    for day in [1, 2, 3, 4, 5] {
        sqlx::query("INSERT INTO availability_rules (id, event_type_id, day_of_week, start_time, end_time) VALUES (?, ?, ?, '09:00', '17:00')")
            .bind(uuid::Uuid::new_v4().to_string())
            .bind(&et_id)
            .bind(day)
            .execute(pool)
            .await
            .unwrap();
    }
    et_id
}

#[tokio::test]
async fn the_page_answer_holds_each_listed_type_first_start_and_free_days() {
    let (app, _, _, _) = setup_test_app().await;
    let response = app
        .clone()
        .oneshot(get("/u/testuser/next-free"))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    assert_eq!(response.headers()["content-type"], "application/json");
    assert_eq!(response.headers()["cache-control"], "private, max-age=60");
    assert_eq!(response.headers()["vary"], "Cookie");
    let json = json_of(response).await;
    assert_eq!(slugs(&json), ["test-meeting"]);
    let one = &json["test-meeting"];
    let start = one["start"]
        .as_str()
        .expect("a start, Mon-Fri 09-17 is open");
    let start = chrono::DateTime::parse_from_rfc3339(start).unwrap();
    assert!(start > Utc::now(), "the hint is never in the past");
    assert!(start < Utc::now() + Duration::days(8));
    // The open days of the coming week, in order: every weekday from
    // tomorrow on, and today too while a slot is left.
    let today = Utc::now().date_naive();
    let days: Vec<NaiveDate> = one["days"]
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
    let (status, json) = next_free_json(&app, "/u/testuser/next-free?tz=Pacific/Kiritimati").await;
    assert_eq!(status, 200);
    let tz: Tz = "Pacific/Kiritimati".parse().unwrap();
    let today = Utc::now().with_timezone(&tz).date_naive();
    for day in json["test-meeting"]["days"].as_array().unwrap() {
        let day = NaiveDate::parse_from_str(day.as_str().unwrap(), "%Y-%m-%d").unwrap();
        assert!(day >= today && day < today + Duration::days(7), "{day}");
    }
    // An unknown zone falls back to the host's, it is not an error.
    let (status, json) = next_free_json(&app, "/u/testuser/next-free?tz=Mars/Base").await;
    assert_eq!(status, 200);
    assert!(json["test-meeting"]["start"].is_string());
}

/// The answer covers exactly what /u/{username} lists: public, personal,
/// enabled types, not past their last day, of an enabled user.
#[tokio::test]
async fn the_user_answer_covers_exactly_the_listed_types() {
    let (app, pool, _, _) = setup_test_app().await;
    let internal = add_personal_type(&pool, "internal-one").await;
    let private = add_personal_type(&pool, "private-one").await;
    let disabled = add_personal_type(&pool, "disabled-one").await;
    let expired = add_personal_type(&pool, "expired-one").await;
    for (id, sql) in [
        (
            &internal,
            "UPDATE event_types SET visibility = 'internal' WHERE id = ?",
        ),
        (
            &private,
            "UPDATE event_types SET visibility = 'private' WHERE id = ?",
        ),
        (&disabled, "UPDATE event_types SET enabled = 0 WHERE id = ?"),
        (
            &expired,
            "UPDATE event_types SET bookable_until = '2000-01-01' WHERE id = ?",
        ),
    ] {
        sqlx::query(sql).bind(id).execute(&pool).await.unwrap();
    }
    seed_team_event_type(&pool).await;

    let (status, json) = next_free_json(&app, "/u/testuser/next-free").await;
    assert_eq!(status, 200);
    assert_eq!(slugs(&json), ["test-meeting"], "{json}");

    // The same list the page renders.
    let page = body_string(app.clone().oneshot(get("/u/testuser")).await.unwrap()).await;
    let listed: Vec<&str> = page
        .split(r#"data-slug=""#)
        .skip(1)
        .map(|s| s.split('"').next().unwrap())
        .collect();
    assert_eq!(listed, ["test-meeting"]);

    let response = app
        .clone()
        .oneshot(get("/u/nobody/next-free"))
        .await
        .unwrap();
    assert_eq!(response.status(), 404);
    sqlx::query("UPDATE users SET enabled = 0 WHERE username = 'testuser'")
        .execute(&pool)
        .await
        .unwrap();
    let response = app
        .clone()
        .oneshot(get("/u/testuser/next-free"))
        .await
        .unwrap();
    assert_eq!(response.status(), 404);
}

#[tokio::test]
async fn search_stops_at_sixty_days_and_respects_the_booking_window() {
    let (app, pool, _, _) = setup_test_app().await;
    // Opening in 70 days: past the search, so nothing free (null).
    let late = add_personal_type(&pool, "opens-late").await;
    let from = (Utc::now() + Duration::days(70))
        .format("%Y-%m-%d")
        .to_string();
    sqlx::query("UPDATE event_types SET bookable_from = ? WHERE id = ?")
        .bind(&from)
        .bind(&late)
        .execute(&pool)
        .await
        .unwrap();
    // Opening in 40 days: found in the last span, no free day this week.
    let later = add_personal_type(&pool, "opens-in-40").await;
    let from40 = Utc::now().date_naive() + Duration::days(40);
    sqlx::query("UPDATE event_types SET bookable_from = ? WHERE id = ?")
        .bind(from40.format("%Y-%m-%d").to_string())
        .bind(&later)
        .execute(&pool)
        .await
        .unwrap();
    // A two-day horizon: the hint never looks past it.
    let short = add_personal_type(&pool, "short-horizon").await;
    set_horizon(&pool, &short, 2).await;

    let (_, json) = next_free_json(&app, "/u/testuser/next-free").await;
    assert!(json["opens-late"]["start"].is_null(), "{json}");
    assert_eq!(json["opens-late"]["days"], serde_json::json!([]));

    let start = json["opens-in-40"]["start"].as_str().expect("found");
    let start = chrono::DateTime::parse_from_rfc3339(start).unwrap();
    assert!(start.date_naive() >= from40, "{start}");
    assert_eq!(json["opens-in-40"]["days"], serde_json::json!([]));

    if let Some(start) = json["short-horizon"]["start"].as_str() {
        let start = chrono::DateTime::parse_from_rfc3339(start).unwrap();
        assert!(start < Utc::now() + Duration::days(3), "{start}");
    }
    assert!(json["short-horizon"]["days"].as_array().unwrap().len() <= 3);
}

#[tokio::test]
async fn the_team_answer_follows_the_team_page_rules() {
    let (app, pool, session, _) = setup_test_app().await;
    let (team_id, et_id) = seed_team_event_type(&pool).await;
    let uri = "/team/horizon-team/next-free";

    let response = app.clone().oneshot(get(uri)).await.unwrap();
    assert_eq!(response.status(), 200);
    assert_eq!(response.headers()["cache-control"], "private, max-age=60");
    assert_eq!(response.headers()["vary"], "Cookie");
    let json = json_of(response).await;
    assert_eq!(slugs(&json), ["team-sync"]);
    assert!(json["team-sync"]["start"].is_string(), "{json}");

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
    let with_invite = format!("{uri}?invite=tok-123");
    let (status, json) = next_free_json(&app, &with_invite).await;
    assert_eq!(status, 200);
    assert_eq!(slugs(&json), ["team-sync"]);

    // An internal type: left out for visitors, even with the team invite,
    // and listed for a member, as the team page lists it. A signed-in
    // answer is never stored.
    sqlx::query("UPDATE event_types SET visibility = 'internal' WHERE id = ?")
        .bind(&et_id)
        .execute(&pool)
        .await
        .unwrap();
    let (_, json) = next_free_json(&app, &with_invite).await;
    assert!(slugs(&json).is_empty(), "{json}");
    let response = app
        .clone()
        .oneshot(get_authed(uri, &session))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    assert_eq!(response.headers()["cache-control"], "private, no-store");
    assert_eq!(slugs(&json_of(response).await), ["team-sync"]);

    // A signed-in non-member is a visitor: the private team is not found.
    let (_, outsider) =
        insert_test_user_with_session(&pool, "Out Sider", "out@example.com", "outsider").await;
    let response = app
        .clone()
        .oneshot(get_authed(uri, &outsider))
        .await
        .unwrap();
    assert_eq!(response.status(), 404);

    // A disabled type is never listed.
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
    assert!(slugs(&json_of(response).await).is_empty());
    assert_eq!(
        app.clone()
            .oneshot(get("/team/no-such-team/next-free"))
            .await
            .unwrap()
            .status(),
        404
    );
}

/// One request per page view, so the per-address budget is page views.
#[tokio::test]
async fn next_free_is_rate_limited_per_address() {
    let (app, _, _, _) = setup_test_app().await;
    let uri = "/u/testuser/next-free";
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
    assert_eq!(body_string(response).await, "{}");
    // Another address keeps its own budget.
    let response = app
        .clone()
        .oneshot(get_from(uri, "203.0.113.8"))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
}

#[tokio::test]
async fn landing_lists_point_at_their_page_endpoint() {
    let (app, pool, _, _) = setup_test_app().await;
    seed_team_event_type(&pool).await;
    let profile = body_string(app.clone().oneshot(get("/u/testuser")).await.unwrap()).await;
    assert!(profile.contains(r#"data-next="/u/testuser/next-free""#));
    assert!(profile.contains(r#"data-slug="test-meeting""#));
    let team = body_string(
        app.clone()
            .oneshot(get("/team/horizon-team"))
            .await
            .unwrap(),
    )
    .await;
    assert!(team.contains(r#"data-next="/team/horizon-team/next-free""#));
    assert!(team.contains(r#"data-slug="team-sync""#));
}

#[test]
fn first_names_skip_abbreviated_titles() {
    assert_eq!(first_name("Jane Doe"), "Jane");
    assert_eq!(first_name("Dr. Jane Doe"), "Jane");
    assert_eq!(first_name("Prof. Dr. Hans Meier"), "Hans");
    assert_eq!(first_name("  Ana  "), "Ana");
    assert_eq!(first_name("St."), "St.");
}

/// The landing pages' hint endpoint owns the "next-free" segment, so no
/// event type may take that slug.
#[tokio::test]
async fn the_next_free_slug_is_reserved() {
    let pool = setup_test_db().await;
    assert_eq!(
        slug_conflict(&pool, "next-free", "acc", None, None).await,
        Some("form-error-event-type-slug-taken")
    );
    assert_eq!(
        slug_conflict(&pool, "next-free-call", "acc", None, None).await,
        None
    );
}

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
    // A screen reader hears the names, not the initials.
    assert!(
        with.contains(r#"<span class="lp-sr">Test User</span>"#),
        "{with}"
    );
    assert!(with.contains(r#"<span class="lp-with-faces" aria-hidden="true">"#));
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

/// A meeting's length is drawn as clock dials, one per hour (at most three),
/// the last one filled in proportion, with the exact length written under
/// them and spoken in words.
#[test]
fn landing_dials_draw_one_dial_per_hour() {
    let mut env = minijinja::Environment::new();
    env.set_undefined_behavior(minijinja::UndefinedBehavior::Lenient);
    env.set_loader(minijinja::path_loader("templates"));
    crate::i18n::register(&mut env);
    // The landing header includes the public logo partial; no logo here.
    register_company_logo(
        &mut env,
        std::env::temp_dir().join(format!("calrs_no_logo_{}", uuid::Uuid::new_v4())),
    );
    let render = |minutes: i32| {
        let html = env
            .get_template("profile.html")
            .unwrap()
            .render(context! {
                host_name => "Alice",
                username => "alice",
                event_types => vec![context! { slug => "m", title => "M", duration_min => minutes }],
                lang => "en",
            })
            .unwrap();
        let dur = html
            .split(r#"<span class="lp-dur""#)
            .nth(1)
            .unwrap()
            .to_string();
        let dur = dur
            .split(r#"<div class="lp-main">"#)
            .next()
            .unwrap()
            .to_string();
        let dashes: Vec<String> = dur
            .split(r#"stroke-dasharray=""#)
            .skip(1)
            .map(|s| s.split(' ').next().unwrap().to_string())
            .collect();
        (dur, dashes)
    };

    let (dur, dashes) = render(15);
    assert!(dur.contains("lp-dials-1") && dur.contains(r#"aria-label="15 minutes""#));
    assert_eq!(dashes, ["25.0"]);
    assert!(dur.contains(">15 min</span>"));

    let (dur, dashes) = render(60);
    assert!(dur.contains(r#"aria-label="1 hour""#) && dur.contains(">1 h</span>"));
    assert_eq!(dashes, ["100"]);

    let (dur, dashes) = render(90);
    assert!(dur.contains("lp-dials-2") && dur.contains(r#"aria-label="1 hour 30 minutes""#));
    assert_eq!(dashes, ["100", "50.0"]);
    assert!(dur.contains(">1 h 30</span>"));

    let (_, dashes) = render(135);
    assert_eq!(dashes, ["100", "100", "25.0"]);

    // From three hours on, three full dials and the label says the rest.
    let (dur, dashes) = render(240);
    assert!(dur.contains("lp-dials-3") && dur.contains(">4 h</span>"));
    assert_eq!(dashes, ["100", "100", "100"]);
    let (dur, dashes) = render(195);
    assert!(dur.contains(r#"aria-label="3 hours 15 minutes""#));
    assert_eq!(dashes.len(), 3);
}
