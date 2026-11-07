// Write destination per event type (#47): confirmed bookings go to the
// calendar the host chose for the event type, else to the write calendar of
// each of their sources. A child of `web::tests`.

use super::writeback_failures::{push_fresh, spawn_mock_dav, writeback_state, DavMode, MockDav};
use super::*;

/// A calendar row under `source_id` with `href`. Returns its id.
async fn insert_calendar(pool: &SqlitePool, source_id: &str, href: &str, name: &str) -> String {
    let id = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO calendars (id, source_id, href, display_name, is_busy) VALUES (?, ?, ?, ?, 1)",
    )
    .bind(&id)
    .bind(source_id)
    .bind(href)
    .bind(name)
    .execute(pool)
    .await
    .unwrap();
    id
}

/// An account of its own for `user_id`, before `insert_personal_et`
/// creates one with the shared placeholder address.
async fn insert_account(pool: &SqlitePool, user_id: &str) {
    sqlx::query(
        "INSERT INTO accounts (id, name, email, timezone, user_id) VALUES (?, 'A', ?, 'UTC', ?)",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(format!("{user_id}@accounts.test"))
    .bind(user_id)
    .execute(pool)
    .await
    .unwrap();
}

/// Store a destination directly, bypassing the checks of `set_destination`.
async fn choose(pool: &SqlitePool, et_id: &str, user_id: &str, calendar_id: &str) {
    sqlx::query(
        "INSERT OR REPLACE INTO event_type_write_targets (event_type_id, user_id, calendar_id) \
         VALUES (?, ?, ?)",
    )
    .bind(et_id)
    .bind(user_id)
    .bind(calendar_id)
    .execute(pool)
    .await
    .unwrap();
}

async fn copies(pool: &SqlitePool, uid: &str) -> Vec<(String, String)> {
    sqlx::query_as(
        "SELECT user_id, calendar_href FROM booking_write_copies WHERE booking_uid = ? \
         ORDER BY user_id, calendar_href",
    )
    .bind(uid)
    .fetch_all(pool)
    .await
    .unwrap()
}

/// What [`personal_fixture`] returns.
struct Personal {
    pool: SqlitePool,
    dav: Arc<MockDav>,
    host: String,
    et_id: String,
    source_id: String,
    /// The source's write calendar, `/dav/cal/`.
    work_cal: String,
    /// Another calendar of the same source, `/dav/coaching/`.
    coaching_cal: String,
    id: String,
    uid: String,
}

/// A host whose source writes to `/dav/cal/` and also has a `/dav/coaching/`
/// calendar, and a confirmed booking ten days out.
async fn personal_fixture() -> Personal {
    let pool = setup_test_db().await;
    let host = insert_role_user(&pool, "host@dest.test", "user").await;
    let et_id = insert_personal_et(&pool, &host, "coaching").await;
    let (base, dav) = spawn_mock_dav(DavMode::Ok).await;
    let source_id = seed_write_source(&pool, &host, &base).await;
    let work_cal = insert_calendar(&pool, &source_id, "/dav/cal/", "Work").await;
    let coaching_cal = insert_calendar(&pool, &source_id, "/dav/coaching/", "Coaching").await;
    let id = insert_v1_booking(
        &pool,
        &et_id,
        "guest@dest.test",
        Utc::now() + Duration::days(10),
        30,
        "confirmed",
        &[],
    )
    .await;
    let uid = format!("uid-{id}");
    Personal {
        pool,
        dav,
        host,
        et_id,
        source_id,
        work_cal,
        coaching_cal,
        id,
        uid,
    }
}

/// Without a choice nothing changes: the source write calendar, and no
/// copy recorded.
#[tokio::test]
async fn without_a_destination_bookings_go_to_the_source_write_calendar() {
    let p = personal_fixture().await;
    push_fresh(&p.pool, &p.host, &p.uid).await;
    assert_eq!(p.dav.paths(), vec![format!("/dav/cal/{}.ics", p.uid)]);
    let (_, _, _, href, _) = writeback_state(&p.pool, &p.id).await;
    assert_eq!(href.as_deref(), Some("/dav/cal/"));
    assert!(copies(&p.pool, &p.uid).await.is_empty());
}

/// With a choice the booking goes to that calendar only, and the copy is
/// recorded where it was written.
#[tokio::test]
async fn a_destination_receives_the_booking_instead_of_the_write_calendar() {
    let p = personal_fixture().await;
    choose(&p.pool, &p.et_id, &p.host, &p.coaching_cal).await;
    push_fresh(&p.pool, &p.host, &p.uid).await;
    assert_eq!(p.dav.paths(), vec![format!("/dav/coaching/{}.ics", p.uid)]);
    let (error, _, _, href, _) = writeback_state(&p.pool, &p.id).await;
    assert_eq!(error, None);
    assert_eq!(href.as_deref(), Some("/dav/coaching/"));
    assert_eq!(
        copies(&p.pool, &p.uid).await,
        vec![(p.host.clone(), "/dav/coaching/".to_string())]
    );

    caldav_delete_booking(&p.pool, &[0u8; 32], &p.host, &p.uid).await;
    assert_eq!(p.dav.stored(), 0);
    assert!(copies(&p.pool, &p.uid).await.is_empty());
}

/// Another event type of the same host keeps the source setting.
#[tokio::test]
async fn a_destination_only_applies_to_its_event_type() {
    let p = personal_fixture().await;
    let other = insert_personal_et(&p.pool, &p.host, "team-call").await;
    choose(&p.pool, &other, &p.host, &p.coaching_cal).await;
    push_fresh(&p.pool, &p.host, &p.uid).await;
    assert_eq!(p.dav.paths(), vec![format!("/dav/cal/{}.ics", p.uid)]);
}

/// A destination whose source is disabled no longer resolves: the booking
/// falls back to the source setting of the sources still enabled, and is
/// never refused for it.
#[tokio::test]
async fn a_destination_on_a_disabled_source_falls_back() {
    let p = personal_fixture().await;
    let (base, other_dav) = spawn_mock_dav(DavMode::Ok).await;
    let other_source = seed_write_source(&p.pool, &p.host, &base).await;
    let other_cal = insert_calendar(&p.pool, &other_source, "/dav/other/", "Other").await;
    choose(&p.pool, &p.et_id, &p.host, &other_cal).await;
    sqlx::query("UPDATE caldav_sources SET enabled = 0 WHERE id = ?")
        .bind(&other_source)
        .execute(&p.pool)
        .await
        .unwrap();
    push_fresh(&p.pool, &p.host, &p.uid).await;
    assert_eq!(other_dav.stored(), 0);
    assert_eq!(p.dav.paths(), vec![format!("/dav/cal/{}.ics", p.uid)]);
    assert_eq!(booking_status(&p.pool, &p.id).await, "confirmed");
}

/// Removing the destination's source removes the choice with it.
#[tokio::test]
async fn removing_the_source_removes_the_destination() {
    let p = personal_fixture().await;
    choose(&p.pool, &p.et_id, &p.host, &p.coaching_cal).await;
    sqlx::query("DELETE FROM caldav_sources WHERE id = ?")
        .bind(&p.source_id)
        .execute(&p.pool)
        .await
        .unwrap();
    assert_eq!(stored_destination(&p.pool, &p.et_id, &p.host).await, None);
}

/// Someone else's calendar is refused, whoever asks; so is an event type the
/// user does not host.
#[tokio::test]
async fn a_destination_must_be_the_users_own_calendar_for_an_event_type_they_host() {
    let p = personal_fixture().await;
    let mallory = insert_role_user(&p.pool, "mallory@dest.test", "user").await;
    insert_account(&p.pool, &mallory).await;
    let mallory_et = insert_personal_et(&p.pool, &mallory, "m").await;
    assert!(
        set_destination(&p.pool, &mallory, &mallory_et, &p.coaching_cal)
            .await
            .is_err()
    );
    assert!(set_destination(&p.pool, &mallory, &p.et_id, "")
        .await
        .is_err());
    assert_eq!(
        stored_destination(&p.pool, &mallory_et, &mallory).await,
        None
    );

    set_destination(&p.pool, &p.host, &p.et_id, &p.coaching_cal)
        .await
        .unwrap();
    assert_eq!(
        stored_destination(&p.pool, &p.et_id, &p.host).await,
        Some(p.coaching_cal.clone())
    );
    set_destination(&p.pool, &p.host, &p.et_id, "")
        .await
        .unwrap();
    assert_eq!(stored_destination(&p.pool, &p.et_id, &p.host).await, None);

    // Read time checks ownership again: a row pointing at another user's
    // calendar is ignored.
    choose(&p.pool, &mallory_et, &mallory, &p.coaching_cal).await;
    assert!(destination_source(&p.pool, &mallory_et, &mallory)
        .await
        .is_none());
}

/// A booking written before the destination was chosen moves there at its
/// next push (a reschedule, a retry), and back when the choice is cleared.
#[tokio::test]
async fn changing_the_destination_moves_the_booking_at_its_next_push() {
    let p = personal_fixture().await;
    push_fresh(&p.pool, &p.host, &p.uid).await;
    assert_eq!(p.dav.paths(), vec![format!("/dav/cal/{}.ics", p.uid)]);

    choose(&p.pool, &p.et_id, &p.host, &p.coaching_cal).await;
    push_fresh(&p.pool, &p.host, &p.uid).await;
    assert_eq!(p.dav.paths(), vec![format!("/dav/coaching/{}.ics", p.uid)]);

    sqlx::query("DELETE FROM event_type_write_targets")
        .execute(&p.pool)
        .await
        .unwrap();
    push_fresh(&p.pool, &p.host, &p.uid).await;
    assert_eq!(p.dav.paths(), vec![format!("/dav/cal/{}.ics", p.uid)]);
    assert!(copies(&p.pool, &p.uid).await.is_empty());
    let (_, _, _, href, _) = writeback_state(&p.pool, &p.id).await;
    assert_eq!(href.as_deref(), Some("/dav/cal/"));
}

/// A cancellation finds the copy where it was written, even when the
/// destination changed since.
#[tokio::test]
async fn cancelling_after_the_destination_changed_deletes_the_written_copy() {
    let p = personal_fixture().await;
    choose(&p.pool, &p.et_id, &p.host, &p.coaching_cal).await;
    push_fresh(&p.pool, &p.host, &p.uid).await;
    choose(&p.pool, &p.et_id, &p.host, &p.work_cal).await;
    caldav_delete_booking(&p.pool, &[0u8; 32], &p.host, &p.uid).await;
    assert_eq!(p.dav.stored(), 0);
}

/// A failed write to the destination is recorded and retried there, like
/// any write-back failure.
#[tokio::test]
async fn a_failed_destination_write_is_recorded_and_retried_there() {
    let p = personal_fixture().await;
    choose(&p.pool, &p.et_id, &p.host, &p.coaching_cal).await;
    p.dav.set_mode(DavMode::Reject);
    push_fresh(&p.pool, &p.host, &p.uid).await;
    let (error, attempts, _, href, status) = writeback_state(&p.pool, &p.id).await;
    assert!(error.is_some());
    assert_eq!(attempts, 1);
    assert_eq!(href, None);
    assert_eq!(status, "confirmed");

    p.dav.set_mode(DavMode::Ok);
    sqlx::query("UPDATE bookings SET writeback_next_attempt_at = '2000-01-01 00:00:00'")
        .execute(&p.pool)
        .await
        .unwrap();
    writeback::retry_failed_writebacks(&p.pool, &[0u8; 32], Utc::now()).await;
    assert_eq!(p.dav.paths(), vec![format!("/dav/coaching/{}.ics", p.uid)]);
    let (error, _, _, href, _) = writeback_state(&p.pool, &p.id).await;
    assert_eq!(error, None);
    assert_eq!(href.as_deref(), Some("/dav/coaching/"));
}

/// A collective team booking: each member's copy goes to their own choice,
/// or their source setting, and a cancellation deletes every copy.
#[tokio::test]
async fn collective_members_each_write_to_their_own_destination() {
    let pool = setup_test_db().await;
    let alice = insert_role_user(&pool, "alice@dest.test", "user").await;
    let bob = insert_role_user(&pool, "bob@dest.test", "user").await;
    for user in [&alice, &bob] {
        insert_account(&pool, user).await;
    }
    let (_, et_id) = insert_team_with_et(
        &pool,
        "squad",
        "sync",
        &[(&alice, "admin"), (&bob, "member")],
    )
    .await;
    sqlx::query("UPDATE event_types SET scheduling_mode = 'collective' WHERE id = ?")
        .bind(&et_id)
        .execute(&pool)
        .await
        .unwrap();
    let (alice_base, alice_dav) = spawn_mock_dav(DavMode::Ok).await;
    let alice_source = seed_write_source(&pool, &alice, &alice_base).await;
    let alice_team = insert_calendar(&pool, &alice_source, "/dav/team/", "Team").await;
    let (bob_base, bob_dav) = spawn_mock_dav(DavMode::Ok).await;
    seed_write_source(&pool, &bob, &bob_base).await;
    choose(&pool, &et_id, &alice, &alice_team).await;

    let id = insert_v1_booking(
        &pool,
        &et_id,
        "guest@dest.test",
        Utc::now() + Duration::days(10),
        30,
        "confirmed",
        &[],
    )
    .await;
    let uid = format!("uid-{id}");
    push_fresh(&pool, &alice, &uid).await;
    assert_eq!(alice_dav.paths(), vec![format!("/dav/team/{uid}.ics")]);
    assert_eq!(bob_dav.paths(), vec![format!("/dav/cal/{uid}.ics")]);
    assert_eq!(
        copies(&pool, &uid).await,
        vec![(alice.clone(), "/dav/team/".to_string())]
    );

    // Alice goes back to her source setting: her copy moves at the next
    // push, Bob's stays.
    sqlx::query("DELETE FROM event_type_write_targets")
        .execute(&pool)
        .await
        .unwrap();
    push_fresh(&pool, &alice, &uid).await;
    assert_eq!(alice_dav.paths(), vec![format!("/dav/cal/{uid}.ics")]);
    assert_eq!(bob_dav.paths(), vec![format!("/dav/cal/{uid}.ics")]);

    choose(&pool, &et_id, &alice, &alice_team).await;
    push_fresh(&pool, &alice, &uid).await;
    assert_eq!(alice_dav.paths(), vec![format!("/dav/team/{uid}.ics")]);
    caldav_delete_booking(&pool, &[0u8; 32], &alice, &uid).await;
    assert_eq!(alice_dav.stored(), 0);
    assert_eq!(bob_dav.stored(), 0);
}

/// A team member can only choose for team event types of their own teams.
#[tokio::test]
async fn only_team_members_choose_for_a_team_event_type() {
    let pool = setup_test_db().await;
    let member = insert_role_user(&pool, "member@dest.test", "user").await;
    let outsider = insert_role_user(&pool, "outsider@dest.test", "user").await;
    let (_, et_id) = insert_team_with_et(&pool, "crew", "call", &[(&member, "member")]).await;
    assert!(hosts_event_type(&pool, &member, &et_id).await);
    assert!(!hosts_event_type(&pool, &outsider, &et_id).await);
}

/// The Meet event of a Google Meet booking is created on the chosen Google
/// calendar, and stays where it was created: the push records that
/// calendar without writing a second copy, and a choice made after the
/// Meet was created on the source setting does not move it.
#[tokio::test]
async fn a_google_meet_event_is_created_on_the_destination_and_never_moves() {
    let pool = setup_test_db().await;
    let alice = insert_role_user(&pool, "alice@meetdest.test", "user").await;
    let et_id = insert_personal_et(&pool, &alice, "meet-dest").await;
    sqlx::query("UPDATE event_types SET location_type = 'google_meet' WHERE id = ?")
        .bind(&et_id)
        .execute(&pool)
        .await
        .unwrap();
    let account_id: String = sqlx::query_scalar("SELECT id FROM accounts WHERE user_id = ?")
        .bind(&alice)
        .fetch_one(&pool)
        .await
        .unwrap();
    let write_href = "https://apidata.example.test/dav/alice/events/";
    let coaching_href = "https://apidata.example.test/dav/coaching/events/";
    let source_id = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO caldav_sources (id, account_id, name, url, username, enabled, \
             write_calendar_href, auth_type, oauth2_provider, access_token_enc, token_expires_at) \
             VALUES (?, ?, 'Google', 'https://apidata.example.test/dav/', 'alice', 1, ?, \
             'oauth2', 'google', 'unopenable', '2099-01-01T00:00:00')",
    )
    .bind(&source_id)
    .bind(&account_id)
    .bind(write_href)
    .execute(&pool)
    .await
    .unwrap();
    let coaching = insert_calendar(&pool, &source_id, coaching_href, "Coaching").await;
    choose(&pool, &et_id, &alice, &coaching).await;
    let mut uids = Vec::new();
    for day in [17, 18] {
        let start = NaiveDate::from_ymd_opt(2026, 3, day)
            .unwrap()
            .and_hms_opt(10, 0, 0)
            .unwrap();
        insert_booking(&pool, &et_id, None, start, "confirmed").await;
        let uid: String = sqlx::query_scalar("SELECT uid FROM bookings WHERE start_at = ?")
            .bind(start.format("%Y-%m-%dT%H:%M:%S").to_string())
            .fetch_one(&pool)
            .await
            .unwrap();
        uids.push(uid);
    }

    // Not created yet: the Meet goes to the chosen Google calendar.
    let created_here = &uids[0];
    assert_eq!(
        crate::google_meet::meet_destination_calendar(&pool, created_here, &alice).await,
        Some((source_id.clone(), coaching_href.to_string()))
    );
    // What `create_meet_for_booking` records once the event is there.
    record_copy(&pool, created_here, &alice, &source_id, coaching_href).await;
    let details = |uid: &str| crate::email::BookingDetails {
        event_title: "Meet".to_string(),
        date: "2026-03-17".to_string(),
        start_time: "10:00".to_string(),
        end_time: "10:30".to_string(),
        guest_name: "G".to_string(),
        guest_email: "g@e.com".to_string(),
        guest_timezone: "UTC".to_string(),
        host_name: "Alice".to_string(),
        host_email: "alice@meetdest.test".to_string(),
        uid: uid.to_string(),
        host_timezone: "UTC".to_string(),
        ..Default::default()
    };
    for uid in &uids {
        sqlx::query("UPDATE bookings SET meeting_url = ? WHERE uid = ?")
            .bind("https://meet.google.com/aaa-bbbb-ccc")
            .bind(uid)
            .execute(&pool)
            .await
            .unwrap();
    }
    // Created on the source setting before the choice: it stays there.
    assert_eq!(
        crate::google_meet::meet_destination_calendar(&pool, &uids[1], &alice).await,
        None
    );

    for (uid, expected) in [(&uids[0], coaching_href), (&uids[1], write_href)] {
        // The source is unreachable: anything but the Meet skip would fail
        // and leave the href unset.
        let failures = writeback::push_and_record(
            &pool,
            &[0u8; 32],
            &alice,
            uid,
            &details(uid),
            writeback::PushKind::Fresh,
        )
        .await;
        assert!(failures.is_empty(), "{failures:?}");
        let stored: Option<String> =
            sqlx::query_scalar("SELECT caldav_calendar_href FROM bookings WHERE uid = ?")
                .bind(uid)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(stored.as_deref(), Some(expected));
    }
}

