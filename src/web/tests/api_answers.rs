// REST API v1 (#169) with booking questions (#210): `POST /bookings` sends
// the guest's answers, read by the booking form's own rules and stored as
// the form stores them.

use super::api::{api_get, api_post, create_body, error_code, monday_at, pat, send, user_id, ALL};
use super::writeback_failures::{spawn_mock_dav, DavMode};
use super::*;
use axum::http::StatusCode;
use serde_json::{json, Value};

/// Every kind on an event type: a required short text, a long text, a
/// single choice, a multiple choice, a required consent with a policy link,
/// and an optional checkbox kept out of the calendar event.
async fn seed(pool: &SqlitePool, et_id: &str) {
    let json = json!([
        {"kind": "short_text", "label": "Licence plate", "required": true},
        {"kind": "long_text", "label": "Anything else"},
        {"kind": "single_choice", "label": "Where are the tyres", "options": [{"label": "At the shop"}, {"label": "I bring them"}]},
        {"kind": "multiple_choice", "label": "Topics", "options": [{"label": "Tax"}, {"label": "Accounting"}, {"label": "Strategy"}]},
        {"kind": "checkbox", "label": "I agree to the privacy policy", "consent": true, "required": true, "link": "https://example.com/privacy"},
        {"kind": "checkbox", "label": "Area of concern: housing", "in_calendar": false}
    ]);
    let qs = crate::web::questions::parse_editor(&json.to_string(), &[]).unwrap();
    crate::web::questions::save(pool, et_id, Some(&qs), None)
        .await
        .unwrap();
}

/// Answers to every question of `seed` but the last checkbox.
fn full_answers() -> Value {
    json!({
        "licence_plate": "AB-123",
        "anything_else": "Line one\r\nLine two",
        "where_are_the_tyres": "o2",
        "topics": ["o1", "o3"],
        "i_agree_to_the_privacy_policy": true
    })
}

fn with_answers(et_id: &str, start: &str, email: &str, answers: Value) -> Value {
    let mut body = create_body(et_id, start, email);
    body["answers"] = answers;
    body
}

#[allow(clippy::type_complexity)]
async fn stored(
    pool: &SqlitePool,
) -> Vec<(
    String,
    String,
    String,
    Option<String>,
    bool,
    Option<String>,
    bool,
    bool,
)> {
    sqlx::query_as(
        "SELECT key, kind, value, value_ids, consent, link_url, in_calendar, question_id IS NOT NULL
         FROM booking_answers ORDER BY position",
    )
    .fetch_all(pool)
    .await
    .unwrap()
}

async fn count(pool: &SqlitePool, table: &str) -> i64 {
    sqlx::query_scalar(&format!("SELECT COUNT(*) FROM {table}"))
        .fetch_one(pool)
        .await
        .unwrap()
}

