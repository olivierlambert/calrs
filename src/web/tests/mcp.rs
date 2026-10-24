use super::*;
use axum::http::{Request, StatusCode};
use serde_json::{json, Value};

fn rpc(token: &str, value: Value) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri("/mcp")
        .header("content-type", "application/json")
        .header("accept", "application/json, text/event-stream")
        .header("mcp-protocol-version", "2025-11-25")
        .header("authorization", format!("Bearer {token}"))
        .body(Body::from(value.to_string()))
        .unwrap()
}

async fn call(app: &Router, token: &str, name: &str, args: Value) -> Value {
    let response = app.clone().oneshot(rpc(token, json!({"jsonrpc":"2.0","id":7,"method":"tools/call","params":{"name":name,"arguments":args}}))).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let value: Value = serde_json::from_str(&body_string(response).await).unwrap();
    assert_eq!(value["id"], 7);
    value["result"].clone()
}

#[tokio::test]
async fn mcp_transport_auth_discovery_lifecycle_and_errors() {
    let _lock = crate::test_support::ENV_LOCK.lock().await;
    let _env = oauth::environment();
    let (app, _, session, _) = setup_test_app().await;
    let response = app.clone().oneshot(rpc("", json!({}))).await.unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert!(response.headers()["www-authenticate"]
        .to_str()
        .unwrap()
        .contains("https://cal.example/.well-known/oauth-protected-resource/mcp"));
    // A dashboard session does not authorize an MCP request.
    assert_eq!(
        app.clone()
            .oneshot(get_authed("/mcp", &session))
            .await
            .unwrap()
            .status(),
        StatusCode::UNAUTHORIZED
    );
    let (_, tokens) = oauth::issue_tokens(&app, &session).await;
    let token = tokens["access_token"].as_str().unwrap();
    let response = app.clone().oneshot(rpc(token,json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"future-version","capabilities":{},"clientInfo":{"name":"test","version":"1"}}}))).await.unwrap();
    assert!(response.headers().get("mcp-session-id").is_none());
    let value: Value = serde_json::from_str(&body_string(response).await).unwrap();
    assert_eq!(value["result"]["protocolVersion"], "2025-11-25");
    assert_eq!(value["result"]["serverInfo"]["name"], "calrs");
    let response = app
        .clone()
        .oneshot(rpc(
            token,
            json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::ACCEPTED);
    assert!(body_string(response).await.is_empty());
    let response = app
        .clone()
        .oneshot(rpc(
            token,
            json!({"jsonrpc":"2.0","id":"tools","method":"tools/list"}),
        ))
        .await
        .unwrap();
    let value: Value = serde_json::from_str(&body_string(response).await).unwrap();
    assert_eq!(value["result"]["tools"].as_array().unwrap().len(), 3);
    for method in ["GET", "DELETE"] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(method)
                    .uri("/mcp")
                    .header("authorization", format!("Bearer {token}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
    }
    for (header, value, status) in [
        ("origin", "https://evil.example", StatusCode::FORBIDDEN),
        ("origin", "null", StatusCode::FORBIDDEN),
        ("mcp-protocol-version", "future", StatusCode::BAD_REQUEST),
        ("accept", "application/json", StatusCode::NOT_ACCEPTABLE),
        (
            "content-type",
            "text/plain",
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
        ),
    ] {
        let mut request = rpc(token, json!({"jsonrpc":"2.0","id":2,"method":"ping"}));
        request.headers_mut().insert(header, value.parse().unwrap());
        assert_eq!(
            app.clone().oneshot(request).await.unwrap().status(),
            status,
            "{header}"
        );
    }
    let mut request = rpc(token, json!({"jsonrpc":"2.0","id":2,"method":"ping"}));
    request
        .headers_mut()
        .insert("origin", "https://cal.example".parse().unwrap());
    request.headers_mut().remove("mcp-protocol-version");
    assert_eq!(
        app.clone().oneshot(request).await.unwrap().status(),
        StatusCode::OK
    );
    for invalid in [
        json!([]),
        json!({"jsonrpc":"1.0","id":1,"method":"ping"}),
        json!({"jsonrpc":"2.0","id":null,"method":"ping"}),
    ] {
        assert_eq!(
            app.clone()
                .oneshot(rpc(token, invalid))
                .await
                .unwrap()
                .status(),
            StatusCode::BAD_REQUEST
        );
    }
    let mut request = rpc(token, json!({}));
    *request.body_mut() = Body::from("{");
    let response = app.clone().oneshot(request).await.unwrap();
    let value: Value = serde_json::from_str(&body_string(response).await).unwrap();
    assert_eq!(value["error"]["code"], -32700);
    let mut request = rpc(token, json!({}));
    *request.body_mut() = Body::from("x".repeat(65537));
    assert_eq!(
        app.clone().oneshot(request).await.unwrap().status(),
        StatusCode::PAYLOAD_TOO_LARGE
    );
    let response = app
        .clone()
        .oneshot(rpc(
            token,
            json!({"jsonrpc":"2.0","id":3,"method":"unknown"}),
        ))
        .await
        .unwrap();
    let value: Value = serde_json::from_str(&body_string(response).await).unwrap();
    assert_eq!(value["error"]["code"], -32601);
}

#[tokio::test]
async fn mcp_invite_expiry_options_are_persisted_and_validated() {
    let _lock = crate::test_support::ENV_LOCK.lock().await;
    let _env = oauth::environment();
    let (app, pool, session, event) = setup_test_app().await;
    sqlx::query("UPDATE event_types SET visibility = 'private'")
        .execute(&pool)
        .await
        .unwrap();
    let (_, tokens) = oauth::issue_tokens(&app, &session).await;
    let token = tokens["access_token"].as_str().unwrap();
    for days in [7, 14, 30, 0] {
        let before = Utc::now().naive_utc();
        let result = call(
            &app,
            token,
            "create_invite_link",
            json!({"event_type_id":event,"expires_days":days}),
        )
        .await;
        assert_eq!(result["isError"], false);
        let invite = &result["structuredContent"];
        assert_eq!(invite["max_uses"], 1);
        let url = reqwest::Url::parse(invite["url"].as_str().unwrap()).unwrap();
        let invite_token = url
            .query_pairs()
            .find(|(k, _)| k == "invite")
            .unwrap()
            .1
            .into_owned();
        let expiry: Option<String> =
            sqlx::query_scalar("SELECT expires_at FROM booking_invites WHERE token = ?")
                .bind(invite_token)
                .fetch_one(&pool)
                .await
                .unwrap();
        if days == 0 {
            assert!(expiry.is_none());
            assert!(invite["expires_at"].is_null());
        } else {
            let expiry = expiry.unwrap();
            assert_eq!(invite["expires_at"], expiry.replace(' ', "T") + "Z");
            let actual = NaiveDateTime::parse_from_str(&expiry, "%Y-%m-%d %H:%M:%S").unwrap();
            assert!(actual >= before + Duration::days(days) - Duration::seconds(1));
            assert!(actual <= Utc::now().naive_utc() + Duration::days(days));
        }
    }
    for invalid in [json!(-1), json!(8), json!("never"), json!(null), json!(7.5)] {
        let result = call(
            &app,
            token,
            "create_invite_link",
            json!({"event_type_id":event,"expires_days":invalid}),
        )
        .await;
        assert_eq!(result["isError"], true);
    }
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM booking_invites")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 4);
}

#[tokio::test]
async fn mcp_invites_follow_permissions_and_use_real_booking_flow() {
    let _lock = crate::test_support::ENV_LOCK.lock().await;
    let _env = oauth::environment();
    let (app, pool, session, own) = setup_test_app().await;
    sqlx::query("UPDATE users SET role = 'user'")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE event_types SET visibility = 'private'")
        .execute(&pool)
        .await
        .unwrap();
    let owner: String = sqlx::query_scalar("SELECT user_id FROM sessions WHERE id = ?")
        .bind(&session)
        .fetch_one(&pool)
        .await
        .unwrap();
    let other = insert_role_user(&pool, "other@test.com", "user").await;
    let private = insert_personal_et(&pool, &other, "private").await;
    let internal = insert_personal_et(&pool, &other, "internal").await;
    sqlx::query("UPDATE event_types SET visibility = 'private' WHERE id = ?")
        .bind(&private)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE event_types SET visibility = 'internal' WHERE id = ?")
        .bind(&internal)
        .execute(&pool)
        .await
        .unwrap();
    let (team, team_event) = insert_team_with_et(
        &pool,
        "staff",
        "intro",
        &[(&owner, "member"), (&other, "admin")],
    )
    .await;
    sqlx::query("UPDATE event_types SET visibility = 'private' WHERE id = ?")
        .bind(&team_event)
        .execute(&pool)
        .await
        .unwrap();
    let (_, tokens) = oauth::issue_tokens(&app, &session).await;
    let token = tokens["access_token"].as_str().unwrap();
    let list = call(&app, token, "list_event_types", json!({})).await;
    let ids: Vec<&str> = list["structuredContent"]["event_types"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v["id"].as_str().unwrap())
        .collect();
    assert!(ids.contains(&own.as_str()) && ids.contains(&internal.as_str()));
    assert!(!ids.contains(&private.as_str()) && !ids.contains(&team_event.as_str()));
    for denied in [&private, &team_event] {
        assert_eq!(
            call(
                &app,
                token,
                "create_invite_link",
                json!({"event_type_id":denied})
            )
            .await["isError"],
            true
        );
    }
    let result = call(
        &app,
        token,
        "create_invite_link",
        json!({"event_type_id":own}),
    )
    .await;
    assert_eq!(result["isError"], false);
    let invite = &result["structuredContent"];
    assert_eq!(invite["max_uses"], 1);
    let url = reqwest::Url::parse(invite["url"].as_str().unwrap()).unwrap();
    assert_eq!(url.path(), "/u/testuser/test-meeting");
    let invite_token = url
        .query_pairs()
        .find(|(k, _)| k == "invite")
        .unwrap()
        .1
        .into_owned();
    let (creator, uses, expiry): (String, i64, String) = sqlx::query_as(
        "SELECT created_by_user_id, max_uses, expires_at FROM booking_invites WHERE token = ?",
    )
    .bind(&invite_token)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(creator, owner);
    assert_eq!(uses, 1);
    assert!(
        NaiveDateTime::parse_from_str(&expiry, "%Y-%m-%d %H:%M:%S").unwrap()
            > Utc::now().naive_utc() + Duration::days(6)
    );
    let response = app
        .clone()
        .oneshot(get(&booking_form_uri(Some(&invite_token))))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert!(body_string(response).await.contains("Test Meeting"));
    assert_eq!(
        call(
            &app,
            token,
            "create_invite_link",
            json!({"event_type_id":internal})
        )
        .await["isError"],
        false
    );
    sqlx::query("UPDATE team_members SET role = 'admin' WHERE team_id = ? AND user_id = ?")
        .bind(&team)
        .bind(&owner)
        .execute(&pool)
        .await
        .unwrap();
    let result = call(
        &app,
        token,
        "create_invite_link",
        json!({"event_type_id":team_event}),
    )
    .await;
    assert_eq!(result["isError"], false);
    assert!(result["structuredContent"]["url"]
        .as_str()
        .unwrap()
        .contains("/team/staff/intro?invite="));
    sqlx::query("UPDATE event_types SET enabled = 0 WHERE id = ?")
        .bind(&own)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        call(
            &app,
            token,
            "create_invite_link",
            json!({"event_type_id":own})
        )
        .await["isError"],
        true
    );
    sqlx::query("UPDATE oauth_grants SET scope = 'event-types:read'")
        .execute(&pool)
        .await
        .unwrap();
    let response = app.clone().oneshot(rpc(token,json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"create_invite_link","arguments":{"event_type_id":internal}}}))).await.unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    assert!(response.headers()["www-authenticate"]
        .to_str()
        .unwrap()
        .contains("invites:write"));
}

