//! Directory discovery, per-user favorites, live visibility and migration cleanup.

use super::*;

async fn favorite(app: &Router, session: &str, id: &str, value: bool) -> Response {
    app.clone()
        .oneshot(post_form(
            &format!("/dashboard/directory/{id}/favorite"),
            session,
            "directory-csrf",
            &format!("_csrf=directory-csrf&favorite={value}"),
        ))
        .await
        .unwrap()
}

fn has_entry(body: &str, id: &str) -> bool {
    body.contains(&format!("data-event-id=\"{id}\""))
}

#[tokio::test]
async fn directory_searches_person_team_and_meeting_for_an_ordinary_colleague() {
    let (app, pool, _, personal) = setup_test_app().await;
    let (_, viewer) = insert_test_user_with_session(&pool, "Viewer", "viewer@x", "viewer").await;
    let (team, team_et) = insert_team_with_et(&pool, "sales-fr", "demo", &[]).await;
    sqlx::query("UPDATE event_types SET title = 'Démo FR' WHERE id = ?")
        .bind(&team_et)
        .execute(&pool)
        .await
        .unwrap();
    for (query, expected, absent) in [
        ("testuser", &personal, &team_et),
        ("test%20meeting", &personal, &team_et),
        ("sales-fr", &team_et, &personal),
        ("D%C3%89MO%20fr", &team_et, &personal),
    ] {
        let response = app
            .clone()
            .oneshot(get_authed(
                &format!("/dashboard/directory?q={query}"),
                &viewer,
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        let body = body_string(response).await;
        assert!(has_entry(&body, expected), "query {query}");
        assert!(!has_entry(&body, absent), "query {query}");
    }
    // A team meeting does not require its placeholder account to have a user.
    assert_eq!(favorite(&app, &viewer, &team_et, true).await.status(), 200);
    sqlx::query("UPDATE teams SET visibility = 'private' WHERE id = ?")
        .bind(&team)
        .execute(&pool)
        .await
        .unwrap();
    let body = body_string(
        app.clone()
            .oneshot(get_authed("/dashboard/directory", &viewer))
            .await
            .unwrap(),
    )
    .await;
    assert!(!has_entry(&body, &team_et));
    assert_eq!(favorite(&app, &viewer, &team_et, true).await.status(), 404);
    // Internal meetings explicitly allow any colleague to generate an invite.
    sqlx::query("UPDATE event_types SET visibility = 'internal' WHERE id = ?")
        .bind(&team_et)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(favorite(&app, &viewer, &team_et, true).await.status(), 200);
}

#[tokio::test]
async fn favorites_are_personal_idempotent_and_track_live_titles_and_urls() {
    let (app, pool, owner_session, et_id) = setup_test_app().await;
    let (_, viewer) = insert_test_user_with_session(&pool, "Viewer", "viewer@x", "viewer").await;
    for _ in 0..2 {
        assert_eq!(favorite(&app, &viewer, &et_id, true).await.status(), 200);
    }
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM event_type_favorites")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 1);
    assert_eq!(
        favorite(&app, &owner_session, &et_id, false).await.status(),
        200
    );
    for uri in ["/dashboard", "/dashboard/directory?favorites=true"] {
        let body = body_string(app.clone().oneshot(get_authed(uri, &viewer)).await.unwrap()).await;
        assert!(has_entry(&body, &et_id));
        let body = body_string(
            app.clone()
                .oneshot(get_authed(uri, &owner_session))
                .await
                .unwrap(),
        )
        .await;
        assert!(!has_entry(&body, &et_id));
    }
    // The dashboard card only appears once something is starred.
    let dashboard = |session: String| {
        let app = app.clone();
        async move {
            body_string(
                app.oneshot(get_authed("/dashboard", &session))
                    .await
                    .unwrap(),
            )
            .await
        }
    };
    assert!(dashboard(viewer.clone())
        .await
        .contains("Favorite meetings"));
    assert!(!dashboard(owner_session.clone())
        .await
        .contains("Favorite meetings"));
    // Hand-written flags are accepted rather than rejected with a 400.
    for (flag, favorites_only) in [("1", true), ("on", true), ("0", false), ("", false)] {
        let response = app
            .clone()
            .oneshot(get_authed(
                &format!("/dashboard/directory?favorites={flag}"),
                &owner_session,
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), 200, "favorites={flag}");
        let body = body_string(response).await;
        assert_eq!(
            !has_entry(&body, &et_id),
            favorites_only,
            "favorites={flag}"
        );
    }
    sqlx::query("UPDATE event_types SET title = 'TAM revised', slug = 'new-tam' WHERE id = ?")
        .bind(&et_id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE users SET username = 'new-host' WHERE username = 'testuser'")
        .execute(&pool)
        .await
        .unwrap();
    let body = body_string(
        app.clone()
            .oneshot(get_authed("/dashboard", &viewer))
            .await
            .unwrap(),
    )
    .await;
    assert!(body.contains("TAM revised"));
    assert!(body.replace("&#x2f;", "/").contains("/u/new-host/new-tam"));
    for _ in 0..2 {
        assert_eq!(favorite(&app, &viewer, &et_id, false).await.status(), 200);
    }
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM event_type_favorites")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
}

#[tokio::test]
async fn favorites_recheck_visibility_enabled_owner_and_expiry() {
    let (app, pool, _, et_id) = setup_test_app().await;
    let (_, viewer) = insert_test_user_with_session(&pool, "Viewer", "viewer@x", "viewer").await;
    assert_eq!(favorite(&app, &viewer, &et_id, true).await.status(), 200);
    for update in [
        "UPDATE event_types SET visibility = 'private'",
        "UPDATE event_types SET visibility = 'public', enabled = 0",
        "UPDATE event_types SET enabled = 1, bookable_until = '2000-01-01'",
        "UPDATE event_types SET bookable_until = NULL; UPDATE users SET enabled = 0 WHERE username = 'testuser'",
        "UPDATE users SET enabled = 1, username = NULL WHERE username = 'testuser'",
    ] {
        sqlx::raw_sql(update).execute(&pool).await.unwrap();
        for uri in ["/dashboard", "/dashboard/directory"] {
            let body = body_string(app.clone().oneshot(get_authed(uri, &viewer)).await.unwrap()).await;
            assert!(!has_entry(&body, &et_id), "{update} on {uri}");
        }
        assert_eq!(favorite(&app, &viewer, &et_id, true).await.status(), 404);
    }
    assert_eq!(favorite(&app, &viewer, &et_id, false).await.status(), 200);
    assert_eq!(
        favorite(&app, &viewer, "does-not-exist", true)
            .await
            .status(),
        404
    );
}

#[tokio::test]
async fn directory_requires_auth_and_favorites_require_csrf() {
    let (app, _, session, et_id) = setup_test_app().await;
    let response = app
        .clone()
        .oneshot(get("/dashboard/directory"))
        .await
        .unwrap();
    assert_eq!(response.status(), 303);
    let uri = format!("/dashboard/directory/{et_id}/favorite");
    let response = app
        .clone()
        .oneshot(post_form_unauthed(&uri, "csrf", "_csrf=csrf&favorite=true"))
        .await
        .unwrap();
    assert_eq!(response.status(), 303);
    let response = app
        .oneshot(post_form(
            &uri,
            &session,
            "csrf",
            "_csrf=wrong&favorite=true",
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), 403);
}

#[tokio::test]
async fn directory_escapes_user_content_and_treats_search_as_literal_text() {
    let (app, pool, session, et_id) = setup_test_app().await;
    sqlx::query("UPDATE event_types SET title = '<script>alert(1)</script> %_' WHERE id = ?")
        .bind(&et_id)
        .execute(&pool)
        .await
        .unwrap();
    let response = app
        .clone()
        .oneshot(get_authed("/dashboard/directory?q=%3Cscript%3E", &session))
        .await
        .unwrap();
    let body = body_string(response).await;
    assert!(has_entry(&body, &et_id));
    assert!(!body.contains("<script>alert(1)</script>"));
    let body = body_string(
        app.clone()
            .oneshot(get_authed("/dashboard/directory?q=%25_", &session))
            .await
            .unwrap(),
    )
    .await;
    assert!(has_entry(&body, &et_id));
    let body = body_string(
        app.oneshot(get_authed("/dashboard/directory?q=missing", &session))
            .await
            .unwrap(),
    )
    .await;
    assert!(!has_entry(&body, &et_id));
}

#[tokio::test]
async fn favorite_migration_is_additive_repeatable_and_cascades_deletions() {
    let pool = setup_test_db().await;
    let user = insert_role_user(&pool, "favorite@x", "user").await;
    let et = insert_personal_et(&pool, &user, "test").await;
    let viewer = insert_role_user(&pool, "viewer@x", "user").await;
    // Recreate the pre-feature schema with existing meeting data, then upgrade.
    sqlx::raw_sql("DROP TABLE event_type_favorites; DELETE FROM _migrations WHERE name = '078_event_type_favorites'")
        .execute(&pool).await.unwrap();
    let before: String =
        sqlx::query_scalar("SELECT sql FROM sqlite_master WHERE name = 'event_types'")
            .fetch_one(&pool)
            .await
            .unwrap();
    crate::db::migrate(&pool).await.unwrap();
    sqlx::query("INSERT INTO event_type_favorites (user_id, event_type_id) VALUES (?, ?)")
        .bind(&viewer)
        .bind(&et)
        .execute(&pool)
        .await
        .unwrap();
    crate::db::migrate(&pool).await.unwrap();
    let after: String =
        sqlx::query_scalar("SELECT sql FROM sqlite_master WHERE name = 'event_types'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(before, after);
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM event_type_favorites")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 1);
    sqlx::query("DELETE FROM users WHERE id = ?")
        .bind(&viewer)
        .execute(&pool)
        .await
        .unwrap();
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM event_type_favorites")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
    sqlx::query("INSERT INTO event_type_favorites (user_id, event_type_id) VALUES (?, ?)")
        .bind(&user)
        .bind(&et)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM event_types WHERE id = ?")
        .bind(&et)
        .execute(&pool)
        .await
        .unwrap();
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM event_type_favorites")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
}

#[tokio::test]
async fn internal_favorites_generate_fresh_single_use_invites() {
    let (app, pool, owner, et_id) = setup_test_app().await;
    let (_, viewer) = insert_test_user_with_session(&pool, "Viewer", "viewer@x", "viewer").await;
    sqlx::query("UPDATE event_types SET visibility = 'internal' WHERE id = ?")
        .bind(&et_id)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(favorite(&app, &viewer, &et_id, true).await.status(), 200);
    let mut urls = Vec::new();
    for _ in 0..2 {
        let response = app
            .clone()
            .oneshot(post_form(
                &format!("/dashboard/invites/{et_id}/quick-link"),
                &viewer,
                "csrf",
                "_csrf=csrf",
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        let json: serde_json::Value = serde_json::from_str(&body_string(response).await).unwrap();
        urls.push(json["url"].as_str().unwrap().to_string());
    }
    assert_ne!(urls[0], urls[1]);
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM booking_invites WHERE event_type_id = ? AND max_uses = 1 AND used_count = 0 AND expires_at > datetime('now', '+6 days') AND expires_at <= datetime('now', '+7 days')")
        .bind(&et_id).fetch_one(&pool).await.unwrap();
    assert_eq!(count, 2);

    sqlx::query("UPDATE event_types SET enabled = 0 WHERE id = ?")
        .bind(&et_id)
        .execute(&pool)
        .await
        .unwrap();
    let response = app
        .clone()
        .oneshot(post_form(
            &format!("/dashboard/invites/{et_id}/quick-link"),
            &viewer,
            "csrf",
            "_csrf=csrf",
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), 404);
    // The owner can still prepare a link before enabling the meeting.
    let response = app
        .clone()
        .oneshot(post_form(
            &format!("/dashboard/invites/{et_id}/quick-link"),
            &owner,
            "csrf",
            "_csrf=csrf",
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);

    // A storage failure must not return a URL for an invite that was never saved.
    sqlx::query("UPDATE event_types SET enabled = 1 WHERE id = ?")
        .bind(&et_id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::raw_sql("CREATE TRIGGER reject_invite BEFORE INSERT ON booking_invites BEGIN SELECT RAISE(ABORT, 'test failure'); END")
        .execute(&pool).await.unwrap();
    let response = app
        .oneshot(post_form(
            &format!("/dashboard/invites/{et_id}/quick-link"),
            &viewer,
            "csrf",
            "_csrf=csrf",
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), 500);
    assert!(!body_string(response).await.contains("test failure"));
}

#[tokio::test]
async fn team_meetings_match_their_hosts_without_revealing_private_teams() {
    let (app, pool, _, personal) = setup_test_app().await;
    let (_, viewer) = insert_test_user_with_session(&pool, "Viewer", "viewer@x", "viewer").await;
    let (andy, andy_session) =
        insert_test_user_with_session(&pool, "Andy Host", "andy@x", "ahost").await;
    let (bob, _) = insert_test_user_with_session(&pool, "Bob Benched", "bob@x", "bob").await;
    let (carl, _) = insert_test_user_with_session(&pool, "Carl Gone", "carl@x", "carl").await;
    let (team, et) = insert_team_with_et(
        &pool,
        "sales",
        "demo",
        &[(&andy, "member"), (&bob, "member"), (&carl, "member")],
    )
    .await;
    // Bob takes no bookings for this meeting; Carl's account is suspended.
    sqlx::query(
        "INSERT INTO event_type_member_weights (event_type_id, user_id, weight) VALUES (?, ?, 0)",
    )
    .bind(&et)
    .bind(&bob)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("UPDATE users SET enabled = 0 WHERE id = ?")
        .bind(&carl)
        .execute(&pool)
        .await
        .unwrap();
    let search = |session: String, q: &str| {
        let app = app.clone();
        let uri = format!("/dashboard/directory?q={q}");
        async move { body_string(app.oneshot(get_authed(&uri, &session)).await.unwrap()).await }
    };

    for q in ["andy", "AHOST", "andy%20sales"] {
        let body = search(viewer.clone(), q).await;
        assert!(has_entry(&body, &et), "query {q}");
        assert!(!has_entry(&body, &personal), "query {q}");
        assert!(body.contains("directory-hosts"), "query {q}");
        assert!(body.contains("Andy Host"), "query {q}");
    }
    for q in ["bob", "carl", "andy%20nobody"] {
        assert!(
            !has_entry(&search(viewer.clone(), q).await, &et),
            "query {q}"
        );
    }
    // A match on the team name alone does not list the hosts.
    let body = search(viewer.clone(), "sales").await;
    assert!(has_entry(&body, &et));
    assert!(!body.contains("directory-hosts"));

    // Members of a private team are only searchable by fellow members.
    sqlx::query("UPDATE teams SET visibility = 'private' WHERE id = ?")
        .bind(&team)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE event_types SET visibility = 'internal' WHERE id = ?")
        .bind(&et)
        .execute(&pool)
        .await
        .unwrap();
    assert!(has_entry(&search(viewer.clone(), "sales").await, &et));
    let body = search(viewer.clone(), "andy").await;
    assert!(!has_entry(&body, &et));
    assert!(!body.contains("Andy Host"));
    let body = search(andy_session.clone(), "andy").await;
    assert!(has_entry(&body, &et));
    assert!(body.contains("directory-hosts"));
}

fn entry_count(body: &str) -> usize {
    body.matches("class=\"directory-entry\"").count()
}

/// Add `n` public meetings to the owner of `et_id`, titled to sort after
/// "Test Meeting" and before nothing else.
async fn add_meetings(pool: &SqlitePool, et_id: &str, n: usize) -> Vec<String> {
    let account: String = sqlx::query_scalar("SELECT account_id FROM event_types WHERE id = ?")
        .bind(et_id)
        .fetch_one(pool)
        .await
        .unwrap();
    let mut ids = Vec::new();
    for i in 0..n {
        let id = uuid::Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO event_types (id, account_id, slug, title, duration_min, buffer_before, buffer_after, min_notice_min, enabled)
             VALUES (?, ?, ?, ?, 30, 0, 0, 0, 1)",
        )
        .bind(&id)
        .bind(&account)
        .bind(format!("extra-{i:03}"))
        .bind(format!("Extra {i:03}"))
        .execute(pool)
        .await
        .unwrap();
        ids.push(id);
    }
    ids
}

#[tokio::test]
async fn directory_pages_large_catalogues_and_shows_no_pager_when_everything_fits() {
    let (app, pool, _, et_id) = setup_test_app().await;
    let (_, viewer) = insert_test_user_with_session(&pool, "Viewer", "viewer@x", "viewer").await;
    let page = |uri: String| {
        let app = app.clone();
        let viewer = viewer.clone();
        async move { body_string(app.oneshot(get_authed(&uri, &viewer)).await.unwrap()).await }
    };
    // A small directory: one page, no controls, exactly as before.
    let body = page("/dashboard/directory".into()).await;
    assert_eq!(entry_count(&body), 1);
    assert!(!body.contains("class=\"ui-pager\""));

    let extra = add_meetings(&pool, &et_id, 60).await;
    let body = page("/dashboard/directory".into()).await;
    assert_eq!(entry_count(&body), DIRECTORY_PAGE_SIZE as usize);
    assert!(body.contains("class=\"ui-pager\""));
    assert!(body.contains("1 to 50 of 61"));
    assert!(body.contains("rel=\"next\""));
    assert!(!body.contains("rel=\"prev\""));
    // Ordered by owner then title: "Extra 000".."Extra 059", then "Test Meeting".
    assert!(has_entry(&body, &extra[0]));
    assert!(!has_entry(&body, &et_id));

    let body = page("/dashboard/directory?page=2".into()).await;
    assert_eq!(entry_count(&body), 11);
    assert!(has_entry(&body, &et_id));
    assert!(body.contains("51 to 61 of 61"));
    // Past the end, or nonsense: the last page, or the first.
    let body = page("/dashboard/directory?page=99".into()).await;
    assert!(body.contains("51 to 61 of 61"));
    let body = page("/dashboard/directory?page=abc".into()).await;
    assert!(body.contains("1 to 50 of 61"));

    // The search narrows in SQL and the pager keeps it in its links.
    let body = page("/dashboard/directory?q=extra%2005".into()).await;
    assert_eq!(entry_count(&body), 11);
    assert!(!body.contains("class=\"ui-pager\""));
    let body = page("/dashboard/directory?q=extra".into()).await;
    assert!(body.contains("1 to 50 of 60"));
    assert!(body
        .replace("&amp;", "&")
        .replace("&#x2f;", "/")
        .contains("/dashboard/directory?q=extra&page=2"));
}

#[tokio::test]
async fn paged_queries_keep_every_visibility_check_for_favorites_and_searches() {
    let (app, pool, _, et_id) = setup_test_app().await;
    let (_, viewer) = insert_test_user_with_session(&pool, "Viewer", "viewer@x", "viewer").await;
    let extra = add_meetings(&pool, &et_id, 2).await;
    for id in [&et_id, &extra[0], &extra[1]] {
        assert_eq!(favorite(&app, &viewer, id, true).await.status(), 200);
    }
    // Starred, then hidden in every way a meeting can be.
    for update in [
        "UPDATE event_types SET visibility = 'private' WHERE slug = 'extra-000'",
        "UPDATE event_types SET enabled = 0 WHERE slug = 'extra-001'",
    ] {
        sqlx::raw_sql(update).execute(&pool).await.unwrap();
    }
    for uri in [
        "/dashboard",
        "/dashboard/directory",
        "/dashboard/directory?favorites=true",
        "/dashboard/directory?favorites=true&q=extra",
        "/dashboard/directory?q=extra",
        "/dashboard/directory?q=extra&page=2",
    ] {
        let body = body_string(app.clone().oneshot(get_authed(uri, &viewer)).await.unwrap()).await;
        assert!(!has_entry(&body, &extra[0]), "{uri}");
        assert!(!has_entry(&body, &extra[1]), "{uri}");
    }
    for id in &extra {
        assert_eq!(favorite(&app, &viewer, id, true).await.status(), 404);
    }
    // A private team's public meeting is never offered, starred or searched.
    let (team, team_et) = insert_team_with_et(&pool, "hidden-team", "demo", &[]).await;
    assert_eq!(favorite(&app, &viewer, &team_et, true).await.status(), 200);
    sqlx::query("UPDATE teams SET visibility = 'private' WHERE id = ?")
        .bind(&team)
        .execute(&pool)
        .await
        .unwrap();
    for uri in [
        "/dashboard",
        "/dashboard/directory?favorites=true",
        "/dashboard/directory?q=hidden-team",
    ] {
        let body = body_string(app.clone().oneshot(get_authed(uri, &viewer)).await.unwrap()).await;
        assert!(!has_entry(&body, &team_et), "{uri}");
    }
    // The count behind the overview link only counts what the viewer may see.
    let page = directory_page(
        &pool,
        &crate_user_id(&pool, "viewer@x").await,
        &DirectoryFilter {
            favorites_only: true,
            ..DirectoryFilter::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(page.total, 1);
}

async fn crate_user_id(pool: &SqlitePool, email: &str) -> String {
    sqlx::query_scalar("SELECT id FROM users WHERE email = ?")
        .bind(email)
        .fetch_one(pool)
        .await
        .unwrap()
}

#[tokio::test]
async fn closed_booking_windows_follow_each_meetings_own_zone() {
    let pool = setup_test_db().await;
    let owner = insert_role_user(&pool, "owner@x", "user").await;
    let viewer = insert_role_user(&pool, "viewer@x", "user").await;
    let utc_today = Utc::now().date_naive();
    let mut expected = Vec::new();
    for (i, tz) in [
        "Pacific/Kiritimati",
        "Pacific/Pago_Pago",
        "UTC",
        "Europe/Paris",
    ]
    .iter()
    .enumerate()
    {
        for days in -3..=2 {
            let until = utc_today + Duration::days(days);
            let id = insert_personal_et(&pool, &owner, &format!("w{i}-{}", days + 3)).await;
            sqlx::query("UPDATE event_types SET timezone = ?, bookable_until = ? WHERE id = ?")
                .bind(tz)
                .bind(until.format("%Y-%m-%d").to_string())
                .bind(&id)
                .execute(&pool)
                .await
                .unwrap();
            let window = BookingWindow {
                bookable_until: Some(until),
                ..BookingWindow::default()
            };
            let open = !window.is_expired(today_in(tz.parse().unwrap()));
            expected.push((id, open, format!("{tz} until {until}")));
        }
    }
    // Values SQLite does not read as dates are no bound, as in BookingWindow.
    for (slug, value) in [
        ("odd-1", " 2000-01-01 "),
        ("odd-2", "2000-02-30"),
        ("odd-3", "soon"),
    ] {
        let id = insert_personal_et(&pool, &owner, slug).await;
        sqlx::query("UPDATE event_types SET bookable_until = ? WHERE id = ?")
            .bind(value)
            .bind(&id)
            .execute(&pool)
            .await
            .unwrap();
        let open = parse_optional_date(Some(value)).is_none_or(|d| d >= utc_today);
        expected.push((id, open, format!("odd {value:?}")));
    }
    let page = directory_page(&pool, &viewer, &DirectoryFilter::default())
        .await
        .unwrap();
    let listed: Vec<&str> = page.entries.iter().map(|e| e.id.as_str()).collect();
    for (id, open, what) in &expected {
        assert_eq!(listed.contains(&id.as_str()), *open, "{what}");
    }
    assert_eq!(page.total as usize, listed.len());
}

#[tokio::test]
async fn overview_shows_a_bounded_number_of_favorites_and_links_to_the_rest() {
    let (app, pool, _, et_id) = setup_test_app().await;
    let (_, viewer) = insert_test_user_with_session(&pool, "Viewer", "viewer@x", "viewer").await;
    let extra = add_meetings(&pool, &et_id, 14).await;
    for id in extra.iter().take(3) {
        assert_eq!(favorite(&app, &viewer, id, true).await.status(), 200);
    }
    let overview = || {
        let app = app.clone();
        let viewer = viewer.clone();
        async move {
            body_string(
                app.oneshot(get_authed("/dashboard", &viewer))
                    .await
                    .unwrap(),
            )
            .await
        }
    };
    let body = overview().await;
    assert_eq!(entry_count(&body), 3);
    assert!(body.contains("Browse directory"));
    assert!(!body.contains("All favorites"));
    for id in extra.iter().skip(3) {
        assert_eq!(favorite(&app, &viewer, id, true).await.status(), 200);
    }
    let body = overview().await;
    assert_eq!(entry_count(&body), OVERVIEW_FAVORITES as usize);
    assert!(body.contains("All favorites (14)"));
    assert!(body
        .replace("&#x2f;", "/")
        .contains("/dashboard/directory?favorites=true"));
}

#[tokio::test]
async fn search_treats_glob_characters_literally() {
    let (app, pool, session, et_id) = setup_test_app().await;
    let extra = add_meetings(&pool, &et_id, 1).await;
    sqlx::query("UPDATE event_types SET title = 'Plan [a*b?]' WHERE id = ?")
        .bind(&extra[0])
        .execute(&pool)
        .await
        .unwrap();
    for q in ["%5Ba*b%3F%5D", "*", "%3F", "%5B"] {
        let body = body_string(
            app.clone()
                .oneshot(get_authed(&format!("/dashboard/directory?q={q}"), &session))
                .await
                .unwrap(),
        )
        .await;
        assert!(has_entry(&body, &extra[0]), "{q}");
        assert!(!has_entry(&body, &et_id), "{q}");
    }
}
