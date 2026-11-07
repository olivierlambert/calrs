//! Calendar source forms.

use super::*;

#[tokio::test]
async fn sources_page_returns_200() {
    let (app, _, session, _) = setup_test_app().await;
    let response = app
        .oneshot(get_authed("/dashboard/sources", &session))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
}

#[tokio::test]
async fn new_source_form_returns_200() {
    let (app, _, session, _) = setup_test_app().await;
    let response = app
        .oneshot(get_authed("/dashboard/sources/new", &session))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
}

/// Helper for the source-edit tests: insert a caldav source for the test user.
/// Returns the source id. Encrypts with the all-zeroes test secret_key so
/// the update handler's decrypt-on-keep-existing path works. The edit
/// requests post an IP literal from TEST-NET-1 (192.0.2.0/24), which
/// `validate_caldav_url` checks without a DNS lookup, so these tests also
/// pass on a builder with no network (discussion #274).
async fn seed_source(pool: &SqlitePool, name: &str, password: &str) -> String {
    let account_id: (String,) = sqlx::query_as(
        "SELECT id FROM accounts WHERE user_id = (SELECT id FROM users WHERE username = 'testuser') LIMIT 1",
    )
    .fetch_one(pool)
    .await
    .unwrap();
    let id = uuid::Uuid::new_v4().to_string();
    let enc = crate::crypto::encrypt_password(&[0u8; 32], password).unwrap();
    sqlx::query(
        "INSERT INTO caldav_sources (id, account_id, name, url, username, password_enc) VALUES (?, ?, ?, 'https://example.com/dav/old/', 'olduser', ?)",
    )
    .bind(&id)
    .bind(&account_id.0)
    .bind(name)
    .bind(&enc)
    .execute(pool)
    .await
    .unwrap();
    id
}

