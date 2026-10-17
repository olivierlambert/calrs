use super::*;
use axum::http::{Request, StatusCode};
use serde_json::{json, Value};

const CALLBACK: &str = "http://127.0.0.1:4321/callback";
const RESOURCE: &str = "https://cal.example/mcp";
const VERIFIER: &str = "abcdefghijklmnopqrstuvwxyz0123456789ABCDEFGH";

pub(super) fn environment() -> crate::test_support::EnvGuard {
    let guard = crate::test_support::EnvGuard::new();
    std::env::set_var("CALRS_BASE_URL", "https://cal.example");
    guard
}

async fn json_body(response: Response) -> Value {
    serde_json::from_str(&body_string(response).await).unwrap()
}

async fn register_client(app: &Router) -> String {
    let response = app.clone().oneshot(Request::builder().method("POST").uri("/oauth/register")
        .header("content-type", "application/json")
        .body(Body::from(json!({"client_name": "Test <app>", "redirect_uris": [CALLBACK], "token_endpoint_auth_method": "none"}).to_string())).unwrap()).await.unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
    json_body(response).await["client_id"]
        .as_str()
        .unwrap()
        .to_string()
}

fn authorization_uri(client: &str) -> String {
    format!("/oauth/authorize?client_id={client}&redirect_uri={}&response_type=code&code_challenge={}&code_challenge_method=S256&resource={}&state=opaque-state", urlencoding::encode(CALLBACK), crate::oauth::hash(VERIFIER), urlencoding::encode(RESOURCE))
}

fn request_cookie(response: &Response) -> String {
    response
        .headers()
        .get_all("set-cookie")
        .iter()
        .filter_map(|v| v.to_str().ok())
        .find_map(|v| {
            v.strip_prefix("__Host-calrs_oauth=")
                .map(|s| s.split(';').next().unwrap().to_string())
        })
        .unwrap()
}

fn consent_request(session: &str, request: &str, decision: &str, csrf: &str) -> Request<Body> {
    Request::builder().method("POST").uri("/oauth/consent")
        .header("content-type", "application/x-www-form-urlencoded")
        .header("cookie", format!("__Host-calrs_session={session}; __Host-calrs_oauth={request}; __Host-calrs_csrf=csrf"))
        .body(Body::from(format!("_csrf={csrf}&request_id={request}&decision={decision}"))).unwrap()
}

async fn begin_consent(app: &Router, client: &str, session: &str) -> String {
    let response = app
        .clone()
        .oneshot(get(&authorization_uri(client)))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    let request = request_cookie(&response);
    let page = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/oauth/consent")
                .header(
                    "cookie",
                    format!("__Host-calrs_session={session}; __Host-calrs_oauth={request}"),
                )
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(page.status(), StatusCode::OK);
    let html = body_string(page).await;
    assert!(html.contains("Test &lt;app&gt;"));
    assert!(html.contains("127.0.0.1:4321"));
    request
}

pub(super) async fn issue_tokens(app: &Router, session: &str) -> (String, Value) {
    let client = register_client(app).await;
    let request = begin_consent(app, &client, session).await;
    let response = app
        .clone()
        .oneshot(consent_request(session, &request, "allow", "csrf"))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    let url = reqwest::Url::parse(response.headers()["location"].to_str().unwrap()).unwrap();
    assert!(url
        .query_pairs()
        .any(|(k, v)| k == "state" && v == "opaque-state"));
    let code = url
        .query_pairs()
        .find(|(k, _)| k == "code")
        .unwrap()
        .1
        .into_owned();
    let response = exchange(app, &client, &code, VERIFIER, RESOURCE).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()["cache-control"], "no-store");
    (client, json_body(response).await)
}

async fn exchange(
    app: &Router,
    client: &str,
    code: &str,
    verifier: &str,
    resource: &str,
) -> Response {
    app.clone().oneshot(post_form_unauthed("/oauth/token", "", &format!("grant_type=authorization_code&client_id={client}&code={code}&code_verifier={verifier}&redirect_uri={}&resource={}",urlencoding::encode(CALLBACK),urlencoding::encode(resource)))).await.unwrap()
}

async fn refresh(app: &Router, client: &str, token: &str) -> Response {
    app.clone()
        .oneshot(post_form_unauthed(
            "/oauth/token",
            "",
            &format!(
                "grant_type=refresh_token&client_id={client}&refresh_token={token}&resource={}",
                urlencoding::encode(RESOURCE)
            ),
        ))
        .await
        .unwrap()
}

