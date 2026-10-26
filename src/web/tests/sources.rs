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
/// the update handler's decrypt-on-keep-existing path works. Uses
/// `example.com` which has public DNS, so `validate_caldav_url` (which does
/// DNS resolution to check for SSRF) doesn't fail in the test sandbox.
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
        "_csrf={}&name=Renamed&url=https%3A%2F%2Fexample.com%2Fdav%2Fnew%2F&username=newuser&password=&no_test=on",
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
    assert_eq!(row.1, "https://example.com/dav/new/");
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
        "_csrf={}&name=My+Source&url=https%3A%2F%2Fexample.com%2Fdav%2Fold%2F&username=olduser&password=new-pw-rotated&no_test=on",
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

/// A validation error re-renders the form with the connect cards as they
/// are, not as "not configured".
#[tokio::test]
async fn source_form_error_keeps_the_microsoft_card_configured() {
    let (app, pool, session, _) = setup_test_app().await;
    crate::microsoft_oauth::save_config(
        &pool,
        &[0u8; 32],
        "00000000-0000-0000-0000-000000000001",
        "secret",
        "common",
    )
    .await
    .unwrap();
    let csrf = "test-csrf-source-error";
    let response = app
        .oneshot(post_form(
            "/dashboard/sources/new",
            &session,
            csrf,
            &format!("_csrf={csrf}&name=&url=&username=&password="),
        ))
        .await
        .unwrap();
    let body = body_string(response).await;
    assert!(body.contains("action=\"/dashboard/sources/microsoft/connect\""));
}

async fn seed_calendar(pool: &SqlitePool, source: &str, href: &str, read_only: bool, events: u32) {
    let id = uuid::Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO calendars (id, source_id, href, display_name, read_only) VALUES (?, ?, ?, ?, ?)")
        .bind(&id)
        .bind(source)
        .bind(href)
        .bind(format!("Calendar {href}"))
        .bind(read_only)
        .execute(pool)
        .await
        .unwrap();
    for n in 0..events {
        sqlx::query("INSERT INTO events (id, calendar_id, uid, start_at, end_at) VALUES (?, ?, ?, '20260101T090000Z', '20260101T100000Z')")
            .bind(uuid::Uuid::new_v4().to_string())
            .bind(&id)
            .bind(format!("{href}-{n}"))
            .execute(pool)
            .await
            .unwrap();
    }
}

/// A colleague's calendar shared read-only is not offered for write-back,
/// even though it holds the most events (the setup page pre-selects the
/// busiest calendar), and a crafted form cannot select it.
#[tokio::test]
async fn read_only_calendars_are_not_write_back_choices() {
    let (app, pool, session, _) = setup_test_app().await;
    let sid = seed_source(&pool, "Microsoft Calendar", "pw").await;
    seed_calendar(&pool, &sid, "own", false, 1).await;
    seed_calendar(&pool, &sid, "colleague", true, 5).await;

    let body = body_string(
        app.clone()
            .oneshot(get_authed(
                &format!("/dashboard/sources/{sid}/setup-write"),
                &session,
            ))
            .await
            .unwrap(),
    )
    .await;
    assert!(body.contains("value=\"own\" checked"), "{body}");
    assert!(!body.contains("Calendar colleague"));

    let body = body_string(
        app.clone()
            .oneshot(get_authed("/dashboard/sources", &session))
            .await
            .unwrap(),
    )
    .await;
    assert!(body.contains("Calendar own"));
    assert!(!body.contains("Calendar colleague"));

    let csrf = "csrf-read-only";
    let write_target = || async {
        sqlx::query_scalar::<_, Option<String>>(
            "SELECT write_calendar_href FROM caldav_sources WHERE id = ?",
        )
        .bind(&sid)
        .fetch_one(&pool)
        .await
        .unwrap()
    };
    for href in ["colleague", "own"] {
        app.clone()
            .oneshot(post_form(
                &format!("/dashboard/sources/{sid}/write-calendar"),
                &session,
                csrf,
                &format!("_csrf={csrf}&calendar_href={href}"),
            ))
            .await
            .unwrap();
    }
    assert_eq!(write_target().await.as_deref(), Some("own"));
    app.clone()
        .oneshot(post_form(
            &format!("/dashboard/sources/{sid}/write-calendar"),
            &session,
            csrf,
            &format!("_csrf={csrf}&calendar_href=colleague"),
        ))
        .await
        .unwrap();
    assert_eq!(write_target().await.as_deref(), Some("own"));

    // Already configured before it was known to be read-only: still shown,
    // so the form does not silently read as "none".
    sqlx::query("UPDATE caldav_sources SET write_calendar_href = 'colleague' WHERE id = ?")
        .bind(&sid)
        .execute(&pool)
        .await
        .unwrap();
    let body = body_string(
        app.oneshot(get_authed("/dashboard/sources", &session))
            .await
            .unwrap(),
    )
    .await;
    assert!(body.contains("value=\"colleague\" selected"), "{body}");
}
