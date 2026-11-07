//! The guest email footer (#223): storage, who may edit it, and what goes out.

use super::*;

const FOOTER: &str = "Questions? **Call** us";

async fn set_default_footer(pool: &SqlitePool, footer: Option<&str>) {
    sqlx::query("UPDATE auth_config SET guest_email_footer = ? WHERE id = 'singleton'")
        .bind(footer)
        .execute(pool)
        .await
        .unwrap();
}

async fn set_event_type_footer(pool: &SqlitePool, et_id: &str, footer: Option<&str>) {
    sqlx::query("UPDATE event_types SET guest_email_footer = ? WHERE id = ?")
        .bind(footer)
        .bind(et_id)
        .execute(pool)
        .await
        .unwrap();
}

async fn allow_all_users(pool: &SqlitePool) {
    sqlx::query("UPDATE auth_config SET guest_footer_allow_all_users = 1 WHERE id = 'singleton'")
        .execute(pool)
        .await
        .unwrap();
}

async fn make_test_user_a_member(pool: &SqlitePool) {
    sqlx::query("UPDATE users SET role = 'user' WHERE username = 'testuser'")
        .execute(pool)
        .await
        .unwrap();
}

async fn stored_footer(pool: &SqlitePool, slug: &str) -> Option<String> {
    sqlx::query_scalar("SELECT guest_email_footer FROM event_types WHERE slug = ?")
        .bind(slug)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn user_named(pool: &SqlitePool, username: &str) -> crate::models::User {
    sqlx::query_as("SELECT * FROM users WHERE username = ?")
        .bind(username)
        .fetch_one(pool)
        .await
        .unwrap()
}

fn encode(s: &str) -> String {
    urlencoding::encode(s).into_owned()
}

fn edit_body(csrf: &str, footer: Option<&str>) -> String {
    let mut body = format!(
        "_csrf={csrf}&title=Test+Meeting&slug=test-meeting&duration_min=30&location_value=https%3A%2F%2Fmeet.example.com&avail_days=1,2,3,4,5&avail_start=09:00&avail_end=17:00"
    );
    if let Some(footer) = footer {
        body.push_str(&format!("&guest_email_footer={}", encode(footer)));
    }
    body
}

// --- Which footer an event type uses ---

#[tokio::test]
async fn the_event_type_footer_replaces_the_instance_default() {
    let pool = setup_test_db().await;
    let (_, _, et_id) = seed_test_data(&pool).await;

    assert_eq!(crate::email::guest_footer(&pool, &et_id).await, None);

    set_default_footer(&pool, Some("Default footer")).await;
    assert_eq!(
        crate::email::guest_footer(&pool, &et_id).await.as_deref(),
        Some("Default footer")
    );

    set_event_type_footer(&pool, &et_id, Some("Own footer")).await;
    assert_eq!(
        crate::email::guest_footer(&pool, &et_id).await.as_deref(),
        Some("Own footer")
    );

    // A blank own footer is not an override.
    set_event_type_footer(&pool, &et_id, Some("  \n ")).await;
    assert_eq!(
        crate::email::guest_footer(&pool, &et_id).await.as_deref(),
        Some("Default footer")
    );

    set_event_type_footer(&pool, &et_id, None).await;
    set_default_footer(&pool, Some(" ")).await;
    assert_eq!(crate::email::guest_footer(&pool, &et_id).await, None);

    assert_eq!(crate::email::guest_footer(&pool, "no-such-id").await, None);
}

// --- Who may edit it ---

#[tokio::test]
async fn only_admins_edit_footers_unless_the_policy_allows_everyone() {
    let pool = setup_test_db().await;
    let (_, _, et_id) = seed_test_data(&pool).await;
    sqlx::query("INSERT INTO users (id, email, name, role, auth_provider, username, enabled) VALUES ('m1', 'member@example.com', 'Member', 'user', 'local', 'member', 1)")
        .execute(&pool)
        .await
        .unwrap();
    let admin = user_named(&pool, "testuser").await;
    let member = user_named(&pool, "member").await;

    assert!(can_edit_guest_footer(&pool, &admin).await);
    assert!(!can_edit_guest_footer(&pool, &member).await);

    // An admin's post is stored, normalised; blank clears.
    assert_eq!(
        resolve_guest_footer(&pool, &admin, Some("  hi\r\nthere "), None)
            .await
            .as_deref(),
        Some("hi\nthere")
    );
    assert_eq!(
        resolve_guest_footer(&pool, &admin, Some(" "), Some(&et_id)).await,
        None
    );

    // A member's post is ignored on create, and the stored value is carried
    // forward on edit: they cannot clear an admin's footer either.
    assert_eq!(
        resolve_guest_footer(&pool, &member, Some("mine"), None).await,
        None
    );
    set_event_type_footer(&pool, &et_id, Some("Admin footer")).await;
    assert_eq!(
        resolve_guest_footer(&pool, &member, Some(""), Some(&et_id))
            .await
            .as_deref(),
        Some("Admin footer")
    );

    allow_all_users(&pool).await;
    assert!(can_edit_guest_footer(&pool, &member).await);
    assert_eq!(
        resolve_guest_footer(&pool, &member, Some("mine"), Some(&et_id))
            .await
            .as_deref(),
        Some("mine")
    );
}

// --- The event type form ---

#[tokio::test]
async fn an_admin_saves_a_footer_through_the_event_type_form() {
    let (app, pool, session, _) = setup_test_app().await;

    let page = body_string(
        app.clone()
            .oneshot(get_authed(
                "/dashboard/event-types/test-meeting/edit",
                &session,
            ))
            .await
            .unwrap(),
    )
    .await;
    assert!(page.contains(r#"name="guest_email_footer""#), "{page}");
    assert!(page.contains(r#"data-max-chars="500""#));

    let csrf = "csrf-footer-admin";
    let response = app
        .clone()
        .oneshot(post_form(
            "/dashboard/event-types/test-meeting/edit",
            &session,
            csrf,
            &edit_body(csrf, Some(&format!("  {FOOTER}\r\n"))),
        ))
        .await
        .unwrap();
    assert!(response.status().is_redirection());
    assert_eq!(
        stored_footer(&pool, "test-meeting").await.as_deref(),
        Some(FOOTER)
    );

    // The saved text comes back in the form, so a save does not blank it.
    let page = body_string(
        app.oneshot(get_authed(
            "/dashboard/event-types/test-meeting/edit",
            &session,
        ))
        .await
        .unwrap(),
    )
    .await;
    assert!(page.contains("Questions? **Call** us"), "{page}");

    // Posting nothing clears it.
    let (app, ..) = (
        create_router(pool.clone(), std::env::temp_dir(), [0u8; 32]).await,
        (),
    );
    let response = app
        .oneshot(post_form(
            "/dashboard/event-types/test-meeting/edit",
            &session,
            csrf,
            &edit_body(csrf, Some("")),
        ))
        .await
        .unwrap();
    assert!(response.status().is_redirection());
    assert_eq!(stored_footer(&pool, "test-meeting").await, None);
}

#[tokio::test]
async fn a_footer_over_the_limit_is_refused_and_nothing_is_saved() {
    let (app, pool, session, et_id) = setup_test_app().await;
    set_event_type_footer(&pool, &et_id, Some("Kept")).await;

    let csrf = "csrf-footer-long";
    let too_long = "ł".repeat(501);
    let response = app
        .clone()
        .oneshot(post_form(
            "/dashboard/event-types/test-meeting/edit",
            &session,
            csrf,
            &edit_body(csrf, Some(&too_long)),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), 200, "re-renders the form");
    let page = body_string(response).await;
    assert!(
        page.contains("The email footer must be 500 characters or less."),
        "{page}"
    );
    assert_eq!(
        stored_footer(&pool, "test-meeting").await.as_deref(),
        Some("Kept")
    );

    // Exactly the limit is accepted, counting characters not bytes.
    let at_limit = "ł".repeat(500);
    let response = app
        .oneshot(post_form(
            "/dashboard/event-types/test-meeting/edit",
            &session,
            csrf,
            &edit_body(csrf, Some(&at_limit)),
        ))
        .await
        .unwrap();
    assert!(response.status().is_redirection());
    assert_eq!(
        stored_footer(&pool, "test-meeting").await.as_deref(),
        Some(at_limit.as_str())
    );
}

#[tokio::test]
async fn a_form_without_the_footer_field_keeps_the_stored_footer() {
    // A member's form, opened while the policy was off, has no footer field.
    // Saving it after an admin turned the policy on must not clear the footer;
    // an explicit blank value still does (covered above).
    let (app, pool, session, et_id) = setup_test_app().await;
    make_test_user_a_member(&pool).await;
    allow_all_users(&pool).await;
    set_event_type_footer(&pool, &et_id, Some("Admin footer")).await;

    let csrf = "csrf-footer-stale-form";
    let response = app
        .oneshot(post_form(
            "/dashboard/event-types/test-meeting/edit",
            &session,
            csrf,
            &edit_body(csrf, None),
        ))
        .await
        .unwrap();
    assert!(response.status().is_redirection());
    assert_eq!(
        stored_footer(&pool, "test-meeting").await.as_deref(),
        Some("Admin footer")
    );
}

#[tokio::test]
async fn a_member_who_may_not_edit_the_footer_is_not_refused_for_an_overlong_one() {
    let (app, pool, session, et_id) = setup_test_app().await;
    make_test_user_a_member(&pool).await;
    set_event_type_footer(&pool, &et_id, Some("Admin footer")).await;

    let csrf = "csrf-footer-member-long";
    let response = app
        .oneshot(post_form(
            "/dashboard/event-types/test-meeting/edit",
            &session,
            csrf,
            &edit_body(csrf, Some(&"x".repeat(501))),
        ))
        .await
        .unwrap();
    assert!(
        response.status().is_redirection(),
        "the ignored field must not fail the save"
    );
    assert_eq!(
        stored_footer(&pool, "test-meeting").await.as_deref(),
        Some("Admin footer")
    );
}

#[tokio::test]
async fn a_member_neither_sees_nor_changes_the_footer_until_the_policy_allows_it() {
    let (app, pool, session, et_id) = setup_test_app().await;
    make_test_user_a_member(&pool).await;
    set_event_type_footer(&pool, &et_id, Some("Admin footer")).await;

    let edit = "/dashboard/event-types/test-meeting/edit";
    let page = body_string(
        app.clone()
            .oneshot(get_authed(edit, &session))
            .await
            .unwrap(),
    )
    .await;
    assert!(!page.contains(r#"name="guest_email_footer""#), "{page}");

    // A forged post is ignored.
    let csrf = "csrf-footer-member";
    let response = app
        .clone()
        .oneshot(post_form(
            edit,
            &session,
            csrf,
            &edit_body(csrf, Some("forged")),
        ))
        .await
        .unwrap();
    assert!(response.status().is_redirection());
    assert_eq!(
        stored_footer(&pool, "test-meeting").await.as_deref(),
        Some("Admin footer")
    );
    // So is a plain save of the form as the member sees it.
    let response = app
        .oneshot(post_form(edit, &session, csrf, &edit_body(csrf, None)))
        .await
        .unwrap();
    assert!(response.status().is_redirection());
    assert_eq!(
        stored_footer(&pool, "test-meeting").await.as_deref(),
        Some("Admin footer")
    );

    // With the policy on, the field appears and saves.
    allow_all_users(&pool).await;
    let app = create_router(pool.clone(), std::env::temp_dir(), [0u8; 32]).await;
    let page = body_string(
        app.clone()
            .oneshot(get_authed(edit, &session))
            .await
            .unwrap(),
    )
    .await;
    assert!(page.contains(r#"name="guest_email_footer""#));
    app.oneshot(post_form(
        edit,
        &session,
        csrf,
        &edit_body(csrf, Some("Member footer")),
    ))
    .await
    .unwrap();
    assert_eq!(
        stored_footer(&pool, "test-meeting").await.as_deref(),
        Some("Member footer")
    );
}

#[tokio::test]
async fn creating_an_event_type_stores_the_footer_only_for_who_may_edit_it() {
    let (app, pool, session, _) = setup_test_app().await;
    let csrf = "csrf-footer-create";
    let body = |slug: &str| {
        format!(
            "_csrf={csrf}&title=New&slug={slug}&duration_min=30&location_value=https%3A%2F%2Fm.example.com&avail_days=1,2,3,4,5&avail_start=09:00&avail_end=17:00&guest_email_footer={}",
            encode(FOOTER)
        )
    };
    app.oneshot(post_form(
        "/dashboard/event-types/new",
        &session,
        csrf,
        &body("by-admin"),
    ))
    .await
    .unwrap();
    assert_eq!(
        stored_footer(&pool, "by-admin").await.as_deref(),
        Some(FOOTER)
    );

    make_test_user_a_member(&pool).await;
    let app = create_router(pool.clone(), std::env::temp_dir(), [0u8; 32]).await;
    app.oneshot(post_form(
        "/dashboard/event-types/new",
        &session,
        csrf,
        &body("by-member"),
    ))
    .await
    .unwrap();
    assert_eq!(stored_footer(&pool, "by-member").await, None);
}

// --- Duplicating ---

#[tokio::test]
async fn duplicating_copies_the_footer_only_for_who_may_edit_it() {
    let (app, pool, session, et_id) = setup_test_app().await;
    set_event_type_footer(&pool, &et_id, Some(FOOTER)).await;
    let csrf = "csrf-footer-dup";
    let dup = |app: Router| {
        let req = post_form(
            "/dashboard/event-types/test-meeting/duplicate",
            &session,
            csrf,
            &format!("_csrf={csrf}"),
        );
        async move { app.oneshot(req).await.unwrap() }
    };

    dup(app).await;
    assert_eq!(
        stored_footer(&pool, "test-meeting-copy").await.as_deref(),
        Some(FOOTER)
    );

    make_test_user_a_member(&pool).await;
    let app = create_router(pool.clone(), std::env::temp_dir(), [0u8; 32]).await;
    dup(app).await;
    assert_eq!(stored_footer(&pool, "test-meeting-copy-2").await, None);
}

// --- The admin panel ---

#[tokio::test]
async fn the_admin_panel_saves_the_default_footer_and_the_policy() {
    let (app, pool, session, _) = setup_test_app().await;
    let csrf = "csrf-footer-admin-panel";

    let page = body_string(
        app.clone()
            .oneshot(get_authed("/dashboard/admin", &session))
            .await
            .unwrap(),
    )
    .await;
    assert!(
        page.contains(r#"action="/dashboard/admin/guest-footer""#),
        "{page}"
    );

    let response = app
        .clone()
        .oneshot(post_form(
            "/dashboard/admin/guest-footer",
            &session,
            csrf,
            &format!(
                "_csrf={csrf}&guest_email_footer={}&allow_all_users=on",
                encode(&format!(" {FOOTER} "))
            ),
        ))
        .await
        .unwrap();
    assert!(response.status().is_redirection());
    let (footer, allow): (Option<String>, bool) = sqlx::query_as(
        "SELECT guest_email_footer, guest_footer_allow_all_users FROM auth_config WHERE id = 'singleton'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(footer.as_deref(), Some(FOOTER));
    assert!(allow);

    // Too long: refused with a message, nothing changes.
    let response = app
        .clone()
        .oneshot(post_form(
            "/dashboard/admin/guest-footer",
            &session,
            csrf,
            &format!("_csrf={csrf}&guest_email_footer={}", "x".repeat(501)),
        ))
        .await
        .unwrap();
    let to = response
        .headers()
        .get("location")
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    assert!(to.starts_with("/dashboard/admin?error="), "{to}");
    let footer: Option<String> =
        sqlx::query_scalar("SELECT guest_email_footer FROM auth_config WHERE id = 'singleton'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(footer.as_deref(), Some(FOOTER));

    // Cleared, and the policy unticked.
    app.oneshot(post_form(
        "/dashboard/admin/guest-footer",
        &session,
        csrf,
        &format!("_csrf={csrf}&guest_email_footer="),
    ))
    .await
    .unwrap();
    let (footer, allow): (Option<String>, bool) = sqlx::query_as(
        "SELECT guest_email_footer, guest_footer_allow_all_users FROM auth_config WHERE id = 'singleton'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(footer, None);
    assert!(!allow);
}

#[tokio::test]
async fn a_member_cannot_reach_the_admin_footer_endpoint() {
    let (app, pool, session, _) = setup_test_app().await;
    make_test_user_a_member(&pool).await;
    let csrf = "csrf-footer-forbidden";
    let response = app
        .oneshot(post_form(
            "/dashboard/admin/guest-footer",
            &session,
            csrf,
            &format!("_csrf={csrf}&guest_email_footer=x&allow_all_users=on"),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), 403);
    let footer: Option<String> =
        sqlx::query_scalar("SELECT guest_email_footer FROM auth_config WHERE id = 'singleton'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(footer, None, "nothing was written");
    let allow: bool = sqlx::query_scalar(
        "SELECT guest_footer_allow_all_users FROM auth_config WHERE id = 'singleton'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(!allow);
}

// --- What goes out ---

fn smtp_config(port: u16) -> crate::email::SmtpConfig {
    crate::email::SmtpConfig {
        host: "127.0.0.1".to_string(),
        port,
        username: String::new(),
        password: String::new(),
        from_email: "noreply@test.com".to_string(),
        from_name: None,
        tls_mode: crate::email::SmtpTlsMode::Plaintext,
    }
}

/// The guest confirmation, reminder and cancellation are built inside their
/// senders, so read them off the wire: footer in both bodies, none without.
#[tokio::test]
async fn confirmation_reminder_and_cancellation_carry_the_footer_on_the_wire() {
    for with_footer in [true, false] {
        let (port, mails) = spawn_capturing_mta().await;
        let config = smtp_config(port);
        let footer = with_footer.then(|| FOOTER.to_string());
        let details = crate::email::BookingDetails {
            event_title: "Intro".to_string(),
            date: "2026-10-27".to_string(),
            start_time: "10:00".to_string(),
            end_time: "10:30".to_string(),
            guest_name: "Gina".to_string(),
            guest_email: "guest@test.com".to_string(),
            guest_timezone: "Europe/Berlin".to_string(),
            host_name: "Alice".to_string(),
            host_email: "alice@test.com".to_string(),
            uid: "uid-footer".to_string(),
            guest_language: Some("fr".to_string()),
            guest_footer: footer.clone(),
            ..Default::default()
        };
        let cancelled = crate::email::CancellationDetails {
            event_title: details.event_title.clone(),
            date: details.date.clone(),
            start_time: details.start_time.clone(),
            end_time: details.end_time.clone(),
            guest_name: details.guest_name.clone(),
            guest_email: details.guest_email.clone(),
            guest_timezone: details.guest_timezone.clone(),
            host_name: details.host_name.clone(),
            host_email: details.host_email.clone(),
            uid: details.uid.clone(),
            cancelled_by_host: true,
            invite_sent: true,
            guest_language: details.guest_language.clone(),
            guest_footer: footer,
            ..Default::default()
        };
        crate::email::send_guest_confirmation(&config, &details, None)
            .await
            .unwrap();
        crate::email::send_guest_reminder(&config, &details, None)
            .await
            .unwrap();
        crate::email::send_guest_cancellation(&config, &cancelled)
            .await
            .unwrap();

        let sent: Vec<String> = mails
            .lock()
            .unwrap()
            .iter()
            .map(|m| decode_quoted_printable(&m.data))
            .collect();
        assert_eq!(sent.len(), 3);
        for (i, mail) in sent.iter().enumerate() {
            if with_footer {
                // Plain text without markers, and the bold in the HTML.
                assert!(mail.contains("Questions? Call us"), "mail {i}: {mail}");
                assert!(mail.contains("<strong>Call</strong>"), "mail {i}: {mail}");
            } else {
                assert!(!mail.contains("Call us"), "mail {i}: {mail}");
            }
        }
    }
}

/// A booking made through the public form: the guest's confirmation carries
/// the event type's footer, the host's notification does not.
#[tokio::test]
async fn a_booking_confirmation_carries_the_footer_and_the_host_email_does_not() {
    let _env = crate::test_support::ENV_LOCK.lock().await;
    let (app, pool, _, et_id) = setup_test_app().await;
    let (port, mails) = spawn_capturing_mta().await;
    use_capturing_mta(&pool, port).await;
    set_default_footer(&pool, Some("Default footer text")).await;
    set_event_type_footer(&pool, &et_id, Some(FOOTER)).await;

    let mut base = Utc::now().date_naive() + Duration::days(1);
    while base.weekday() != chrono::Weekday::Mon {
        base += Duration::days(1);
    }
    let csrf = "csrf-footer-booking";
    let form = format!(
        "_csrf={csrf}&date={}&time=10%3A00&name=Footer+Guest&email=footer-guest%40test.com&notes=",
        base.format("%Y-%m-%d")
    );
    let response = app
        .oneshot(post_form_unauthed(
            "/u/testuser/test-meeting/book",
            csrf,
            &form,
        ))
        .await
        .unwrap();
    assert!(response.status() == 200 || response.status().is_redirection());

    let guest = mail_to(&mails, "footer-guest@test.com").await;
    assert!(guest.contains("Questions? Call us"), "{guest}");
    assert!(guest.contains("<strong>Call</strong>"), "{guest}");
    assert!(!guest.contains("Default footer text"), "{guest}");

    let host = mail_to(&mails, "test@example.com").await;
    assert!(!host.contains("Questions? Call us"), "{host}");
}
