// Pending requests hold their slot, expire, and are re-checked on approval
// (#234). A child of `web::tests`, so it shares that module's helpers.

use super::*;

fn utc(y: i32, m: u32, d: u32, h: u32, min: u32) -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(y, m, d, h, min, 0).unwrap()
}

async fn second_event_type(pool: &SqlitePool, account_id: &str) -> String {
    let id = uuid::Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO event_types (id, account_id, slug, title, duration_min, buffer_before, buffer_after, min_notice_min, enabled) VALUES (?, ?, 'other-meeting', 'Other Meeting', 30, 0, 0, 0, 1)")
        .bind(&id)
        .bind(account_id)
        .execute(pool)
        .await
        .unwrap();
    id
}

#[test]
fn pending_expiry_deadline_uses_the_expiry_window() {
    let now = utc(2030, 1, 1, 0, 0);
    let start = utc(2030, 1, 10, 10, 0);
    assert_eq!(
        pending_expiry_deadline(now, start, Some(24), 60),
        Some(utc(2030, 1, 2, 0, 0))
    );
}

#[test]
fn pending_expiry_deadline_is_capped_by_the_minimum_notice() {
    let now = utc(2030, 1, 1, 0, 0);
    let start = utc(2030, 1, 1, 10, 0);
    // 24h from now is past the meeting; the latest bookable moment wins.
    assert_eq!(
        pending_expiry_deadline(now, start, Some(24), 60),
        Some(utc(2030, 1, 1, 9, 0))
    );
    // Without a minimum notice the cap is the start itself.
    assert_eq!(
        pending_expiry_deadline(now, start, Some(24), 0),
        Some(start)
    );
}

#[test]
fn pending_expiry_deadline_inside_the_notice_window_waits_for_the_start() {
    // Booked at the edge of a 2h minimum notice: start - notice is already
    // behind us, so expiring early would free a slot nobody can book.
    let now = utc(2030, 1, 1, 8, 0);
    let start = utc(2030, 1, 1, 9, 50);
    assert_eq!(
        pending_expiry_deadline(now, start, Some(24), 120),
        Some(start),
        "capped by the start, not by a moment in the past"
    );
    // A short expiry still applies when it comes first.
    assert_eq!(
        pending_expiry_deadline(now, start, Some(1), 120),
        Some(utc(2030, 1, 1, 9, 0))
    );
    // Exactly at the release point counts as inside the window.
    let start = utc(2030, 1, 1, 10, 0);
    assert_eq!(
        pending_expiry_deadline(now, start, Some(24), 120),
        Some(start)
    );
}

#[test]
fn pending_expiry_deadline_is_never_in_the_past() {
    let now = utc(2030, 1, 1, 12, 0);
    let start = utc(2030, 1, 1, 11, 0);
    assert_eq!(pending_expiry_deadline(now, start, Some(24), 60), Some(now));
}

#[test]
fn pending_expiry_deadline_is_none_when_requests_never_expire() {
    let now = utc(2030, 1, 1, 0, 0);
    let start = utc(2030, 1, 10, 10, 0);
    assert_eq!(pending_expiry_deadline(now, start, None, 60), None);
    assert_eq!(pending_expiry_deadline(now, start, Some(0), 60), None);
}

#[test]
fn pending_expiry_select_values_round_trip() {
    for stored in [None, Some(4), Some(24), Some(12)] {
        assert_eq!(
            PendingSettings::parse_expiry(&PendingSettings::expiry_value(stored)),
            stored
        );
    }
    assert_eq!(PendingSettings::parse_expiry("bogus"), None);
    assert_eq!(PendingSettings::parse_expiry("0"), None);
    assert_eq!(PendingSettings::parse_expiry("-3"), None);
    assert_eq!(PendingSettings::parse_expiry("99999"), None);
}

/// The options come from `PENDING_EXPIRY_CHOICES`, and a stored value that
/// is not one of them is offered too so it stays selected on save (#234
/// review): before, the template hard-coded the list and such a value fell
/// back to "never".
#[test]
fn pending_settings_context_keeps_an_unlisted_stored_expiry() {
    let ctx = PendingSettings {
        blocks_slot: false,
        expiry_hours: Some(12),
    }
    .context();
    let values: Vec<String> = ctx
        .get_attr("options")
        .unwrap()
        .try_iter()
        .unwrap()
        .map(|o| o.get_attr("value").unwrap().to_string())
        .collect();
    let mut expected = vec!["never".to_string()];
    let mut hours = PENDING_EXPIRY_CHOICES.to_vec();
    hours.push(12);
    hours.sort_unstable();
    expected.extend(hours.iter().map(|h| h.to_string()));
    assert_eq!(values, expected);
    assert_eq!(ctx.get_attr("selected").unwrap().to_string(), "12");
    assert!(!ctx.get_attr("blocks_slot").unwrap().is_true());
}