// --- Pages ---

/// The sources page lists the event types the user hosts with their
/// calendars, and saves a choice.
#[tokio::test]
async fn the_sources_page_saves_a_destination_per_event_type() {
    let (app, pool, session, et_id) = setup_test_app().await;
    let host: String = sqlx::query_scalar("SELECT id FROM users WHERE email = 'test@example.com'")
        .fetch_one(&pool)
        .await
        .unwrap();
    let source = seed_write_source(&pool, &host, "http://127.0.0.1:9/dav/").await;
    let cal = insert_calendar(&pool, &source, "/dav/coaching/", "Coaching").await;

    let response = app
        .clone()
        .oneshot(get_authed("/dashboard/sources", &session))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = body_string(response).await;
    assert!(body.contains("Bookings by event type"), "{body}");
    assert!(body.contains(&format!("name=\"et_{et_id}\"")), "{body}");
    assert!(body.contains("Coaching (Work calendar)"), "{body}");

    let csrf = "csrf-dest-47";
    let response = app
        .clone()
        .oneshot(post_form(
            "/dashboard/sources/destinations",
            &session,
            csrf,
            &format!("_csrf={csrf}&et_{et_id}={cal}"),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), 303);
    assert!(response.headers()["location"]
        .to_str()
        .unwrap()
        .contains("destinations=saved"));
    assert_eq!(stored_destination(&pool, &et_id, &host).await, Some(cal));

    let response = app
        .oneshot(get_authed(
            "/dashboard/sources?destinations=saved",
            &session,
        ))
        .await
        .unwrap();
    let body = body_string(response).await;
    assert_eq!(
        selected_options(&body, &format!("dest-{et_id}")),
        vec![
            sqlx::query_scalar::<_, String>("SELECT calendar_id FROM event_type_write_targets")
                .fetch_one(&pool)
                .await
                .unwrap()
        ]
    );
}

