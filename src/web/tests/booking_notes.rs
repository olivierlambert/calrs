//! Host notes on a booking (#173): who may read and edit them, the page and
//! its POST, and the guest surfaces that must never show them.

use super::*;

const SECRET: &str = "HOST-ONLY-173 call the CFO first";

async fn set_host_notes(pool: &SqlitePool, booking_id: &str, notes: &str) {
    sqlx::query("UPDATE bookings SET host_notes = ? WHERE id = ?")
        .bind(notes)
        .bind(booking_id)
        .execute(pool)
        .await
        .unwrap();
}

async fn stored_host_notes(pool: &SqlitePool, booking_id: &str) -> Option<String> {
    sqlx::query_scalar("SELECT host_notes FROM bookings WHERE id = ?")
        .bind(booking_id)
        .fetch_one(pool)
        .await
        .unwrap()
}

/// A confirmed booking of the fixture's personal event type, a day out.
async fn future_booking(pool: &SqlitePool, et_id: &str) -> (String, String, String) {
    let start = Utc::now() + Duration::days(1);
    let id = insert_v1_booking(pool, et_id, "guest@test.com", start, 30, "confirmed", &[]).await;
    let (cancel, resched): (String, String) =
        sqlx::query_as("SELECT cancel_token, reschedule_token FROM bookings WHERE id = ?")
            .bind(&id)
            .fetch_one(pool)
            .await
            .unwrap();
    (id, cancel, resched)
}

fn notes_body(csrf: &str, notes: &str, version: i64) -> String {
    format!(
        "_csrf={csrf}&host_notes={}&version={version}&back=bookings",
        urlencoding::encode(notes),
    )
}

