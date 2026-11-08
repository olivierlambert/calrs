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

/// A receiver that waits `delay` before answering 200.
async fn spawn_slow_receiver(delay: std::time::Duration) -> String {
    let app = Router::new().route(
        "/hook",
        post(move || async move {
            tokio::time::sleep(delay).await;
            "ok"
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("http://127.0.0.1:{port}/hook")
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
    // ...and the pass drops it rather than keep the guest data.
    assert!(deliveries(&pool).await.is_empty());
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
        Context::rescheduled("2026-07-01T14:00:00", "2026-07-01T14:30:00", "pending"),
    )
    .await;
    let rows = deliveries(&pool).await;
    let v = json(&rows[0].1);
    assert_eq!(v["event"], "booking.rescheduled");
    assert_eq!(v["booking"]["previous"]["start"], "2026-07-01T12:00:00Z");
    assert_eq!(v["booking"]["previous"]["end"], "2026-07-01T12:30:00Z");
    assert_eq!(v["booking"]["start"], utc_stamp(start));
    // The status the reschedule left it in, not the row's.
    assert_eq!(v["booking"]["status"], "pending");
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
                "_csrf={csrf}&scope=personal&url=https%3A%2F%2F93.184.215.14%2Fhook&secret={SECRET}&events=booking.created"
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
            &format!("_csrf={csrf}&url=https%3A%2F%2F93.184.215.14%2Fother&secret=&events=booking.created&events=booking.cancelled"),
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
    let events: Option<String> = sqlx::query_scalar("SELECT events FROM webhooks")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(events.as_deref(), Some("booking.created,booking.cancelled"));

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
            "webhooks-error-address",
        ),
        (
            "https%3A%2F%2F%5B%3A%3Affff%3A127.0.0.1%5D%2Fh",
            SECRET,
            "webhooks-error-address",
        ),
        (
            "https%3A%2F%2Fu%3Ap%4093.184.215.14%2Fh",
            SECRET,
            "webhooks-error-credentials",
        ),
        // Unresolvable reads the same as private: no probing of host names.
        (
            "https%3A%2F%2Fno-such-host.invalid%2Fh",
            SECRET,
            "webhooks-error-address",
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
                &format!(
                    "_csrf={csrf}&scope=personal&url={url}&secret={secret}&events=booking.created"
                ),
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
                "_csrf={csrf}&scope=no-such-id&url=https%3A%2F%2F93.184.215.14%2Fh&secret={SECRET}&events=booking.created"
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
            &format!("scope=personal&url=https%3A%2F%2F93.184.215.14%2Fh&secret={SECRET}&events=booking.created"),
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
        format!("_csrf={csrf}&scope=personal&url=https%3A%2F%2F93.184.215.14%2Fh&secret={SECRET}&events=booking.created");

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
                &format!("_csrf={csrf}&url=https%3A%2F%2F93.184.215.14%2Fevil&secret=&events=booking.created"),
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
                "_csrf={csrf}&scope={admin_et}&url=https%3A%2F%2F93.184.215.14%2Fh&secret={SECRET}&events=booking.created"
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
        format!("_csrf={csrf}&scope={team_et}&url=https%3A%2F%2F93.184.215.14%2Fh&secret={SECRET}&events=booking.created");
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
    for event in wh::ALL_EVENTS {
        let key = event.label_key();
        assert_ne!(crate::i18n::translate("en", key, None), key, "{key}");
    }
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
        "webhooks-error-address",
        "webhooks-error-secret",
        "webhooks-error-scope",
        "webhooks-error-limit",
        "webhooks-error-test",
        "webhooks-error-test-limit",
        "webhooks-error-events",
        "webhooks-added-by-former",
    ] {
        assert_ne!(crate::i18n::translate("en", key, None), key, "{key}");
    }
}

#[tokio::test]
async fn unticked_events_are_not_queued() {
    let (app, pool, session, et_id) = setup_test_app().await;
    let csrf = "csrf-wh-events";
    // Nothing ticked is refused.
    let response = app
        .clone()
        .oneshot(post_form(
            "/dashboard/webhooks",
            &session,
            csrf,
            &format!(
                "_csrf={csrf}&scope=personal&url=https%3A%2F%2F93.184.215.14%2Fh&secret={SECRET}"
            ),
        ))
        .await
        .unwrap();
    assert_eq!(
        response.headers()["location"],
        "/dashboard/webhooks?error=webhooks-error-events"
    );
    // Every box ticked is stored as "all".
    let all: String = wh::ALL_EVENTS
        .iter()
        .map(|e| format!("&events={}", e.name()))
        .collect();
    app.clone()
        .oneshot(post_form(
            "/dashboard/webhooks",
            &session,
            csrf,
            &format!("_csrf={csrf}&scope=personal&url=https%3A%2F%2F93.184.215.14%2Fall&secret={SECRET}{all}"),
        ))
        .await
        .unwrap();
    // Only cancellations.
    app.clone()
        .oneshot(post_form(
            "/dashboard/webhooks",
            &session,
            csrf,
            &format!("_csrf={csrf}&scope=personal&url=https%3A%2F%2F93.184.215.14%2Fcancel&secret={SECRET}&events=booking.cancelled"),
        ))
        .await
        .unwrap();
    let stored: Vec<(String, Option<String>)> =
        sqlx::query_as("SELECT url, events FROM webhooks ORDER BY url")
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(
        stored,
        [
            ("https://93.184.215.14/all".to_string(), None),
            (
                "https://93.184.215.14/cancel".to_string(),
                Some("booking.cancelled".to_string())
            ),
        ]
    );

    let (booking_id, _, _) = insert_test_booking(
        &pool,
        &et_id,
        "2030-01-07T10:00:00",
        "2030-01-07T10:30:00",
        "confirmed",
    )
    .await;
    wh::enqueue(&pool, &booking_id, Event::Created, Context::default()).await;
    wh::enqueue(
        &pool,
        &booking_id,
        Event::Cancelled,
        Context::cancelled(Actor::Host, None),
    )
    .await;
    let queued: Vec<(String, String)> = sqlx::query_as(
        "SELECT w.url, d.event FROM webhook_deliveries d JOIN webhooks w ON w.id = d.webhook_id ORDER BY w.url, d.seq",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    let queued: Vec<(&str, &str)> = queued
        .iter()
        .map(|(u, e)| (u.as_str(), e.as_str()))
        .collect();
    assert_eq!(
        queued,
        [
            ("https://93.184.215.14/all", "booking.created"),
            ("https://93.184.215.14/all", "booking.cancelled"),
            ("https://93.184.215.14/cancel", "booking.cancelled"),
        ]
    );
    // The page shows the selection.
    let page = body_string(
        app.oneshot(get_authed("/dashboard/webhooks", &session))
            .await
            .unwrap(),
    )
    .await;
    assert!(page.contains(&crate::i18n::translate("en", "webhooks-events-all", None)));
    assert!(page.contains(r#"value="booking.rescheduled" checked"#));
}

/// A team with the test admin and a second team admin, `leaver`, who adds
/// a webhook on the team event type. Returns (team id, team et, leaver,
/// webhook id).
async fn team_webhook_by_second_admin(pool: &SqlitePool) -> (String, String, String, String) {
    let admin_id = test_user_id(pool).await;
    let (leaver, _) =
        insert_test_user_with_session(pool, "Leaver", "leaver@example.com", "leaver").await;
    let (team_id, team_et) = insert_team_with_et(
        pool,
        "keep-team",
        "keep-call",
        &[(&admin_id, "admin"), (&leaver, "admin")],
    )
    .await;
    let wid = insert_webhook(pool, &leaver, Some(&team_et), "https://93.184.215.14/team").await;
    sqlx::query("UPDATE auth_config SET webhooks_allow_all_users = 1 WHERE id = 'singleton'")
        .execute(pool)
        .await
        .unwrap();
    (team_id, team_et, leaver, wid)
}

async fn queue_one(pool: &SqlitePool, et_id: &str) -> usize {
    // A new slot each time: one event type cannot hold two bookings at once.
    let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM bookings")
        .fetch_one(pool)
        .await
        .unwrap();
    let start = Utc::now() + Duration::days(10 + n);
    let booking_id =
        insert_v1_booking(pool, et_id, "q@example.com", start, 30, "confirmed", &[]).await;
    let before = deliveries(pool).await.len();
    wh::enqueue(pool, &booking_id, Event::Created, Context::default()).await;
    deliveries(pool).await.len() - before
}

#[tokio::test]
async fn an_event_type_webhook_outlives_the_user_who_added_it_suspended() {
    let (app, pool, admin_session, _) = setup_test_app().await;
    let (_, team_et, leaver, wid) = team_webhook_by_second_admin(&pool).await;
    insert_webhook(&pool, &leaver, None, "https://93.184.215.14/mine").await;
    assert_eq!(queue_one(&pool, &team_et).await, 1);
    crate::auth::delete_user(&pool, &leaver, None, None)
        .await
        .unwrap();

    // The team webhook stays, the personal one went with its user.
    let rows: Vec<(String, Option<String>)> = sqlx::query_as("SELECT id, user_id FROM webhooks")
        .fetch_all(&pool)
        .await
        .unwrap();
    assert_eq!(rows, [(wid.clone(), None)]);
    // Suspended: nothing new is queued, and what was queued is dropped.
    assert_eq!(queue_one(&pool, &team_et).await, 0);
    wh::deliver_due_with(&pool, &[0u8; 32], Utc::now(), &local()).await;
    assert!(deliveries(&pool).await.is_empty());

    let page = body_string(
        app.clone()
            .oneshot(get_authed("/dashboard/webhooks", &admin_session))
            .await
            .unwrap(),
    )
    .await;
    assert!(page.contains(&crate::i18n::translate(
        "en",
        "webhooks-added-by-former",
        None
    )));
    assert!(page.contains(&format!("/dashboard/webhooks/{wid}/adopt")));

    // A team admin takes it over and it sends again.
    let csrf = "csrf-wh-adopt";
    let response = app
        .oneshot(post_form(
            &format!("/dashboard/webhooks/{wid}/adopt"),
            &admin_session,
            csrf,
            &format!("_csrf={csrf}&url=https%3A%2F%2F93.184.215.14%2Fmine&secret=fresh-secret-0123456789"),
        ))
        .await
        .unwrap();
    assert_eq!(
        response.headers()["location"],
        "/dashboard/webhooks?notice=webhooks-notice-adopted"
    );
    assert_eq!(queue_one(&pool, &team_et).await, 1);
}

#[tokio::test]
async fn leaving_the_team_suspends_the_webhook() {
    let (_, pool, _, _) = setup_test_app().await;
    let (team_id, team_et, leaver, _) = team_webhook_by_second_admin(&pool).await;
    // Demoted to member: no longer manages the event type.
    sqlx::query("UPDATE team_members SET role = 'member' WHERE team_id = ? AND user_id = ?")
        .bind(&team_id)
        .bind(&leaver)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(queue_one(&pool, &team_et).await, 0);
    // Removed from the team altogether: same.
    sqlx::query("DELETE FROM team_members WHERE team_id = ? AND user_id = ?")
        .bind(&team_id)
        .bind(&leaver)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(queue_one(&pool, &team_et).await, 0);
    // The team gone, its event type turns personal: still not theirs.
    sqlx::query("UPDATE event_types SET team_id = NULL WHERE id = ?")
        .bind(&team_et)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(queue_one(&pool, &team_et).await, 0);
}

#[tokio::test]
async fn the_take_over_is_for_managers_only() {
    let (app, pool, _, _) = setup_test_app().await;
    let (_, _, leaver, wid) = team_webhook_by_second_admin(&pool).await;
    crate::auth::delete_user(&pool, &leaver, None, None)
        .await
        .unwrap();
    let (_, outsider) =
        insert_test_user_with_session(&pool, "Out", "out@example.com", "outsider").await;
    let csrf = "csrf-wh-adopt-out";
    app.oneshot(post_form(
        &format!("/dashboard/webhooks/{wid}/adopt"),
        &outsider,
        csrf,
        &format!("_csrf={csrf}&url=https%3A%2F%2F93.184.215.14%2Fx&secret=fresh-secret-0123456789"),
    ))
    .await
    .unwrap();
    let owner: Option<String> = sqlx::query_scalar("SELECT user_id FROM webhooks")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(owner.is_none());
}

#[tokio::test]
async fn queues_of_webhooks_that_stop_sending_are_dropped() {
    let (app, pool, admin_session, _) = setup_test_app().await;
    let member = insert_role_user(&pool, "member@example.com", "user").await;
    let member_et = insert_personal_et(&pool, &member, "member-meeting").await;
    sqlx::query("UPDATE auth_config SET webhooks_allow_all_users = 1 WHERE id = 'singleton'")
        .execute(&pool)
        .await
        .unwrap();
    insert_webhook(&pool, &member, None, "https://93.184.215.14/m").await;
    assert_eq!(queue_one(&pool, &member_et).await, 1);

    // The admin switch goes off: the member's queue goes at once.
    let csrf = "csrf-wh-policy";
    app.oneshot(post_form(
        "/dashboard/admin/webhooks/policy",
        &admin_session,
        csrf,
        &format!("_csrf={csrf}"),
    ))
    .await
    .unwrap();
    assert!(deliveries(&pool).await.is_empty());

    // The member is disabled: dropped at the next pass.
    sqlx::query("UPDATE auth_config SET webhooks_allow_all_users = 1 WHERE id = 'singleton'")
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(queue_one(&pool, &member_et).await, 1);
    sqlx::query("UPDATE users SET enabled = 0 WHERE id = ?")
        .bind(&member)
        .execute(&pool)
        .await
        .unwrap();
    wh::start_pass(&pool, &[0u8; 32], Utc::now(), &local()).await;
    assert!(deliveries(&pool).await.is_empty());
}

#[tokio::test]
async fn deliveries_are_given_up_after_a_day() {
    let pool = setup_test_db().await;
    let (user_id, _, et_id) = seed_test_data(&pool).await;
    insert_webhook(&pool, &user_id, None, "https://93.184.215.14/hook").await;
    assert_eq!(queue_one(&pool, &et_id).await, 1);
    // A day later, still pending (held back, say): given up, then pruned.
    wh::prune(&pool, Utc::now()).await;
    assert_eq!(deliveries(&pool).await[0].2, "pending");
    wh::prune(&pool, Utc::now() + Duration::hours(25)).await;
    assert_eq!(deliveries(&pool).await[0].2, "failed");
    wh::prune(&pool, Utc::now() + Duration::days(8)).await;
    assert!(deliveries(&pool).await.is_empty());
}

#[tokio::test]
async fn send_test_is_rate_limited_per_user() {
    let (app, pool, session, _) = setup_test_app().await;
    let admin_id = test_user_id(&pool).await;
    // A private target is refused before any network traffic.
    let id = insert_webhook(&pool, &admin_id, None, "https://10.255.255.1/hook").await;
    let csrf = "csrf-wh-limit";
    for _ in 0..10 {
        let response = app
            .clone()
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
    }
    let response = app
        .clone()
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
        "/dashboard/webhooks?error=webhooks-error-test-limit"
    );
    let page = body_string(
        app.oneshot(get_authed(
            "/dashboard/webhooks?error=webhooks-error-test-limit",
            &session,
        ))
        .await
        .unwrap(),
    )
    .await;
    assert!(page.contains(&crate::i18n::translate(
        "en",
        "webhooks-error-test-limit",
        None
    )));
}

async fn pending_for(pool: &SqlitePool, webhook_id: &str) -> Vec<(i64, String)> {
    sqlx::query_as(
        "SELECT attempts, next_attempt_at FROM webhook_deliveries WHERE webhook_id = ? AND state = 'pending' ORDER BY seq",
    )
    .bind(webhook_id)
    .fetch_all(pool)
    .await
    .unwrap()
}

#[tokio::test]
async fn a_slow_endpoint_does_not_hold_up_the_others() {
    let pool = setup_test_db().await;
    let (user_id, _, et_id) = seed_test_data(&pool).await;
    let slow_url = spawn_slow_receiver(std::time::Duration::from_secs(3)).await;
    let (fast_url, fast_got, _) = spawn_receiver(200).await;
    let slow = insert_webhook(&pool, &user_id, None, &slow_url).await;
    let fast = insert_webhook(&pool, &user_id, Some(&et_id), &fast_url).await;
    let (booking_id, _, _) = insert_test_booking(
        &pool,
        &et_id,
        "2030-01-07T10:00:00",
        "2030-01-07T10:30:00",
        "confirmed",
    )
    .await;
    wh::enqueue(&pool, &booking_id, Event::Created, Context::default()).await;

    let now = Utc::now();
    let handles = wh::start_pass(&pool, &[0u8; 32], now, &local()).await;
    assert_eq!(handles.len(), 2);
    // The healthy endpoint is served while the slow one still waits.
    let started = std::time::Instant::now();
    while fast_got.lock().unwrap().is_empty() || !pending_for(&pool, &fast).await.is_empty() {
        assert!(
            started.elapsed() < std::time::Duration::from_secs(2),
            "fast endpoint waited"
        );
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    assert_eq!(pending_for(&pool, &slow).await.len(), 1);
    // A pass meanwhile starts nothing for the endpoint already in flight.
    assert!(wh::start_pass(&pool, &[0u8; 32], now, &local())
        .await
        .is_empty());
    for h in handles {
        h.await.unwrap();
    }
    assert!(pending_for(&pool, &slow).await.is_empty());
}

#[tokio::test]
async fn a_failing_endpoint_gets_one_request_per_retry() {
    let pool = setup_test_db().await;
    let (user_id, _, et_id) = seed_test_data(&pool).await;
    let (url, got, _) = spawn_receiver(500).await;
    let wid = insert_webhook(&pool, &user_id, None, &url).await;
    for day in 7..10 {
        let (booking_id, _, _) = insert_test_booking(
            &pool,
            &et_id,
            &format!("2030-01-{day:02}T10:00:00"),
            &format!("2030-01-{day:02}T10:30:00"),
            "confirmed",
        )
        .await;
        wh::enqueue(&pool, &booking_id, Event::Created, Context::default()).await;
    }
    let now = Utc::now();
    wh::deliver_due_with(&pool, &[0u8; 32], now, &local()).await;
    assert_eq!(got.lock().unwrap().len(), 1);
    let rows = pending_for(&pool, &wid).await;
    let attempts: Vec<i64> = rows.iter().map(|r| r.0).collect();
    assert_eq!(attempts, [1, 0, 0]);
    // All three wait for the same retry.
    assert!(rows.iter().all(|r| r.1 == rows[0].1));
    assert!(rows[0].1 > utc_stamp(now));
}

#[tokio::test]
async fn errors_never_carry_the_url() {
    let pool = setup_test_db().await;
    let (user_id, _, et_id) = seed_test_data(&pool).await;
    // A port nobody listens on, with a token in the path.
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    let url = format!("http://127.0.0.1:{port}/hooks/T0KEN-s3cret");
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
    wh::deliver_due_with(&pool, &[0u8; 32], Utc::now(), &local()).await;
    let (endpoint_error, delivery_error): (Option<String>, Option<String>) = sqlx::query_as(
        "SELECT w.last_error, d.last_error FROM webhooks w JOIN webhook_deliveries d ON d.webhook_id = w.id WHERE w.id = ?",
    )
    .bind(&wid)
    .fetch_one(&pool)
    .await
    .unwrap();
    for error in [endpoint_error.unwrap(), delivery_error.unwrap()] {
        assert!(!error.contains("T0KEN"), "{error}");
        assert!(!error.contains("127.0.0.1"), "{error}");
    }
}

#[test]
fn the_webhook_client_ignores_proxy_settings() {
    // A proxy resolves the host itself and would bypass the address pinning.
    // The environment is process-wide, so this is checked on the source
    // rather than by setting HTTPS_PROXY under parallel tests.
    // The webhook client comes from the shared guarded builder (#363),
    // which turns proxies off.
    let source = include_str!("../webhooks.rs");
    assert!(source.contains("crate::net_guard::builder_with("));
    assert!(!source.contains("reqwest::Client::builder()"));
    let guard = include_str!("../../net_guard.rs");
    let builder = guard
        .split("pub fn builder_with(")
        .nth(1)
        .expect("the shared client builder");
    let body = &builder[..builder.find("\n}").unwrap()];
    assert!(body.contains(".no_proxy()"), "{body}");
}

#[tokio::test]
async fn an_event_keeps_its_place_and_its_status_while_its_payload_waits() {
    let pool = setup_test_db().await;
    let (user_id, _, et_id) = seed_test_data(&pool).await;
    let (url, got, _) = spawn_receiver(200).await;
    insert_webhook(&pool, &user_id, None, &url).await;
    let (booking_id, _, _) = insert_test_booking(
        &pool,
        &et_id,
        "2030-01-07T10:00:00",
        "2030-01-07T10:30:00",
        "confirmed",
    )
    .await;
    // The booking commits; its meeting link is still being minted when the
    // guest cancels.
    let created = wh::reserve(&pool, &booking_id, Event::Created).await;
    sqlx::query("UPDATE bookings SET status = 'cancelled' WHERE id = ?")
        .bind(&booking_id)
        .execute(&pool)
        .await
        .unwrap();
    wh::enqueue(
        &pool,
        &booking_id,
        Event::Cancelled,
        Context::cancelled(Actor::Guest, None),
    )
    .await;
    // The cancellation waits behind the unfinished creation.
    assert_eq!(
        wh::deliver_due_with(&pool, &[0u8; 32], Utc::now(), &local()).await,
        0
    );
    created.fill(Context::default()).await;
    assert_eq!(
        wh::deliver_due_with(&pool, &[0u8; 32], Utc::now(), &local()).await,
        2
    );
    let got = got.lock().unwrap().clone();
    let bodies: Vec<serde_json::Value> = got
        .iter()
        .map(|r| serde_json::from_slice(&r.body).unwrap())
        .collect();
    assert_eq!(bodies[0]["event"], "booking.created");
    assert_eq!(bodies[0]["booking"]["status"], "confirmed");
    assert_eq!(bodies[1]["event"], "booking.cancelled");
    assert_eq!(bodies[1]["booking"]["status"], "cancelled");
}

#[tokio::test]
async fn a_reservation_dropped_unfilled_is_filled_anyway() {
    let pool = setup_test_db().await;
    let (user_id, _, et_id) = seed_test_data(&pool).await;
    insert_webhook(&pool, &user_id, None, "https://93.184.215.14/hook").await;
    let (booking_id, _, _) = insert_test_booking(
        &pool,
        &et_id,
        "2030-01-07T10:00:00",
        "2030-01-07T10:30:00",
        "confirmed",
    )
    .await;
    drop(wh::reserve(&pool, &booking_id, Event::Created).await);
    let started = std::time::Instant::now();
    loop {
        let rows = deliveries(&pool).await;
        if rows.len() == 1 && rows[0].2 == "pending" {
            assert_eq!(json(&rows[0].1)["event"], "booking.created");
            break;
        }
        assert!(started.elapsed() < std::time::Duration::from_secs(5));
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
}

#[tokio::test]
async fn a_claim_is_sent_with_the_claimant() {
    let pool = setup_test_db().await;
    let (user_id, _, et_id) = seed_test_data(&pool).await;
    let claimant = insert_role_user(&pool, "watcher@example.com", "user").await;
    insert_webhook(&pool, &user_id, None, "https://93.184.215.14/hook").await;
    let (booking_id, _, _) = insert_test_booking(
        &pool,
        &et_id,
        "2030-01-07T10:00:00",
        "2030-01-07T10:30:00",
        "confirmed",
    )
    .await;
    sqlx::query("UPDATE bookings SET claimed_by_user_id = ? WHERE id = ?")
        .bind(&claimant)
        .bind(&booking_id)
        .execute(&pool)
        .await
        .unwrap();
    wh::enqueue(&pool, &booking_id, Event::Claimed, Context::default()).await;
    let rows = deliveries(&pool).await;
    let v = json(&rows[0].1);
    assert_eq!(v["event"], "booking.claimed");
    assert_eq!(v["booking"]["claimed_by"]["email"], "watcher@example.com");
    assert_eq!(v["booking"]["hosts"][0]["email"], "test@example.com");
}

#[tokio::test]
async fn a_new_url_clears_the_last_result_and_errors_stay_visible() {
    let (app, pool, session, _) = setup_test_app().await;
    let admin_id = test_user_id(&pool).await;
    let id = insert_webhook(&pool, &admin_id, None, "https://93.184.215.14/a").await;
    sqlx::query("UPDATE webhooks SET enabled = 0, last_status = 500, last_error = 'HTTP 500: boom', last_attempt_at = '2030-01-01T00:00:00Z' WHERE id = ?")
        .bind(&id)
        .execute(&pool)
        .await
        .unwrap();
    // Switched off, the last error is still shown.
    let page = body_string(
        app.clone()
            .oneshot(get_authed("/dashboard/webhooks", &session))
            .await
            .unwrap(),
    )
    .await;
    assert!(page.contains("HTTP 500: boom"));

    let csrf = "csrf-wh-reset";
    let update = |url: &'static str| {
        post_form(
            &format!("/dashboard/webhooks/{id}/update"),
            &session,
            csrf,
            &format!("_csrf={csrf}&url={url}&secret=&events=booking.created"),
        )
    };
    // Same URL: kept.
    app.clone()
        .oneshot(update("https%3A%2F%2F93.184.215.14%2Fa"))
        .await
        .unwrap();
    let kept: (Option<i64>, Option<String>) =
        sqlx::query_as("SELECT last_status, last_error FROM webhooks")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(kept, (Some(500), Some("HTTP 500: boom".to_string())));
    // New URL: cleared.
    app.oneshot(update("https%3A%2F%2F93.184.215.14%2Fb"))
        .await
        .unwrap();
    let cleared: (Option<i64>, Option<String>, Option<String>) =
        sqlx::query_as("SELECT last_status, last_error, last_attempt_at FROM webhooks")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(cleared, (None, None, None));
}

#[tokio::test]
async fn a_refused_delivery_does_not_say_why_in_detail() {
    let pool = setup_test_db().await;
    let (user_id, _, et_id) = seed_test_data(&pool).await;
    let wid = insert_webhook(&pool, &user_id, None, "https://10.9.8.7/hook").await;
    assert_eq!(queue_one(&pool, &et_id).await, 1);
    wh::deliver_due_with(&pool, &[0u8; 32], Utc::now(), &[]).await;
    let error: String = sqlx::query_scalar("SELECT last_error FROM webhooks WHERE id = ?")
        .bind(&wid)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(error.starts_with("refused"), "{error}");
    assert!(!error.contains("10.9.8.7"), "{error}");
}

#[tokio::test]
async fn host_notes_never_reach_a_webhook() {
    let pool = setup_test_db().await;
    let (user_id, _, et_id) = seed_test_data(&pool).await;
    insert_webhook(&pool, &user_id, None, "https://93.184.215.14/hook").await;
    let start = Utc::now() + Duration::days(10);
    let booking_id = insert_v1_booking(
        &pool,
        &et_id,
        "n@example.com",
        start,
        30,
        "confirmed",
        &[
            ("notes", Some("Guest wrote this".to_string())),
            ("host_notes", Some("HOST-ONLY-NOTE".to_string())),
        ],
    )
    .await;
    wh::enqueue(&pool, &booking_id, Event::Created, Context::default()).await;
    let rows = deliveries(&pool).await;
    assert_eq!(rows.len(), 1);
    assert!(rows[0].1.contains("Guest wrote this"));
    assert!(!rows[0].1.contains("HOST-ONLY-NOTE"));
}

#[tokio::test]
async fn two_passes_send_one_delivery_once() {
    let pool = setup_test_db().await;
    let (user_id, _, et_id) = seed_test_data(&pool).await;
    let (url, got, _) = spawn_receiver(200).await;
    insert_webhook(&pool, &user_id, None, &url).await;
    assert_eq!(queue_one(&pool, &et_id).await, 1);
    let now = Utc::now();
    let allow = local();
    let (a, b) = tokio::join!(
        wh::deliver_due_with(&pool, &[0u8; 32], now, &allow),
        wh::deliver_due_with(&pool, &[0u8; 32], now, &allow),
    );
    assert_eq!(a + b, 1);
    assert_eq!(got.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn a_delivery_is_leased_once_and_only_to_the_url_it_was_read_with() {
    let pool = setup_test_db().await;
    let (user_id, _, et_id) = seed_test_data(&pool).await;
    let wid = insert_webhook(&pool, &user_id, None, "https://93.184.215.14/a").await;
    assert_eq!(queue_one(&pool, &et_id).await, 1);
    let (id, secret_enc): (String, String) = sqlx::query_as(
        "SELECT d.id, w.secret_enc FROM webhook_deliveries d JOIN webhooks w ON w.id = d.webhook_id",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    let now = Utc::now();
    // The URL changed after the row was read: not sent.
    sqlx::query("UPDATE webhooks SET url = 'https://93.184.215.14/b' WHERE id = ?")
        .bind(&wid)
        .execute(&pool)
        .await
        .unwrap();
    assert!(!wh::lease(&pool, &id, "https://93.184.215.14/a", &secret_enc, now).await);
    // The current URL: taken once, then not again.
    assert!(wh::lease(&pool, &id, "https://93.184.215.14/b", &secret_enc, now).await);
    assert!(!wh::lease(&pool, &id, "https://93.184.215.14/b", &secret_enc, now).await);
    // A switched-off webhook is not sent to either.
    let later = now + Duration::hours(1);
    sqlx::query("UPDATE webhooks SET enabled = 0 WHERE id = ?")
        .bind(&wid)
        .execute(&pool)
        .await
        .unwrap();
    assert!(!wh::lease(&pool, &id, "https://93.184.215.14/b", &secret_enc, later).await);
}

#[tokio::test]
async fn an_unfilled_reservation_is_filled_with_its_context_later() {
    let pool = setup_test_db().await;
    let (user_id, _, et_id) = seed_test_data(&pool).await;
    // A private address: refused before any network traffic.
    let wid = insert_webhook(&pool, &user_id, None, "https://10.9.9.9/hook").await;
    let start = Utc::now() + Duration::days(10);
    let booking_id =
        insert_v1_booking(&pool, &et_id, "r@example.com", start, 30, "cancelled", &[]).await;
    let uid: String = sqlx::query_scalar("SELECT uid FROM bookings WHERE id = ?")
        .bind(&booking_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    // As left by a fill that kept its context, then hit a database error.
    let ctx = serde_json::to_string(&Context::cancelled(Actor::Guest, Some("Ill".into()))).unwrap();
    sqlx::query(
        "INSERT INTO webhook_deliveries (id, webhook_id, booking_uid, event, payload, state, next_attempt_at, created_at)
         VALUES ('r1', ?, ?, 'booking.cancelled', ?, 'reserved', '2000-01-01T00:00:00Z', datetime('now', '-5 minutes'))",
    )
    .bind(&wid)
    .bind(&uid)
    .bind(&ctx)
    .execute(&pool)
    .await
    .unwrap();
    // A fresh one is left to its own fill.
    sqlx::query(
        "INSERT INTO webhook_deliveries (id, webhook_id, booking_uid, event, payload, state, next_attempt_at)
         VALUES ('r2', ?, ?, 'booking.cancelled', '', 'reserved', '2000-01-01T00:00:00Z')",
    )
    .bind(&wid)
    .bind(&uid)
    .execute(&pool)
    .await
    .unwrap();
    // The target is refused, so nothing is sent; the fill is what counts.
    wh::start_pass(&pool, &[0u8; 32], Utc::now(), &[]).await;
    let rows: Vec<(String, String, String)> =
        sqlx::query_as("SELECT id, state, payload FROM webhook_deliveries ORDER BY id")
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(rows[0].1, "pending");
    let v = json(&rows[0].2);
    assert_eq!(v["booking"]["reason"], "Ill");
    assert_eq!(v["booking"]["cancelled_by"], "guest");
    assert_eq!(rows[1].1, "reserved");
}

#[tokio::test]
async fn taking_over_needs_a_suspended_webhook_and_a_new_secret() {
    let (app, pool, admin_session, _) = setup_test_app().await;
    let (team_id, team_et, leaver, wid) = team_webhook_by_second_admin(&pool).await;
    let csrf = "csrf-wh-adopt2";
    let adopt = |body: String| {
        post_form(
            &format!("/dashboard/webhooks/{wid}/adopt"),
            &admin_session,
            csrf,
            &body,
        )
    };
    // Active: not anyone's to take.
    let response = app
        .clone()
        .oneshot(adopt(format!(
            "_csrf={csrf}&url=https%3A%2F%2F93.184.215.14%2Fmine&secret=fresh-secret-0123456789"
        )))
        .await
        .unwrap();
    assert_eq!(
        response.headers()["location"],
        "/dashboard/webhooks?error=webhooks-error-not-suspended"
    );
    let owner: Option<String> = sqlx::query_scalar("SELECT user_id FROM webhooks")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(owner.as_deref(), Some(leaver.as_str()));

    // Queue one, then the leaver is demoted: suspended.
    assert_eq!(queue_one(&pool, &team_et).await, 1);
    sqlx::query("UPDATE team_members SET role = 'member' WHERE team_id = ? AND user_id = ?")
        .bind(&team_id)
        .bind(&leaver)
        .execute(&pool)
        .await
        .unwrap();
    let old_secret: String = sqlx::query_scalar("SELECT secret_enc FROM webhooks")
        .fetch_one(&pool)
        .await
        .unwrap();
    // A secret is required.
    let response = app
        .clone()
        .oneshot(adopt(format!(
            "_csrf={csrf}&url=https%3A%2F%2F93.184.215.14%2Fmine&secret="
        )))
        .await
        .unwrap();
    assert_eq!(
        response.headers()["location"],
        "/dashboard/webhooks?error=webhooks-error-secret"
    );
    app.oneshot(adopt(format!(
        "_csrf={csrf}&url=https%3A%2F%2F93.184.215.14%2Fmine&secret=fresh-secret-0123456789"
    )))
    .await
    .unwrap();
    let (owner, url, secret_enc): (Option<String>, String, String) =
        sqlx::query_as("SELECT user_id, url, secret_enc FROM webhooks")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(owner, Some(test_user_id(&pool).await));
    assert_eq!(url, "https://93.184.215.14/mine");
    assert_ne!(secret_enc, old_secret);
    assert_eq!(
        crate::crypto::decrypt_value(&[0u8; 32], &secret_enc).unwrap(),
        "fresh-secret-0123456789"
    );
    // What was queued for the old receiver is gone.
    assert!(deliveries(&pool).await.is_empty());
}

#[tokio::test]
async fn a_url_change_drops_the_queue_and_another_host_needs_a_new_secret() {
    let (app, pool, session, et_id) = setup_test_app().await;
    let admin_id = test_user_id(&pool).await;
    let wid = insert_webhook(&pool, &admin_id, None, "https://93.184.215.14/a").await;
    let csrf = "csrf-wh-url";
    let update = |url: &str, secret: &str| {
        post_form(
            &format!("/dashboard/webhooks/{wid}/update"),
            &session,
            csrf,
            &format!("_csrf={csrf}&url={url}&secret={secret}&events=booking.created"),
        )
    };
    assert_eq!(queue_one(&pool, &et_id).await, 1);
    // Another host, no new secret: refused, queue kept.
    let response = app
        .clone()
        .oneshot(update("https%3A%2F%2F93.184.215.15%2Fa", ""))
        .await
        .unwrap();
    assert_eq!(
        response.headers()["location"],
        "/dashboard/webhooks?error=webhooks-error-new-secret"
    );
    assert_eq!(deliveries(&pool).await.len(), 1);
    // Same host, another path: fine without a secret, and the queue goes.
    app.clone()
        .oneshot(update("https%3A%2F%2F93.184.215.14%2Fb", ""))
        .await
        .unwrap();
    assert!(deliveries(&pool).await.is_empty());
    // Another host with a new secret: fine.
    app.oneshot(update(
        "https%3A%2F%2F93.184.215.15%2Fc",
        "another-secret-0123456789",
    ))
    .await
    .unwrap();
    let url: String = sqlx::query_scalar("SELECT url FROM webhooks")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(url, "https://93.184.215.15/c");
}

#[tokio::test]
async fn only_the_webhooks_user_sees_its_full_url() {
    let (app, pool, admin_session, _) = setup_test_app().await;
    let (lead, lead_session) =
        insert_test_user_with_session(&pool, "Lead", "lead@example.com", "lead").await;
    let admin_id = test_user_id(&pool).await;
    let (_, team_et) = insert_team_with_et(
        &pool,
        "mask-team",
        "mask-call",
        &[(&admin_id, "admin"), (&lead, "admin")],
    )
    .await;
    sqlx::query("UPDATE auth_config SET webhooks_allow_all_users = 1 WHERE id = 'singleton'")
        .execute(&pool)
        .await
        .unwrap();
    insert_webhook(
        &pool,
        &lead,
        Some(&team_et),
        "https://hooks.example.com/services/T0KEN/s3cret",
    )
    .await;
    let page = |session: String| {
        let app = app.clone();
        async move {
            body_string(
                app.oneshot(get_authed("/dashboard/webhooks", &session))
                    .await
                    .unwrap(),
            )
            .await
        }
    };
    let own = page(lead_session).await;
    assert!(own.contains("T0KEN"));
    let other = page(admin_session).await;
    assert!(!other.contains("T0KEN"), "the path leaked");
    assert!(other.contains("hooks.example.com"));
}

/// The answers to the booking questions travel with the booking (#210),
/// keyed by the question's stable key, with the label as the guest read it.
#[tokio::test]
async fn the_payload_carries_the_answers() {
    let (app, pool, _, et_id) = setup_test_app().await;
    let user_id = test_user_id(&pool).await;
    insert_webhook(&pool, &user_id, None, "https://93.184.215.14/hook").await;
    let json_qs = serde_json::json!([
        {"kind": "single_choice", "label": "Where are the tyres", "options": [{"label": "At the shop"}, {"label": "I bring them"}]},
        {"kind": "checkbox", "label": "Recording", "consent": true}
    ]);
    let qs = crate::web::questions::parse_editor(&json_qs.to_string(), &[]).unwrap();
    crate::web::questions::save(&pool, &et_id, Some(&qs), None)
        .await
        .unwrap();
    let csrf = "csrf-wh-answers";
    let body = format!("{}&q_{}=o2", book_body(csrf), qs[0].id);
    app.oneshot(post_form_unauthed(
        "/u/testuser/test-meeting/book",
        csrf,
        &body,
    ))
    .await
    .unwrap();
    let rows = deliveries(&pool).await;
    let v = json(&rows[0].1);
    let answers = &v["booking"]["answers"];
    assert_eq!(answers[0]["key"], "where_are_the_tyres");
    assert_eq!(answers[0]["label"], "Where are the tyres");
    assert_eq!(answers[0]["kind"], "single_choice");
    assert_eq!(answers[0]["value"], "I bring them");
    assert_eq!(answers[0]["option_ids"][0], "o2");
    assert_eq!(answers[1]["value"], "no");
    assert_eq!(answers[1]["consent"], true);
    assert!(answers[1]["answered_at"].as_str().unwrap().ends_with('Z'));
}