#[tokio::test]
async fn pending_booking_blocks_its_slot_and_overlapping_event_types() {
    let pool = setup_test_db().await;
    let (user_id, account_id, et_id) = seed_test_data(&pool).await;
    let other_et = second_event_type(&pool, &account_id).await;
    sqlx::query("UPDATE event_types SET requires_confirmation = 1 WHERE id = ?")
        .bind(&et_id)
        .execute(&pool)
        .await
        .unwrap();
    // Monday 2030-03-18, 10:00-10:30 UTC.
    insert_v1_booking(
        &pool,
        &et_id,
        "first@test.com",
        utc(2030, 3, 18, 10, 0),
        30,
        "pending",
        &[],
    )
    .await;

    let busy_for = |et: String| {
        let pool = pool.clone();
        let user_id = user_id.clone();
        async move {
            fetch_busy_times_for_user(
                &pool,
                &user_id,
                dt(2030, 3, 18, 0, 0),
                dt(2030, 3, 18, 23, 59),
                Tz::UTC,
                Some(&et),
            )
            .await
        }
    };

    // Same event type: the exact slot is taken.
    let busy = busy_for(et_id.clone()).await;
    assert!(has_conflict(
        &busy,
        dt(2030, 3, 18, 10, 0),
        dt(2030, 3, 18, 10, 30)
    ));
    // Another event type of the same host: an overlapping slot is taken too.
    let busy = busy_for(other_et.clone()).await;
    assert!(has_conflict(
        &busy,
        dt(2030, 3, 18, 10, 15),
        dt(2030, 3, 18, 10, 45)
    ));
    assert!(!has_conflict(
        &busy,
        dt(2030, 3, 18, 10, 30),
        dt(2030, 3, 18, 11, 0)
    ));

    // Opting out on the pending booking's own event type frees the time,
    // whichever event type is asking.
    sqlx::query("UPDATE event_types SET pending_blocks_slot = 0 WHERE id = ?")
        .bind(&et_id)
        .execute(&pool)
        .await
        .unwrap();
    assert!(busy_for(et_id.clone()).await.is_empty());
    assert!(busy_for(other_et).await.is_empty());
}

