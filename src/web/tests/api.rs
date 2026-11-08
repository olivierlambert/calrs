// REST API v1 (#169): authentication, scopes, ownership, the booking path
// shared with the web form, idempotency and rate limits.

use super::writeback_failures::{spawn_mock_dav, DavMode};
use super::*;
use axum::http::{Request, StatusCode};
use serde_json::{json, Value};

const API: &str = "https://cal.example/api/v1";
const MCP: &str = "https://cal.example/mcp";

fn api_get(uri: &str, token: &str) -> Request<Body> {
    Request::builder()
        .uri(uri)
        .header("authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap()
}

fn api_post(uri: &str, token: &str, body: &Value) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(uri)
        .header("authorization", format!("Bearer {token}"))
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap()
}

async fn send(app: &Router, request: Request<Body>) -> (StatusCode, Value) {
    let response = app.clone().oneshot(request).await.unwrap();
    let status = response.status();
    assert_eq!(
        response
            .headers()
            .get("cache-control")
            .map(|v| v.to_str().unwrap()),
        Some("no-store")
    );
    assert!(
        response.headers().get("set-cookie").is_none(),
        "the API never sets cookies"
    );
    let body = body_string(response).await;
    (status, serde_json::from_str(&body).unwrap_or(Value::Null))
}

async fn user_id(pool: &SqlitePool, username: &str) -> String {
    sqlx::query_scalar("SELECT id FROM users WHERE username = ?")
        .bind(username)
        .fetch_one(pool)
        .await
        .unwrap()
}

