// Calendar write-back failures: verify, record, retry (#162). A child of
// `web::tests`, so it shares that module's helpers.

use super::*;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum DavMode {
    /// Accept writes and keep them.
    Ok,
    /// Answer 201 to a PUT and keep nothing: the Yandex case from #162.
    Drop,
    /// Refuse writes with a 400.
    Reject,
}

/// A tiny CalDAV collection on loopback: PUT, HEAD, PROPFIND and DELETE by
/// path. Keeps each stored event's body so a test can read what the calendar
/// got.
struct MockDav {
    mode: std::sync::Mutex<DavMode>,
    stored: std::sync::Mutex<std::collections::HashMap<String, String>>,
    puts: std::sync::atomic::AtomicUsize,
    /// Answer every HEAD with 400, as iCloud does.
    head_bad_request: std::sync::atomic::AtomicBool,
}

impl MockDav {
    fn set_mode(&self, mode: DavMode) {
        *self.mode.lock().unwrap() = mode;
    }
    fn refuse_head_with_400(&self) {
        self.head_bad_request
            .store(true, std::sync::atomic::Ordering::SeqCst);
    }
    fn puts(&self) -> usize {
        self.puts.load(std::sync::atomic::Ordering::SeqCst)
    }
    fn stored(&self) -> usize {
        self.stored.lock().unwrap().len()
    }
    /// The body of the one event the collection holds.
    fn only_event(&self) -> String {
        let stored = self.stored.lock().unwrap();
        assert_eq!(stored.len(), 1, "expected one stored event");
        stored.values().next().unwrap().clone()
    }
}

/// Start a mock CalDAV server and return its base URL.
async fn spawn_mock_dav(mode: DavMode) -> (String, Arc<MockDav>) {
    use axum::http::{Method, StatusCode, Uri};
    let dav = Arc::new(MockDav {
        mode: std::sync::Mutex::new(mode),
        stored: std::sync::Mutex::new(std::collections::HashMap::new()),
        puts: std::sync::atomic::AtomicUsize::new(0),
        head_bad_request: std::sync::atomic::AtomicBool::new(false),
    });
    let state = dav.clone();
    let app = Router::new().fallback(move |method: Method, uri: Uri, body: String| {
        let dav = state.clone();
        async move {
            let path = uri.path().to_string();
            if method == Method::PUT {
                dav.puts.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                let mode = *dav.mode.lock().unwrap();
                match mode {
                    DavMode::Reject => (StatusCode::BAD_REQUEST, String::new()),
                    DavMode::Drop => (StatusCode::CREATED, String::new()),
                    DavMode::Ok => {
                        dav.stored.lock().unwrap().insert(path, body);
                        (StatusCode::CREATED, String::new())
                    }
                }
            } else if method == Method::HEAD {
                let status = if dav
                    .head_bad_request
                    .load(std::sync::atomic::Ordering::SeqCst)
                {
                    StatusCode::BAD_REQUEST
                } else if dav.stored.lock().unwrap().contains_key(&path) {
                    StatusCode::OK
                } else {
                    StatusCode::NOT_FOUND
                };
                (status, String::new())
            } else if method.as_str() == "PROPFIND" {
                // Always 207, as servers do: whether the event exists is the
                // resource's own status inside the body, not the outer one.
                let inner = if dav.stored.lock().unwrap().contains_key(&path) {
                    "<d:propstat><d:prop><d:getetag>\"1\"</d:getetag></d:prop>\
                     <d:status>HTTP/1.1 200 OK</d:status></d:propstat>"
                } else {
                    "<d:status>HTTP/1.1 404 Not Found</d:status>"
                };
                let xml = format!(
                    "<?xml version=\"1.0\"?><d:multistatus xmlns:d=\"DAV:\"><d:response>\
                     <d:href>{path}</d:href>{inner}</d:response></d:multistatus>"
                );
                (StatusCode::MULTI_STATUS, xml)
            } else if method == Method::DELETE {
                dav.stored.lock().unwrap().remove(&path);
                (StatusCode::NO_CONTENT, String::new())
            } else {
                (StatusCode::METHOD_NOT_ALLOWED, String::new())
            }
        }
    });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    (format!("http://127.0.0.1:{port}/dav/"), dav)
}

/// A host with a write source on a fresh mock server, and a confirmed
/// booking ten days out. Returns (pool, mock, host id, booking id, uid).
async fn writeback_fixture(mode: DavMode) -> (SqlitePool, Arc<MockDav>, String, String, String) {
    let pool = setup_test_db().await;
    let host = insert_role_user(&pool, "host@wb.test", "user").await;
    let et_id = insert_personal_et(&pool, &host, "wb").await;
    let (base, dav) = spawn_mock_dav(mode).await;
    seed_write_source(&pool, &host, &base).await;
    let id = insert_v1_booking(
        &pool,
        &et_id,
        "guest@wb.test",
        Utc::now() + Duration::days(10),
        30,
        "confirmed",
        &[],
    )
    .await;
    let uid = format!("uid-{id}");
    (pool, dav, host, id, uid)
}

