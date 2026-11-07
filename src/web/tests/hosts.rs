//! Host resolution for bookings: identity, write targets, reschedule hosts.

use super::*;

/// The reschedule/approve ICS must carry the organizer the booking flow
/// wrote, otherwise Google CalDAV 403s the PUT and the change silently
/// never reaches the calendar.
#[tokio::test]
async fn booking_host_identity_follows_the_booking_not_the_owner() {
    let pool = setup_test_db().await;
    let alice = insert_role_user(&pool, "alice@organizer.test", "user").await;
    let bob = insert_role_user(&pool, "bob@organizer.test", "user").await;
    let (_team, et_id) = insert_team_with_et(
        &pool,
        "organizer",
        "organizer-et",
        &[(&alice, "member"), (&bob, "member")],
    )
    .await;
    let start = NaiveDate::from_ymd_opt(2026, 3, 17)
        .unwrap()
        .and_hms_opt(10, 0, 0)
        .unwrap();

    // Assigned booking: the assigned member, never the event type owner.
    insert_booking(&pool, &et_id, Some(&bob), start, "confirmed").await;
    let uid: String =
        sqlx::query_scalar("SELECT uid FROM bookings WHERE event_type_id = ? LIMIT 1")
            .bind(&et_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        booking_host_identity(&pool, &uid).await,
        Some((
            "bob@organizer.test".to_string(),
            "bob@organizer.test".to_string(),
            None
        )),
        "assigned booking must keep the assigned member as organizer"
    );

    // Collective + unassigned: whole roster joined, first member's address,
    // matching how the booking flow builds it.
    sqlx::query("UPDATE event_types SET scheduling_mode = 'collective' WHERE id = ?")
        .bind(&et_id)
        .execute(&pool)
        .await
        .unwrap();
    let later = start + Duration::hours(2);
    insert_booking(&pool, &et_id, None, later, "confirmed").await;
    let uid2: String = sqlx::query_scalar(
        "SELECT uid FROM bookings WHERE event_type_id = ? AND assigned_user_id IS NULL",
    )
    .bind(&et_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        booking_host_identity(&pool, &uid2).await,
        Some((
            "alice@organizer.test & bob@organizer.test".to_string(),
            "alice@organizer.test".to_string(),
            None
        )),
        "collective booking must carry the roster and the first member's address"
    );

    // Personal event type: the owner.
    let (owner, _, personal_et) = seed_test_data(&pool).await;
    insert_booking(&pool, &personal_et, None, start, "confirmed").await;
    let uid3: String =
        sqlx::query_scalar("SELECT uid FROM bookings WHERE event_type_id = ? LIMIT 1")
            .bind(&personal_et)
            .fetch_one(&pool)
            .await
            .unwrap();
    let owner_email: String =
        sqlx::query_scalar("SELECT COALESCE(booking_email, email) FROM users WHERE id = ?")
            .bind(&owner)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        booking_host_identity(&pool, &uid3)
            .await
            .map(|(_, email, _)| email),
        Some(owner_email),
        "personal booking falls back to the event type owner"
    );
}

