//! Lead capture: the public endpoint, the leads page, the admin card, the
//! per-event-type toggle and the abandonment alert.

use super::*;

const CSRF: &str = "csrf-leads";

/// Turn lead capture on for the seeded event type, globally and per event type.
async fn enable_capture(pool: &SqlitePool, et_id: &str) {
    sqlx::query("UPDATE auth_config SET lead_capture_enabled = 1 WHERE id = 'singleton'")
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("UPDATE event_types SET lead_capture = 1 WHERE id = ?")
        .bind(et_id)
        .execute(pool)
        .await
        .unwrap();
}

/// `POST /api/lead-capture` the way the page script does: JSON, the CSRF
/// cookie and the same token in the body.
fn capture_request(body: serde_json::Value) -> axum::http::Request<Body> {
    capture_request_with(body, CSRF, CSRF)
}

fn capture_request_with(
    mut body: serde_json::Value,
    cookie: &str,
    token: &str,
) -> axum::http::Request<Body> {
    body["_csrf"] = token.into();
    axum::http::Request::builder()
        .method("POST")
        .uri("/api/lead-capture")
        .header("cookie", format!("__Host-calrs_csrf={cookie}"))
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap()
}

fn payload(lead_id: &str) -> serde_json::Value {
    serde_json::json!({
        "lead_id": lead_id,
        "event_type_slug": "test-meeting",
        "username": "testuser",
        "name": "Pat Guest",
        "email": "pat@example.com",
        "target_date": "2026-11-02",
        "target_time": "10:00",
    })
}

async fn lead_count(pool: &SqlitePool) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM partial_bookings")
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn insert_lead(pool: &SqlitePool, et_id: &str, host: Option<&str>) -> String {
    let id = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO partial_bookings (id, event_type_id, host_user_id, lead_id, email)
         VALUES (?, ?, ?, ?, 'x@example.com')",
    )
    .bind(&id)
    .bind(et_id)
    .bind(host)
    .bind(uuid::Uuid::new_v4().to_string())
    .execute(pool)
    .await
    .unwrap();
    id
}

// --- The public endpoint ----------------------------------------------------

