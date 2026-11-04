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
    let (app, pool, _, et_id) = setup_test_app().await;
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
