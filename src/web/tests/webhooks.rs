//! Outgoing booking webhooks (#168): queueing, delivery, retries, scope,
//! the admin switch, and the dashboard page's authorization.

use super::*;
use crate::web::webhooks::{self as wh, Actor, Context, Event};

/// One request the test receiver got.
#[derive(Clone, Debug)]
struct Received {
    headers: axum::http::HeaderMap,
    body: Vec<u8>,
}

/// A local HTTP receiver answering every POST with `status`.
async fn spawn_receiver(
    status: u16,
) -> (
    String,
    Arc<std::sync::Mutex<Vec<Received>>>,
    Arc<std::sync::atomic::AtomicU16>,
) {
    let got: Arc<std::sync::Mutex<Vec<Received>>> = Arc::default();
    let code = Arc::new(std::sync::atomic::AtomicU16::new(status));
    let (got2, code2) = (got.clone(), code.clone());
    let app = Router::new().route(
        "/hook",
        post(
            move |headers: axum::http::HeaderMap, body: axum::body::Bytes| {
                let (got, code) = (got2.clone(), code2.clone());
                async move {
                    got.lock().unwrap().push(Received {
                        headers,
                        body: body.to_vec(),
                    });
                    let status = code.load(std::sync::atomic::Ordering::SeqCst);
                    (
                        axum::http::StatusCode::from_u16(status).unwrap(),
                        "receiver says no",
                    )
                }
            },
        ),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    (format!("http://127.0.0.1:{port}/hook"), got, code)
}

fn local() -> Vec<String> {
    vec!["127.0.0.1".to_string()]
}

const SECRET: &str = "0123456789abcdef-secret";

/// Insert an endpoint directly. `event_type_id` None = the user's personal
/// event types.
async fn insert_webhook(
    pool: &SqlitePool,
    user_id: &str,
    event_type_id: Option<&str>,
    url: &str,
) -> String {
    let id = uuid::Uuid::new_v4().to_string();
    let enc = crate::crypto::encrypt_value(&[0u8; 32], SECRET).unwrap();
    sqlx::query(
        "INSERT INTO webhooks (id, user_id, event_type_id, url, secret_enc) VALUES (?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(user_id)
    .bind(event_type_id)
    .bind(url)
    .bind(enc)
    .execute(pool)
    .await
    .unwrap();
    id
}

async fn test_user_id(pool: &SqlitePool) -> String {
    sqlx::query_scalar("SELECT id FROM users WHERE username = 'testuser'")
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn deliveries(pool: &SqlitePool) -> Vec<(String, String, String, i64)> {
    sqlx::query_as("SELECT event, payload, state, attempts FROM webhook_deliveries ORDER BY seq")
        .fetch_all(pool)
        .await
        .unwrap()
}

fn json(payload: &str) -> serde_json::Value {
    serde_json::from_str(payload).unwrap()
}

#[tokio::test]
async fn nothing_is_queued_without_a_webhook() {
    let (app, pool, _, _) = setup_test_app().await;
    let csrf = "csrf-wh-none";
    let response = app
        .oneshot(post_form_unauthed(
            "/u/testuser/test-meeting/book",
            csrf,
            &book_body(csrf),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let booked: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM bookings")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(booked, 1);
    assert!(deliveries(&pool).await.is_empty());
}

#[tokio::test]
async fn a_new_booking_queues_a_versioned_payload() {
    let (app, pool, _, et_id) = setup_test_app().await;
    let user_id = test_user_id(&pool).await;
    sqlx::query("UPDATE event_types SET timezone = 'Europe/Paris', location_type = 'link', location_value = 'https://meet.example.com/x' WHERE id = ?")
        .bind(&et_id)
        .execute(&pool)
        .await
        .unwrap();
    insert_webhook(&pool, &user_id, None, "https://93.184.215.14/hook").await;

    let csrf = "csrf-wh-new";
    let body = format!("{}&notes=Bring+the+slides&phone=", book_body(csrf));
    let response = app
        .oneshot(post_form_unauthed(
            "/u/testuser/test-meeting/book",
            csrf,
            &body,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);

    let rows = deliveries(&pool).await;
    assert_eq!(rows.len(), 1);
    let (event, payload, state, attempts) = &rows[0];
    assert_eq!(event, "booking.created");
    assert_eq!(state, "pending");
    assert_eq!(*attempts, 0);
    let v = json(payload);
    assert_eq!(v["version"], 1);
    assert_eq!(v["event"], "booking.created");
    assert_eq!(v["test"], false);
    assert!(!v["id"].as_str().unwrap().is_empty());
    assert_eq!(v["booking"]["status"], "confirmed");
    assert_eq!(v["booking"]["guest"]["email"], "guest@test.com");
    assert_eq!(v["booking"]["guest"]["notes"], "Bring the slides");
    assert_eq!(v["booking"]["hosts"][0]["email"], "test@example.com");
    assert_eq!(v["booking"]["location"]["type"], "link");
    assert_eq!(
        v["booking"]["location"]["value"],
        "https://meet.example.com/x"
    );
    assert_eq!(v["event_type"]["slug"], "test-meeting");
    assert_eq!(v["event_type"]["timezone"], "Europe/Paris");
    // Booked at 10:00 in the guest's UTC: stored and sent as a UTC instant.
    let start = v["booking"]["start"].as_str().unwrap();
    assert!(start.ends_with("T10:00:00Z"), "{start}");
    // Tokens never leave calrs.
    for secret in ["cancel_token", "reschedule_token", "confirm_token"] {
        let token: Option<String> =
            sqlx::query_scalar(&format!("SELECT {secret} FROM bookings LIMIT 1"))
                .fetch_one(&pool)
                .await
                .unwrap();
        if let Some(token) = token {
            assert!(!payload.contains(&token), "{secret} leaked");
        }
    }
}

#[tokio::test]
async fn delivery_is_signed_and_clears_the_queue() {
    let pool = setup_test_db().await;
    let (user_id, _, et_id) = seed_test_data(&pool).await;
    let (url, got, _) = spawn_receiver(200).await;
    let wid = insert_webhook(&pool, &user_id, Some(&et_id), &url).await;
    let (booking_id, _, _) = insert_test_booking(
        &pool,
        &et_id,
        "2030-01-07T10:00:00",
        "2030-01-07T10:30:00",
        "confirmed",
    )
    .await;
    wh::enqueue(&pool, &booking_id, Event::Created, Context::default()).await;

    let delivered = wh::deliver_due_with(&pool, &[0u8; 32], Utc::now(), &local()).await;
    assert_eq!(delivered, 1);
    assert!(deliveries(&pool).await.is_empty());

    let got = got.lock().unwrap().clone();
    assert_eq!(got.len(), 1);
    let req = &got[0];
    let signature = req.headers["x-calrs-signature"].to_str().unwrap();
    let expected = crate::web::meeting::sign_hmac_sha256(SECRET.as_bytes(), &req.body);
    assert_eq!(signature, format!("sha256={expected}"));
    assert_eq!(req.headers["x-calrs-event"], "booking.created");
    assert_eq!(req.headers["content-type"], "application/json");
    let v: serde_json::Value = serde_json::from_slice(&req.body).unwrap();
    assert_eq!(req.headers["x-calrs-delivery"].to_str().unwrap(), v["id"]);
    let ts = v["timestamp"].as_i64().unwrap();
    assert!((Utc::now().timestamp() - ts).abs() < 60);
    assert_eq!(v["booking"]["status"], "confirmed");

    let (status, error, success): (Option<i64>, Option<String>, Option<String>) = sqlx::query_as(
        "SELECT last_status, last_error, last_success_at FROM webhooks WHERE id = ?",
    )
    .bind(&wid)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(status, Some(200));
    assert!(error.is_none());
    assert!(success.is_some());
}

#[tokio::test]
async fn failures_back_off_then_give_up() {
    let pool = setup_test_db().await;
    let (user_id, _, et_id) = seed_test_data(&pool).await;
    let (url, got, _) = spawn_receiver(500).await;
    let wid = insert_webhook(&pool, &user_id, None, &url).await;
    let (booking_id, _, _) = insert_test_booking(
        &pool,
        &et_id,
        "2030-01-07T10:00:00",
        "2030-01-07T10:30:00",
        "confirmed",
    )
    .await;
    wh::enqueue(&pool, &booking_id, Event::Created, Context::default()).await;

    let mut now = Utc::now();
    assert_eq!(
        wh::deliver_due_with(&pool, &[0u8; 32], now, &local()).await,
        0
    );
    let rows = deliveries(&pool).await;
    assert_eq!(rows[0].2, "pending");
    assert_eq!(rows[0].3, 1);
    let (status, error): (Option<i64>, Option<String>) =
        sqlx::query_as("SELECT last_status, last_error FROM webhooks WHERE id = ?")
            .bind(&wid)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(status, Some(500));
    assert_eq!(error.as_deref(), Some("HTTP 500: receiver says no"));

    // Not due again straight away.
    wh::deliver_due_with(&pool, &[0u8; 32], now, &local()).await;
    assert_eq!(got.lock().unwrap().len(), 1);

    // Every retry, far enough apart, until the attempts run out.
    for _ in 1..wh::MAX_ATTEMPTS {
        now += chrono::Duration::hours(13);
        wh::deliver_due_with(&pool, &[0u8; 32], now, &local()).await;
    }
    assert_eq!(got.lock().unwrap().len() as i64, wh::MAX_ATTEMPTS);
    let rows = deliveries(&pool).await;
    assert_eq!(rows[0].2, "failed");
    assert_eq!(rows[0].3, wh::MAX_ATTEMPTS);
    now += chrono::Duration::days(1);
    wh::deliver_due_with(&pool, &[0u8; 32], now, &local()).await;
    assert_eq!(got.lock().unwrap().len() as i64, wh::MAX_ATTEMPTS);

    // Pruned a week later.
    sqlx::query("UPDATE webhook_deliveries SET created_at = '2000-01-01 00:00:00'")
        .execute(&pool)
        .await
        .unwrap();
    wh::prune(&pool, Utc::now()).await;
    assert!(deliveries(&pool).await.is_empty());
}

#[tokio::test]
async fn a_bookings_events_arrive_in_order() {
    let pool = setup_test_db().await;
    let (user_id, _, et_id) = seed_test_data(&pool).await;
    let (url, got, code) = spawn_receiver(503).await;
    insert_webhook(&pool, &user_id, None, &url).await;
    let (booking_id, _, _) = insert_test_booking(
        &pool,
        &et_id,
        "2030-01-07T10:00:00",
        "2030-01-07T10:30:00",
        "confirmed",
    )
    .await;
    wh::enqueue(&pool, &booking_id, Event::Created, Context::default()).await;
    sqlx::query("UPDATE bookings SET status = 'cancelled' WHERE id = ?")
        .bind(&booking_id)
        .execute(&pool)
        .await
        .unwrap();
    wh::enqueue(
        &pool,
        &booking_id,
        Event::Cancelled,
        Context::cancelled(Actor::Guest, Some("Ill".to_string())),
    )
    .await;

    // The first fails: the cancellation is held back behind it.
    let now = Utc::now();
    wh::deliver_due_with(&pool, &[0u8; 32], now, &local()).await;
    assert_eq!(got.lock().unwrap().len(), 1);

    code.store(200, std::sync::atomic::Ordering::SeqCst);
    let later = now + chrono::Duration::minutes(2);
    assert_eq!(
        wh::deliver_due_with(&pool, &[0u8; 32], later, &local()).await,
        2
    );
    let got = got.lock().unwrap().clone();
    let events: Vec<String> = got
        .iter()
        .map(|r| r.headers["x-calrs-event"].to_str().unwrap().to_string())
        .collect();
    assert_eq!(
        events,
        ["booking.created", "booking.created", "booking.cancelled"]
    );
    let last: serde_json::Value = serde_json::from_slice(&got[2].body).unwrap();
    assert_eq!(last["booking"]["status"], "cancelled");
    assert_eq!(last["booking"]["cancelled_by"], "guest");
    assert_eq!(last["booking"]["reason"], "Ill");
}

#[tokio::test]
async fn a_private_target_is_refused_at_delivery() {
    let pool = setup_test_db().await;
    let (user_id, _, et_id) = seed_test_data(&pool).await;
    let (url, got, _) = spawn_receiver(200).await;
    let wid = insert_webhook(&pool, &user_id, None, &url).await;
    let (booking_id, _, _) = insert_test_booking(
        &pool,
        &et_id,
        "2030-01-07T10:00:00",
        "2030-01-07T10:30:00",
        "confirmed",
    )
    .await;
    wh::enqueue(&pool, &booking_id, Event::Created, Context::default()).await;
    // No allowlist: 127.0.0.1 is private, and http is not https.
    assert_eq!(
        wh::deliver_due_with(&pool, &[0u8; 32], Utc::now(), &[]).await,
        0
    );
    assert!(got.lock().unwrap().is_empty());
    let error: Option<String> = sqlx::query_scalar("SELECT last_error FROM webhooks WHERE id = ?")
        .bind(&wid)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(error.unwrap().starts_with("refused"));
}

#[tokio::test]
async fn redirects_are_not_followed() {
    let pool = setup_test_db().await;
    let (user_id, _, et_id) = seed_test_data(&pool).await;
    let (url, got, _) = spawn_receiver(302).await;
    let wid = insert_webhook(&pool, &user_id, None, &url).await;
    let (booking_id, _, _) = insert_test_booking(
        &pool,
        &et_id,
        "2030-01-07T10:00:00",
        "2030-01-07T10:30:00",
        "confirmed",
    )
    .await;
    wh::enqueue(&pool, &booking_id, Event::Created, Context::default()).await;
    assert_eq!(
        wh::deliver_due_with(&pool, &[0u8; 32], Utc::now(), &local()).await,
        0
    );
    assert_eq!(got.lock().unwrap().len(), 1);
    let error: Option<String> = sqlx::query_scalar("SELECT last_error FROM webhooks WHERE id = ?")
        .bind(&wid)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(error.unwrap().contains("redirects are not followed"));
}

#[tokio::test]
async fn scope_and_admin_switch_decide_who_receives() {
    let pool = setup_test_db().await;
    let (admin_id, _, admin_et) = seed_test_data(&pool).await;
    let member = insert_role_user(&pool, "member@example.com", "user").await;
    let member_et = insert_personal_et(&pool, &member, "member-meeting").await;
    // The member's personal endpoint, and the admin's.
    insert_webhook(&pool, &member, None, "https://93.184.215.14/m").await;
    insert_webhook(&pool, &admin_id, None, "https://93.184.215.14/a").await;

    let (member_booking, _, _) = insert_test_booking(
        &pool,
        &member_et,
        "2030-01-07T10:00:00",
        "2030-01-07T10:30:00",
        "confirmed",
    )
    .await;
    // Switch off (the default): the member's endpoint gets nothing.
    wh::enqueue(&pool, &member_booking, Event::Created, Context::default()).await;
    assert!(deliveries(&pool).await.is_empty());

    sqlx::query("UPDATE auth_config SET webhooks_allow_all_users = 1 WHERE id = 'singleton'")
        .execute(&pool)
        .await
        .unwrap();
    wh::enqueue(&pool, &member_booking, Event::Created, Context::default()).await;
    let targets: Vec<String> = sqlx::query_scalar(
        "SELECT w.url FROM webhook_deliveries d JOIN webhooks w ON w.id = d.webhook_id",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    // Only the member's: the admin's personal endpoint covers the admin's
    // event types, not everyone's.
    assert_eq!(targets, ["https://93.184.215.14/m"]);

    // A personal endpoint does not cover team event types.
    let (_, team_et) =
        insert_team_with_et(&pool, "wh-team", "team-call", &[(&admin_id, "admin")]).await;
    let (team_booking, _, _) = insert_test_booking(
        &pool,
        &team_et,
        "2030-01-08T10:00:00",
        "2030-01-08T10:30:00",
        "confirmed",
    )
    .await;
    wh::enqueue(&pool, &team_booking, Event::Created, Context::default()).await;
    let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM webhook_deliveries")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(n, 1);
    let _ = admin_et;

    // Switching it back off stops what is already queued for the member.
    sqlx::query("UPDATE auth_config SET webhooks_allow_all_users = 0 WHERE id = 'singleton'")
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        wh::deliver_due_with(&pool, &[0u8; 32], Utc::now(), &local()).await,
        0
    );
    let attempts: i64 = sqlx::query_scalar("SELECT attempts FROM webhook_deliveries")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(attempts, 0);
}

#[tokio::test]
async fn reschedule_carries_the_previous_times() {
    let pool = setup_test_db().await;
    let (user_id, _, et_id) = seed_test_data(&pool).await;
    sqlx::query("UPDATE event_types SET timezone = 'Europe/Paris' WHERE id = ?")
        .bind(&et_id)
        .execute(&pool)
        .await
        .unwrap();
    insert_webhook(&pool, &user_id, None, "https://93.184.215.14/hook").await;
    let start = Utc::now() + Duration::days(10);
    let booking_id =
        insert_v1_booking(&pool, &et_id, "g@example.com", start, 30, "confirmed", &[]).await;
    // A legacy wall-clock time, as an older row would hold it (Paris, summer).
    wh::enqueue(
        &pool,
        &booking_id,
        Event::Rescheduled,
        Context::rescheduled("2026-07-01T14:00:00", "2026-07-01T14:30:00"),
    )
    .await;
    let rows = deliveries(&pool).await;
    let v = json(&rows[0].1);
    assert_eq!(v["event"], "booking.rescheduled");
    assert_eq!(v["booking"]["previous"]["start"], "2026-07-01T12:00:00Z");
    assert_eq!(v["booking"]["previous"]["end"], "2026-07-01T12:30:00Z");
    assert_eq!(v["booking"]["start"], utc_stamp(start));
}

#[tokio::test]
async fn guest_cancel_and_host_decline_are_queued() {
    let (app, pool, session, et_id) = setup_test_app().await;
    let user_id = test_user_id(&pool).await;
    insert_webhook(&pool, &user_id, Some(&et_id), "https://93.184.215.14/hook").await;
    let start = Utc::now() + Duration::days(10);
    let confirmed =
        insert_v1_booking(&pool, &et_id, "c@example.com", start, 30, "confirmed", &[]).await;
    let cancel_token: String = sqlx::query_scalar("SELECT cancel_token FROM bookings WHERE id = ?")
        .bind(&confirmed)
        .fetch_one(&pool)
        .await
        .unwrap();
    let csrf = "csrf-wh-cancel";
    let response = app
        .clone()
        .oneshot(post_form_unauthed(
            &format!("/booking/cancel/{cancel_token}"),
            csrf,
            &format!("_csrf={csrf}&reason=Double+booked"),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);

    let pending = insert_v1_booking(
        &pool,
        &et_id,
        "p@example.com",
        start + Duration::days(1),
        30,
        "pending",
        &[],
    )
    .await;
    let response = app
        .oneshot(post_form(
            &format!("/dashboard/bookings/{pending}/cancel"),
            &session,
            csrf,
            &format!("_csrf={csrf}"),
        ))
        .await
        .unwrap();
    assert!(response.status().is_redirection());

    let rows = deliveries(&pool).await;
    let events: Vec<&str> = rows.iter().map(|r| r.0.as_str()).collect();
    assert_eq!(events, ["booking.cancelled", "booking.declined"]);
    let cancelled = json(&rows[0].1);
    assert_eq!(cancelled["booking"]["cancelled_by"], "guest");
    assert_eq!(cancelled["booking"]["reason"], "Double booked");
    assert_eq!(cancelled["booking"]["status"], "cancelled");
    let declined = json(&rows[1].1);
    assert_eq!(declined["booking"]["status"], "declined");
    assert!(declined["booking"]["cancelled_by"].is_null());
}

#[tokio::test]
async fn an_expired_request_is_queued() {
    let pool = setup_test_db().await;
    let (user_id, _, et_id) = seed_test_data(&pool).await;
    insert_webhook(&pool, &user_id, None, "https://93.184.215.14/hook").await;
    let start = Utc::now() + Duration::days(3);
    insert_v1_booking(
        &pool,
        &et_id,
        "x@example.com",
        start,
        30,
        "pending",
        &[(
            "pending_expires_at",
            Some(utc_stamp(Utc::now() - Duration::minutes(1))),
        )],
    )
    .await;
    sweep_pending_requests(&pool, &[0u8; 32], Utc::now()).await;
    let rows = deliveries(&pool).await;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].0, "booking.expired");
    assert_eq!(json(&rows[0].1)["booking"]["status"], "declined");
}

// --- The dashboard page ---

async fn webhook_count(pool: &SqlitePool) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM webhooks")
        .fetch_one(pool)
        .await
        .unwrap()
}

#[tokio::test]
async fn admin_adds_a_webhook_and_the_secret_is_encrypted() {
    let (app, pool, session, _) = setup_test_app().await;
    let csrf = "csrf-wh-add";
    let response = app
        .clone()
        .oneshot(post_form(
            "/dashboard/webhooks",
            &session,
            csrf,
            &format!(
                "_csrf={csrf}&scope=personal&url=https%3A%2F%2F93.184.215.14%2Fhook&secret={SECRET}"
            ),
        ))
        .await
        .unwrap();
    let location = response.headers()["location"].to_str().unwrap().to_string();
    assert_eq!(
        location,
        "/dashboard/webhooks?notice=webhooks-notice-created"
    );
    let (id, url, enc): (String, String, String) =
        sqlx::query_as("SELECT id, url, secret_enc FROM webhooks")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(url, "https://93.184.215.14/hook");
    assert!(!enc.contains(SECRET));
    assert_eq!(
        crate::crypto::decrypt_value(&[0u8; 32], &enc).unwrap(),
        SECRET
    );

    // An empty secret on edit keeps the stored one.
    let response = app
        .clone()
        .oneshot(post_form(
            &format!("/dashboard/webhooks/{id}/update"),
            &session,
            csrf,
            &format!("_csrf={csrf}&url=https%3A%2F%2F93.184.215.14%2Fother&secret="),
        ))
        .await
        .unwrap();
    assert!(response.status().is_redirection());
    let (url, enc2): (String, String) = sqlx::query_as("SELECT url, secret_enc FROM webhooks")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(url, "https://93.184.215.14/other");
    assert_eq!(enc2, enc);

    // The page lists it and never the secret.
    let response = app
        .oneshot(get_authed("/dashboard/webhooks", &session))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = body_string(response).await;
    assert!(body.contains("93.184.215.14") && body.contains("other"));
    assert!(!body.contains(SECRET));
    assert!(!body.contains(&enc));
}

#[tokio::test]
async fn unsafe_urls_and_weak_secrets_are_refused() {
    let (app, pool, session, _) = setup_test_app().await;
    let csrf = "csrf-wh-bad";
    for (url, secret, key) in [
        (
            "http%3A%2F%2F93.184.215.14%2Fh",
            SECRET,
            "webhooks-error-https",
        ),
        (
            "https%3A%2F%2F10.1.2.3%2Fh",
            SECRET,
            "webhooks-error-private",
        ),
        (
            "https%3A%2F%2F%5B%3A%3Affff%3A127.0.0.1%5D%2Fh",
            SECRET,
            "webhooks-error-private",
        ),
        (
            "https%3A%2F%2Fu%3Ap%4093.184.215.14%2Fh",
            SECRET,
            "webhooks-error-credentials",
        ),
        ("nope", SECRET, "webhooks-error-url"),
        (
            "https%3A%2F%2F93.184.215.14%2Fh",
            "short",
            "webhooks-error-secret",
        ),
    ] {
        let response = app
            .clone()
            .oneshot(post_form(
                "/dashboard/webhooks",
                &session,
                csrf,
                &format!("_csrf={csrf}&scope=personal&url={url}&secret={secret}"),
            ))
            .await
            .unwrap();
        assert_eq!(
            response.headers()["location"],
            format!("/dashboard/webhooks?error={key}"),
            "{url}"
        );
    }
    // An admin naming an event type that does not exist.
    let response = app
        .clone()
        .oneshot(post_form(
            "/dashboard/webhooks",
            &session,
            csrf,
            &format!(
                "_csrf={csrf}&scope=no-such-id&url=https%3A%2F%2F93.184.215.14%2Fh&secret={SECRET}"
            ),
        ))
        .await
        .unwrap();
    assert_eq!(
        response.headers()["location"],
        "/dashboard/webhooks?error=webhooks-error-scope"
    );
    assert_eq!(webhook_count(&pool).await, 0);
    // A missing CSRF token changes nothing either.
    let response = app
        .oneshot(post_form(
            "/dashboard/webhooks",
            &session,
            csrf,
            &format!("scope=personal&url=https%3A%2F%2F93.184.215.14%2Fh&secret={SECRET}"),
        ))
        .await
        .unwrap();
    assert!(!response.status().is_success() || webhook_count(&pool).await == 0);
    assert_eq!(webhook_count(&pool).await, 0);
}

#[tokio::test]
async fn non_admins_need_the_switch() {
    let (app, pool, _, _) = setup_test_app().await;
    let (member, member_session) =
        insert_test_user_with_session(&pool, "Member", "member@example.com", "member").await;
    insert_personal_et(&pool, &member, "member-meeting").await;
    let csrf = "csrf-wh-switch";
    let add =
        format!("_csrf={csrf}&scope=personal&url=https%3A%2F%2F93.184.215.14%2Fh&secret={SECRET}");

    let page = body_string(
        app.clone()
            .oneshot(get_authed("/dashboard/webhooks", &member_session))
            .await
            .unwrap(),
    )
    .await;
    assert!(page.contains(&crate::i18n::translate("en", "webhooks-not-allowed", None)));
    app.clone()
        .oneshot(post_form(
            "/dashboard/webhooks",
            &member_session,
            csrf,
            &add,
        ))
        .await
        .unwrap();
    assert_eq!(webhook_count(&pool).await, 0);

    sqlx::query("UPDATE auth_config SET webhooks_allow_all_users = 1 WHERE id = 'singleton'")
        .execute(&pool)
        .await
        .unwrap();
    app.clone()
        .oneshot(post_form(
            "/dashboard/webhooks",
            &member_session,
            csrf,
            &add,
        ))
        .await
        .unwrap();
    assert_eq!(webhook_count(&pool).await, 1);
}

#[tokio::test]
async fn users_cannot_touch_each_others_webhooks() {
    let (app, pool, admin_session, admin_et) = setup_test_app().await;
    sqlx::query("UPDATE auth_config SET webhooks_allow_all_users = 1 WHERE id = 'singleton'")
        .execute(&pool)
        .await
        .unwrap();
    let admin_id = test_user_id(&pool).await;
    let (member, member_session) =
        insert_test_user_with_session(&pool, "Member", "member@example.com", "member").await;
    let personal = insert_webhook(&pool, &admin_id, None, "https://93.184.215.14/a").await;
    let on_et = insert_webhook(&pool, &admin_id, Some(&admin_et), "https://93.184.215.14/b").await;
    let csrf = "csrf-wh-other";

    // Not listed for the member.
    let page = body_string(
        app.clone()
            .oneshot(get_authed("/dashboard/webhooks", &member_session))
            .await
            .unwrap(),
    )
    .await;
    assert!(!page.contains("93.184.215.14"));

    for id in [&personal, &on_et] {
        for action in ["delete", "toggle", "test"] {
            app.clone()
                .oneshot(post_form(
                    &format!("/dashboard/webhooks/{id}/{action}"),
                    &member_session,
                    csrf,
                    &format!("_csrf={csrf}"),
                ))
                .await
                .unwrap();
        }
        app.clone()
            .oneshot(post_form(
                &format!("/dashboard/webhooks/{id}/update"),
                &member_session,
                csrf,
                &format!("_csrf={csrf}&url=https%3A%2F%2F93.184.215.14%2Fevil&secret="),
            ))
            .await
            .unwrap();
    }
    let rows: Vec<(String, bool, Option<String>)> =
        sqlx::query_as("SELECT url, enabled, last_attempt_at FROM webhooks ORDER BY url")
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(rows.len(), 2);
    assert!(rows
        .iter()
        .all(|(url, enabled, tried)| !url.ends_with("evil") && *enabled && tried.is_none()));

    // Nor can the member attach one to the admin's event type.
    app.clone()
        .oneshot(post_form(
            "/dashboard/webhooks",
            &member_session,
            csrf,
            &format!(
                "_csrf={csrf}&scope={admin_et}&url=https%3A%2F%2F93.184.215.14%2Fh&secret={SECRET}"
            ),
        ))
        .await
        .unwrap();
    assert_eq!(webhook_count(&pool).await, 2);
    let _ = member;

    // The owner can remove theirs.
    app.oneshot(post_form(
        &format!("/dashboard/webhooks/{personal}/delete"),
        &admin_session,
        csrf,
        &format!("_csrf={csrf}"),
    ))
    .await
    .unwrap();
    assert_eq!(webhook_count(&pool).await, 1);
}

#[tokio::test]
async fn team_admins_manage_their_team_event_types_only() {
    let (app, pool, _, _) = setup_test_app().await;
    sqlx::query("UPDATE auth_config SET webhooks_allow_all_users = 1 WHERE id = 'singleton'")
        .execute(&pool)
        .await
        .unwrap();
    let (lead, lead_session) =
        insert_test_user_with_session(&pool, "Lead", "lead@example.com", "lead").await;
    let (plain, plain_session) =
        insert_test_user_with_session(&pool, "Plain", "plain@example.com", "plain").await;
    let (_, team_et) = insert_team_with_et(
        &pool,
        "webhook-team",
        "team-call",
        &[(&lead, "admin"), (&plain, "member")],
    )
    .await;
    let csrf = "csrf-wh-team";
    let add =
        format!("_csrf={csrf}&scope={team_et}&url=https%3A%2F%2F93.184.215.14%2Fh&secret={SECRET}");
    let response = app
        .clone()
        .oneshot(post_form("/dashboard/webhooks", &plain_session, csrf, &add))
        .await
        .unwrap();
    assert_eq!(
        response.headers()["location"],
        "/dashboard/webhooks?error=webhooks-error-scope"
    );
    assert_eq!(webhook_count(&pool).await, 0);

    app.clone()
        .oneshot(post_form("/dashboard/webhooks", &lead_session, csrf, &add))
        .await
        .unwrap();
    assert_eq!(webhook_count(&pool).await, 1);
    let page = body_string(
        app.oneshot(get_authed("/dashboard/webhooks", &lead_session))
            .await
            .unwrap(),
    )
    .await;
    assert!(page.contains("webhook-team"));
}

#[tokio::test]
async fn send_test_refuses_a_private_target() {
    let _env = crate::test_support::ENV_LOCK.lock().await;
    let _guard = crate::test_support::EnvGuard::new();
    let (app, pool, session, _) = setup_test_app().await;
    let admin_id = test_user_id(&pool).await;
    let (url, got, _) = spawn_receiver(200).await;
    let id = insert_webhook(&pool, &admin_id, None, &url).await;
    let csrf = "csrf-wh-test";
    let response = app
        .oneshot(post_form(
            &format!("/dashboard/webhooks/{id}/test"),
            &session,
            csrf,
            &format!("_csrf={csrf}"),
        ))
        .await
        .unwrap();
    assert_eq!(
        response.headers()["location"],
        "/dashboard/webhooks?error=webhooks-error-test"
    );
    assert!(got.lock().unwrap().is_empty());
    let error: Option<String> = sqlx::query_scalar("SELECT last_error FROM webhooks")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(error.unwrap().starts_with("refused"));
}

#[tokio::test]
async fn send_test_posts_a_signed_ping() {
    let pool = setup_test_db().await;
    let (user_id, _, et_id) = seed_test_data(&pool).await;
    let (url, got, _) = spawn_receiver(204).await;
    let id = insert_webhook(&pool, &user_id, Some(&et_id), &url).await;
    let attempt = wh::send_test_with(&pool, &[0u8; 32], &id, &local())
        .await
        .unwrap();
    assert!(attempt.ok());
    assert_eq!(attempt.status, Some(204));
    let got = got.lock().unwrap().clone();
    let v: serde_json::Value = serde_json::from_slice(&got[0].body).unwrap();
    assert_eq!(v["event"], "ping");
    assert_eq!(v["test"], true);
    assert_eq!(v["event_type"]["slug"], "test-meeting");
    // A test is not queued.
    assert!(deliveries(&pool).await.is_empty());
}

#[tokio::test]
async fn webhook_page_keys_exist() {
    for key in [
        "webhooks-notice-created",
        "webhooks-notice-updated",
        "webhooks-notice-deleted",
        "webhooks-notice-enabled",
        "webhooks-notice-disabled",
        "webhooks-notice-test-ok",
        "webhooks-error-url",
        "webhooks-error-https",
        "webhooks-error-credentials",
        "webhooks-error-private",
        "webhooks-error-resolve",
        "webhooks-error-secret",
        "webhooks-error-scope",
        "webhooks-error-limit",
        "webhooks-error-test",
    ] {
        assert_ne!(crate::i18n::translate("en", key, None), key, "{key}");
    }
}
