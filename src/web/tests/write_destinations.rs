// Write destination per event type (#47): confirmed bookings go to the
// calendar the host chose for the event type, else to the write calendar of
// each of their sources. A child of `web::tests`.

use super::writeback_failures::{push_fresh, spawn_mock_dav, writeback_state, DavMode, MockDav};
use super::*;
use crate::commands::sync::cancel_orphaned_bookings;

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

/// The booking's event as a sync of `calendar_id` would have cached it.
async fn cache_event(pool: &SqlitePool, calendar_id: &str, uid: &str) {
    sqlx::query(
        "INSERT INTO events (id, calendar_id, uid, summary, start_at, end_at, all_day, timezone, status, transp) \
         VALUES (?, ?, ?, 'Booking', '20990101T100000', '20990101T103000', 0, 'UTC', 'CONFIRMED', 'OPAQUE')",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(calendar_id)
    .bind(uid)
    .execute(pool)
    .await
    .unwrap();
}

/// A second source of `user_id` on its own mock server, with one calendar
/// `href`. Returns (server, source id, calendar id).
async fn second_source(
    pool: &SqlitePool,
    user_id: &str,
    mode: DavMode,
    href: &str,
) -> (Arc<MockDav>, String, String) {
    let (base, dav) = spawn_mock_dav(mode).await;
    let source = seed_write_source(pool, user_id, &base).await;
    // Its write calendar is elsewhere: only the chosen calendar is written.
    sqlx::query("UPDATE caldav_sources SET write_calendar_href = NULL WHERE id = ?")
        .bind(&source)
        .execute(pool)
        .await
        .unwrap();
    let cal = insert_calendar(pool, &source, href, "Chosen").await;
    (dav, source, cal)
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

// --- Review fixes ---

/// The blocker: a booking already on the source write calendar W must stay
/// there, cached and pointed at, until its write to the new destination has
/// landed. A failed write leaves it all in place, so the orphan sweep of W
/// keeps the booking; a write that lands moves it, and the sweep of W no
/// longer looks at it.
#[tokio::test]
async fn a_failed_move_keeps_the_old_copy_and_the_orphan_sweep_keeps_the_booking() {
    let p = personal_fixture().await;
    push_fresh(&p.pool, &p.host, &p.uid).await;
    cache_event(&p.pool, &p.work_cal, &p.uid).await;
    let (dest_dav, dest_source, dest_cal) =
        second_source(&p.pool, &p.host, DavMode::Reject, "/dav/chosen/").await;
    choose(&p.pool, &p.et_id, &p.host, &dest_cal).await;

    push_fresh(&p.pool, &p.host, &p.uid).await;
    assert_eq!(p.dav.paths(), vec![format!("/dav/cal/{}.ics", p.uid)]);
    assert_eq!(dest_dav.stored(), 0);
    let (error, _, _, href, _) = writeback_state(&p.pool, &p.id).await;
    assert!(error.is_some());
    assert_eq!(href.as_deref(), Some("/dav/cal/"));
    let cached: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM events WHERE uid = ?")
        .bind(&p.uid)
        .fetch_one(&p.pool)
        .await
        .unwrap();
    assert_eq!(cached, 1);
    cancel_orphaned_bookings(&p.pool, &[0u8; 32], None, &p.source_id).await;
    assert_eq!(booking_status(&p.pool, &p.id).await, "confirmed");

    // The destination comes back: the next push lands there, then W goes.
    dest_dav.set_mode(DavMode::Ok);
    push_fresh(&p.pool, &p.host, &p.uid).await;
    assert_eq!(dest_dav.paths(), vec![format!("/dav/chosen/{}.ics", p.uid)]);
    assert_eq!(p.dav.stored(), 0);
    let (_, _, _, href, _) = writeback_state(&p.pool, &p.id).await;
    assert_eq!(href.as_deref(), Some("/dav/chosen/"));
    cancel_orphaned_bookings(&p.pool, &[0u8; 32], None, &p.source_id).await;
    assert_eq!(booking_status(&p.pool, &p.id).await, "confirmed");
    cache_event(&p.pool, &dest_cal, &p.uid).await;
    cancel_orphaned_bookings(&p.pool, &[0u8; 32], None, &dest_source).await;
    assert_eq!(booking_status(&p.pool, &p.id).await, "confirmed");

    // Deleted from the destination by the host: the sweep of its source
    // matches the recorded copy and cancels.
    sqlx::query("DELETE FROM events WHERE uid = ?")
        .bind(&p.uid)
        .execute(&p.pool)
        .await
        .unwrap();
    cancel_orphaned_bookings(&p.pool, &[0u8; 32], None, &dest_source).await;
    assert_eq!(booking_status(&p.pool, &p.id).await, "cancelled");
}

/// A delete that fails keeps the copy recorded; the next attempt (here the
/// background sweep, the booking being cancelled) removes it.
#[tokio::test]
async fn a_failed_delete_is_retried_by_the_background_sweep() {
    let p = personal_fixture().await;
    choose(&p.pool, &p.et_id, &p.host, &p.coaching_cal).await;
    push_fresh(&p.pool, &p.host, &p.uid).await;
    sqlx::query("UPDATE bookings SET status = 'cancelled' WHERE id = ?")
        .bind(&p.id)
        .execute(&p.pool)
        .await
        .unwrap();
    p.dav.fail_deletes(true);
    caldav_delete_booking(&p.pool, &[0u8; 32], &p.host, &p.uid).await;
    assert_eq!(p.dav.stored(), 1);
    assert_eq!(copies(&p.pool, &p.uid).await.len(), 1);

    p.dav.fail_deletes(false);
    assert_eq!(
        sweep_leftover_copies(&p.pool, &[0u8; 32], Utc::now()).await,
        1
    );
    assert_eq!(p.dav.stored(), 0);
    assert!(copies(&p.pool, &p.uid).await.is_empty());
    assert_eq!(
        sweep_leftover_copies(&p.pool, &[0u8; 32], Utc::now()).await,
        0
    );
}

/// A collective member removed from the team after the booking was written
/// keeps no copy: a cancellation and a push both find theirs.
#[tokio::test]
async fn a_removed_members_copy_is_deleted() {
    let pool = setup_test_db().await;
    let alice = insert_role_user(&pool, "alice@former.test", "user").await;
    let bob = insert_role_user(&pool, "bob@former.test", "user").await;
    for user in [&alice, &bob] {
        insert_account(&pool, user).await;
    }
    let (team_id, et_id) = insert_team_with_et(
        &pool,
        "former",
        "sync",
        &[(&alice, "admin"), (&bob, "member")],
    )
    .await;
    sqlx::query("UPDATE event_types SET scheduling_mode = 'collective' WHERE id = ?")
        .bind(&et_id)
        .execute(&pool)
        .await
        .unwrap();
    let (alice_base, _alice_dav) = spawn_mock_dav(DavMode::Ok).await;
    seed_write_source(&pool, &alice, &alice_base).await;
    let (bob_dav, _, bob_cal) = second_source(&pool, &bob, DavMode::Ok, "/dav/bob/").await;
    choose(&pool, &et_id, &bob, &bob_cal).await;
    let mut uids = Vec::new();
    for days in [10, 11] {
        let id = insert_v1_booking(
            &pool,
            &et_id,
            "guest@former.test",
            Utc::now() + Duration::days(days),
            30,
            "confirmed",
            &[],
        )
        .await;
        let uid = format!("uid-{id}");
        push_fresh(&pool, &alice, &uid).await;
        uids.push(uid);
    }
    assert_eq!(bob_dav.stored(), 2);
    sqlx::query("DELETE FROM team_members WHERE team_id = ? AND user_id = ?")
        .bind(&team_id)
        .bind(&bob)
        .execute(&pool)
        .await
        .unwrap();

    caldav_delete_booking(&pool, &[0u8; 32], &alice, &uids[0]).await;
    push_fresh(&pool, &alice, &uids[1]).await;
    assert_eq!(bob_dav.stored(), 0);
    assert!(copies(&pool, &uids[1]).await.is_empty());
}

/// A host whose only Google calendar to write to is the one chosen for the
/// event type still gets the Meet.
#[tokio::test]
async fn a_google_destination_makes_its_host_the_meet_owner() {
    let pool = setup_test_db().await;
    let alice = insert_role_user(&pool, "alice@meetowner.test", "user").await;
    let et_id = insert_personal_et(&pool, &alice, "meet-owner").await;
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
    let source_id = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO caldav_sources (id, account_id, name, url, username, enabled, \
             auth_type, oauth2_provider, access_token_enc, token_expires_at) \
             VALUES (?, ?, 'Google', 'https://apidata.example.test/dav/', 'alice', 1, \
             'oauth2', 'google', 'unopenable', '2099-01-01T00:00:00')",
    )
    .bind(&source_id)
    .bind(&account_id)
    .execute(&pool)
    .await
    .unwrap();
    let cal = insert_calendar(
        &pool,
        &source_id,
        "https://apidata.example.test/dav/c/",
        "C",
    )
    .await;
    let start = NaiveDate::from_ymd_opt(2026, 3, 17)
        .unwrap()
        .and_hms_opt(10, 0, 0)
        .unwrap();
    insert_booking(&pool, &et_id, None, start, "confirmed").await;
    let booking_id: String = sqlx::query_scalar("SELECT id FROM bookings LIMIT 1")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        crate::google_meet::elect_meet_owner(&pool, &booking_id).await,
        None
    );
    assert!(
        crate::google_meet::google_meet_prereq_error(&pool, Some(&alice), None, Some(&et_id))
            .await
            .is_some()
    );
    choose(&pool, &et_id, &alice, &cal).await;
    assert_eq!(
        crate::google_meet::elect_meet_owner(&pool, &booking_id).await,
        Some(alice.clone())
    );
    assert_eq!(
        crate::google_meet::google_meet_prereq_error(&pool, Some(&alice), None, Some(&et_id)).await,
        None
    );
}