/// A personal access token of `user` with `scope`, inserted directly.
async fn pat(pool: &SqlitePool, user: &str, scope: &str) -> String {
    let secret = crate::oauth::random_token();
    let token = format!("calrs_pat_{secret}");
    sqlx::query(
        "INSERT INTO api_tokens (id, user_id, name, token_hash, hint, scope, created_at) VALUES (?, ?, 'test', ?, ?, ?, ?)",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(user)
    .bind(crate::oauth::hash(&token))
    .bind(&secret[..4])
    .bind(scope)
    .bind(Utc::now().timestamp())
    .execute(pool)
    .await
    .unwrap();
    token
}

const ALL: &str = "event-types:read slots:read bookings:read bookings:write";

fn error_code(body: &Value) -> &str {
    body["error"]["code"].as_str().unwrap_or("")
}

/// `far_future_monday()` at `hh:mm` UTC, as RFC 3339.
fn monday_at(hhmm: &str) -> String {
    format!("{}T{hhmm}:00Z", far_future_monday())
}

fn create_body(et_id: &str, start: &str, email: &str) -> Value {
    json!({"event_type_id": et_id, "start": start, "timezone": "UTC", "name": "Api Guest", "email": email})
}

#[tokio::test]
async fn api_needs_a_bearer_token_and_never_a_cookie() {
    let _lock = crate::test_support::ENV_LOCK.lock().await;
    let _env = super::oauth::environment();
    let (app, pool, session, _) = setup_test_app().await;

    let (status, body) = send(&app, get("/api/v1/event-types")).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(error_code(&body), "unauthorized");
    assert!(body["error"]["message"].is_string());
    let response = app
        .clone()
        .oneshot(get("/api/v1/event-types"))
        .await
        .unwrap();
    let challenge = response.headers()["www-authenticate"].to_str().unwrap();
    assert!(challenge.contains("https://cal.example/.well-known/oauth-protected-resource/api/v1"));
    // RFC 6750 section 3.1: no error code without credentials, one with a bad token.
    assert!(!challenge.contains("error="), "{challenge}");
    let response = app
        .clone()
        .oneshot(api_get("/api/v1/event-types", "calrs_pat_unknown"))
        .await
        .unwrap();
    assert!(response.headers()["www-authenticate"]
        .to_str()
        .unwrap()
        .contains("error=\"invalid_token\""));

    // A dashboard session is no credential here (no ambient authority).
    let (status, _) = send(&app, get_authed("/api/v1/event-types", &session)).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (status, _) = send(
        &app,
        Request::builder()
            .uri("/api/v1/event-types")
            .header("authorization", format!("Bearer {session}"))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    let token = pat(&pool, &user_id(&pool, "testuser").await, ALL).await;
    let (status, body) = send(&app, api_get("/api/v1/event-types", &token)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["event_types"].as_array().unwrap().len(), 1);
    assert_eq!(body["next_cursor"], Value::Null);

    // JSON errors everywhere under /api, including unknown paths and methods.
    let (status, body) = send(&app, api_get("/api/v1/nothing", &token)).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(error_code(&body), "not_found");
    let (status, body) = send(&app, api_get("/api/v1/bookings/%FF", &token)).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(error_code(&body), "not_found");
    let (status, body) = send(&app, api_post("/api/v1/event-types", &token, &json!({}))).await;
    assert_eq!(status, StatusCode::METHOD_NOT_ALLOWED);
    assert_eq!(error_code(&body), "method_not_allowed");

    // No CORS: a browser page of another origin is refused outright.
    let mut request = api_get("/api/v1/event-types", &token);
    request
        .headers_mut()
        .insert("origin", "https://evil.example".parse().unwrap());
    let (status, body) = send(&app, request).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(error_code(&body), "origin_not_allowed");
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("OPTIONS")
                .uri("/api/v1/bookings")
                .header("origin", "https://evil.example")
                .header("access-control-request-method", "POST")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert!(response
        .headers()
        .get("access-control-allow-origin")
        .is_none());
}

#[tokio::test]
async fn api_scopes_are_enforced_per_endpoint() {
    let _lock = crate::test_support::ENV_LOCK.lock().await;
    let _env = super::oauth::environment();
    let (app, pool, _, et_id) = setup_test_app().await;
    let user = user_id(&pool, "testuser").await;
    let day = far_future_monday();
    let cases: Vec<(&str, Request<Body>)> = vec![
        ("event-types:read", api_get("/api/v1/event-types", "")),
        (
            "event-types:read",
            api_get(&format!("/api/v1/event-types/{et_id}"), ""),
        ),
        (
            "slots:read",
            api_get(
                &format!("/api/v1/event-types/{et_id}/slots?from={day}&to={day}"),
                "",
            ),
        ),
        ("bookings:read", api_get("/api/v1/bookings", "")),
        (
            "bookings:write",
            api_post(
                "/api/v1/bookings",
                "",
                &create_body(&et_id, &monday_at("10:00"), "s@test.com"),
            ),
        ),
    ];
    for (scope, request) in cases {
        let others: Vec<&str> = crate::oauth::API_SCOPES
            .iter()
            .copied()
            .filter(|s| *s != scope)
            .collect();
        let narrow = pat(&pool, &user, &others.join(" ")).await;
        let (mut parts, body) = request.into_parts();
        parts
            .headers
            .insert("authorization", format!("Bearer {narrow}").parse().unwrap());
        let response = app
            .clone()
            .oneshot(Request::from_parts(parts, body))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN, "{scope}");
        assert!(response.headers()["www-authenticate"]
            .to_str()
            .unwrap()
            .contains(&format!("scope=\"{scope}\"")));
    }
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM bookings")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
}

#[tokio::test]
async fn api_tokens_page_creates_once_and_revokes() {
    let _lock = crate::test_support::ENV_LOCK.lock().await;
    let _env = super::oauth::environment();
    let (app, pool, session, _) = setup_test_app().await;
    let csrf = "csrf-api";
    let response = app
        .clone()
        .oneshot(post_form(
            "/dashboard/settings/api-tokens",
            &session,
            csrf,
            &format!("_csrf={csrf}&name=CRM+sync&scope_event_types=on&expires_days=30"),
        ))
        .await
        .unwrap();
    // Post/Redirect/Get: the secret is on the page the redirect opens, once.
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    let location = response.headers()["location"].to_str().unwrap().to_string();
    assert!(location.starts_with("/dashboard/settings/api-tokens?created="));
    let response = app
        .clone()
        .oneshot(get_authed(&location, &session))
        .await
        .unwrap();
    assert_eq!(response.headers()["cache-control"], "no-store");
    let page = body_string(response).await;
    let reload = body_string(
        app.clone()
            .oneshot(get_authed(&location, &session))
            .await
            .unwrap(),
    )
    .await;
    let start = page.find("calrs_pat_").unwrap();
    let token: String = page[start..]
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
        .collect();
    assert!(!reload.contains(&token), "shown once");
    assert_eq!(token.len(), "calrs_pat_".len() + 43);
    // Stored hashed; the list shows only a hint afterwards.
    let stored: (String, String, Option<i64>) =
        sqlx::query_as("SELECT token_hash, scope, expires_at FROM api_tokens")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(stored.0, crate::oauth::hash(&token));
    assert_eq!(stored.1, "event-types:read");
    assert!(stored.2.unwrap() > Utc::now().timestamp() + 29 * 86400);
    let list = body_string(
        app.clone()
            .oneshot(get_authed("/dashboard/settings/api-tokens", &session))
            .await
            .unwrap(),
    )
    .await;
    assert!(list.contains("CRM sync"));
    assert!(!list.contains(&token));

    let (status, _) = send(&app, api_get("/api/v1/event-types", &token)).await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = send(&app, api_get("/api/v1/bookings", &token)).await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    // Without CSRF nothing is created; without a scope neither.
    let response = app
        .clone()
        .oneshot(post_form(
            "/dashboard/settings/api-tokens",
            &session,
            csrf,
            "name=x&scope_slots=on",
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let response = app
        .clone()
        .oneshot(post_form(
            "/dashboard/settings/api-tokens",
            &session,
            csrf,
            &format!("_csrf={csrf}&name=x&expires_days=7"),
        ))
        .await
        .unwrap();
    assert!(body_string(response)
        .await
        .contains("Choose at least one permission."));
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM api_tokens")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 1);

    // Another user's revoke deletes nothing.
    let (_, other_session) =
        insert_test_user_with_session(&pool, "Other", "other@test.com", "other").await;
    let id: String = sqlx::query_scalar("SELECT id FROM api_tokens")
        .fetch_one(&pool)
        .await
        .unwrap();
    let uri = format!("/dashboard/settings/api-tokens/{id}/revoke");
    app.clone()
        .oneshot(post_form(
            &uri,
            &other_session,
            csrf,
            &format!("_csrf={csrf}"),
        ))
        .await
        .unwrap();
    let (status, _) = send(&app, api_get("/api/v1/event-types", &token)).await;
    assert_eq!(status, StatusCode::OK);
    let response = app
        .clone()
        .oneshot(post_form(&uri, &session, csrf, &format!("_csrf={csrf}")))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    let (status, _) = send(&app, api_get("/api/v1/event-types", &token)).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn api_tokens_follow_the_admin_switch_suspension_expiry_and_mfa() {
    let _lock = crate::test_support::ENV_LOCK.lock().await;
    let _env = super::oauth::environment();
    let (app, pool, _, _) = setup_test_app().await;
    let (member, member_session) =
        insert_test_user_with_session(&pool, "Member", "member@test.com", "member").await;
    let csrf = "csrf-api";
    let create = |session: String| {
        post_form(
            "/dashboard/settings/api-tokens",
            &session,
            csrf,
            &format!("_csrf={csrf}&name=n&scope_event_types=on&expires_days=0"),
        )
    };
    // Off by default: a member cannot create one, and the page says so.
    let response = app
        .clone()
        .oneshot(create(member_session.clone()))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let page = body_string(
        app.clone()
            .oneshot(get_authed(
                "/dashboard/settings/api-tokens",
                &member_session,
            ))
            .await
            .unwrap(),
    )
    .await;
    assert!(page.contains("Only administrators can create API tokens"));

    let admin_session: String = sqlx::query_scalar("SELECT id FROM sessions WHERE user_id = ?")
        .bind(user_id(&pool, "testuser").await)
        .fetch_one(&pool)
        .await
        .unwrap();
    let response = app
        .clone()
        .oneshot(post_form(
            "/dashboard/admin/api-tokens",
            &admin_session,
            csrf,
            &format!("_csrf={csrf}&allow_all_users=on"),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    // A member cannot flip the switch.
    let response = app
        .clone()
        .oneshot(post_form(
            "/dashboard/admin/api-tokens",
            &member_session,
            csrf,
            &format!("_csrf={csrf}"),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let response = app
        .clone()
        .oneshot(create(member_session.clone()))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    let token = pat(&pool, &member, "event-types:read").await;
    let (status, _) = send(&app, api_get("/api/v1/event-types", &token)).await;
    assert_eq!(status, StatusCode::OK);

    // Switching it off stops the member's tokens at once.
    sqlx::query("UPDATE auth_config SET api_tokens_allow_all_users = 0")
        .execute(&pool)
        .await
        .unwrap();
    let (status, _) = send(&app, api_get("/api/v1/event-types", &token)).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    sqlx::query("UPDATE auth_config SET api_tokens_allow_all_users = 1")
        .execute(&pool)
        .await
        .unwrap();
    let (status, _) = send(&app, api_get("/api/v1/event-types", &token)).await;
    assert_eq!(status, StatusCode::OK);

    // Expired.
    sqlx::query("UPDATE api_tokens SET expires_at = ? WHERE user_id = ?")
        .bind(Utc::now().timestamp() - 1)
        .bind(&member)
        .execute(&pool)
        .await
        .unwrap();
    let (status, _) = send(&app, api_get("/api/v1/event-types", &token)).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    sqlx::query("UPDATE api_tokens SET expires_at = NULL WHERE user_id = ?")
        .bind(&member)
        .execute(&pool)
        .await
        .unwrap();

    // The admin's MFA requirement applies to a token created without MFA.
    sqlx::query("UPDATE auth_config SET mfa_required = 1")
        .execute(&pool)
        .await
        .unwrap();
    let (status, _) = send(&app, api_get("/api/v1/event-types", &token)).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    sqlx::query("UPDATE auth_config SET mfa_required = 0")
        .execute(&pool)
        .await
        .unwrap();
    let (status, _) = send(&app, api_get("/api/v1/event-types", &token)).await;
    assert_eq!(status, StatusCode::OK);

    // Suspension deletes the tokens: re-enabling does not bring them back.
    crate::auth::set_user_enabled(&pool, &member, false)
        .await
        .unwrap();
    crate::auth::set_user_enabled(&pool, &member, true)
        .await
        .unwrap();
    let (status, _) = send(&app, api_get("/api/v1/event-types", &token)).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let left: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM api_tokens WHERE user_id = ?")
        .bind(&member)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(left, 0);

    // The admin's "revoke connected apps" takes the tokens too.
    let token = pat(&pool, &member, "event-types:read").await;
    let response = app
        .clone()
        .oneshot(post_form(
            &format!("/dashboard/admin/users/{member}/revoke-apps"),
            &admin_session,
            csrf,
            &format!("_csrf={csrf}"),
        ))
        .await
        .unwrap();
    assert!(response.status().is_redirection());
    let (status, _) = send(&app, api_get("/api/v1/event-types", &token)).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn api_oauth_grants_are_bound_to_the_api_resource() {
    let _lock = crate::test_support::ENV_LOCK.lock().await;
    let _env = super::oauth::environment();
    let (app, pool, session, _) = setup_test_app().await;

    let metadata: Value = serde_json::from_str(
        &body_string(
            app.clone()
                .oneshot(get("/.well-known/oauth-protected-resource/api/v1"))
                .await
                .unwrap(),
        )
        .await,
    )
    .unwrap();
    assert_eq!(metadata["resource"], API);
    assert_eq!(metadata["scopes_supported"].as_array().unwrap().len(), 4);

    let tokens = super::oauth::issue_tokens_for(&app, &session, API, None).await;
    assert_eq!(tokens["scope"], ALL);
    let api_token = tokens["access_token"].as_str().unwrap().to_string();
    let (status, body) = send(&app, api_get("/api/v1/event-types", &api_token)).await;
    assert_eq!(status, StatusCode::OK, "{body}");

    // An MCP token does not open the API, nor an API token MCP.
    let mcp = super::oauth::issue_tokens_for(&app, &session, MCP, None).await;
    let mcp_token = mcp["access_token"].as_str().unwrap();
    let (status, _) = send(&app, api_get("/api/v1/event-types", mcp_token)).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let mut headers = HeaderMap::new();
    headers.insert(
        "authorization",
        format!("Bearer {api_token}").parse().unwrap(),
    );
    assert!(crate::oauth::bearer_user(&pool, &headers, MCP)
        .await
        .unwrap()
        .is_none());

    // MCP scopes cannot be asked for the API, nor API scopes for MCP.
    for (resource, scope) in [(API, "calendar:read"), (MCP, "bookings:write")] {
        let client = super::oauth::register_client(&app).await;
        let uri = format!("/oauth/authorize?client_id={client}&redirect_uri={}&response_type=code&code_challenge={}&code_challenge_method=S256&resource={}&scope={scope}", urlencoding::encode("http://127.0.0.1:4321/callback"), crate::oauth::hash("abcdefghijklmnopqrstuvwxyz0123456789ABCDEFGH"), urlencoding::encode(resource));
        let response = app.clone().oneshot(get(&uri)).await.unwrap();
        assert!(response.headers()["location"]
            .to_str()
            .unwrap()
            .contains("error=invalid_scope"));
    }

    // A narrower grant only gets what it asked for, and shows on Connected
    // apps with the API's own permissions.
    let narrow = super::oauth::issue_tokens_for(&app, &session, API, Some("bookings:read")).await;
    let narrow = narrow["access_token"].as_str().unwrap();
    let (status, _) = send(&app, api_get("/api/v1/bookings", narrow)).await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = send(&app, api_get("/api/v1/event-types", narrow)).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let apps = body_string(
        app.clone()
            .oneshot(get_authed("/dashboard/settings/apps", &session))
            .await
            .unwrap(),
    )
    .await;
    assert!(apps.contains("Read the bookings you host"));
    assert!(apps.contains("Book, cancel and reschedule"));

    // Revocation from Connected apps ends it at once.
    let grants: Vec<String> = sqlx::query_scalar("SELECT id FROM oauth_grants WHERE resource = ?")
        .bind(API)
        .fetch_all(&pool)
        .await
        .unwrap();
    for grant in grants {
        app.clone()
            .oneshot(post_form(
                &format!("/dashboard/settings/apps/{grant}/revoke"),
                &session,
                "c",
                "_csrf=c",
            ))
            .await
            .unwrap();
    }
    let (status, _) = send(&app, api_get("/api/v1/event-types", &api_token)).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn api_event_types_follow_the_integration_rules() {
    let _lock = crate::test_support::ENV_LOCK.lock().await;
    let _env = super::oauth::environment();
    let (app, pool, _, et_id) = setup_test_app().await;
    let owner = user_id(&pool, "testuser").await;
    let other = insert_role_user(&pool, "other@test.com", "user").await;
    let member = insert_role_user(&pool, "member@test.com", "user").await;
    sqlx::query("UPDATE auth_config SET api_tokens_allow_all_users = 1")
        .execute(&pool)
        .await
        .unwrap();
    let (_, team_et) = insert_team_with_et(
        &pool,
        "crew",
        "crew-call",
        &[(&owner, "admin"), (&member, "member")],
    )
    .await;
    let other_token = pat(&pool, &other, ALL).await;
    let member_token = pat(&pool, &member, ALL).await;
    let owner_token = pat(&pool, &owner, ALL).await;

    let ids = |body: &Value| -> Vec<(String, bool)> {
        body["event_types"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| {
                (
                    e["id"].as_str().unwrap().to_string(),
                    e["manageable"].as_bool().unwrap(),
                )
            })
            .collect()
    };
    let (_, body) = send(&app, api_get("/api/v1/event-types", &owner_token)).await;
    let mut listed = ids(&body);
    listed.sort();
    let mut expected = vec![(et_id.clone(), true), (team_et.clone(), true)];
    expected.sort();
    assert_eq!(listed, expected);
    let (_, body) = send(&app, api_get("/api/v1/event-types", &other_token)).await;
    assert!(ids(&body).is_empty());
    let (_, body) = send(&app, api_get("/api/v1/event-types", &member_token)).await;
    assert!(ids(&body).is_empty(), "a plain team member manages nothing");

    // Another user's event type is not found, whatever the endpoint.
    let day = far_future_monday();
    for request in [
        api_get(&format!("/api/v1/event-types/{et_id}"), &other_token),
        api_get(
            &format!("/api/v1/event-types/{et_id}/slots?from={day}&to={day}"),
            &other_token,
        ),
        api_post(
            "/api/v1/bookings",
            &other_token,
            &create_body(&et_id, &monday_at("10:00"), "x@test.com"),
        ),
    ] {
        let (status, body) = send(&app, request).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    }

    // An internal type the directory offers is listed read-only.
    sqlx::query("UPDATE event_types SET visibility = 'internal' WHERE id = ?")
        .bind(&et_id)
        .execute(&pool)
        .await
        .unwrap();
    let (_, body) = send(&app, api_get("/api/v1/event-types", &other_token)).await;
    assert_eq!(ids(&body), vec![(et_id.clone(), false)]);
    let (status, body) = send(
        &app,
        api_get(&format!("/api/v1/event-types/{et_id}"), &other_token),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["event_type"]["manageable"], false);
    assert!(body["event_type"].get("location_value").is_none());
    let (status, body) = send(
        &app,
        api_get(
            &format!("/api/v1/event-types/{et_id}/slots?from={day}&to={day}"),
            &other_token,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(error_code(&body), "forbidden");
    let (status, _) = send(
        &app,
        api_post(
            "/api/v1/bookings",
            &other_token,
            &create_body(&et_id, &monday_at("10:00"), "x@test.com"),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    // Disabled types disappear.
    sqlx::query("UPDATE event_types SET enabled = 0 WHERE id = ?")
        .bind(&et_id)
        .execute(&pool)
        .await
        .unwrap();
    let (status, _) = send(
        &app,
        api_get(&format!("/api/v1/event-types/{et_id}"), &owner_token),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn api_slots_come_from_the_slot_engine_and_booking_takes_them() {
    let _lock = crate::test_support::ENV_LOCK.lock().await;
    let _env = super::oauth::environment();
    let (app, pool, _, et_id) = setup_test_app().await;
    let owner = user_id(&pool, "testuser").await;
    let account: String = sqlx::query_scalar("SELECT id FROM accounts WHERE user_id = ?")
        .bind(&owner)
        .fetch_one(&pool)
        .await
        .unwrap();
    let day = far_future_monday();
    let compact = day.replace('-', "");
    seed_synced_event(
        &pool,
        &account,
        &format!("{compact}T100000"),
        &format!("{compact}T110000"),
        "UTC",
        "OPAQUE",
    )
    .await;
    // Fresh sources: no sync is attempted.
    sqlx::query("UPDATE caldav_sources SET last_synced = datetime('now')")
        .execute(&pool)
        .await
        .unwrap();
    let token = pat(&pool, &owner, ALL).await;
    let uri = format!("/api/v1/event-types/{et_id}/slots?from={day}&to={day}");
    let (status, body) = send(&app, api_get(&uri, &token)).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let times: Vec<String> = body["slots"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["time"].as_str().unwrap().to_string())
        .collect();
    let mut expected = Vec::new();
    for h in 9..17 {
        for m in [0, 30] {
            if h != 10 {
                expected.push(format!("{h:02}:{m:02}"));
            }
        }
    }
    assert_eq!(times, expected);
    assert_eq!(body["slots"][0]["start"], monday_at("09:00"));
    assert_eq!(body["slots"][0]["end"], monday_at("09:30"));

    // In another timezone: the guest's day, the guest's times.
    let (_, tokyo) = send(&app, api_get(&format!("{uri}&timezone=Asia/Tokyo"), &token)).await;
    assert_eq!(tokyo["timezone"], "Asia/Tokyo");
    let first = &tokyo["slots"][0];
    assert_eq!(first["start"], monday_at("09:00"));
    assert_eq!(first["time"], "18:00");
    assert!(tokyo["slots"]
        .as_array()
        .unwrap()
        .iter()
        .all(|s| s["date"] == day.as_str()));
    // 15:00 UTC is the next day in Tokyo.
    assert!(!tokyo["slots"]
        .as_array()
        .unwrap()
        .iter()
        .any(|s| s["start"] == monday_at("15:00").as_str()));

    // A time the engine offers can be booked, and is then gone.
    let mut create = create_body(&et_id, &monday_at("09:00"), "tokyo@test.com");
    create["timezone"] = json!("Asia/Tokyo");
    let (status, body) = send(&app, api_post("/api/v1/bookings", &token, &create)).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(body["booking"]["guest"]["timezone"], "Asia/Tokyo");
    assert_eq!(body["booking"]["start"], monday_at("09:00"));
    let (_, again) = send(&app, api_get(&uri, &token)).await;
    assert_ne!(again["slots"][0]["time"], "09:00");

    // Bad windows.
    for query in [
        format!("from={day}"),
        format!("from={day}&to=2020-01-01"),
        format!("from={day}&to={day}&timezone=Mars/Base"),
        format!("from={day}&to={day}&extra=1"),
        "from=2020-01-01&to=2020-01-02".to_string(),
    ] {
        let (status, body) = send(
            &app,
            api_get(
                &format!("/api/v1/event-types/{et_id}/slots?{query}"),
                &token,
            ),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{query}: {body}");
    }
}

/// Counts what one booking produced: emails per recipient and calendar
/// writes, for the comparison of the web form and the API.
async fn booking_effects(
    pool: &SqlitePool,
    mails: &std::sync::Mutex<Vec<CapturedMail>>,
    guest: &str,
) -> (usize, usize, bool, String) {
    let guest_mails = settled_mails_to(mails, guest).await;
    let with_ics = guest_mails
        .iter()
        .filter(|m| m.contains("text/calendar"))
        .count();
    let (href, status): (Option<String>, String) =
        sqlx::query_as("SELECT caldav_calendar_href, status FROM bookings WHERE guest_email = ?")
            .bind(guest)
            .fetch_one(pool)
            .await
            .unwrap();
    (guest_mails.len(), with_ics, href.is_some(), status)
}

#[tokio::test]
async fn api_booking_goes_through_the_web_booking_path() {
    let _lock = crate::test_support::ENV_LOCK.lock().await;
    let _env = super::oauth::environment();
    let (app, pool, _, et_id) = setup_test_app().await;
    let (port, mails) = spawn_capturing_mta().await;
    use_capturing_mta(&pool, port).await;
    let owner = user_id(&pool, "testuser").await;
    let (base, dav) = spawn_mock_dav(DavMode::Ok).await;
    seed_write_source(&pool, &owner, &base).await;
    sqlx::query("UPDATE caldav_sources SET last_synced = datetime('now')")
        .execute(&pool)
        .await
        .unwrap();
    let token = pat(&pool, &owner, ALL).await;

    // The web form.
    let csrf = "csrf-web";
    let response = app
        .clone()
        .oneshot(post_form_unauthed(
            "/u/testuser/test-meeting/book",
            csrf,
            &book_body(csrf),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    // The API, an hour later.
    let (status, body) = send(
        &app,
        api_post(
            "/api/v1/bookings",
            &token,
            &create_body(&et_id, &monday_at("11:00"), "api@test.com"),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(body["booking"]["status"], "confirmed");

    let web = booking_effects(&pool, &mails, "guest@test.com").await;
    let api = booking_effects(&pool, &mails, "api@test.com").await;
    assert_eq!(web, (1, 1, true, "confirmed".to_string()));
    assert_eq!(api, web);
    assert_eq!(dav.stored(), 2, "both pushed to the host calendar");
    assert_eq!(settled_mails_to(&mails, "test@example.com").await.len(), 2);

    // The stored rows agree on everything the form path sets.
    #[allow(clippy::type_complexity)]
    let rows: Vec<(i64, String, String, Option<String>, bool, bool)> = sqlx::query_as(
        "SELECT time_version, guest_timezone, language, confirm_token, cancel_token IS NOT NULL, reschedule_token IS NOT NULL FROM bookings ORDER BY start_at",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(rows[0], rows[1]);

    // No token of the booking leaves through the API.
    let (cancel, reschedule): (String, String) = sqlx::query_as(
        "SELECT cancel_token, reschedule_token FROM bookings WHERE guest_email = 'api@test.com'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    let text = body.to_string();
    assert!(!text.contains(&cancel) && !text.contains(&reschedule));

    // The same slot again: refused under the same rules as the form.
    let (status, body) = send(
        &app,
        api_post(
            "/api/v1/bookings",
            &token,
            &create_body(&et_id, &monday_at("10:00"), "late@test.com"),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(error_code(&body), "slot_unavailable");

    // An event type that needs approval: a pending request, no write-back,
    // and the host is asked to approve.
    sqlx::query("UPDATE event_types SET requires_confirmation = 1 WHERE id = ?")
        .bind(&et_id)
        .execute(&pool)
        .await
        .unwrap();
    let (status, body) = send(
        &app,
        api_post(
            "/api/v1/bookings",
            &token,
            &create_body(&et_id, &monday_at("14:00"), "pending@test.com"),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(body["booking"]["status"], "pending");
    let pending = booking_effects(&pool, &mails, "pending@test.com").await;
    assert_eq!(pending, (1, 0, false, "pending".to_string()));
    assert_eq!(dav.stored(), 2);
    let host = settled_mails_to(&mails, "test@example.com").await;
    assert_eq!(host.len(), 3);
    assert!(host.iter().any(|m| m.contains("/booking/approve/")));
}

#[tokio::test]
async fn api_booking_validates_its_input() {
    let _lock = crate::test_support::ENV_LOCK.lock().await;
    let _env = super::oauth::environment();
    let (app, pool, _, et_id) = setup_test_app().await;
    let token = pat(&pool, &user_id(&pool, "testuser").await, ALL).await;
    let base = create_body(&et_id, &monday_at("10:00"), "v@test.com");
    let with = |key: &str, value: Value| {
        let mut body = base.clone();
        body[key] = value;
        body
    };
    let cases = [
        (with("email", json!("not-an-email")), "invalid_request"),
        (with("name", json!("")), "invalid_request"),
        (with("start", json!("tomorrow")), "invalid_start"),
        (
            with("start", json!(monday_at("10:00").replace(":00Z", ":30Z"))),
            "invalid_start",
        ),
        (with("timezone", json!("Nowhere/City")), "invalid_timezone"),
        (with("language", json!("xx")), "invalid_request"),
        (with("attendees", json!(["a@x.com"])), "invalid_attendees"),
        (with("surprise", json!(1)), "invalid_json"),
    ];
    for (body, code) in cases {
        let (status, answer) = send(&app, api_post("/api/v1/bookings", &token, &body)).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
        assert_eq!(error_code(&answer), code, "{body}");
    }
    // Off the grid, in the past, outside the horizon.
    let (status, answer) = send(
        &app,
        api_post(
            "/api/v1/bookings",
            &token,
            &with("start", json!(monday_at("10:10"))),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(error_code(&answer), "slot_unavailable");
    let (status, answer) = send(
        &app,
        api_post(
            "/api/v1/bookings",
            &token,
            &with("start", json!("2020-01-06T10:00:00Z")),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(error_code(&answer), "too_soon");
    set_horizon(&pool, &et_id, 7).await;
    let (status, answer) = send(&app, api_post("/api/v1/bookings", &token, &base)).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(error_code(&answer), "outside_booking_window");

    // Not JSON, and too big.
    let mut request = api_post("/api/v1/bookings", &token, &base);
    request
        .headers_mut()
        .insert("content-type", "text/plain".parse().unwrap());
    let (status, answer) = send(&app, request).await;
    assert_eq!(status, StatusCode::UNSUPPORTED_MEDIA_TYPE);
    assert_eq!(error_code(&answer), "unsupported_media_type");
    let (status, answer) = send(
        &app,
        api_post(
            "/api/v1/bookings",
            &token,
            &with("notes", json!("x".repeat(20 * 1024))),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE);
    assert_eq!(error_code(&answer), "payload_too_large");
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM bookings")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
}

#[tokio::test]
async fn api_idempotency_key_replays_the_first_answer() {
    let _lock = crate::test_support::ENV_LOCK.lock().await;
    let _env = super::oauth::environment();
    let (app, pool, _, et_id) = setup_test_app().await;
    let owner = user_id(&pool, "testuser").await;
    let token = pat(&pool, &owner, ALL).await;
    let body = create_body(&et_id, &monday_at("10:00"), "idem@test.com");
    let keyed = |body: &Value, key: &str| {
        let mut request = api_post("/api/v1/bookings", &token, body);
        request
            .headers_mut()
            .insert("idempotency-key", key.parse().unwrap());
        request
    };
    let first = app.clone().oneshot(keyed(&body, "k-1")).await.unwrap();
    assert_eq!(first.status(), StatusCode::CREATED);
    let first_body = body_string(first).await;
    let second = app.clone().oneshot(keyed(&body, "k-1")).await.unwrap();
    assert_eq!(second.status(), StatusCode::CREATED);
    assert_eq!(second.headers()["idempotency-replayed"], "true");
    assert_eq!(body_string(second).await, first_body);
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM bookings")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 1);

    // The key with another body, and a malformed key.
    let other = create_body(&et_id, &monday_at("11:00"), "idem@test.com");
    let (status, answer) = send(&app, keyed(&other, "k-1")).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(error_code(&answer), "idempotency_key_reused");
    let (status, _) = send(&app, keyed(&other, "has space")).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // Keys are per user: another user's identical key is theirs.
    let other_user = insert_role_user(&pool, "o@test.com", "admin").await;
    let other_et = insert_personal_et(&pool, &other_user, "o-meet").await;
    for day in [1, 2, 3, 4, 5] {
        sqlx::query("INSERT INTO availability_rules (id, event_type_id, day_of_week, start_time, end_time) VALUES (?, ?, ?, '09:00', '17:00')")
            .bind(uuid::Uuid::new_v4().to_string())
            .bind(&other_et)
            .bind(day)
            .execute(&pool)
            .await
            .unwrap();
    }
    let other_token = pat(&pool, &other_user, ALL).await;
    let mut request = api_post(
        "/api/v1/bookings",
        &other_token,
        &create_body(&other_et, &monday_at("10:00"), "idem@test.com"),
    );
    request
        .headers_mut()
        .insert("idempotency-key", "k-1".parse().unwrap());
    let (status, _) = send(&app, request).await;
    assert_eq!(status, StatusCode::CREATED);

    // A refusal is replayed too; a key older than a day is forgotten.
    let taken = create_body(&et_id, &monday_at("10:00"), "again@test.com");
    let (status, _) = send(&app, keyed(&taken, "k-2")).await;
    assert_eq!(status, StatusCode::CONFLICT);
    let response = app.clone().oneshot(keyed(&taken, "k-2")).await.unwrap();
    assert_eq!(response.status(), StatusCode::CONFLICT);
    assert_eq!(response.headers()["idempotency-replayed"], "true");
    sqlx::query("UPDATE api_idempotency SET created_at = created_at - 90000")
        .execute(&pool)
        .await
        .unwrap();
    crate::oauth::cleanup(&pool).await.unwrap();
    let left: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM api_idempotency")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(left, 0);
}

#[tokio::test]
async fn api_bookings_are_read_and_changed_by_their_hosts_only() {
    let _lock = crate::test_support::ENV_LOCK.lock().await;
    let _env = super::oauth::environment();
    let (app, pool, _, et_id) = setup_test_app().await;
    let (port, mails) = spawn_capturing_mta().await;
    use_capturing_mta(&pool, port).await;
    let owner = user_id(&pool, "testuser").await;
    let other = insert_role_user(&pool, "other@test.com", "admin").await;
    let token = pat(&pool, &owner, ALL).await;
    let other_token = pat(&pool, &other, ALL).await;
    let start = Utc::now() + Duration::days(10);
    let id = insert_v1_booking(
        &pool,
        &et_id,
        "guest@b.test",
        start,
        30,
        "confirmed",
        &[("host_notes", Some("met at the fair".to_string()))],
    )
    .await;
    let pending = insert_v1_booking(
        &pool,
        &et_id,
        "pending@b.test",
        start + Duration::hours(2),
        30,
        "pending",
        &[],
    )
    .await;

    let (status, body) = send(&app, api_get("/api/v1/bookings", &token)).await;
    assert_eq!(status, StatusCode::OK);
    let listed = body["bookings"].as_array().unwrap();
    assert_eq!(listed.len(), 2);
    assert_eq!(listed[0]["id"], id.as_str());
    assert_eq!(listed[0]["host_notes"], "met at the fair");
    assert_eq!(listed[0]["guest"]["email"], "guest@b.test");
    assert!(listed[0].get("cancel_token").is_none());
    let (_, body) = send(&app, api_get("/api/v1/bookings?status=pending", &token)).await;
    assert_eq!(body["bookings"].as_array().unwrap().len(), 1);
    let from = (start + Duration::hours(1)).to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    let (_, body) = send(
        &app,
        api_get(&format!("/api/v1/bookings?from={from}"), &token),
    )
    .await;
    assert_eq!(body["bookings"][0]["id"], pending.as_str());
    let (status, _) = send(&app, api_get("/api/v1/bookings?status=gone", &token)).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // Another user (an admin, even) sees and changes none of it.
    let (_, body) = send(&app, api_get("/api/v1/bookings", &other_token)).await;
    assert!(body["bookings"].as_array().unwrap().is_empty());
    for request in [
        api_get(&format!("/api/v1/bookings/{id}"), &other_token),
        api_post(
            &format!("/api/v1/bookings/{id}/cancel"),
            &other_token,
            &json!({}),
        ),
        api_post(
            &format!("/api/v1/bookings/{id}/request-reschedule"),
            &other_token,
            &json!({}),
        ),
    ] {
        let (status, _) = send(&app, request).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }
    assert_eq!(booking_status(&pool, &id).await, "confirmed");

    // The host asks for a new time: a new link, the guest emailed.
    let before: String = sqlx::query_scalar("SELECT reschedule_token FROM bookings WHERE id = ?")
        .bind(&id)
        .fetch_one(&pool)
        .await
        .unwrap();
    let (status, body) = send(
        &app,
        api_post(
            &format!("/api/v1/bookings/{id}/request-reschedule"),
            &token,
            &json!({}),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let (after, by_host): (String, i64) =
        sqlx::query_as("SELECT reschedule_token, reschedule_by_host FROM bookings WHERE id = ?")
            .bind(&id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_ne!(before, after);
    assert_eq!(by_host, 1);
    assert!(!body.to_string().contains(&after));
    assert_eq!(settled_mails_to(&mails, "guest@b.test").await.len(), 1);

    // Cancel with a reason; the pending one is declined.
    let (status, body) = send(
        &app,
        api_post(
            &format!("/api/v1/bookings/{id}/cancel"),
            &token,
            &json!({"reason": "double booked"}),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["booking"]["status"], "cancelled");
    let guest = settled_mails_to(&mails, "guest@b.test").await;
    assert_eq!(guest.len(), 2);
    assert!(guest[1].contains("double booked"));
    let (status, body) = send(
        &app,
        api_post(&format!("/api/v1/bookings/{id}/cancel"), &token, &json!({})),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(error_code(&body), "booking_not_active");
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/v1/bookings/{pending}/cancel"))
                .header("authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(booking_status(&pool, &pending).await, "declined");
}

#[tokio::test]
async fn api_a_host_who_does_not_own_the_event_type_reads_but_cannot_cancel() {
    let _lock = crate::test_support::ENV_LOCK.lock().await;
    let _env = super::oauth::environment();
    let (app, pool, _, _) = setup_test_app().await;
    let owner = user_id(&pool, "testuser").await;
    let member = insert_role_user(&pool, "member@test.com", "admin").await;
    let (_, team_et) = insert_team_with_et(
        &pool,
        "crew",
        "crew-call",
        &[(&owner, "admin"), (&member, "member")],
    )
    .await;
    let id = insert_v1_booking(
        &pool,
        &team_et,
        "guest@t.test",
        Utc::now() + Duration::days(5),
        30,
        "confirmed",
        &[("assigned_user_id", Some(member.clone()))],
    )
    .await;
    let token = pat(&pool, &member, ALL).await;
    let (status, body) = send(&app, api_get(&format!("/api/v1/bookings/{id}"), &token)).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["booking"]["event_type"]["team_id"].is_string());
    let (status, body) = send(
        &app,
        api_post(&format!("/api/v1/bookings/{id}/cancel"), &token, &json!({})),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(error_code(&body), "forbidden");
    assert_eq!(booking_status(&pool, &id).await, "confirmed");
}

#[tokio::test]
async fn api_requests_are_rate_limited_per_token_and_writes_per_user() {
    let _lock = crate::test_support::ENV_LOCK.lock().await;
    let _env = super::oauth::environment();
    let (app, pool, _, _) = setup_test_app().await;
    let owner = user_id(&pool, "testuser").await;
    let token = pat(&pool, &owner, ALL).await;
    let second = pat(&pool, &owner, ALL).await;
    for _ in 0..300 {
        let (status, _) = send(&app, api_get("/api/v1/bookings", &token)).await;
        assert_eq!(status, StatusCode::OK);
    }
    let response = app
        .clone()
        .oneshot(api_get("/api/v1/bookings", &token))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
    assert!(response.headers().contains_key("retry-after"));
    let (_, body) = send(&app, api_get("/api/v1/bookings", &token)).await;
    assert_eq!(error_code(&body), "rate_limited");
    // Another token has its own read budget...
    let (status, _) = send(&app, api_get("/api/v1/bookings", &second)).await;
    assert_eq!(status, StatusCode::OK);
    // ...but writes share one per user.
    for _ in 0..60 {
        let (status, _) = send(
            &app,
            api_post("/api/v1/bookings/nope/cancel", &second, &json!({})),
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }
    let third = pat(&pool, &owner, ALL).await;
    let (status, _) = send(
        &app,
        api_post("/api/v1/bookings/nope/cancel", &third, &json!({})),
    )
    .await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
}

/// Webhook rows queued so far: (event, payload) in queue order.
async fn queued_webhooks(pool: &SqlitePool) -> Vec<(String, Value)> {
    let rows: Vec<(String, String)> =
        sqlx::query_as("SELECT event, payload FROM webhook_deliveries ORDER BY seq")
            .fetch_all(pool)
            .await
            .unwrap();
    rows.into_iter()
        .map(|(event, payload)| (event, serde_json::from_str(&payload).unwrap()))
        .collect()
}

/// A webhook payload without what differs from one booking to the next.
fn comparable(mut payload: Value) -> Value {
    for key in ["id", "occurred_at", "timestamp"] {
        payload[key] = Value::Null;
    }
    for key in ["uid", "start", "end", "created_at"] {
        payload["booking"][key] = Value::Null;
    }
    payload["booking"]["guest"]["email"] = Value::Null;
    payload
}

#[tokio::test]
async fn api_bookings_and_cancellations_queue_the_same_webhooks_as_the_web() {
    let _lock = crate::test_support::ENV_LOCK.lock().await;
    let _env = super::oauth::environment();
    let (app, pool, session, et_id) = setup_test_app().await;
    let owner = user_id(&pool, "testuser").await;
    sqlx::query(
        "INSERT INTO webhooks (id, user_id, event_type_id, url, secret_enc) VALUES (?, ?, NULL, 'https://93.184.215.14/hook', ?)",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(&owner)
    .bind(crate::crypto::encrypt_value(&[0u8; 32], "0123456789abcdef-secret").unwrap())
    .execute(&pool)
    .await
    .unwrap();
    let token = pat(&pool, &owner, ALL).await;

    let csrf = "csrf-wh";
    let response = app
        .clone()
        .oneshot(post_form_unauthed(
            "/u/testuser/test-meeting/book",
            csrf,
            &book_body(csrf),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let (status, body) = send(
        &app,
        api_post(
            "/api/v1/bookings",
            &token,
            &create_body(&et_id, &monday_at("11:00"), "api@test.com"),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let api_id = body["booking"]["id"].as_str().unwrap().to_string();
    let rows = queued_webhooks(&pool).await;
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].0, "booking.created");
    assert_eq!(rows[1].0, rows[0].0);
    let (mut web, mut api) = (rows[0].1.clone(), rows[1].1.clone());
    // The guest's name differs by design of the two fixtures.
    web["booking"]["guest"]["name"] = Value::Null;
    api["booking"]["guest"]["name"] = Value::Null;
    assert_eq!(comparable(web), comparable(api));
    assert_eq!(rows[1].1["booking"]["guest"]["email"], "api@test.com");

    // Cancelling: the dashboard button and the API queue the same event.
    let web_id: String =
        sqlx::query_scalar("SELECT id FROM bookings WHERE guest_email = 'guest@test.com'")
            .fetch_one(&pool)
            .await
            .unwrap();
    let response = app
        .clone()
        .oneshot(post_form(
            &format!("/dashboard/bookings/{web_id}/cancel"),
            &session,
            csrf,
            &format!("_csrf={csrf}&reason=Moved"),
        ))
        .await
        .unwrap();
    assert!(response.status().is_redirection());
    let (status, _) = send(
        &app,
        api_post(
            &format!("/api/v1/bookings/{api_id}/cancel"),
            &token,
            &json!({"reason": "Moved"}),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let rows = queued_webhooks(&pool).await;
    assert_eq!(rows.len(), 4);
    assert_eq!(rows[2].0, "booking.cancelled");
    assert_eq!(rows[3].0, rows[2].0);
    for row in &rows[2..] {
        assert_eq!(row.1["booking"]["cancelled_by"], "host");
        assert_eq!(row.1["booking"]["reason"], "Moved");
    }

    // A reschedule request queues nothing, on the web as through the API.
    let other = insert_v1_booking(
        &pool,
        &et_id,
        "r@test.com",
        Utc::now() + Duration::days(9),
        30,
        "confirmed",
        &[],
    )
    .await;
    let (status, _) = send(
        &app,
        api_post(
            &format!("/api/v1/bookings/{other}/request-reschedule"),
            &token,
            &json!({}),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(queued_webhooks(&pool).await.len(), 4);
}

#[tokio::test]
async fn api_oauth_grants_follow_the_admin_switch() {
    let _lock = crate::test_support::ENV_LOCK.lock().await;
    let _env = super::oauth::environment();
    let (app, pool, _, _) = setup_test_app().await;
    let (_, member_session) =
        insert_test_user_with_session(&pool, "Member", "member@test.com", "member").await;

    // Closed to non-admins: the consent answers access_denied, no grant.
    let client = super::oauth::register_client(&app).await;
    let uri = format!("/oauth/authorize?client_id={client}&redirect_uri={}&response_type=code&code_challenge={}&code_challenge_method=S256&resource={}", urlencoding::encode("http://127.0.0.1:4321/callback"), crate::oauth::hash("abcdefghijklmnopqrstuvwxyz0123456789ABCDEFGH"), urlencoding::encode(API));
    let response = app.clone().oneshot(get(&uri)).await.unwrap();
    let request = super::oauth::request_cookie(&response);
    let page = body_string(
        app.clone()
            .oneshot(
                Request::builder()
                    .uri("/oauth/consent")
                    .header(
                        "cookie",
                        format!(
                            "__Host-calrs_session={member_session}; __Host-calrs_oauth={request}"
                        ),
                    )
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap(),
    )
    .await;
    // Said before the click: no Allow button, an explanation instead.
    assert!(page.contains("only administrators can use on this instance"));
    assert!(!page.contains("value=\"allow\""));
    let response = app
        .clone()
        .oneshot(super::oauth::consent_request(
            &member_session,
            &request,
            "allow",
            "csrf",
        ))
        .await
        .unwrap();
    let location = response.headers()["location"].to_str().unwrap().to_string();
    assert!(location.contains("error=access_denied"), "{location}");
    assert!(!location.contains("code="));
    let grants: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM oauth_grants")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(grants, 0);
    // MCP stays open to everyone.
    let mcp = super::oauth::issue_tokens_for(&app, &member_session, MCP, None).await;
    assert!(mcp["access_token"].is_string());

    // Opened: the member connects; closed again: the grant stops working.
    sqlx::query("UPDATE auth_config SET api_tokens_allow_all_users = 1")
        .execute(&pool)
        .await
        .unwrap();
    let tokens = super::oauth::issue_tokens_for(&app, &member_session, API, None).await;
    let token = tokens["access_token"].as_str().unwrap();
    let (status, _) = send(&app, api_get("/api/v1/event-types", token)).await;
    assert_eq!(status, StatusCode::OK);
    sqlx::query("UPDATE auth_config SET api_tokens_allow_all_users = 0")
        .execute(&pool)
        .await
        .unwrap();
    let (status, _) = send(&app, api_get("/api/v1/event-types", token)).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn api_bookings_are_capped_per_user_and_in_attendees() {
    let _lock = crate::test_support::ENV_LOCK.lock().await;
    let _env = super::oauth::environment();
    let (app, pool, _, et_id) = setup_test_app().await;
    let owner = user_id(&pool, "testuser").await;
    sqlx::query("UPDATE event_types SET max_additional_guests = 100 WHERE id = ?")
        .bind(&et_id)
        .execute(&pool)
        .await
        .unwrap();
    let token = pat(&pool, &owner, ALL).await;
    let mut body = create_body(&et_id, &monday_at("10:00"), "cap@test.com");
    body["attendees"] = json!((0..11)
        .map(|i| format!("a{i}@test.com"))
        .collect::<Vec<_>>());
    let (status, answer) = send(&app, api_post("/api/v1/bookings", &token, &body)).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(error_code(&answer), "too_many_attendees");
    body["attendees"] = json!((0..10)
        .map(|i| format!("a{i}@test.com"))
        .collect::<Vec<_>>());
    let (status, _) = send(&app, api_post("/api/v1/bookings", &token, &body)).await;
    assert_eq!(status, StatusCode::CREATED);

    // 50 bookings a day per user: the one just made counts.
    let now = Utc::now().timestamp();
    for _ in 0..49 {
        sqlx::query(
            "INSERT INTO api_booking_log (user_id, booking_id, created_at) VALUES (?, 'x', ?)",
        )
        .bind(&owner)
        .bind(now)
        .execute(&pool)
        .await
        .unwrap();
    }
    let (status, answer) = send(
        &app,
        api_post(
            "/api/v1/bookings",
            &token,
            &create_body(&et_id, &monday_at("11:00"), "cap2@test.com"),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(error_code(&answer), "daily_limit_reached");
    // A day later the log is swept and the cap frees up.
    sqlx::query("UPDATE api_booking_log SET created_at = created_at - 86401")
        .execute(&pool)
        .await
        .unwrap();
    crate::oauth::cleanup(&pool).await.unwrap();
    let (status, _) = send(
        &app,
        api_post(
            "/api/v1/bookings",
            &token,
            &create_body(&et_id, &monday_at("11:00"), "cap2@test.com"),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
}

#[tokio::test]
async fn a_reschedule_request_is_not_sent_twice() {
    let _lock = crate::test_support::ENV_LOCK.lock().await;
    let _env = super::oauth::environment();
    let (app, pool, session, et_id) = setup_test_app().await;
    let (port, mails) = spawn_capturing_mta().await;
    use_capturing_mta(&pool, port).await;
    let token = pat(&pool, &user_id(&pool, "testuser").await, ALL).await;
    let start = Utc::now() + Duration::days(8);
    let web = insert_v1_booking(&pool, &et_id, "web@r.test", start, 30, "confirmed", &[]).await;
    let api = insert_v1_booking(
        &pool,
        &et_id,
        "api@r.test",
        start + Duration::hours(2),
        30,
        "confirmed",
        &[],
    )
    .await;
    // The dashboard: the second click is refused with its own message.
    let csrf = "csrf-rr";
    let uri = format!("/dashboard/bookings/{web}/reschedule");
    let first = app
        .clone()
        .oneshot(post_form(&uri, &session, csrf, &format!("_csrf={csrf}")))
        .await
        .unwrap();
    assert_eq!(first.headers()["location"], "/dashboard/bookings");
    let token_after: String =
        sqlx::query_scalar("SELECT reschedule_token FROM bookings WHERE id = ?")
            .bind(&web)
            .fetch_one(&pool)
            .await
            .unwrap();
    let second = app
        .clone()
        .oneshot(post_form(&uri, &session, csrf, &format!("_csrf={csrf}")))
        .await
        .unwrap();
    assert_eq!(
        second.headers()["location"],
        "/dashboard/bookings?error=reschedule-pending"
    );
    let page = body_string(
        app.clone()
            .oneshot(get_authed(
                "/dashboard/bookings?error=reschedule-pending",
                &session,
            ))
            .await
            .unwrap(),
    )
    .await;
    assert!(page.contains("already asked to pick a new time"));
    let unchanged: String =
        sqlx::query_scalar("SELECT reschedule_token FROM bookings WHERE id = ?")
            .bind(&web)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(unchanged, token_after, "the guest's link still works");
    assert_eq!(settled_mails_to(&mails, "web@r.test").await.len(), 1);

    // The API: 409 the second time.
    let uri = format!("/api/v1/bookings/{api}/request-reschedule");
    let (status, _) = send(&app, api_post(&uri, &token, &json!({}))).await;
    assert_eq!(status, StatusCode::OK);
    let (status, body) = send(&app, api_post(&uri, &token, &json!({}))).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(error_code(&body), "reschedule_already_requested");
    assert_eq!(settled_mails_to(&mails, "api@r.test").await.len(), 1);
}

#[tokio::test]
async fn api_write_answers_and_replays_follow_the_tokens_read_scope() {
    let _lock = crate::test_support::ENV_LOCK.lock().await;
    let _env = super::oauth::environment();
    let (app, pool, _, et_id) = setup_test_app().await;
    let owner = user_id(&pool, "testuser").await;
    let write_only = pat(&pool, &owner, "bookings:write").await;
    let full = pat(&pool, &owner, ALL).await;
    let body = create_body(&et_id, &monday_at("10:00"), "w@test.com");
    let keyed = |token: &str| {
        let mut request = api_post("/api/v1/bookings", token, &body);
        request
            .headers_mut()
            .insert("idempotency-key", "k-w".parse().unwrap());
        request
    };
    let (status, answer) = send(&app, keyed(&write_only)).await;
    assert_eq!(status, StatusCode::CREATED);
    let booking = answer["booking"].as_object().unwrap();
    let mut keys: Vec<&str> = booking.keys().map(String::as_str).collect();
    keys.sort();
    assert_eq!(keys, ["id", "status"]);
    // The full token gets the full object, even on a replay of the write-only one.
    let (status, answer) = send(&app, keyed(&full)).await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(answer["booking"]["guest"]["email"], "w@test.com");
    let (_, answer) = send(&app, keyed(&write_only)).await;
    assert!(answer["booking"].get("guest").is_none());
    // A replay re-checks that the event type is still the caller's to book.
    sqlx::query("UPDATE event_types SET enabled = 0 WHERE id = ?")
        .bind(&et_id)
        .execute(&pool)
        .await
        .unwrap();
    let (status, _) = send(&app, keyed(&full)).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    // Cancel answers in the same shape.
    sqlx::query("UPDATE event_types SET enabled = 1 WHERE id = ?")
        .bind(&et_id)
        .execute(&pool)
        .await
        .unwrap();
    let id: String = sqlx::query_scalar("SELECT id FROM bookings")
        .fetch_one(&pool)
        .await
        .unwrap();
    let (status, answer) = send(
        &app,
        api_post(
            &format!("/api/v1/bookings/{id}/cancel"),
            &write_only,
            &json!({}),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(answer["booking"], json!({"id": id, "status": "cancelled"}));
}

#[tokio::test]
async fn api_bookings_use_formal_german_when_the_instance_does() {
    let _lock = crate::test_support::ENV_LOCK.lock().await;
    let _env = super::oauth::environment();
    let pool = setup_test_db().await;
    let (owner, _, et_id) = seed_test_data(&pool).await;
    sqlx::query("UPDATE auth_config SET german_formal = 1")
        .execute(&pool)
        .await
        .unwrap();
    let dir = std::sync::Arc::new(TestDataDir::new());
    let app = create_router(pool.clone(), dir.to_path_buf(), [0u8; 32])
        .await
        .layer(axum::Extension(dir));
    let token = pat(&pool, &owner, ALL).await;
    let mut body = create_body(&et_id, &monday_at("10:00"), "sie@test.com");
    body["language"] = json!("de");
    let (status, answer) = send(&app, api_post("/api/v1/bookings", &token, &body)).await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(answer["booking"]["guest"]["language"], "de-formal");
    // A host whose saved language is German gets the same by default.
    sqlx::query("UPDATE users SET language = 'de' WHERE id = ?")
        .bind(&owner)
        .execute(&pool)
        .await
        .unwrap();
    let (_, answer) = send(
        &app,
        api_post(
            "/api/v1/bookings",
            &token,
            &create_body(&et_id, &monday_at("11:00"), "sie2@test.com"),
        ),
    )
    .await;
    assert_eq!(answer["booking"]["guest"]["language"], "de-formal");
}

#[tokio::test]
async fn api_slots_skip_a_repeated_hour_as_the_booking_check_does() {
    let _lock = crate::test_support::ENV_LOCK.lock().await;
    let _env = super::oauth::environment();
    let (app, pool, _, et_id) = setup_test_app().await;
    let owner = user_id(&pool, "testuser").await;
    // The next last Sunday of October: Paris repeats 02:00-03:00 that night.
    let mut fold = host_today();
    loop {
        fold += Duration::days(1);
        let next_week = fold + Duration::days(7);
        if fold.month() == 10 && fold.weekday() == chrono::Weekday::Sun && next_week.month() == 11 {
            break;
        }
    }
    sqlx::query("UPDATE event_types SET timezone = 'Europe/Paris' WHERE id = ?")
        .bind(&et_id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO availability_rules (id, event_type_id, day_of_week, start_time, end_time) VALUES (?, ?, 0, '01:00', '04:00')")
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(&et_id)
        .execute(&pool)
        .await
        .unwrap();
    let token = pat(&pool, &owner, ALL).await;
    let (status, body) = send(
        &app,
        api_get(
            &format!("/api/v1/event-types/{et_id}/slots?from={fold}&to={fold}"),
            &token,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let times: Vec<&str> = body["slots"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["time"].as_str().unwrap())
        .collect();
    assert_eq!(times, ["01:00", "01:30", "03:00", "03:30"]);
    // Both 02:00s are refused by the booking check too.
    for instant in ["T00:00:00Z", "T01:00:00Z"] {
        let (status, body) = send(
            &app,
            api_post(
                "/api/v1/bookings",
                &token,
                &create_body(&et_id, &format!("{fold}{instant}"), "fold@test.com"),
            ),
        )
        .await;
        assert_eq!(status, StatusCode::CONFLICT, "{instant}: {body}");
    }
}

#[tokio::test]
async fn the_daily_cap_holds_under_concurrency_and_refusals_give_their_place_back() {
    let _lock = crate::test_support::ENV_LOCK.lock().await;
    let _env = super::oauth::environment();
    // Each of the ten bookings below holds a connection for its transaction
    // and reads on another. On the usual two-connection pool, two requests
    // holding both connections in their transactions starved each other's
    // reads until sqlx's 30 second acquire timeout, again and again: the
    // test took minutes, which under tarpaulin, with every other test of
    // the ENV_LOCK queued behind it, exceeded its 60 second silence limit.
    // Eleven connections always leave one free for a read.
    let (app, pool, _, et_id) = setup_test_app_with(11).await;
    let owner = user_id(&pool, "testuser").await;
    let token = pat(&pool, &owner, ALL).await;
    let log_count = || async {
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM api_booking_log")
            .fetch_one(&pool)
            .await
            .unwrap()
    };
    // A refused booking (the time is not offered) gives its place back.
    let (status, _) = send(
        &app,
        api_post(
            "/api/v1/bookings",
            &token,
            &create_body(&et_id, &monday_at("08:00"), "early@test.com"),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(log_count().await, 0);

    let now = Utc::now().timestamp();
    for _ in 0..48 {
        sqlx::query(
            "INSERT INTO api_booking_log (user_id, booking_id, created_at) VALUES (?, 'x', ?)",
        )
        .bind(&owner)
        .bind(now)
        .execute(&pool)
        .await
        .unwrap();
    }
    // Two places left, ten requests at once on ten different times.
    let mut requests = tokio::task::JoinSet::new();
    for (i, time) in [
        "09:00", "09:30", "10:00", "10:30", "11:00", "11:30", "12:00", "12:30", "13:00", "13:30",
    ]
    .into_iter()
    .enumerate()
    {
        let app = app.clone();
        let request = api_post(
            "/api/v1/bookings",
            &token,
            &create_body(&et_id, &monday_at(time), &format!("c{i}@test.com")),
        );
        requests.spawn(async move { app.oneshot(request).await.unwrap().status() });
    }
    let (mut created, mut capped) = (0, 0);
    while let Some(status) = requests.join_next().await {
        match status.unwrap() {
            StatusCode::CREATED => created += 1,
            StatusCode::TOO_MANY_REQUESTS => capped += 1,
            // Two requests racing in the booking core: the loser's place
            // was given back.
            StatusCode::CONFLICT => {}
            other => panic!("unexpected {other}"),
        }
    }
    // Some of the ten can also lose in the booking core (409), to SQLite's
    // locking; whatever happens, the cap holds.
    assert!(created <= 2, "{created} created");
    assert!(capped + created <= 10);
    assert_eq!(log_count().await, 48 + created);
    let bookings: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM bookings")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(bookings, created);
}

#[tokio::test]
async fn concurrent_reservations_never_pass_the_daily_cap() {
    let pool = setup_test_db().await;
    let (owner, _, _) = seed_test_data(&pool).await;
    let now = Utc::now().timestamp();
    for _ in 0..48 {
        sqlx::query(
            "INSERT INTO api_booking_log (user_id, booking_id, created_at) VALUES (?, 'x', ?)",
        )
        .bind(&owner)
        .bind(now)
        .execute(&pool)
        .await
        .unwrap();
    }
    let mut tasks = tokio::task::JoinSet::new();
    for _ in 0..20 {
        let (pool, owner) = (pool.clone(), owner.clone());
        tasks.spawn(async move {
            super::super::api::reserve_api_booking(&pool, &owner)
                .await
                .is_ok()
        });
    }
    let mut granted = 0;
    while let Some(ok) = tasks.join_next().await {
        granted += ok.unwrap() as i32;
    }
    assert_eq!(granted, 2);
    let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM api_booking_log")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(total, 50);
}

#[tokio::test]
async fn dropping_an_idempotent_request_stops_its_lease_renewals() {
    let pool = setup_test_db().await;
    let (owner, _, _) = seed_test_data(&pool).await;
    sqlx::query("INSERT INTO api_idempotency (user_id, key, request_hash, created_at, lease_at) VALUES (?, 'k', 'h', 0, 0)")
        .bind(&owner)
        .execute(&pool)
        .await
        .unwrap();
    let lease_at = || async {
        sqlx::query_scalar::<_, i64>("SELECT lease_at FROM api_idempotency")
            .fetch_one(&pool)
            .await
            .unwrap()
    };
    let every = std::time::Duration::from_millis(10);
    let guard = super::super::api::spawn_lease(pool.clone(), owner.clone(), "k".into(), every);
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    assert!(lease_at().await > 0, "renewed while running");
    // A request future dropped mid-flight drops its guard with it.
    let request = async move {
        let _guard = guard;
        std::future::pending::<()>().await;
    };
    let handle = tokio::spawn(request);
    tokio::time::sleep(std::time::Duration::from_millis(30)).await;
    handle.abort();
    let _ = handle.await;
    sqlx::query("UPDATE api_idempotency SET lease_at = 0")
        .execute(&pool)
        .await
        .unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    assert_eq!(lease_at().await, 0, "no renewal after the drop");
}

#[tokio::test]
async fn another_user_cannot_burn_a_token_flash() {
    let _lock = crate::test_support::ENV_LOCK.lock().await;
    let _env = super::oauth::environment();
    let (app, _pool, session, _) = setup_test_app().await;
    let (_, other_session) =
        insert_test_user_with_session(&_pool, "Other", "other@test.com", "other").await;
    let csrf = "csrf-flash";
    let response = app
        .clone()
        .oneshot(post_form(
            "/dashboard/settings/api-tokens",
            &session,
            csrf,
            &format!("_csrf={csrf}&name=n&scope_event_types=on&expires_days=7"),
        ))
        .await
        .unwrap();
    let location = response.headers()["location"].to_str().unwrap().to_string();
    let theirs = body_string(
        app.clone()
            .oneshot(get_authed(&location, &other_session))
            .await
            .unwrap(),
    )
    .await;
    assert!(!theirs.contains("api-tokens-created") && !theirs.contains("Copy this token now"));
    let mine = body_string(
        app.clone()
            .oneshot(get_authed(&location, &session))
            .await
            .unwrap(),
    )
    .await;
    assert!(mine.contains("Copy this token now"));
}

#[tokio::test]
async fn a_write_only_token_cannot_tell_a_hosted_booking_from_a_missing_one() {
    let _lock = crate::test_support::ENV_LOCK.lock().await;
    let _env = super::oauth::environment();
    let (app, pool, _, _) = setup_test_app().await;
    let owner = user_id(&pool, "testuser").await;
    let member = insert_role_user(&pool, "member@test.com", "admin").await;
    let (_, team_et) = insert_team_with_et(
        &pool,
        "crew",
        "crew-call",
        &[(&owner, "admin"), (&member, "member")],
    )
    .await;
    let id = insert_v1_booking(
        &pool,
        &team_et,
        "guest@t.test",
        Utc::now() + Duration::days(5),
        30,
        "confirmed",
        &[("assigned_user_id", Some(member.clone()))],
    )
    .await;
    let write_only = pat(&pool, &member, "bookings:write").await;
    for uri in [
        format!("/api/v1/bookings/{id}/cancel"),
        "/api/v1/bookings/missing/cancel".to_string(),
    ] {
        let (status, body) = send(&app, api_post(&uri, &write_only, &json!({}))).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{uri}");
        assert_eq!(error_code(&body), "not_found");
    }
    // The owner's own ended booking stays a 409 for a write-only token.
    sqlx::query("UPDATE bookings SET status = 'cancelled' WHERE id = ?")
        .bind(&id)
        .execute(&pool)
        .await
        .unwrap();
    let personal = insert_v1_booking(
        &pool,
        &sqlx::query_scalar::<_, String>("SELECT id FROM event_types WHERE slug = 'test-meeting'")
            .fetch_one(&pool)
            .await
            .unwrap(),
        "p@t.test",
        Utc::now() + Duration::days(6),
        30,
        "cancelled",
        &[],
    )
    .await;
    let owner_write = pat(&pool, &owner, "bookings:write").await;
    let (status, _) = send(
        &app,
        api_post(
            &format!("/api/v1/bookings/{personal}/cancel"),
            &owner_write,
            &json!({}),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
}

#[tokio::test]
async fn a_pending_reschedule_request_can_be_resent_once_an_hour() {
    let _lock = crate::test_support::ENV_LOCK.lock().await;
    let _env = super::oauth::environment();
    let (app, pool, session, et_id) = setup_test_app().await;
    let (port, mails) = spawn_capturing_mta().await;
    use_capturing_mta(&pool, port).await;
    let token = pat(&pool, &user_id(&pool, "testuser").await, ALL).await;
    let start = Utc::now() + Duration::days(8);
    let web = insert_v1_booking(&pool, &et_id, "web@rs.test", start, 30, "confirmed", &[]).await;
    let api = insert_v1_booking(
        &pool,
        &et_id,
        "api@rs.test",
        start + Duration::hours(2),
        30,
        "confirmed",
        &[],
    )
    .await;
    let csrf = "csrf-rs";
    let resend_uri = format!("/dashboard/bookings/{web}/reschedule/resend");
    let post = |uri: String| post_form(&uri, &session, csrf, &format!("_csrf={csrf}"));
    // Nothing pending: the resend does nothing.
    app.clone().oneshot(post(resend_uri.clone())).await.unwrap();
    assert!(settled_mails_to(&mails, "web@rs.test").await.is_empty());
    app.clone()
        .oneshot(post(format!("/dashboard/bookings/{web}/reschedule")))
        .await
        .unwrap();
    let link: String = sqlx::query_scalar("SELECT reschedule_token FROM bookings WHERE id = ?")
        .bind(&web)
        .fetch_one(&pool)
        .await
        .unwrap();
    let page = body_string(
        app.clone()
            .oneshot(get_authed("/dashboard/bookings", &session))
            .await
            .unwrap(),
    )
    .await;
    assert!(page.contains(&resend_uri), "the button is offered");
    // Within the hour: refused with its message.
    let response = app.clone().oneshot(post(resend_uri.clone())).await.unwrap();
    assert_eq!(
        response.headers()["location"],
        "/dashboard/bookings?error=reschedule-resend-too-soon"
    );
    assert_eq!(settled_mails_to(&mails, "web@rs.test").await.len(), 1);
    // An hour later: the same link, emailed again.
    sqlx::query("UPDATE bookings SET reschedule_requested_at = reschedule_requested_at - 3601")
        .execute(&pool)
        .await
        .unwrap();
    let response = app.clone().oneshot(post(resend_uri.clone())).await.unwrap();
    assert_eq!(response.headers()["location"], "/dashboard/bookings");
    let sent = settled_mails_to(&mails, "web@rs.test").await;
    assert_eq!(sent.len(), 2);
    assert!(sent[1].contains(&link));
    let after: String = sqlx::query_scalar("SELECT reschedule_token FROM bookings WHERE id = ?")
        .bind(&web)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(after, link);

    // The API: resend, too soon, nothing pending.
    let uri = format!("/api/v1/bookings/{api}/request-reschedule");
    let (status, body) = send(&app, api_post(&uri, &token, &json!({"resend": true}))).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(error_code(&body), "no_reschedule_pending");
    let (status, _) = send(&app, api_post(&uri, &token, &json!({}))).await;
    assert_eq!(status, StatusCode::OK);
    let (status, body) = send(&app, api_post(&uri, &token, &json!({"resend": true}))).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(error_code(&body), "resend_too_soon");
    sqlx::query("UPDATE bookings SET reschedule_requested_at = reschedule_requested_at - 3601")
        .execute(&pool)
        .await
        .unwrap();
    let (status, _) = send(&app, api_post(&uri, &token, &json!({"resend": true}))).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(settled_mails_to(&mails, "api@rs.test").await.len(), 2);
}