#[allow(clippy::type_complexity)]
async fn writeback_state(
    pool: &SqlitePool,
    id: &str,
) -> (Option<String>, i64, Option<String>, Option<String>, String) {
    sqlx::query_as(
        "SELECT writeback_error, writeback_attempts, writeback_next_attempt_at, caldav_calendar_href, status \
         FROM bookings WHERE id = ?",
    )
    .bind(id)
    .fetch_one(pool)
    .await
    .unwrap()
}

/// Push the booking the way a booking handler does.
async fn push_fresh(pool: &SqlitePool, host: &str, uid: &str) {
    let (details, _) = writeback::writeback_details(pool, uid).await.unwrap();
    caldav_push_booking(pool, &[0u8; 32], host, uid, &details).await;
}

#[test]
fn retry_schedule_backs_off_and_stops() {
    let now = Utc::now();
    assert_eq!(
        writeback::next_attempt_at(1, now),
        Some(now + Duration::minutes(5))
    );
    assert_eq!(
        writeback::next_attempt_at(2, now),
        Some(now + Duration::minutes(15))
    );
    assert_eq!(
        writeback::next_attempt_at(writeback::MAX_ATTEMPTS - 1, now),
        Some(now + Duration::minutes(480))
    );
    assert_eq!(
        writeback::next_attempt_at(writeback::MAX_ATTEMPTS, now),
        None
    );
    assert_eq!(writeback::next_attempt_at(0, now), None);
}

/// The #162 repro: the server accepts the PUT, the event is not there. The
/// booking stays confirmed, the failure is recorded and a retry scheduled.
#[tokio::test]
async fn unreadable_writeback_is_recorded_and_the_booking_stays_confirmed() {
    let (pool, dav, host, id, uid) = writeback_fixture(DavMode::Drop).await;
    push_fresh(&pool, &host, &uid).await;

    let (error, attempts, next, href, status) = writeback_state(&pool, &id).await;
    assert_eq!(status, "confirmed");
    let error = error.expect("the failure must be recorded");
    assert!(error.contains("not readable back"), "{error}");
    assert!(error.contains("Work calendar"), "names the source: {error}");
    assert_eq!(attempts, 1);
    assert!(next.is_some(), "a retry is scheduled");
    assert_eq!(
        href.as_deref(),
        Some("/dav/cal/"),
        "the write was accepted, so cancellation must still try to delete it"
    );
    assert_eq!(dav.puts(), 1);
}

/// A refused write is recorded too, and does not claim an href: there is
/// nothing on the calendar for a cancellation to delete.
#[tokio::test]
async fn refused_writeback_is_recorded_without_an_href() {
    let (pool, _dav, host, id, uid) = writeback_fixture(DavMode::Reject).await;
    push_fresh(&pool, &host, &uid).await;

    let (error, attempts, _, href, _) = writeback_state(&pool, &id).await;
    assert!(error.unwrap().contains("400"));
    assert_eq!(attempts, 1);
    assert_eq!(href, None);
}

#[tokio::test]
async fn verified_writeback_records_nothing() {
    let (pool, dav, host, id, uid) = writeback_fixture(DavMode::Ok).await;
    push_fresh(&pool, &host, &uid).await;

    let (error, attempts, next, href, _) = writeback_state(&pool, &id).await;
    assert_eq!(error, None);
    assert_eq!(attempts, 0);
    assert_eq!(next, None);
    assert_eq!(href.as_deref(), Some("/dav/cal/"));
    assert_eq!(dav.stored(), 1);
}

/// iCloud stores the event but answers the verifying HEAD with 400. The
/// check falls back to PROPFIND, as for 405 and 501, instead of recording a
/// failure and rewriting the event eight times.
#[tokio::test]
async fn head_refused_with_400_is_verified_by_propfind() {
    let (pool, dav, host, id, uid) = writeback_fixture(DavMode::Ok).await;
    dav.refuse_head_with_400();
    push_fresh(&pool, &host, &uid).await;

    let (error, attempts, next, _, _) = writeback_state(&pool, &id).await;
    assert_eq!(error, None);
    assert_eq!(attempts, 0);
    assert_eq!(next, None);
    assert_eq!(dav.puts(), 1);
}

/// The other half: HEAD is refused with 400 and the server dropped the event.
/// PROPFIND answers 207 with a 404 for the event itself, and that must be read
/// as a failed write-back, not as confirmation by the outer 207.
#[tokio::test]
async fn propfind_207_reporting_a_missing_event_is_a_failure() {
    let (pool, dav, host, id, uid) = writeback_fixture(DavMode::Drop).await;
    dav.refuse_head_with_400();
    push_fresh(&pool, &host, &uid).await;

    let (error, attempts, next, _, status) = writeback_state(&pool, &id).await;
    assert_eq!(status, "confirmed");
    let error = error.expect("the failure must be recorded");
    assert!(error.contains("not readable back"), "{error}");
    assert_eq!(attempts, 1);
    assert!(next.is_some(), "a retry is scheduled");
}