/// A destination deleted on the server (404) or read-only (403) refuses the
/// write for good: the booking goes to the source setting at once, without
/// a failure or retries, and the sources page says so. A later write that
/// lands on the destination clears the note.
#[tokio::test]
async fn a_destination_refusing_writes_falls_back_to_the_source_setting() {
    let (app, pool, session, et_id) = setup_test_app().await;
    let host: String = sqlx::query_scalar("SELECT id FROM users WHERE email = 'test@example.com'")
        .fetch_one(&pool)
        .await
        .unwrap();
    let (base, work_dav) = spawn_mock_dav(DavMode::Ok).await;
    seed_write_source(&pool, &host, &base).await;
    let (dest_dav, _, dest_cal) = second_source(&pool, &host, DavMode::Gone, "/dav/gone/").await;
    choose(&pool, &et_id, &host, &dest_cal).await;
    let id = insert_v1_booking(
        &pool,
        &et_id,
        "guest@refused.test",
        Utc::now() + Duration::days(10),
        30,
        "confirmed",
        &[],
    )
    .await;
    let uid = format!("uid-{id}");
    push_fresh(&pool, &host, &uid).await;
    assert_eq!(work_dav.paths(), vec![format!("/dav/cal/{uid}.ics")]);
    let (error, attempts, _, href, _) = writeback_state(&pool, &id).await;
    assert_eq!(error, None);
    assert_eq!(attempts, 0);
    assert_eq!(href.as_deref(), Some("/dav/cal/"));
    assert!(refusal(&pool, &et_id, &host).await.is_some());
    let body = body_string(
        app.clone()
            .oneshot(get_authed("/dashboard/sources", &session))
            .await
            .unwrap(),
    )
    .await;
    assert!(
        body.contains("This calendar refused the last booking"),
        "{body}"
    );

    // The calendar is back: the next push goes there and the note goes.
    dest_dav.set_mode(DavMode::Ok);
    push_fresh(&pool, &host, &uid).await;
    assert_eq!(dest_dav.paths(), vec![format!("/dav/gone/{uid}.ics")]);
    assert_eq!(work_dav.stored(), 0);
    assert_eq!(refusal(&pool, &et_id, &host).await, None);
    // The note is the status line, never the server's HTML body.
    sqlx::query("UPDATE event_type_write_targets SET last_refused_at = datetime('now')")
        .execute(&pool)
        .await
        .unwrap();
    dest_dav.set_mode(DavMode::Gone);
    push_fresh(&pool, &host, &uid).await;
    assert_eq!(
        refusal(&pool, &et_id, &host).await.as_deref(),
        Some("404 Not Found")
    );
}