#[tokio::test]
async fn booking_write_targets_resolves_assigned_collective_and_fallback() {
    let pool = setup_test_db().await;
    let alice = insert_role_user(&pool, "alice@i147.test", "user").await;
    let bob = insert_role_user(&pool, "bob@i147.test", "user").await;
    let (_team, et_id) = insert_team_with_et(
        &pool,
        "i147",
        "i147-et",
        &[(&alice, "member"), (&bob, "member")],
    )
    .await;
    let start = NaiveDate::from_ymd_opt(2026, 3, 17)
        .unwrap()
        .and_hms_opt(10, 0, 0)
        .unwrap();

    // Assigned booking: the assigned member, never the fallback owner.
    insert_booking(&pool, &et_id, Some(&alice), start, "confirmed").await;
    let uid: String =
        sqlx::query_scalar("SELECT uid FROM bookings WHERE event_type_id = ? LIMIT 1")
            .bind(&et_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        booking_write_targets(&pool, &uid, "owner-id").await,
        vec![alice.clone()],
        "assigned booking targets the assigned member (#147)"
    );

    // Collective + unassigned: every eligible member.
    sqlx::query("UPDATE event_types SET scheduling_mode = 'collective' WHERE id = ?")
        .bind(&et_id)
        .execute(&pool)
        .await
        .unwrap();
    let later = start + Duration::hours(2);
    insert_booking(&pool, &et_id, None, later, "confirmed").await;
    let uid2: String = sqlx::query_scalar(
        "SELECT uid FROM bookings WHERE event_type_id = ? AND assigned_user_id IS NULL",
    )
    .bind(&et_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    let mut targets = booking_write_targets(&pool, &uid2, "owner-id").await;
    targets.sort();
    let mut expected = vec![alice.clone(), bob.clone()];
    expected.sort();
    assert_eq!(targets, expected, "collective targets every member");

    // Personal (no team): the event type owner, even if the caller
    // passed someone else (same person `elect_meet_owner` uses).
    let (owner, _, personal_et) = seed_test_data(&pool).await;
    insert_booking(&pool, &personal_et, None, start, "confirmed").await;
    let uid3: String =
        sqlx::query_scalar("SELECT uid FROM bookings WHERE event_type_id = ? LIMIT 1")
            .bind(&personal_et)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        booking_write_targets(&pool, &uid3, "not-the-owner").await,
        vec![owner.clone()]
    );

    // Unassigned round-robin: event type owner, not the caller.
    sqlx::query("UPDATE event_types SET scheduling_mode = 'round_robin' WHERE id = ?")
        .bind(&et_id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "UPDATE accounts SET user_id = ? WHERE id = (SELECT account_id FROM event_types WHERE id = ?)",
    )
    .bind(&alice)
    .bind(&et_id)
    .execute(&pool)
    .await
    .unwrap();
    let rr_start = start + Duration::hours(4);
    insert_booking(&pool, &et_id, None, rr_start, "confirmed").await;
    let uid4: String =
        sqlx::query_scalar("SELECT uid FROM bookings WHERE event_type_id = ? AND start_at = ?")
            .bind(&et_id)
            .bind(rr_start.format("%Y-%m-%dT%H:%M:%S").to_string())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        booking_write_targets(&pool, &uid4, &bob).await,
        vec![alice],
        "unassigned round-robin writes to the event type owner, matching Meet minting"
    );
}

