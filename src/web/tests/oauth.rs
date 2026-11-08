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
async fn oauth_loopback_port_changes_preserve_token_redirect_binding() {
    let _lock = crate::test_support::ENV_LOCK.lock().await;
    let _env = environment();
    let (app, _, session, _) = setup_test_app().await;
    let client = register_client(&app).await;
    let response = app
        .clone()
        .oneshot(get(&authorization_uri(&client).replace("4321", "52319")))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    let request = request_cookie(&response);
    let response = app
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
    assert_eq!(response.status(), StatusCode::OK);
    let response = app
        .clone()
        .oneshot(consent_request(&session, &request, "allow", "csrf"))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    let url = reqwest::Url::parse(response.headers()["location"].to_str().unwrap()).unwrap();
    assert_eq!(url.port(), Some(52319));
    let code = url
        .query_pairs()
        .find(|(k, _)| k == "code")
        .unwrap()
        .1
        .into_owned();
    // The registered port must not redeem a code issued for the new port.
    let response = exchange(&app, &client, &code, VERIFIER, RESOURCE).await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(json_body(response).await["error"], "invalid_grant");
    let response = app.clone().oneshot(post_form_unauthed("/oauth/token", "", &format!(
        "grant_type=authorization_code&client_id={client}&code={code}&code_verifier={VERIFIER}&redirect_uri={}&resource={}",
        urlencoding::encode("http://127.0.0.1:52319/callback"), urlencoding::encode(RESOURCE),
    ))).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
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
        authorization_uri(&client).replace("callback", "other-path"),
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

#[tokio::test]
async fn oauth_tokens_follow_the_mfa_policy_of_the_consenting_session() {
    let _lock = crate::test_support::ENV_LOCK.lock().await;
    let _env = environment();
    let (app, pool, session, _) = setup_test_app().await;
    let (client, tokens) = issue_tokens(&app, &session).await;
    let access = tokens["access_token"].as_str().unwrap();
    let refresh_token = tokens["refresh_token"].as_str().unwrap();
    assert!(accepted(&pool, access).await);

    // The admin now requires MFA: a grant made from a session that never
    // passed it stops working, exactly like that session.
    sqlx::query("UPDATE auth_config SET mfa_required = 1")
        .execute(&pool)
        .await
        .unwrap();
    assert!(!accepted(&pool, access).await);
    assert_eq!(
        refresh(&app, &client, refresh_token).await.status(),
        StatusCode::BAD_REQUEST
    );
    let response = app
        .clone()
        .oneshot(get(&authorization_uri(&client)))
        .await
        .unwrap();
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
    assert_eq!(page.status(), StatusCode::SEE_OTHER);
    assert_eq!(page.headers()["location"], "/auth/login");

    // Once the user has enrolled and the session passed MFA, consent works
    // and its tokens satisfy the policy.
    let user_id: String = sqlx::query_scalar("SELECT user_id FROM sessions WHERE id = ?")
        .bind(&session)
        .fetch_one(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO user_mfa (user_id, secret_enc, last_step) VALUES (?, 'x', 0)")
        .bind(&user_id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE sessions SET mfa_verified = 1 WHERE id = ?")
        .bind(&session)
        .execute(&pool)
        .await
        .unwrap();
    assert!(!accepted(&pool, access).await);
    let (client, tokens) = issue_tokens(&app, &session).await;
    assert!(accepted(&pool, tokens["access_token"].as_str().unwrap()).await);
    assert_eq!(
        refresh(&app, &client, tokens["refresh_token"].as_str().unwrap())
            .await
            .status(),
        StatusCode::OK
    );
}

#[tokio::test]
async fn oauth_grants_are_revoked_with_the_sessions() {
    let _lock = crate::test_support::ENV_LOCK.lock().await;
    let _env = environment();
    let (app, pool, session, _) = setup_test_app().await;
    let user_id: String = sqlx::query_scalar("SELECT user_id FROM sessions WHERE id = ?")
        .bind(&session)
        .fetch_one(&pool)
        .await
        .unwrap();
    let (client, tokens) = issue_tokens(&app, &session).await;
    let access = tokens["access_token"].as_str().unwrap();
    // A pending authorization of the same user goes too.
    let response = app
        .clone()
        .oneshot(get(&authorization_uri(&client)))
        .await
        .unwrap();
    let request = request_cookie(&response);
    sqlx::query("UPDATE oauth_requests SET user_id = ?")
        .bind(&user_id)
        .execute(&pool)
        .await
        .unwrap();

    // Another admin, so the test user is not the last one.
    sqlx::query("INSERT INTO users (id, email, name, role, auth_provider, username, enabled) VALUES ('other-admin', 'other@example.com', 'Other', 'admin', 'local', 'other', 1)")
        .execute(&pool)
        .await
        .unwrap();
    // Suspending revokes everything: re-enabling the account must not bring
    // the tokens back (every revoke_access caller: suspension, password
    // reset, MFA enrollment, disable or reset, OIDC linking).
    assert_eq!(
        crate::auth::set_user_enabled(&pool, &user_id, false)
            .await
            .unwrap(),
        crate::auth::AdminChange::Changed
    );
    crate::auth::set_user_enabled(&pool, &user_id, true)
        .await
        .unwrap();
    assert!(!accepted(&pool, access).await);
    assert_eq!(
        refresh(&app, &client, tokens["refresh_token"].as_str().unwrap())
            .await
            .status(),
        StatusCode::BAD_REQUEST
    );
    let left: i64 = sqlx::query_scalar(
        "SELECT (SELECT COUNT(*) FROM oauth_grants) + (SELECT COUNT(*) FROM oauth_credentials)
                + (SELECT COUNT(*) FROM oauth_requests WHERE id = ?)",
    )
    .bind(crate::oauth::hash(&request))
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(left, 0);
}

#[tokio::test]
async fn oauth_consent_page_cannot_be_framed() {
    let _lock = crate::test_support::ENV_LOCK.lock().await;
    let _env = environment();
    let (app, _, session, _) = setup_test_app().await;
    let client = register_client(&app).await;
    let response = app
        .clone()
        .oneshot(get(&authorization_uri(&client)))
        .await
        .unwrap();
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
    assert_eq!(page.headers()["x-frame-options"], "DENY");
    let csp = page.headers()["content-security-policy"].to_str().unwrap();
    assert!(csp.contains("frame-ancestors 'none'"), "{csp}");
    assert!(!csp.contains("frame-ancestors 'self'"), "{csp}");
}

#[tokio::test]
async fn oauth_registration_refuses_spoofing_names_and_expires_unused_clients() {
    let _lock = crate::test_support::ENV_LOCK.lock().await;
    let _env = environment();
    let (app, pool, session, _) = setup_test_app().await;
    for name in ["Claude\u{202e}edualC", "Cl\u{200b}aude", "\u{2066}x"] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/oauth/register")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        json!({"client_name": name, "redirect_uris": [CALLBACK]}).to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{name:?}");
    }
    let unused = register_client(&app).await;
    let (used, _) = issue_tokens(&app, &session).await;
    sqlx::query("UPDATE oauth_clients SET created_at = created_at - 2 * 86400")
        .execute(&pool)
        .await
        .unwrap();
    crate::oauth::cleanup(&pool).await.unwrap();
    let left: Vec<String> = sqlx::query_scalar("SELECT id FROM oauth_clients")
        .fetch_all(&pool)
        .await
        .unwrap();
    assert_eq!(left, vec![used]);
    assert!(!left.contains(&unused));
}

async fn grant_ids(pool: &SqlitePool, user_id: &str) -> Vec<String> {
    sqlx::query_scalar("SELECT id FROM oauth_grants WHERE user_id = ? ORDER BY id")
        .bind(user_id)
        .fetch_all(pool)
        .await
        .unwrap()
}

#[tokio::test]
async fn connected_apps_lists_and_revokes_only_the_users_own_grants() {
    let _lock = crate::test_support::ENV_LOCK.lock().await;
    let _env = environment();
    let (app, pool, session, _) = setup_test_app().await;
    let owner: String = sqlx::query_scalar("SELECT user_id FROM sessions WHERE id = ?")
        .bind(&session)
        .fetch_one(&pool)
        .await
        .unwrap();
    let (other, other_session) =
        insert_test_user_with_session(&pool, "Other", "other@example.com", "other").await;

    // Empty state first.
    let page = app
        .clone()
        .oneshot(get_authed("/dashboard/settings/apps", &other_session))
        .await
        .unwrap();
    assert_eq!(page.status(), StatusCode::OK);
    let html = body_string(page).await;
    assert!(html.contains("No application is connected."), "{html}");

    let (_, tokens) = issue_tokens(&app, &session).await;
    let access = tokens["access_token"].as_str().unwrap();
    let (_, other_tokens) = issue_tokens(&app, &other_session).await;
    let other_access = other_tokens["access_token"].as_str().unwrap();
    let mine = grant_ids(&pool, &owner).await;
    let theirs = grant_ids(&pool, &other).await;
    assert_eq!((mine.len(), theirs.len()), (1, 1));

    // The code exchange counts as a use; clear it to see the "never" state,
    // then a bearer call records one again.
    sqlx::query("UPDATE oauth_grants SET last_used_at = NULL")
        .execute(&pool)
        .await
        .unwrap();
    let html = body_string(
        app.clone()
            .oneshot(get_authed("/dashboard/settings/apps", &session))
            .await
            .unwrap(),
    )
    .await;
    assert!(html.contains("Test &lt;app&gt;"));
    assert!(html.contains("Approved on "));
    assert!(html.contains("Not used yet"));
    assert!(html.contains("Read your calendar busy times"));
    assert!(html.contains(&mine[0]));
    assert!(!html.contains(&theirs[0]), "another user's grant is listed");
    assert!(accepted(&pool, access).await);
    let html = body_string(
        app.clone()
            .oneshot(get_authed("/dashboard/settings/apps", &session))
            .await
            .unwrap(),
    )
    .await;
    assert!(html.contains("Last used on "), "{html}");

    // Revoking the other user's grant id from this session deletes nothing.
    let response = app
        .clone()
        .oneshot(post_form(
            &format!("/dashboard/settings/apps/{}/revoke", theirs[0]),
            &session,
            "csrf",
            "_csrf=csrf",
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert_eq!(grant_ids(&pool, &other).await, theirs);
    assert!(accepted(&pool, other_access).await);

    // Without CSRF nothing happens either.
    let response = app
        .clone()
        .oneshot(post_form(
            &format!("/dashboard/settings/apps/{}/revoke", mine[0]),
            &session,
            "csrf",
            "_csrf=wrong",
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    assert_eq!(grant_ids(&pool, &owner).await, mine);

    // The owner's revoke deletes the grant with its tokens.
    let response = app
        .clone()
        .oneshot(post_form(
            &format!("/dashboard/settings/apps/{}/revoke", mine[0]),
            &session,
            "csrf",
            "_csrf=csrf",
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert_eq!(
        response.headers()["location"],
        "/dashboard/settings/apps?revoked=1"
    );
    assert!(grant_ids(&pool, &owner).await.is_empty());
    assert!(!accepted(&pool, access).await);
    let left: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM oauth_credentials WHERE grant_id = ?")
        .bind(&mine[0])
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(left, 0);
    assert!(accepted(&pool, other_access).await);
}

#[tokio::test]
async fn admin_sees_and_revokes_a_users_connected_apps() {
    let _lock = crate::test_support::ENV_LOCK.lock().await;
    let _env = environment();
    let (app, pool, admin_session, _) = setup_test_app().await;
    let (user, user_session) =
        insert_test_user_with_session(&pool, "Member", "member@example.com", "member").await;
    let (_, tokens) = issue_tokens(&app, &user_session).await;
    issue_tokens(&app, &user_session).await;
    let access = tokens["access_token"].as_str().unwrap();

    let html = body_string(
        app.clone()
            .oneshot(get_authed("/dashboard/admin", &admin_session))
            .await
            .unwrap(),
    )
    .await;
    assert!(html.contains(&format!("/dashboard/admin/users/{user}/revoke-apps")));
    assert!(html.contains("Revoke connected apps (2)"), "count shown");

    // A non-admin cannot use the admin action, even on themselves.
    let response = app
        .clone()
        .oneshot(post_form(
            &format!("/dashboard/admin/users/{user}/revoke-apps"),
            &user_session,
            "csrf",
            "_csrf=csrf",
        ))
        .await
        .unwrap();
    assert_ne!(response.status(), StatusCode::SEE_OTHER);
    assert_eq!(grant_ids(&pool, &user).await.len(), 2);

    let response = app
        .clone()
        .oneshot(post_form(
            &format!("/dashboard/admin/users/{user}/revoke-apps"),
            &admin_session,
            "csrf",
            "_csrf=csrf",
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert!(grant_ids(&pool, &user).await.is_empty());
    assert!(!accepted(&pool, access).await);
    let html = body_string(
        app.clone()
            .oneshot(get_authed("/dashboard/admin", &admin_session))
            .await
            .unwrap(),
    )
    .await;
    assert!(!html.contains(&format!("/dashboard/admin/users/{user}/revoke-apps")));
}

#[tokio::test]
async fn oauth_endpoints_are_rate_limited_per_ip() {
    let _lock = crate::test_support::ENV_LOCK.lock().await;
    let _env = environment();
    let (app, _, _, _) = setup_test_app().await;
    let from = |ip: &str, mut request: Request<Body>| {
        request
            .headers_mut()
            .insert("x-forwarded-for", ip.parse().unwrap());
        request
    };
    for (uri, body, budget) in [
        (
            "/oauth/token",
            "grant_type=refresh_token&client_id=x&refresh_token=y&resource=https%3A%2F%2Fcal.example%2Fmcp",
            60,
        ),
        ("/oauth/revoke", "client_id=x&token=y", 60),
    ] {
        for _ in 0..budget {
            let status = app
                .clone()
                .oneshot(from("203.0.113.7", post_form_unauthed(uri, "", body)))
                .await
                .unwrap()
                .status();
            assert_ne!(status, StatusCode::TOO_MANY_REQUESTS, "{uri}");
        }
        let response = app
            .clone()
            .oneshot(from("203.0.113.7", post_form_unauthed(uri, "", body)))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS, "{uri}");
        // Another address keeps its own budget.
        let status = app
            .clone()
            .oneshot(from("198.51.100.9", post_form_unauthed(uri, "", body)))
            .await
            .unwrap()
            .status();
        assert_ne!(status, StatusCode::TOO_MANY_REQUESTS, "{uri}");
    }
    let register = || {
        Request::builder()
            .method("POST")
            .uri("/oauth/register")
            .header("content-type", "application/json")
            .body(Body::from(
                json!({"client_name": "Flood", "redirect_uris": [CALLBACK]}).to_string(),
            ))
            .unwrap()
    };
    for _ in 0..10 {
        let response = app
            .clone()
            .oneshot(from("203.0.113.7", register()))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::CREATED);
    }
    let response = app
        .clone()
        .oneshot(from("203.0.113.7", register()))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
}

#[tokio::test]
async fn oauth_registration_requires_ascii_normalized_redirects_and_consent_shows_the_host() {
    let _lock = crate::test_support::ENV_LOCK.lock().await;
    let _env = environment();
    let (app, _, session, _) = setup_test_app().await;
    for uri in [
        "https://ex\u{0430}mple.com/cb",
        "https://Example.com/cb",
        "https://example.com",
        "https://example.com/a/../cb",
        "https://example.com:443/cb",
        "https://example.com/cb\n",
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/oauth/register")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        json!({"client_name": "x", "redirect_uris": [uri]}).to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{uri:?}");
    }
    let client = register_client(&app).await;
    let response = app
        .clone()
        .oneshot(get(&authorization_uri(&client)))
        .await
        .unwrap();
    let request = request_cookie(&response);
    let html = body_string(
        app.clone()
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
            .unwrap(),
    )
    .await;
    assert!(
        html.contains(r#"<p class="oauth-consent-host"><strong>127.0.0.1</strong></p>"#),
        "{html}"
    );
}

async fn register_from(app: &Router, ip: &str) -> StatusCode {
    let mut request = Request::builder()
        .method("POST")
        .uri("/oauth/register")
        .header("content-type", "application/json")
        .body(Body::from(
            json!({"client_name": "Cap", "redirect_uris": [CALLBACK]}).to_string(),
        ))
        .unwrap();
    request
        .headers_mut()
        .insert("x-forwarded-for", ip.parse().unwrap());
    app.clone().oneshot(request).await.unwrap().status()
}

#[tokio::test]
async fn oauth_registration_at_the_cap_reclaims_the_oldest_unused_client() {
    let _lock = crate::test_support::ENV_LOCK.lock().await;
    let _env = environment();
    let (app, pool, session, _) = setup_test_app().await;
    let (used, _) = issue_tokens(&app, &session).await;
    sqlx::query("UPDATE oauth_clients SET created_at = 0")
        .execute(&pool)
        .await
        .unwrap();
    // Fill up to the cap with unused clients, newer than the used one.
    sqlx::query(
        "WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < 9999)
         INSERT INTO oauth_clients (id, name, redirect_uris, created_at)
         SELECT 'filler-' || i, 'Filler', '[]', 1000 + i FROM n",
    )
    .execute(&pool)
    .await
    .unwrap();
    let total = || async {
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM oauth_clients")
            .fetch_one(&pool)
            .await
            .unwrap()
    };
    assert_eq!(total().await, 10_000);
    assert_eq!(
        register_from(&app, "2001:db8::1").await,
        StatusCode::CREATED
    );
    assert_eq!(total().await, 10_000);
    let oldest_unused: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM oauth_clients WHERE id = 'filler-1')")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(!oldest_unused, "the oldest unused client made room");
    let kept: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM oauth_clients WHERE id = ?)")
        .bind(&used)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(kept, "a client with a grant is never reclaimed");

    // When every client is in use, registration is refused.
    sqlx::query(
        "INSERT INTO oauth_requests (id, client_id, parameters, expires_at)
         SELECT 'req-' || id, id, '{}', 9999999999 FROM oauth_clients
         WHERE NOT EXISTS (SELECT 1 FROM oauth_grants g WHERE g.client_id = oauth_clients.id)",
    )
    .execute(&pool)
    .await
    .unwrap();
    assert_eq!(
        register_from(&app, "2001:db8::2").await,
        StatusCode::SERVICE_UNAVAILABLE
    );
    assert_eq!(total().await, 10_000);
}

#[tokio::test]
async fn oauth_registration_limit_counts_an_ipv6_64_as_one_client() {
    let _lock = crate::test_support::ENV_LOCK.lock().await;
    let _env = environment();
    let (app, _, _, _) = setup_test_app().await;
    for i in 0..10 {
        let ip = format!("2001:db8:aa:bb::{:x}", i + 1);
        assert_eq!(register_from(&app, &ip).await, StatusCode::CREATED);
    }
    assert_eq!(
        register_from(&app, "2001:db8:aa:bb:ffff::9").await,
        StatusCode::TOO_MANY_REQUESTS
    );
    assert_eq!(
        register_from(&app, "2001:db8:aa:bc::1").await,
        StatusCode::CREATED
    );
}

#[tokio::test]
async fn oauth_authorize_has_a_looser_limit_than_registration() {
    let _lock = crate::test_support::ENV_LOCK.lock().await;
    let _env = environment();
    let (app, _, _, _) = setup_test_app().await;
    let client = register_client(&app).await;
    for _ in 0..60 {
        let status = app
            .clone()
            .oneshot(get(&authorization_uri(&client)))
            .await
            .unwrap()
            .status();
        assert_eq!(status, StatusCode::SEE_OTHER);
    }
    let status = app
        .clone()
        .oneshot(get(&authorization_uri(&client)))
        .await
        .unwrap()
        .status();
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
}