/// A choice whose source is disabled stays selected (as unavailable) and
/// survives saving the forms unchanged.
#[tokio::test]
async fn a_choice_on_a_disabled_source_survives_saving_the_forms() {
    let (app, pool, session, et_id) = setup_test_app().await;
    let host: String = sqlx::query_scalar("SELECT id FROM users WHERE email = 'test@example.com'")
        .fetch_one(&pool)
        .await
        .unwrap();
    seed_write_source(&pool, &host, "http://127.0.0.1:9/dav/").await;
    let other = seed_write_source(&pool, &host, "http://127.0.0.1:9/dav/").await;
    let cal = insert_calendar(&pool, &other, "/dav/off/", "Paused").await;
    choose(&pool, &et_id, &host, &cal).await;
    sqlx::query("UPDATE caldav_sources SET enabled = 0 WHERE id = ?")
        .bind(&other)
        .execute(&pool)
        .await
        .unwrap();

    let body = body_string(
        app.clone()
            .oneshot(get_authed("/dashboard/sources", &session))
            .await
            .unwrap(),
    )
    .await;
    assert!(
        body.contains("Paused (Work calendar) (unavailable)"),
        "{body}"
    );
    assert_eq!(
        selected_options(&body, &format!("dest-{et_id}")),
        vec![cal.clone()]
    );
    let csrf = "csrf-dest-off";
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
    assert!(response.headers()["location"]
        .to_str()
        .unwrap()
        .contains("destinations=saved"));

    let body = body_string(
        app.clone()
            .oneshot(get_authed(
                "/dashboard/event-types/test-meeting/edit",
                &session,
            ))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(
        selected_options(&body, "write_calendar_id"),
        vec![cal.clone()]
    );
    let response = app
        .oneshot(post_form(
            "/dashboard/event-types/test-meeting/edit",
            &session,
            csrf,
            &format!(
                "_csrf={csrf}&title=Test+Meeting&slug=test-meeting&duration_min=30&location_type=link&location_value=https%3A%2F%2Fmeet.example.com&write_calendar_id={cal}"
            ),
        ))
        .await
        .unwrap();
    assert!(response.status().is_redirection(), "{}", response.status());
    assert_eq!(stored_destination(&pool, &et_id, &host).await, Some(cal));
}

/// The same href on two servers is two places: a recorded copy on one does
/// not stop the delete on the other.
#[tokio::test]
async fn deletes_tell_apart_the_same_href_on_two_servers() {
    let pool = setup_test_db().await;
    let alice = insert_role_user(&pool, "alice@twohref.test", "user").await;
    let bob = insert_role_user(&pool, "bob@twohref.test", "user").await;
    for user in [&alice, &bob] {
        insert_account(&pool, user).await;
    }
    let (_, et_id) = insert_team_with_et(
        &pool,
        "twohref",
        "sync",
        &[(&alice, "admin"), (&bob, "member")],
    )
    .await;
    sqlx::query("UPDATE event_types SET scheduling_mode = 'collective' WHERE id = ?")
        .bind(&et_id)
        .execute(&pool)
        .await
        .unwrap();
    let (base_a, dav_a) = spawn_mock_dav(DavMode::Ok).await;
    let source_a = seed_write_source(&pool, &alice, &base_a).await;
    let (base_b, dav_b) = spawn_mock_dav(DavMode::Ok).await;
    seed_write_source(&pool, &alice, &base_b).await;
    let id = insert_v1_booking(
        &pool,
        &et_id,
        "guest@twohref.test",
        Utc::now() + Duration::days(10),
        30,
        "confirmed",
        &[],
    )
    .await;
    let uid = format!("uid-{id}");
    push_fresh(&pool, &alice, &uid).await;
    assert_eq!((dav_a.stored(), dav_b.stored()), (1, 1));
    record_copy(&pool, &uid, &alice, &source_a, "/dav/cal/").await;
    caldav_delete_booking(&pool, &[0u8; 32], &alice, &uid).await;
    assert_eq!((dav_a.stored(), dav_b.stored()), (0, 0));
}

/// Moving a host's copy off a calendar the claimant shares with them, by a
/// retry, writes the claimant's copy back there.
#[tokio::test]
async fn a_retry_that_moves_the_hosts_copy_rewrites_the_claimants() {
    let p = personal_fixture().await;
    push_fresh(&p.pool, &p.host, &p.uid).await;
    let watcher = insert_role_user(&p.pool, "watcher@dest.test", "user").await;
    insert_account(&p.pool, &watcher).await;
    // The claimant writes to the host's own collection (a shared calendar).
    let base: String = sqlx::query_scalar("SELECT url FROM caldav_sources WHERE id = ?")
        .bind(&p.source_id)
        .fetch_one(&p.pool)
        .await
        .unwrap();
    seed_write_source(&p.pool, &watcher, &base).await;
    sqlx::query("UPDATE bookings SET claimed_by_user_id = ? WHERE id = ?")
        .bind(&watcher)
        .bind(&p.id)
        .execute(&p.pool)
        .await
        .unwrap();
    choose(&p.pool, &p.et_id, &p.host, &p.coaching_cal).await;
    let (details, _) = writeback::writeback_details(&p.pool, &p.uid).await.unwrap();
    writeback::push_and_record(
        &p.pool,
        &[0u8; 32],
        &p.host,
        &p.uid,
        &details,
        writeback::PushKind::Retry,
    )
    .await;
    assert_eq!(
        p.dav.paths(),
        vec![
            format!("/dav/cal/{}.ics", p.uid),
            format!("/dav/coaching/{}.ics", p.uid)
        ]
    );
    // The first write, the host's copy at the destination, and the
    // claimant's copy written again on the shared calendar.
    assert_eq!(p.dav.puts(), 3);
}

// --- Second review ---

/// A PUT refused for good is a permanent refusal, from the status and the
/// server's answer, never from a message; a rate limit and a write that was
/// accepted but not verified are not.
#[test]
fn only_a_definitive_refusal_counts() {
    use crate::caldav::refused_for_good;
    assert!(refused_for_good(404, ""));
    assert!(refused_for_good(405, ""));
    assert!(refused_for_good(403, "<error>forbidden</error>"));
    assert!(!refused_for_good(
        403,
        r#"{"error":{"errors":[{"reason":"rateLimitExceeded"}]}}"#
    ));
    assert!(!refused_for_good(403, "userRateLimitExceeded"));
    assert!(!refused_for_good(503, ""));
    assert!(!refused_for_good(400, ""));
    let verify = crate::providers::WriteBackError::Verify(anyhow::anyhow!("returned 404"));
    assert_eq!(verify.permanent_refusal(), None);
    let named =
        crate::providers::WriteBackError::Put(anyhow::anyhow!("PUT returned 404 Not Found"));
    assert_eq!(named.permanent_refusal(), None);
    let refused = crate::providers::WriteBackError::Put(anyhow::Error::new(
        crate::providers::PermanentRefusal {
            summary: "404 Not Found".to_string(),
            message: "PUT http://x returned 404 Not Found".to_string(),
        },
    ));
    assert_eq!(refused.permanent_refusal(), Some("404 Not Found"));
}

/// EWS: a folder gone or not writable is a permanent refusal too.
#[test]
fn ews_access_denied_and_folder_not_found_are_permanent() {
    use crate::ews::permanent_write_fault;
    let prefix = crate::ews::soap::SOAP_FAULT_PREFIX;
    assert_eq!(
        permanent_write_fault(&format!("{prefix}ErrorAccessDenied: Access is denied.")),
        Some("ErrorAccessDenied")
    );
    assert_eq!(
        permanent_write_fault(&format!("{prefix}ErrorFolderNotFound")),
        Some("ErrorFolderNotFound")
    );
    assert_eq!(
        permanent_write_fault(&format!("{prefix}ErrorServerBusy")),
        None
    );
    assert_eq!(permanent_write_fault("ErrorAccessDenied"), None);
}

/// Two sources on different servers with the same calendar path: once the
/// booking moved from A to B, neither the delta path nor the sweep of A may
/// take A's 404 for the booking's.
#[tokio::test]
async fn the_same_path_on_another_server_is_not_the_bookings_calendar() {
    let p = personal_fixture().await;
    push_fresh(&p.pool, &p.host, &p.uid).await;
    let (b_dav, b_source, b_cal) = second_source(&p.pool, &p.host, DavMode::Ok, "/dav/cal/").await;
    choose(&p.pool, &p.et_id, &p.host, &b_cal).await;
    push_fresh(&p.pool, &p.host, &p.uid).await;
    assert_eq!(p.dav.stored(), 0);
    assert_eq!(b_dav.paths(), vec![format!("/dav/cal/{}.ics", p.uid)]);
    let stored: (Option<String>, Option<String>) =
        sqlx::query_as("SELECT caldav_calendar_href, caldav_source_id FROM bookings WHERE id = ?")
            .bind(&p.id)
            .fetch_one(&p.pool)
            .await
            .unwrap();
    assert_eq!(
        stored,
        (Some("/dav/cal/".to_string()), Some(b_source.clone()))
    );

    // A's delta reports the uid gone, its server confirms: not this booking.
    let outcome = crate::commands::sync::cancel_orphaned_booking(
        &p.pool,
        &[0u8; 32],
        crate::commands::sync::OrphanCheck::Caldav(None),
        &p.source_id,
        &p.uid,
    )
    .await;
    assert_eq!(outcome, crate::commands::sync::OrphanOutcome::NoAction);
    // B not synced yet: the sweep of A leaves it too.
    cancel_orphaned_bookings(&p.pool, &[0u8; 32], None, &p.source_id).await;
    assert_eq!(booking_status(&p.pool, &p.id).await, "confirmed");
    // B's own sweep still watches it.
    cancel_orphaned_bookings(&p.pool, &[0u8; 32], None, &b_source).await;
    assert_eq!(booking_status(&p.pool, &p.id).await, "cancelled");
}

/// A first move whose write to the destination is accepted but not
/// verified keeps the source-setting copy recorded, so the cancellation
/// still deletes it.
#[tokio::test]
async fn an_unverified_first_move_still_lets_the_cancellation_delete_the_old_copy() {
    let p = personal_fixture().await;
    push_fresh(&p.pool, &p.host, &p.uid).await;
    let (dest_dav, _, dest_cal) =
        second_source(&p.pool, &p.host, DavMode::Drop, "/dav/chosen/").await;
    choose(&p.pool, &p.et_id, &p.host, &dest_cal).await;
    push_fresh(&p.pool, &p.host, &p.uid).await;
    assert_eq!(dest_dav.puts(), 1);
    assert_eq!(p.dav.stored(), 1);
    let (error, _, _, href, _) = writeback_state(&p.pool, &p.id).await;
    assert!(error.is_some());
    assert_eq!(href.as_deref(), Some("/dav/chosen/"));
    caldav_delete_booking(&p.pool, &[0u8; 32], &p.host, &p.uid).await;
    assert_eq!(p.dav.stored(), 0);
    assert!(copies(&p.pool, &p.uid).await.is_empty());
}

/// The background sweep backs off between attempts, gives up after the
/// last, and drops the rows of bookings that are over.
#[tokio::test]
async fn the_leftover_sweep_backs_off_gives_up_and_prunes_past_bookings() {
    let p = personal_fixture().await;
    choose(&p.pool, &p.et_id, &p.host, &p.coaching_cal).await;
    push_fresh(&p.pool, &p.host, &p.uid).await;
    sqlx::query("UPDATE bookings SET status = 'cancelled' WHERE id = ?")
        .bind(&p.id)
        .execute(&p.pool)
        .await
        .unwrap();
    p.dav.fail_deletes(true);
    let key = [0u8; 32];
    let mut now = Utc::now();
    assert_eq!(sweep_leftover_copies(&p.pool, &key, now).await, 1);
    let (attempts, next): (i64, Option<String>) =
        sqlx::query_as("SELECT delete_attempts, next_delete_at FROM booking_write_copies")
            .fetch_one(&p.pool)
            .await
            .unwrap();
    assert_eq!(attempts, 1);
    assert!(next.is_some());
    // Not due yet.
    assert_eq!(sweep_leftover_copies(&p.pool, &key, now).await, 0);
    for _ in 0..writeback::RETRY_DELAYS_MIN.len() {
        now += Duration::hours(9);
        assert_eq!(sweep_leftover_copies(&p.pool, &key, now).await, 1);
    }
    // Given up after the last attempt.
    assert!(copies(&p.pool, &p.uid).await.is_empty());
    assert_eq!(p.dav.stored(), 1);

    // A booking that is over: its rows go without a delete.
    let past = insert_v1_booking(
        &p.pool,
        &p.et_id,
        "past@dest.test",
        Utc::now() - Duration::days(2),
        30,
        "confirmed",
        &[],
    )
    .await;
    let past_uid = format!("uid-{past}");
    record_copy(&p.pool, &past_uid, &p.host, &p.source_id, "/dav/coaching/").await;
    sweep_leftover_copies(&p.pool, &key, Utc::now()).await;
    assert!(copies(&p.pool, &past_uid).await.is_empty());
}

/// With nothing to fall back to, a refusal stays a write-back failure, so
/// the host sees the badge and gets the alert.
#[tokio::test]
async fn a_refusal_with_no_source_write_calendar_is_a_failure() {
    let pool = setup_test_db().await;
    let host = insert_role_user(&pool, "host@nofallback.test", "user").await;
    let et_id = insert_personal_et(&pool, &host, "nofallback").await;
    let (_, _, dest_cal) = second_source(&pool, &host, DavMode::Gone, "/dav/gone/").await;
    choose(&pool, &et_id, &host, &dest_cal).await;
    let id = insert_v1_booking(
        &pool,
        &et_id,
        "guest@nofallback.test",
        Utc::now() + Duration::days(10),
        30,
        "confirmed",
        &[],
    )
    .await;
    push_fresh(&pool, &host, &format!("uid-{id}")).await;
    let (error, attempts, next, _, _) = writeback_state(&pool, &id).await;
    assert!(error.unwrap_or_default().contains("404"));
    assert_eq!(attempts, 1);
    assert!(next.is_some());
    assert!(refusal(&pool, &et_id, &host).await.is_some());
}

/// Choosing another calendar clears the refusal noted on the earlier one.
#[tokio::test]
async fn changing_the_choice_clears_the_refusal() {
    let p = personal_fixture().await;
    set_destination(&p.pool, &p.host, &p.et_id, &p.coaching_cal)
        .await
        .unwrap();
    note_refusal(&p.pool, &p.uid, &p.host, "404 Not Found").await;
    assert!(refusal(&p.pool, &p.et_id, &p.host).await.is_some());
    // Saving the same choice again keeps it.
    set_destination(&p.pool, &p.host, &p.et_id, &p.coaching_cal)
        .await
        .unwrap();
    assert!(refusal(&p.pool, &p.et_id, &p.host).await.is_some());
    set_destination(&p.pool, &p.host, &p.et_id, &p.work_cal)
        .await
        .unwrap();
    assert_eq!(refusal(&p.pool, &p.et_id, &p.host).await, None);
}

/// A copy recorded on a disabled source is left alone, like writes, and
/// stays recorded for when the source comes back.
#[tokio::test]
async fn copies_on_a_disabled_source_are_left_alone() {
    let p = personal_fixture().await;
    choose(&p.pool, &p.et_id, &p.host, &p.coaching_cal).await;
    push_fresh(&p.pool, &p.host, &p.uid).await;
    sqlx::query("UPDATE caldav_sources SET enabled = 0 WHERE id = ?")
        .bind(&p.source_id)
        .execute(&p.pool)
        .await
        .unwrap();
    caldav_delete_booking(&p.pool, &[0u8; 32], &p.host, &p.uid).await;
    assert_eq!(p.dav.stored(), 1);
    assert_eq!(copies(&p.pool, &p.uid).await.len(), 1);
}

/// Pushes of one booking run one at a time.
#[tokio::test]
async fn pushes_of_one_booking_are_serialised() {
    let p = personal_fixture().await;
    let guard = writeback::hold_push_lock(&p.uid).await;
    let pool = p.pool.clone();
    let host = p.host.clone();
    let uid = p.uid.clone();
    let push = tokio::spawn(async move { push_fresh(&pool, &host, &uid).await });
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    assert_eq!(p.dav.puts(), 0);
    drop(guard);
    push.await.unwrap();
    assert_eq!(p.dav.puts(), 1);
}
