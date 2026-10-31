// A multi-host booking keeps the organizer it was created with (#161).
// A child of `web::tests`, so it shares that module's helpers.

use super::*;

/// A CalDAV collection on loopback that keeps every PUT body, so a test can
/// read the ORGANIZER each push carried. HEAD answers for stored paths, as
/// the post-write verification (#162) expects.
async fn spawn_recording_dav() -> (String, Arc<std::sync::Mutex<Vec<String>>>) {
    use axum::http::{Method, StatusCode, Uri};
    let puts = Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
    let stored = Arc::new(std::sync::Mutex::new(
        std::collections::HashSet::<String>::new(),
    ));
    let (puts_state, stored_state) = (puts.clone(), stored.clone());
    let app = Router::new().fallback(move |method: Method, uri: Uri, body: String| {
        let puts = puts_state.clone();
        let stored = stored_state.clone();
        async move {
            let path = uri.path().to_string();
            if method == Method::PUT {
                puts.lock().unwrap().push(body);
                stored.lock().unwrap().insert(path);
                StatusCode::CREATED
            } else if method == Method::HEAD {
                if stored.lock().unwrap().contains(&path) {
                    StatusCode::OK
                } else {
                    StatusCode::NOT_FOUND
                }
            } else if method == Method::DELETE {
                stored.lock().unwrap().remove(&path);
                StatusCode::NO_CONTENT
            } else {
                StatusCode::METHOD_NOT_ALLOWED
            }
        }
    });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    (format!("http://127.0.0.1:{port}/dav/"), puts)
}

/// The ORGANIZER line of the last pushed event.
fn last_pushed_organizer(puts: &std::sync::Mutex<Vec<String>>) -> String {
    let puts = puts.lock().unwrap();
    let ics = puts.last().expect("no event was pushed");
    ics.lines()
        .find(|l| l.starts_with("ORGANIZER"))
        .unwrap_or_else(|| panic!("no ORGANIZER in {ics}"))
        .trim_end()
        .to_string()
}

/// Add an enabled team member with an account (so it can own a source).
async fn add_named_member(pool: &SqlitePool, team_id: &str, name: &str, email: &str) -> String {
    let id = uuid::Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO users (id, email, name, role, auth_provider, username, enabled) VALUES (?, ?, ?, 'user', 'local', ?, 1)")
        .bind(&id)
        .bind(email)
        .bind(name)
        .bind(email.split('@').next().unwrap())
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO accounts (id, name, email, timezone, user_id) VALUES (?, ?, ?, 'UTC', ?)",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(name)
    .bind(email)
    .bind(&id)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO team_members (team_id, user_id, role, source) VALUES (?, ?, 'member', 'direct')")
        .bind(team_id)
        .bind(&id)
        .execute(pool)
        .await
        .unwrap();
    id
}

#[allow(clippy::type_complexity)]
async fn stored_organizer(
    pool: &SqlitePool,
    booking_id: &str,
) -> (Option<String>, Option<String>, Option<String>) {
    sqlx::query_as(
        "SELECT organizer_user_id, organizer_name, organizer_email FROM bookings WHERE id = ?",
    )
    .bind(booking_id)
    .fetch_one(pool)
    .await
    .unwrap()
}

/// The guest's one email since the last `clear`, then forget it.
async fn take_guest_mail(mails: &std::sync::Mutex<Vec<CapturedMail>>) -> String {
    let mail = mail_to(mails, "guest@test.com").await;
    // Only the guest's: a host notice sent alongside may already be here,
    // and clearing it raced the background sends (#311, on macOS).
    mails.lock().unwrap().retain(|m| !m.is_to("guest@test.com"));
    mail
}

