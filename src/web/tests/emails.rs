//! Emails and texts sent by the booking flows (#209, #221), via a capturing MTA.

use super::*;

// --- The daily cap stops sending without failing bookings ---

#[tokio::test]
async fn the_daily_cap_counts_todays_messages() {
    let pool = setup_test_db().await;
    assert_eq!(crate::sms::sent_today(&pool).await, 0);

    for i in 0..3 {
        sqlx::query(
            "INSERT INTO sms_usage (id, event, provider, segments, cost, currency) \
                 VALUES (?, 'confirmed', 'sevenio', 1, 0.075, 'EUR')",
        )
        .bind(format!("usage-{}", i))
        .execute(&pool)
        .await
        .unwrap();
    }
    assert_eq!(crate::sms::sent_today(&pool).await, 3);

    let (cost, currency) = crate::sms::cost_today(&pool).await;
    assert!((cost - 0.225).abs() < 1e-9, "cost was {}", cost);
    assert_eq!(currency, "EUR");

    // Yesterday's traffic does not count against today's allowance.
    sqlx::query(
        "INSERT INTO sms_usage (id, sent_at, event, provider) \
             VALUES ('old', datetime('now', '-2 days'), 'reminder', 'sevenio')",
    )
    .execute(&pool)
    .await
    .unwrap();
    assert_eq!(crate::sms::sent_today(&pool).await, 3);
}

#[tokio::test]
async fn task_tracker_drains_spawned_tasks() {
    static TRACKER: TaskTracker = TaskTracker::new();
    let done = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let flag = done.clone();
    TRACKER.spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        flag.store(true, std::sync::atomic::Ordering::SeqCst);
    });
    let left = TRACKER.drain(std::time::Duration::from_secs(5)).await;
    assert_eq!(left, 0);
    assert!(done.load(std::sync::atomic::Ordering::SeqCst));

    // A panicking task still counts as finished.
    TRACKER.spawn(async { panic!("send blew up") });
    assert_eq!(TRACKER.drain(std::time::Duration::from_secs(5)).await, 0);

    // A task that outlives the timeout is reported, not waited on.
    TRACKER.spawn(tokio::time::sleep(std::time::Duration::from_secs(60)));
    let left = TRACKER.drain(std::time::Duration::from_millis(50)).await;
    assert_eq!(left, 1);
}

