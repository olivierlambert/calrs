// Google copies of a booking and their attendees (#237). Google puts an
// event on the Google calendar of each of its attendees by itself, so only
// one copy per booking may carry the guests: the Meet owner's, else the
// primary host's. A child of `web::tests`, so it shares that module's helpers.

use super::*;
use crate::google_calendar::fake::{self, assert_nothing_announced, FakeGoogle};
use serde_json::json;

const KEY: [u8; 32] = [0u8; 32];

/// A Google write source with a valid access token, writing to the primary
/// calendar `calendar` on the fake. Creates the user's account if missing.
async fn google_source(pool: &SqlitePool, id: &str, user_id: &str, base: &str, calendar: &str) {
    let account_id: Option<String> =
        sqlx::query_scalar("SELECT id FROM accounts WHERE user_id = ?")
            .bind(user_id)
            .fetch_optional(pool)
            .await
            .unwrap();
    let account_id = match account_id {
        Some(id) => id,
        None => {
            let id = uuid::Uuid::new_v4().to_string();
            sqlx::query(
                "INSERT INTO accounts (id, name, email, timezone, user_id) \
                 VALUES (?, 'A', ?, 'UTC', ?)",
            )
            .bind(&id)
            .bind(format!("account-{user_id}@x"))
            .bind(user_id)
            .execute(pool)
            .await
            .unwrap();
            id
        }
    };
    sqlx::query(
        "INSERT INTO caldav_sources (id, account_id, name, url, username, enabled, write_calendar_href, \
         auth_type, oauth2_provider, access_token_enc, token_expires_at) \
         VALUES (?, ?, 'Google', ?, ?, 1, ?, 'oauth2', 'google', ?, ?)",
    )
    .bind(id)
    .bind(&account_id)
    .bind(base)
    .bind(calendar)
    .bind(format!(
        "/caldav/v2/{}/events",
        urlencoding::encode(calendar)
    ))
    .bind(crate::crypto::encrypt_password(&KEY, "access-token").unwrap())
    .bind((Utc::now() + Duration::hours(1)).to_rfc3339())
    .execute(pool)
    .await
    .unwrap();
}

async fn only_booking_uid(pool: &SqlitePool, et_id: &str) -> String {
    sqlx::query_scalar("SELECT uid FROM bookings WHERE event_type_id = ?")
        .bind(et_id)
        .fetch_one(pool)
        .await
        .unwrap()
}

fn details(uid: &str) -> crate::email::BookingDetails {
    crate::email::BookingDetails {
        event_title: "Intro".to_string(),
        date: "2026-03-17".to_string(),
        start_time: "10:00".to_string(),
        end_time: "10:30".to_string(),
        guest_name: "Jane Doe".to_string(),
        guest_email: "jane@guest.test".to_string(),
        guest_timezone: "UTC".to_string(),
        host_name: "Host".to_string(),
        host_email: "host@google.test".to_string(),
        uid: uid.to_string(),
        host_timezone: "UTC".to_string(),
        additional_attendees: vec!["sam@guest.test".to_string()],
        ..Default::default()
    }
}

/// The attendee list of the copy that carries the guests.
fn guests() -> Vec<serde_json::Value> {
    vec![
        json!({"email": "jane@guest.test", "displayName": "Jane Doe"}),
        json!({"email": "sam@guest.test"}),
    ]
}

fn start() -> NaiveDateTime {
    dt(2026, 3, 17, 10, 0)
}

/// Round-robin: the assigned member's copy carries the guests, on their
/// first Google source only; a second Google calendar of theirs gets none,
/// and the event type owner, who is not the host, gets nothing at all.
#[tokio::test]
async fn round_robin_puts_the_guests_on_the_assigned_members_first_google_copy() {
    let pool = setup_test_db().await;
    let alice = insert_role_user(&pool, "alice@google.test", "user").await;
    let bob = insert_role_user(&pool, "bob@google.test", "user").await;
    let (_, et_id) = insert_team_with_et(
        &pool,
        "g237rr",
        "g237rr-et",
        &[(&alice, "member"), (&bob, "member")],
    )
    .await;
    let google = Arc::new(FakeGoogle::default());
    let (base, server) = fake::serve(google.clone()).await;
    google_source(&pool, "src-a", &alice, &base, "alice@google.test").await;
    google_source(&pool, "src-b1", &bob, &base, "bob@google.test").await;
    google_source(&pool, "src-b2", &bob, &base, "bob-team@google.test").await;
    insert_booking(&pool, &et_id, Some(&bob), start(), "confirmed").await;
    let uid = only_booking_uid(&pool, &et_id).await;

    caldav_push_booking(&pool, &KEY, &alice, &uid, &details(&uid)).await;
    server.abort();

    assert_eq!(google.attendees("bob@google.test", &uid), Some(guests()));
    assert_eq!(
        google.attendees("bob-team@google.test", &uid),
        Some(Vec::new())
    );
    assert_eq!(google.attendees("alice@google.test", &uid), None);
    assert_nothing_announced(&google.log());
}