#[tokio::test]
async fn mcp_blocks_are_bounded_merged_utc_private_and_include_pending() {
    let _lock = crate::test_support::ENV_LOCK.lock().await;
    let _env = oauth::environment();
    let (app, pool, session, et) = setup_test_app().await;
    let account: String = sqlx::query_scalar("SELECT account_id FROM event_types WHERE id = ?")
        .bind(&et)
        .fetch_one(&pool)
        .await
        .unwrap();
    seed_synced_event(
        &pool,
        &account,
        "20261019T090000",
        "20261019T100000",
        "Europe/Paris",
        "OPAQUE",
    )
    .await;
    seed_synced_event(
        &pool,
        &account,
        "20261019T120000",
        "20261019T130000",
        "UTC",
        "TRANSPARENT",
    )
    .await;
    insert_test_booking(
        &pool,
        &et,
        "2026-10-19T07:30:00Z",
        "2026-10-19T09:00:00Z",
        "pending",
    )
    .await;
    sqlx::query("UPDATE bookings SET time_version = 1")
        .execute(&pool)
        .await
        .unwrap();
    let other = insert_role_user(&pool, "other@test.com", "user").await;
    let other_et = insert_personal_et(&pool, &other, "other").await;
    insert_test_booking(
        &pool,
        &other_et,
        "2026-10-19T10:00:00Z",
        "2026-10-19T11:00:00Z",
        "confirmed",
    )
    .await;
    let (_, tokens) = oauth::issue_tokens(&app, &session).await;
    let token = tokens["access_token"].as_str().unwrap();
    let result = call(
        &app,
        token,
        "get_calendar_blocks",
        json!({"start":"2026-10-19T07:15:00Z","end":"2026-10-19T15:00:00Z"}),
    )
    .await;
    assert_eq!(result["isError"], false, "{result}");
    assert_eq!(
        result["structuredContent"]["blocks"],
        json!([{"start":"2026-10-19T07:15:00+00:00","end":"2026-10-19T09:00:00+00:00"}])
    );
    assert_eq!(result["structuredContent"]["cached"], true);
    for args in [
        json!({"start":"2026-10-19T00:00:00Z","end":"2026-12-19T00:00:00Z"}),
        json!({"start":"invalid","end":"invalid"}),
        json!({"start":"2026-10-19T00:00:00Z","end":"2026-10-18T00:00:00Z"}),
        json!({"start":"2026-10-19T00:00:00Z","end":"2026-10-20T00:00:00Z","user_id":other}),
    ] {
        assert_eq!(
            call(&app, token, "get_calendar_blocks", args).await["isError"],
            true
        );
    }
    // A failed query must not masquerade as an empty/free calendar.
    sqlx::query("DROP TABLE events")
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        call(
            &app,
            token,
            "get_calendar_blocks",
            json!({"start":"2026-10-19T00:00:00Z","end":"2026-10-20T00:00:00Z"})
        )
        .await["isError"],
        true
    );
}