#[tokio::test]
async fn edit_source_form_renders_prefilled() {
    let (app, pool, session, _) = setup_test_app().await;
    let sid = seed_source(&pool, "My Source", "old-pw").await;

    let response = app
        .oneshot(get_authed(
            &format!("/dashboard/sources/{}/edit", sid),
            &session,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = body_string(response).await;
    assert!(
        body.contains("Edit calendar source"),
        "should render in editing mode"
    );
    assert!(body.contains("My Source"), "name pre-filled");
    // minijinja escapes '/' as '&#x2f;' in attribute values; check both the
    // host and a path segment (which won't include slashes).
    assert!(body.contains("example.com"), "url host pre-filled");
    assert!(
        body.contains("dav") && body.contains("old"),
        "url path pre-filled"
    );
    assert!(body.contains("olduser"), "username pre-filled");
    assert!(
        body.contains("Leave blank to keep existing"),
        "password placeholder"
    );
}

#[tokio::test]
async fn update_source_changes_fields_and_keeps_password_when_blank() {
    let (app, pool, session, _) = setup_test_app().await;
    let sid = seed_source(&pool, "My Source", "old-pw").await;

    let csrf = "test-csrf-update-source";
    let body = format!(
        "_csrf={}&name=Renamed&url=https%3A%2F%2F192.0.2.10%2Fdav%2Fnew%2F&username=newuser&password=&no_test=on",
        csrf
    );
    let response = app
        .oneshot(post_form(
            &format!("/dashboard/sources/{}/edit", sid),
            &session,
            csrf,
            &body,
        ))
        .await
        .unwrap();
    assert!(response.status().is_redirection() || response.status() == 200);

    let row: (String, String, String, String) =
        sqlx::query_as("SELECT name, url, username, password_enc FROM caldav_sources WHERE id = ?")
            .bind(&sid)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(row.0, "Renamed");
    assert_eq!(row.1, "https://192.0.2.10/dav/new/");
    assert_eq!(row.2, "newuser");
    // Decrypt the stored blob and confirm the password is unchanged.
    let pw = crate::crypto::decrypt_password(&[0u8; 32], &row.3).unwrap();
    assert_eq!(pw, "old-pw", "blank password must keep the existing one");
}

#[tokio::test]
async fn update_source_rotates_password() {
    let (app, pool, session, _) = setup_test_app().await;
    let sid = seed_source(&pool, "My Source", "old-pw").await;

    let csrf = "test-csrf-rotate";
    let body = format!(
        "_csrf={}&name=My+Source&url=https%3A%2F%2F192.0.2.10%2Fdav%2Fold%2F&username=olduser&password=new-pw-rotated&no_test=on",
        csrf
    );
    let response = app
        .oneshot(post_form(
            &format!("/dashboard/sources/{}/edit", sid),
            &session,
            csrf,
            &body,
        ))
        .await
        .unwrap();
    assert!(response.status().is_redirection() || response.status() == 200);

    let enc: (String,) = sqlx::query_as("SELECT password_enc FROM caldav_sources WHERE id = ?")
        .bind(&sid)
        .fetch_one(&pool)
        .await
        .unwrap();
    let pw = crate::crypto::decrypt_password(&[0u8; 32], &enc.0).unwrap();
    assert_eq!(
        pw, "new-pw-rotated",
        "non-blank password must replace the existing one"
    );
}

#[tokio::test]
async fn update_source_404_for_other_user_source() {
    // The edit handler must scope by user_id; trying to edit a source the
    // current user doesn't own should redirect away (not leak/edit).
    let (app, pool, session, _) = setup_test_app().await;

    // Insert an account belonging to a *different* user, and a source under it.
    let other_user_id = uuid::Uuid::new_v4().to_string();
    let other_account_id = uuid::Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO users (id, email, name, role, auth_provider, username, enabled) VALUES (?, 'other@example.com', 'Other', 'user', 'local', 'otheruser', 1)")
        .bind(&other_user_id)
        .execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO accounts (id, name, email, timezone, user_id) VALUES (?, 'Other', 'other@example.com', 'UTC', ?)")
        .bind(&other_account_id)
        .bind(&other_user_id)
        .execute(&pool).await.unwrap();
    let other_sid = uuid::Uuid::new_v4().to_string();
    let enc = crate::crypto::encrypt_password(&[0u8; 32], "secret").unwrap();
    sqlx::query("INSERT INTO caldav_sources (id, account_id, name, url, username, password_enc) VALUES (?, ?, 'Other source', 'https://other.example.com/', 'someone', ?)")
        .bind(&other_sid)
        .bind(&other_account_id)
        .bind(&enc)
        .execute(&pool).await.unwrap();

    let response = app
        .oneshot(get_authed(
            &format!("/dashboard/sources/{}/edit", other_sid),
            &session,
        ))
        .await
        .unwrap();
    // Handler redirects to /dashboard/sources for unauthorised access.
    assert!(response.status().is_redirection());
}

// --- Calendars synced by a source (#324) ---

/// Two calendars under `source_id`, each with one cached busy event on
/// 2026-06-18 10:00-11:30 Paris. Returns (mine, colleague) calendar ids.
async fn seed_two_calendars(pool: &SqlitePool, source_id: &str) -> (String, String) {
    let mut ids = Vec::new();
    for (href, name, color) in [
        ("/cals/mine/", "Mine", "#3b82f6"),
        ("/cals/colleague/", "Colleague", "red;background:url(x)"),
    ] {
        let cal_id = uuid::Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO calendars (id, source_id, href, display_name, color, is_busy, ctag, sync_token) VALUES (?, ?, ?, ?, ?, 1, 'ctag-1', 'tok-1')")
            .bind(&cal_id).bind(source_id).bind(href).bind(name).bind(color)
            .execute(pool).await.unwrap();
        sqlx::query("INSERT INTO events (id, calendar_id, uid, summary, start_at, end_at, all_day, timezone, status, transp) VALUES (?, ?, ?, 'Busy', '20260618T100000', '20260618T113000', 0, 'Europe/Paris', 'CONFIRMED', 'OPAQUE')")
            .bind(uuid::Uuid::new_v4().to_string()).bind(&cal_id).bind(format!("ev-{name}"))
            .execute(pool).await.unwrap();
        ids.push(cal_id);
    }
    (ids.remove(0), ids.remove(0))
}

async fn busy_on_june_18(pool: &SqlitePool, user_id: &str) -> usize {
    let paris: Tz = "Europe/Paris".parse().unwrap();
    fetch_busy_times_for_user(
        pool,
        user_id,
        dt(2026, 6, 18, 9, 0),
        dt(2026, 6, 18, 12, 0),
        paris,
        None,
    )
    .await
    .len()
}

#[tokio::test]
async fn source_calendars_page_lists_calendars_with_their_state() {
    let (app, pool, session, _) = setup_test_app().await;
    let sid = seed_source(&pool, "Kerio", "pw").await;
    let (mine, colleague) = seed_two_calendars(&pool, &sid).await;
    sqlx::query("UPDATE calendars SET sync_enabled = 0 WHERE id = ?")
        .bind(&colleague)
        .execute(&pool)
        .await
        .unwrap();

    let response = app
        .oneshot(get_authed(
            &format!("/dashboard/sources/{sid}/calendars"),
            &session,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = body_string(response).await;
    assert!(body.contains("Calendars to sync"));
    assert!(body.contains("Select all") && body.contains("Select none"));
    assert!(body.contains(&format!("value=\"{mine}\" checked")));
    assert!(body.contains(&format!("value=\"{colleague}\" >")));
    // A hex color is drawn, anything else never reaches the style attribute.
    assert!(body.contains("background: #3b82f6"));
    assert!(!body.contains("url(x)"));
}

#[tokio::test]
async fn saving_the_calendar_selection_stops_syncing_the_unchecked_ones() {
    let (app, pool, session, _) = setup_test_app().await;
    let user_id: String = sqlx::query_scalar("SELECT id FROM users WHERE username = 'testuser'")
        .fetch_one(&pool)
        .await
        .unwrap();
    let sid = seed_source(&pool, "Kerio", "pw").await;
    let (mine, colleague) = seed_two_calendars(&pool, &sid).await;
    assert_eq!(busy_on_june_18(&pool, &user_id).await, 2);

    let csrf = "test-csrf-calendars";
    let response = app
        .oneshot(post_form(
            &format!("/dashboard/sources/{sid}/calendars"),
            &session,
            csrf,
            &format!("_csrf={csrf}&sync={mine}"),
        ))
        .await
        .unwrap();
    assert!(response.status().is_redirection());

    let state: Vec<(String, bool, Option<String>, Option<String>, i64)> = sqlx::query_as(
        "SELECT c.id, c.sync_enabled, c.ctag, c.sync_token,
                (SELECT COUNT(*) FROM events e WHERE e.calendar_id = c.id)
         FROM calendars c WHERE c.source_id = ?",
    )
    .bind(&sid)
    .fetch_all(&pool)
    .await
    .unwrap();
    for (id, enabled, ctag, token, events) in &state {
        if id == &mine {
            // Unchanged: still synced, sync state and events kept.
            assert!(*enabled);
            assert_eq!(ctag.as_deref(), Some("ctag-1"));
            assert_eq!(token.as_deref(), Some("tok-1"));
            assert_eq!(*events, 1);
        } else {
            assert_eq!(id, &colleague);
            assert!(!*enabled);
            assert!(ctag.is_none() && token.is_none());
            assert_eq!(*events, 0, "the unsynced calendar's events are dropped");
        }
    }
    assert_eq!(
        busy_on_june_18(&pool, &user_id).await,
        1,
        "only the synced calendar blocks"
    );

    // Even an event a sync in flight wrote back never blocks.
    sqlx::query("INSERT INTO events (id, calendar_id, uid, summary, start_at, end_at, all_day, timezone, status, transp) VALUES ('late', ?, 'late', 'Busy', '20260618T100000', '20260618T113000', 0, 'Europe/Paris', 'CONFIRMED', 'OPAQUE')")
        .bind(&colleague)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(busy_on_june_18(&pool, &user_id).await, 1);
}

/// An event type whose conflict calendars are all turned off must not end
/// up with no busy time at all: it falls back to every synced calendar,
/// as an empty selection does, so the host cannot be double-booked.
#[tokio::test]
async fn conflict_selection_of_only_unsynced_calendars_falls_back_to_all() {
    let (app, pool, session, _) = setup_test_app().await;
    let user_id: String = sqlx::query_scalar("SELECT id FROM users WHERE username = 'testuser'")
        .fetch_one(&pool)
        .await
        .unwrap();
    let et_id: String =
        sqlx::query_scalar("SELECT id FROM event_types WHERE slug = 'test-meeting'")
            .fetch_one(&pool)
            .await
            .unwrap();
    let sid = seed_source(&pool, "Kerio", "pw").await;
    let (mine, colleague) = seed_two_calendars(&pool, &sid).await;
    sqlx::query("INSERT INTO event_type_calendars (event_type_id, calendar_id) VALUES (?, ?)")
        .bind(&et_id)
        .bind(&colleague)
        .execute(&pool)
        .await
        .unwrap();

    let csrf = "test-csrf-fallback";
    let response = app
        .oneshot(post_form(
            &format!("/dashboard/sources/{sid}/calendars"),
            &session,
            csrf,
            &format!("_csrf={csrf}&sync={mine}"),
        ))
        .await
        .unwrap();
    assert!(response.status().is_redirection());

    let paris: Tz = "Europe/Paris".parse().unwrap();
    let busy = fetch_busy_times_for_user(
        &pool,
        &user_id,
        dt(2026, 6, 18, 9, 0),
        dt(2026, 6, 18, 12, 0),
        paris,
        Some(&et_id),
    )
    .await;
    assert_eq!(busy.len(), 1, "the synced calendar still blocks");
}

#[tokio::test]
async fn saving_with_nothing_checked_turns_every_calendar_off() {
    let (app, pool, session, _) = setup_test_app().await;
    let sid = seed_source(&pool, "Kerio", "pw").await;
    seed_two_calendars(&pool, &sid).await;

    let csrf = "test-csrf-none";
    let response = app
        .oneshot(post_form(
            &format!("/dashboard/sources/{sid}/calendars"),
            &session,
            csrf,
            &format!("_csrf={csrf}"),
        ))
        .await
        .unwrap();
    assert!(response.status().is_redirection());
    let synced: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM calendars WHERE source_id = ? AND sync_enabled = 1",
    )
    .bind(&sid)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(synced, 0);
}

#[tokio::test]
async fn calendar_selection_requires_csrf_and_ownership() {
    let (app, pool, session, _) = setup_test_app().await;

    // A source owned by another user.
    let other_user_id = uuid::Uuid::new_v4().to_string();
    let other_account_id = uuid::Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO users (id, email, name, role, auth_provider, username, enabled) VALUES (?, 'other@example.com', 'Other', 'user', 'local', 'otheruser', 1)")
        .bind(&other_user_id)
        .execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO accounts (id, name, email, timezone, user_id) VALUES (?, 'Other', 'other@example.com', 'UTC', ?)")
        .bind(&other_account_id)
        .bind(&other_user_id)
        .execute(&pool).await.unwrap();
    let other_sid = uuid::Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO caldav_sources (id, account_id, name, url, username) VALUES (?, ?, 'Other source', 'https://other.example.com/', 'someone')")
        .bind(&other_sid)
        .bind(&other_account_id)
        .execute(&pool).await.unwrap();
    seed_two_calendars(&pool, &other_sid).await;

    let csrf = "test-csrf-foreign";
    let response = app
        .clone()
        .oneshot(post_form(
            &format!("/dashboard/sources/{other_sid}/calendars"),
            &session,
            csrf,
            &format!("_csrf={csrf}"),
        ))
        .await
        .unwrap();
    assert!(response.status().is_redirection());
    let response = app
        .clone()
        .oneshot(get_authed(
            &format!("/dashboard/sources/{other_sid}/calendars"),
            &session,
        ))
        .await
        .unwrap();
    assert!(response.status().is_redirection());

    // Own source, but the CSRF token does not match the cookie.
    let sid = seed_source(&pool, "Mine", "pw").await;
    seed_two_calendars(&pool, &sid).await;
    let response = app
        .oneshot(post_form(
            &format!("/dashboard/sources/{sid}/calendars"),
            &session,
            csrf,
            "_csrf=wrong",
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), 403);

    let disabled: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM calendars WHERE sync_enabled = 0")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(disabled, 0, "nothing was changed");
}

#[tokio::test]
async fn sources_dashboard_links_the_calendar_selection() {
    let (app, pool, session, _) = setup_test_app().await;
    let sid = seed_source(&pool, "Kerio", "pw").await;
    let (_, colleague) = seed_two_calendars(&pool, &sid).await;
    sqlx::query("UPDATE calendars SET sync_enabled = 0 WHERE id = ?")
        .bind(&colleague)
        .execute(&pool)
        .await
        .unwrap();
    let response = app
        .oneshot(get_authed("/dashboard/sources", &session))
        .await
        .unwrap();
    let body = body_string(response).await;
    assert!(body.contains(&format!("/dashboard/sources/{sid}/calendars")));
    assert!(body.contains("Calendars synced: 1 of 2"));
}