#[tokio::test]
async fn capture_records_what_the_guest_typed() {
    let (app, pool, _, et_id) = setup_test_app().await;
    enable_capture(&pool, &et_id).await;

    let lead_id = uuid::Uuid::new_v4().to_string();
    let response = app
        .oneshot(capture_request(payload(&lead_id)))
        .await
        .unwrap();
    assert_eq!(response.status(), 204);

    let (event_type, host, email, name): (String, Option<String>, String, String) = sqlx::query_as(
        "SELECT event_type_id, host_user_id, email, name FROM partial_bookings WHERE lead_id = ?",
    )
    .bind(&lead_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(event_type, et_id);
    assert!(host.is_some(), "the host is resolved from the page");
    assert_eq!(email, "pat@example.com");
    assert_eq!(name, "Pat Guest");
}

#[tokio::test]
async fn capture_needs_the_csrf_token() {
    let (app, pool, _, et_id) = setup_test_app().await;
    enable_capture(&pool, &et_id).await;
    let id = uuid::Uuid::new_v4().to_string();

    let response = app
        .clone()
        .oneshot(capture_request_with(payload(&id), CSRF, "another-token"))
        .await
        .unwrap();
    assert_eq!(response.status(), 403);

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .method("POST")
                .uri("/api/lead-capture")
                .header("content-type", "application/json")
                .body(Body::from(payload(&id).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 403);
    assert_eq!(lead_count(&pool).await, 0);
}

#[tokio::test]
async fn capture_records_nothing_while_either_switch_is_off() {
    let (app, pool, _, et_id) = setup_test_app().await;

    // Event type opted in, admin switch off.
    sqlx::query("UPDATE event_types SET lead_capture = 1 WHERE id = ?")
        .bind(&et_id)
        .execute(&pool)
        .await
        .unwrap();
    let response = app
        .clone()
        .oneshot(capture_request(payload(&uuid::Uuid::new_v4().to_string())))
        .await
        .unwrap();
    assert_eq!(
        response.status(),
        204,
        "a stale tab is told nothing alarming"
    );
    assert_eq!(lead_count(&pool).await, 0);

    // Admin switch on, event type opted out.
    enable_capture(&pool, &et_id).await;
    sqlx::query("UPDATE event_types SET lead_capture = 0 WHERE id = ?")
        .bind(&et_id)
        .execute(&pool)
        .await
        .unwrap();
    let response = app
        .oneshot(capture_request(payload(&uuid::Uuid::new_v4().to_string())))
        .await
        .unwrap();
    assert_eq!(response.status(), 204);
    assert_eq!(lead_count(&pool).await, 0);
}

#[tokio::test]
async fn capture_refuses_a_lead_id_that_is_not_an_id() {
    let (app, pool, _, et_id) = setup_test_app().await;
    enable_capture(&pool, &et_id).await;
    for bad in [
        "",
        "short",
        "<script>alert(1)</script>",
        "has a space",
        &"a".repeat(65),
    ] {
        let response = app
            .clone()
            .oneshot(capture_request(payload(bad)))
            .await
            .unwrap();
        assert_eq!(response.status(), 400, "{bad:?}");
    }
    assert_eq!(lead_count(&pool).await, 0);
}

#[tokio::test]
async fn capture_leaves_private_and_unknown_event_types_alone() {
    let (app, pool, _, et_id) = setup_test_app().await;
    enable_capture(&pool, &et_id).await;

    let mut unknown = payload(&uuid::Uuid::new_v4().to_string());
    unknown["event_type_slug"] = "no-such-event".into();
    let response = app.clone().oneshot(capture_request(unknown)).await.unwrap();
    assert_eq!(response.status(), 404);

    sqlx::query("UPDATE event_types SET visibility = 'private' WHERE id = ?")
        .bind(&et_id)
        .execute(&pool)
        .await
        .unwrap();
    let response = app
        .oneshot(capture_request(payload(&uuid::Uuid::new_v4().to_string())))
        .await
        .unwrap();
    assert_eq!(response.status(), 404, "private pages need an invite");
    assert_eq!(lead_count(&pool).await, 0);
}

/// Two hosts may use the same slug: the lead goes to the host whose page the
/// guest is on, not to whichever row the database returns first.
#[tokio::test]
async fn capture_binds_the_lead_to_the_host_of_the_page() {
    let (app, pool, _, first_et) = setup_test_app().await;
    let (other_user, _) =
        insert_test_user_with_session(&pool, "Other Host", "other@example.com", "otherhost").await;
    let other_account = uuid::Uuid::new_v4().to_string();
    let other_et = uuid::Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO accounts (id, name, email, timezone, user_id) VALUES (?, 'Other', 'other@example.com', 'UTC', ?)")
        .bind(&other_account)
        .bind(&other_user)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO event_types (id, account_id, slug, title, duration_min, enabled, lead_capture) VALUES (?, ?, 'test-meeting', 'Other Meeting', 30, 1, 1)")
        .bind(&other_et)
        .bind(&other_account)
        .execute(&pool)
        .await
        .unwrap();
    enable_capture(&pool, &first_et).await;

    let lead_id = uuid::Uuid::new_v4().to_string();
    let mut body = payload(&lead_id);
    body["username"] = "otherhost".into();
    let response = app.oneshot(capture_request(body)).await.unwrap();
    assert_eq!(response.status(), 204);

    let (event_type, host): (String, Option<String>) = sqlx::query_as(
        "SELECT event_type_id, host_user_id FROM partial_bookings WHERE lead_id = ?",
    )
    .bind(&lead_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(event_type, other_et);
    assert_eq!(host.as_deref(), Some(other_user.as_str()));
}

#[tokio::test]
async fn capture_throttles_one_ip() {
    let (app, pool, _, et_id) = setup_test_app().await;
    enable_capture(&pool, &et_id).await;
    let id = uuid::Uuid::new_v4().to_string();
    let mut last = 204;
    for _ in 0..=crate::leads::limits::MAX_REQUESTS_PER_MINUTE {
        let response = app
            .clone()
            .oneshot(capture_request(payload(&id)))
            .await
            .unwrap();
        last = response.status().as_u16();
    }
    assert_eq!(last, 429);
}

// --- Closing a lead with the booking -----------------------------------------

#[tokio::test]
async fn booking_closes_the_lead_it_came_from() {
    let (app, pool, _, et_id) = setup_test_app().await;
    enable_capture(&pool, &et_id).await;
    let lead_id = uuid::Uuid::new_v4().to_string();
    app.clone()
        .oneshot(capture_request(payload(&lead_id)))
        .await
        .unwrap();

    let mut monday = Utc::now().date_naive() + Duration::days(1);
    while monday.weekday() != chrono::Weekday::Mon {
        monday += Duration::days(1);
    }
    let form = format!(
        "_csrf={CSRF}&date={}&time=10%3A00&name=Pat+Guest&email=pat%40example.com&lead_id={lead_id}",
        monday.format("%Y-%m-%d")
    );
    let response = app
        .oneshot(post_form_unauthed(
            "/u/testuser/test-meeting/book",
            CSRF,
            &form,
        ))
        .await
        .unwrap();
    assert!(response.status() == 200 || response.status().is_redirection());

    let completed: Option<String> =
        sqlx::query_scalar("SELECT completed_at FROM partial_bookings WHERE lead_id = ?")
            .bind(&lead_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(completed.is_some());
}

// --- The pages guests see ------------------------------------------------------

#[tokio::test]
async fn slots_page_shows_the_gate_only_when_capture_is_active() {
    let (app, pool, _, et_id) = setup_test_app().await;
    let page = |app: Router| async move {
        body_string(app.oneshot(get("/u/testuser/test-meeting")).await.unwrap()).await
    };

    assert!(!page(app.clone()).await.contains("id=\"lead-gate\""));

    sqlx::query("UPDATE event_types SET lead_capture = 1 WHERE id = ?")
        .bind(&et_id)
        .execute(&pool)
        .await
        .unwrap();
    assert!(
        !page(app.clone()).await.contains("id=\"lead-gate\""),
        "the admin switch is the kill switch"
    );

    enable_capture(&pool, &et_id).await;
    let body = page(app).await;
    assert!(body.contains("id=\"lead-gate\""));
    assert!(
        !body.contains("id=\"lead-gate-phone\""),
        "no phone field while the event type collects none"
    );
}

#[tokio::test]
async fn gate_asks_for_a_phone_when_the_event_type_collects_one() {
    let (app, pool, _, et_id) = setup_test_app().await;
    enable_capture(&pool, &et_id).await;
    sqlx::query("UPDATE event_types SET sms_phone_mode = 'optional' WHERE id = ?")
        .bind(&et_id)
        .execute(&pool)
        .await
        .unwrap();
    let body = body_string(app.oneshot(get("/u/testuser/test-meeting")).await.unwrap()).await;
    assert!(body.contains("id=\"lead-gate-phone\""));
}

#[tokio::test]
async fn booking_pages_carry_the_privacy_notice_or_the_admin_link() {
    let (app, pool, _, et_id) = setup_test_app().await;
    enable_capture(&pool, &et_id).await;

    let body = body_string(
        app.clone()
            .oneshot(get("/u/testuser/test-meeting"))
            .await
            .unwrap(),
    )
    .await;
    assert!(
        body.contains("<details class=\"lead-capture-notice inline\">"),
        "inline notice by default"
    );

    sqlx::query("UPDATE auth_config SET legal_mentions_url = 'https://example.com/privacy' WHERE id = 'singleton'")
        .execute(&pool)
        .await
        .unwrap();
    let body = body_string(app.oneshot(get("/u/testuser/test-meeting")).await.unwrap()).await;
    assert!(
        body.contains("example.com&#x2f;privacy"),
        "the admin's link replaces the inline notice"
    );
    assert!(!body.contains("<details class=\"lead-capture-notice inline\">"));
}

// --- The leads page and its worklist -------------------------------------------

#[tokio::test]
async fn leads_page_renders_and_follows_the_user_language() {
    let (app, pool, session, et_id) = setup_test_app().await;
    insert_lead(&pool, &et_id, None).await;

    let body = body_string(
        app.clone()
            .oneshot(get_authed("/dashboard/leads", &session))
            .await
            .unwrap(),
    )
    .await;
    assert!(body.contains("Conversion"));
    assert!(body.contains("x@example.com"));

    sqlx::query("UPDATE users SET language = 'fr'")
        .execute(&pool)
        .await
        .unwrap();
    let body = body_string(
        app.oneshot(get_authed("/dashboard/leads", &session))
            .await
            .unwrap(),
    )
    .await;
    assert!(body.contains("Prospects"), "{body}");
    assert!(!body.contains("Open leads"));
}

#[tokio::test]
async fn leads_page_only_links_plain_addresses_and_numbers() {
    let (app, pool, session, et_id) = setup_test_app().await;
    let id = insert_lead(&pool, &et_id, None).await;
    sqlx::query("UPDATE partial_bookings SET email = ?, phone = ?, name = ? WHERE id = ?")
        .bind("pat@example.com?bcc=boss@example.com")
        .bind("javascript:alert(1)")
        .bind("<script>alert(1)</script>")
        .bind(&id)
        .execute(&pool)
        .await
        .unwrap();
    let body = body_string(
        app.oneshot(get_authed("/dashboard/leads", &session))
            .await
            .unwrap(),
    )
    .await;
    assert!(!body.contains("mailto:pat@example.com?bcc"), "{body}");
    assert!(!body.contains("href=\"tel:javascript"));
    assert!(
        !body.contains("<script>alert(1)</script>"),
        "guest input is escaped"
    );
}

#[tokio::test]
async fn a_stranger_cannot_touch_someone_elses_lead() {
    let (app, pool, _, et_id) = setup_test_app().await;
    let host: String = sqlx::query_scalar("SELECT id FROM users WHERE username = 'testuser'")
        .fetch_one(&pool)
        .await
        .unwrap();
    let lead = insert_lead(&pool, &et_id, Some(&host)).await;
    let (_, stranger) =
        insert_test_user_with_session(&pool, "Stranger", "s@example.com", "stranger").await;

    for action in ["archive", "contacted"] {
        let response = app
            .clone()
            .oneshot(post_form(
                &format!("/dashboard/leads/{lead}/{action}"),
                &stranger,
                CSRF,
                &format!("_csrf={CSRF}"),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), 403, "{action}");
    }
    let (archived, contacted): (Option<String>, Option<String>) =
        sqlx::query_as("SELECT archived_at, contacted_at FROM partial_bookings WHERE id = ?")
            .bind(&lead)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(archived.is_none() && contacted.is_none());

    // And a stranger's leads page does not list it.
    let body = body_string(
        app.oneshot(get_authed("/dashboard/leads", &stranger))
            .await
            .unwrap(),
    )
    .await;
    assert!(!body.contains("x@example.com"));
}

#[tokio::test]
async fn worklist_needs_the_csrf_token() {
    let (app, pool, session, et_id) = setup_test_app().await;
    let lead = insert_lead(&pool, &et_id, None).await;
    let response = app
        .oneshot(post_form(
            &format!("/dashboard/leads/{lead}/archive"),
            &session,
            CSRF,
            "_csrf=forged",
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), 403);
}

#[tokio::test]
async fn an_admin_can_archive_and_flip_contacted() {
    let (app, pool, session, et_id) = setup_test_app().await;
    let lead = insert_lead(&pool, &et_id, None).await;
    let post = |action: &'static str| {
        post_form(
            &format!("/dashboard/leads/{lead}/{action}"),
            &session,
            CSRF,
            &format!("_csrf={CSRF}"),
        )
    };
    let contacted = || async {
        sqlx::query_scalar::<_, Option<String>>("SELECT contacted_at FROM partial_bookings")
            .fetch_one(&pool)
            .await
            .unwrap()
    };

    app.clone().oneshot(post("contacted")).await.unwrap();
    assert!(contacted().await.is_some());
    app.clone().oneshot(post("contacted")).await.unwrap();
    assert!(contacted().await.is_none(), "a second press clears it");

    let response = app.oneshot(post("archive")).await.unwrap();
    assert!(response.status().is_redirection());
    let archived: Option<String> = sqlx::query_scalar("SELECT archived_at FROM partial_bookings")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(archived.is_some());
}

// --- The admin card --------------------------------------------------------------

#[tokio::test]
async fn admin_saves_the_global_settings_and_clamps_retention() {
    let (app, pool, session, _) = setup_test_app().await;
    let response = app
        .oneshot(post_form(
            "/dashboard/admin/lead-capture",
            &session,
            CSRF,
            &format!("_csrf={CSRF}&enabled=on&retention_days=9999"),
        ))
        .await
        .unwrap();
    assert!(response.status().is_redirection());
    let (enabled, days): (i64, i64) = sqlx::query_as(
        "SELECT lead_capture_enabled, lead_retention_days FROM auth_config WHERE id = 'singleton'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!((enabled, days), (1, 365));
}

#[tokio::test]
async fn only_an_admin_can_change_the_lead_settings() {
    let (app, pool, _, _) = setup_test_app().await;
    let (_, user) = insert_test_user_with_session(&pool, "User", "u@example.com", "plain").await;
    for (uri, body) in [
        (
            "/dashboard/admin/lead-capture",
            "enabled=on&retention_days=3",
        ),
        (
            "/dashboard/admin/legal-mentions",
            "legal_mentions_url=https%3A%2F%2Fa.example",
        ),
    ] {
        let response = app
            .clone()
            .oneshot(post_form(uri, &user, CSRF, &format!("_csrf={CSRF}&{body}")))
            .await
            .unwrap();
        assert!(
            response.status() == 403 || response.status().is_redirection(),
            "{uri}: {}",
            response.status()
        );
    }
    let enabled: i64 =
        sqlx::query_scalar("SELECT lead_capture_enabled FROM auth_config WHERE id = 'singleton'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(enabled, 0);
    let url: Option<String> =
        sqlx::query_scalar("SELECT legal_mentions_url FROM auth_config WHERE id = 'singleton'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(url.is_none());
}

#[tokio::test]
async fn legal_mentions_url_must_be_http() {
    let (app, pool, session, _) = setup_test_app().await;
    let save = |url: &'static str| {
        post_form(
            "/dashboard/admin/legal-mentions",
            &session,
            CSRF,
            &format!("_csrf={CSRF}&legal_mentions_url={url}"),
        )
    };
    let stored = || async {
        sqlx::query_scalar::<_, Option<String>>(
            "SELECT legal_mentions_url FROM auth_config WHERE id = 'singleton'",
        )
        .fetch_one(&pool)
        .await
        .unwrap()
    };

    let response = app
        .clone()
        .oneshot(save("javascript%3Aalert(1)"))
        .await
        .unwrap();
    assert!(response.status().is_redirection());
    assert!(response.headers()["location"]
        .to_str()
        .unwrap()
        .contains("error="));
    assert!(
        stored().await.is_none(),
        "a javascript: URL is never stored"
    );

    app.clone()
        .oneshot(save("https%3A%2F%2Fexample.com%2Fprivacy"))
        .await
        .unwrap();
    assert_eq!(
        stored().await.as_deref(),
        Some("https://example.com/privacy")
    );

    app.oneshot(save("")).await.unwrap();
    assert!(stored().await.is_none(), "an empty URL clears it");
}

// --- The per-event-type toggle ---------------------------------------------------

mod toggle {
    use super::*;
    use crate::web::leads::ToggleError;

    #[tokio::test]
    async fn enabling_needs_the_acknowledgement_once() {
        let (_, pool, _, et_id) = setup_test_app().await;
        let apply = |on, ack| crate::web::leads::apply_toggle(&pool, &et_id, on, ack);

        assert_eq!(apply(true, false).await, Err(ToggleError::NotAcknowledged));
        assert!(apply(true, true).await.is_ok());

        let stamp = || async {
            sqlx::query_scalar::<_, Option<String>>(
                "SELECT lead_capture_acknowledged_at FROM event_types WHERE id = ?",
            )
            .bind(&et_id)
            .fetch_one(&pool)
            .await
            .unwrap()
        };
        let first = stamp().await;
        assert!(first.is_some());

        // Off and on again needs no new acknowledgement and keeps the stamp.
        assert!(apply(false, false).await.is_ok());
        assert!(apply(true, false).await.is_ok());
        assert_eq!(stamp().await, first);
    }

    #[tokio::test]
    async fn an_unticked_box_never_stamps_the_acknowledgement() {
        let (_, pool, _, et_id) = setup_test_app().await;
        crate::web::leads::apply_toggle(&pool, &et_id, false, true)
            .await
            .unwrap();
        let stamp: Option<String> =
            sqlx::query_scalar("SELECT lead_capture_acknowledged_at FROM event_types WHERE id = ?")
                .bind(&et_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert!(
            stamp.is_none(),
            "acknowledging while leaving it off proves nothing"
        );
    }

    #[tokio::test]
    async fn an_event_type_that_does_not_exist_is_an_error_not_a_success() {
        let (_, pool, _, _) = setup_test_app().await;
        assert_eq!(
            crate::web::leads::apply_toggle(&pool, "missing", true, true).await,
            Err(ToggleError::Database)
        );
    }

    #[tokio::test]
    async fn the_event_type_form_saves_the_toggle_and_refuses_without_acknowledgement() {
        let (app, pool, session, et_id) = setup_test_app().await;
        let form = |extra: &str| {
            format!(
                "_csrf={CSRF}&title=Test+Meeting&slug=test-meeting&duration_min=30&location_type=link&location_value=https%3A%2F%2Fmeet.example.com&{extra}"
            )
        };
        let enabled = || async {
            sqlx::query_scalar::<_, i64>("SELECT lead_capture FROM event_types WHERE id = ?")
                .bind(&et_id)
                .fetch_one(&pool)
                .await
                .unwrap()
        };

        let response = app
            .clone()
            .oneshot(post_form(
                "/dashboard/event-types/test-meeting/edit",
                &session,
                CSRF,
                &form("lead_capture=on"),
            ))
            .await
            .unwrap();
        let body = body_string(response).await;
        assert_eq!(enabled().await, 0, "refused without the acknowledgement");
        assert!(
            body.contains("Confirm that bookers are informed"),
            "{}",
            body.split("class=\"error\"")
                .nth(1)
                .unwrap_or("no error banner")
                .chars()
                .take(300)
                .collect::<String>()
        );

        let response = app
            .oneshot(post_form(
                "/dashboard/event-types/test-meeting/edit",
                &session,
                CSRF,
                &form("lead_capture=on&lead_capture_acknowledged=on"),
            ))
            .await
            .unwrap();
        assert!(response.status().is_redirection(), "{}", response.status());
        assert_eq!(enabled().await, 1);
    }
}

// --- The abandonment alert -------------------------------------------------------

/// Age a lead so the notifier considers it abandoned.
async fn abandon(pool: &SqlitePool, lead: &str) {
    sqlx::query(
        "UPDATE partial_bookings SET updated_at = datetime('now', '-60 minutes') WHERE id = ?",
    )
    .bind(lead)
    .execute(pool)
    .await
    .unwrap();
}

#[tokio::test]
async fn the_host_is_told_once_about_an_abandoned_booking() {
    let _env = crate::test_support::ENV_LOCK.lock().await;
    let (_, pool, _, et_id) = setup_test_app().await;
    enable_capture(&pool, &et_id).await;
    let (port, mails) = spawn_capturing_mta().await;
    use_capturing_mta(&pool, port).await;
    let host: String = sqlx::query_scalar("SELECT id FROM users WHERE username = 'testuser'")
        .fetch_one(&pool)
        .await
        .unwrap();
    let lead = insert_lead(&pool, &et_id, Some(&host)).await;
    sqlx::query(
        "UPDATE partial_bookings SET name = '<b>Pat</b>', email = 'pat@example.com' WHERE id = ?",
    )
    .bind(&lead)
    .execute(&pool)
    .await
    .unwrap();
    abandon(&pool, &lead).await;

    crate::web::leads::notify_abandoned(&pool, &[0u8; 32], &mut Default::default()).await;
    let mail = mail_to(&mails, "test@example.com").await;
    assert!(mail.contains("Abandoned booking"), "{mail}");
    assert!(mail.contains("pat@example.com"));
    assert!(
        mail.contains("&lt;b&gt;Pat&lt;/b&gt;"),
        "guest input is escaped in the HTML: {mail}"
    );
    let subject = mail.lines().find(|l| l.starts_with("Subject:")).unwrap();
    assert!(
        !subject.contains("Pat"),
        "nothing the guest typed in the subject"
    );

    // The second pass finds nothing left to say.
    crate::web::leads::notify_abandoned(&pool, &[0u8; 32], &mut Default::default()).await;
    assert_eq!(mails.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn no_alert_while_the_admin_switch_is_off() {
    let _env = crate::test_support::ENV_LOCK.lock().await;
    let (_, pool, _, et_id) = setup_test_app().await;
    let (port, mails) = spawn_capturing_mta().await;
    use_capturing_mta(&pool, port).await;
    let host: String = sqlx::query_scalar("SELECT id FROM users WHERE username = 'testuser'")
        .fetch_one(&pool)
        .await
        .unwrap();
    let lead = insert_lead(&pool, &et_id, Some(&host)).await;
    abandon(&pool, &lead).await;

    crate::web::leads::notify_abandoned(&pool, &[0u8; 32], &mut Default::default()).await;
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    assert!(mails.lock().unwrap().is_empty());
}

#[tokio::test]
async fn a_host_gets_a_bounded_number_of_alerts_an_hour() {
    let _env = crate::test_support::ENV_LOCK.lock().await;
    let (_, pool, _, et_id) = setup_test_app().await;
    enable_capture(&pool, &et_id).await;
    let (port, mails) = spawn_capturing_mta().await;
    use_capturing_mta(&pool, port).await;
    let host: String = sqlx::query_scalar("SELECT id FROM users WHERE username = 'testuser'")
        .fetch_one(&pool)
        .await
        .unwrap();
    for _ in 0..25 {
        let lead = insert_lead(&pool, &et_id, Some(&host)).await;
        abandon(&pool, &lead).await;
    }

    crate::web::leads::notify_abandoned(&pool, &[0u8; 32], &mut Default::default()).await;
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    let sent = mails.lock().unwrap().len();
    assert_eq!(sent, 10, "capped at ten an hour per host");
}

// --- Deleting, restoring, views and search -------------------------------------------

async fn host_id(pool: &SqlitePool) -> String {
    sqlx::query_scalar("SELECT id FROM users WHERE username = 'testuser'")
        .fetch_one(pool)
        .await
        .unwrap()
}

fn action(session: &str, lead: &str, action: &str, extra: &str) -> axum::http::Request<Body> {
    post_form(
        &format!("/dashboard/leads/{lead}/{action}"),
        session,
        CSRF,
        &format!("_csrf={CSRF}{extra}"),
    )
}

fn location(response: &axum::http::Response<Body>) -> String {
    response.headers()["location"].to_str().unwrap().to_string()
}

#[tokio::test]
async fn a_host_can_delete_a_lead_in_any_state_and_lands_back_where_they_were() {
    let (app, pool, session, et_id) = setup_test_app().await;
    let host = host_id(&pool).await;
    for state in [
        "",
        "archived_at = datetime('now')",
        "completed_at = datetime('now')",
    ] {
        let lead = insert_lead(&pool, &et_id, Some(&host)).await;
        if !state.is_empty() {
            sqlx::query(&format!("UPDATE partial_bookings SET {state} WHERE id = ?"))
                .bind(&lead)
                .execute(&pool)
                .await
                .unwrap();
        }
        let response = app
            .clone()
            .oneshot(action(
                &session,
                &lead,
                "delete",
                "&view=archived&q=pat%20x",
            ))
            .await
            .unwrap();
        assert!(response.status().is_redirection(), "{state}");
        assert_eq!(
            location(&response),
            "/dashboard/leads?view=archived&q=pat%20x"
        );
    }
    assert_eq!(lead_count(&pool).await, 0);
}

#[tokio::test]
async fn the_redirect_after_an_action_cannot_be_steered() {
    let (app, pool, session, et_id) = setup_test_app().await;
    let lead = insert_lead(&pool, &et_id, None).await;
    let response = app
        .oneshot(action(
            &session,
            &lead,
            "contacted",
            "&view=https%3A%2F%2Fevil.example&q=%0d%0aSet-Cookie%3A%20a%3Db",
        ))
        .await
        .unwrap();
    let to = location(&response);
    assert!(to.starts_with("/dashboard/leads?view=open"), "{to}");
    assert!(!to.contains('\n') && !to.contains('\r'));
}

#[tokio::test]
async fn deleting_needs_rights_and_the_csrf_token() {
    let (app, pool, session, et_id) = setup_test_app().await;
    let host = host_id(&pool).await;
    let lead = insert_lead(&pool, &et_id, Some(&host)).await;
    let (_, stranger) =
        insert_test_user_with_session(&pool, "Stranger", "s@example.com", "stranger").await;

    let response = app
        .clone()
        .oneshot(action(&stranger, &lead, "delete", ""))
        .await
        .unwrap();
    assert_eq!(response.status(), 403);

    let response = app
        .clone()
        .oneshot(post_form(
            &format!("/dashboard/leads/{lead}/delete"),
            &session,
            CSRF,
            "_csrf=forged",
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), 403);

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .method("POST")
                .uri(format!("/dashboard/leads/{lead}/delete"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert!(
        response.status() == 401 || response.status() == 403 || response.status().is_redirection(),
        "{}",
        response.status()
    );
    assert_eq!(lead_count(&pool).await, 1, "nothing was deleted");
}

#[tokio::test]
async fn archived_leads_can_be_listed_restored_and_searched() {
    let (app, pool, session, et_id) = setup_test_app().await;
    let host = host_id(&pool).await;
    let lead = insert_lead(&pool, &et_id, Some(&host)).await;
    sqlx::query(
        "UPDATE partial_bookings SET name = 'Pat Archive', email = 'pat@arch.example' WHERE id = ?",
    )
    .bind(&lead)
    .execute(&pool)
    .await
    .unwrap();
    app.clone()
        .oneshot(action(&session, &lead, "archive", ""))
        .await
        .unwrap();

    let page = |uri: &'static str| {
        let app = app.clone();
        let session = session.clone();
        async move { body_string(app.oneshot(get_authed(uri, &session)).await.unwrap()).await }
    };
    assert!(!page("/dashboard/leads").await.contains("pat@arch.example"));
    assert!(page("/dashboard/leads?view=archived")
        .await
        .contains("pat@arch.example"));
    assert!(page("/dashboard/leads?view=archived&q=ARCH")
        .await
        .contains("pat@arch.example"));
    assert!(!page("/dashboard/leads?view=archived&q=nobody")
        .await
        .contains("pat@arch.example"));
    // `%` and `_` are plain characters, not wildcards.
    assert!(!page("/dashboard/leads?view=archived&q=%25")
        .await
        .contains("pat@arch.example"));
    assert!(!page("/dashboard/leads?view=archived&q=_")
        .await
        .contains("pat@arch.example"));

    app.clone()
        .oneshot(action(&session, &lead, "unarchive", "&view=archived"))
        .await
        .unwrap();
    assert!(page("/dashboard/leads").await.contains("pat@arch.example"));
}

#[tokio::test]
async fn the_booked_tab_lists_leads_that_turned_into_a_booking() {
    let (app, pool, session, et_id) = setup_test_app().await;
    let lead = insert_lead(&pool, &et_id, None).await;
    sqlx::query("UPDATE partial_bookings SET completed_at = datetime('now') WHERE id = ?")
        .bind(&lead)
        .execute(&pool)
        .await
        .unwrap();
    let get_page = |uri: &'static str| {
        let (app, session) = (app.clone(), session.clone());
        async move { body_string(app.oneshot(get_authed(uri, &session)).await.unwrap()).await }
    };
    assert!(!get_page("/dashboard/leads").await.contains("x@example.com"));
    assert!(get_page("/dashboard/leads?view=completed")
        .await
        .contains("x@example.com"));
}

// --- Combined pages and teams -----------------------------------------------------------

async fn second_host(pool: &SqlitePool) -> String {
    let (user, _) =
        insert_test_user_with_session(pool, "Second Host", "second@example.com", "secondhost")
            .await;
    let account = uuid::Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO accounts (id, name, email, timezone, user_id) VALUES (?, 'Second', 'second@example.com', 'UTC', ?)")
        .bind(&account)
        .bind(&user)
        .execute(pool)
        .await
        .unwrap();
    user
}

#[tokio::test]
async fn a_combined_page_records_the_lead_for_the_first_user() {
    let (app, pool, _, et_id) = setup_test_app().await;
    second_host(&pool).await;
    enable_capture(&pool, &et_id).await;

    let lead_id = uuid::Uuid::new_v4().to_string();
    let mut body = payload(&lead_id);
    body["username"] = "testuser+secondhost".into();
    let response = app.clone().oneshot(capture_request(body)).await.unwrap();
    assert_eq!(response.status(), 204);
    let (event_type, host): (String, Option<String>) = sqlx::query_as(
        "SELECT event_type_id, host_user_id FROM partial_bookings WHERE lead_id = ?",
    )
    .bind(&lead_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(event_type, et_id);
    assert_eq!(host, Some(host_id(&pool).await));

    // The page carries the gate, and the second user is not the owner.
    let page = body_string(
        app.clone()
            .oneshot(get("/u/testuser+secondhost/test-meeting"))
            .await
            .unwrap(),
    )
    .await;
    assert!(page.contains("id=\"lead-gate\""));
    let mut body = payload(&uuid::Uuid::new_v4().to_string());
    body["username"] = "secondhost+testuser".into();
    let response = app.oneshot(capture_request(body)).await.unwrap();
    assert_eq!(response.status(), 404, "secondhost has no such event type");
}

#[tokio::test]
async fn a_combined_page_needs_every_user_to_exist_and_allow_it() {
    let (app, pool, _, et_id) = setup_test_app().await;
    let second = second_host(&pool).await;
    enable_capture(&pool, &et_id).await;
    for username in ["testuser+nobody", "testuser+testuser", "testuser+"] {
        let mut body = payload(&uuid::Uuid::new_v4().to_string());
        body["username"] = username.into();
        let response = app.clone().oneshot(capture_request(body)).await.unwrap();
        assert_eq!(response.status(), 404, "{username}");
    }
    sqlx::query("UPDATE users SET allow_dynamic_group = 0 WHERE id = ?")
        .bind(&second)
        .execute(&pool)
        .await
        .unwrap();
    let mut body = payload(&uuid::Uuid::new_v4().to_string());
    body["username"] = "testuser+secondhost".into();
    let response = app.oneshot(capture_request(body)).await.unwrap();
    assert_eq!(
        response.status(),
        404,
        "a user who opted out of combined links"
    );
    assert_eq!(lead_count(&pool).await, 0);
}

#[tokio::test]
async fn a_combined_booking_closes_the_lead() {
    let (app, pool, _, et_id) = setup_test_app().await;
    second_host(&pool).await;
    enable_capture(&pool, &et_id).await;
    let lead_id = uuid::Uuid::new_v4().to_string();
    let mut body = payload(&lead_id);
    body["username"] = "testuser+secondhost".into();
    app.clone().oneshot(capture_request(body)).await.unwrap();

    let mut monday = Utc::now().date_naive() + Duration::days(1);
    while monday.weekday() != chrono::Weekday::Mon {
        monday += Duration::days(1);
    }
    let form = format!(
        "_csrf={CSRF}&date={}&time=10%3A00&name=Pat&email=pat%40example.com&lead_id={lead_id}",
        monday.format("%Y-%m-%d")
    );
    let response = app
        .oneshot(post_form_unauthed(
            "/u/testuser+secondhost/test-meeting/book",
            CSRF,
            &form,
        ))
        .await
        .unwrap();
    // The second user has no calendar rules in this fixture, so the booking
    // itself may be refused; what matters is that a created booking closes it.
    let booked: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM bookings WHERE guest_email = 'pat@example.com'")
            .fetch_one(&pool)
            .await
            .unwrap();
    let completed: Option<String> =
        sqlx::query_scalar("SELECT completed_at FROM partial_bookings WHERE lead_id = ?")
            .bind(&lead_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(booked > 0, completed.is_some(), "{}", response.status());
}

/// A team event type owned by `testuser`; returns (team id, event type id).
async fn team_event_type(pool: &SqlitePool, visibility: &str) -> (String, String) {
    let host = host_id(pool).await;
    let account: String = sqlx::query_scalar("SELECT id FROM accounts LIMIT 1")
        .fetch_one(pool)
        .await
        .unwrap();
    let team = uuid::Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO teams (id, name, slug, visibility) VALUES (?, 'Crew', 'crew', ?)")
        .bind(&team)
        .bind(visibility)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO team_members (team_id, user_id, role, source) VALUES (?, ?, 'admin', 'direct')")
        .bind(&team)
        .bind(&host)
        .execute(pool)
        .await
        .unwrap();
    let et = uuid::Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO event_types (id, account_id, slug, title, duration_min, enabled, lead_capture, team_id, created_by_user_id, visibility) VALUES (?, ?, 'sync', 'Sync', 30, 1, 1, ?, ?, 'public')")
        .bind(&et)
        .bind(&account)
        .bind(&team)
        .bind(&host)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("UPDATE auth_config SET lead_capture_enabled = 1 WHERE id = 'singleton'")
        .execute(pool)
        .await
        .unwrap();
    (team, et)
}

fn team_payload(lead_id: &str) -> serde_json::Value {
    serde_json::json!({
        "lead_id": lead_id,
        "event_type_slug": "sync",
        "team_slug": "crew",
        "email": "pat@example.com",
    })
}

#[tokio::test]
async fn a_public_team_page_records_leads_and_a_private_team_does_not() {
    let (app, pool, _, _) = setup_test_app().await;
    let (team, et) = team_event_type(&pool, "public").await;
    let id = uuid::Uuid::new_v4().to_string();
    let response = app
        .clone()
        .oneshot(capture_request(team_payload(&id)))
        .await
        .unwrap();
    assert_eq!(response.status(), 204);
    let stored: String =
        sqlx::query_scalar("SELECT event_type_id FROM partial_bookings WHERE lead_id = ?")
            .bind(&id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(stored, et);

    // The team goes private: its pages need an invite, so no more capture.
    sqlx::query("UPDATE teams SET visibility = 'private' WHERE id = ?")
        .bind(&team)
        .execute(&pool)
        .await
        .unwrap();
    let before = lead_count(&pool).await;
    let response = app
        .oneshot(capture_request(team_payload(
            &uuid::Uuid::new_v4().to_string(),
        )))
        .await
        .unwrap();
    assert_eq!(response.status(), 204, "told nothing, stores nothing");
    assert_eq!(lead_count(&pool).await, before);
}

#[tokio::test]
async fn team_members_work_the_teams_leads_and_nobody_else_does() {
    let (app, pool, _, _) = setup_test_app().await;
    let (team, et) = team_event_type(&pool, "public").await;
    let lead = insert_lead(&pool, &et, Some(&host_id(&pool).await)).await;
    let (mate, mate_session) =
        insert_test_user_with_session(&pool, "Mate", "mate@example.com", "mate").await;
    let (_, outsider_session) =
        insert_test_user_with_session(&pool, "Out", "out@example.com", "outsider").await;
    sqlx::query("INSERT INTO team_members (team_id, user_id, role, source) VALUES (?, ?, 'member', 'direct')")
        .bind(&team)
        .bind(&mate)
        .execute(&pool)
        .await
        .unwrap();

    let page = body_string(
        app.clone()
            .oneshot(get_authed("/dashboard/leads", &mate_session))
            .await
            .unwrap(),
    )
    .await;
    assert!(
        page.contains("x@example.com"),
        "a team member sees the team's leads"
    );
    let page = body_string(
        app.clone()
            .oneshot(get_authed("/dashboard/leads", &outsider_session))
            .await
            .unwrap(),
    )
    .await;
    assert!(!page.contains("x@example.com"));

    let response = app
        .clone()
        .oneshot(action(&outsider_session, &lead, "delete", ""))
        .await
        .unwrap();
    assert_eq!(response.status(), 403);
    let response = app
        .oneshot(action(&mate_session, &lead, "delete", ""))
        .await
        .unwrap();
    assert!(response.status().is_redirection());
    assert_eq!(lead_count(&pool).await, 0);
}

#[tokio::test]
async fn a_team_alert_goes_to_the_creator_while_on_the_team_else_to_the_admins() {
    let _env = crate::test_support::ENV_LOCK.lock().await;
    let (_, pool, _, _) = setup_test_app().await;
    let (team, et) = team_event_type(&pool, "public").await;
    let (port, mails) = spawn_capturing_mta().await;
    use_capturing_mta(&pool, port).await;
    let creator = host_id(&pool).await;
    let (admin, _) =
        insert_test_user_with_session(&pool, "Team Admin", "teamadmin@example.com", "teamadmin")
            .await;
    sqlx::query("INSERT INTO team_members (team_id, user_id, role, source) VALUES (?, ?, 'admin', 'direct')")
        .bind(&team)
        .bind(&admin)
        .execute(&pool)
        .await
        .unwrap();

    let lead = insert_lead(&pool, &et, Some(&creator)).await;
    abandon(&pool, &lead).await;
    crate::web::leads::notify_abandoned(&pool, &[0u8; 32], &mut Default::default()).await;
    mail_to(&mails, "test@example.com").await;
    assert_eq!(mails.lock().unwrap().len(), 1, "only the creator");

    // The creator leaves the team: the next lead goes to the remaining admin.
    sqlx::query("DELETE FROM team_members WHERE team_id = ? AND user_id = ?")
        .bind(&team)
        .bind(&creator)
        .execute(&pool)
        .await
        .unwrap();
    let lead = insert_lead(&pool, &et, Some(&creator)).await;
    abandon(&pool, &lead).await;
    crate::web::leads::notify_abandoned(&pool, &[0u8; 32], &mut Default::default()).await;
    mail_to(&mails, "teamadmin@example.com").await;
}

// --- Awkward sessions --------------------------------------------------------------------

#[tokio::test]
async fn a_lead_moving_to_another_event_type_starts_over() {
    let (app, pool, _, et_id) = setup_test_app().await;
    enable_capture(&pool, &et_id).await;
    let (team, team_et) = team_event_type(&pool, "public").await;
    let _ = team;
    let lead_id = uuid::Uuid::new_v4().to_string();
    app.clone()
        .oneshot(capture_request(payload(&lead_id)))
        .await
        .unwrap();
    // The guest booked the first event type...
    sqlx::query("UPDATE partial_bookings SET completed_at = datetime('now'), notified_at = datetime('now'), archived_at = datetime('now') WHERE lead_id = ?")
        .bind(&lead_id)
        .execute(&pool)
        .await
        .unwrap();
    // ... and, in the same tab, starts another one with the same lead id.
    let response = app
        .oneshot(capture_request(team_payload(&lead_id)))
        .await
        .unwrap();
    assert_eq!(response.status(), 204);
    let (event_type, completed, notified, archived): (String, Option<String>, Option<String>, Option<String>) =
        sqlx::query_as("SELECT event_type_id, completed_at, notified_at, archived_at FROM partial_bookings WHERE lead_id = ?")
            .bind(&lead_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(event_type, team_et);
    assert!(completed.is_none() && notified.is_none() && archived.is_none());
}

#[tokio::test]
async fn the_confirmation_page_forgets_the_lead() {
    let (app, pool, _, et_id) = setup_test_app().await;
    enable_capture(&pool, &et_id).await;
    let mut monday = Utc::now().date_naive() + Duration::days(1);
    while monday.weekday() != chrono::Weekday::Mon {
        monday += Duration::days(1);
    }
    let form = format!(
        "_csrf={CSRF}&date={}&time=10%3A00&name=Pat&email=pat%40example.com",
        monday.format("%Y-%m-%d")
    );
    let response = app
        .oneshot(post_form_unauthed(
            "/u/testuser/test-meeting/book",
            CSRF,
            &form,
        ))
        .await
        .unwrap();
    let body = body_string(response).await;
    assert!(
        body.contains("sessionStorage.removeItem"),
        "{}",
        &body[..body.len().min(300)]
    );
}

#[tokio::test]
async fn private_disabled_and_unlisted_event_types_show_no_gate() {
    let (app, pool, _, et_id) = setup_test_app().await;
    enable_capture(&pool, &et_id).await;
    let gate = |app: Router| async move {
        body_string(app.oneshot(get("/u/testuser/test-meeting")).await.unwrap())
            .await
            .contains("id=\"lead-gate\"")
    };
    assert!(gate(app.clone()).await);
    sqlx::query("UPDATE event_types SET visibility = 'internal' WHERE id = ?")
        .bind(&et_id)
        .execute(&pool)
        .await
        .unwrap();
    assert!(!gate(app).await);
}

#[tokio::test]
async fn deleting_an_event_type_or_a_host_takes_the_leads_along_or_orphans_them_safely() {
    let (app, pool, session, et_id) = setup_test_app().await;
    let (mate, mate_session) =
        insert_test_user_with_session(&pool, "Mate", "mate@example.com", "mate").await;
    insert_lead(&pool, &et_id, Some(&mate)).await;
    // The host row goes: the lead keeps existing, owned by nobody, seen by admins.
    sqlx::query("DELETE FROM users WHERE id = ?")
        .bind(&mate)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(lead_count(&pool).await, 1);
    let _ = mate_session;
    let page = body_string(
        app.oneshot(get_authed("/dashboard/leads", &session))
            .await
            .unwrap(),
    )
    .await;
    assert!(
        page.contains("x@example.com"),
        "an admin still reaches an orphaned lead"
    );

    sqlx::query("DELETE FROM event_types WHERE id = ?")
        .bind(&et_id)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(lead_count(&pool).await, 0, "leads go with their event type");
}

#[tokio::test]
async fn delete_all_takes_the_whole_view_but_only_what_the_user_may_reach() {
    let (app, pool, _, et_id) = setup_test_app().await;
    let host = host_id(&pool).await;
    // The seeded event type is `testuser`'s; a second host's lead is not theirs.
    let (other, _) =
        insert_test_user_with_session(&pool, "Other", "other@example.com", "other").await;
    sqlx::query("UPDATE users SET role = 'user' WHERE id = ?")
        .bind(&host)
        .execute(&pool)
        .await
        .unwrap();
    let (_, session) = {
        let t = uuid::Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO sessions (id, user_id, expires_at) VALUES (?, ?, datetime('now', '+1 day'))")
            .bind(&t)
            .bind(&host)
            .execute(&pool)
            .await
            .unwrap();
        ((), t)
    };
    for _ in 0..3 {
        insert_lead(&pool, &et_id, Some(&host)).await;
    }
    let theirs = insert_lead(&pool, &et_id, Some(&other)).await;

    let forged = app
        .clone()
        .oneshot(post_form(
            "/dashboard/leads/delete-all",
            &session,
            CSRF,
            "_csrf=forged",
        ))
        .await
        .unwrap();
    assert_eq!(forged.status(), 403);
    assert_eq!(lead_count(&pool).await, 4);

    let response = app
        .oneshot(post_form(
            "/dashboard/leads/delete-all",
            &session,
            CSRF,
            &format!("_csrf={CSRF}&view=open"),
        ))
        .await
        .unwrap();
    assert!(response.status().is_redirection());
    let left: Vec<String> = sqlx::query_scalar("SELECT id FROM partial_bookings")
        .fetch_all(&pool)
        .await
        .unwrap();
    assert_eq!(left, vec![theirs], "only the user's own leads went");
}

// --- Export, paging, badge ----------------------------------------------------------------

async fn as_plain_host(pool: &SqlitePool) -> (String, String) {
    let host = host_id(pool).await;
    sqlx::query("UPDATE users SET role = 'user' WHERE id = ?")
        .bind(&host)
        .execute(pool)
        .await
        .unwrap();
    let token = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO sessions (id, user_id, expires_at) VALUES (?, ?, datetime('now', '+1 day'))",
    )
    .bind(&token)
    .bind(&host)
    .execute(pool)
    .await
    .unwrap();
    (host, token)
}

#[tokio::test]
async fn csv_export_has_the_users_leads_only_and_defuses_formulas() {
    let (app, pool, _, et_id) = setup_test_app().await;
    let (host, session) = as_plain_host(&pool).await;
    let mine = insert_lead(&pool, &et_id, Some(&host)).await;
    sqlx::query("UPDATE partial_bookings SET name = '=HYPERLINK(\"http://evil\")', notes = 'a,b' WHERE id = ?")
        .bind(&mine)
        .execute(&pool)
        .await
        .unwrap();
    let (other, _) =
        insert_test_user_with_session(&pool, "Other", "other@example.com", "other").await;
    let theirs = insert_lead(&pool, &et_id, Some(&other)).await;
    sqlx::query("UPDATE partial_bookings SET email = 'secret@theirs.example' WHERE id = ?")
        .bind(&theirs)
        .execute(&pool)
        .await
        .unwrap();

    let response = app
        .clone()
        .oneshot(get_authed("/dashboard/leads/export.csv", &session))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let headers = response.headers().clone();
    assert!(headers["content-type"]
        .to_str()
        .unwrap()
        .starts_with("text/csv"));
    assert!(headers["content-disposition"]
        .to_str()
        .unwrap()
        .contains("attachment; filename=\"leads-open-"));
    let body = body_string(response).await;
    assert!(body.contains("x@example.com"));
    assert!(
        !body.contains("secret@theirs.example"),
        "someone else's lead is not in the file"
    );
    assert!(body.contains("'=HYPERLINK"), "a formula is defused: {body}");
    assert!(body.contains("\"a,b\""));

    let anon = app
        .oneshot(get("/dashboard/leads/export.csv"))
        .await
        .unwrap();
    assert!(anon.status().is_redirection());
}

#[tokio::test]
async fn the_list_is_paged_and_the_page_number_is_clamped() {
    let (app, pool, session, et_id) = setup_test_app().await;
    for i in 0..120 {
        let lead = insert_lead(&pool, &et_id, None).await;
        sqlx::query("UPDATE partial_bookings SET email = ? WHERE id = ?")
            .bind(format!("p{i:03}@example.com"))
            .bind(&lead)
            .execute(&pool)
            .await
            .unwrap();
    }
    let rows = |uri: &'static str| {
        let (app, session) = (app.clone(), session.clone());
        async move {
            let body = body_string(app.oneshot(get_authed(uri, &session)).await.unwrap()).await;
            (body.matches("href=\"mailto:").count(), body)
        }
    };
    let (first, body) = rows("/dashboard/leads").await;
    assert_eq!(first, 50, "a page holds 50");
    assert!(body.contains("Page 1 of 3"));
    assert!(body.contains("page=2"));
    let (last, body) = rows("/dashboard/leads?page=3").await;
    assert_eq!(last, 20);
    assert!(body.contains("Page 3 of 3") && !body.contains("page=4"));
    let (beyond, body) = rows("/dashboard/leads?page=999").await;
    assert_eq!(beyond, 20);
    assert!(body.contains("Page 3 of 3"));
    let (negative, _) = rows("/dashboard/leads?page=-4").await;
    assert_eq!(negative, 50);
    // The export is not paged.
    let csv = body_string(
        app.oneshot(get_authed("/dashboard/leads/export.csv", &session))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(csv.matches("@example.com").count(), 120);
}

#[tokio::test]
async fn the_sidebar_badge_counts_leads_nobody_followed_up() {
    let (app, pool, _, et_id) = setup_test_app().await;
    let (host, session) = as_plain_host(&pool).await;
    insert_lead(&pool, &et_id, Some(&host)).await;
    let done = insert_lead(&pool, &et_id, Some(&host)).await;
    sqlx::query("UPDATE partial_bookings SET contacted_at = datetime('now') WHERE id = ?")
        .bind(&done)
        .execute(&pool)
        .await
        .unwrap();
    let (other, _) =
        insert_test_user_with_session(&pool, "Other", "other@example.com", "other").await;
    insert_lead(&pool, &et_id, Some(&other)).await;
    let count = || async {
        let response = app
            .clone()
            .oneshot(get_authed("/dashboard/leads/badge", &session))
            .await
            .unwrap();
        assert_eq!(response.headers()["cache-control"], "no-store");
        let json: serde_json::Value = serde_json::from_str(&body_string(response).await).unwrap();
        json["count"].as_i64().unwrap()
    };
    assert_eq!(count().await, 0, "capture is off for the instance");
    sqlx::query("UPDATE auth_config SET lead_capture_enabled = 1 WHERE id = 'singleton'")
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(count().await, 1, "mine, open, not contacted");
}

// --- Settings: anonymise, idle time, digest -----------------------------------------------

#[tokio::test]
async fn the_admin_card_saves_and_clamps_the_new_settings() {
    let (app, pool, session, _) = setup_test_app().await;
    let save = |body: &str| {
        post_form(
            "/dashboard/admin/lead-capture",
            &session,
            CSRF,
            &format!("_csrf={CSRF}&{body}"),
        )
    };
    app.clone()
        .oneshot(save("enabled=on&retention_days=10&alert_after_minutes=1&digest_minutes=99999&anonymize_on_booking=on"))
        .await
        .unwrap();
    let row = || async {
        sqlx::query_as::<_, (i64, i64, i64, i64)>(
            "SELECT lead_anonymize_on_booking, lead_alert_after_minutes, lead_digest_minutes, lead_retention_days FROM auth_config WHERE id = 'singleton'",
        )
        .fetch_one(&pool)
        .await
        .unwrap()
    };
    assert_eq!(row().await, (1, 5, 1440, 10));
    app.oneshot(save(
        "enabled=on&retention_days=10&alert_after_minutes=45&digest_minutes=-3",
    ))
    .await
    .unwrap();
    assert_eq!(
        row().await,
        (0, 45, 0, 10),
        "an unticked box is off, a negative digest is off"
    );
}

#[tokio::test]
async fn a_booking_blanks_the_person_but_keeps_the_count_when_asked() {
    for anonymize in [false, true] {
        let (app, pool, session, et_id) = setup_test_app().await;
        enable_capture(&pool, &et_id).await;
        sqlx::query("UPDATE auth_config SET lead_anonymize_on_booking = ? WHERE id = 'singleton'")
            .bind(anonymize)
            .execute(&pool)
            .await
            .unwrap();
        let lead_id = uuid::Uuid::new_v4().to_string();
        let mut body = payload(&lead_id);
        body["phone"] = "+33612345678".into();
        body["notes"] = "private".into();
        app.clone().oneshot(capture_request(body)).await.unwrap();

        let mut monday = Utc::now().date_naive() + Duration::days(1);
        while monday.weekday() != chrono::Weekday::Mon {
            monday += Duration::days(1);
        }
        let form = format!(
            "_csrf={CSRF}&date={}&time=10%3A00&name=Pat+Guest&email=pat%40example.com&lead_id={lead_id}",
            monday.format("%Y-%m-%d")
        );
        app.clone()
            .oneshot(post_form_unauthed(
                "/u/testuser/test-meeting/book",
                CSRF,
                &form,
            ))
            .await
            .unwrap();

        let (name, email, phone, notes, ip, completed): (
            Option<String>,
            Option<String>,
            Option<String>,
            Option<String>,
            Option<String>,
            Option<String>,
        ) = sqlx::query_as(
            "SELECT name, email, phone, notes, ip, completed_at FROM partial_bookings WHERE lead_id = ?",
        )
        .bind(&lead_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(completed.is_some());
        if anonymize {
            assert!(
                name.is_none()
                    && email.is_none()
                    && phone.is_none()
                    && notes.is_none()
                    && ip.is_none()
            );
            let page = body_string(
                app.oneshot(get_authed("/dashboard/leads?view=completed", &session))
                    .await
                    .unwrap(),
            )
            .await;
            assert!(page.contains("anonymised"));
        } else {
            assert_eq!(email.as_deref(), Some("pat@example.com"));
            assert_eq!(notes.as_deref(), Some("private"));
        }
        let started: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM partial_bookings")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(started, 1, "the lead stays in the statistics either way");
    }
}

// --- Combined pages: everyone named is in ---------------------------------------------------

#[tokio::test]
async fn everyone_named_on_a_combined_page_sees_the_lead() {
    let (app, pool, _, et_id) = setup_test_app().await;
    let second = second_host(&pool).await;
    enable_capture(&pool, &et_id).await;
    let (_, outsider) =
        insert_test_user_with_session(&pool, "Out", "out@example.com", "outsider").await;
    let lead_id = uuid::Uuid::new_v4().to_string();
    let mut body = payload(&lead_id);
    body["username"] = "testuser+secondhost".into();
    app.clone().oneshot(capture_request(body)).await.unwrap();

    let ids: Vec<String> =
        sqlx::query_scalar("SELECT user_id FROM partial_booking_participants ORDER BY user_id")
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(ids.len(), 2);

    let session2 = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO sessions (id, user_id, expires_at) VALUES (?, ?, datetime('now', '+1 day'))",
    )
    .bind(&session2)
    .bind(&second)
    .execute(&pool)
    .await
    .unwrap();
    for (who, sees) in [(&session2, true), (&outsider, false)] {
        let page = body_string(
            app.clone()
                .oneshot(get_authed("/dashboard/leads", who))
                .await
                .unwrap(),
        )
        .await;
        assert_eq!(page.contains("pat@example.com"), sees);
    }

    // The same browser then opens a single-user page: the audience shrinks back.
    app.oneshot(capture_request(payload(&lead_id)))
        .await
        .unwrap();
    let left: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM partial_booking_participants")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(left, 0);
}

#[tokio::test]
async fn a_combined_lead_alerts_every_participant() {
    let _env = crate::test_support::ENV_LOCK.lock().await;
    let (app, pool, _, et_id) = setup_test_app().await;
    second_host(&pool).await;
    enable_capture(&pool, &et_id).await;
    let (port, mails) = spawn_capturing_mta().await;
    use_capturing_mta(&pool, port).await;
    let lead_id = uuid::Uuid::new_v4().to_string();
    let mut body = payload(&lead_id);
    body["username"] = "testuser+secondhost".into();
    app.oneshot(capture_request(body)).await.unwrap();
    sqlx::query("UPDATE partial_bookings SET updated_at = datetime('now', '-60 minutes')")
        .execute(&pool)
        .await
        .unwrap();
    crate::web::leads::notify_abandoned(&pool, &[0u8; 32], &mut Default::default()).await;
    mail_to(&mails, "test@example.com").await;
    mail_to(&mails, "second@example.com").await;
}

// --- The idle threshold and the digest ---------------------------------------------------------

#[tokio::test]
async fn the_idle_threshold_is_the_admins_to_set() {
    let _env = crate::test_support::ENV_LOCK.lock().await;
    let (_, pool, _, et_id) = setup_test_app().await;
    enable_capture(&pool, &et_id).await;
    sqlx::query("UPDATE auth_config SET lead_alert_after_minutes = 120 WHERE id = 'singleton'")
        .execute(&pool)
        .await
        .unwrap();
    let (port, mails) = spawn_capturing_mta().await;
    use_capturing_mta(&pool, port).await;
    let lead = insert_lead(&pool, &et_id, Some(&host_id(&pool).await)).await;
    abandon(&pool, &lead).await; // 60 minutes of silence
    crate::web::leads::notify_abandoned(&pool, &[0u8; 32], &mut Default::default()).await;
    tokio::time::sleep(std::time::Duration::from_millis(250)).await;
    assert!(
        mails.lock().unwrap().is_empty(),
        "60 minutes is not yet 120"
    );

    sqlx::query(
        "UPDATE partial_bookings SET updated_at = datetime('now', '-150 minutes') WHERE id = ?",
    )
    .bind(&lead)
    .execute(&pool)
    .await
    .unwrap();
    crate::web::leads::notify_abandoned(&pool, &[0u8; 32], &mut Default::default()).await;
    mail_to(&mails, "test@example.com").await;
}

#[tokio::test]
async fn digest_mode_sends_one_email_per_window_with_everything_that_piled_up() {
    let _env = crate::test_support::ENV_LOCK.lock().await;
    let (_, pool, _, et_id) = setup_test_app().await;
    enable_capture(&pool, &et_id).await;
    sqlx::query("UPDATE auth_config SET lead_digest_minutes = 60 WHERE id = 'singleton'")
        .execute(&pool)
        .await
        .unwrap();
    let (port, mails) = spawn_capturing_mta().await;
    use_capturing_mta(&pool, port).await;
    let host = host_id(&pool).await;
    for i in 0..3 {
        let lead = insert_lead(&pool, &et_id, Some(&host)).await;
        sqlx::query("UPDATE partial_bookings SET email = ? WHERE id = ?")
            .bind(format!("guest{i}@example.com"))
            .bind(&lead)
            .execute(&pool)
            .await
            .unwrap();
        abandon(&pool, &lead).await;
    }
    let mut clock = Default::default();
    crate::web::leads::notify_abandoned(&pool, &[0u8; 32], &mut clock).await;
    let mail = mail_to(&mails, "test@example.com").await;
    assert!(mail.contains("Abandoned bookings: 3"), "{mail}");
    for i in 0..3 {
        assert!(mail.contains(&format!("guest{i}@example.com")));
    }

    // A fourth guest gives up inside the window: it waits.
    let late = insert_lead(&pool, &et_id, Some(&host)).await;
    abandon(&pool, &late).await;
    crate::web::leads::notify_abandoned(&pool, &[0u8; 32], &mut clock).await;
    tokio::time::sleep(std::time::Duration::from_millis(250)).await;
    assert_eq!(
        mails.lock().unwrap().len(),
        1,
        "still inside the 60 minutes"
    );
    let pending: Option<String> =
        sqlx::query_scalar("SELECT notified_at FROM partial_bookings WHERE id = ?")
            .bind(&late)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(pending.is_none(), "and it keeps waiting, not lost");
}
