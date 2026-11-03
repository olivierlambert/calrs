//! First-day preferences must rotate display, never the stored availability.
use super::*;

fn data(html: &str) -> serde_json::Value {
    let json = html
        .split("id=\"calendar-data\">")
        .nth(1)
        .unwrap()
        .split("</script>")
        .next()
        .unwrap();
    serde_json::from_str(json).unwrap()
}

async fn event_value(pool: &SqlitePool, et_id: &str) -> Option<String> {
    sqlx::query_scalar("SELECT week_start FROM event_types WHERE id = ?")
        .bind(et_id)
        .fetch_one(pool)
        .await
        .unwrap()
}

fn event_body(slug: &str, extra: &str) -> String {
    format!("_csrf=week-test&title=Meeting&slug={slug}&duration_min=30&location_type=link&location_value=https%3A%2F%2Fexample.com&avail_schedule=0:10:00-11:00;1:09:00-17:00&{extra}")
}

#[test]
fn month_windows_cover_the_configured_week_at_both_edges() {
    let start = NaiveDate::from_ymd_opt(2026, 10, 1).unwrap();
    let end = NaiveDate::from_ymd_opt(2026, 10, 31).unwrap();
    let today = NaiveDate::from_ymd_opt(2026, 9, 1).unwrap();
    // Includes one timezone buffer day before and after the calendar grid.
    for (first, from, through) in [
        (WeekStart::Mon, "2026-09-27", "2026-11-02"),
        (WeekStart::Sun, "2026-09-26", "2026-11-01"),
        (WeekStart::Sat, "2026-09-25", "2026-11-07"),
    ] {
        let (offset, days) = month_slot_window(start, end, today, first);
        assert_eq!((today + Duration::days(offset.into())).to_string(), from);
        assert_eq!(
            (today + Duration::days((offset + days - 1).into())).to_string(),
            through
        );
    }
}