#[tokio::test]
async fn troubleshoot_labels_a_held_pending_request() {
    let (app, pool, session, et_id) = setup_test_app().await;
    // Within the 366 days the troubleshoot view covers, like the slot pages.
    let day = far_future_monday();
    let at = |h: u32| {
        let d = NaiveDate::parse_from_str(&day, "%Y-%m-%d").unwrap();
        utc(d.year(), d.month(), d.day(), h, 0)
    };
    insert_v1_booking(&pool, &et_id, "held@test.com", at(10), 30, "pending", &[]).await;
    let response = app
        .oneshot(get_authed(
            &format!("/dashboard/troubleshoot?date={day}&event_type=test-meeting"),
            &session,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = body_string(response).await;
    assert!(body.contains("(pending request)"), "{body}");
}

#[tokio::test]
async fn new_pending_booking_gets_an_expiry_deadline() {
    let (app, pool, _, et_id) = setup_test_app().await;
    sqlx::query(
        "UPDATE event_types SET requires_confirmation = 1, pending_expiry_hours = 24 WHERE id = ?",
    )
    .bind(&et_id)
    .execute(&pool)
    .await
    .unwrap();
    let csrf = "csrf-pending-deadline";
    let body = format!(
        "_csrf={}&date={}&time=10%3A00&name=Pending+Guest&email=deadline%40test.com&tz=UTC",
        csrf,
        far_future_monday()
    );
    let before = Utc::now();
    let response = app
        .oneshot(post_form_unauthed(
            "/u/testuser/test-meeting/book",
            csrf,
            &body,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let (status, expires, since): (String, Option<String>, Option<String>) = sqlx::query_as(
        "SELECT status, pending_expires_at, pending_since FROM bookings WHERE guest_email = 'deadline@test.com'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(status, "pending");
    let expires = chrono::DateTime::parse_from_rfc3339(&expires.unwrap()).unwrap();
    let expected = before + Duration::hours(24);
    assert!((expires.with_timezone(&Utc) - expected).num_seconds().abs() < 120);
    assert!(since.is_some());
}

#[tokio::test]
async fn sweep_declines_expired_requests_once_and_emails_the_guest_in_their_language() {
    let _env = crate::test_support::ENV_LOCK.lock().await;
    let _guard = crate::test_support::EnvGuard::new();
    std::env::set_var("CALRS_BASE_URL", "https://cal.example.com");
    let pool = setup_test_db().await;
    let (_user_id, _account_id, et_id) = seed_test_data(&pool).await;
    sqlx::query("UPDATE event_types SET requires_confirmation = 1 WHERE id = ?")
        .bind(&et_id)
        .execute(&pool)
        .await
        .unwrap();
    let (port, mails) = spawn_capturing_mta().await;
    use_capturing_mta(&pool, port).await;

    let now = Utc::now();
    let start = now + Duration::days(3);
    let past = Some(utc_stamp(now - Duration::minutes(1)));
    let future = Some(utc_stamp(now + Duration::hours(5)));
    let expired = insert_v1_booking(
        &pool,
        &et_id,
        "expired@test.com",
        start,
        30,
        "pending",
        &[
            ("pending_expires_at", past.clone()),
            ("language", Some("fr".into())),
        ],
    )
    .await;
    let waiting = insert_v1_booking(
        &pool,
        &et_id,
        "waiting@test.com",
        start + Duration::hours(1),
        30,
        "pending",
        &[("pending_expires_at", future)],
    )
    .await;
    let never = insert_v1_booking(
        &pool,
        &et_id,
        "never@test.com",
        start + Duration::hours(2),
        30,
        "pending",
        &[],
    )
    .await;
    let confirmed = insert_v1_booking(
        &pool,
        &et_id,
        "confirmed@test.com",
        start + Duration::hours(3),
        30,
        "confirmed",
        &[("pending_expires_at", past)],
    )
    .await;

    let sweep = sweep_pending_requests(&pool, &[0u8; 32], now).await;
    assert_eq!(sweep.expired, 1);
    assert_eq!(booking_status(&pool, &expired).await, "declined");
    assert_eq!(booking_status(&pool, &waiting).await, "pending");
    assert_eq!(booking_status(&pool, &never).await, "pending");
    assert_eq!(booking_status(&pool, &confirmed).await, "confirmed");

    let guest = mail_to(&mails, "expired@test.com").await;
    assert!(guest.contains("Bonjour Guest,"), "{guest}");
    assert!(
        guest.contains("Votre demande de réservation a expiré"),
        "{guest}"
    );
    assert!(
        guest.contains("https://cal.example.com/u/testuser/test-meeting"),
        "the guest gets a link to pick another time: {guest}"
    );
    // A guest email: sent as the host, replies go to the host (#221).
    assert_eq!(
        mail_header(&guest, "From"),
        Some("\"Test User\" <noreply@test.com>"),
        "{guest}"
    );
    assert_eq!(
        mail_header(&guest, "Reply-To"),
        Some("test@example.com"),
        "{guest}"
    );
    let host = mail_to(&mails, "test@example.com").await;
    // The host has no saved language, so their notice is English.
    assert!(host.contains("Booking request expired"), "{host}");
    assert!(host.contains("<html lang=\"en\">"), "{host}");
    assert_eq!(mail_header(&host, "From"), Some("noreply@test.com"));
    assert_eq!(mail_header(&host, "Reply-To"), None, "{host}");

    // Idempotent: a second pass finds nothing and sends nothing.
    let sent = mails.lock().unwrap().len();
    let again = sweep_pending_requests(&pool, &[0u8; 32], now).await;
    assert_eq!(again, PendingSweep::default());
    assert_eq!(mails.lock().unwrap().len(), sent);
    assert_eq!(booking_status(&pool, &expired).await, "declined");
}

#[tokio::test]
async fn sweep_nudges_the_host_once_halfway_to_the_deadline() {
    let _env = crate::test_support::ENV_LOCK.lock().await;
    let _guard = crate::test_support::EnvGuard::new();
    let pool = setup_test_db().await;
    let (_user_id, _account_id, et_id) = seed_test_data(&pool).await;
    let (port, mails) = spawn_capturing_mta().await;
    use_capturing_mta(&pool, port).await;

    let now = Utc::now();
    let start = now + Duration::days(3);
    let due = insert_v1_booking(
        &pool,
        &et_id,
        "due@test.com",
        start,
        30,
        "pending",
        &[
            ("pending_since", Some(utc_stamp(now - Duration::hours(10)))),
            (
                "pending_expires_at",
                Some(utc_stamp(now + Duration::hours(2))),
            ),
        ],
    )
    .await;
    let early = insert_v1_booking(
        &pool,
        &et_id,
        "early@test.com",
        start + Duration::hours(1),
        30,
        "pending",
        &[
            ("pending_since", Some(utc_stamp(now - Duration::hours(1)))),
            (
                "pending_expires_at",
                Some(utc_stamp(now + Duration::hours(10))),
            ),
        ],
    )
    .await;

    let sweep = sweep_pending_requests(&pool, &[0u8; 32], now).await;
    assert_eq!(
        sweep,
        PendingSweep {
            expired: 0,
            nudged: 1
        }
    );
    let host = mail_to(&mails, "test@example.com").await;
    assert!(host.contains("is still waiting for your answer"), "{host}");
    assert!(host.contains("due@test.com"), "{host}");
    // Host-facing: the instance sender, no Reply-To (#221).
    assert_eq!(mail_header(&host, "From"), Some("noreply@test.com"));
    assert_eq!(mail_header(&host, "Reply-To"), None, "{host}");

    let sent_at = |id: String| {
        let pool = pool.clone();
        async move {
            sqlx::query_scalar::<_, Option<String>>(
                "SELECT pending_reminder_sent_at FROM bookings WHERE id = ?",
            )
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap()
        }
    };
    assert!(sent_at(due.clone()).await.is_some());
    assert!(sent_at(early).await.is_none());

    let again = sweep_pending_requests(&pool, &[0u8; 32], now).await;
    assert_eq!(again, PendingSweep::default());
    assert_eq!(booking_status(&pool, &due).await, "pending");
}

#[tokio::test]
async fn dashboard_approval_is_refused_when_the_host_became_busy() {
    let (app, pool, session, et_id) = setup_test_app().await;
    let account_id: String = sqlx::query_scalar("SELECT account_id FROM event_types WHERE id = ?")
        .bind(&et_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    let other_et = second_event_type(&pool, &account_id).await;
    let pending = insert_v1_booking(
        &pool,
        &et_id,
        "pending@test.com",
        utc(2030, 3, 18, 10, 0),
        30,
        "pending",
        &[],
    )
    .await;
    // The host took another booking over the same time in the meantime.
    let blocker = insert_v1_booking(
        &pool,
        &other_et,
        "blocker@test.com",
        utc(2030, 3, 18, 10, 15),
        30,
        "confirmed",
        &[],
    )
    .await;

    let csrf = "csrf-approve-busy";
    let response = app
        .clone()
        .oneshot(post_form(
            &format!("/dashboard/bookings/{}/confirm", pending),
            &session,
            csrf,
            &format!("_csrf={}", csrf),
        ))
        .await
        .unwrap();
    assert!(response.status().is_redirection());
    let location = response.headers()["location"].to_str().unwrap().to_string();
    assert_eq!(location, "/dashboard/bookings?error=host-busy");
    assert_eq!(booking_status(&pool, &pending).await, "pending");

    // The dashboard renders the reason, translated.
    let page = app
        .clone()
        .oneshot(get_authed(&location, &session))
        .await
        .unwrap();
    assert!(body_string(page)
        .await
        .contains("is no longer free at this time"));

    // Once the conflict is gone the approval goes through: the pending
    // booking's own hold does not count against itself.
    sqlx::query("UPDATE bookings SET status = 'cancelled' WHERE id = ?")
        .bind(&blocker)
        .execute(&pool)
        .await
        .unwrap();
    let csrf = "csrf-approve-free";
    let response = app
        .oneshot(post_form(
            &format!("/dashboard/bookings/{}/confirm", pending),
            &session,
            csrf,
            &format!("_csrf={}", csrf),
        ))
        .await
        .unwrap();
    assert!(response.status().is_redirection());
    assert_eq!(booking_status(&pool, &pending).await, "confirmed");
}

async fn approve_from_dashboard(
    app: &Router,
    session: &str,
    id: &str,
) -> axum::http::Response<Body> {
    let csrf = format!("csrf-approve-{id}");
    app.clone()
        .oneshot(post_form(
            &format!("/dashboard/bookings/{}/confirm", id),
            session,
            &csrf,
            &format!("_csrf={}", csrf),
        ))
        .await
        .unwrap()
}

fn location(response: &axum::http::Response<Body>) -> String {
    response.headers()["location"].to_str().unwrap().to_string()
}

/// Review finding on #234: the approval re-check counted other pending
/// requests as busy, so two overlapping requests (e.g. made before the
/// upgrade) blocked each other's approval forever. Between two waiting
/// requests the host decides; the loser is refused once the winner is
/// confirmed.
#[tokio::test]
async fn overlapping_pending_requests_do_not_block_each_others_approval() {
    let (app, pool, session, et_id) = setup_test_app().await;
    let first = insert_v1_booking(
        &pool,
        &et_id,
        "first@test.com",
        utc(2030, 3, 18, 10, 0),
        30,
        "pending",
        &[],
    )
    .await;
    let second = insert_v1_booking(
        &pool,
        &et_id,
        "second@test.com",
        utc(2030, 3, 18, 10, 15),
        30,
        "pending",
        &[],
    )
    .await;

    // The later request can be approved although the earlier one holds
    // an overlapping slot...
    let response = approve_from_dashboard(&app, &session, &second).await;
    assert_eq!(location(&response), "/dashboard/bookings");
    assert_eq!(booking_status(&pool, &second).await, "confirmed");

    // ...and then the earlier one is refused: the host is now busy.
    let response = approve_from_dashboard(&app, &session, &first).await;
    assert_eq!(location(&response), "/dashboard/bookings?error=host-busy");
    assert_eq!(booking_status(&pool, &first).await, "pending");
}

/// Review finding on #234: an empty collective roster counted as free, the
/// opposite of the reschedule path where it means nobody can host.
#[tokio::test]
async fn approval_is_refused_when_the_collective_roster_is_empty() {
    let (app, pool, session, _) = setup_test_app().await;
    let (team_id, team_et) = seed_team_event_type(&pool).await;
    sqlx::query("UPDATE event_types SET scheduling_mode = 'collective' WHERE id = ?")
        .bind(&team_et)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM team_members WHERE team_id = ?")
        .bind(&team_id)
        .execute(&pool)
        .await
        .unwrap();
    let id = insert_v1_booking(
        &pool,
        &team_et,
        "collective@test.com",
        utc(2030, 3, 18, 10, 0),
        30,
        "pending",
        &[],
    )
    .await;

    let response = approve_from_dashboard(&app, &session, &id).await;
    assert_eq!(location(&response), "/dashboard/bookings?error=host-busy");
    assert_eq!(booking_status(&pool, &id).await, "pending");
}

/// Review finding on #234: the host check ran outside any lock, so two
/// overlapping requests (on an event type that does not hold pending slots)
/// approved at the same moment could both pass it and both be confirmed.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_approvals_of_overlapping_requests_confirm_only_one() {
    let (app, pool, session, et_id) = setup_test_app().await;
    sqlx::query("UPDATE event_types SET pending_blocks_slot = 0 WHERE id = ?")
        .bind(&et_id)
        .execute(&pool)
        .await
        .unwrap();
    let first = insert_v1_booking(
        &pool,
        &et_id,
        "first@test.com",
        utc(2030, 3, 18, 10, 0),
        30,
        "pending",
        &[],
    )
    .await;
    let second = insert_v1_booking(
        &pool,
        &et_id,
        "second@test.com",
        utc(2030, 3, 18, 10, 15),
        30,
        "pending",
        &[],
    )
    .await;

    let by_dashboard = approve_from_dashboard(&app, &session, &first);
    let csrf = "csrf-concurrent-token";
    let by_token = app.clone().oneshot(post_form_unauthed(
        &format!("/booking/approve/confirm-{second}"),
        csrf,
        &format!("_csrf={}", csrf),
    ));
    let (a, b) = tokio::join!(by_dashboard, by_token);
    b.unwrap();
    drop(a);

    let confirmed: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM bookings WHERE id IN (?, ?) AND status = 'confirmed'",
    )
    .bind(&first)
    .bind(&second)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(confirmed, 1, "exactly one overlapping request is approved");
}

#[tokio::test]
async fn token_approval_is_refused_when_a_calendar_event_now_overlaps() {
    let (app, pool, _, et_id) = setup_test_app().await;
    let account_id: String = sqlx::query_scalar("SELECT account_id FROM event_types WHERE id = ?")
        .bind(&et_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    let pending = insert_v1_booking(
        &pool,
        &et_id,
        "pending@test.com",
        utc(2030, 3, 18, 10, 0),
        30,
        "pending",
        &[],
    )
    .await;
    seed_synced_event(
        &pool,
        &account_id,
        "20300318T100000",
        "20300318T110000",
        "UTC",
        "OPAQUE",
    )
    .await;

    let response = app
        .oneshot(post_bare(&format!("/booking/approve/confirm-{}", pending)))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = body_string(response).await;
    assert!(body.contains("is no longer free at this time"), "{body}");
    assert_eq!(booking_status(&pool, &pending).await, "pending");
}

async fn pending_columns(
    pool: &SqlitePool,
    id: &str,
) -> (Option<String>, Option<String>, Option<String>) {
    sqlx::query_as(
        "SELECT pending_expires_at, pending_since, pending_reminder_sent_at FROM bookings WHERE id = ?",
    )
    .bind(id)
    .fetch_one(pool)
    .await
    .unwrap()
}

/// Review finding on #234: an approved booking kept its old, now past,
/// deadline, and a guest reschedule back to pending only re-stamped it
/// afterwards on another connection. A sweep landing in between declined the
/// fresh request at once. The deadline is now cleared on approval and written
/// by the same UPDATE that puts the booking back to pending.
#[tokio::test]
async fn approved_then_rescheduled_request_is_not_expired_by_the_old_deadline() {
    let (app, pool, session, et_id) = setup_test_app().await;
    sqlx::query(
        "UPDATE event_types SET requires_confirmation = 1, pending_expiry_hours = 24 WHERE id = ?",
    )
    .bind(&et_id)
    .execute(&pool)
    .await
    .unwrap();
    let past = Some(utc_stamp(Utc::now() - Duration::hours(1)));
    let id = insert_v1_booking(
        &pool,
        &et_id,
        "approved@test.com",
        Utc::now() + Duration::days(2),
        30,
        "pending",
        &[
            ("pending_expires_at", past.clone()),
            ("pending_since", past),
        ],
    )
    .await;

    // The host approves just before the sweep would have expired it.
    let csrf = "csrf-approve-then-resched";
    let response = app
        .clone()
        .oneshot(post_form(
            &format!("/dashboard/bookings/{}/confirm", id),
            &session,
            csrf,
            &format!("_csrf={}", csrf),
        ))
        .await
        .unwrap();
    assert!(response.status().is_redirection());
    assert_eq!(booking_status(&pool, &id).await, "confirmed");
    assert_eq!(pending_columns(&pool, &id).await, (None, None, None));

    // The guest moves it: back to pending, with a fresh deadline.
    let token: String = sqlx::query_scalar("SELECT reschedule_token FROM bookings WHERE id = ?")
        .bind(&id)
        .fetch_one(&pool)
        .await
        .unwrap();
    let csrf = "csrf-resched-after-approve";
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
    assert_eq!(booking_status(&pool, &id).await, "pending");

    let sweep = sweep_pending_requests(&pool, &[0u8; 32], Utc::now()).await;
    assert_eq!(sweep.expired, 0);
    assert_eq!(booking_status(&pool, &id).await, "pending");
}

/// Declining or cancelling a request clears its pending window too.
#[tokio::test]
async fn leaving_pending_clears_the_window() {
    let (app, pool, session, et_id) = setup_test_app().await;
    let future = Some(utc_stamp(Utc::now() + Duration::hours(5)));
    let id = insert_v1_booking(
        &pool,
        &et_id,
        "declined@test.com",
        Utc::now() + Duration::days(2),
        30,
        "pending",
        &[
            ("pending_expires_at", future.clone()),
            ("pending_since", future),
        ],
    )
    .await;
    let csrf = "csrf-decline-clears";
    let response = app
        .oneshot(post_form(
            &format!("/dashboard/bookings/{}/cancel", id),
            &session,
            csrf,
            &format!("_csrf={}", csrf),
        ))
        .await
        .unwrap();
    assert!(response.status().is_redirection());
    assert_eq!(booking_status(&pool, &id).await, "declined");
    assert_eq!(pending_columns(&pool, &id).await, (None, None, None));
}

#[tokio::test]
async fn guest_reschedule_back_to_pending_restarts_the_expiry_clock() {
    let (app, pool, _, et_id) = setup_test_app().await;
    sqlx::query(
        "UPDATE event_types SET requires_confirmation = 1, pending_expiry_hours = 24 WHERE id = ?",
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
        "pending",
        &[
            ("pending_expires_at", Some("2000-01-01T00:00:00Z".into())),
            (
                "pending_reminder_sent_at",
                Some("2000-01-01T00:00:00Z".into()),
            ),
        ],
    )
    .await;
    let token: String = sqlx::query_scalar("SELECT reschedule_token FROM bookings WHERE id = ?")
        .bind(&id)
        .fetch_one(&pool)
        .await
        .unwrap();

    let csrf = "csrf-resched-pending";
    let before = Utc::now();
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

    let (status, expires, nudged): (String, Option<String>, Option<String>) = sqlx::query_as(
        "SELECT status, pending_expires_at, pending_reminder_sent_at FROM bookings WHERE id = ?",
    )
    .bind(&id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(status, "pending");
    let expires = chrono::DateTime::parse_from_rfc3339(&expires.unwrap()).unwrap();
    let expected = before + Duration::hours(24);
    assert!(
        (expires.with_timezone(&Utc) - expected).num_seconds().abs() < 120,
        "deadline restarted from the reschedule: {expires}"
    );
    assert!(nudged.is_none(), "a fresh window gets a fresh nudge");
}

#[tokio::test]
async fn event_type_form_persists_the_pending_settings() {
    let (app, pool, session, _) = setup_test_app().await;
    let settings = |slug: &'static str| {
        let pool = pool.clone();
        async move {
            sqlx::query_as::<_, (i32, Option<i32>)>(
                "SELECT pending_blocks_slot, pending_expiry_hours FROM event_types WHERE slug = ?",
            )
            .bind(slug)
            .fetch_one(&pool)
            .await
            .unwrap()
        }
    };
    let base = "title=Held&slug=held&duration_min=30&requires_confirmation=on&location_type=link&location_value=https%3A%2F%2Fmeet.example.com%2Froom&avail_days=1,2,3,4,5&avail_start=09:00&avail_end=17:00";

    // A form without the section gets the defaults.
    let csrf = "csrf-pending-create";
    let response = app
        .clone()
        .oneshot(post_form(
            "/dashboard/event-types/new",
            &session,
            csrf,
            &format!("_csrf={}&{}", csrf, base),
        ))
        .await
        .unwrap();
    assert!(response.status().is_redirection());
    assert_eq!(settings("held").await, (1, Some(24)));

    // Unchecked box and a chosen expiry are stored as such.
    let csrf = "csrf-pending-edit";
    let response = app
        .clone()
        .oneshot(post_form(
            "/dashboard/event-types/held/edit",
            &session,
            csrf,
            &format!("_csrf={}&{}&pending_expiry_hours=48", csrf, base),
        ))
        .await
        .unwrap();
    assert!(response.status().is_redirection());
    assert_eq!(settings("held").await, (0, Some(48)));

    // "never" stores NULL; an edit without the section keeps what is stored.
    let csrf = "csrf-pending-never";
    app.clone()
        .oneshot(post_form(
            "/dashboard/event-types/held/edit",
            &session,
            csrf,
            &format!(
                "_csrf={}&{}&pending_blocks_slot=on&pending_expiry_hours=never",
                csrf, base
            ),
        ))
        .await
        .unwrap();
    assert_eq!(settings("held").await, (1, None));
    let csrf = "csrf-pending-keep";
    app.clone()
        .oneshot(post_form(
            "/dashboard/event-types/held/edit",
            &session,
            csrf,
            &format!("_csrf={}&{}", csrf, base),
        ))
        .await
        .unwrap();
    assert_eq!(settings("held").await, (1, None));

    // The edit form shows them.
    let page = app
        .oneshot(get_authed("/dashboard/event-types/held/edit", &session))
        .await
        .unwrap();
    let body = body_string(page).await;
    assert!(body.contains("id=\"pending-options\""), "{body}");
    assert!(body.contains("<option value=\"never\" selected>"), "{body}");
}

#[tokio::test]
async fn event_type_form_keeps_an_unlisted_stored_expiry_on_save() {
    let (app, pool, session, et_id) = setup_test_app().await;
    sqlx::query(
        "UPDATE event_types SET requires_confirmation = 1, pending_expiry_hours = 12 WHERE id = ?",
    )
    .bind(&et_id)
    .execute(&pool)
    .await
    .unwrap();
    let page = app
        .clone()
        .oneshot(get_authed(
            "/dashboard/event-types/test-meeting/edit",
            &session,
        ))
        .await
        .unwrap();
    let body = body_string(page).await;
    assert!(body.contains("<option value=\"12\" selected>"), "{body}");

    // What the browser posts back for that select.
    let csrf = "csrf-pending-unlisted";
    let response = app
        .oneshot(post_form(
            "/dashboard/event-types/test-meeting/edit",
            &session,
            csrf,
            &format!(
                "_csrf={}&title=Test+Meeting&slug=test-meeting&duration_min=30&requires_confirmation=on&pending_blocks_slot=on&pending_expiry_hours=12&location_type=link&location_value=https%3A%2F%2Fmeet.example.com%2Froom&avail_days=1,2,3,4,5&avail_start=09:00&avail_end=17:00",
                csrf
            ),
        ))
        .await
        .unwrap();
    assert!(response.status().is_redirection());
    let stored: Option<i32> =
        sqlx::query_scalar("SELECT pending_expiry_hours FROM event_types WHERE id = ?")
            .bind(&et_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(stored, Some(12));
}

#[tokio::test]
async fn team_event_type_form_persists_the_pending_settings() {
    let (app, pool, session, _) = setup_test_app().await;
    let (team_id, team_et_id) = seed_team_event_type(&pool).await;
    let csrf = "csrf-pending-team";
    let body = format!(
        "_csrf={}&title=Team+Sync&slug=team-sync&duration_min=30&requires_confirmation=on&pending_expiry_hours=4&location_type=link&location_value=https%3A%2F%2Fmeet.example.com%2Froom&avail_days=1,2,3,4,5&avail_start=09:00&avail_end=17:00&scheduling_mode=round_robin",
        csrf
    );
    let response = app
        .oneshot(post_form(
            &format!("/dashboard/group-event-types/{}/team-sync/edit", team_id),
            &session,
            csrf,
            &body,
        ))
        .await
        .unwrap();
    assert!(response.status().is_redirection());
    let stored: (i32, Option<i32>) = sqlx::query_as(
        "SELECT pending_blocks_slot, pending_expiry_hours FROM event_types WHERE id = ?",
    )
    .bind(&team_et_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(stored, (0, Some(4)));
}

#[tokio::test]
async fn troubleshoot_counts_the_same_bookings_as_the_slot_pages() {
    let (app, pool, session, et_id) = setup_test_app().await;
    // Within the 366 days the troubleshoot view covers, like the slot pages.
    let day = far_future_monday();
    let at = |h: u32| {
        let d = NaiveDate::parse_from_str(&day, "%Y-%m-%d").unwrap();
        utc(d.year(), d.month(), d.day(), h, 0)
    };
    let user_id: String = sqlx::query_scalar("SELECT id FROM users WHERE username = 'testuser'")
        .fetch_one(&pool)
        .await
        .unwrap();
    let bob = insert_role_user(&pool, "bob@test.com", "user").await;
    let bobs_et = insert_personal_et(&pool, &bob, "bobs-team-sync").await;
    // Assigned to testuser on someone else's event type: testuser is busy.
    insert_v1_booking(
        &pool,
        &bobs_et,
        "assigned@test.com",
        at(11),
        30,
        "confirmed",
        &[
            ("assigned_user_id", Some(user_id)),
            ("guest_name", Some("Assigned Guest".into())),
        ],
    )
    .await;
    // On testuser's own event type but assigned to bob: testuser is free.
    insert_v1_booking(
        &pool,
        &et_id,
        "bobs@test.com",
        at(14),
        30,
        "confirmed",
        &[
            ("assigned_user_id", Some(bob)),
            ("guest_name", Some("Bobs Guest".into())),
        ],
    )
    .await;

    let response = app
        .oneshot(get_authed(
            &format!("/dashboard/troubleshoot?date={day}&event_type=test-meeting"),
            &session,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = body_string(response).await;
    assert!(body.contains("Assigned Guest"), "{body}");
    assert!(!body.contains("Bobs Guest"), "{body}");
}

/// Review finding on #234: the sweep sent its emails inline and serially,
/// before the reminder batch of the same tick, so a relay that stalls (or a
/// burst of expiries) delayed every reminder. The flips stay inline; the
/// notifications are spawned.
#[tokio::test]
async fn sweep_does_not_wait_for_a_stalled_mail_relay() {
    let _env = crate::test_support::ENV_LOCK.lock().await;
    let _guard = crate::test_support::EnvGuard::new();
    let pool = setup_test_db().await;
    let (_user_id, _account_id, et_id) = seed_test_data(&pool).await;
    // A relay that accepts connections and never says a word.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        let mut held = Vec::new();
        while let Ok((sock, _)) = listener.accept().await {
            held.push(sock);
        }
    });
    use_capturing_mta(&pool, port).await;

    let now = Utc::now();
    let mut expired = Vec::new();
    for i in 0..3 {
        expired.push(
            insert_v1_booking(
                &pool,
                &et_id,
                &format!("stalled{i}@test.com"),
                now + Duration::days(3) + Duration::hours(i),
                30,
                "pending",
                &[(
                    "pending_expires_at",
                    Some(utc_stamp(now - Duration::minutes(1))),
                )],
            )
            .await,
        );
    }
    insert_v1_booking(
        &pool,
        &et_id,
        "nudge@test.com",
        now + Duration::days(4),
        30,
        "pending",
        &[
            ("pending_since", Some(utc_stamp(now - Duration::hours(10)))),
            (
                "pending_expires_at",
                Some(utc_stamp(now + Duration::hours(2))),
            ),
        ],
    )
    .await;

    let sweep = tokio::time::timeout(
        std::time::Duration::from_secs(2),
        sweep_pending_requests(&pool, &[0u8; 32], now),
    )
    .await
    .expect("the sweep must not wait on SMTP");
    assert_eq!(
        sweep,
        PendingSweep {
            expired: 3,
            nudged: 1
        }
    );
    for id in &expired {
        assert_eq!(booking_status(&pool, id).await, "declined");
    }
}

/// Review finding on #234: a public event type on a private team linked to
/// /team/{team}/{slug} without the team invite, which lands the guest on
/// "event type not found".
#[tokio::test]
async fn rebook_link_carries_the_private_team_invite() {
    let _env = crate::test_support::ENV_LOCK.lock().await;
    let _guard = crate::test_support::EnvGuard::new();
    std::env::set_var("CALRS_BASE_URL", "https://cal.example.com");
    let pool = setup_test_db().await;
    seed_test_data(&pool).await;
    let (team_id, team_et) = seed_team_event_type(&pool).await;
    let set_team = |visibility: &'static str, token: Option<&'static str>| {
        let pool = pool.clone();
        let team_id = team_id.clone();
        async move {
            sqlx::query("UPDATE teams SET visibility = ?, invite_token = ? WHERE id = ?")
                .bind(visibility)
                .bind(token)
                .bind(&team_id)
                .execute(&pool)
                .await
                .unwrap();
        }
    };

    set_team("public", None).await;
    assert_eq!(
        event_type_rebook_url(&pool, &team_et).await.as_deref(),
        Some("https://cal.example.com/team/horizon-team/team-sync")
    );
    set_team("private", Some("tok-123")).await;
    assert_eq!(
        event_type_rebook_url(&pool, &team_et).await.as_deref(),
        Some("https://cal.example.com/team/horizon-team/team-sync?invite=tok-123")
    );
    set_team("private", None).await;
    assert_eq!(event_type_rebook_url(&pool, &team_et).await, None);
}

/// #221: an expired collective request reaches the guest from the joined
/// roster, with Reply-To on the ICS organizer (first member by name), like
/// the confirmation they would have got. It used to name only the first host
/// the recipient query returned.
#[tokio::test]
async fn expired_collective_request_comes_from_the_roster() {
    let _env = crate::test_support::ENV_LOCK.lock().await;
    let _guard = crate::test_support::EnvGuard::new();
    let pool = setup_test_db().await;
    seed_test_data(&pool).await;
    let (team_id, team_et) = seed_team_event_type(&pool).await;
    sqlx::query(
        "UPDATE event_types SET scheduling_mode = 'collective', requires_confirmation = 1 WHERE id = ?",
    )
    .bind(&team_et)
    .execute(&pool)
    .await
    .unwrap();
    let ann_id = uuid::Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO users (id, email, name, role, auth_provider, username, enabled) VALUES (?, 'ann@test.com', 'Ann Member', 'user', 'local', 'ann', 1)")
        .bind(&ann_id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO team_members (team_id, user_id, role, source) VALUES (?, ?, 'member', 'direct')")
        .bind(&team_id)
        .bind(&ann_id)
        .execute(&pool)
        .await
        .unwrap();
    let (port, mails) = spawn_capturing_mta().await;
    use_capturing_mta(&pool, port).await;

    let now = Utc::now();
    insert_v1_booking(
        &pool,
        &team_et,
        "roster@test.com",
        now + Duration::days(3),
        30,
        "pending",
        &[(
            "pending_expires_at",
            Some(utc_stamp(now - Duration::minutes(1))),
        )],
    )
    .await;

    let sweep = sweep_pending_requests(&pool, &[0u8; 32], now).await;
    assert_eq!(sweep.expired, 1);

    let guest = mail_to(&mails, "roster@test.com").await;
    // lettre RFC 2047-encodes a display name containing "&":
    // base64("Ann Member & Test User").
    assert_eq!(
        mail_header(&guest, "From"),
        Some("=?utf-8?b?QW5uIE1lbWJlciAmIFRlc3QgVXNlcg==?= <noreply@test.com>"),
        "{guest}"
    );
    assert_eq!(
        mail_header(&guest, "Reply-To"),
        Some("ann@test.com"),
        "{guest}"
    );
}

/// Past the 366 days every booking page stops at, the troubleshoot view says
/// so, even for an event type with no horizon of its own.
#[tokio::test]
async fn troubleshoot_marks_days_past_the_366_day_cap() {
    let (app, _pool, session, _et_id) = setup_test_app().await;
    let mut day = host_today() + Duration::days(400);
    while day.weekday() != chrono::Weekday::Mon {
        day += Duration::days(1);
    }
    let response = app
        .oneshot(get_authed(
            &format!("/dashboard/troubleshoot?date={day}&event_type=test-meeting"),
            &session,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = body_string(response).await;
    assert!(body.contains("Beyond booking horizon (366 days)"), "{body}");
}