#[tokio::test]
async fn api_answers_are_stored_and_shown_as_for_the_booking_form() {
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
    sqlx::query(
        "INSERT INTO webhooks (id, user_id, event_type_id, url, secret_enc) VALUES (?, ?, NULL, 'https://93.184.215.14/hook', ?)",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(&owner)
    .bind(crate::crypto::encrypt_value(&[0u8; 32], "0123456789abcdef-secret").unwrap())
    .execute(&pool)
    .await
    .unwrap();
    seed(&pool, &et_id).await;
    let token = pat(&pool, &owner, ALL).await;

    // The event type lists its questions with the keys and option ids to
    // answer with.
    let (status, body) = send(
        &app,
        api_get(&format!("/api/v1/event-types/{et_id}"), &token),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let questions = body["event_type"]["questions"].as_array().unwrap();
    let keys: Vec<&str> = questions
        .iter()
        .map(|q| q["key"].as_str().unwrap())
        .collect();
    assert_eq!(
        keys,
        [
            "licence_plate",
            "anything_else",
            "where_are_the_tyres",
            "topics",
            "i_agree_to_the_privacy_policy",
            "area_of_concern_housing"
        ]
    );
    assert_eq!(
        questions[2]["options"][1],
        json!({"id": "o2", "label": "I bring them"})
    );
    assert_eq!(questions[4]["required"], true);
    assert_eq!(questions[4]["consent"], true);
    assert_eq!(questions[5]["in_calendar"], false);

    let mut request = with_answers(&et_id, &monday_at("10:00"), "api@test.com", full_answers());
    request["notes"] = json!("Kept");
    let (status, body) = send(&app, api_post("/api/v1/bookings", &token, &request)).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let answers = body["booking"]["answers"].as_array().unwrap();
    let got: Vec<(&str, &str)> = answers
        .iter()
        .map(|a| (a["key"].as_str().unwrap(), a["value"].as_str().unwrap()))
        .collect();
    assert_eq!(
        got,
        [
            ("licence_plate", "AB-123"),
            ("anything_else", "Line one\nLine two"),
            ("where_are_the_tyres", "I bring them"),
            ("topics", "Tax, Strategy"),
            ("i_agree_to_the_privacy_policy", "yes"),
            ("area_of_concern_housing", "no"),
        ]
    );
    assert_eq!(answers[3]["option_ids"], json!(["o1", "o3"]));
    assert_eq!(answers[4]["link_url"], "https://example.com/privacy");

    // Stored as the form stores them: a snapshot linked to its question.
    let rows = stored(&pool).await;
    assert_eq!(rows.len(), 6);
    assert_eq!(rows[2].3.as_deref(), Some(r#"["o2"]"#));
    assert!(rows[4].4, "the consent flag is snapshotted");
    assert!(!rows[5].6, "in_calendar is snapshotted");
    assert!(rows.iter().all(|r| r.7));
    let id = body["booking"]["id"].as_str().unwrap().to_string();
    let (_, read) = send(&app, api_get(&format!("/api/v1/bookings/{id}"), &token)).await;
    assert_eq!(read["booking"]["answers"], body["booking"]["answers"]);
    let notes: Option<String> = sqlx::query_scalar("SELECT notes FROM bookings WHERE id = ?")
        .bind(&id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(notes.as_deref(), Some("Kept"));

    // The host and the guest read them in their emails.
    let host = mail_to(&mails, "test@example.com").await;
    assert!(host.contains("Licence plate"), "{host}");
    assert!(host.contains("AB-123"));
    assert!(host.contains("Agreed"));
    let guest = mail_to(&mails, "api@test.com").await;
    assert!(guest.contains("AB-123"));
    assert!(guest.contains("You agreed"));

    // The host's calendar event carries them, except the one kept out.
    let event = dav.only_event().replace("\r\n ", "");
    assert!(event.contains("AB-123"), "{event}");
    assert!(!event.contains("Area of concern"), "{event}");

    // And the webhook, keyed by question key.
    let payload: String =
        sqlx::query_scalar("SELECT payload FROM webhook_deliveries ORDER BY seq LIMIT 1")
            .fetch_one(&pool)
            .await
            .unwrap();
    let payload: Value = serde_json::from_str(&payload).unwrap();
    let hooked = payload["booking"]["answers"].as_array().unwrap();
    assert_eq!(hooked.len(), 6);
    assert_eq!(hooked[0]["key"], "licence_plate");
    assert_eq!(hooked[0]["value"], "AB-123");
    assert_eq!(hooked[3]["option_ids"], json!(["o1", "o3"]));
}

#[tokio::test]
async fn api_answers_are_refused_by_the_forms_rules_naming_the_key() {
    let _lock = crate::test_support::ENV_LOCK.lock().await;
    let _env = super::oauth::environment();
    let (app, pool, _, et_id) = setup_test_app().await;
    seed(&pool, &et_id).await;
    let token = pat(&pool, &user_id(&pool, "testuser").await, ALL).await;
    let base = json!({"licence_plate": "AB-123", "i_agree_to_the_privacy_policy": true});
    let with = |key: &str, value: Value| {
        let mut answers = base.clone();
        answers[key] = value;
        answers
    };
    let without = |key: &str| {
        let mut answers = base.clone();
        answers.as_object_mut().unwrap().remove(key);
        answers
    };
    let cases = [
        (without("licence_plate"), "licence_plate"),
        (with("licence_plate", json!("   ")), "licence_plate"),
        (
            without("i_agree_to_the_privacy_policy"),
            "i_agree_to_the_privacy_policy",
        ),
        (
            with("i_agree_to_the_privacy_policy", json!(false)),
            "i_agree_to_the_privacy_policy",
        ),
        (with("surprise", json!("x")), "surprise"),
        (
            with("where_are_the_tyres", json!("o9")),
            "where_are_the_tyres",
        ),
        (with("topics", json!(["o1", "o9"])), "topics"),
        (with("topics", json!([""])), "topics"),
        (
            with("licence_plate", json!("x".repeat(501))),
            "licence_plate",
        ),
        (
            with("anything_else", json!("x".repeat(5001))),
            "anything_else",
        ),
        (with("licence_plate", json!(5)), "licence_plate"),
        (with("licence_plate", Value::Null), "licence_plate"),
        (
            with("where_are_the_tyres", json!(["o1"])),
            "where_are_the_tyres",
        ),
        (with("topics", json!("o1")), "topics"),
        (with("topics", json!([1])), "topics"),
        (
            with("i_agree_to_the_privacy_policy", json!("yes")),
            "i_agree_to_the_privacy_policy",
        ),
        (json!({}), "licence_plate"),
    ];
    for (answers, key) in cases {
        let body = with_answers(&et_id, &monday_at("10:00"), "v@test.com", answers);
        let (status, answer) = send(&app, api_post("/api/v1/bookings", &token, &body)).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
        assert_eq!(error_code(&answer), "invalid_request", "{body}");
        let message = answer["error"]["message"].as_str().unwrap();
        assert!(message.contains(key), "{message} names {key}");
    }
    // An unknown key too long to be any question's is not echoed back.
    let long = "k".repeat(1000);
    let body = with_answers(
        &et_id,
        &monday_at("10:00"),
        "v@test.com",
        with(&long, json!("x")),
    );
    let (status, answer) = send(&app, api_post("/api/v1/bookings", &token, &body)).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(!answer["error"]["message"].as_str().unwrap().contains(&long));
    // Not an object at all.
    let body = with_answers(&et_id, &monday_at("10:00"), "v@test.com", json!(["AB-123"]));
    let (status, answer) = send(&app, api_post("/api/v1/bookings", &token, &body)).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(error_code(&answer), "invalid_json");
    // Nothing was booked, stored or counted against the daily cap.
    assert_eq!(count(&pool, "bookings").await, 0);
    assert_eq!(count(&pool, "booking_answers").await, 0);
    assert_eq!(count(&pool, "api_booking_log").await, 0);

    // At the limits, in a body over the 16 KiB of other requests: accepted.
    let mut answers = base.clone();
    answers["licence_plate"] = json!("é".repeat(500));
    answers["anything_else"] = json!("€".repeat(5000));
    let mut body = with_answers(&et_id, &monday_at("10:00"), "v@test.com", answers);
    body["notes"] = json!("x".repeat(5000));
    assert!(body.to_string().len() > 16 * 1024);
    let (status, answer) = send(&app, api_post("/api/v1/bookings", &token, &body)).await;
    assert_eq!(status, StatusCode::CREATED, "{answer}");
    // The required ones only: the optional checkbox is stored as not ticked.
    let values: Vec<String> = stored(&pool).await.into_iter().map(|r| r.2).collect();
    assert_eq!(values.len(), 4);
    assert_eq!(values[2], "yes");
    assert_eq!(values[3], "no");
}

#[tokio::test]
async fn api_bookings_without_answers_ask_no_question() {
    let _lock = crate::test_support::ENV_LOCK.lock().await;
    let _env = super::oauth::environment();
    let (app, pool, _, et_id) = setup_test_app().await;
    seed(&pool, &et_id).await;
    // Notes hidden on the booking page: the API keeps them all the same.
    crate::web::questions::save(&pool, &et_id, None, Some("off"))
        .await
        .unwrap();
    let token = pat(&pool, &user_id(&pool, "testuser").await, ALL).await;
    let mut body = create_body(&et_id, &monday_at("10:00"), "old@test.com");
    body["notes"] = json!("Kept");
    let (status, answer) = send(&app, api_post("/api/v1/bookings", &token, &body)).await;
    assert_eq!(status, StatusCode::CREATED, "{answer}");
    assert_eq!(answer["booking"]["answers"], json!([]));
    assert_eq!(answer["booking"]["guest"]["notes"], "Kept");
    assert_eq!(count(&pool, "booking_answers").await, 0);
    // `null` reads as left out.
    let mut body = create_body(&et_id, &monday_at("11:00"), "null@test.com");
    body["answers"] = Value::Null;
    let (status, _) = send(&app, api_post("/api/v1/bookings", &token, &body)).await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(count(&pool, "booking_answers").await, 0);
}

#[tokio::test]
async fn api_team_bookings_take_answers_too() {
    let _lock = crate::test_support::ENV_LOCK.lock().await;
    let _env = super::oauth::environment();
    let (app, pool, _, _) = setup_test_app().await;
    let (_, et_id) = seed_team_event_type(&pool).await;
    seed(&pool, &et_id).await;
    let token = pat(&pool, &user_id(&pool, "testuser").await, ALL).await;
    let body = with_answers(&et_id, &monday_at("10:00"), "team@test.com", full_answers());
    let (status, answer) = send(&app, api_post("/api/v1/bookings", &token, &body)).await;
    assert_eq!(status, StatusCode::CREATED, "{answer}");
    assert_eq!(stored(&pool).await.len(), 6);
    let body = with_answers(&et_id, &monday_at("11:00"), "team@test.com", json!({}));
    let (status, answer) = send(&app, api_post("/api/v1/bookings", &token, &body)).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(answer["error"]["message"]
        .as_str()
        .unwrap()
        .contains("licence_plate"));
}

#[tokio::test]
async fn the_idempotency_key_covers_the_answers() {
    let _lock = crate::test_support::ENV_LOCK.lock().await;
    let _env = super::oauth::environment();
    let (app, pool, _, et_id) = setup_test_app().await;
    seed(&pool, &et_id).await;
    let token = pat(&pool, &user_id(&pool, "testuser").await, ALL).await;
    let keyed = |body: &Value| {
        let mut request = api_post("/api/v1/bookings", &token, body);
        request
            .headers_mut()
            .insert("idempotency-key", "answers-1".parse().unwrap());
        request
    };
    let body = with_answers(&et_id, &monday_at("10:00"), "idem@test.com", full_answers());
    let (status, first) = send(&app, keyed(&body)).await;
    assert_eq!(status, StatusCode::CREATED, "{first}");
    let response = app.clone().oneshot(keyed(&body)).await.unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
    assert_eq!(response.headers()["idempotency-replayed"], "true");
    let replayed: Value = serde_json::from_str(&body_string(response).await).unwrap();
    assert_eq!(replayed["booking"]["answers"], first["booking"]["answers"]);

    // The same key with other answers is another request.
    let mut answers = full_answers();
    answers["licence_plate"] = json!("ZZ-999");
    let other = with_answers(&et_id, &monday_at("10:00"), "idem@test.com", answers);
    let (status, answer) = send(&app, keyed(&other)).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(error_code(&answer), "idempotency_key_reused");
    assert_eq!(count(&pool, "bookings").await, 1);
    assert_eq!(count(&pool, "booking_answers").await, 6);
}
