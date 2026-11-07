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
    seed_test_data(&pool).await;
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
        resolve_guest_footer(&pool, &admin, Some("  hi\r\nthere ")).await,
        Some(Some("hi\nthere".to_string()))
    );
    assert_eq!(
        resolve_guest_footer(&pool, &admin, Some(" ")).await,
        Some(None)
    );
    // A form without the field leaves the stored footer alone.
    assert_eq!(resolve_guest_footer(&pool, &admin, None).await, None);

    // A member's post is ignored: nothing on create, and the stored value is
    // kept on edit, so they cannot clear an admin's footer either.
    assert_eq!(
        resolve_guest_footer(&pool, &member, Some("mine")).await,
        None
    );
    assert_eq!(resolve_guest_footer(&pool, &member, Some("")).await, None);

    allow_all_users(&pool).await;
    assert!(can_edit_guest_footer(&pool, &member).await);
    assert_eq!(
        resolve_guest_footer(&pool, &member, Some("mine")).await,
        Some(Some("mine".to_string()))
    );

    // Keeping the footer reads nothing, so a save can never write back an
    // older footer than the one an admin stored meanwhile: the update keeps
    // the row's own value. With the database gone, a member still keeps it.
    pool.close().await;
    assert_eq!(resolve_guest_footer(&pool, &member, Some("x")).await, None);
}