/// Collective with two Google members: only the organizer's copy (the
/// first member by name here, none being stored) carries the guests. The
/// other member's copy, which an earlier calrs wrote with attendees, loses
/// them silently, and keeps none through a retry. Deleting the booking
/// goes through the Calendar API for the copy with attendees only.
#[tokio::test]
async fn collective_puts_the_guests_on_one_google_copy_only() {
    let pool = setup_test_db().await;
    let alice = insert_role_user(&pool, "alice@google.test", "user").await;
    let bob = insert_role_user(&pool, "bob@google.test", "user").await;
    let (_, et_id) = insert_team_with_et(
        &pool,
        "g237co",
        "g237co-et",
        &[(&alice, "member"), (&bob, "member")],
    )
    .await;
    sqlx::query("UPDATE event_types SET scheduling_mode = 'collective' WHERE id = ?")
        .bind(&et_id)
        .execute(&pool)
        .await
        .unwrap();
    let google = Arc::new(FakeGoogle::default());
    let (base, server) = fake::serve(google.clone()).await;
    google_source(&pool, "src-a", &alice, &base, "alice@google.test").await;
    google_source(&pool, "src-b", &bob, &base, "bob@google.test").await;
    insert_booking(&pool, &et_id, None, start(), "confirmed").await;
    let uid = only_booking_uid(&pool, &et_id).await;
    google.insert("bob@google.test", &uid, "ev-bob", guests());

    caldav_push_booking(&pool, &KEY, &alice, &uid, &details(&uid)).await;
    assert_eq!(google.attendees("alice@google.test", &uid), Some(guests()));
    assert_eq!(google.attendees("bob@google.test", &uid), Some(Vec::new()));
    assert!(google.log().iter().any(|e| e
        .starts_with("PATCH /api/calendars/bob%40google.test/events/ev-bob?")
        && e.ends_with(r#"{"attendees":[]}"#)));

    let failures = super::super::writeback::push_and_record(
        &pool,
        &KEY,
        &alice,
        &uid,
        &details(&uid),
        super::super::writeback::PushKind::Retry,
    )
    .await;
    assert!(failures.is_empty(), "{failures:?}");
    assert_eq!(google.attendees("alice@google.test", &uid), Some(guests()));
    assert_eq!(google.attendees("bob@google.test", &uid), Some(Vec::new()));

    caldav_delete_booking(&pool, &KEY, &alice, &uid).await;
    server.abort();
    let log = google.log();
    assert_nothing_announced(&log);
    assert!(log
        .iter()
        .any(|e| e.starts_with("DELETE /api/calendars/alice%40google.test/events/")));
    assert!(log.iter().any(|e| e.starts_with(&format!(
        "DELETE /caldav/v2/bob%40google.test/events/{uid}.ics"
    ))));
    assert!(google.is_empty());
}

/// A claimed booking, the claimant listed as an attendee the way a claim
/// lists them (row by user, address under their claimant alias), pushed
/// once. Returns the uid and the fake.
async fn claimed_booking(claimant_writes: bool) -> (String, Arc<FakeGoogle>) {
    let pool = setup_test_db().await;
    let alice = insert_role_user(&pool, "alice@google.test", "user").await;
    let carol = insert_role_user(&pool, "carol@google.test", "user").await;
    let et_id = insert_personal_et(&pool, &alice, "g237claim").await;
    let google = Arc::new(FakeGoogle::default());
    let (base, server) = fake::serve(google.clone()).await;
    google_source(&pool, "src-a", &alice, &base, "alice@google.test").await;
    if claimant_writes {
        google_source(&pool, "src-c", &carol, &base, "carol@google.test").await;
    }
    insert_booking(&pool, &et_id, None, start(), "confirmed").await;
    let uid = only_booking_uid(&pool, &et_id).await;
    sqlx::query("UPDATE bookings SET claimed_by_user_id = ? WHERE uid = ?")
        .bind(&carol)
        .bind(&uid)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO booking_attendees (id, booking_id, email, added_by_claim, claimant_user_id) \
         SELECT 'ba-carol', id, 'Carol.Watch@google.test', 1, ? FROM bookings WHERE uid = ?",
    )
    .bind(&carol)
    .bind(&uid)
    .execute(&pool)
    .await
    .unwrap();
    caldav_push_booking(&pool, &KEY, &alice, &uid, &claimed_details(&uid)).await;
    server.abort();
    (uid, google)
}

fn claimed_details(uid: &str) -> crate::email::BookingDetails {
    let mut details = details(uid);
    details
        .additional_attendees
        .push("Carol.Watch@google.test".to_string());
    details
}

/// A claimed booking: the host's copy carries the guests and not the
/// claimant, who has their own copy and would otherwise see the meeting
/// twice; the claimant's copy carries no attendees, even when an earlier
/// calrs gave it some.
#[tokio::test]
async fn a_claimants_google_copy_carries_no_attendees() {
    let pool = setup_test_db().await;
    let alice = insert_role_user(&pool, "alice@google.test", "user").await;
    let carol = insert_role_user(&pool, "carol@google.test", "user").await;
    let et_id = insert_personal_et(&pool, &alice, "g237claim").await;
    let google = Arc::new(FakeGoogle::default());
    let (base, server) = fake::serve(google.clone()).await;
    google_source(&pool, "src-a", &alice, &base, "alice@google.test").await;
    google_source(&pool, "src-c", &carol, &base, "carol@google.test").await;
    insert_booking(&pool, &et_id, None, start(), "confirmed").await;
    let uid = only_booking_uid(&pool, &et_id).await;
    sqlx::query("UPDATE bookings SET claimed_by_user_id = ? WHERE uid = ?")
        .bind(&carol)
        .bind(&uid)
        .execute(&pool)
        .await
        .unwrap();
    google.insert("carol@google.test", &uid, "ev-carol", guests());

    caldav_push_booking(&pool, &KEY, &alice, &uid, &details(&uid)).await;
    server.abort();

    assert_eq!(google.attendees("alice@google.test", &uid), Some(guests()));
    assert_eq!(
        google.attendees("carol@google.test", &uid),
        Some(Vec::new())
    );
    assert_nothing_announced(&google.log());
}

/// The claimant is left off the host's Google copy when calrs writes them
/// a copy of their own, under the address the claim listed them with.
#[tokio::test]
async fn a_claimant_with_their_own_copy_is_not_a_google_attendee() {
    let (uid, google) = claimed_booking(true).await;
    assert_eq!(google.attendees("alice@google.test", &uid), Some(guests()));
    assert_eq!(
        google.attendees("carol@google.test", &uid),
        Some(Vec::new())
    );
    assert_nothing_announced(&google.log());
}

/// Without a copy of their own, the claimant stays on the host's Google
/// copy: the invitation is how the meeting reaches their calendar.
#[tokio::test]
async fn a_claimant_without_their_own_copy_stays_a_google_attendee() {
    let (uid, google) = claimed_booking(false).await;
    let mut expected = guests();
    expected.push(json!({"email": "Carol.Watch@google.test"}));
    assert_eq!(google.attendees("alice@google.test", &uid), Some(expected));
    assert_eq!(google.attendees("carol@google.test", &uid), None);
}

/// A Meet booking whose owner (the stored organizer) is not the first
/// host: the Meet copy, written when the conference was minted, keeps its
/// guests and is not rewritten; the other member's copy carries none.
#[tokio::test]
async fn the_meet_owners_copy_is_the_one_with_attendees() {
    let pool = setup_test_db().await;
    let alice = insert_role_user(&pool, "alice@google.test", "user").await;
    let bob = insert_role_user(&pool, "bob@google.test", "user").await;
    let (_, et_id) = insert_team_with_et(
        &pool,
        "g237meet",
        "g237meet-et",
        &[(&alice, "member"), (&bob, "member")],
    )
    .await;
    sqlx::query(
        "UPDATE event_types SET scheduling_mode = 'collective', location_type = ? WHERE id = ?",
    )
    .bind(crate::google_meet::LOCATION_TYPE)
    .bind(&et_id)
    .execute(&pool)
    .await
    .unwrap();
    let google = Arc::new(FakeGoogle::default());
    let (base, server) = fake::serve(google.clone()).await;
    google_source(&pool, "src-a", &alice, &base, "alice@google.test").await;
    google_source(&pool, "src-b", &bob, &base, "bob@google.test").await;
    insert_booking(&pool, &et_id, None, start(), "confirmed").await;
    let uid = only_booking_uid(&pool, &et_id).await;
    sqlx::query("UPDATE bookings SET organizer_user_id = ?, meeting_url = ? WHERE uid = ?")
        .bind(&bob)
        .bind("https://meet.google.com/aaa-bbbb-ccc")
        .bind(&uid)
        .execute(&pool)
        .await
        .unwrap();
    // What minting the Meet left on the owner's calendar.
    google.insert("bob@google.test", &uid, "ev-meet", guests());
    assert_eq!(
        crate::google_calendar::attendee_copy_source(&pool, &uid)
            .await
            .as_deref(),
        Some("src-b")
    );

    caldav_push_booking(&pool, &KEY, &alice, &uid, &details(&uid)).await;
    server.abort();

    assert_eq!(google.attendees("bob@google.test", &uid), Some(guests()));
    assert_eq!(
        google.attendees("alice@google.test", &uid),
        Some(Vec::new())
    );
    let log = google.log();
    assert_nothing_announced(&log);
    assert!(
        !log.iter()
            .any(|e| e.starts_with("PUT /caldav/v2/bob%40google.test/")),
        "the Meet copy must not be rewritten: {log:#?}"
    );
}

/// Clearing and the PUT landed but adding the guests back failed: the
/// write-back keeps the href (a cancellation deletes the event), records
/// the failure, and its retry puts the guests back.
#[tokio::test]
async fn a_failed_attendee_add_is_retried_by_the_write_back() {
    let pool = setup_test_db().await;
    let alice = insert_role_user(&pool, "alice@google.test", "user").await;
    let et_id = insert_personal_et(&pool, &alice, "g237retry").await;
    let google = Arc::new(FakeGoogle::failing_adds(1));
    let (base, server) = fake::serve(google.clone()).await;
    google_source(&pool, "src-a", &alice, &base, "alice@google.test").await;
    insert_booking(&pool, &et_id, None, start(), "confirmed").await;
    let uid = only_booking_uid(&pool, &et_id).await;

    caldav_push_booking(&pool, &KEY, &alice, &uid, &details(&uid)).await;
    let (error, href): (Option<String>, Option<String>) =
        sqlx::query_as("SELECT writeback_error, caldav_calendar_href FROM bookings WHERE uid = ?")
            .bind(&uid)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(error.is_some_and(|e| e.contains("attendees")));
    assert_eq!(
        href.as_deref(),
        Some("/caldav/v2/alice%40google.test/events")
    );
    assert_eq!(
        google.attendees("alice@google.test", &uid),
        Some(Vec::new())
    );

    let failures = super::super::writeback::push_and_record(
        &pool,
        &KEY,
        &alice,
        &uid,
        &details(&uid),
        super::super::writeback::PushKind::Retry,
    )
    .await;
    server.abort();
    assert!(failures.is_empty(), "{failures:?}");
    let error: Option<String> =
        sqlx::query_scalar("SELECT writeback_error FROM bookings WHERE uid = ?")
            .bind(&uid)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(error, None);
    assert_eq!(google.attendees("alice@google.test", &uid), Some(guests()));
}

/// A Meet event that minting left without its guests gets them through
/// the Calendar API on the next push, without the CalDAV rewrite that
/// would strip its conference; while that fails, the write-back retries.
#[tokio::test]
async fn a_meet_copy_without_its_guests_gets_them_without_a_rewrite() {
    let pool = setup_test_db().await;
    let alice = insert_role_user(&pool, "alice@google.test", "user").await;
    let et_id = insert_personal_et(&pool, &alice, "g237meetfix").await;
    sqlx::query("UPDATE event_types SET location_type = ? WHERE id = ?")
        .bind(crate::google_meet::LOCATION_TYPE)
        .bind(&et_id)
        .execute(&pool)
        .await
        .unwrap();
    let google = Arc::new(FakeGoogle::failing_adds(1));
    let (base, server) = fake::serve(google.clone()).await;
    google_source(&pool, "src-a", &alice, &base, "alice@google.test").await;
    insert_booking(&pool, &et_id, None, start(), "confirmed").await;
    let uid = only_booking_uid(&pool, &et_id).await;
    sqlx::query("UPDATE bookings SET meeting_url = ? WHERE uid = ?")
        .bind("https://meet.google.com/aaa-bbbb-ccc")
        .bind(&uid)
        .execute(&pool)
        .await
        .unwrap();
    google.insert("alice@google.test", &uid, "ev-meet", Vec::new());

    caldav_push_booking(&pool, &KEY, &alice, &uid, &details(&uid)).await;
    let error: Option<String> =
        sqlx::query_scalar("SELECT writeback_error FROM bookings WHERE uid = ?")
            .bind(&uid)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(error.is_some(), "the missing guests are a failure to retry");
    // With an href, the retry would also patch the Meet's times through
    // the real Calendar API; this test is about the guests only.
    sqlx::query("UPDATE bookings SET caldav_calendar_href = NULL WHERE uid = ?")
        .bind(&uid)
        .execute(&pool)
        .await
        .unwrap();

    let failures = super::super::writeback::push_and_record(
        &pool,
        &KEY,
        &alice,
        &uid,
        &details(&uid),
        super::super::writeback::PushKind::Retry,
    )
    .await;
    server.abort();
    assert!(failures.is_empty(), "{failures:?}");
    assert_eq!(google.attendees("alice@google.test", &uid), Some(guests()));
    let log = google.log();
    assert_nothing_announced(&log);
    assert!(!log.iter().any(|e| e.starts_with("PUT ")), "{log:#?}");
}