/// A retry that lands clears the failure, and retrying never duplicates the
/// event: the PUT is keyed on the booking uid.
#[tokio::test]
async fn retry_success_clears_the_error_without_duplicating() {
    let (pool, dav, host, id, uid) = writeback_fixture(DavMode::Drop).await;
    push_fresh(&pool, &host, &uid).await;
    assert!(writeback_state(&pool, &id).await.0.is_some());

    // Not due yet: nothing happens.
    assert_eq!(
        writeback::retry_failed_writebacks(&pool, &[0u8; 32], Utc::now()).await,
        0
    );

    dav.set_mode(DavMode::Ok);
    let later = Utc::now() + Duration::minutes(10);
    assert_eq!(
        writeback::retry_failed_writebacks(&pool, &[0u8; 32], later).await,
        1
    );
    let (error, attempts, next, _, status) = writeback_state(&pool, &id).await;
    assert_eq!(error, None);
    assert_eq!(attempts, 0);
    assert_eq!(next, None);
    assert_eq!(status, "confirmed");
    assert_eq!(dav.stored(), 1);

    // Nothing left to retry, and a second push replaces the same event.
    assert_eq!(
        writeback::retry_failed_writebacks(&pool, &[0u8; 32], later).await,
        0
    );
    push_fresh(&pool, &host, &uid).await;
    assert_eq!(dav.stored(), 1, "a re-push must not add a second event");
}

/// #266: the retry rebuilt the event without the guest phone, so a booking
/// whose first push failed reached the host's calendar with the notes and
/// no phone once a retry landed.
#[tokio::test]
async fn a_retried_writeback_keeps_the_guest_phone_and_notes() {
    let (pool, dav, host, id, uid) = writeback_fixture(DavMode::Reject).await;
    sqlx::query(
        "UPDATE event_types SET sms_phone_mode = 'optional' \
         WHERE id = (SELECT event_type_id FROM bookings WHERE id = ?)",
    )
    .bind(&id)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "UPDATE bookings SET guest_phone = '+33612345678', notes = 'Bring the quote' WHERE id = ?",
    )
    .bind(&id)
    .execute(&pool)
    .await
    .unwrap();
    push_fresh(&pool, &host, &uid).await;
    assert_eq!(dav.stored(), 0);

    dav.set_mode(DavMode::Ok);
    let later = Utc::now() + Duration::minutes(10);
    assert_eq!(
        writeback::retry_failed_writebacks(&pool, &[0u8; 32], later).await,
        1
    );
    let event = dav.only_event();
    assert!(event.contains("+33612345678"), "{event}");
    assert!(event.contains("Bring the quote"), "{event}");
}

/// #266: a claim re-pushed the host's event with no `host_language`, so the
/// phone label fell back to English, and listed the claimant before the
/// attendees the guest invited, the reverse of every later push.
#[tokio::test]
async fn a_claim_keeps_the_host_language_and_the_attendee_order() {
    let _env = crate::test_support::ENV_LOCK.lock().await;
    let (app, pool, _, et_id) = setup_test_app().await;
    let host: String = sqlx::query_scalar("SELECT id FROM users WHERE email = 'test@example.com'")
        .fetch_one(&pool)
        .await
        .unwrap();
    set_user_language(&pool, "test@example.com", "fr").await;
    let (base, dav) = spawn_mock_dav(DavMode::Ok).await;
    seed_write_source(&pool, &host, &base).await;
    sqlx::query("UPDATE event_types SET sms_phone_mode = 'optional' WHERE id = ?")
        .bind(&et_id)
        .execute(&pool)
        .await
        .unwrap();
    let id = insert_v1_booking(
        &pool,
        &et_id,
        "guest@test.com",
        Utc::now() + Duration::days(10),
        30,
        "confirmed",
        &[("guest_phone", Some("+33612345678".to_string()))],
    )
    .await;
    sqlx::query(
        "INSERT INTO booking_attendees (id, booking_id, email, created_at) \
         VALUES ('att-266', ?, 'friend@test.com', '2026-01-01 00:00:00')",
    )
    .bind(&id)
    .execute(&pool)
    .await
    .unwrap();
    let watcher = insert_role_user(&pool, "watcher@test.com", "user").await;
    sqlx::query(
        "INSERT INTO booking_claim_tokens (id, booking_id, user_id, token, expires_at) \
         VALUES ('bct-266', ?, ?, 'claim-tok-266', datetime('now', '+7 days'))",
    )
    .bind(&id)
    .bind(&watcher)
    .execute(&pool)
    .await
    .unwrap();

    let csrf = "test-csrf-266-claim";
    let response = app
        .oneshot(post_form_unauthed(
            &format!("/booking/claim/{id}"),
            csrf,
            &format!("_csrf={csrf}&token=claim-tok-266"),
        ))
        .await
        .unwrap();
    assert!(response.status().is_success(), "{}", response.status());

    let event = dav.only_event();
    assert!(event.contains("Téléphone : +33612345678"), "{event}");
    let friend = event.find("mailto:friend@test.com").expect("friend listed");
    let claimant = event
        .find("mailto:watcher@test.com")
        .expect("claimant listed");
    assert!(friend < claimant, "{event}");
}