/// #161 end to end: a collective booking is created with Bob as organizer
/// (first member by name). Ann then joins and sorts first, which used to
/// move the organizer: the approval and the reschedule pushed an event naming
/// Ann (Google answers 403 to that), and the guest's later emails replied to
/// her. The stored organizer keeps both on Bob through approval, reschedule,
/// cancellation and a write-back retry.
#[tokio::test]
async fn collective_booking_keeps_its_organizer_when_the_roster_changes() {
    let _env = crate::test_support::ENV_LOCK.lock().await;
    let (app, pool, _, _) = setup_test_app().await;
    let (port, mails) = spawn_capturing_mta().await;
    use_capturing_mta(&pool, port).await;
    let (team_id, team_et) = seed_team_event_type(&pool).await;
    sqlx::query(
        "UPDATE event_types SET scheduling_mode = 'collective', requires_confirmation = 1 WHERE id = ?",
    )
    .bind(&team_et)
    .execute(&pool)
    .await
    .unwrap();
    let bob = add_named_member(&pool, &team_id, "Bob", "bob@test.com").await;
    let (dav_base, puts) = spawn_recording_dav().await;
    seed_write_source(&pool, &bob, &dav_base).await;

    let csrf = "csrf-161-book";
    let response = app
        .clone()
        .oneshot(post_form_unauthed(
            "/team/horizon-team/team-sync/book",
            csrf,
            &book_body(csrf),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let (booking_id, uid, confirm_token, reschedule_token): (String, String, String, String) =
        sqlx::query_as(
            "SELECT id, uid, confirm_token, reschedule_token FROM bookings WHERE event_type_id = ?",
        )
        .bind(&team_et)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        stored_organizer(&pool, &booking_id).await,
        (
            Some(bob.clone()),
            Some("Bob & Test User".to_string()),
            Some("bob@test.com".to_string())
        )
    );
    let pending = take_guest_mail(&mails).await;
    assert_eq!(mail_header(&pending, "Reply-To"), Some("bob@test.com"));

    // Ann joins and now sorts first by name; Bob is renamed for good measure.
    add_named_member(&pool, &team_id, "Ann", "ann@test.com").await;
    sqlx::query("UPDATE users SET name = 'Robert' WHERE id = ?")
        .bind(&bob)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        booking_host_identity(&pool, &uid).await,
        Some((
            "Bob & Test User".to_string(),
            "bob@test.com".to_string(),
            None
        ))
    );

    // Approval pushes the event and confirms to the guest.
    let response = app
        .clone()
        .oneshot(post_bare(&format!("/booking/approve/{confirm_token}")))
        .await
        .unwrap();
    assert!(response.status().is_success() || response.status().is_redirection());
    assert_eq!(booking_status(&pool, &booking_id).await, "confirmed");
    assert_eq!(
        last_pushed_organizer(&puts),
        "ORGANIZER;CN=Bob & Test User:mailto:bob@test.com"
    );
    let confirmed = take_guest_mail(&mails).await;
    assert_eq!(mail_header(&confirmed, "Reply-To"), Some("bob@test.com"));

    // The guest moves it; no approval needed this time, so it is pushed.
    sqlx::query("UPDATE event_types SET requires_confirmation = 0 WHERE id = ?")
        .bind(&team_et)
        .execute(&pool)
        .await
        .unwrap();
    let pushes_before = puts.lock().unwrap().len();
    let csrf = "csrf-161-resched";
    let response = app
        .clone()
        .oneshot(post_form_unauthed(
            &format!("/booking/reschedule/{reschedule_token}"),
            csrf,
            &format!(
                "_csrf={}&date={}&time=11%3A00&tz=UTC",
                csrf,
                far_future_monday()
            ),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    assert_eq!(booking_status(&pool, &booking_id).await, "confirmed");
    assert!(
        puts.lock().unwrap().len() > pushes_before,
        "reschedule pushed nothing"
    );
    assert_eq!(
        last_pushed_organizer(&puts),
        "ORGANIZER;CN=Bob & Test User:mailto:bob@test.com"
    );
    let rescheduled = take_guest_mail(&mails).await;
    assert_eq!(mail_header(&rescheduled, "Reply-To"), Some("bob@test.com"));

    // A write-back retry rebuilds the event from the row.
    let (details, _) = writeback::writeback_details(&pool, &uid).await.unwrap();
    assert_eq!(
        (details.host_name.as_str(), details.host_email.as_str()),
        ("Bob & Test User", "bob@test.com")
    );

    // The guest cancels: the cancellation still replies to Bob. The
    // reschedule rotated the tokens.
    let cancel_token: String = sqlx::query_scalar("SELECT cancel_token FROM bookings WHERE id = ?")
        .bind(&booking_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    let response = app
        .oneshot(post_form_unauthed(
            &format!("/booking/cancel/{cancel_token}"),
            "csrf-161-cancel",
            "_csrf=csrf-161-cancel",
        ))
        .await
        .unwrap();
    assert!(response.status().is_success() || response.status().is_redirection());
    assert_eq!(booking_status(&pool, &booking_id).await, "cancelled");
    let cancelled = take_guest_mail(&mails).await;
    assert_eq!(mail_header(&cancelled, "Reply-To"), Some("bob@test.com"));

    // Every member hosting it hears about the guest's cancellation, as they
    // did about the booking, not the event type owner alone.
    for member in ["bob@test.com", "ann@test.com", "test@example.com"] {
        assert!(
            wait_for_mail_containing(&mails, member, "cancelled their booking").await,
            "{member} got no cancellation notice"
        );
    }
}

/// Whether a mail to `email` containing `text` arrives within five seconds.
async fn wait_for_mail_containing(
    mails: &std::sync::Mutex<Vec<CapturedMail>>,
    email: &str,
    text: &str,
) -> bool {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        let found = mails
            .lock()
            .unwrap()
            .iter()
            .any(|m| m.is_to(email) && decode_quoted_printable(&m.data).contains(text));
        if found || std::time::Instant::now() >= deadline {
            return found;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
}

/// The snapshot outlives its user: disabling the organizer does not move
/// the event to the next member. Only the language is read live.
#[tokio::test]
async fn stored_organizer_survives_the_organizer_being_disabled() {
    let pool = setup_test_db().await;
    let alice = insert_role_user(&pool, "alice@snap.test", "user").await;
    let bob = insert_role_user(&pool, "bob@snap.test", "user").await;
    let (_team, et_id) = insert_team_with_et(
        &pool,
        "snap",
        "snap-et",
        &[(&alice, "member"), (&bob, "member")],
    )
    .await;
    sqlx::query("UPDATE event_types SET scheduling_mode = 'collective' WHERE id = ?")
        .bind(&et_id)
        .execute(&pool)
        .await
        .unwrap();
    let start = NaiveDate::from_ymd_opt(2026, 3, 18)
        .unwrap()
        .and_hms_opt(10, 0, 0)
        .unwrap();
    insert_booking(&pool, &et_id, None, start, "confirmed").await;
    let uid: String = sqlx::query_scalar("SELECT uid FROM bookings WHERE event_type_id = ?")
        .bind(&et_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    sqlx::query(
        "UPDATE bookings SET organizer_user_id = ?, organizer_name = 'Alice & Bob', \
         organizer_email = 'alice@snap.test' WHERE uid = ?",
    )
    .bind(&alice)
    .bind(&uid)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("UPDATE users SET language = 'fr' WHERE id = ?")
        .bind(&alice)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        booking_host_identity(&pool, &uid).await,
        Some((
            "Alice & Bob".to_string(),
            "alice@snap.test".to_string(),
            Some("fr".to_string())
        ))
    );

    sqlx::query("UPDATE users SET enabled = 0 WHERE id = ?")
        .bind(&alice)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        booking_host_identity(&pool, &uid).await,
        Some((
            "Alice & Bob".to_string(),
            "alice@snap.test".to_string(),
            Some("fr".to_string())
        ))
    );
}

/// Rows from before migration 070 have no snapshot and keep the previous
/// behaviour: the organizer is recomputed from the current roster.
#[tokio::test]
async fn legacy_collective_booking_still_follows_the_current_roster() {
    let pool = setup_test_db().await;
    let bob = insert_role_user(&pool, "bob@legacy.test", "user").await;
    let carol = insert_role_user(&pool, "carol@legacy.test", "user").await;
    let (team_id, et_id) = insert_team_with_et(
        &pool,
        "legacy",
        "legacy-et",
        &[(&bob, "member"), (&carol, "member")],
    )
    .await;
    sqlx::query("UPDATE event_types SET scheduling_mode = 'collective' WHERE id = ?")
        .bind(&et_id)
        .execute(&pool)
        .await
        .unwrap();
    let start = NaiveDate::from_ymd_opt(2026, 3, 19)
        .unwrap()
        .and_hms_opt(10, 0, 0)
        .unwrap();
    insert_booking(&pool, &et_id, None, start, "confirmed").await;
    let uid: String = sqlx::query_scalar("SELECT uid FROM bookings WHERE event_type_id = ?")
        .bind(&et_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        booking_host_identity(&pool, &uid).await.map(|(_, e, _)| e),
        Some("bob@legacy.test".to_string())
    );

    let aaron = insert_role_user(&pool, "aaron@legacy.test", "user").await;
    sqlx::query("INSERT INTO team_members (team_id, user_id, role, source) VALUES (?, ?, 'member', 'direct')")
        .bind(&team_id)
        .bind(&aaron)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        booking_host_identity(&pool, &uid).await.map(|(_, e, _)| e),
        Some("aaron@legacy.test".to_string()),
        "a row without a snapshot is resolved as before"
    );
}

/// A dynamic group booking (`a+b`) stores the joined roster and the first
/// user's address. Before, every later push resolved the event type owner
/// alone, so the ORGANIZER name lost the other hosts after an approval or a
/// reschedule.
#[tokio::test]
async fn dynamic_group_booking_stores_its_organizer() {
    let (app, pool, _, et_id) = setup_test_app().await;
    let second = uuid::Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO users (id, email, name, role, auth_provider, username, enabled) VALUES (?, 'second@example.com', 'Second User', 'user', 'local', 'seconduser', 1)")
        .bind(&second)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO accounts (id, name, email, timezone, user_id) VALUES (?, 'Second User', 'second@example.com', 'UTC', ?)")
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(&second)
        .execute(&pool)
        .await
        .unwrap();
    let owner: String = sqlx::query_scalar("SELECT id FROM users WHERE username = 'testuser'")
        .fetch_one(&pool)
        .await
        .unwrap();

    let csrf = "csrf-161-dyn";
    let response = app
        .oneshot(post_form_unauthed(
            "/u/testuser+seconduser/test-meeting/book",
            csrf,
            &book_body(csrf),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let (booking_id, uid): (String, String) =
        sqlx::query_as("SELECT id, uid FROM bookings WHERE event_type_id = ?")
            .bind(&et_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        stored_organizer(&pool, &booking_id).await,
        (
            Some(owner),
            Some("Test User & Second User".to_string()),
            Some("test@example.com".to_string())
        )
    );
    assert_eq!(
        booking_host_identity(&pool, &uid)
            .await
            .map(|(n, e, _)| (n, e)),
        Some((
            "Test User & Second User".to_string(),
            "test@example.com".to_string()
        ))
    );
}

/// Single-host bookings resolve through a stable user id and store nothing.
#[tokio::test]
async fn personal_and_round_robin_bookings_store_no_organizer() {
    let (app, pool, _, et_id) = setup_test_app().await;
    let csrf = "csrf-161-personal";
    app.clone()
        .oneshot(post_form_unauthed(
            "/u/testuser/test-meeting/book",
            csrf,
            &book_body(csrf),
        ))
        .await
        .unwrap();
    let (_, team_et) = seed_team_event_type(&pool).await;
    // Another hour: testuser is also the team's only member.
    let csrf = "csrf-161-rr";
    app.oneshot(post_form_unauthed(
        "/team/horizon-team/team-sync/book",
        csrf,
        &book_body(csrf).replace("time=10:00", "time=13:00"),
    ))
    .await
    .unwrap();
    for et in [&et_id, &team_et] {
        let id: String = sqlx::query_scalar("SELECT id FROM bookings WHERE event_type_id = ?")
            .bind(et)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(stored_organizer(&pool, &id).await, (None, None, None));
    }
}

/// The ORGANIZER line of the calendar attachment in a captured email,
/// whatever transfer encoding lettre picked for it.
fn ics_organizer_in_mail(mail: &str) -> Option<String> {
    use base64::Engine;
    let find = |text: &str| {
        text.lines()
            .find(|l| l.starts_with("ORGANIZER"))
            .map(|l| l.trim_end().to_string())
    };
    if let Some(line) = find(mail) {
        return Some(line);
    }
    // Base64 parts: decode every run of base64 lines and look inside.
    let mut chunk = String::new();
    for line in mail.lines().chain(std::iter::once("")) {
        let line = line.trim_end();
        let is_b64 = !line.is_empty()
            && line
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '/' || c == '=');
        if is_b64 {
            chunk.push_str(line);
            continue;
        }
        if !chunk.is_empty() {
            if let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(&chunk) {
                if let Some(found) = find(&String::from_utf8_lossy(&bytes)) {
                    return Some(found);
                }
            }
            chunk.clear();
        }
    }
    None
}

/// A collective booking cancelled from the dashboard by a member who is not
/// its organizer: the guest's CANCEL must still name the original organizer.
/// Calendar clients match a cancellation on UID + ORGANIZER and may ignore
/// one from someone else, leaving the event in the guest's calendar. The
/// decline of a pending request replies to the organizer the same way.
#[tokio::test]
async fn dashboard_cancel_by_another_member_keeps_the_original_organizer() {
    let _env = crate::test_support::ENV_LOCK.lock().await;
    let (app, pool, session, _) = setup_test_app().await;
    let (port, mails) = spawn_capturing_mta().await;
    use_capturing_mta(&pool, port).await;
    let (team_id, team_et) = seed_team_event_type(&pool).await;
    sqlx::query("UPDATE event_types SET scheduling_mode = 'collective' WHERE id = ?")
        .bind(&team_et)
        .execute(&pool)
        .await
        .unwrap();
    // Bob sorts before "Test User", the event type owner who will cancel.
    add_named_member(&pool, &team_id, "Bob", "bob@test.com").await;

    let csrf = "csrf-161-dash";
    let response = app
        .clone()
        .oneshot(post_form_unauthed(
            "/team/horizon-team/team-sync/book",
            csrf,
            &book_body(csrf),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let booking_id: String = sqlx::query_scalar("SELECT id FROM bookings WHERE event_type_id = ?")
        .bind(&team_et)
        .fetch_one(&pool)
        .await
        .unwrap();
    let confirmation = take_guest_mail(&mails).await;
    let original = ics_organizer_in_mail(&confirmation)
        .unwrap_or_else(|| panic!("no ORGANIZER in the confirmation: {confirmation}"));
    assert_eq!(original, "ORGANIZER;CN=Bob & Test User:mailto:bob@test.com");

    // Ann joins and sorts first, so even a fresh resolution would differ.
    add_named_member(&pool, &team_id, "Ann", "ann@test.com").await;

    let csrf = "csrf-161-dash-cancel";
    let response = app
        .clone()
        .oneshot(post_form(
            &format!("/dashboard/bookings/{booking_id}/cancel"),
            &session,
            csrf,
            &format!("_csrf={csrf}"),
        ))
        .await
        .unwrap();
    assert!(response.status().is_redirection());
    assert_eq!(booking_status(&pool, &booking_id).await, "cancelled");
    let cancelled = take_guest_mail(&mails).await;
    assert_eq!(
        ics_organizer_in_mail(&cancelled).as_deref(),
        Some(original.as_str()),
        "{cancelled}"
    );
    assert_eq!(mail_header(&cancelled, "Reply-To"), Some("bob@test.com"));

    // A request made after Ann joined has her as organizer; declined from
    // the dashboard by the owner, it still replies to her, not to him.
    sqlx::query("UPDATE event_types SET requires_confirmation = 1 WHERE id = ?")
        .bind(&team_et)
        .execute(&pool)
        .await
        .unwrap();
    let csrf = "csrf-161-dash-pending";
    app.clone()
        .oneshot(post_form_unauthed(
            "/team/horizon-team/team-sync/book",
            csrf,
            &book_body(csrf).replace("time=10:00", "time=14:00"),
        ))
        .await
        .unwrap();
    let pending_id: String = sqlx::query_scalar(
        "SELECT id FROM bookings WHERE event_type_id = ? AND status = 'pending'",
    )
    .bind(&team_et)
    .fetch_one(&pool)
    .await
    .unwrap();
    take_guest_mail(&mails).await;
    let csrf = "csrf-161-dash-decline";
    app.oneshot(post_form(
        &format!("/dashboard/bookings/{pending_id}/cancel"),
        &session,
        csrf,
        &format!("_csrf={csrf}"),
    ))
    .await
    .unwrap();
    assert_eq!(booking_status(&pool, &pending_id).await, "declined");
    let declined = take_guest_mail(&mails).await;
    assert_eq!(mail_header(&declined, "Reply-To"), Some("ann@test.com"));
}