async fn accepted(pool: &SqlitePool, token: &str) -> bool {
    let mut headers = HeaderMap::new();
    headers.insert("authorization", format!("Bearer {token}").parse().unwrap());
    crate::oauth::bearer_user(pool, &headers, RESOURCE)
        .await
        .unwrap()
        .is_some()
}

#[tokio::test]
async fn oauth_discovery_and_registration_validation() {
    let _lock = crate::test_support::ENV_LOCK.lock().await;
    let _env = environment();
    let (app, _, _, _) = setup_test_app().await;
    let meta = json_body(
        app.clone()
            .oneshot(get("/.well-known/oauth-authorization-server"))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(meta["issuer"], "https://cal.example");
    assert_eq!(meta["code_challenge_methods_supported"], json!(["S256"]));
    let meta = json_body(
        app.clone()
            .oneshot(get("/.well-known/oauth-protected-resource/mcp"))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(meta["resource"], RESOURCE);
    for uri in [
        "https://example.com/#fragment",
        "http://evil.example/cb",
        "javascript:alert(1)",
        "https://user:pass@example.com/cb",
        "https://example.com/?code=bad",
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/oauth/register")
                    .header("content-type", "application/json")
                    .body(Body::from(json!({"redirect_uris":[uri]}).to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{uri}");
    }
}

#[tokio::test]
async fn oauth_pkce_client_resource_redirect_and_code_replay() {
    let _lock = crate::test_support::ENV_LOCK.lock().await;
    let _env = environment();
    let (app, pool, session, _) = setup_test_app().await;
    let client = register_client(&app).await;
    for uri in [
        authorization_uri(&client).replace("S256", "plain"),
        authorization_uri(&client).replace("cal.example", "evil.example"),
        authorization_uri(&client).replace("4321", "4322"),
    ] {
        let response = app.clone().oneshot(get(&uri)).await.unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert!(response.headers().get("location").is_none());
    }
    let request = begin_consent(&app, &client, &session).await;
    let response = app
        .clone()
        .oneshot(consent_request(&session, &request, "allow", "csrf"))
        .await
        .unwrap();
    let url = reqwest::Url::parse(response.headers()["location"].to_str().unwrap()).unwrap();
    let code = url
        .query_pairs()
        .find(|(k, _)| k == "code")
        .unwrap()
        .1
        .into_owned();
    for (client_id, verifier, resource) in [
        (client.as_str(), "wrong", RESOURCE),
        ("other-client", VERIFIER, RESOURCE),
        (client.as_str(), VERIFIER, "https://evil.example/mcp"),
    ] {
        assert_eq!(
            exchange(&app, client_id, &code, verifier, resource)
                .await
                .status(),
            StatusCode::BAD_REQUEST
        );
    }
    let response = exchange(&app, &client, &code, VERIFIER, RESOURCE).await;
    assert_eq!(response.status(), StatusCode::OK);
    let tokens = json_body(response).await;
    let access = tokens["access_token"].as_str().unwrap();
    assert!(accepted(&pool, access).await);
    assert!(!accepted(&pool, &session).await);
    assert_eq!(
        exchange(&app, &client, &code, VERIFIER, RESOURCE)
            .await
            .status(),
        StatusCode::BAD_REQUEST
    );
    assert!(!accepted(&pool, access).await);
}

#[tokio::test]
async fn oauth_consent_requires_login_csrf_and_displayed_user_and_can_deny() {
    let _lock = crate::test_support::ENV_LOCK.lock().await;
    let _env = environment();
    let (app, pool, session, _) = setup_test_app().await;
    let client = register_client(&app).await;
    let response = app
        .clone()
        .oneshot(get(&authorization_uri(&client)))
        .await
        .unwrap();
    let request = request_cookie(&response);
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/oauth/consent")
                .header("cookie", format!("__Host-calrs_oauth={request}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.headers()["location"], "/auth/login");
    // An existing login returns to consent instead of losing the flow.
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/auth/login")
                .header(
                    "cookie",
                    format!("__Host-calrs_oauth={request}; __Host-calrs_session={session}"),
                )
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.headers()["location"], "/oauth/consent");
    let request = begin_consent(&app, &client, &session).await;
    assert_eq!(
        app.clone()
            .oneshot(consent_request(&session, &request, "allow", "bad"))
            .await
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN
    );
    let (_, other) = insert_test_user_with_session(&pool, "Other", "other@test.com", "other").await;
    assert_eq!(
        app.clone()
            .oneshot(consent_request(&other, &request, "allow", "csrf"))
            .await
            .unwrap()
            .status(),
        StatusCode::BAD_REQUEST
    );
    let response = app
        .clone()
        .oneshot(consent_request(&session, &request, "deny", "csrf"))
        .await
        .unwrap();
    assert!(response.headers()["location"]
        .to_str()
        .unwrap()
        .contains("error=access_denied"));
    let grants: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM oauth_grants")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(grants, 0);
    assert_eq!(
        app.clone()
            .oneshot(consent_request(&session, &request, "allow", "csrf"))
            .await
            .unwrap()
            .status(),
        StatusCode::BAD_REQUEST
    );
}

#[tokio::test]
async fn oauth_rotation_revocation_expiry_and_disabled_users() {
    let _lock = crate::test_support::ENV_LOCK.lock().await;
    let _env = environment();
    let (app, pool, session, _) = setup_test_app().await;
    let (client, tokens) = issue_tokens(&app, &session).await;
    let refresh_token = tokens["refresh_token"].as_str().unwrap();
    let response = refresh(&app, &client, refresh_token).await;
    assert_eq!(response.status(), StatusCode::OK);
    let rotated = json_body(response).await;
    let access = rotated["access_token"].as_str().unwrap();
    assert!(accepted(&pool, access).await);
    assert_eq!(
        refresh(&app, &client, refresh_token).await.status(),
        StatusCode::BAD_REQUEST
    );
    assert!(!accepted(&pool, access).await);
    assert!(!accepted(&pool, tokens["access_token"].as_str().unwrap()).await);

    let (client, tokens) = issue_tokens(&app, &session).await;
    let access = tokens["access_token"].as_str().unwrap();
    sqlx::query("UPDATE users SET enabled = 0")
        .execute(&pool)
        .await
        .unwrap();
    assert!(!accepted(&pool, access).await);
    assert_eq!(
        refresh(&app, &client, tokens["refresh_token"].as_str().unwrap())
            .await
            .status(),
        StatusCode::BAD_REQUEST
    );
    sqlx::query("UPDATE users SET enabled = 1")
        .execute(&pool)
        .await
        .unwrap();
    assert!(accepted(&pool, access).await);
    let response = app
        .clone()
        .oneshot(post_form_unauthed(
            "/oauth/revoke",
            "",
            &format!("client_id={client}&token={access}"),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert!(!accepted(&pool, access).await);

    let (_, tokens) = issue_tokens(&app, &session).await;
    let access = tokens["access_token"].as_str().unwrap();
    sqlx::query("UPDATE oauth_credentials SET expires_at = 0 WHERE kind = 'access'")
        .execute(&pool)
        .await
        .unwrap();
    assert!(!accepted(&pool, access).await);
    let stored: Vec<String> = sqlx::query_scalar("SELECT hash FROM oauth_credentials")
        .fetch_all(&pool)
        .await
        .unwrap();
    assert!(!stored
        .iter()
        .any(|h| h == access || h == tokens["refresh_token"].as_str().unwrap()));
    sqlx::query("UPDATE oauth_grants SET expires_at = 0")
        .execute(&pool)
        .await
        .unwrap();
    crate::oauth::cleanup(&pool).await.unwrap();
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM oauth_credentials")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
}

#[tokio::test]
async fn oauth_refresh_scopes_are_sets_and_narrow_only_the_new_access_token() {
    let _lock = crate::test_support::ENV_LOCK.lock().await;
    let _env = environment();
    let (app, pool, session, _) = setup_test_app().await;
    let (client, original) = issue_tokens(&app, &session).await;
    let mut current = original.clone();
    for requested in [
        "invites:write calendar:read event-types:read",
        "calendar:read calendar:read",
    ] {
        let response = app.clone().oneshot(post_form_unauthed("/oauth/token", "", &format!(
            "grant_type=refresh_token&client_id={client}&refresh_token={}&resource={}&scope={}",
            current["refresh_token"].as_str().unwrap(), urlencoding::encode(RESOURCE), urlencoding::encode(requested)
        ))).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        current = json_body(response).await;
    }
    assert_eq!(current["scope"], "calendar:read");
    let mut headers = HeaderMap::new();
    headers.insert(
        "authorization",
        format!("Bearer {}", current["access_token"].as_str().unwrap())
            .parse()
            .unwrap(),
    );
    let (_, actual_scope) = crate::oauth::bearer_user(&pool, &headers, RESOURCE)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(actual_scope, "calendar:read");
    // Exercise enforcement at the protected resource, not just the token JSON.
    let response = app.clone().oneshot(Request::builder().method("POST").uri("/mcp")
        .header("authorization", headers["authorization"].clone())
        .header("content-type", "application/json")
        .header("accept", "application/json, text/event-stream")
        .body(Body::from(json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"create_invite_link","arguments":{"event_type_id":"irrelevant"}}}).to_string())).unwrap()).await.unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    headers.insert(
        "authorization",
        format!("Bearer {}", original["access_token"].as_str().unwrap())
            .parse()
            .unwrap(),
    );
    let (_, actual_scope) = crate::oauth::bearer_user(&pool, &headers, RESOURCE)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(actual_scope, original["scope"].as_str().unwrap());
    for requested in ["", "  ", "calendar:read admin:write"] {
        let response = app.clone().oneshot(post_form_unauthed("/oauth/token", "", &format!(
            "grant_type=refresh_token&client_id={client}&refresh_token={}&resource={}&scope={}",
            current["refresh_token"].as_str().unwrap(), urlencoding::encode(RESOURCE), urlencoding::encode(requested)
        ))).await.unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert_eq!(json_body(response).await["error"], "invalid_scope");
    }
    let response = refresh(&app, &client, current["refresh_token"].as_str().unwrap()).await;
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "invalid scope must not consume the refresh token"
    );
    assert_eq!(json_body(response).await["scope"], original["scope"]);
    let granted: String = sqlx::query_scalar("SELECT scope FROM oauth_grants WHERE client_id = ?")
        .bind(&client)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(granted, original["scope"].as_str().unwrap());
}

#[tokio::test]
async fn oauth_concurrent_refresh_has_one_winner_and_revokes_the_family() {
    let _lock = crate::test_support::ENV_LOCK.lock().await;
    let _env = environment();
    let (app, pool, session, _) = setup_test_app().await;
    let (client, tokens) = issue_tokens(&app, &session).await;
    let original = tokens["refresh_token"].as_str().unwrap();
    let (a, b) = tokio::join!(
        refresh(&app, &client, original),
        refresh(&app, &client, original)
    );
    let (winner, loser) = if a.status() == StatusCode::OK {
        (a, b)
    } else {
        (b, a)
    };
    assert_eq!(winner.status(), StatusCode::OK);
    assert_eq!(loser.status(), StatusCode::BAD_REQUEST);
    let winner = json_body(winner).await;
    assert!(!accepted(&pool, winner["access_token"].as_str().unwrap()).await);
    assert_eq!(
        refresh(&app, &client, winner["refresh_token"].as_str().unwrap())
            .await
            .status(),
        StatusCode::BAD_REQUEST
    );
}

#[tokio::test]
async fn oauth_password_login_resumes_and_expired_consent_is_rejected() {
    let _lock = crate::test_support::ENV_LOCK.lock().await;
    let _env = environment();
    let (app, pool, session, _) = setup_test_app().await;
    let password = crate::auth::hash_password("test-password").unwrap();
    sqlx::query("UPDATE users SET password_hash = ? WHERE email = 'test@example.com'")
        .bind(password)
        .execute(&pool)
        .await
        .unwrap();
    let client = register_client(&app).await;
    let response = app
        .clone()
        .oneshot(get(&authorization_uri(&client)))
        .await
        .unwrap();
    let request = request_cookie(&response);
    let mut login = post_form_unauthed(
        "/auth/login",
        "csrf",
        "_csrf=csrf&email=test%40example.com&password=test-password",
    );
    login.headers_mut().insert(
        "cookie",
        format!("__Host-calrs_csrf=csrf; __Host-calrs_oauth={request}")
            .parse()
            .unwrap(),
    );
    let response = app.clone().oneshot(login).await.unwrap();
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert_eq!(response.headers()["location"], "/oauth/consent");
    assert!(response
        .headers()
        .get_all("set-cookie")
        .iter()
        .any(|v| v.to_str().unwrap().starts_with("__Host-calrs_session=")));
    let request = begin_consent(&app, &client, &session).await;
    sqlx::query("UPDATE oauth_requests SET expires_at = 0")
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        app.clone()
            .oneshot(consent_request(&session, &request, "allow", "csrf"))
            .await
            .unwrap()
            .status(),
        StatusCode::BAD_REQUEST
    );
    let grants: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM oauth_grants")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(grants, 0);
}