/// A booking cancelled (or declined) after its write-back failed is never
/// pushed again: retrying would put a cancelled meeting on the calendar.
#[tokio::test]
async fn cancelled_and_past_bookings_are_not_retried() {
    let (pool, dav, host, id, uid) = writeback_fixture(DavMode::Reject).await;
    push_fresh(&pool, &host, &uid).await;
    sqlx::query("UPDATE bookings SET status = 'cancelled' WHERE id = ?")
        .bind(&id)
        .execute(&pool)
        .await
        .unwrap();
    dav.set_mode(DavMode::Ok);
    let later = Utc::now() + Duration::hours(1);
    assert_eq!(
        writeback::retry_failed_writebacks(&pool, &[0u8; 32], later).await,
        0
    );
    assert_eq!(dav.puts(), 1, "only the original push");

    // Same for a confirmed booking whose start has passed.
    sqlx::query("UPDATE bookings SET status = 'confirmed' WHERE id = ?")
        .bind(&id)
        .execute(&pool)
        .await
        .unwrap();
    let after_start = Utc::now() + Duration::days(11);
    assert_eq!(
        writeback::retry_failed_writebacks(&pool, &[0u8; 32], after_start).await,
        0
    );
    assert_eq!(dav.puts(), 1);
}

/// Retries continue the attempt count and stop once it is used up.
#[tokio::test]
async fn retries_stop_after_the_last_attempt() {
    let (pool, dav, host, id, uid) = writeback_fixture(DavMode::Reject).await;
    push_fresh(&pool, &host, &uid).await;
    for _ in 1..writeback::MAX_ATTEMPTS {
        let later = Utc::now() + Duration::days(1);
        assert_eq!(
            writeback::retry_failed_writebacks(&pool, &[0u8; 32], later).await,
            1
        );
    }
    let (error, attempts, next, _, status) = writeback_state(&pool, &id).await;
    assert!(error.is_some());
    assert_eq!(attempts, writeback::MAX_ATTEMPTS);
    assert_eq!(next, None, "no retry after the last attempt");
    assert_eq!(status, "confirmed");
    assert_eq!(
        writeback::retry_failed_writebacks(&pool, &[0u8; 32], Utc::now() + Duration::days(2)).await,
        0
    );
    assert_eq!(dav.puts() as i64, writeback::MAX_ATTEMPTS);
}