#[tokio::test]
async fn week_start_inherits_event_then_owner_then_site_without_changing_hours() {
    let (app, pool, session, et_id) = setup_test_app().await;
    let owner: String = sqlx::query_scalar("SELECT user_id FROM accounts LIMIT 1")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        week_start::event(&pool, &et_id).await.effective(),
        WeekStart::Mon
    );

    let response = app
        .clone()
        .oneshot(post_form(
            "/dashboard/admin/general",
            &session,
            "week-test",
            "_csrf=week-test&week_start=sun",
        ))
        .await
        .unwrap();
    assert!(response.status().is_redirection());
    assert_eq!(
        week_start::event(&pool, &et_id).await.effective(),
        WeekStart::Sun
    );
    let html = body_string(
        app.clone()
            .oneshot(get_authed("/dashboard/settings", &session))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(selected_options(&html, "week_start"), vec![""]);
    assert!(html.contains("Use site default (Sunday)"));

    for (extra, expected) in [
        ("&week_start=sat", WeekStart::Sat),
        ("", WeekStart::Sat),
        ("&week_start=", WeekStart::Sun),
    ] {
        let response = app
            .clone()
            .oneshot(post_form(
                "/dashboard/settings",
                &session,
                "week-test",
                &format!("_csrf=week-test&name=Test+User{extra}"),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        assert_eq!(week_start::user(&pool, &owner).await.effective(), expected);
        assert_eq!(week_start::event(&pool, &et_id).await.effective(), expected);
    }

    for (extra, expected) in [
        ("week_start=tue", Some("tue")),
        ("", Some("tue")),
        ("week_start=", None),
    ] {
        let response = app
            .clone()
            .oneshot(post_form(
                "/dashboard/event-types/test-meeting/edit",
                &session,
                "week-test",
                &event_body("test-meeting", extra),
            ))
            .await
            .unwrap();
        assert!(
            response.status().is_redirection(),
            "{}",
            body_string(response).await
        );
        assert_eq!(event_value(&pool, &et_id).await.as_deref(), expected);
        let rules: Vec<(i32, String)> = sqlx::query_as("SELECT day_of_week, start_time FROM availability_rules WHERE event_type_id = ? ORDER BY day_of_week")
            .bind(&et_id).fetch_all(&pool).await.unwrap();
        assert_eq!(rules, vec![(0, "10:00".into()), (1, "09:00".into())]);
    }

    // Invalid preferences cannot partially save other settings.
    for (uri, body) in [
        (
            "/dashboard/admin/general",
            "_csrf=week-test&week_start=0".to_string(),
        ),
        (
            "/dashboard/settings",
            "_csrf=week-test&name=Changed&week_start=Monday".to_string(),
        ),
        (
            "/dashboard/event-types/test-meeting/edit",
            event_body("changed", "week_start=7"),
        ),
    ] {
        let response = app
            .clone()
            .oneshot(post_form(uri, &session, "week-test", &body))
            .await
            .unwrap();
        assert_eq!(response.status(), 422, "{uri}");
    }
    assert_eq!(week_start::site(&pool).await, WeekStart::Sun);
    let name: String = sqlx::query_scalar("SELECT name FROM users WHERE id = ?")
        .bind(owner)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(name, "Test User");
    let slug: String = sqlx::query_scalar("SELECT slug FROM event_types WHERE id = ?")
        .bind(&et_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(slug, "test-meeting");
}

#[tokio::test]
async fn personal_and_team_forms_create_edit_reset_and_copy_the_preference() {
    let (app, pool, session, _) = setup_test_app().await;
    let (team_id, team_et) = seed_team_event_type(&pool).await;
    for (uri, slug) in [
        ("/dashboard/event-types/new".to_string(), "personal-week"),
        ("/dashboard/group-event-types/new".to_string(), "team-week"),
    ] {
        let extra = if slug == "team-week" {
            format!("week_start=thu&team_id={team_id}")
        } else {
            "week_start=thu".to_string()
        };
        let body = event_body(slug, &extra);
        let response = app
            .clone()
            .oneshot(post_form(&uri, &session, "week-test", &body))
            .await
            .unwrap();
        assert!(
            response.status().is_redirection(),
            "{}",
            body_string(response).await
        );
        let value: String = sqlx::query_scalar("SELECT week_start FROM event_types WHERE slug = ?")
            .bind(slug)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(value, "thu");
    }
    let edit = format!("/dashboard/group-event-types/{team_id}/team-sync/edit");
    for (extra, expected) in [
        ("week_start=fri", Some("fri")),
        ("", Some("fri")),
        ("week_start=", None),
    ] {
        let response = app
            .clone()
            .oneshot(post_form(
                &edit,
                &session,
                "week-test",
                &event_body("team-sync", extra),
            ))
            .await
            .unwrap();
        assert!(response.status().is_redirection());
        assert_eq!(event_value(&pool, &team_et).await.as_deref(), expected);
        let html = body_string(
            app.clone()
                .oneshot(get_authed(&edit, &session))
                .await
                .unwrap(),
        )
        .await;
        assert_eq!(
            selected_options(&html, "week_start"),
            vec![expected.unwrap_or("")]
        );
    }

    // A validation error keeps a submitted override when the form is rebuilt.
    let response = app
        .clone()
        .oneshot(post_form(
            &edit,
            &session,
            "week-test",
            &event_body(
                "team-sync",
                "week_start=wed&bookable_from=2030-12-31&bookable_until=2030-01-01",
            ),
        ))
        .await
        .unwrap();
    assert_eq!(
        selected_options(&body_string(response).await, "week_start"),
        vec!["wed"]
    );
    assert_eq!(event_value(&pool, &team_et).await, None);

    let owner: crate::models::User =
        sqlx::query_as("SELECT * FROM users WHERE username = 'testuser'")
            .fetch_one(&pool)
            .await
            .unwrap();
    let et: String = sqlx::query_scalar("SELECT id FROM event_types WHERE slug = 'personal-week'")
        .fetch_one(&pool)
        .await
        .unwrap();
    let slug = duplicate_event_type_rows(&pool, &et, &owner, "en")
        .await
        .unwrap();
    let value: String = sqlx::query_scalar("SELECT week_start FROM event_types WHERE slug = ?")
        .bind(slug)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(value, "thu");
}

#[tokio::test]
async fn public_calendars_use_the_owner_preference_for_every_view_and_route() {
    let (app, pool, session, et) = setup_test_app().await;
    let (_, team_et) = seed_team_event_type(&pool).await;
    let (_, reschedule, _) = insert_test_booking(
        &pool,
        &et,
        &booking_start_at(Duration::days(10)),
        &booking_end_at(Duration::days(10), 30),
        "confirmed",
    )
    .await;
    // Owner override must survive a different viewer's preference.
    sqlx::query(
        "UPDATE users SET week_start = 'sun', allow_dynamic_group = 1 WHERE username = 'testuser'",
    )
    .execute(&pool)
    .await
    .unwrap();
    let (viewer, viewer_session) =
        insert_test_user_with_session(&pool, "Viewer", "viewer@example.com", "viewer").await;
    sqlx::query("UPDATE users SET week_start = 'fri', allow_dynamic_group = 1 WHERE id = ?")
        .bind(&viewer)
        .execute(&pool)
        .await
        .unwrap();
    for uri in [
        "/u/testuser/test-meeting?month=2026-11&tz=UTC".to_string(),
        "/test-meeting?month=2026-11&tz=UTC".to_string(),
        "/u/testuser/test-meeting?month=2026-11&embed=1&deferred=1&tz=UTC".to_string(),
        "/team/horizon-team/team-sync?month=2026-11&deferred=1&tz=UTC".to_string(),
        "/u/testuser+viewer/test-meeting?month=2026-11&deferred=1&tz=UTC".to_string(),
        format!("/booking/reschedule/{reschedule}?month=2026-11&tz=UTC"),
    ] {
        let html = body_string(
            app.clone()
                .oneshot(get_authed(&uri, &viewer_session))
                .await
                .unwrap(),
        )
        .await;
        assert!(html.contains("id=\"calendar-data\""), "{uri}: {html}");
        let json = data(&html);
        assert_eq!(json["weekStartDay"], 0, "{uri}");
        assert_eq!(
            json["firstWeekday"], 0,
            "November 2026 begins on Sunday: {uri}"
        );
        let labels = html.split("<div class=\"cal-weekdays\">").nth(1).unwrap();
        assert!(labels.find(">Sun<").unwrap() < labels.find(">Mon<").unwrap());
    }
    sqlx::query("UPDATE event_types SET week_start = 'sat' WHERE id = ?")
        .bind(&team_et)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        week_start::event(&pool, &team_et).await.effective(),
        WeekStart::Sat
    );
    let html = body_string(
        app.oneshot(get_authed(
            "/dashboard/event-types/test-meeting/edit",
            &session,
        ))
        .await
        .unwrap(),
    )
    .await;
    assert!(html.contains("Use owner’s default (Sunday)"));
}