/// A foreign calendar or event type refuses the whole save; a missing CSRF
/// token refuses the request.
#[tokio::test]
async fn the_sources_page_refuses_foreign_calendars_and_event_types() {
    let (app, pool, session, et_id) = setup_test_app().await;
    let host: String = sqlx::query_scalar("SELECT id FROM users WHERE email = 'test@example.com'")
        .fetch_one(&pool)
        .await
        .unwrap();
    let source = seed_write_source(&pool, &host, "http://127.0.0.1:9/dav/").await;
    let own = insert_calendar(&pool, &source, "/dav/own/", "Own").await;
    let other = insert_role_user(&pool, "other@dest.test", "user").await;
    insert_account(&pool, &other).await;
    let other_et = insert_personal_et(&pool, &other, "other").await;
    let other_source = seed_write_source(&pool, &other, "http://127.0.0.1:9/dav/").await;
    let foreign = insert_calendar(&pool, &other_source, "/dav/foreign/", "Foreign").await;
    let csrf = "csrf-dest-47b";

    for body in [
        format!("_csrf={csrf}&et_{et_id}={own}&et_{other_et}={own}"),
        format!("_csrf={csrf}&et_{et_id}={foreign}"),
    ] {
        let response = app
            .clone()
            .oneshot(post_form(
                "/dashboard/sources/destinations",
                &session,
                csrf,
                &body,
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), 303);
        assert!(response.headers()["location"]
            .to_str()
            .unwrap()
            .contains("destinations=refused"));
    }
    let rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM event_type_write_targets")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(rows, 0);

    let response = app
        .oneshot(post_form(
            "/dashboard/sources/destinations",
            &session,
            csrf,
            &format!("et_{et_id}={own}"),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), 403);
}