/// Exhausting the retries emails the host exactly once, in the host's own
/// language, and later failed retries do not email again.
#[tokio::test]
async fn persistent_writeback_failure_emails_the_host_once() {
    let _env = crate::test_support::ENV_LOCK.lock().await;
    let _guard = crate::test_support::EnvGuard::new();
    let (pool, _dav, host, id, uid) = writeback_fixture(DavMode::Drop).await;
    sqlx::query("UPDATE users SET language = 'fr', name = 'Hélène Hôte' WHERE id = ?")
        .bind(&host)
        .execute(&pool)
        .await
        .unwrap();
    let (port, mails) = spawn_capturing_mta().await;
    use_capturing_mta(&pool, port).await;

    push_fresh(&pool, &host, &uid).await;
    for _ in 1..writeback::MAX_ATTEMPTS {
        let later = Utc::now() + Duration::days(1);
        writeback::retry_failed_writebacks(&pool, &[0u8; 32], later).await;
    }
    let (_, attempts, next, _, _) = writeback_state(&pool, &id).await;
    assert_eq!(attempts, writeback::MAX_ATTEMPTS);
    assert_eq!(next, None);

    // Inline sends: every message has been delivered by now.
    let body = mail_to(&mails, "host@wb.test").await;
    assert!(body.contains("Une réservation manque"), "in French: {body}");
    assert!(
        body.contains("Work calendar"),
        "names the failing source: {body}"
    );
    let alerted: Option<String> =
        sqlx::query_scalar("SELECT writeback_alerted_at FROM bookings WHERE id = ?")
            .bind(&id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(alerted.is_some());
    assert_eq!(
        mails.lock().unwrap().len(),
        1,
        "one email for the whole failure, not one per retry"
    );
}

/// Before the alert threshold nobody is emailed: a brief outage that a
/// retry fixes stays quiet.
#[tokio::test]
async fn a_writeback_fixed_by_a_retry_sends_no_email() {
    let _env = crate::test_support::ENV_LOCK.lock().await;
    let _guard = crate::test_support::EnvGuard::new();
    let (pool, dav, host, _id, uid) = writeback_fixture(DavMode::Reject).await;
    let (port, mails) = spawn_capturing_mta().await;
    use_capturing_mta(&pool, port).await;

    push_fresh(&pool, &host, &uid).await;
    writeback::retry_failed_writebacks(&pool, &[0u8; 32], Utc::now() + Duration::hours(1)).await;
    dav.set_mode(DavMode::Ok);
    writeback::retry_failed_writebacks(&pool, &[0u8; 32], Utc::now() + Duration::hours(2)).await;
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    assert!(mails.lock().unwrap().is_empty());
}

/// The bookings dashboard flags a booking whose write-back is failing, with
/// the error.
#[tokio::test]
async fn bookings_dashboard_flags_failed_writebacks() {
    let (app, pool, session, et_id) = setup_test_app().await;
    let id = insert_v1_booking(
        &pool,
        &et_id,
        "flagged@wb.test",
        Utc::now() + Duration::days(3),
        30,
        "confirmed",
        &[(
            "writeback_error",
            Some("Work calendar: PUT returned 400 Bad Request".into()),
        )],
    )
    .await;
    insert_v1_booking(
        &pool,
        &et_id,
        "fine@wb.test",
        Utc::now() + Duration::days(4),
        30,
        "confirmed",
        &[],
    )
    .await;

    let response = app
        .oneshot(get_authed("/dashboard/bookings", &session))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = body_string(response).await;
    assert_eq!(body.matches("not in your calendar").count(), 1, "{body}");
    assert!(body.contains("PUT returned 400 Bad Request"), "{body}");
    assert!(!id.is_empty());
}

// --- Calendar source sync failures (#162) ---

/// A host whose only source points at a closed loopback port, so every sync
/// fails fast with a connection error. Returns (pool, host id, source id).
async fn failing_source_fixture() -> (SqlitePool, String, String) {
    let pool = setup_test_db().await;
    let host = insert_role_user(&pool, "owner@sync.test", "user").await;
    sqlx::query("UPDATE users SET name = 'Olga Owner', language = 'de' WHERE id = ?")
        .bind(&host)
        .execute(&pool)
        .await
        .unwrap();
    insert_personal_et(&pool, &host, "sync").await;
    let source_id = seed_write_source(&pool, &host, "http://127.0.0.1:1/dav/").await;
    (pool, host, source_id)
}

async fn sync_once(pool: &SqlitePool, source_id: &str) -> anyhow::Result<()> {
    let (url, password_enc): (String, Option<String>) =
        sqlx::query_as("SELECT url, password_enc FROM caldav_sources WHERE id = ?")
            .bind(source_id)
            .fetch_one(pool)
            .await
            .unwrap();
    crate::commands::sync::sync_one_source(
        pool,
        &[0u8; 32],
        source_id,
        &url,
        "u",
        password_enc.as_deref(),
        "basic",
        None,
        None,
        "caldav",
    )
    .await
}

async fn source_health(
    pool: &SqlitePool,
    source_id: &str,
) -> (Option<String>, i64, Option<String>) {
    sqlx::query_as(
        "SELECT last_sync_error, sync_failure_count, sync_alerted_at FROM caldav_sources WHERE id = ?",
    )
    .bind(source_id)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn backdate_failing_since(pool: &SqlitePool, source_id: &str) {
    sqlx::query(
        "UPDATE caldav_sources SET sync_failing_since = datetime('now', '-2 hours') WHERE id = ?",
    )
    .bind(source_id)
    .execute(pool)
    .await
    .unwrap();
}

/// Mails to `email` once the background sends settle.
async fn settled_mail_count(mails: &std::sync::Mutex<Vec<CapturedMail>>, email: &str) -> usize {
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    mails
        .lock()
        .unwrap()
        .iter()
        .filter(|m| m.is_to(email))
        .count()
}

/// Failures are recorded; the owner is emailed once the failure has lasted
/// an hour over three attempts, not before and not twice; a recovery clears
/// everything and re-arms the alert.
#[tokio::test]
async fn source_sync_failure_is_recorded_alerted_once_and_cleared() {
    let _env = crate::test_support::ENV_LOCK.lock().await;
    let _guard = crate::test_support::EnvGuard::new();
    let (pool, _host, source_id) = failing_source_fixture().await;
    let (port, mails) = spawn_capturing_mta().await;
    use_capturing_mta(&pool, port).await;

    for _ in 0..3 {
        assert!(sync_once(&pool, &source_id).await.is_err());
    }
    let (error, count, alerted) = source_health(&pool, &source_id).await;
    assert!(error.is_some());
    assert_eq!(count, 3);
    assert_eq!(alerted, None, "three quick failures are not an hour yet");
    assert_eq!(settled_mail_count(&mails, "owner@sync.test").await, 0);

    backdate_failing_since(&pool, &source_id).await;
    assert!(sync_once(&pool, &source_id).await.is_err());
    let body = mail_to(&mails, "owner@sync.test").await;
    assert!(
        body.contains("Eine Kalenderquelle synchronisiert nicht mehr"),
        "in the owner's language: {body}"
    );
    assert!(body.contains("Work calendar"), "{body}");
    assert!(sync_once(&pool, &source_id).await.is_err());
    assert_eq!(
        settled_mail_count(&mails, "owner@sync.test").await,
        1,
        "one email per failure run"
    );

    // Recovery clears the record and re-arms the alert.
    crate::commands::sync_health::record_sync_outcome(&pool, &[0u8; 32], &source_id, &Ok(())).await;
    assert_eq!(source_health(&pool, &source_id).await, (None, 0, None));

    // Failing again for long enough alerts again.
    for _ in 0..3 {
        assert!(sync_once(&pool, &source_id).await.is_err());
    }
    backdate_failing_since(&pool, &source_id).await;
    assert!(sync_once(&pool, &source_id).await.is_err());
    assert_eq!(settled_mail_count(&mails, "owner@sync.test").await, 2);
}

/// On-demand syncs (slot pages) do not retry a source that failed within
/// the last five minutes, so a dead server does not slow every page load.
#[tokio::test]
async fn on_demand_sync_backs_off_after_a_failure() {
    let (pool, host, source_id) = failing_source_fixture().await;
    crate::commands::sync::sync_if_stale(&pool, &[0u8; 32], &host).await;
    assert_eq!(source_health(&pool, &source_id).await.1, 1);
    crate::commands::sync::sync_if_stale(&pool, &[0u8; 32], &host).await;
    assert_eq!(
        source_health(&pool, &source_id).await.1,
        1,
        "not retried within the stale window"
    );
}

/// `calrs sync` used to print "Sync complete." after a failure and exit 0.
#[tokio::test]
async fn cli_sync_fails_when_a_source_fails() {
    let (pool, _host, source_id) = failing_source_fixture().await;
    let err = crate::commands::sync::run(&pool, &[0u8; 32], false)
        .await
        .expect_err("a failed source must fail the command");
    assert!(err.to_string().contains("1 of 1 source(s) failed"), "{err}");
    assert!(source_health(&pool, &source_id).await.0.is_some());
}

/// The calendar sources page shows a failing source's error.
#[tokio::test]
async fn sources_dashboard_shows_the_sync_error() {
    let (app, pool, session, _et_id) = setup_test_app().await;
    let user_id: String = sqlx::query_scalar("SELECT id FROM users WHERE username = 'testuser'")
        .fetch_one(&pool)
        .await
        .unwrap();
    let source_id = seed_write_source(&pool, &user_id, "http://127.0.0.1:1/dav/").await;
    sqlx::query(
        "UPDATE caldav_sources SET last_sync_error = 'PROPFIND returned 401 Unauthorized', \
         sync_failing_since = '2026-10-11 08:00:00', sync_failure_count = 4 WHERE id = ?",
    )
    .bind(&source_id)
    .execute(&pool)
    .await
    .unwrap();

    let response = app
        .oneshot(get_authed("/dashboard/sources", &session))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = body_string(response).await;
    assert!(
        body.contains("Sync failing since 2026-10-11 08:00:00 UTC"),
        "{body}"
    );
    assert!(
        body.contains("PROPFIND returned 401 Unauthorized"),
        "{body}"
    );
}

/// How [`claimed_fixture_with`] sets a claim up.
struct ClaimSetup {
    /// The host has a write calendar.
    host_writes: bool,
    /// How the claimant's server answers writes.
    claimant_mode: DavMode,
    /// The claimant writes to the host's own collection (a shared calendar).
    shared_collection: bool,
    /// The booking's status when the claim is posted.
    status: &'static str,
}

impl Default for ClaimSetup {
    fn default() -> Self {
        ClaimSetup {
            host_writes: true,
            claimant_mode: DavMode::Ok,
            shared_collection: false,
            status: "confirmed",
        }
    }
}

/// What [`claimed_fixture_with`] returns.
struct Claimed {
    app: Router,
    pool: SqlitePool,
    host: String,
    id: String,
    uid: String,
    host_dav: Arc<MockDav>,
    claimant_dav: Arc<MockDav>,
    claim_body: String,
}

/// A booking with a phone number claimed by a watcher, each with a write
/// calendar on their own server unless `shared_collection` (#269). The
/// claimant's write calendar has its own href, so a copy recorded in the
/// host's place would show.
async fn claimed_fixture_with(setup: ClaimSetup) -> Claimed {
    let (app, pool, _, et_id) = setup_test_app().await;
    let host: String = sqlx::query_scalar("SELECT id FROM users WHERE email = 'test@example.com'")
        .fetch_one(&pool)
        .await
        .unwrap();
    let (host_base, host_dav) = spawn_mock_dav(DavMode::Ok).await;
    if setup.host_writes {
        seed_write_source(&pool, &host, &host_base).await;
    }
    sqlx::query("UPDATE event_types SET sms_phone_mode = 'optional' WHERE id = ?")
        .bind(&et_id)
        .execute(&pool)
        .await
        .unwrap();
    let id = insert_v1_booking(
        &pool,
        &et_id,
        "guest@test.com",
        Utc::now() + Duration::days(10),
        30,
        setup.status,
        &[("guest_phone", Some("+33612345678".to_string()))],
    )
    .await;
    let watcher = insert_role_user(&pool, "watcher@test.com", "user").await;
    sqlx::query(
        "INSERT INTO accounts (id, name, email, timezone, user_id) \
         VALUES ('acc-watcher-269', 'Watcher', 'watcher@test.com', 'UTC', ?)",
    )
    .bind(&watcher)
    .execute(&pool)
    .await
    .unwrap();
    let (claimant_dav, claimant_base) = if setup.shared_collection {
        (host_dav.clone(), host_base)
    } else {
        let (base, dav) = spawn_mock_dav(setup.claimant_mode).await;
        (dav, base)
    };
    let source = seed_write_source(&pool, &watcher, &claimant_base).await;
    if !setup.shared_collection {
        sqlx::query(
            "UPDATE caldav_sources SET write_calendar_href = '/dav/claimant/' WHERE id = ?",
        )
        .bind(&source)
        .execute(&pool)
        .await
        .unwrap();
    }
    sqlx::query(
        "INSERT INTO booking_claim_tokens (id, booking_id, user_id, token, expires_at) \
         VALUES ('bct-269', ?, ?, 'claim-tok-269', datetime('now', '+7 days'))",
    )
    .bind(&id)
    .bind(&watcher)
    .execute(&pool)
    .await
    .unwrap();
    let csrf = "test-csrf-269-claim";
    let response = app
        .clone()
        .oneshot(post_form_unauthed(
            &format!("/booking/claim/{id}"),
            csrf,
            &format!("_csrf={csrf}&token=claim-tok-269"),
        ))
        .await
        .unwrap();
    assert!(response.status().is_success(), "{}", response.status());
    let claim_body = body_string(response).await;
    let uid = format!("uid-{id}");
    Claimed {
        app,
        pool,
        host,
        id,
        uid,
        host_dav,
        claimant_dav,
        claim_body,
    }
}

/// [`claimed_fixture_with`] for a confirmed booking, the host writing to a
/// calendar of their own or not. Returns (pool, host id, booking id, uid,
/// host server, claimant server).
async fn claimed_fixture(
    host_writes: bool,
) -> (
    SqlitePool,
    String,
    String,
    String,
    Arc<MockDav>,
    Arc<MockDav>,
) {
    let c = claimed_fixture_with(ClaimSetup {
        host_writes,
        ..Default::default()
    })
    .await;
    (c.pool, c.host, c.id, c.uid, c.host_dav, c.claimant_dav)
}

/// #269: the claim writes the claimant's copy, without the guest's phone,
/// and leaves the host's href alone so the host's delete still finds it.
#[tokio::test]
async fn a_claim_writes_the_claimants_copy_and_keeps_the_hosts_href() {
    let (pool, _, id, _, host_dav, claimant_dav) = claimed_fixture(true).await;
    assert!(host_dav.only_event().contains("+33612345678"));
    let copy = claimant_dav.only_event();
    assert!(copy.contains("mailto:watcher@test.com"), "{copy}");
    assert!(!copy.contains("+33612345678"), "{copy}");
    let (_, _, _, href, _) = writeback_state(&pool, &id).await;
    assert_eq!(href.as_deref(), Some("/dav/cal/"));
}

/// Cancelling a claimed booking removes the claimant's copy with the host's.
#[tokio::test]
async fn cancelling_a_claimed_booking_deletes_the_claimants_copy() {
    let (pool, host, _, uid, host_dav, claimant_dav) = claimed_fixture(true).await;
    caldav_delete_booking(&pool, &[0u8; 32], &host, &uid).await;
    assert_eq!(host_dav.stored(), 0);
    assert_eq!(claimant_dav.stored(), 0);
}

/// The host may have no write calendar at all: the claimant's copy is still
/// deleted, although the booking records no href of its own.
#[tokio::test]
async fn the_claimants_copy_is_deleted_when_the_host_has_no_write_calendar() {
    let (pool, host, id, uid, _, claimant_dav) = claimed_fixture(false).await;
    assert_eq!(claimant_dav.stored(), 1);
    let (_, _, _, href, _) = writeback_state(&pool, &id).await;
    assert_eq!(href, None);
    caldav_delete_booking(&pool, &[0u8; 32], &host, &uid).await;
    assert_eq!(claimant_dav.stored(), 0);
}

/// A reschedule deletes the booking from the calendars and pushes it at its
/// new time: the claimant's copy moves with the host's.
#[tokio::test]
async fn a_rescheduled_claimed_booking_moves_the_claimants_copy() {
    let (pool, host, id, uid, host_dav, claimant_dav) = claimed_fixture(true).await;
    caldav_delete_booking(&pool, &[0u8; 32], &host, &uid).await;
    assert_eq!(claimant_dav.stored(), 0);
    let start = Utc::now() + Duration::days(12);
    sqlx::query("UPDATE bookings SET start_at = ?, end_at = ? WHERE id = ?")
        .bind(utc_stamp(start))
        .bind(utc_stamp(start + Duration::minutes(30)))
        .bind(&id)
        .execute(&pool)
        .await
        .unwrap();
    push_fresh(&pool, &host, &uid).await;
    let stamp = format!("DTSTART:{}", start.format("%Y%m%dT%H%M%SZ"));
    assert!(host_dav.only_event().contains(&stamp));
    let copy = claimant_dav.only_event();
    assert!(copy.contains(&stamp), "{copy}");
    assert!(!copy.contains("+33612345678"), "{copy}");
}

/// A pending guest reschedule deletes the claimant's copy even when the host
/// has no write calendar, so a decline or expiry leaves nothing behind.
#[tokio::test]
async fn a_pending_reschedule_deletes_the_claimants_copy_without_a_host_calendar() {
    let c = claimed_fixture_with(ClaimSetup {
        host_writes: false,
        ..Default::default()
    })
    .await;
    assert_eq!(c.claimant_dav.stored(), 1);
    sqlx::query(
        "UPDATE event_types SET requires_confirmation = 1 \
         WHERE id = (SELECT event_type_id FROM bookings WHERE id = ?)",
    )
    .bind(&c.id)
    .execute(&c.pool)
    .await
    .unwrap();
    let token: String = sqlx::query_scalar("SELECT reschedule_token FROM bookings WHERE id = ?")
        .bind(&c.id)
        .fetch_one(&c.pool)
        .await
        .unwrap();
    let new_date = (Local::now().naive_local().date() + Duration::days(12))
        .format("%Y-%m-%d")
        .to_string();
    let csrf = "csrf-269-resched";
    let response = c
        .app
        .oneshot(post_form_unauthed(
            &format!("/booking/reschedule/{token}"),
            csrf,
            &format!("_csrf={csrf}&date={new_date}&time=10%3A00&tz=UTC"),
        ))
        .await
        .unwrap();
    assert!(response.status().is_success(), "{}", response.status());
    let (_, _, _, _, status) = writeback_state(&c.pool, &c.id).await;
    assert_eq!(status, "pending");
    assert_eq!(c.claimant_dav.stored(), 0);
}

/// A claimant whose copy cannot be written leaves the host's write-back
/// bookkeeping alone: the retries and the alert are about the host.
#[tokio::test]
async fn a_failed_claimant_copy_is_not_recorded_as_a_host_failure() {
    let c = claimed_fixture_with(ClaimSetup {
        claimant_mode: DavMode::Reject,
        ..Default::default()
    })
    .await;
    assert_eq!(c.claimant_dav.puts(), 1);
    assert_eq!(c.claimant_dav.stored(), 0);
    let (error, attempts, next, href, _) = writeback_state(&c.pool, &c.id).await;
    assert_eq!(error, None);
    assert_eq!(attempts, 0);
    assert_eq!(next, None);
    assert_eq!(href.as_deref(), Some("/dav/cal/"));
}

/// A claimant writing to the host's own collection gets no second PUT, which
/// would replace the host's event with the copy that leaves the phone out.
#[tokio::test]
async fn a_claimant_sharing_the_hosts_calendar_does_not_overwrite_it() {
    let c = claimed_fixture_with(ClaimSetup {
        shared_collection: true,
        ..Default::default()
    })
    .await;
    assert_eq!(c.host_dav.puts(), 1);
    assert!(c.host_dav.only_event().contains("+33612345678"));
}

/// A claim link outlives the booking it was sent for: a cancelled booking
/// can no longer be claimed, and nothing is written for it.
#[tokio::test]
async fn a_cancelled_booking_cannot_be_claimed() {
    let c = claimed_fixture_with(ClaimSetup {
        status: "cancelled",
        ..Default::default()
    })
    .await;
    assert!(c.claim_body.contains("no longer valid"), "{}", c.claim_body);
    let claimed: Option<String> =
        sqlx::query_scalar("SELECT claimed_by_user_id FROM bookings WHERE id = ?")
            .bind(&c.id)
            .fetch_one(&c.pool)
            .await
            .unwrap();
    assert_eq!(claimed, None);
    assert_eq!(c.claimant_dav.puts(), 0);
    assert_eq!(c.host_dav.puts(), 0);
}

/// A fresh push that lands after the booking was cancelled (a claim racing
/// the cancellation) deletes what it just wrote, host's and claimant's.
#[tokio::test]
async fn a_push_finishing_after_a_cancellation_cleans_up_after_itself() {
    let c = claimed_fixture_with(ClaimSetup::default()).await;
    sqlx::query("UPDATE bookings SET status = 'cancelled' WHERE id = ?")
        .bind(&c.id)
        .execute(&c.pool)
        .await
        .unwrap();
    push_fresh(&c.pool, &c.host, &c.uid).await;
    assert_eq!(c.host_dav.stored(), 0);
    assert_eq!(c.claimant_dav.stored(), 0);
}

/// A claimant with one collection shared with the host and one of their
/// own still gets their copy in the one of their own.
#[tokio::test]
async fn only_the_shared_collection_is_skipped_for_the_claimant() {
    let c = claimed_fixture_with(ClaimSetup {
        shared_collection: true,
        ..Default::default()
    })
    .await;
    let watcher: String =
        sqlx::query_scalar("SELECT id FROM users WHERE email = 'watcher@test.com'")
            .fetch_one(&c.pool)
            .await
            .unwrap();
    let (own_base, own_dav) = spawn_mock_dav(DavMode::Ok).await;
    seed_write_source(&c.pool, &watcher, &own_base).await;
    push_fresh(&c.pool, &c.host, &c.uid).await;
    assert!(c.host_dav.only_event().contains("+33612345678"));
    assert!(!own_dav.only_event().contains("+33612345678"));
}