/// A Google Meet booking is PUT to the owner's Google calendar by
/// `create_meet_for_booking`, so the write-back loop skips its own PUT to
/// keep a second ICS from stripping `conferenceData`. It must still record
/// `caldav_calendar_href`: without it `caldav_delete_booking` reads the
/// booking as never pushed and returns early, so cancelling leaves the
/// Google event and its Meet on the host's calendar, blocking their own
/// availability from then on.
/// #269: a claimant who owns the booking's Google Meet event gets no copy
/// written or deleted by uid: that event is the meeting on their calendar.
#[tokio::test]
async fn a_claimant_holding_the_meet_is_left_alone() {
    let pool = setup_test_db().await;
    let alice = insert_role_user(&pool, "alice@meetclaim.test", "user").await;
    let bob = insert_role_user(&pool, "bob@meetclaim.test", "user").await;
    let et_id = insert_personal_et(&pool, &alice, "meet-claim").await;
    sqlx::query("UPDATE event_types SET location_type = 'google_meet' WHERE id = ?")
        .bind(&et_id)
        .execute(&pool)
        .await
        .unwrap();
    // Alice writes to Google, which makes her the Meet owner.
    sqlx::query(
        "INSERT INTO caldav_sources (id, account_id, name, url, username, enabled, \
             write_calendar_href, auth_type, oauth2_provider, access_token_enc, token_expires_at) \
             SELECT 'src-meet-claim', a.id, 'Google', 'https://apidata.example.test/dav/', 'alice', 1, \
             'https://apidata.example.test/dav/alice/events/', 'oauth2', 'google', 'unopenable', \
             '2099-01-01T00:00:00' FROM accounts a WHERE a.user_id = ?",
    )
    .bind(&alice)
    .execute(&pool)
    .await
    .unwrap();
    let start = NaiveDate::from_ymd_opt(2026, 3, 17)
        .unwrap()
        .and_hms_opt(10, 0, 0)
        .unwrap();
    insert_booking(&pool, &et_id, None, start, "confirmed").await;
    let uid: String =
        sqlx::query_scalar("SELECT uid FROM bookings WHERE event_type_id = ? LIMIT 1")
            .bind(&et_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    let claim = |user: String| {
        let pool = pool.clone();
        let uid = uid.clone();
        async move {
            sqlx::query("UPDATE bookings SET claimed_by_user_id = ? WHERE uid = ?")
                .bind(user)
                .bind(uid)
                .execute(&pool)
                .await
                .unwrap();
        }
    };

    claim(alice.clone()).await;
    // No Meet yet: an ordinary claimant.
    assert_eq!(booking_claimant(&pool, &uid).await, Some(alice.clone()));
    sqlx::query("UPDATE bookings SET meeting_url = ? WHERE uid = ?")
        .bind("https://meet.google.com/aaa-bbbb-ccc")
        .bind(&uid)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(booking_claimant(&pool, &uid).await, None);
    claim(bob.clone()).await;
    assert_eq!(booking_claimant(&pool, &uid).await, Some(bob));
}

#[tokio::test]
async fn google_meet_skip_branch_still_records_caldav_href() {
    let pool = setup_test_db().await;
    let alice = insert_role_user(&pool, "alice@meethref.test", "user").await;
    let et_id = insert_personal_et(&pool, &alice, "meet-href").await;
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
    // A Meet URL is what makes `should_skip_caldav_put` fire. The write
    // href is deliberately not `/caldav/v2/`, so a times patch (reschedule)
    // would fail calendarId parse and never hit the network.
    let start = NaiveDate::from_ymd_opt(2026, 3, 17)
        .unwrap()
        .and_hms_opt(10, 0, 0)
        .unwrap();
    insert_booking(&pool, &et_id, None, start, "confirmed").await;
    let uid: String =
        sqlx::query_scalar("SELECT uid FROM bookings WHERE event_type_id = ? LIMIT 1")
            .bind(&et_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    sqlx::query("UPDATE bookings SET meeting_url = ? WHERE uid = ?")
        .bind("https://meet.google.com/aaa-bbbb-ccc")
        .bind(&uid)
        .execute(&pool)
        .await
        .unwrap();
    assert!(
        crate::google_meet::should_skip_caldav_put(
            &pool,
            &uid,
            &alice,
            &source_id,
            "oauth2",
            Some("google")
        )
        .await,
        "precondition: the owner's Google source is the one holding the Meet"
    );
    let details = crate::email::BookingDetails {
        event_title: "Meet".to_string(),
        date: "2026-03-17".to_string(),
        start_time: "10:00".to_string(),
        end_time: "10:30".to_string(),
        guest_name: "G".to_string(),
        guest_email: "g@e.com".to_string(),
        guest_timezone: "UTC".to_string(),
        host_name: "Alice".to_string(),
        host_email: "alice@meethref.test".to_string(),
        uid: uid.clone(),
        host_timezone: "UTC".to_string(),
        ..Default::default()
    };
    caldav_push_booking_for_user(
        &pool,
        &[0u8; 32],
        &alice,
        &uid,
        &details,
        CopyOwner::Host,
        &[],
    )
    .await;
    let stored: Option<String> =
        sqlx::query_scalar("SELECT caldav_calendar_href FROM bookings WHERE uid = ?")
            .bind(&uid)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        stored.as_deref(),
        Some(write_href),
        "the skip branch must record the href, or cancellation cannot find the Google event"
    );
}

/// Reschedule availability must be checked against the booking's real
/// host(s), not the event type owner (#166).
#[tokio::test]
async fn reschedule_hosts_follows_the_booking_not_the_owner() {
    let pool = setup_test_db().await;
    let alice = insert_role_user(&pool, "alice@r166.test", "user").await;
    let bob = insert_role_user(&pool, "bob@r166.test", "user").await;
    let (team, et_id) = insert_team_with_et(
        &pool,
        "r166",
        "r166-et",
        &[(&alice, "member"), (&bob, "member")],
    )
    .await;
    let start = NaiveDate::from_ymd_opt(2026, 3, 17)
        .unwrap()
        .and_hms_opt(10, 0, 0)
        .unwrap();

    // Round-robin, assigned: only the assigned member's calendar counts.
    insert_booking(&pool, &et_id, Some(&bob), start, "confirmed").await;
    let uid: String =
        sqlx::query_scalar("SELECT uid FROM bookings WHERE event_type_id = ? LIMIT 1")
            .bind(&et_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    let hosts = reschedule_hosts(&pool, &uid, "owner-id").await;
    assert_eq!(hosts.user_ids, vec![bob.clone()]);
    assert_eq!(hosts.team_id.as_deref(), Some(team.as_str()));
    assert!(
        !hosts.collective,
        "an assigned booking is one member's, not the roster's"
    );

    // Collective + unassigned: every eligible member must be free.
    sqlx::query("UPDATE event_types SET scheduling_mode = 'collective' WHERE id = ?")
        .bind(&et_id)
        .execute(&pool)
        .await
        .unwrap();
    let later = start + Duration::hours(2);
    insert_booking(&pool, &et_id, None, later, "confirmed").await;
    let uid2: String = sqlx::query_scalar(
        "SELECT uid FROM bookings WHERE event_type_id = ? AND assigned_user_id IS NULL",
    )
    .bind(&et_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    let collective_hosts = reschedule_hosts(&pool, &uid2, "owner-id").await;
    let mut members = collective_hosts.user_ids.clone();
    members.sort();
    let mut expected = vec![alice.clone(), bob.clone()];
    expected.sort();
    assert_eq!(members, expected, "collective needs the whole roster free");
    assert!(collective_hosts.collective);

    // Personal event type: the owner, and no team semantics.
    let (owner, _, personal_et) = seed_test_data(&pool).await;
    insert_booking(&pool, &personal_et, None, start, "confirmed").await;
    let uid3: String =
        sqlx::query_scalar("SELECT uid FROM bookings WHERE event_type_id = ? LIMIT 1")
            .bind(&personal_et)
            .fetch_one(&pool)
            .await
            .unwrap();
    let personal_hosts = reschedule_hosts(&pool, &uid3, &owner).await;
    assert_eq!(personal_hosts.user_ids, vec![owner]);
    assert_eq!(personal_hosts.team_id, None);
    assert!(!personal_hosts.collective);
}

/// A collective booking cannot be rescheduled when its event type no
/// longer has an eligible member. The CalDAV write path falls back to the
/// event type owner for legacy events, but availability must not inherit
/// that fallback and invent a host outside the eligible roster.
#[tokio::test]
async fn reschedule_collective_without_eligible_members_has_no_available_slot() {
    let pool = setup_test_db().await;
    let alice = insert_role_user(&pool, "alice@empty166.test", "user").await;
    let bob = insert_role_user(&pool, "bob@empty166.test", "user").await;
    let (_team, et_id) = insert_team_with_et(
        &pool,
        "empty166",
        "empty166-et",
        &[(&alice, "member"), (&bob, "member")],
    )
    .await;
    sqlx::query("UPDATE event_types SET scheduling_mode = 'collective' WHERE id = ?")
        .bind(&et_id)
        .execute(&pool)
        .await
        .unwrap();
    for member in [&alice, &bob] {
        sqlx::query(
            "INSERT INTO event_type_member_weights (event_type_id, user_id, weight) \
                 VALUES (?, ?, 0)",
        )
        .bind(&et_id)
        .bind(member)
        .execute(&pool)
        .await
        .unwrap();
    }

    let start = NaiveDate::from_ymd_opt(2026, 3, 17)
        .unwrap()
        .and_hms_opt(10, 0, 0)
        .unwrap();
    insert_booking(&pool, &et_id, None, start, "confirmed").await;
    let uid: String =
        sqlx::query_scalar("SELECT uid FROM bookings WHERE event_type_id = ? LIMIT 1")
            .bind(&et_id)
            .fetch_one(&pool)
            .await
            .unwrap();

    let owner_fallback = "owner-fallback";
    assert_eq!(
        booking_write_targets(&pool, &uid, owner_fallback).await,
        vec![owner_fallback.to_string()],
        "CalDAV write-back keeps its legacy owner fallback"
    );

    let hosts = reschedule_hosts(&pool, &uid, owner_fallback).await;
    assert!(hosts.collective);
    assert!(
        hosts.user_ids.is_empty(),
        "availability must preserve the empty eligible roster"
    );

    let end = start + Duration::minutes(30);
    let busy = reschedule_busy_source(
        &pool,
        &hosts,
        &et_id,
        start,
        end,
        chrono_tz::Tz::UTC,
        "",
        "",
    )
    .await;
    assert!(
        !busy_source_is_free(&busy, start, end),
        "an empty collective roster must make every slot unavailable"
    );
}

/// A round-robin booking with no assigned member is a data anomaly — the
/// booking flow refuses the slot when `pick_group_member` finds nobody — but
/// pre-assignment rows exist. It must stay gated on the event type owner,
/// the fallback `booking_write_targets` has always applied. Resolving it to
/// "any member free" instead would let a guest reschedule onto a slot with
/// no host who is actually free.
#[tokio::test]
async fn reschedule_hosts_unassigned_round_robin_falls_back_to_the_owner() {
    let pool = setup_test_db().await;
    let (owner, _, _) = seed_test_data(&pool).await;
    let bob = insert_role_user(&pool, "bob@rrnull.test", "user").await;
    let (_team, et_id) = insert_team_with_et(
        &pool,
        "rrnull",
        "rrnull-et",
        &[(&owner, "member"), (&bob, "member")],
    )
    .await;
    let mode: String = sqlx::query_scalar("SELECT scheduling_mode FROM event_types WHERE id = ?")
        .bind(&et_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(mode, "round_robin", "the fixture must not be collective");

    let start = NaiveDate::from_ymd_opt(2026, 3, 17)
        .unwrap()
        .and_hms_opt(10, 0, 0)
        .unwrap();
    insert_booking(&pool, &et_id, None, start, "confirmed").await;
    let uid: String =
        sqlx::query_scalar("SELECT uid FROM bookings WHERE event_type_id = ? LIMIT 1")
            .bind(&et_id)
            .fetch_one(&pool)
            .await
            .unwrap();

    let hosts = reschedule_hosts(&pool, &uid, &owner).await;
    assert_eq!(
        hosts.user_ids,
        vec![owner.clone()],
        "no assigned member and not collective: the owner is the fallback host"
    );
    assert!(!hosts.collective);

    // One host, so Team's "all must be free" is "the owner must be free" —
    // the same gate as before #166, not a loosened any-member check.
    let busy = reschedule_busy_source(
        &pool,
        &hosts,
        &et_id,
        start,
        start + Duration::minutes(30),
        chrono_tz::Tz::UTC,
        "",
        "",
    )
    .await;
    match &busy {
        BusySource::Team(member_busy) => {
            assert_eq!(member_busy.len(), 1);
            assert!(member_busy.contains_key(&owner));
        }
        _ => panic!("a team event type must go through BusySource::Team"),
    }
}

/// The busy source a reschedule is checked against must see the assigned
/// member's bookings and ignore the owner's availability (#166).
#[tokio::test]
async fn reschedule_busy_source_uses_the_assigned_member_calendar() {
    let pool = setup_test_db().await;
    let (owner, _, _) = seed_test_data(&pool).await;
    let bob = insert_role_user(&pool, "bob@busy166.test", "user").await;
    let (_team, et_id) = insert_team_with_et(
        &pool,
        "busy166",
        "busy166-et",
        &[(&owner, "member"), (&bob, "member")],
    )
    .await;

    let taken = NaiveDate::from_ymd_opt(2026, 3, 17)
        .unwrap()
        .and_hms_opt(10, 0, 0)
        .unwrap();
    let taken_end = taken + Duration::minutes(30);
    // Bob is already booked at 10:00, the owner is free all day.
    insert_booking(&pool, &et_id, Some(&bob), taken, "confirmed").await;
    // The booking being rescheduled, assigned to Bob, sits elsewhere.
    insert_booking(
        &pool,
        &et_id,
        Some(&bob),
        taken + Duration::hours(4),
        "confirmed",
    )
    .await;
    let uid: String =
        sqlx::query_scalar("SELECT uid FROM bookings WHERE event_type_id = ? AND start_at = ?")
            .bind(&et_id)
            .bind(
                (taken + Duration::hours(4))
                    .format("%Y-%m-%dT%H:%M:%S")
                    .to_string(),
            )
            .fetch_one(&pool)
            .await
            .unwrap();

    // The pre-#166 check ran against the owner, who is free at 10:00 —
    // which is exactly how the guest got offered a slot Bob could not take.
    assert!(
        fetch_busy_times_for_user_ex(
            &pool,
            &owner,
            taken,
            taken_end,
            chrono_tz::Tz::UTC,
            Some(&et_id),
            None,
            None,
        )
        .await
        .is_empty(),
        "the event type owner is free at 10:00; only Bob is busy"
    );

    let hosts = reschedule_hosts(&pool, &uid, &owner).await;
    let busy = reschedule_busy_source(
        &pool,
        &hosts,
        &et_id,
        taken,
        taken_end,
        chrono_tz::Tz::UTC,
        "",
        "",
    )
    .await;
    assert!(
        !busy_source_is_free(&busy, taken, taken_end),
        "the slot must be busy: it is taken on the assigned member's calendar"
    );
}

/// The reschedule page's host profile follows the booking too: the guest
/// must not be shown the event type owner as their host (#166).
#[tokio::test]
async fn reschedule_host_profile_shows_the_booking_host() {
    let pool = setup_test_db().await;
    let (owner, _, _) = seed_test_data(&pool).await;
    let bob = insert_role_user(&pool, "bob@profile166.test", "user").await;
    let (team, et_id) = insert_team_with_et(
        &pool,
        "profile166",
        "profile166-et",
        &[(&owner, "member"), (&bob, "member")],
    )
    .await;
    sqlx::query("UPDATE teams SET name = 'Support' WHERE id = ?")
        .bind(&team)
        .execute(&pool)
        .await
        .unwrap();
    let start = NaiveDate::from_ymd_opt(2026, 3, 17)
        .unwrap()
        .and_hms_opt(10, 0, 0)
        .unwrap();

    insert_booking(&pool, &et_id, Some(&bob), start, "confirmed").await;
    let uid: String =
        sqlx::query_scalar("SELECT uid FROM bookings WHERE event_type_id = ? LIMIT 1")
            .bind(&et_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    let assigned_hosts = reschedule_hosts(&pool, &uid, &owner).await;
    let profile = reschedule_host_profile(&pool, &assigned_hosts, &et_id).await;
    assert_eq!(profile.name, "bob@profile166.test");
    assert_eq!(profile.user_id.as_deref(), Some(bob.as_str()));
    assert!(
        profile.team.is_none(),
        "a round-robin booking is with one person"
    );

    // Collective: the booking belongs to the team, so the team card renders.
    sqlx::query("UPDATE event_types SET scheduling_mode = 'collective' WHERE id = ?")
        .bind(&et_id)
        .execute(&pool)
        .await
        .unwrap();
    insert_booking(&pool, &et_id, None, start + Duration::hours(2), "confirmed").await;
    let uid2: String = sqlx::query_scalar(
        "SELECT uid FROM bookings WHERE event_type_id = ? AND assigned_user_id IS NULL",
    )
    .bind(&et_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    let collective_hosts = reschedule_hosts(&pool, &uid2, &owner).await;
    let team_profile = reschedule_host_profile(&pool, &collective_hosts, &et_id).await;
    assert_eq!(team_profile.name, "Support");
    assert_eq!(team_profile.user_id, None);
    assert_eq!(
        team_profile.team.map(|t| t.members.len()),
        Some(2),
        "the team card lists the eligible roster"
    );
}