#[tokio::test]
async fn mcp_sparse_pagination_and_failed_insert_do_not_issue_links() {
    let _lock = crate::test_support::ENV_LOCK.lock().await;
    let _env = oauth::environment();
    let (app, pool, session, et) = setup_test_app().await;
    sqlx::query("UPDATE users SET role = 'user'")
        .execute(&pool)
        .await
        .unwrap();
    let other = insert_role_user(&pool, "other@test.com", "user").await;
    for i in 0..101 {
        insert_personal_et(&pool, &other, &format!("hidden-{i}")).await;
    }
    sqlx::query("UPDATE event_types SET visibility = 'private'")
        .execute(&pool)
        .await
        .unwrap();
    let (_, tokens) = oauth::issue_tokens(&app, &session).await;
    let token = tokens["access_token"].as_str().unwrap();
    let first = call(&app, token, "list_event_types", json!({})).await;
    let cursor = &first["structuredContent"]["next_cursor"];
    assert!(cursor.is_string());
    let second = call(&app, token, "list_event_types", json!({"cursor":cursor})).await;
    assert!(second["structuredContent"]["next_cursor"].is_null());
    let count = first["structuredContent"]["event_types"]
        .as_array()
        .unwrap()
        .len()
        + second["structuredContent"]["event_types"]
            .as_array()
            .unwrap()
            .len();
    assert_eq!(count, 1);
    sqlx::query("CREATE TRIGGER refuse_invites BEFORE INSERT ON booking_invites BEGIN SELECT RAISE(FAIL, 'test failure'); END").execute(&pool).await.unwrap();
    let result = call(
        &app,
        token,
        "create_invite_link",
        json!({"event_type_id":et}),
    )
    .await;
    assert_eq!(result["isError"], true);
    assert!(result.get("structuredContent").is_none());
    // Requests without IDs never perform writes.
    let response = app.clone().oneshot(rpc(token,json!({"jsonrpc":"2.0","method":"tools/call","params":{"name":"create_invite_link","arguments":{"event_type_id":et}}}))).await.unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn mcp_legacy_bookings_use_event_or_owner_timezone_and_preserve_utc_rows() {
    let _lock = crate::test_support::ENV_LOCK.lock().await;
    let _env = oauth::environment();
    let (app, pool, session, event) = setup_test_app().await;
    sqlx::query("UPDATE users SET timezone = 'America/New_York'")
        .execute(&pool)
        .await
        .unwrap();
    let owner: String = sqlx::query_scalar("SELECT user_id FROM sessions WHERE id = ?")
        .bind(&session)
        .fetch_one(&pool)
        .await
        .unwrap();
    let inherited = insert_personal_et(&pool, &owner, "owner-zone").await;
    let tokyo = insert_personal_et(&pool, &owner, "tokyo-zone").await;
    sqlx::query("UPDATE event_types SET timezone = 'Europe/Paris' WHERE id = ?")
        .bind(&event)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE event_types SET timezone = 'Asia/Tokyo' WHERE id = ?")
        .bind(&tokyo)
        .execute(&pool)
        .await
        .unwrap();
    // The legacy Z suffix does not indicate UTC when time_version is zero.
    insert_test_booking(
        &pool,
        &event,
        "2026-07-01T10:00:00Z",
        "2026-07-01T10:30:00Z",
        "confirmed",
    )
    .await;
    insert_test_booking(
        &pool,
        &inherited,
        "2026-07-01T10:00:00",
        "2026-07-01T10:30:00",
        "pending",
    )
    .await;
    insert_test_booking(
        &pool,
        &tokyo,
        "2026-07-02T00:30:00",
        "2026-07-02T01:00:00",
        "confirmed",
    )
    .await;
    let (utc, _, _) = insert_test_booking(
        &pool,
        &event,
        "2026-07-01T12:00:00Z",
        "2026-07-01T12:30:00Z",
        "confirmed",
    )
    .await;
    sqlx::query("UPDATE bookings SET time_version = 1 WHERE id = ?")
        .bind(&utc)
        .execute(&pool)
        .await
        .unwrap();
    let (_, tokens) = oauth::issue_tokens(&app, &session).await;
    let token = tokens["access_token"].as_str().unwrap();
    let result = call(
        &app,
        token,
        "get_calendar_blocks",
        json!({"start":"2026-07-01T00:00:00Z", "end":"2026-07-02T00:00:00Z"}),
    )
    .await;
    assert_eq!(
        result["structuredContent"]["blocks"],
        json!([
            {"start":"2026-07-01T08:00:00+00:00", "end":"2026-07-01T08:30:00+00:00"},
            {"start":"2026-07-01T12:00:00+00:00", "end":"2026-07-01T12:30:00+00:00"},
            {"start":"2026-07-01T14:00:00+00:00", "end":"2026-07-01T14:30:00+00:00"},
            {"start":"2026-07-01T15:30:00+00:00", "end":"2026-07-01T16:00:00+00:00"}
        ])
    );
    // A repeated legacy wall clock has two possible UTC interpretations.
    insert_test_booking(
        &pool,
        &event,
        "2026-10-25T02:15:00",
        "2026-10-25T02:45:00",
        "confirmed",
    )
    .await;
    let result = call(
        &app,
        token,
        "get_calendar_blocks",
        json!({"start":"2026-10-25T00:00:00Z", "end":"2026-10-25T04:00:00Z"}),
    )
    .await;
    assert_eq!(
        result["structuredContent"]["blocks"],
        json!([
            {"start":"2026-10-25T00:15:00+00:00", "end":"2026-10-25T01:45:00+00:00"}
        ])
    );
}

#[tokio::test]
async fn mcp_public_links_are_reusable_and_private_team_links_work() {
    let _lock = crate::test_support::ENV_LOCK.lock().await;
    let _env = oauth::environment();
    let (app, pool, session, et) = setup_test_app().await;
    let (_, tokens) = oauth::issue_tokens(&app, &session).await;
    let token = tokens["access_token"].as_str().unwrap();
    let result = call(
        &app,
        token,
        "create_invite_link",
        json!({"event_type_id":et}),
    )
    .await;
    assert_eq!(result["isError"], false);
    assert_eq!(
        result["structuredContent"]["url"],
        "https://cal.example/u/testuser/test-meeting"
    );
    assert!(result["structuredContent"]["expires_at"].is_null());
    assert!(result["structuredContent"]["max_uses"].is_null());
    let (team, team_et) = insert_team_with_et(&pool, "hidden-team", "demo", &[]).await;
    sqlx::query(
        "UPDATE teams SET visibility = 'private', invite_token = 'team-token' WHERE id = ?",
    )
    .bind(&team)
    .execute(&pool)
    .await
    .unwrap();
    let result = call(
        &app,
        token,
        "create_invite_link",
        json!({"event_type_id":team_et}),
    )
    .await;
    assert_eq!(result["isError"], false);
    assert_eq!(
        result["structuredContent"]["url"],
        "https://cal.example/team/hidden-team/demo?invite=team-token"
    );
    let response = app
        .clone()
        .oneshot(get("/team/hidden-team/demo?invite=team-token"))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM booking_invites")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        count, 0,
        "public links must not promise unenforced invite limits"
    );
}