/// Both edit handlers leave the footer column to the row when the save does
/// not set it, rather than binding a value read earlier in the request.
#[test]
fn the_event_type_updates_keep_the_rows_own_footer() {
    let src = std::fs::read_to_string("src/web/event_types.rs").unwrap();
    assert_eq!(
        src.matches("guest_email_footer = CASE WHEN ? THEN ? ELSE guest_email_footer END")
            .count(),
        2
    );
    assert!(!src.contains("guest_email_footer = ?,"));
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

/// A form without the footer field that fails validation re-renders with
/// the stored footer, so resubmitting it from the error page keeps it. A
/// blank that was posted stays blank on the error page.
#[tokio::test]
async fn an_error_re_render_shows_the_stored_footer_when_the_field_was_omitted() {
    let (app, pool, session, et_id) = setup_test_app().await;
    set_event_type_footer(&pool, &et_id, Some("Admin footer")).await;

    let csrf = "csrf-footer-rerender";
    for (footer, shown) in [(None, true), (Some(""), false)] {
        let body = edit_body(csrf, footer).replace("title=Test+Meeting", "title=");
        let response = app
            .clone()
            .oneshot(post_form(
                "/dashboard/event-types/test-meeting/edit",
                &session,
                csrf,
                &body,
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), 200, "re-renders the form");
        let page = body_string(response).await;
        assert!(page.contains("name=\"guest_email_footer\""), "{page}");
        assert_eq!(page.contains("Admin footer"), shown, "{footer:?}: {page}");
        assert_eq!(
            stored_footer(&pool, "test-meeting").await.as_deref(),
            Some("Admin footer")
        );
    }
}

/// A stored footer that cannot be read is never shown as a blank field:
/// saving that form would erase it. Both the edit page and an error
/// re-render of a form without the field return the error page instead.
#[tokio::test]
async fn a_footer_that_cannot_be_read_is_never_shown_blank() {
    let (app, pool, session, _) = setup_test_app().await;
    // Every other read of the form keeps working; only the footer's fails.
    sqlx::query("ALTER TABLE event_types DROP COLUMN guest_email_footer")
        .execute(&pool)
        .await
        .unwrap();

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
    assert!(page.contains("An internal error occurred"), "{page}");
    assert!(!page.contains("name=\"guest_email_footer\""), "{page}");

    let csrf = "csrf-footer-unreadable";
    let body = edit_body(csrf, None).replace("title=Test+Meeting", "title=");
    let page = body_string(
        app.oneshot(post_form(
            "/dashboard/event-types/test-meeting/edit",
            &session,
            csrf,
            &body,
        ))
        .await
        .unwrap(),
    )
    .await;
    assert!(page.contains("An internal error occurred"), "{page}");
    assert!(!page.contains("name=\"guest_email_footer\""), "{page}");
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

/// The claimant's attendee row keeps the address they had when they claimed
/// the booking. Changing their booking email afterwards must not make calrs
/// take them for an attendee the guest invited: their reschedule and
/// cancellation copies, sent through the attendee loops, carry no guest
/// footer and are in their own language, while the guest's invitee keeps
/// both the footer and the booking's language.
#[tokio::test]
async fn a_claimant_who_changes_their_booking_email_still_gets_no_footer() {
    let _env = crate::test_support::ENV_LOCK.lock().await;
    let (app, pool, _, et_id) = setup_test_app().await;
    let (port, mails) = spawn_capturing_mta().await;
    use_capturing_mta(&pool, port).await;
    set_event_type_footer(&pool, &et_id, Some(FOOTER)).await;
    let id = insert_v1_booking(
        &pool,
        &et_id,
        "guest@test.com",
        Utc::now() + Duration::days(10),
        30,
        "confirmed",
        &[("language", Some("fr".to_string()))],
    )
    .await;
    let uid = format!("uid-{id}");
    sqlx::query(
        "INSERT INTO booking_attendees (id, booking_id, email, created_at) \
         VALUES ('att-footer-friend', ?, 'friend@test.com', '2026-01-01 00:00:00')",
    )
    .bind(&id)
    .execute(&pool)
    .await
    .unwrap();
    let watcher = insert_role_user(&pool, "watcher@test.com", "user").await;
    set_user_language(&pool, "watcher@test.com", "de").await;
    sqlx::query(
        "INSERT INTO booking_claim_tokens (id, booking_id, user_id, token, expires_at) \
         VALUES ('bct-footer', ?, ?, 'claim-tok-footer', datetime('now', '+7 days'))",
    )
    .bind(&id)
    .bind(&watcher)
    .execute(&pool)
    .await
    .unwrap();

    let csrf = "csrf-footer-claim";
    let response = app
        .clone()
        .oneshot(post_form_unauthed(
            &format!("/booking/claim/{id}"),
            csrf,
            &format!("_csrf={csrf}&token=claim-tok-footer"),
        ))
        .await
        .unwrap();
    assert!(response.status().is_success(), "{}", response.status());
    let mark: Option<String> = sqlx::query_scalar(
        "SELECT claimant_user_id FROM booking_attendees \
         WHERE booking_id = ? AND email = 'watcher@test.com'",
    )
    .bind(&id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(mark.as_deref(), Some(watcher.as_str()));

    crate::web::notify::drain_emails(std::time::Duration::from_secs(10)).await;
    mails.lock().unwrap().clear();
    sqlx::query("UPDATE users SET booking_email = 'watcher-new@test.com' WHERE id = ?")
        .bind(&watcher)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        booking_attendee_languages(&pool, &uid, &[]).await,
        vec![("watcher@test.com".to_string(), Some("de".to_string()))]
    );

    // A reschedule's attendee copies, through the dispatch loop.
    let details = crate::email::BookingDetails {
        event_title: "Intro".to_string(),
        date: "2026-10-27".to_string(),
        start_time: "10:00".to_string(),
        end_time: "10:30".to_string(),
        guest_name: "Guest".to_string(),
        guest_email: "guest@test.com".to_string(),
        guest_timezone: "UTC".to_string(),
        host_name: "Alice".to_string(),
        host_email: "test@example.com".to_string(),
        uid: uid.clone(),
        guest_language: Some("fr".to_string()),
        guest_footer: crate::email::guest_footer(&pool, &et_id).await,
        additional_attendees: booking_additional_attendees(&pool, &uid).await,
        attendee_languages: booking_attendee_languages(&pool, &uid, &[]).await,
        ..Default::default()
    };
    crate::email::send_guest_attendee_reschedule(&smtp_config(port), &details, None)
        .await
        .unwrap();

    // The guest cancels: the handler sends the attendee cancellations.
    let cancel_token: String = sqlx::query_scalar("SELECT cancel_token FROM bookings WHERE id = ?")
        .bind(&id)
        .fetch_one(&pool)
        .await
        .unwrap();
    let csrf = "csrf-footer-cancel";
    let response = app
        .oneshot(post_form_unauthed(
            &format!("/booking/cancel/{cancel_token}"),
            csrf,
            &format!("_csrf={csrf}"),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);

    let claimant = settled_mails_to(&mails, "watcher@test.com").await;
    assert_eq!(claimant.len(), 2, "{claimant:#?}");
    for mail in &claimant {
        assert!(!mail.contains("Call us"), "{mail}");
        assert!(mail.contains("Gebucht von:"), "{mail}");
    }
    let friend = settled_mails_to(&mails, "friend@test.com").await;
    assert_eq!(friend.len(), 2, "{friend:#?}");
    for mail in &friend {
        assert!(mail.contains("Questions? Call us"), "{mail}");
        assert!(mail.contains("Organisateur"), "{mail}");
    }
}

/// Insert an additional attendee row as of `created_at`, marked as added by
/// a claim when `claimant` is given.
async fn insert_attendee_row(
    pool: &SqlitePool,
    booking_id: &str,
    email: &str,
    created_at: &str,
    claimant: Option<&str>,
) {
    sqlx::query(
        "INSERT INTO booking_attendees (id, booking_id, email, created_at, added_by_claim, claimant_user_id) \
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(booking_id)
    .bind(email)
    .bind(created_at)
    .bind(claimant.is_some())
    .bind(claimant)
    .execute(pool)
    .await
    .unwrap();
}

/// The claimant's role outlives their account: once the user is deleted,
/// their attendee row is still staff, so its copies carry no footer (and
/// read the booking's language, since theirs is gone), while the guest's
/// invitee keeps the footer.
#[tokio::test]
async fn a_deleted_claimants_copies_still_carry_no_footer() {
    let _env = crate::test_support::ENV_LOCK.lock().await;
    let (_app, pool, _, et_id) = setup_test_app().await;
    let (port, mails) = spawn_capturing_mta().await;
    set_event_type_footer(&pool, &et_id, Some(FOOTER)).await;
    let watcher = insert_role_user(&pool, "watcher@test.com", "user").await;
    let id = insert_v1_booking(
        &pool,
        &et_id,
        "guest@test.com",
        Utc::now() + Duration::days(10),
        30,
        "confirmed",
        &[
            ("language", Some("fr".to_string())),
            ("claimed_by_user_id", Some(watcher.clone())),
        ],
    )
    .await;
    let uid = format!("uid-{id}");
    insert_attendee_row(&pool, &id, "friend@test.com", "2026-01-01 00:00:00", None).await;
    insert_attendee_row(
        &pool,
        &id,
        "watcher@test.com",
        "2026-01-02 00:00:00",
        Some(&watcher),
    )
    .await;

    sqlx::query("DELETE FROM users WHERE id = ?")
        .bind(&watcher)
        .execute(&pool)
        .await
        .unwrap();
    let attendees = booking_additional_attendees(&pool, &uid).await;
    assert_eq!(attendees, vec!["friend@test.com", "watcher@test.com"]);
    let languages = booking_attendee_languages(&pool, &uid, &attendees).await;
    assert_eq!(languages, vec![("watcher@test.com".to_string(), None)]);

    let details = crate::email::BookingDetails {
        event_title: "Intro".to_string(),
        date: "2026-10-27".to_string(),
        start_time: "10:00".to_string(),
        end_time: "10:30".to_string(),
        guest_name: "Guest".to_string(),
        guest_email: "guest@test.com".to_string(),
        guest_timezone: "UTC".to_string(),
        host_name: "Alice".to_string(),
        host_email: "test@example.com".to_string(),
        uid: uid.clone(),
        guest_language: Some("fr".to_string()),
        guest_footer: crate::email::guest_footer(&pool, &et_id).await,
        additional_attendees: attendees,
        attendee_languages: languages,
        ..Default::default()
    };
    crate::email::send_guest_attendee_reschedule(&smtp_config(port), &details, None)
        .await
        .unwrap();
    let claimant = settled_mails_to(&mails, "watcher@test.com").await;
    assert_eq!(claimant.len(), 1, "{claimant:#?}");
    assert!(!claimant[0].contains("Call us"), "{}", claimant[0]);
    let friend = settled_mails_to(&mails, "friend@test.com").await;
    assert_eq!(friend.len(), 1, "{friend:#?}");
    assert!(friend[0].contains("Questions? Call us"), "{}", friend[0]);
}

/// When the claimant cannot be looked up, nobody is known to be a guest:
/// every attendee is returned as possibly the claimant, so no copy carries
/// the footer, rather than every copy, the claimant's included.
#[tokio::test]
async fn a_failed_claimant_lookup_sends_no_attendee_the_footer() {
    let pool = setup_test_db().await;
    pool.close().await;
    let attendees = vec![
        "friend@test.com".to_string(),
        "watcher@test.com".to_string(),
    ];
    let languages = booking_attendee_languages(&pool, "uid-x", &attendees).await;
    assert_eq!(
        languages,
        vec![
            ("friend@test.com".to_string(), None),
            ("watcher@test.com".to_string(), None)
        ]
    );
    let details = crate::email::BookingDetails {
        guest_footer: Some(FOOTER.to_string()),
        additional_attendees: attendees.clone(),
        attendee_languages: languages.clone(),
        ..Default::default()
    };
    let cancellation = crate::email::CancellationDetails {
        guest_footer: Some(FOOTER.to_string()),
        additional_attendees: attendees.clone(),
        attendee_languages: languages,
        ..Default::default()
    };
    for email in &attendees {
        assert_eq!(
            crate::email::attendee_booking_details(&details, email).guest_footer,
            None
        );
        assert_eq!(
            crate::email::attendee_cancellation_details(&cancellation, email).guest_footer,
            None
        );
    }
}

/// The migration's backfill of claims made before it, run again over rows
/// reset to their legacy state. Attendees older than the claim are the
/// guest's; the claim's row is found by its time, under the address the
/// claimant had then, and so is a deleted claimant's; a row created within
/// the claim's second is taken as possibly the claimant's.
#[tokio::test]
async fn the_claimant_backfill_finds_legacy_claims_by_their_time() {
    let pool = setup_test_db().await;
    let (_, _, et_id) = seed_test_data(&pool).await;
    let watcher = insert_role_user(&pool, "watcher@test.com", "user").await;
    sqlx::query("UPDATE users SET booking_email = 'watcher-new@test.com' WHERE id = ?")
        .bind(&watcher)
        .execute(&pool)
        .await
        .unwrap();
    let claim = |claimant: Option<String>| {
        vec![
            ("claimed_by_user_id", claimant),
            ("claimed_at", Some("2026-03-01 10:00:00".to_string())),
        ]
    };
    let start = Utc::now() + Duration::days(10);
    let changed = insert_v1_booking(
        &pool,
        &et_id,
        "g1@test.com",
        start,
        30,
        "confirmed",
        &claim(Some(watcher.clone())),
    )
    .await;
    let deleted = insert_v1_booking(
        &pool,
        &et_id,
        "g2@test.com",
        start + Duration::days(1),
        30,
        "confirmed",
        &claim(None),
    )
    .await;
    let same_second = insert_v1_booking(
        &pool,
        &et_id,
        "g3@test.com",
        start + Duration::days(2),
        30,
        "confirmed",
        &claim(Some(watcher.clone())),
    )
    .await;
    let unclaimed = insert_v1_booking(
        &pool,
        &et_id,
        "g4@test.com",
        start + Duration::days(3),
        30,
        "confirmed",
        &[],
    )
    .await;
    for (booking, email, at) in [
        (&changed, "friend1@test.com", "2026-02-01 09:00:00"),
        (&changed, "watcher@test.com", "2026-03-01 10:00:00"),
        (&deleted, "friend2@test.com", "2026-02-01 09:00:00"),
        (&deleted, "gone@test.com", "2026-03-01 10:00:01"),
        (&same_second, "friend3@test.com", "2026-03-01 10:00:00"),
        (&same_second, "watcher-new@test.com", "2026-03-01 10:00:00"),
        (&unclaimed, "friend4@test.com", "2026-03-02 00:00:00"),
    ] {
        insert_attendee_row(&pool, booking, email, at, None).await;
    }

    let migration =
        std::fs::read_to_string("migrations/20261107212745_claimant_attendee.sql").unwrap();
    // Comments first: they may hold a `;`.
    let sql: String = migration
        .lines()
        .filter(|line| !line.trim_start().starts_with("--"))
        .collect::<Vec<_>>()
        .join("\n");
    let backfill: Vec<&str> = sql
        .split(';')
        .filter(|statement| statement.trim_start().starts_with("UPDATE"))
        .collect();
    assert_eq!(backfill.len(), 2);
    for statement in backfill {
        sqlx::query(statement).execute(&pool).await.unwrap();
    }

    let rows: Vec<(String, bool, Option<String>)> = sqlx::query_as(
        "SELECT email, added_by_claim, claimant_user_id FROM booking_attendees ORDER BY email",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    let w = Some(watcher.clone());
    assert_eq!(
        rows,
        vec![
            ("friend1@test.com".to_string(), false, None),
            ("friend2@test.com".to_string(), false, None),
            ("friend3@test.com".to_string(), true, None),
            ("friend4@test.com".to_string(), false, None),
            ("gone@test.com".to_string(), true, None),
            ("watcher-new@test.com".to_string(), true, w.clone()),
            ("watcher@test.com".to_string(), true, w),
        ]
    );
}

/// The footer editor's preview (`renderMdPreview` in `dashboard_base.html`)
/// follows the email: inline code and images are shown as typed, so a link
/// or an emphasis inside one is not formatted, while one outside still is.
/// Runs the template's script under `node`; skipped where it is not
/// installed (the CI runners have it).
#[test]
fn the_footer_preview_shows_code_and_images_as_typed() {
    let template = std::fs::read_to_string("templates/dashboard_base.html").unwrap();
    let start = template.find("function mdSafeHref").unwrap();
    let end = template.find("// Shared markdown toolbar handler").unwrap();
    let script = template[start..end].replace(
        "{{ t('editor-nothing-to-preview') | tojson }}",
        "\"nothing\"",
    );
    let cases = [
        ("`[a](https://x.y)`", "`[a](https://x.y)`"),
        ("`*not em*` and *em*", "`*not em*` and <em>em</em>"),
        (
            "![*alt*](https://x.y/i.png)",
            "![*alt*](https://x.y/i.png)",
        ),
        (
            "**bold `<b>`** [l](https://x.y)",
            "<strong>bold `&lt;b&gt;`</strong> <a href=\"https://x.y\" style=\"color:var(--accent);text-decoration:underline;\">l</a>",
        ),
        ("``a ` b``\nnext", "``a ` b``<br>next"),
    ];
    let inputs: Vec<&str> = cases.iter().map(|(input, _)| *input).collect();
    let program = format!(
        "{script}\nconsole.log(JSON.stringify({}.map(function(md) {{ return renderMdPreview(md, 'email'); }})));",
        serde_json::to_string(&inputs).unwrap()
    );
    let output = match std::process::Command::new("node")
        .arg("-e")
        .arg(&program)
        .output()
    {
        Ok(output) => output,
        Err(e) => {
            eprintln!("skipped, node is not available: {e}");
            return;
        }
    };
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let rendered: Vec<String> = serde_json::from_slice(&output.stdout).unwrap();
    for ((input, expected), got) in cases.iter().zip(&rendered) {
        assert_eq!(got, expected, "{input:?}");
    }
}