/// The host's approval-request email carries the guest phone (#231).
#[tokio::test]
async fn approval_request_email_includes_the_guest_phone() {
    let _env = crate::test_support::ENV_LOCK.lock().await;
    let (app, pool, _, et_id) = setup_test_app().await;
    let (port, mails) = spawn_capturing_mta().await;
    use_capturing_mta(&pool, port).await;
    sqlx::query("UPDATE event_types SET sms_phone_mode = 'optional', requires_confirmation = 1 WHERE id = ?")
        .bind(&et_id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO sms_config (id, provider, api_secret_enc, sender, default_country_code) \
             VALUES ('cfg', 'sevenio', 'x', 'calrs', '+33')",
    )
    .execute(&pool)
    .await
    .unwrap();

    // Next Monday: the fixture's availability is weekdays only, so a fixed
    // "+3 days" failed whenever it landed on a weekend.
    let mut base = Utc::now().date_naive() + Duration::days(1);
    while base.weekday() != chrono::Weekday::Mon {
        base += Duration::days(1);
    }
    let csrf = "test-csrf-231-email";
    let form = format!(
        "_csrf={}&date={}&time=10%3A00&name=Phone+Guest&email=phone-guest%40test.com&notes=&phone=06+12+34+56+78",
        csrf,
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

    let mail = mail_to(&mails, "test@example.com").await;
    assert!(mail.contains("+33612345678"), "{mail}");
}

/// #261: approving a pending booking used to rebuild the details with no
/// notes and no phone, so the host's "Booking confirmed" email dropped both.
#[tokio::test]
async fn dashboard_approval_host_email_keeps_the_guest_phone_and_notes() {
    let _env = crate::test_support::ENV_LOCK.lock().await;
    let (app, pool, session, et_id) = setup_test_app().await;
    let (port, mails) = spawn_capturing_mta().await;
    use_capturing_mta(&pool, port).await;
    sqlx::query("UPDATE event_types SET sms_phone_mode = 'optional' WHERE id = ?")
        .bind(&et_id)
        .execute(&pool)
        .await
        .unwrap();
    let booking_id = insert_pending_french_booking(&pool, &et_id, "uid-261-approve").await;
    sqlx::query(
        "UPDATE bookings SET guest_phone = '+33612345678', notes = 'Bring the quote' WHERE id = ?",
    )
    .bind(&booking_id)
    .execute(&pool)
    .await
    .unwrap();

    let csrf = "test-csrf-261-approve";
    let response = app
        .oneshot(post_form(
            &format!("/dashboard/bookings/{}/confirm", booking_id),
            &session,
            csrf,
            &format!("_csrf={}", csrf),
        ))
        .await
        .unwrap();
    assert!(response.status().is_redirection());

    let mail = mail_to(&mails, "test@example.com").await;
    assert!(mail.contains("+33612345678"), "{mail}");
    assert!(mail.contains("Bring the quote"), "{mail}");
}

/// #261, the emailed Approve link: same rebuild, same loss.
#[tokio::test]
async fn token_approval_host_email_keeps_the_guest_phone_and_notes() {
    let _env = crate::test_support::ENV_LOCK.lock().await;
    let (app, pool, _, et_id) = setup_test_app().await;
    let (port, mails) = spawn_capturing_mta().await;
    use_capturing_mta(&pool, port).await;
    sqlx::query("UPDATE event_types SET sms_phone_mode = 'optional' WHERE id = ?")
        .bind(&et_id)
        .execute(&pool)
        .await
        .unwrap();
    let booking_id = insert_pending_french_booking(&pool, &et_id, "uid-261-token").await;
    sqlx::query(
        "UPDATE bookings SET guest_phone = '+33612345678', notes = 'Bring the quote' WHERE id = ?",
    )
    .bind(&booking_id)
    .execute(&pool)
    .await
    .unwrap();

    let response = app
        .oneshot(post_bare("/booking/approve/confirm-uid-261-token"))
        .await
        .unwrap();
    assert!(response.status().is_success() || response.status().is_redirection());

    let mail = mail_to(&mails, "test@example.com").await;
    assert!(mail.contains("+33612345678"), "{mail}");
    assert!(mail.contains("Bring the quote"), "{mail}");
}

/// Approval used to rebuild the details with no additional attendees: the
/// calendar event and the guest's invite listed only the guest, and the
/// attendees never heard the booking was confirmed (found while fixing #261).
#[tokio::test]
async fn approval_keeps_the_additional_attendees() {
    let _env = crate::test_support::ENV_LOCK.lock().await;
    let (app, pool, _, et_id) = setup_test_app().await;
    let (port, mails) = spawn_capturing_mta().await;
    use_capturing_mta(&pool, port).await;
    let booking_id = insert_pending_french_booking(&pool, &et_id, "uid-261-attendees").await;
    sqlx::query(
        "INSERT INTO booking_attendees (id, booking_id, email) VALUES ('att-261', ?, 'friend@test.com')",
    )
    .bind(&booking_id)
    .execute(&pool)
    .await
    .unwrap();

    let response = app
        .oneshot(post_bare("/booking/approve/confirm-uid-261-attendees"))
        .await
        .unwrap();
    assert!(response.status().is_success() || response.status().is_redirection());

    let guest = mail_to(&mails, "guest@test.com").await;
    assert!(guest.contains("mailto:friend@test.com"), "{guest}");
    mail_to(&mails, "friend@test.com").await;
}

/// A booking confirmed at submit tells its additional attendees there and
/// then, and remembers it was confirmed (#266).
#[tokio::test]
async fn a_confirmed_booking_emails_its_attendees_at_submit() {
    let _env = crate::test_support::ENV_LOCK.lock().await;
    let (app, pool, _, et_id) = setup_test_app().await;
    let (port, mails) = spawn_capturing_mta().await;
    use_capturing_mta(&pool, port).await;
    sqlx::query("UPDATE event_types SET max_additional_guests = 3 WHERE id = ?")
        .bind(&et_id)
        .execute(&pool)
        .await
        .unwrap();

    let csrf = "csrf-266-submit";
    let form = format!(
        "_csrf={}&date={}&time=10%3A00&name=Guest&email=submit%40test.com&notes=&additional_guests=friend%40test.com",
        csrf,
        far_future_monday()
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

    let attendee = mail_to(&mails, "friend@test.com").await;
    assert!(attendee.contains("METHOD:REQUEST"), "{attendee}");
    let stamped: bool = sqlx::query_scalar(
        "SELECT first_confirmed_at IS NOT NULL FROM bookings WHERE guest_email = 'submit@test.com'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(stamped);
}

/// #266: both approval paths now carry the attendees, so approving a guest
/// reschedule of a booking they had already been told about sent them the
/// "you've been added" email again. #267: they get the move instead, once,
/// with the invite under the same UID and the bumped SEQUENCE, so their
/// calendar moves the copy they hold.
#[tokio::test]
async fn approving_a_reschedule_moves_the_attendees_copy() {
    let _env = crate::test_support::ENV_LOCK.lock().await;
    let (app, pool, session, et_id) = setup_test_app().await;
    let (port, mails) = spawn_capturing_mta().await;
    use_capturing_mta(&pool, port).await;
    for (path, via_dashboard) in [("token", false), ("dashboard", true)] {
        // Confirmed first (the insert trigger stamps it), then sent back to
        // pending by a guest reschedule, as `guest_reschedule_booking` does.
        let id = insert_v1_booking(
            &pool,
            &et_id,
            &format!("{path}@test.com"),
            Utc::now() + Duration::days(if via_dashboard { 3 } else { 4 }),
            30,
            "confirmed",
            &[],
        )
        .await;
        sqlx::query("INSERT INTO booking_attendees (id, booking_id, email) VALUES (?, ?, ?)")
            .bind(format!("att-266-{path}"))
            .bind(&id)
            .bind(format!("friend-{path}@test.com"))
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("UPDATE bookings SET status = 'pending', ics_sequence = 1 WHERE id = ?")
            .bind(&id)
            .execute(&pool)
            .await
            .unwrap();

        let response = if via_dashboard {
            let csrf = "csrf-266-dash";
            app.clone()
                .oneshot(post_form(
                    &format!("/dashboard/bookings/{}/confirm", id),
                    &session,
                    csrf,
                    &format!("_csrf={}", csrf),
                ))
                .await
                .unwrap()
        } else {
            app.clone()
                .oneshot(post_bare(&format!("/booking/approve/confirm-{id}")))
                .await
                .unwrap()
        };
        assert!(response.status().is_success() || response.status().is_redirection());

        let guest = mail_to(&mails, &format!("{path}@test.com")).await;
        assert!(
            guest.contains(&format!("mailto:friend-{path}@test.com")),
            "{guest}"
        );
        assert!(guest.contains("SEQUENCE:1"), "{guest}");
        // The guest is told the new time is confirmed, as the SMS says it
        // moved, not that a booking was confirmed (#267).
        assert!(
            guest.contains("Test User has confirmed the new time of your booking."),
            "{guest}"
        );
        assert!(
            !guest.contains("Your booking has been confirmed!"),
            "{guest}"
        );
        assert!(guest.contains("METHOD:REQUEST"), "{guest}");
        // One email to the attendee: the move, not "you've been added" too.
        let to_attendee = settled_mails_to(&mails, &format!("friend-{path}@test.com")).await;
        assert_eq!(to_attendee.len(), 1, "emails to the attendee");
        let attendee = &to_attendee[0];
        assert!(attendee.contains("has moved to a new time"), "{attendee}");
        assert!(!attendee.contains("added as an attendee"), "{attendee}");
        assert!(attendee.contains("METHOD:REQUEST"), "{attendee}");
        assert!(attendee.contains(&format!("UID:uid-{id}")), "{attendee}");
        assert!(attendee.contains("SEQUENCE:1"), "{attendee}");
    }
}

/// #266: a guest reschedule that needs approval rebuilt the emails with no
/// notes, so the host's request for the new time lost the phone and the
/// notes the original request showed, and the guest's pending notice lost
/// their notes.
#[tokio::test]
async fn reschedule_needing_approval_keeps_the_guest_phone_and_notes() {
    let _env = crate::test_support::ENV_LOCK.lock().await;
    let (app, pool, _, et_id) = setup_test_app().await;
    let (port, mails) = spawn_capturing_mta().await;
    use_capturing_mta(&pool, port).await;
    sqlx::query(
        "UPDATE event_types SET requires_confirmation = 1, sms_phone_mode = 'optional' WHERE id = ?",
    )
    .bind(&et_id)
    .execute(&pool)
    .await
    .unwrap();
    let id = insert_v1_booking(
        &pool,
        &et_id,
        "resched@test.com",
        Utc::now() + Duration::days(2),
        30,
        "confirmed",
        &[
            ("guest_phone", Some("+33612345678".to_string())),
            ("notes", Some("Bring the quote".to_string())),
        ],
    )
    .await;
    let token: String = sqlx::query_scalar("SELECT reschedule_token FROM bookings WHERE id = ?")
        .bind(&id)
        .fetch_one(&pool)
        .await
        .unwrap();

    let csrf = "csrf-266-resched";
    let response = app
        .oneshot(post_form_unauthed(
            &format!("/booking/reschedule/{}", token),
            csrf,
            &format!(
                "_csrf={}&date={}&time=10%3A00&tz=UTC",
                csrf,
                far_future_monday()
            ),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);

    let host = mail_to(&mails, "test@example.com").await;
    assert!(host.contains("+33612345678"), "{host}");
    assert!(host.contains("Bring the quote"), "{host}");
    let guest = mail_to(&mails, "resched@test.com").await;
    assert!(guest.contains("Bring the quote"), "{guest}");
}

/// The host approving from the dashboard browses in their own language;
/// the guest still gets the confirmation in the language they booked in.
/// Before #209 this path never read `bookings.language` and fell back to
/// English.
#[tokio::test]
async fn dashboard_approval_emails_the_guest_in_the_booking_language() {
    let _env = crate::test_support::ENV_LOCK.lock().await;
    let (app, pool, session, et_id) = setup_test_app().await;
    let (port, mails) = spawn_capturing_mta().await;
    use_capturing_mta(&pool, port).await;
    let booking_id = insert_pending_french_booking(&pool, &et_id, "uid-209-approve").await;

    let csrf = "test-csrf-209-approve";
    let response = app
        .oneshot(post_form(
            &format!("/dashboard/bookings/{}/confirm", booking_id),
            &session,
            csrf,
            &format!("_csrf={}", csrf),
        ))
        .await
        .unwrap();
    assert!(response.status().is_redirection());

    let mail = mail_to(&mails, "guest@test.com").await;
    assert!(mail.contains("Bonjour Guest,"), "{mail}");
    assert!(mail.contains("<html lang=\"fr\">"), "{mail}");
    assert!(!mail.contains("Hi Guest,"), "{mail}");
}

/// A guest cancelling a round-robin booking used to notify the event
/// type owner, who was not hosting it: the host notice now goes to the
/// assigned member, in the member's language, and the cancel page names
/// them too.
#[tokio::test]
async fn guest_cancel_notifies_the_assigned_member_not_the_owner() {
    let _env = crate::test_support::ENV_LOCK.lock().await;
    let (app, pool, _, et_id) = setup_test_app().await;
    let (port, mails) = spawn_capturing_mta().await;
    use_capturing_mta(&pool, port).await;

    let member_id = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO users (id, email, name, role, auth_provider, username, enabled, language) \
             VALUES (?, 'member@test.com', 'Morgan Member', 'user', 'local', 'morgan', 1, 'fr')",
    )
    .bind(&member_id)
    .execute(&pool)
    .await
    .unwrap();
    let cancel_tok = uuid::Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO bookings (id, event_type_id, uid, guest_name, guest_email, guest_timezone, start_at, end_at, status, cancel_token, reschedule_token, assigned_user_id, language) VALUES (?, ?, 'uid-rr-gcancel', 'Guest', 'guest@test.com', 'UTC', '2030-07-02T10:00:00', '2030-07-02T10:30:00', 'confirmed', ?, ?, ?, 'en')")
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(&et_id)
        .bind(&cancel_tok)
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(&member_id)
        .execute(&pool)
        .await
        .unwrap();

    let response = app
        .clone()
        .oneshot(get(&format!("/booking/cancel/{}", cancel_tok)))
        .await
        .unwrap();
    let body = body_string(response).await;
    assert!(body.contains("Morgan Member"), "{body}");
    assert!(!body.contains("Test User"), "{body}");

    let csrf = "test-csrf-rr-gcancel";
    let response = app
        .oneshot(post_form_unauthed(
            &format!("/booking/cancel/{}", cancel_tok),
            csrf,
            &format!("_csrf={}", csrf),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);

    // Both emails have arrived once these return.
    mail_to(&mails, "guest@test.com").await;
    let host_mail = mail_to(&mails, "member@test.com").await;
    assert!(host_mail.contains("<html lang=\"fr\">"), "{host_mail}");
    let to_owner = mails
        .lock()
        .unwrap()
        .iter()
        .filter(|m| m.rcpt.iter().any(|r| r.contains("test@example.com")))
        .count();
    assert_eq!(to_owner, 0, "the event type owner was not hosting it");
}

/// A webhook SMS gateway on a local port that records each request and
/// accepts it.
async fn spawn_sms_webhook() -> (u16, Arc<std::sync::Mutex<Vec<String>>>) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let received = Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
    let sink = received.clone();
    tokio::spawn(async move {
        while let Ok((mut sock, _)) = listener.accept().await {
            let mut buf = Vec::new();
            let mut chunk = [0u8; 4096];
            loop {
                let n = sock.read(&mut chunk).await.unwrap_or(0);
                if n == 0 {
                    break;
                }
                buf.extend_from_slice(&chunk[..n]);
                let text = String::from_utf8_lossy(&buf);
                if let Some(end) = text.find("\r\n\r\n") {
                    let len = text[..end]
                        .lines()
                        .find_map(|l| {
                            l.to_ascii_lowercase()
                                .strip_prefix("content-length:")
                                .map(|v| v.trim().parse::<usize>().unwrap_or(0))
                        })
                        .unwrap_or(0);
                    if buf.len() >= end + 4 + len {
                        break;
                    }
                }
            }
            sink.lock()
                .unwrap()
                .push(String::from_utf8_lossy(&buf).into_owned());
            let _ = sock
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}")
                .await;
        }
    });

    (port, received)
}

async fn use_sms_webhook(pool: &SqlitePool, port: u16) {
    sqlx::query(
        "INSERT INTO sms_config (id, provider, sender, base_url, default_country_code) \
             VALUES ('cfg', 'webhook', 'calrs', ?, '+33')",
    )
    .bind(format!("http://127.0.0.1:{port}/sms"))
    .execute(pool)
    .await
    .unwrap();
}

/// Two approvals racing (a double click, or the dashboard and the email
/// link together) confirm once: one SMS, one guest email.
#[tokio::test]
async fn concurrent_approvals_notify_the_guest_once() {
    let _env = crate::test_support::ENV_LOCK.lock().await;
    let (app, pool, session, et_id) = setup_test_app().await;
    let (sms_port, received) = spawn_sms_webhook().await;
    use_sms_webhook(&pool, sms_port).await;
    let (mta_port, mails) = spawn_capturing_mta().await;
    use_capturing_mta(&pool, mta_port).await;
    sqlx::query("UPDATE event_types SET sms_phone_mode = 'optional' WHERE id = ?")
        .bind(&et_id)
        .execute(&pool)
        .await
        .unwrap();
    let uid = "uid-double-approve";
    let booking_id = insert_pending_french_booking(&pool, &et_id, uid).await;
    sqlx::query("UPDATE bookings SET guest_phone = '+33612345678' WHERE id = ?")
        .bind(&booking_id)
        .execute(&pool)
        .await
        .unwrap();

    let csrf = "test-csrf-double-approve";
    let dashboard = app.clone().oneshot(post_form(
        &format!("/dashboard/bookings/{}/confirm", booking_id),
        &session,
        csrf,
        &format!("_csrf={}", csrf),
    ));
    let dashboard_again = app.clone().oneshot(post_form(
        &format!("/dashboard/bookings/{}/confirm", booking_id),
        &session,
        csrf,
        &format!("_csrf={}", csrf),
    ));
    let by_token = app.clone().oneshot(post_form_unauthed(
        &format!("/booking/approve/confirm-{uid}"),
        csrf,
        &format!("_csrf={}", csrf),
    ));
    let (a, b, c) = tokio::join!(dashboard, dashboard_again, by_token);
    a.unwrap();
    b.unwrap();
    c.unwrap();

    // The guest confirmation goes out in the background; wait for it.
    let _ = mail_to(&mails, "guest@test.com").await;
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    let guest_mails = mails
        .lock()
        .unwrap()
        .iter()
        .filter(|m| m.rcpt.iter().any(|r| r.contains("guest@test.com")))
        .count();
    assert_eq!(guest_mails, 1, "the guest must be emailed once");
    assert_eq!(
        received.lock().unwrap().len(),
        1,
        "the guest must be texted once"
    );
}

/// Approving from the dashboard emails watcher teams their claim links,
/// as the approve link in the host's email always did.
#[tokio::test]
async fn dashboard_approval_notifies_watchers() {
    let _env = crate::test_support::ENV_LOCK.lock().await;
    let _vars = crate::test_support::EnvGuard::new();
    std::env::set_var("CALRS_BASE_URL", "https://cal.test");
    let (app, pool, session, et_id) = setup_test_app().await;
    let (port, mails) = spawn_capturing_mta().await;
    use_capturing_mta(&pool, port).await;

    let watcher_id = uuid::Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO users (id, email, name, role, auth_provider, username, enabled) VALUES (?, 'watcher@test.com', 'Watcher', 'user', 'local', 'watcher', 1)")
        .bind(&watcher_id)
        .execute(&pool)
        .await
        .unwrap();
    let team_id = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO teams (id, name, slug, visibility) VALUES (?, 'Sales', 'sales', 'public')",
    )
    .bind(&team_id)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO team_members (team_id, user_id, role, source) VALUES (?, ?, 'member', 'direct')")
        .bind(&team_id)
        .bind(&watcher_id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO event_type_watchers (event_type_id, team_id) VALUES (?, ?)")
        .bind(&et_id)
        .bind(&team_id)
        .execute(&pool)
        .await
        .unwrap();
    let booking_id = insert_pending_french_booking(&pool, &et_id, "uid-watch-approve").await;

    let csrf = "test-csrf-watch-approve";
    let response = app
        .oneshot(post_form(
            &format!("/dashboard/bookings/{}/confirm", booking_id),
            &session,
            csrf,
            &format!("_csrf={}", csrf),
        ))
        .await
        .unwrap();
    assert!(response.status().is_redirection());

    let tokens: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM booking_claim_tokens WHERE booking_id = ? AND user_id = ?",
    )
    .bind(&booking_id)
    .bind(&watcher_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(tokens.0, 1, "the watcher should get a claim token");
    let mail = mail_to(&mails, "watcher@test.com").await;
    assert!(mail.contains("https://cal.test/booking/claim/"), "{mail}");
}

/// #267: a booking is offered to watchers when it is first confirmed.
/// Approving a reschedule of it, from the dashboard or the emailed link,
/// must not offer it again: no new claim token, no second email.
#[tokio::test]
async fn approving_a_reschedule_does_not_offer_it_to_watchers_again() {
    let _env = crate::test_support::ENV_LOCK.lock().await;
    let _vars = crate::test_support::EnvGuard::new();
    std::env::set_var("CALRS_BASE_URL", "https://cal.test");
    let (app, pool, session, et_id) = setup_test_app().await;
    let (port, mails) = spawn_capturing_mta().await;
    use_capturing_mta(&pool, port).await;

    let watcher_id = uuid::Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO users (id, email, name, role, auth_provider, username, enabled) VALUES (?, 'watcher-again@test.com', 'Watcher', 'user', 'local', 'watcher-again', 1)")
        .bind(&watcher_id)
        .execute(&pool)
        .await
        .unwrap();
    let team_id = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO teams (id, name, slug, visibility) VALUES (?, 'Sales', 'sales-again', 'public')",
    )
    .bind(&team_id)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO team_members (team_id, user_id, role, source) VALUES (?, ?, 'member', 'direct')")
        .bind(&team_id)
        .bind(&watcher_id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO event_type_watchers (event_type_id, team_id) VALUES (?, ?)")
        .bind(&et_id)
        .bind(&team_id)
        .execute(&pool)
        .await
        .unwrap();

    for (day, path) in [(18, "dashboard"), (19, "token")] {
        // Confirmed first (the trigger stamps it), then moved by its guest
        // to a time awaiting approval.
        let id = insert_v1_booking(
            &pool,
            &et_id,
            &format!("g-watch-{path}@test.com"),
            Utc::now() + Duration::days(day),
            30,
            "confirmed",
            &[],
        )
        .await;
        sqlx::query("UPDATE bookings SET status = 'pending', ics_sequence = 1 WHERE id = ?")
            .bind(&id)
            .execute(&pool)
            .await
            .unwrap();
        let csrf = "csrf-267-watch";
        let request = if path == "dashboard" {
            post_form(
                &format!("/dashboard/bookings/{}/confirm", id),
                &session,
                csrf,
                &format!("_csrf={}", csrf),
            )
        } else {
            post_bare(&format!("/booking/approve/confirm-{id}"))
        };
        let response = app.clone().oneshot(request).await.unwrap();
        assert!(response.status().is_success() || response.status().is_redirection());
        assert_eq!(booking_status(&pool, &id).await, "confirmed", "{path}");

        let tokens: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM booking_claim_tokens WHERE booking_id = ?")
                .bind(&id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(tokens, 0, "{path}: no new claim offer");
        // The guest's email settles the spawned sends before counting.
        settled_mails_to(&mails, &format!("g-watch-{path}@test.com")).await;
        let offers = settled_mails_to(&mails, "watcher-again@test.com").await;
        assert!(offers.is_empty(), "{path}: {offers:?}");
    }
}

/// Two declines racing email the guest once.
#[tokio::test]
async fn concurrent_declines_notify_the_guest_once() {
    let _env = crate::test_support::ENV_LOCK.lock().await;
    let (app, pool, session, et_id) = setup_test_app().await;
    let (port, mails) = spawn_capturing_mta().await;
    use_capturing_mta(&pool, port).await;
    let booking_id = insert_pending_french_booking(&pool, &et_id, "uid-double-decline").await;

    let csrf = "test-csrf-double-decline";
    let dashboard = app.clone().oneshot(post_form(
        &format!("/dashboard/bookings/{}/cancel", booking_id),
        &session,
        csrf,
        &format!("_csrf={}", csrf),
    ));
    let dashboard_again = app.clone().oneshot(post_form(
        &format!("/dashboard/bookings/{}/cancel", booking_id),
        &session,
        csrf,
        &format!("_csrf={}", csrf),
    ));
    let by_token = app.clone().oneshot(post_form_unauthed(
        "/booking/decline/confirm-uid-double-decline",
        csrf,
        &format!("_csrf={}", csrf),
    ));
    let (a, b, c) = tokio::join!(dashboard, dashboard_again, by_token);
    a.unwrap();
    b.unwrap();
    c.unwrap();

    let _ = mail_to(&mails, "guest@test.com").await;
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    let guest_mails = mails
        .lock()
        .unwrap()
        .iter()
        .filter(|m| m.rcpt.iter().any(|r| r.contains("guest@test.com")))
        .count();
    assert_eq!(guest_mails, 1, "the guest must be emailed once");
    let status: (String,) = sqlx::query_as("SELECT status FROM bookings WHERE id = ?")
        .bind(&booking_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(status.0, "declined");
}

/// Approving from the dashboard sends the guest the confirmation SMS, like
/// the approve link in the host's email always did.
#[tokio::test]
async fn dashboard_approval_texts_the_guest() {
    let _env = crate::test_support::ENV_LOCK.lock().await;
    let (app, pool, session, et_id) = setup_test_app().await;

    let (port, received) = spawn_sms_webhook().await;
    use_sms_webhook(&pool, port).await;
    sqlx::query("UPDATE event_types SET sms_phone_mode = 'optional' WHERE id = ?")
        .bind(&et_id)
        .execute(&pool)
        .await
        .unwrap();
    let booking_id = insert_pending_french_booking(&pool, &et_id, "uid-sms-approve").await;
    sqlx::query("UPDATE bookings SET guest_phone = '+33612345678' WHERE id = ?")
        .bind(&booking_id)
        .execute(&pool)
        .await
        .unwrap();

    let csrf = "test-csrf-sms-approve";
    let response = app
        .oneshot(post_form(
            &format!("/dashboard/bookings/{}/confirm", booking_id),
            &session,
            csrf,
            &format!("_csrf={}", csrf),
        ))
        .await
        .unwrap();
    assert!(response.status().is_redirection());

    let requests = received.lock().unwrap().clone();
    assert_eq!(requests.len(), 1, "expected one SMS: {requests:?}");
    assert!(requests[0].contains("+33612345678"), "{}", requests[0]);
    let usage: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM sms_usage")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(usage.0, 1, "the accepted SMS should be recorded");
}

/// Same for the decline link in the host's approval email.
#[tokio::test]
async fn token_decline_emails_the_guest_in_the_booking_language() {
    let _env = crate::test_support::ENV_LOCK.lock().await;
    let (app, pool, _, et_id) = setup_test_app().await;
    let (port, mails) = spawn_capturing_mta().await;
    use_capturing_mta(&pool, port).await;
    insert_pending_french_booking(&pool, &et_id, "uid-209-decline").await;

    let csrf = "test-csrf-209-decline";
    let response = app
        .oneshot(post_form_unauthed(
            "/booking/decline/confirm-uid-209-decline",
            csrf,
            &format!("_csrf={}&reason=Conflit", csrf),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);

    let mail = mail_to(&mails, "guest@test.com").await;
    assert!(mail.contains("Bonjour Guest,"), "{mail}");
    assert!(
        mail.contains("Cette réservation a été refusée par l'hôte."),
        "{mail}"
    );
    assert!(mail.contains("<html lang=\"fr\">"), "{mail}");
    assert!(!mail.contains("has been declined"), "{mail}");
}

/// The guest's confirmation goes out under the host's name in the
/// guest's language, on the instance address, with Reply-To pointing at
/// the host's booking address.
#[tokio::test]
async fn guest_confirmation_comes_from_the_host() {
    let _env = crate::test_support::ENV_LOCK.lock().await;
    let (app, pool, session, et_id) = setup_test_app().await;
    let (port, mails) = spawn_capturing_mta().await;
    use_capturing_mta(&pool, port).await;
    sqlx::query("UPDATE smtp_config SET from_name = 'Acme Scheduling'")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "UPDATE users SET booking_email = 'bookings@example.com' WHERE email = 'test@example.com'",
    )
    .execute(&pool)
    .await
    .unwrap();
    let booking_id = insert_pending_french_booking(&pool, &et_id, "uid-221-approve").await;

    let csrf = "test-csrf-221-approve";
    let response = app
        .oneshot(post_form(
            &format!("/dashboard/bookings/{}/confirm", booking_id),
            &session,
            csrf,
            &format!("_csrf={}", csrf),
        ))
        .await
        .unwrap();
    assert!(response.status().is_redirection());

    let mail = mail_to(&mails, "guest@test.com").await;
    assert_eq!(
        mail_header(&mail, "From"),
        Some("\"Test User via Acme Scheduling\" <noreply@test.com>"),
        "{mail}"
    );
    assert_eq!(
        mail_header(&mail, "Reply-To"),
        Some("bookings@example.com"),
        "{mail}"
    );
}

/// A guest cancelling a booking assigned to a member other than the
/// event type's owner hears back from that member, not the owner, while
/// the host-side notice keeps the instance sender and no Reply-To.
#[tokio::test]
async fn guest_cancellation_comes_from_the_assigned_host() {
    let _env = crate::test_support::ENV_LOCK.lock().await;
    let (app, pool, _, et_id) = setup_test_app().await;
    let (port, mails) = spawn_capturing_mta().await;
    use_capturing_mta(&pool, port).await;

    let bob_id = uuid::Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO users (id, email, name, role, auth_provider, username, enabled, booking_email) VALUES (?, 'bob@test.com', 'Bob Member', 'user', 'local', 'bob', 1, 'bob.bookings@test.com')")
        .bind(&bob_id)
        .execute(&pool)
        .await
        .unwrap();
    let booking_id = insert_pending_french_booking(&pool, &et_id, "uid-221-cancel").await;
    sqlx::query("UPDATE bookings SET status = 'confirmed', assigned_user_id = ?, cancel_token = 'cancel-221', start_at = '2099-06-15T10:00:00', end_at = '2099-06-15T10:30:00' WHERE id = ?")
        .bind(&bob_id)
        .bind(&booking_id)
        .execute(&pool)
        .await
        .unwrap();

    let csrf = "test-csrf-221-cancel";
    let response = app
        .oneshot(post_form_unauthed(
            "/booking/cancel/cancel-221",
            csrf,
            &format!("_csrf={}", csrf),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);

    let mail = mail_to(&mails, "guest@test.com").await;
    assert_eq!(
        mail_header(&mail, "From"),
        Some("\"Bob Member\" <noreply@test.com>"),
        "{mail}"
    );
    assert_eq!(
        mail_header(&mail, "Reply-To"),
        Some("bob.bookings@test.com"),
        "{mail}"
    );

    // The host notice goes to Bob too, not to the owner.
    let host_mail = mail_to(&mails, "bob.bookings@test.com").await;
    assert_eq!(
        mail_header(&host_mail, "From"),
        Some("noreply@test.com"),
        "{host_mail}"
    );
    assert_eq!(mail_header(&host_mail, "Reply-To"), None, "{host_mail}");
}

/// The host reads the new-booking email in their saved language, the
/// guest in the one they booked in. The legacy route takes the page
/// language from `Accept-Language` alone, so the two differ here.
#[tokio::test]
async fn new_booking_emails_the_host_in_their_saved_language() {
    let _env = crate::test_support::ENV_LOCK.lock().await;
    let (app, pool, _, _) = setup_test_app().await;
    let (port, mails) = spawn_capturing_mta().await;
    use_capturing_mta(&pool, port).await;
    set_user_language(&pool, "test@example.com", "fr").await;

    let csrf = "csrf-209-host-lang";
    let response = app
        .oneshot(post_form_unauthed(
            "/test-meeting/book",
            csrf,
            &book_body(csrf),
        ))
        .await
        .unwrap();
    assert!(response.status() == 200 || response.status().is_redirection());

    let host = mail_to(&mails, "test@example.com").await;
    assert!(host.contains("Nouvelle réservation !"), "{host}");
    assert!(
        host.contains("Guest a réservé un créneau avec vous."),
        "{host}"
    );
    assert!(host.contains("<html lang=\"fr\">"), "{host}");
    assert!(!host.contains("booked a slot"), "{host}");

    let guest = mail_to(&mails, "guest@test.com").await;
    assert!(guest.contains("Hi Guest,"), "{guest}");
    assert!(guest.contains("<html lang=\"en\">"), "{guest}");
    assert!(!guest.contains("Bonjour"), "{guest}");
}

/// Collective team bookings email every member, each in their own
/// language.
#[tokio::test]
async fn collective_booking_emails_each_member_in_their_language() {
    let _env = crate::test_support::ENV_LOCK.lock().await;
    let (app, pool, _, _) = setup_test_app().await;
    let (port, mails) = spawn_capturing_mta().await;
    use_capturing_mta(&pool, port).await;
    let (team_id, team_et_id) = seed_team_event_type(&pool).await;
    sqlx::query("UPDATE event_types SET scheduling_mode = 'collective' WHERE id = ?")
        .bind(&team_et_id)
        .execute(&pool)
        .await
        .unwrap();
    let bob = insert_role_user(&pool, "bob@test.com", "user").await;
    sqlx::query("INSERT INTO team_members (team_id, user_id, role, source) VALUES (?, ?, 'member', 'direct')")
        .bind(&team_id)
        .bind(&bob)
        .execute(&pool)
        .await
        .unwrap();
    // insert_role_user names users after their address, which is not a
    // valid display name in a To: header.
    sqlx::query("UPDATE users SET name = 'Bob' WHERE id = ?")
        .bind(&bob)
        .execute(&pool)
        .await
        .unwrap();
    set_user_language(&pool, "test@example.com", "fr").await;
    set_user_language(&pool, "bob@test.com", "de").await;

    let csrf = "csrf-209-collective";
    let response = app
        .oneshot(post_form_unauthed(
            "/team/horizon-team/team-sync/book",
            csrf,
            &book_body(csrf),
        ))
        .await
        .unwrap();
    assert!(response.status() == 200 || response.status().is_redirection());

    let alice = mail_to(&mails, "test@example.com").await;
    assert!(alice.contains("Nouvelle réservation !"), "{alice}");
    assert!(alice.contains("<html lang=\"fr\">"), "{alice}");
    let bob_mail = mail_to(&mails, "bob@test.com").await;
    assert!(bob_mail.contains("Neue Buchung!"), "{bob_mail}");
    assert!(bob_mail.contains("<html lang=\"de\">"), "{bob_mail}");
    assert!(!bob_mail.contains("Nouvelle"), "{bob_mail}");
}

/// A guest rescheduling from a browser in another language still gets
/// the update in the language stored with the booking; the host gets
/// theirs in their own.
#[tokio::test]
async fn reschedule_emails_follow_the_booking_and_host_languages() {
    let _env = crate::test_support::ENV_LOCK.lock().await;
    let (app, pool, _, et_id) = setup_test_app().await;
    let (port, mails) = spawn_capturing_mta().await;
    use_capturing_mta(&pool, port).await;
    set_user_language(&pool, "test@example.com", "de").await;
    let cancel_tok = uuid::Uuid::new_v4().to_string();
    let resched_tok = uuid::Uuid::new_v4().to_string();
    let booking_id = insert_booking_at(
        &pool,
        &et_id,
        &cancel_tok,
        &resched_tok,
        chrono::Duration::hours(48),
    )
    .await;
    sqlx::query("UPDATE bookings SET language = 'fr' WHERE id = ?")
        .bind(&booking_id)
        .execute(&pool)
        .await
        .unwrap();

    let csrf = "csrf-209-resched";
    let body = format!(
        "_csrf={}&date={}&time=10:00&tz=UTC",
        csrf,
        far_future_monday()
    );
    let response = app
        .oneshot(post_form_unauthed(
            &format!("/booking/reschedule/{}", resched_tok),
            csrf,
            &body,
        ))
        .await
        .unwrap();
    assert!(response.status() == 200 || response.status().is_redirection());

    let guest = mail_to(&mails, "guest@test.com").await;
    assert!(
        guest.contains("Votre réservation a été reprogrammée par"),
        "{guest}"
    );
    assert!(guest.contains("<html lang=\"fr\">"), "{guest}");
    assert!(!guest.contains("has been rescheduled"), "{guest}");

    let host = mail_to(&mails, "test@example.com").await;
    assert!(host.contains("Buchung bestätigt"), "{host}");
    assert!(host.contains("<html lang=\"de\">"), "{host}");
}

/// Watchers get the claim offer in their own saved language.
#[tokio::test]
async fn watcher_claim_email_is_in_the_watcher_language() {
    let _env = crate::test_support::ENV_LOCK.lock().await;
    let _vars = crate::test_support::EnvGuard::new();
    std::env::set_var("CALRS_BASE_URL", "https://cal.test");
    let (app, pool, session, et_id) = setup_test_app().await;
    let (port, mails) = spawn_capturing_mta().await;
    use_capturing_mta(&pool, port).await;

    let watcher_id = uuid::Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO users (id, email, name, role, auth_provider, username, enabled, language) VALUES (?, 'watcher@test.com', 'Watcher', 'user', 'local', 'watcher', 1, 'de')")
        .bind(&watcher_id)
        .execute(&pool)
        .await
        .unwrap();
    let team_id = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO teams (id, name, slug, visibility) VALUES (?, 'Sales', 'sales', 'public')",
    )
    .bind(&team_id)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO team_members (team_id, user_id, role, source) VALUES (?, ?, 'member', 'direct')")
        .bind(&team_id)
        .bind(&watcher_id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO event_type_watchers (event_type_id, team_id) VALUES (?, ?)")
        .bind(&et_id)
        .bind(&team_id)
        .execute(&pool)
        .await
        .unwrap();
    let booking_id = insert_pending_french_booking(&pool, &et_id, "uid-209-watch").await;

    let csrf = "test-csrf-209-watch";
    let response = app
        .oneshot(post_form(
            &format!("/dashboard/bookings/{}/confirm", booking_id),
            &session,
            csrf,
            &format!("_csrf={}", csrf),
        ))
        .await
        .unwrap();
    assert!(response.status().is_redirection());

    let mail = mail_to(&mails, "watcher@test.com").await;
    assert!(mail.contains("Hallo Watcher,"), "{mail}");
    assert!(mail.contains("<html lang=\"de\">"), "{mail}");
    assert!(mail.contains("Diese Buchung übernehmen"), "{mail}");
    assert!(!mail.contains("available to claim"), "{mail}");
}

/// Host language is looked up by user id, never by address: here the
/// host's booking address is another user's login email, and the email
/// must still follow the host's own saved language.
#[tokio::test]
async fn host_language_follows_the_host_not_a_user_sharing_the_address() {
    let _env = crate::test_support::ENV_LOCK.lock().await;
    let (app, pool, session, et_id) = setup_test_app().await;
    let (port, mails) = spawn_capturing_mta().await;
    use_capturing_mta(&pool, port).await;
    sqlx::query(
        "UPDATE users SET booking_email = 'carol@test.com', language = 'fr' WHERE email = 'test@example.com'",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO users (id, email, name, role, auth_provider, username, enabled, language) VALUES (?, 'carol@test.com', 'Carol', 'user', 'local', 'carol', 1, 'de')")
        .bind(uuid::Uuid::new_v4().to_string())
        .execute(&pool)
        .await
        .unwrap();
    let booking_id = insert_pending_french_booking(&pool, &et_id, "uid-209-shared-address").await;

    let csrf = "test-csrf-209-shared";
    let response = app
        .oneshot(post_form(
            &format!("/dashboard/bookings/{}/confirm", booking_id),
            &session,
            csrf,
            &format!("_csrf={}", csrf),
        ))
        .await
        .unwrap();
    assert!(response.status().is_redirection());

    let host = mail_to(&mails, "carol@test.com").await;
    assert!(host.contains("Réservation confirmée"), "{host}");
    assert!(host.contains("<html lang=\"fr\">"), "{host}");
    assert!(!host.contains("Buchung"), "{host}");
}

/// A guest who booked in French and cancels from an English browser sees
/// the cancel page in French and is told in French: the stored booking
/// language, like every guest email.
#[tokio::test]
async fn guest_cancel_emails_in_the_booking_language() {
    let _env = crate::test_support::ENV_LOCK.lock().await;
    let (app, pool, _, et_id) = setup_test_app().await;
    let (port, mails) = spawn_capturing_mta().await;
    use_capturing_mta(&pool, port).await;
    let cancel_tok = uuid::Uuid::new_v4().to_string();
    let resched_tok = uuid::Uuid::new_v4().to_string();
    let booking_id = insert_booking_at(
        &pool,
        &et_id,
        &cancel_tok,
        &resched_tok,
        chrono::Duration::hours(48),
    )
    .await;
    sqlx::query("UPDATE bookings SET language = 'fr' WHERE id = ?")
        .bind(&booking_id)
        .execute(&pool)
        .await
        .unwrap();

    let page = body_string(
        app.clone()
            .oneshot(get_with_language(
                &format!("/booking/cancel/{}", cancel_tok),
                "en-US,en;q=0.9",
            ))
            .await
            .unwrap(),
    )
    .await;
    assert!(page.contains("<html lang=\"fr\">"), "{page}");

    let csrf = "csrf-cancel-lang";
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .method("POST")
                .uri(format!("/booking/cancel/{}", cancel_tok))
                .header("cookie", format!("__Host-calrs_csrf={}", csrf))
                .header("accept-language", "en-US,en;q=0.9")
                .header("content-type", "application/x-www-form-urlencoded")
                .body(Body::from(format!("_csrf={}", csrf)))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let done = body_string(response).await;
    assert!(done.contains("<html lang=\"fr\">"), "{done}");

    let guest = mail_to(&mails, "guest@test.com").await;
    assert!(guest.contains("Bonjour Guest,"), "{guest}");
    assert!(guest.contains("<html lang=\"fr\">"), "{guest}");
    assert!(!guest.contains("Hi Guest"), "{guest}");
}

/// #267: approving a guest reschedule of a booking that was confirmed
/// before texted the guest "Booking confirmed". Both approval paths now
/// text "Booking moved" for a reschedule, and still "Booking confirmed" for
/// a new request.
#[tokio::test]
async fn approving_a_reschedule_texts_moved_not_confirmed() {
    let _env = crate::test_support::ENV_LOCK.lock().await;
    let (app, pool, session, et_id) = setup_test_app().await;
    let (sms_port, received) = spawn_sms_webhook().await;
    use_sms_webhook(&pool, sms_port).await;
    sqlx::query("UPDATE event_types SET sms_phone_mode = 'optional' WHERE id = ?")
        .bind(&et_id)
        .execute(&pool)
        .await
        .unwrap();
    let mut day = 3;
    for via_dashboard in [false, true] {
        for rescheduled in [false, true] {
            day += 1;
            // Confirmed first (the insert trigger stamps it) and sent back to
            // pending, as a guest reschedule does; or a new pending request.
            let id = insert_v1_booking(
                &pool,
                &et_id,
                &format!("sms-{day}@test.com"),
                Utc::now() + Duration::days(day),
                30,
                if rescheduled { "confirmed" } else { "pending" },
                &[("guest_phone", Some("+33612345678".to_string()))],
            )
            .await;
            sqlx::query("UPDATE bookings SET status = 'pending' WHERE id = ?")
                .bind(&id)
                .execute(&pool)
                .await
                .unwrap();

            let response = if via_dashboard {
                let csrf = "csrf-267-sms";
                app.clone()
                    .oneshot(post_form(
                        &format!("/dashboard/bookings/{}/confirm", id),
                        &session,
                        csrf,
                        &format!("_csrf={}", csrf),
                    ))
                    .await
                    .unwrap()
            } else {
                app.clone()
                    .oneshot(post_bare(&format!("/booking/approve/confirm-{id}")))
                    .await
                    .unwrap()
            };
            assert!(response.status().is_success() || response.status().is_redirection());

            // The SMS is sent inline, before the response.
            let last = received.lock().unwrap().pop().expect("no SMS sent");
            let expected = if rescheduled {
                "Booking moved"
            } else {
                "Booking confirmed"
            };
            assert!(
                last.contains(expected),
                "dashboard={via_dashboard} rescheduled={rescheduled}: {last}"
            );
        }
    }
}

/// #267: a guest reschedule that needs no approval told the guest and the
/// host, never the additional attendees, whose copy kept the old time. They
/// now get the move with the invite under the same UID and a SEQUENCE one
/// higher, as the guest does. A reschedule awaiting approval tells them
/// nothing yet: like the guest's, their copy stays at the confirmed time.
#[tokio::test]
async fn a_reschedule_moves_the_attendees_copy_once_confirmed() {
    let _env = crate::test_support::ENV_LOCK.lock().await;
    let (app, pool, _, et_id) = setup_test_app().await;
    let (port, mails) = spawn_capturing_mta().await;
    use_capturing_mta(&pool, port).await;
    sqlx::query("UPDATE event_types SET sms_phone_mode = 'optional' WHERE id = ?")
        .bind(&et_id)
        .execute(&pool)
        .await
        .unwrap();
    // The pending case first, while the host has had no other email.
    for needs_approval in [true, false] {
        sqlx::query("UPDATE event_types SET requires_confirmation = ? WHERE id = ?")
            .bind(needs_approval)
            .bind(&et_id)
            .execute(&pool)
            .await
            .unwrap();
        let tag = if needs_approval { "pending" } else { "direct" };
        let id = insert_v1_booking(
            &pool,
            &et_id,
            &format!("moved-{tag}@test.com"),
            Utc::now() + Duration::days(if needs_approval { 5 } else { 6 }),
            30,
            "confirmed",
            &[
                ("guest_phone", Some("+33612345678".to_string())),
                ("language", Some("fr".to_string())),
                ("notes", Some("Bring the quote".to_string())),
            ],
        )
        .await;
        sqlx::query("INSERT INTO booking_attendees (id, booking_id, email) VALUES (?, ?, ?)")
            .bind(format!("att-267-{tag}"))
            .bind(&id)
            .bind(format!("friend-{tag}@test.com"))
            .execute(&pool)
            .await
            .unwrap();
        let token: String =
            sqlx::query_scalar("SELECT reschedule_token FROM bookings WHERE id = ?")
                .bind(&id)
                .fetch_one(&pool)
                .await
                .unwrap();

        let csrf = "csrf-267-resched";
        let time = if needs_approval { "11%3A00" } else { "10%3A00" };
        let response = app
            .clone()
            .oneshot(post_form_unauthed(
                &format!("/booking/reschedule/{}", token),
                csrf,
                &format!(
                    "_csrf={}&date={}&time={}&tz=UTC",
                    csrf,
                    far_future_monday(),
                    time
                ),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), 200);

        let sequence: i64 = sqlx::query_scalar("SELECT ics_sequence FROM bookings WHERE id = ?")
            .bind(&id)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(sequence, 1, "the reschedule bumps the SEQUENCE");

        let guest = mail_to(&mails, &format!("moved-{tag}@test.com")).await;
        if needs_approval {
            // The guest is told the request is pending; once the host's
            // request has arrived too, nothing went to the attendee.
            mail_to(&mails, "test@example.com").await;
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
            assert!(
                !mails
                    .lock()
                    .unwrap()
                    .iter()
                    .any(|m| m.rcpt.iter().any(|r| r.contains("friend-pending"))),
                "a reschedule awaiting approval must not move the attendee's copy"
            );
            continue;
        }
        assert!(guest.contains("SEQUENCE:1"), "{guest}");
        // The guest's moved invite replaces their confirmation one whole:
        // it keeps the attendees and the notes, never the phone.
        assert!(
            guest.contains("ATTENDEE;RSVP=TRUE:mailto:friend-direct@test.com"),
            "{guest}"
        );
        assert!(guest.contains("DESCRIPTION:Bring the quote"), "{guest}");
        assert!(!guest.contains("+33612345678"), "{guest}");
        let attendee = mail_to(&mails, "friend-direct@test.com").await;
        // In the booking's language, with the old time, and the invite that
        // replaces the copy they hold.
        assert!(attendee.contains("<html lang=\"fr\">"), "{attendee}");
        assert!(
            attendee.contains("Une r\u{e9}servation \u{e0} laquelle vous participez"),
            "{attendee}"
        );
        assert!(attendee.contains("Pr\u{e9}c\u{e9}demment"), "{attendee}");
        assert!(attendee.contains("METHOD:REQUEST"), "{attendee}");
        assert!(attendee.contains(&format!("UID:uid-{id}")), "{attendee}");
        assert!(attendee.contains("SEQUENCE:1"), "{attendee}");
        assert!(attendee.contains("DTSTART:"), "{attendee}");
        // Never the host's data.
        assert!(!attendee.contains("+33612345678"), "{attendee}");
    }
}

/// #267: the CANCEL for a booking that was moved carries a SEQUENCE above
/// the one its last invite had; a lower one is ignored by clients as out of
/// date, and RFC 5546 asks for a higher one.
#[tokio::test]
async fn cancelling_a_moved_booking_sends_its_sequence() {
    let _env = crate::test_support::ENV_LOCK.lock().await;
    let (app, pool, _, et_id) = setup_test_app().await;
    let (port, mails) = spawn_capturing_mta().await;
    use_capturing_mta(&pool, port).await;
    let id = insert_v1_booking(
        &pool,
        &et_id,
        "cancel-moved@test.com",
        Utc::now() + Duration::days(7),
        30,
        "confirmed",
        &[],
    )
    .await;
    sqlx::query("UPDATE bookings SET ics_sequence = 2 WHERE id = ?")
        .bind(&id)
        .execute(&pool)
        .await
        .unwrap();
    let token: String = sqlx::query_scalar("SELECT cancel_token FROM bookings WHERE id = ?")
        .bind(&id)
        .fetch_one(&pool)
        .await
        .unwrap();

    let csrf = "csrf-267-cancel";
    let response = app
        .oneshot(post_form_unauthed(
            &format!("/booking/cancel/{}", token),
            csrf,
            &format!("_csrf={}", csrf),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);

    let guest = mail_to(&mails, "cancel-moved@test.com").await;
    assert!(guest.contains("METHOD:CANCEL"), "{guest}");
    // One above the copy the guest holds (RFC 5546), and stored, so a
    // later message about the booking cannot fall behind it.
    assert!(guest.contains("SEQUENCE:3"), "{guest}");
    let stored: i64 = sqlx::query_scalar("SELECT ics_sequence FROM bookings WHERE id = ?")
        .bind(&id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(stored, 3);
}

/// Add an additional attendee to a booking.
async fn add_attendee(pool: &SqlitePool, booking_id: &str, email: &str) {
    sqlx::query("INSERT INTO booking_attendees (id, booking_id, email) VALUES (?, ?, ?)")
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(booking_id)
        .bind(email)
        .execute(pool)
        .await
        .unwrap();
}

/// #267: the attendees of a confirmed booking were sent its moves but never
/// its cancellation, so their copy outlived it. Each now gets one email
/// with a CANCEL under the booking's UID and the next SEQUENCE, whoever
/// cancels, and never the reason, which one party wrote for the other.
#[tokio::test]
async fn cancelling_a_confirmed_booking_cancels_the_attendees_copy() {
    let _env = crate::test_support::ENV_LOCK.lock().await;
    let (app, pool, session, et_id) = setup_test_app().await;
    let (port, mails) = spawn_capturing_mta().await;
    use_capturing_mta(&pool, port).await;
    for (day, by_host) in [(8, false), (9, true)] {
        let tag = if by_host { "host" } else { "guest" };
        let start = Utc::now() + Duration::days(day);
        let id = insert_v1_booking(
            &pool,
            &et_id,
            &format!("g-cancel-{tag}@test.com"),
            start,
            30,
            "confirmed",
            &[("ics_sequence", Some("2".to_string()))],
        )
        .await;
        add_attendee(&pool, &id, &format!("a-cancel-{tag}@test.com")).await;
        let csrf = "csrf-267-att-cancel";
        let body = format!("_csrf={}&reason=Private+matter", csrf);
        let response = if by_host {
            app.clone()
                .oneshot(post_form(
                    &format!("/dashboard/bookings/{}/cancel", id),
                    &session,
                    csrf,
                    &body,
                ))
                .await
                .unwrap()
        } else {
            let token: String =
                sqlx::query_scalar("SELECT cancel_token FROM bookings WHERE id = ?")
                    .bind(&id)
                    .fetch_one(&pool)
                    .await
                    .unwrap();
            app.clone()
                .oneshot(post_form_unauthed(
                    &format!("/booking/cancel/{}", token),
                    csrf,
                    &body,
                ))
                .await
                .unwrap()
        };
        assert!(response.status().is_success() || response.status().is_redirection());

        let to_attendee = settled_mails_to(&mails, &format!("a-cancel-{tag}@test.com")).await;
        assert_eq!(to_attendee.len(), 1, "{tag}: emails to the attendee");
        let attendee = &to_attendee[0];
        assert!(
            attendee.contains("A booking you attend has been cancelled."),
            "{attendee}"
        );
        assert!(attendee.contains("METHOD:CANCEL"), "{attendee}");
        assert!(attendee.contains(&format!("UID:uid-{id}")), "{attendee}");
        // One above the SEQUENCE:2 copy they hold (RFC 5546).
        assert!(attendee.contains("SEQUENCE:3"), "{attendee}");
        assert!(
            attendee.contains(&start.format("%Y-%m-%d").to_string()),
            "a confirmed booking's cancellation shows the time they hold: {attendee}"
        );
        assert!(!attendee.contains("Private matter"), "{attendee}");
        // The guest's CANCEL names everyone the invite listed.
        let guest = mail_to(&mails, &format!("g-cancel-{tag}@test.com")).await;
        assert!(
            guest.contains(&format!("ATTENDEE:mailto:a-cancel-{tag}@test.com")),
            "{guest}"
        );
    }
}

/// #267: a request never confirmed sent its attendees nothing, so ending it
/// sends them nothing either, and its decline or the guest's own
/// cancellation carries no CANCEL for the guest, who holds no invite.
#[tokio::test]
async fn ending_a_never_confirmed_request_tells_its_attendees_nothing() {
    let _env = crate::test_support::ENV_LOCK.lock().await;
    let (app, pool, session, et_id) = setup_test_app().await;
    let (port, mails) = spawn_capturing_mta().await;
    use_capturing_mta(&pool, port).await;
    for (day, path) in [(10, "cancel"), (11, "decline"), (12, "dashboard")] {
        let id = insert_v1_booking(
            &pool,
            &et_id,
            &format!("g-new-{path}@test.com"),
            Utc::now() + Duration::days(day),
            30,
            "pending",
            &[],
        )
        .await;
        add_attendee(&pool, &id, &format!("a-new-{path}@test.com")).await;
        let csrf = "csrf-267-new";
        let body = format!("_csrf={}", csrf);
        let request = match path {
            "cancel" => {
                let token: String =
                    sqlx::query_scalar("SELECT cancel_token FROM bookings WHERE id = ?")
                        .bind(&id)
                        .fetch_one(&pool)
                        .await
                        .unwrap();
                post_form_unauthed(&format!("/booking/cancel/{}", token), csrf, &body)
            }
            "decline" => post_form_unauthed(&format!("/booking/decline/confirm-{id}"), csrf, &body),
            _ => post_form(
                &format!("/dashboard/bookings/{}/cancel", id),
                &session,
                csrf,
                &body,
            ),
        };
        let response = app.clone().oneshot(request).await.unwrap();
        assert!(response.status().is_success() || response.status().is_redirection());

        let guest = settled_mails_to(&mails, &format!("g-new-{path}@test.com")).await;
        assert_eq!(guest.len(), 1, "{path}: emails to the guest");
        // No CANCEL and no note about one, whoever ends it: the guest holds
        // no invite either.
        assert!(!guest[0].contains("text/calendar"), "{path}: {}", guest[0]);
        assert!(
            !guest[0].contains("A calendar cancellation is attached"),
            "{path}: {}",
            guest[0]
        );
        let attendee = settled_mails_to(&mails, &format!("a-new-{path}@test.com")).await;
        assert!(attendee.is_empty(), "{path}: {attendee:?}");
    }
}

/// #267: declining or letting expire a reschedule of a confirmed booking
/// ends a booking whose invite the guest and the attendees still hold at
/// its confirmed time. The guest's decline or expiry email carries a CANCEL
/// now, and each attendee gets a cancellation, without the requested time
/// they never heard of.
#[tokio::test]
async fn declining_or_expiring_a_reschedule_withdraws_every_copy() {
    let _env = crate::test_support::ENV_LOCK.lock().await;
    let (app, pool, session, et_id) = setup_test_app().await;
    let (port, mails) = spawn_capturing_mta().await;
    use_capturing_mta(&pool, port).await;
    for (day, path) in [(13, "decline"), (14, "dashboard"), (15, "expiry")] {
        let start = Utc::now() + Duration::days(day);
        // Confirmed first (the insert trigger stamps it), then sent back to
        // pending by a guest reschedule, as `guest_reschedule_booking` does.
        let id = insert_v1_booking(
            &pool,
            &et_id,
            &format!("g-moved-{path}@test.com"),
            start,
            30,
            "confirmed",
            &[],
        )
        .await;
        add_attendee(&pool, &id, &format!("a-moved-{path}@test.com")).await;
        sqlx::query(
            "UPDATE bookings SET status = 'pending', ics_sequence = 1, pending_expires_at = ? WHERE id = ?",
        )
        .bind(utc_stamp(Utc::now() - Duration::minutes(1)))
        .bind(&id)
        .execute(&pool)
        .await
        .unwrap();

        let csrf = "csrf-267-moved";
        let body = format!("_csrf={}", csrf);
        match path {
            "decline" => {
                let request =
                    post_form_unauthed(&format!("/booking/decline/confirm-{id}"), csrf, &body);
                assert_eq!(app.clone().oneshot(request).await.unwrap().status(), 200);
            }
            "dashboard" => {
                let request = post_form(
                    &format!("/dashboard/bookings/{}/cancel", id),
                    &session,
                    csrf,
                    &body,
                );
                let response = app.clone().oneshot(request).await.unwrap();
                assert!(response.status().is_redirection());
            }
            _ => {
                let sweep = sweep_pending_requests(&pool, &[0u8; 32], Utc::now()).await;
                assert_eq!(sweep.expired, 1);
            }
        }
        assert_eq!(booking_status(&pool, &id).await, "declined");

        let guest = settled_mails_to(&mails, &format!("g-moved-{path}@test.com")).await;
        assert_eq!(guest.len(), 1, "{path}: emails to the guest");
        let guest = &guest[0];
        assert!(guest.contains("METHOD:CANCEL"), "{path}: {guest}");
        assert!(guest.contains(&format!("UID:uid-{id}")), "{path}: {guest}");
        // Already above the SEQUENCE:0 copy they hold: the reschedule bumped
        // it, so ending a pending booking does not bump it again.
        assert!(guest.contains("SEQUENCE:1"), "{path}: {guest}");
        assert!(
            guest.contains("A calendar cancellation is attached"),
            "{path}: {guest}"
        );

        let to_attendee = settled_mails_to(&mails, &format!("a-moved-{path}@test.com")).await;
        assert_eq!(to_attendee.len(), 1, "{path}: emails to the attendee");
        let attendee = &to_attendee[0];
        assert!(
            attendee.contains("A booking you attend has been cancelled."),
            "{path}: {attendee}"
        );
        assert!(attendee.contains("METHOD:CANCEL"), "{path}: {attendee}");
        assert!(attendee.contains("SEQUENCE:1"), "{path}: {attendee}");
        assert!(
            !attendee.contains(&start.format("%Y-%m-%d").to_string()),
            "{path}: the requested time is not the one they hold: {attendee}"
        );
    }
}

/// #267: a watcher who claimed a booking is listed as an attendee, and read
/// its later moves and cancellation in the guest's language. They now read
/// them in their own saved one; the other attendees keep the booking's.
#[tokio::test]
async fn a_claimant_reads_attendee_emails_in_their_own_language() {
    let _env = crate::test_support::ENV_LOCK.lock().await;
    let (app, pool, session, et_id) = setup_test_app().await;
    let (port, mails) = spawn_capturing_mta().await;
    use_capturing_mta(&pool, port).await;
    let claimant_id = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO users (id, email, name, role, auth_provider, username, enabled, language) \
             VALUES (?, 'watcher-267@test.com', 'Wanda Watcher', 'user', 'local', 'wanda', 1, 'fr')",
    )
    .bind(&claimant_id)
    .execute(&pool)
    .await
    .unwrap();
    for (day, path) in [(16, "approval"), (17, "cancel")] {
        let id = insert_v1_booking(
            &pool,
            &et_id,
            &format!("g-claim-{path}@test.com"),
            Utc::now() + Duration::days(day),
            30,
            "confirmed",
            &[("claimed_by_user_id", Some(claimant_id.clone()))],
        )
        .await;
        add_attendee(&pool, &id, &format!("a-claim-{path}@test.com")).await;
        // The claim lists the claimant as an attendee (`claim_booking`).
        add_attendee(&pool, &id, "watcher-267@test.com").await;
        mails.lock().unwrap().clear();

        let csrf = "csrf-267-claim";
        let body = format!("_csrf={}", csrf);
        let (english, french) = if path == "approval" {
            sqlx::query("UPDATE bookings SET status = 'pending', ics_sequence = 1 WHERE id = ?")
                .bind(&id)
                .execute(&pool)
                .await
                .unwrap();
            let request = post_form(
                &format!("/dashboard/bookings/{}/confirm", id),
                &session,
                csrf,
                &body,
            );
            app.clone().oneshot(request).await.unwrap();
            (
                "A booking you attend has moved to a new time.",
                "Une r\u{e9}servation \u{e0} laquelle vous participez a \u{e9}t\u{e9} d\u{e9}plac\u{e9}e",
            )
        } else {
            let request = post_form(
                &format!("/dashboard/bookings/{}/cancel", id),
                &session,
                csrf,
                &body,
            );
            app.clone().oneshot(request).await.unwrap();
            (
                "A booking you attend has been cancelled.",
                "Une r\u{e9}servation \u{e0} laquelle vous participez a \u{e9}t\u{e9} annul\u{e9}e.",
            )
        };

        let claimant = settled_mails_to(&mails, "watcher-267@test.com").await;
        assert_eq!(claimant.len(), 1, "{path}: emails to the claimant");
        assert!(claimant[0].contains(french), "{path}: {}", claimant[0]);
        assert!(claimant[0].contains("<html lang=\"fr\">"), "{path}");
        let other = settled_mails_to(&mails, &format!("a-claim-{path}@test.com")).await;
        assert_eq!(other.len(), 1, "{path}: emails to the other attendee");
        assert!(other[0].contains(english), "{path}: {}", other[0]);
    }
}

/// A host cancelling names the host in the guest's headline. The name is
/// the host's own free text, so the HTML part must escape it like every
/// other value: the raw headline may only appear in the plain part.
#[tokio::test]
async fn host_cancellation_escapes_the_host_name_in_the_headline() {
    let _env = crate::test_support::ENV_LOCK.lock().await;
    let (app, pool, session, et_id) = setup_test_app().await;
    let (port, mails) = spawn_capturing_mta().await;
    use_capturing_mta(&pool, port).await;
    let booking_id = insert_pending_french_booking(&pool, &et_id, "uid-cancel-escape").await;
    sqlx::query("UPDATE bookings SET status = 'confirmed', language = 'en', guest_email = 'escape@test.com', start_at = '2099-06-15T10:00:00', end_at = '2099-06-15T10:30:00' WHERE id = ?")
        .bind(&booking_id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE users SET name = '<b>Mallory</b>'")
        .execute(&pool)
        .await
        .unwrap();

    let csrf = "test-csrf-cancel-escape";
    let response = app
        .clone()
        .oneshot(post_form(
            &format!("/dashboard/bookings/{}/cancel", booking_id),
            &session,
            csrf,
            &format!("_csrf={}", csrf),
        ))
        .await
        .unwrap();
    assert!(response.status().is_success() || response.status().is_redirection());

    let mut args = fluent_bundle::FluentArgs::new();
    args.set("host", "<b>Mallory</b>");
    let raw = crate::i18n::translate("en", "email-cancel-headline-by-host", Some(&args));
    let mail = mail_to(&mails, "escape@test.com").await;
    assert_eq!(mail.matches(raw.as_str()).count(), 1, "{mail}");
}