async fn stored_version(pool: &SqlitePool, booking_id: &str) -> i64 {
    sqlx::query_scalar("SELECT host_notes_version FROM bookings WHERE id = ?")
        .bind(booking_id)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn owner_id(pool: &SqlitePool) -> String {
    sqlx::query_scalar("SELECT id FROM users WHERE username = 'testuser'")
        .fetch_one(pool)
        .await
        .unwrap()
}

#[tokio::test]
async fn owner_reads_saves_and_clears_host_notes() {
    let (app, pool, session, et_id) = setup_test_app().await;
    let (id, _, _) = future_booking(&pool, &et_id).await;
    let uri = format!("/dashboard/bookings/{id}/notes");

    let response = app
        .clone()
        .oneshot(get_authed(&uri, &session))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = body_string(response).await;
    assert!(body.contains(r#"name="host_notes""#), "{body}");
    assert!(
        body.contains("a team member who claimed it"),
        "the hint names every reader"
    );
    assert!(
        !body.contains("maxlength"),
        "no UTF-16 limit in the browser"
    );
    assert!(
        body.contains(r#"data-max="5000""#),
        "a counter counts like the server"
    );

    let csrf = "csrf-173-save";
    let response = app
        .clone()
        .oneshot(post_form(
            &uri,
            &session,
            csrf,
            &notes_body(csrf, &format!("  {SECRET}\r\nline two  "), 0),
        ))
        .await
        .unwrap();
    assert!(response.status().is_redirection());
    let location = response.headers()["location"].to_str().unwrap().to_string();
    assert_eq!(location, format!("{uri}?saved=1&back=bookings"));
    assert_eq!(
        stored_host_notes(&pool, &id).await.as_deref(),
        Some(format!("{SECRET}\nline two").as_str()),
        "trimmed, line endings normalised"
    );
    let by: Option<String> =
        sqlx::query_scalar("SELECT host_notes_updated_by FROM bookings WHERE id = ?")
            .bind(&id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(by, Some(owner_id(&pool).await));

    let response = app
        .clone()
        .oneshot(get_authed(&location, &session))
        .await
        .unwrap();
    let body = body_string(response).await;
    assert!(body.contains("Notes saved."), "{body}");
    assert!(body.contains("Last edited by Test User"), "{body}");
    assert!(body.contains(SECRET));

    // The bookings page shows them on the row, with an edit link.
    let response = app
        .clone()
        .oneshot(get_authed("/dashboard/bookings", &session))
        .await
        .unwrap();
    let body = body_string(response).await;
    assert!(body.contains(SECRET), "bookings page shows the host notes");
    assert!(body.contains(&format!("/dashboard/bookings/{id}/notes?back=bookings")));
    assert!(body.contains("Edit note"));

    // Saving an empty field clears them.
    let base = stored_version(&pool, &id).await;
    assert_eq!(base, 1, "one save so far");
    let response = app
        .oneshot(post_form(
            &uri,
            &session,
            csrf,
            &notes_body(csrf, "   ", base),
        ))
        .await
        .unwrap();
    assert!(response.status().is_redirection());
    assert_eq!(stored_host_notes(&pool, &id).await, None);
}

#[tokio::test]
async fn saving_host_notes_needs_the_csrf_token() {
    let (app, pool, session, et_id) = setup_test_app().await;
    let (id, _, _) = future_booking(&pool, &et_id).await;
    let response = app
        .oneshot(post_form(
            &format!("/dashboard/bookings/{id}/notes"),
            &session,
            "cookie-token",
            &notes_body("other-token", SECRET, 0),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), 403);
    assert_eq!(stored_host_notes(&pool, &id).await, None);
}

#[tokio::test]
async fn host_notes_page_needs_a_session() {
    let (app, pool, _, et_id) = setup_test_app().await;
    let (id, _, _) = future_booking(&pool, &et_id).await;
    let response = app
        .oneshot(get(&format!("/dashboard/bookings/{id}/notes")))
        .await
        .unwrap();
    assert!(response.status().is_redirection(), "redirected to login");
}

#[tokio::test]
async fn a_stranger_can_neither_read_nor_write_host_notes() {
    let (app, pool, _, et_id) = setup_test_app().await;
    let (id, _, _) = future_booking(&pool, &et_id).await;
    set_host_notes(&pool, &id, SECRET).await;
    let (_, stranger) =
        insert_test_user_with_session(&pool, "Mallory", "mallory@test.com", "mallory").await;
    let uri = format!("/dashboard/bookings/{id}/notes");

    let response = app
        .clone()
        .oneshot(get_authed(&uri, &stranger))
        .await
        .unwrap();
    assert_eq!(response.status(), 404);
    assert!(!body_string(response).await.contains(SECRET));

    let csrf = "csrf-173-stranger";
    let response = app
        .clone()
        .oneshot(post_form(
            &uri,
            &stranger,
            csrf,
            &notes_body(csrf, "overwritten", 0),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), 404);
    assert_eq!(stored_host_notes(&pool, &id).await.as_deref(), Some(SECRET));

    // An unknown booking answers the same.
    let response = app
        .oneshot(get_authed("/dashboard/bookings/nope/notes", &stranger))
        .await
        .unwrap();
    assert_eq!(response.status(), 404);
}

#[tokio::test]
async fn too_long_host_notes_are_refused_and_kept_in_the_field() {
    let (app, pool, session, et_id) = setup_test_app().await;
    let (id, _, _) = future_booking(&pool, &et_id).await;
    let long = "é".repeat(HOST_NOTES_MAX_CHARS + 1);
    let csrf = "csrf-173-long";
    let response = app
        .clone()
        .oneshot(post_form(
            &format!("/dashboard/bookings/{id}/notes"),
            &session,
            csrf,
            &notes_body(csrf, &long, 0),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = body_string(response).await;
    assert!(body.contains("at most 5000 characters"), "{body}");
    assert!(body.contains(&long), "the text is not lost");
    assert_eq!(stored_host_notes(&pool, &id).await, None);

    // Exactly the limit (multi-byte characters count once) is accepted.
    let fits = "é".repeat(HOST_NOTES_MAX_CHARS);
    let response = app
        .oneshot(post_form(
            &format!("/dashboard/bookings/{id}/notes"),
            &session,
            csrf,
            &notes_body(csrf, &fits, 0),
        ))
        .await
        .unwrap();
    assert!(response.status().is_redirection());
    assert_eq!(stored_host_notes(&pool, &id).await, Some(fits));
}

#[tokio::test]
async fn a_save_from_a_stale_page_does_not_overwrite_a_newer_one() {
    let (app, pool, session, et_id) = setup_test_app().await;
    let (id, _, _) = future_booking(&pool, &et_id).await;
    let uri = format!("/dashboard/bookings/{id}/notes");
    let csrf = "csrf-173-stale";
    let first = app
        .clone()
        .oneshot(post_form(
            &uri,
            &session,
            csrf,
            &notes_body(csrf, "first", 0),
        ))
        .await
        .unwrap();
    assert!(first.status().is_redirection());

    // Loaded before the first save: its version is still 0.
    let response = app
        .oneshot(post_form(
            &uri,
            &session,
            csrf,
            &notes_body(csrf, "second", 0),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = body_string(response).await;
    assert!(
        body.contains("were changed after this page was opened"),
        "{body}"
    );
    assert!(body.contains("first"), "the saved version is shown");
    assert!(
        body.contains("second"),
        "the submitted text stays in the field"
    );
    assert_eq!(
        stored_host_notes(&pool, &id).await.as_deref(),
        Some("first")
    );
    assert!(
        body.contains(r#"name="version" value="1""#),
        "the refusal hands out the current version"
    );
}

/// The other host cleared the notes: the refusal says so rather than show
/// an empty "saved version", and its version is the one read after the
/// refused save.
#[tokio::test]
async fn a_stale_save_after_the_notes_were_cleared_says_so() {
    let (app, pool, session, et_id) = setup_test_app().await;
    let (id, _, _) = future_booking(&pool, &et_id).await;
    let uri = format!("/dashboard/bookings/{id}/notes");
    let csrf = "csrf-173-cleared";
    for (text, version) in [("first", 0), ("", 1)] {
        let response = app
            .clone()
            .oneshot(post_form(
                &uri,
                &session,
                csrf,
                &notes_body(csrf, text, version),
            ))
            .await
            .unwrap();
        assert!(response.status().is_redirection());
    }
    assert_eq!(stored_host_notes(&pool, &id).await, None);

    let response = app
        .oneshot(post_form(
            &uri,
            &session,
            csrf,
            &notes_body(csrf, "mine", 0),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = body_string(response).await;
    assert!(body.contains("(cleared)"), "{body}");
    assert!(body.contains(r#"name="version" value="2""#), "{body}");
    assert_eq!(stored_host_notes(&pool, &id).await, None);
}

/// A missing or garbled version is refused as stale, never applied.
#[tokio::test]
async fn a_save_without_a_version_is_refused() {
    let (app, pool, session, et_id) = setup_test_app().await;
    let (id, _, _) = future_booking(&pool, &et_id).await;
    let csrf = "csrf-173-noversion";
    let response = app
        .oneshot(post_form(
            &format!("/dashboard/bookings/{id}/notes"),
            &session,
            csrf,
            &format!("_csrf={csrf}&host_notes=x&version=abc"),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    assert_eq!(stored_host_notes(&pool, &id).await, None);
}

/// An admin impersonating the owner is recorded as the author, with the
/// owner as whom they acted.
#[tokio::test]
async fn a_save_made_while_impersonating_records_the_admin() {
    let (app, pool, _, et_id) = setup_test_app().await;
    let (id, _, _) = future_booking(&pool, &et_id).await;
    let owner = owner_id(&pool).await;
    let (admin, admin_session) =
        insert_test_user_with_session(&pool, "Root Admin", "root@test.com", "root").await;
    sqlx::query("UPDATE users SET role = 'admin' WHERE id = ?")
        .bind(&admin)
        .execute(&pool)
        .await
        .unwrap();
    let csrf = "csrf-173-imp";
    let cookie = format!(
        "__Host-calrs_session={admin_session}; __Host-calrs_csrf={csrf}; __Host-calrs_impersonate={owner}"
    );
    let uri = format!("/dashboard/bookings/{id}/notes");
    let request = axum::http::Request::builder()
        .method("POST")
        .uri(&uri)
        .header("cookie", &cookie)
        .header("content-type", "application/x-www-form-urlencoded")
        .body(Body::from(notes_body(csrf, SECRET, 0)))
        .unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
    assert!(response.status().is_redirection());
    let (by, as_user): (Option<String>, Option<String>) = sqlx::query_as(
        "SELECT host_notes_updated_by, host_notes_updated_as FROM bookings WHERE id = ?",
    )
    .bind(&id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(by, Some(admin));
    assert_eq!(as_user, Some(owner));

    let request = axum::http::Request::builder()
        .uri(&uri)
        .header("cookie", &cookie)
        .body(Body::empty())
        .unwrap();
    let body = body_string(app.oneshot(request).await.unwrap()).await;
    assert!(
        body.contains("Last edited by Root Admin (admin, as Test User)"),
        "{body}"
    );
}

/// The 404 is the dashboard's page, the same for a booking that does not
/// exist and for one that is not the user's.
#[tokio::test]
async fn the_notes_404_is_the_dashboard_page_whatever_the_reason() {
    let (app, pool, _, et_id) = setup_test_app().await;
    let (id, _, _) = future_booking(&pool, &et_id).await;
    let (_, stranger) = insert_test_user_with_session(&pool, "Eve", "eve@test.com", "eve").await;
    let mut bodies = Vec::new();
    for uri in [
        format!("/dashboard/bookings/{id}/notes"),
        "/dashboard/bookings/does-not-exist/notes".to_string(),
    ] {
        let response = app
            .clone()
            .oneshot(get_authed(&uri, &stranger))
            .await
            .unwrap();
        assert_eq!(response.status(), 404, "{uri}");
        let body = body_string(response).await;
        assert!(body.contains("Not found"), "{body}");
        assert!(
            body.contains("dashboard-layout") || body.contains("sidebar"),
            "{body}"
        );
        bodies.push(body);
    }
    assert_eq!(bodies[0], bodies[1], "nothing tells the two apart");
}

/// Rows without a note keep their actions: no note button, the title opens
/// the notes page. The note button appears once a note exists.
#[tokio::test]
async fn rows_offer_the_note_button_only_when_there_is_a_note() {
    let (app, pool, session, et_id) = setup_test_app().await;
    let (id, _, _) = future_booking(&pool, &et_id).await;
    let body = body_string(
        app.clone()
            .oneshot(get_authed("/dashboard/bookings", &session))
            .await
            .unwrap(),
    )
    .await;
    assert!(!body.contains("Edit note"), "no note yet");
    assert!(
        body.contains(&format!(
            r#"<a class="tt-link" href="/dashboard/bookings/{id}/notes?back=bookings">"#
        )),
        "{body}"
    );
    set_host_notes(&pool, &id, SECRET).await;
    let body = body_string(
        app.oneshot(get_authed("/dashboard/bookings", &session))
            .await
            .unwrap(),
    )
    .await;
    assert!(body.contains("Edit note"));
}

/// Who may read and edit, per the module documentation of `booking_notes`.
#[tokio::test]
async fn host_notes_access_follows_who_hosts_the_booking() {
    let pool = setup_test_db().await;
    let owner = insert_role_user(&pool, "owner@t.com", "user").await;
    let assignee = insert_role_user(&pool, "assignee@t.com", "user").await;
    let peer = insert_role_user(&pool, "peer@t.com", "user").await;
    let team_admin = insert_role_user(&pool, "teamadmin@t.com", "user").await;
    let excluded = insert_role_user(&pool, "excluded@t.com", "user").await;
    let watcher = insert_role_user(&pool, "watcher@t.com", "user").await;
    let claimant = insert_role_user(&pool, "claimant@t.com", "user").await;
    let instance_admin = insert_role_user(&pool, "root@t.com", "admin").await;

    // Personal event type of `owner`.
    let personal = insert_personal_et(&pool, &owner, "p").await;
    let start = Utc::now() + Duration::days(2);
    let personal_booking =
        insert_v1_booking(&pool, &personal, "g@t.com", start, 30, "confirmed", &[]).await;

    // A team event type created by `owner` (stored under their account).
    let (_, team_et) = insert_team_with_et(
        &pool,
        "sales",
        "demo",
        &[
            (&owner, "admin"),
            (&assignee, "member"),
            (&peer, "member"),
            (&team_admin, "admin"),
            (&excluded, "member"),
        ],
    )
    .await;
    let owner_account: String = sqlx::query_scalar("SELECT id FROM accounts WHERE user_id = ?")
        .bind(&owner)
        .fetch_one(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE event_types SET account_id = ? WHERE id = ?")
        .bind(&owner_account)
        .bind(&team_et)
        .execute(&pool)
        .await
        .unwrap();
    // The same team's collective event type, also the owner's.
    let collective_et = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO event_types (id, account_id, team_id, slug, title, duration_min, buffer_before, buffer_after, min_notice_min, enabled, scheduling_mode) \
         SELECT ?, account_id, team_id, 'all-hands', 'All hands', 30, 0, 0, 0, 1, 'collective' FROM event_types WHERE id = ?",
    )
    .bind(&collective_et)
    .bind(&team_et)
    .execute(&pool)
    .await
    .unwrap();
    for et in [&team_et, &collective_et] {
        sqlx::query(
            "INSERT INTO event_type_member_weights (event_type_id, user_id, weight) VALUES (?, ?, 0)",
        )
        .bind(et)
        .bind(&excluded)
        .execute(&pool)
        .await
        .unwrap();
    }

    let round_robin = insert_v1_booking(
        &pool,
        &team_et,
        "g@t.com",
        start,
        30,
        "confirmed",
        &[("assigned_user_id", Some(assignee.clone()))],
    )
    .await;
    let collective = insert_v1_booking(
        &pool,
        &collective_et,
        "g@t.com",
        start + Duration::hours(2),
        30,
        "confirmed",
        &[],
    )
    .await;
    // A round-robin booking whose assigned member is gone (ON DELETE SET
    // NULL), or a legacy row: unassigned, but not collective.
    let orphaned = insert_v1_booking(
        &pool,
        &team_et,
        "g@t.com",
        start + Duration::hours(4),
        30,
        "confirmed",
        &[],
    )
    .await;

    // A watcher team offered the round-robin booking, `claimant` took a
    // copy of the collective one.
    let watchers = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO teams (id, name, slug, visibility) VALUES (?, 'w', 'watchers', 'public')",
    )
    .bind(&watchers)
    .execute(&pool)
    .await
    .unwrap();
    for member in [&watcher, &claimant] {
        sqlx::query("INSERT INTO team_members (team_id, user_id, role, source) VALUES (?, ?, 'member', 'direct')")
            .bind(&watchers)
            .bind(member)
            .execute(&pool)
            .await
            .unwrap();
    }
    sqlx::query("INSERT INTO event_type_watchers (event_type_id, team_id) VALUES (?, ?)")
        .bind(&team_et)
        .bind(&watchers)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO booking_claim_tokens (id, booking_id, user_id, token, expires_at) VALUES ('ct', ?, ?, 'tok', datetime('now', '+1 day'))")
        .bind(&round_robin)
        .bind(&watcher)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE bookings SET claimed_by_user_id = ? WHERE id = ?")
        .bind(&claimant)
        .bind(&collective)
        .execute(&pool)
        .await
        .unwrap();

    let may = |booking: &str, user: &str| {
        let pool = pool.clone();
        let booking = booking.to_string();
        let user = user.to_string();
        async move { booking_notes_access(&pool, &booking, &user).await }
    };

    // Personal booking: its owner only.
    assert!(may(&personal_booking, &owner).await);
    for user in [&assignee, &peer, &team_admin, &instance_admin] {
        assert!(!may(&personal_booking, user).await);
    }

    // Round robin: the owner and the assigned member; not the other
    // members, not a team admin, not a watcher only offered it.
    assert!(may(&round_robin, &owner).await);
    assert!(may(&round_robin, &assignee).await);
    for user in [
        &peer,
        &team_admin,
        &excluded,
        &watcher,
        &claimant,
        &instance_admin,
    ] {
        assert!(!may(&round_robin, user).await);
    }

    // Unassigned round robin: the owner only, never the whole team.
    assert!(may(&orphaned, &owner).await);
    for user in [&assignee, &peer, &team_admin, &excluded, &watcher] {
        assert!(!may(&orphaned, user).await);
    }

    // Collective: every member who hosts it, and its claimant; not an
    // excluded member, not a watcher without the claim.
    for user in [&owner, &assignee, &peer, &team_admin, &claimant] {
        assert!(may(&collective, user).await);
    }
    for user in [&excluded, &watcher, &instance_admin] {
        assert!(!may(&collective, user).await);
    }
}

async fn booking_notes_access(pool: &SqlitePool, booking_id: &str, user_id: &str) -> bool {
    note_booking(pool, booking_id, user_id)
        .await
        .unwrap()
        .is_some()
}

/// An assigned member reaches the notes from the overview, where the
/// booking is listed for them, though the bookings page (the owner's) is not.
#[tokio::test]
async fn the_overview_links_every_host_to_the_notes() {
    let (app, pool, _, _) = setup_test_app().await;
    let (alice, alice_session) =
        insert_test_user_with_session(&pool, "Alice", "alice@test.com", "alice").await;
    let owner = owner_id(&pool).await;
    let (_, team_et) = insert_team_with_et(
        &pool,
        "t173",
        "rr",
        &[(&owner, "admin"), (&alice, "member")],
    )
    .await;
    let id = insert_v1_booking(
        &pool,
        &team_et,
        "g@t.com",
        Utc::now() + Duration::hours(3),
        30,
        "confirmed",
        &[("assigned_user_id", Some(alice.clone()))],
    )
    .await;
    set_host_notes(&pool, &id, SECRET).await;

    let response = app
        .clone()
        .oneshot(get_authed("/dashboard", &alice_session))
        .await
        .unwrap();
    let body = body_string(response).await;
    assert!(
        body.contains(&format!("/dashboard/bookings/{id}/notes?back=overview")),
        "{body}"
    );
    assert!(body.contains("Host note"), "the agenda marks the note");
    assert!(!body.contains("Edit note"), "no button on agenda rows");

    let response = app
        .oneshot(get_authed(
            &format!("/dashboard/bookings/{id}/notes?back=overview"),
            &alice_session,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = body_string(response).await;
    assert!(body.contains(SECRET));
    assert!(body.contains(r#"name="back" value="overview""#), "{body}");
}

/// Guest token pages and the guest ICS download never carry the notes.
#[tokio::test]
async fn guest_pages_and_the_ics_never_show_host_notes() {
    let (app, pool, _, et_id) = setup_test_app().await;
    let (id, cancel, resched) = future_booking(&pool, &et_id).await;
    set_host_notes(&pool, &id, SECRET).await;

    for uri in [
        format!("/booking/cancel/{cancel}"),
        format!("/booking/reschedule/{resched}"),
        format!("/booking/ics/{cancel}"),
    ] {
        let response = app.clone().oneshot(get(&uri)).await.unwrap();
        assert_eq!(response.status(), 200, "{uri}");
        let body = body_string(response).await;
        assert!(
            !body.contains("HOST-ONLY-173"),
            "{uri} shows the host notes"
        );
    }
}

/// A guest reschedule moves the row and keeps the notes; the page the guest
/// lands on does not show them. A cancellation keeps them too.
#[tokio::test]
async fn host_notes_survive_a_reschedule_and_a_cancellation() {
    let (app, pool, session, et_id) = setup_test_app().await;
    let (id, _, resched) = future_booking(&pool, &et_id).await;
    set_host_notes(&pool, &id, SECRET).await;

    let csrf = "csrf-173-resched";
    let response = app
        .clone()
        .oneshot(post_form_unauthed(
            &format!("/booking/reschedule/{resched}"),
            csrf,
            &format!(
                "_csrf={csrf}&date={}&time=10%3A00&tz=UTC",
                far_future_monday()
            ),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = body_string(response).await;
    assert!(!body.contains("HOST-ONLY-173"), "{body}");
    let start: String = sqlx::query_scalar("SELECT start_at FROM bookings WHERE id = ?")
        .bind(&id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(start.starts_with(&far_future_monday()), "moved: {start}");
    assert_eq!(stored_host_notes(&pool, &id).await.as_deref(), Some(SECRET));

    let csrf = "csrf-173-cancel";
    let response = app
        .oneshot(post_form(
            &format!("/dashboard/bookings/{id}/cancel"),
            &session,
            csrf,
            &format!("_csrf={csrf}"),
        ))
        .await
        .unwrap();
    assert!(response.status().is_redirection());
    assert_eq!(booking_status(&pool, &id).await, "cancelled");
    assert_eq!(stored_host_notes(&pool, &id).await.as_deref(), Some(SECRET));
}

/// The host's reminder carries the notes; the guest's, sent in the same
/// pass from the same details, does not.
#[tokio::test]
async fn reminders_show_host_notes_to_the_host_only() {
    let _env = crate::test_support::ENV_LOCK.lock().await;
    let (_app, pool, _, et_id) = setup_test_app().await;
    let (port, mails) = spawn_capturing_mta().await;
    use_capturing_mta(&pool, port).await;
    sqlx::query("UPDATE event_types SET reminder_minutes = 60 WHERE id = ?")
        .bind(&et_id)
        .execute(&pool)
        .await
        .unwrap();
    let id = insert_v1_booking(
        &pool,
        &et_id,
        "guest@test.com",
        Utc::now() + Duration::minutes(30),
        30,
        "confirmed",
        &[],
    )
    .await;
    set_host_notes(&pool, &id, SECRET).await;

    send_due_reminders(pool.clone(), &[0u8; 32]).await;

    let host = mail_to(&mails, "test@example.com").await;
    assert!(host.contains(SECRET), "{host}");
    assert!(host.contains("Host notes:"), "{host}");
    let guest = mail_to(&mails, "guest@test.com").await;
    assert!(!guest.contains("HOST-ONLY-173"), "{guest}");
}

/// Approving and cancelling from the dashboard: the host's emails carry the
/// notes, the guest's emails and their ICS attachments never do.
#[tokio::test]
async fn approval_and_cancellation_emails_show_host_notes_to_the_host_only() {
    let _env = crate::test_support::ENV_LOCK.lock().await;
    let (app, pool, session, et_id) = setup_test_app().await;
    let (port, mails) = spawn_capturing_mta().await;
    use_capturing_mta(&pool, port).await;
    let id = insert_v1_booking(
        &pool,
        &et_id,
        "guest@test.com",
        Utc::now() + Duration::days(3),
        30,
        "pending",
        &[],
    )
    .await;
    set_host_notes(&pool, &id, SECRET).await;

    let csrf = "csrf-173-approve";
    let response = app
        .clone()
        .oneshot(post_form(
            &format!("/dashboard/bookings/{id}/confirm"),
            &session,
            csrf,
            &format!("_csrf={csrf}"),
        ))
        .await
        .unwrap();
    assert!(response.status().is_redirection());
    let host = settled_mails_to(&mails, "test@example.com").await;
    assert!(host.iter().any(|m| m.contains(SECRET)), "{host:?}");
    let guest = settled_mails_to(&mails, "guest@test.com").await;
    assert!(!guest.is_empty());
    assert!(
        guest.iter().all(|m| !m.contains("HOST-ONLY-173")),
        "{guest:?}"
    );
    assert!(
        guest.iter().any(|m| m.contains("BEGIN:VCALENDAR")),
        "the invite is attached"
    );
    mails.lock().unwrap().clear();

    let response = app
        .oneshot(post_form(
            &format!("/dashboard/bookings/{id}/cancel"),
            &session,
            csrf,
            &format!("_csrf={csrf}"),
        ))
        .await
        .unwrap();
    assert!(response.status().is_redirection());
    let host = settled_mails_to(&mails, "test@example.com").await;
    assert!(host.iter().any(|m| m.contains(SECRET)), "{host:?}");
    let guest = settled_mails_to(&mails, "guest@test.com").await;
    assert!(!guest.is_empty());
    assert!(
        guest.iter().all(|m| !m.contains("HOST-ONLY-173")),
        "{guest:?}"
    );
}

/// The code that builds what guests, their calendars, SMS gateways and
/// webhooks receive never reads the column, so no later change to those
/// paths can start leaking it by reusing a query.
#[test]
fn guest_facing_code_never_reads_host_notes() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut files = vec![
        "src/email/guest.rs",
        "src/email/render.rs",
        "src/email/transport.rs",
        "src/web/calendar_push.rs",
        "src/web/writeback.rs",
        "src/web/meeting.rs",
        "src/web/public_pages.rs",
        "src/web/booking.rs",
        "src/web/branding.rs",
        "src/google_meet.rs",
        "src/resources.rs",
    ]
    .into_iter()
    .map(|f| root.join(f))
    .collect::<Vec<_>>();
    for dir in ["src/sms", "src/providers", "src/caldav", "src/ews"] {
        for entry in std::fs::read_dir(root.join(dir)).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().is_some_and(|e| e == "rs") {
                files.push(path);
            }
        }
    }
    for path in files {
        let src = std::fs::read_to_string(&path).unwrap();
        assert!(
            !src.contains("host_notes"),
            "{} reads the host notes",
            path.display()
        );
    }
    for template in [
        "confirmed.html",
        "book.html",
        "slots.html",
        "embed.html",
        "booking_cancel_form.html",
        "booking_cancelled_guest.html",
        "booking_reschedule_confirm.html",
        "booking_claim_form.html",
        "booking_claimed.html",
        "booking_approve_form.html",
        "booking_decline_form.html",
    ] {
        let src = std::fs::read_to_string(root.join("templates").join(template)).unwrap();
        assert!(
            !src.contains("host_notes"),
            "templates/{template} shows the host notes"
        );
    }
}

/// An unassigned round-robin booking (its member was deleted, or a legacy
/// row) is listed on the overview of the owner only, not of every member.
#[tokio::test]
async fn the_overview_leaves_an_unassigned_round_robin_booking_to_its_owner() {
    let (app, pool, session, _) = setup_test_app().await;
    let (peer, peer_session) =
        insert_test_user_with_session(&pool, "Peer", "peer@test.com", "peer").await;
    let owner = owner_id(&pool).await;
    let (_, team_et) = insert_team_with_et(
        &pool,
        "t173b",
        "rr",
        &[(&owner, "admin"), (&peer, "member")],
    )
    .await;
    let owner_account: String = sqlx::query_scalar("SELECT id FROM accounts WHERE user_id = ?")
        .bind(&owner)
        .fetch_one(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE event_types SET account_id = ? WHERE id = ?")
        .bind(&owner_account)
        .bind(&team_et)
        .execute(&pool)
        .await
        .unwrap();
    let id = insert_v1_booking(
        &pool,
        &team_et,
        "g@t.com",
        Utc::now() + Duration::hours(3),
        30,
        "confirmed",
        &[],
    )
    .await;
    let link = format!("/dashboard/bookings/{id}/notes?back=overview");

    let body = body_string(
        app.clone()
            .oneshot(get_authed("/dashboard", &peer_session))
            .await
            .unwrap(),
    )
    .await;
    assert!(!body.contains(&link), "not the peer's booking");
    let body = body_string(
        app.clone()
            .oneshot(get_authed("/dashboard", &session))
            .await
            .unwrap(),
    )
    .await;
    assert!(body.contains(&link), "still the owner's");

    // Collective, the same booking is every member's.
    sqlx::query("UPDATE event_types SET scheduling_mode = 'collective' WHERE id = ?")
        .bind(&team_et)
        .execute(&pool)
        .await
        .unwrap();
    let body = body_string(
        app.oneshot(get_authed("/dashboard", &peer_session))
            .await
            .unwrap(),
    )
    .await;
    assert!(
        body.contains(&link),
        "a collective booking is the peer's too"
    );
}

/// A user added as a collective member of the fixture's team event type,
/// which is made collective. Returns (team event type id, member id).
async fn collective_team_with_member(pool: &SqlitePool, email: &str) -> (String, String) {
    let (team_id, et_id) = seed_team_event_type(pool).await;
    sqlx::query("UPDATE event_types SET scheduling_mode = 'collective' WHERE id = ?")
        .bind(&et_id)
        .execute(pool)
        .await
        .unwrap();
    let member = insert_role_user(pool, email, "user").await;
    sqlx::query("INSERT INTO team_members (team_id, user_id, role, source) VALUES (?, ?, 'member', 'direct')")
        .bind(&team_id)
        .bind(&member)
        .execute(pool)
        .await
        .unwrap();
    (et_id, member)
}

/// The per-recipient lookup follows the access rule, not the address an
/// email goes to, and a failed lookup reads as "no notes".
#[tokio::test]
async fn host_notes_for_a_recipient_follow_the_access_rule() {
    let (_app, pool, _, _) = setup_test_app().await;
    let (et_id, member) = collective_team_with_member(&pool, "member@test.com").await;
    let id = insert_v1_booking(
        &pool,
        &et_id,
        "g@t.com",
        Utc::now() + Duration::days(2),
        30,
        "confirmed",
        &[],
    )
    .await;
    set_host_notes(&pool, &id, SECRET).await;
    let uid = format!("uid-{id}");

    assert_eq!(
        host_notes_for_reader(&pool, &uid, Some(&member))
            .await
            .as_deref(),
        Some(SECRET)
    );
    assert_eq!(
        host_notes_for_reader(&pool, &uid, Some(&owner_id(&pool).await))
            .await
            .as_deref(),
        Some(SECRET)
    );
    assert_eq!(
        host_notes_for_reader(&pool, &uid, None).await,
        None,
        "unknown recipient"
    );
    sqlx::query("DELETE FROM team_members WHERE user_id = ?")
        .bind(&member)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        host_notes_for_reader(&pool, &uid, Some(&member)).await,
        None,
        "left the team"
    );

    pool.close().await;
    assert_eq!(
        host_notes_for_reader(&pool, &uid, Some(&member)).await,
        None,
        "lookup failed"
    );
}

/// Approving a collective booking emails the stored organizer. Once that
/// member has left the team, the email still goes (the calendar names them)
/// but without the notes.
#[tokio::test]
async fn an_organizer_who_left_the_team_gets_no_host_notes() {
    let _env = crate::test_support::ENV_LOCK.lock().await;
    let (app, pool, session, _) = setup_test_app().await;
    let (port, mails) = spawn_capturing_mta().await;
    use_capturing_mta(&pool, port).await;
    let (et_id, organizer) = collective_team_with_member(&pool, "organizer@test.com").await;
    let mut ids = Vec::new();
    for hours in [50, 80] {
        let id = insert_v1_booking(
            &pool,
            &et_id,
            "guest@test.com",
            Utc::now() + Duration::hours(hours),
            30,
            "pending",
            &[
                ("organizer_user_id", Some(organizer.clone())),
                ("organizer_name", Some("Org".to_string())),
                ("organizer_email", Some("organizer@test.com".to_string())),
            ],
        )
        .await;
        set_host_notes(&pool, &id, SECRET).await;
        ids.push(id);
    }
    let approve = |id: String| {
        let app = app.clone();
        let session = session.clone();
        async move {
            let csrf = "csrf-173-org";
            let response = app
                .oneshot(post_form(
                    &format!("/dashboard/bookings/{id}/confirm"),
                    &session,
                    csrf,
                    &format!("_csrf={csrf}"),
                ))
                .await
                .unwrap();
            assert!(response.status().is_redirection());
        }
    };

    // Still a member: the notes are theirs to read.
    approve(ids[0].clone()).await;
    let mail = settled_mails_to(&mails, "organizer@test.com").await;
    assert!(mail.iter().any(|m| m.contains(SECRET)), "{mail:?}");
    mails.lock().unwrap().clear();

    sqlx::query("DELETE FROM team_members WHERE user_id = ?")
        .bind(&organizer)
        .execute(&pool)
        .await
        .unwrap();
    approve(ids[1].clone()).await;
    let mail = settled_mails_to(&mails, "organizer@test.com").await;
    assert!(!mail.is_empty(), "the organizer is still emailed");
    assert!(
        mail.iter().all(|m| !m.contains("HOST-ONLY-173")),
        "{mail:?}"
    );
}

/// The write checks access itself: a member who lost access after the page
/// was read writes nothing, even with the right version.
#[tokio::test]
async fn the_write_rechecks_access() {
    let (_app, pool, _, _) = setup_test_app().await;
    let (et_id, member) = collective_team_with_member(&pool, "late@test.com").await;
    let id = insert_v1_booking(
        &pool,
        &et_id,
        "g@t.com",
        Utc::now() + Duration::days(2),
        30,
        "confirmed",
        &[],
    )
    .await;
    assert!(
        write_host_notes(&pool, &id, Some("mine"), 0, &member, &member, None)
            .await
            .unwrap()
    );
    sqlx::query("DELETE FROM team_members WHERE user_id = ?")
        .bind(&member)
        .execute(&pool)
        .await
        .unwrap();
    assert!(
        !write_host_notes(&pool, &id, Some("late"), 1, &member, &member, None)
            .await
            .unwrap()
    );
    assert_eq!(stored_host_notes(&pool, &id).await.as_deref(), Some("mine"));
}

/// Past bookings stay reachable for post-call notes: the "Last 30 days"
/// section lists the meetings held over that time, and "Cancelled and
/// declined" the bookings that ended from 30 days ago on, for every host of
/// them, each with its title link.
#[tokio::test]
async fn recent_bookings_lead_to_their_notes() {
    let (app, pool, session, et_id) = setup_test_app().await;
    let past = insert_v1_booking(
        &pool,
        &et_id,
        "past@test.com",
        Utc::now() - Duration::days(2),
        30,
        "confirmed",
        &[],
    )
    .await;
    let cancelled = insert_v1_booking(
        &pool,
        &et_id,
        "gone@test.com",
        Utc::now() - Duration::days(5),
        30,
        "cancelled",
        &[],
    )
    .await;
    let old = insert_v1_booking(
        &pool,
        &et_id,
        "old@test.com",
        Utc::now() - Duration::days(40),
        30,
        "confirmed",
        &[],
    )
    .await;
    set_host_notes(&pool, &past, SECRET).await;

    let body = body_string(
        app.clone()
            .oneshot(get_authed("/dashboard/bookings", &session))
            .await
            .unwrap(),
    )
    .await;
    assert!(body.contains("Last 30 days"), "{body}");
    for id in [&past, &cancelled] {
        assert!(
            body.contains(&format!("/dashboard/bookings/{id}/notes?back=bookings")),
            "{id}"
        );
    }
    assert!(
        !body.contains(&format!("/dashboard/bookings/{old}/notes")),
        "older than 30 days"
    );
    assert!(body.contains(SECRET), "the past booking's notes show");
    assert!(body.contains("Cancelled and declined"));

    // An assigned member, whose bookings page lists none of the owner's
    // bookings, reaches their own past booking there.
    let (alice, alice_session) =
        insert_test_user_with_session(&pool, "Alice", "alice@test.com", "alice").await;
    let owner = owner_id(&pool).await;
    let (_, team_et) = insert_team_with_et(
        &pool,
        "t173r",
        "rr",
        &[(&owner, "admin"), (&alice, "member")],
    )
    .await;
    let hers = insert_v1_booking(
        &pool,
        &team_et,
        "g@t.com",
        Utc::now() - Duration::days(1),
        30,
        "confirmed",
        &[("assigned_user_id", Some(alice.clone()))],
    )
    .await;
    let body = body_string(
        app.clone()
            .oneshot(get_authed("/dashboard/bookings", &alice_session))
            .await
            .unwrap(),
    )
    .await;
    assert!(
        body.contains(&format!("/dashboard/bookings/{hers}/notes")),
        "{body}"
    );
    assert!(
        !body.contains(&format!("/dashboard/bookings/{past}/notes")),
        "not the owner's"
    );

    let (_, stranger) = insert_test_user_with_session(&pool, "Eve", "eve@test.com", "eve").await;
    let body = body_string(
        app.oneshot(get_authed("/dashboard/bookings", &stranger))
            .await
            .unwrap(),
    )
    .await;
    assert!(!body.contains("Last 30 days"), "nothing hosted, no section");
}

/// The part of the bookings page inside the section labelled `id`, empty
/// when the page has no such section.
fn bookings_section<'a>(body: &'a str, id: &str) -> &'a str {
    let marker = format!("aria-labelledby=\"{id}\"");
    match body.find(&marker) {
        Some(at) => {
            let rest = &body[at..];
            &rest[..rest.find("</section>").unwrap_or(rest.len())]
        }
        None => "",
    }
}

/// "Cancelled and declined" lists the bookings that ended, future ones
/// included and back to 30 days, each with its title link to the notes;
/// the notes of a meeting cancelled for a later date open from there.
/// Nothing still on, and nothing older, is in it, and the meetings held in
/// the last 30 days stay under "Last 30 days".
#[tokio::test]
async fn cancelled_bookings_lead_to_their_notes() {
    let (app, pool, session, et_id) = setup_test_app().await;
    let at = |days: i64| Utc::now() + Duration::days(days);
    let later = insert_v1_booking(&pool, &et_id, "later@t.com", at(5), 30, "cancelled", &[]).await;
    let declined = insert_v1_booking(&pool, &et_id, "no@t.com", at(9), 30, "declined", &[]).await;
    let gone = insert_v1_booking(&pool, &et_id, "gone@t.com", at(-3), 30, "cancelled", &[]).await;
    let old = insert_v1_booking(&pool, &et_id, "old@t.com", at(-40), 30, "cancelled", &[]).await;
    let held = insert_v1_booking(&pool, &et_id, "held@t.com", at(-2), 30, "confirmed", &[]).await;
    let on = insert_v1_booking(&pool, &et_id, "on@t.com", at(3), 30, "confirmed", &[]).await;
    set_host_notes(&pool, &later, SECRET).await;

    let body = body_string(
        app.clone()
            .oneshot(get_authed("/dashboard/bookings", &session))
            .await
            .unwrap(),
    )
    .await;
    let cancelled = bookings_section(&body, "bk-cancelled");
    assert!(cancelled.contains("Cancelled and declined"), "{body}");
    for id in [&later, &declined, &gone] {
        assert!(
            cancelled.contains(&format!(
                "class=\"tt-link\" href=\"/dashboard/bookings/{id}/notes?back=bookings\""
            )),
            "{id}: {cancelled}"
        );
    }
    for id in [&old, &held, &on] {
        assert!(!cancelled.contains(id.as_str()), "{id}");
    }
    assert!(cancelled.contains("Declined") && cancelled.contains(SECRET));
    let recent = bookings_section(&body, "bk-recent");
    assert!(recent.contains(held.as_str()), "{recent}");
    assert!(
        !recent.contains(gone.as_str()),
        "listed once, under Cancelled"
    );

    let notes = app
        .oneshot(get_authed(
            &format!("/dashboard/bookings/{later}/notes?back=bookings"),
            &session,
        ))
        .await
        .unwrap();
    assert_eq!(notes.status(), 200);
    assert!(body_string(notes).await.contains(SECRET));
}

/// The section follows the host-notes rule: the member a round-robin
/// booking is assigned to sees its cancellation, the team's other members
/// and a stranger do not, and someone with nothing cancelled gets no
/// section at all.
#[tokio::test]
async fn cancelled_bookings_are_listed_for_their_hosts_only() {
    let (app, pool, session, _) = setup_test_app().await;
    let owner = owner_id(&pool).await;
    let (alice, alice_session) =
        insert_test_user_with_session(&pool, "Alice", "alice@test.com", "alice").await;
    let (bob, bob_session) =
        insert_test_user_with_session(&pool, "Bob", "bob@test.com", "bob").await;
    let (_, eve_session) = insert_test_user_with_session(&pool, "Eve", "eve@test.com", "eve").await;
    let (_, team_et) = insert_team_with_et(
        &pool,
        "t-cancel",
        "rr",
        &[(&owner, "admin"), (&alice, "member"), (&bob, "member")],
    )
    .await;
    let hers = insert_v1_booking(
        &pool,
        &team_et,
        "g@t.com",
        Utc::now() + Duration::days(4),
        30,
        "cancelled",
        &[("assigned_user_id", Some(alice.clone()))],
    )
    .await;

    let page = |s: String| {
        let app = app.clone();
        async move {
            body_string(
                app.oneshot(get_authed("/dashboard/bookings", &s))
                    .await
                    .unwrap(),
            )
            .await
        }
    };
    let body = page(alice_session).await;
    assert!(
        bookings_section(&body, "bk-cancelled")
            .contains(&format!("/dashboard/bookings/{hers}/notes?back=bookings")),
        "{body}"
    );
    for (who, s) in [
        ("bob", bob_session),
        ("eve", eve_session),
        ("owner", session),
    ] {
        let body = page(s).await;
        assert!(!body.contains(hers.as_str()), "{who} sees it");
        assert!(!body.contains("bk-cancelled"), "{who} gets a section");
    }
}

/// The section opens on the cancellations closest to today: a pile of far
/// future ones does not push out the meeting cancelled last week, and the
/// farthest are on the next page.
#[tokio::test]
async fn far_future_cancellations_do_not_hide_recent_ones() {
    let (app, pool, session, et_id) = setup_test_app().await;
    let at = |days: i64| Utc::now() + Duration::days(days);
    let mut far = Vec::new();
    for d in 0..55 {
        let email = format!("far{d}@t.com");
        far.push(insert_v1_booking(&pool, &et_id, &email, at(100 + d), 30, "cancelled", &[]).await);
    }
    let recent = insert_v1_booking(&pool, &et_id, "week@t.com", at(-7), 30, "cancelled", &[]).await;
    let body = body_string(
        app.clone()
            .oneshot(get_authed("/dashboard/bookings", &session))
            .await
            .unwrap(),
    )
    .await;
    let cancelled = bookings_section(&body, "bk-cancelled");
    assert!(cancelled.contains(recent.as_str()), "{cancelled}");
    assert!(cancelled.contains("far0@t.com"), "the nearest future one");
    assert!(
        !cancelled.contains("far54@t.com"),
        "the farthest is on the next page"
    );
    let next = body_string(
        app.oneshot(get_authed("/dashboard/bookings?xp=2", &session))
            .await
            .unwrap(),
    )
    .await;
    assert!(bookings_section(&next, "bk-cancelled").contains("far54@t.com"));
}