/// The personal event type form offers the user's calendars, saves the
/// choice and refuses a calendar that is not theirs.
#[tokio::test]
async fn the_event_type_form_saves_the_destination() {
    let (app, pool, session, et_id) = setup_test_app().await;
    let host: String = sqlx::query_scalar("SELECT id FROM users WHERE email = 'test@example.com'")
        .fetch_one(&pool)
        .await
        .unwrap();
    let source = seed_write_source(&pool, &host, "http://127.0.0.1:9/dav/").await;
    let cal = insert_calendar(&pool, &source, "/dav/coaching/", "Coaching").await;
    let other = insert_role_user(&pool, "other2@dest.test", "user").await;
    insert_account(&pool, &other).await;
    insert_personal_et(&pool, &other, "o").await;
    let other_source = seed_write_source(&pool, &other, "http://127.0.0.1:9/dav/").await;
    let foreign = insert_calendar(&pool, &other_source, "/dav/foreign/", "Foreign").await;

    let response = app
        .clone()
        .oneshot(get_authed(
            "/dashboard/event-types/test-meeting/edit",
            &session,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = body_string(response).await;
    assert!(body.contains("Write confirmed bookings to"), "{body}");
    assert_eq!(
        selected_options(&body, "write_calendar_id"),
        Vec::<String>::new()
    );
    assert!(!body.contains("Foreign"), "{body}");

    let csrf = "csrf-etf-47";
    let form = |calendar: &str| {
        format!(
            "_csrf={csrf}&title=Test+Meeting&slug=test-meeting&duration_min=30&location_type=link&location_value=https%3A%2F%2Fmeet.example.com&write_calendar_id={calendar}"
        )
    };
    let response = app
        .clone()
        .oneshot(post_form(
            "/dashboard/event-types/test-meeting/edit",
            &session,
            csrf,
            &form(&foreign),
        ))
        .await
        .unwrap();
    let body = body_string(response).await;
    assert!(
        body.contains("Nothing was saved: choose one of your own calendars"),
        "{body}"
    );
    assert_eq!(stored_destination(&pool, &et_id, &host).await, None);

    let response = app
        .clone()
        .oneshot(post_form(
            "/dashboard/event-types/test-meeting/edit",
            &session,
            csrf,
            &form(&cal),
        ))
        .await
        .unwrap();
    assert!(response.status().is_redirection(), "{}", response.status());
    assert_eq!(
        stored_destination(&pool, &et_id, &host).await,
        Some(cal.clone())
    );

    let response = app
        .oneshot(get_authed(
            "/dashboard/event-types/test-meeting/edit",
            &session,
        ))
        .await
        .unwrap();
    let body = body_string(response).await;
    assert_eq!(selected_options(&body, "write_calendar_id"), vec![cal]);
}
