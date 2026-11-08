//! Calendar write-back of bookings (CalDAV/EWS) and shared-resource reservations.

use super::*;

// --- CalDAV write-back ---

/// Push a confirmed booking to the calendar(s) it belongs to: the assigned
/// member's, every member's for collective team event types, or the given
/// user's for personal bookings (see [`booking_write_targets`]).
pub(super) async fn caldav_push_booking(
    pool: &SqlitePool,
    key: &[u8; 32],
    user_id: &str,
    booking_uid: &str,
    details: &crate::email::BookingDetails,
) {
    // Records the outcome on the booking: a failure is retried from the
    // background loop and shown to the host (#162).
    writeback::push_and_record(
        pool,
        key,
        user_id,
        booking_uid,
        details,
        writeback::PushKind::Fresh,
    )
    .await;
}

/// Email the host when the Google Meet time patch failed for good.
///
/// `details` carries the booking's host identity, and the skip branch only
/// runs when that person is also the Meet owner (`should_skip_caldav_put`
/// requires `elect_meet_owner == user_id`), so this reaches the calendar that
/// is actually out of date.
async fn notify_host_meet_time_desync(
    pool: &SqlitePool,
    key: &[u8; 32],
    booking_uid: &str,
    details: &crate::email::BookingDetails,
    reason: &str,
) {
    // Google error bodies can be long JSON; the host needs the gist, not the payload.
    let reason: String = reason.chars().take(300).collect();
    match crate::email::load_smtp_config(pool, key).await {
        Ok(Some(smtp)) => {
            // The details come from the CalDAV push path, which may not
            // carry the host's language: take it from the booking's host,
            // the user `host_email` belongs to (#209).
            let mut details = details.clone();
            if details.host_language.is_none() {
                details.host_language = booking_host_identity(pool, booking_uid)
                    .await
                    .and_then(|(_, _, language)| language);
            }
            if let Err(e) =
                crate::email::send_host_calendar_sync_failure(&smtp, &details, &reason).await
            {
                tracing::error!(
                    error = %e,
                    uid = %booking_uid,
                    "google meet: could not email the host about the calendar time desync"
                );
            }
        }
        _ => {
            tracing::warn!(
                uid = %booking_uid,
                "google meet: host calendar left at the old time and SMTP is unconfigured, so nobody can be told"
            );
        }
    }
}

/// Whose copy of the booking a push writes. Only the host's copy is tracked
/// in `bookings.caldav_calendar_href`; a claimant's copy is found again by
/// uid on their write sources (#269).
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum CopyOwner {
    Host,
    Claimant,
}

/// Push a confirmed booking to ONE user's calendar(s). For the host's copy
/// (`CopyOwner::Host`) that is the calendar they chose for the booking's
/// event type, else the write calendar of each of their sources (see
/// [`host_write_sources`]); a claimant's copy always goes to the source
/// setting. Most callers want [`caldav_push_booking`], which resolves the
/// right user(s) first and writes the claimant's copy too.
///
/// Returns one message per source the booking could not be written to (or
/// written but not read back), empty when every write landed or the user
/// has nothing to write to.
pub(super) async fn caldav_push_booking_for_user(
    pool: &SqlitePool,
    key: &[u8; 32],
    user_id: &str,
    booking_uid: &str,
    details: &crate::email::BookingDetails,
    owner: CopyOwner,
    skip: &[Collection],
) -> Vec<String> {
    let (sources, plan) = match owner {
        CopyOwner::Host => host_write_sources(pool, booking_uid, user_id).await,
        CopyOwner::Claimant => (
            default_write_sources(pool, user_id).await,
            WritePlan::Source,
        ),
    };
    push_to_sources(
        pool,
        key,
        user_id,
        booking_uid,
        details,
        owner,
        skip,
        &sources,
        plan,
    )
    .await
    .into_iter()
    .map(|failure| failure.message)
    .collect()
}

/// One source a push could not write to.
pub(super) struct PushFailure {
    /// "source name: error", as stored on the booking.
    pub(super) message: String,
    /// The calendar refused the write for good (see
    /// `WriteBackError::permanent_refusal`): its short description.
    pub(super) refusal: Option<String>,
}

/// [`caldav_push_booking_for_user`] once the sources are resolved.
#[allow(clippy::too_many_arguments)]
pub(super) async fn push_to_sources(
    pool: &SqlitePool,
    key: &[u8; 32],
    user_id: &str,
    booking_uid: &str,
    details: &crate::email::BookingDetails,
    owner: CopyOwner,
    skip: &[Collection],
    sources: &[WriteSourceRow],
    plan: WritePlan,
) -> Vec<PushFailure> {
    let mut failures = Vec::new();
    let track = owner == CopyOwner::Host && plan.tracks_copies();

    if sources.is_empty() {
        // Distinguish "user has no sources" (debug) from "user has sources but none
        // are write-configured" (warn). The second case is almost always a misconfig.
        let unconfigured: i64 = sqlx::query_scalar(&format!(
            "SELECT COUNT(*) FROM caldav_sources cs
             JOIN accounts a ON a.id = cs.account_id
             WHERE a.user_id = ? AND cs.enabled = 1 AND cs.write_calendar_href IS NULL AND {}",
            crate::ews::global::ACTIVE_SOURCE_SQL
        ))
        .bind(user_id)
        .fetch_one(pool)
        .await
        .unwrap_or(0);

        if unconfigured > 0 {
            tracing::warn!(
                user_id = %user_id,
                uid = %booking_uid,
                unconfigured_sources = unconfigured,
                "calendar write-back skipped: booking confirmed but no source has a write calendar selected. Pick one at /dashboard/sources",
            );
        } else {
            // Not debug: for team bookings this is the assigned member,
            // and members usually expect their calendar to be written.
            // Silence here is what made #147 hard to diagnose.
            tracing::warn!(user_id = %user_id, uid = %booking_uid, "calendar write-back skipped: no enabled CalDAV source for this user");
        }
        return failures;
    }

    let ics = crate::email::generate_ics_caldav(details);
    // One Google copy of the booking carries the guests as attendees (#237):
    // Google adds an event to each attendee's own Google calendar, so a
    // second copy with attendees would show them the meeting twice. The
    // others are written without ATTENDEE lines, which also clears, without
    // a word to anyone, attendees an earlier write left on them.
    let is_google = |auth_type: &str, provider: Option<&str>| {
        auth_type == "oauth2" && provider == Some("google")
    };
    let writes_to_google =
        sources
            .iter()
            .any(|(_, _, _, _, _, auth_type, _, _, _, provider, _)| {
                is_google(auth_type, provider.as_deref())
            });
    // This user's copy carries them when they hold the booking's attendee
    // copy: on their Google row with the lowest source id among the rows
    // this push writes (the chosen destination, the Meet's calendar, or the
    // first Google source of the source setting).
    let attendee_source: Option<String> = match owner {
        CopyOwner::Host if writes_to_google => {
            let holder = crate::google_calendar::attendee_copy_holder(pool, booking_uid).await;
            if holder.as_deref() == Some(user_id) {
                // The rule the Meet source follows (`google_meet`): a token,
                // and a write calendar, so guests and Meet share one copy.
                sources
                    .iter()
                    .filter(|row| is_google(&row.5, row.9.as_deref()))
                    .filter(|row| row.6.as_deref().is_some_and(|t| !t.is_empty()))
                    .filter(|row| !row.4.trim().is_empty())
                    .map(|row| row.0.clone())
                    .min()
            } else {
                None
            }
        }
        _ => None,
    };
    let without_attendees = crate::google_calendar::split_scheduling(&ics).0;
    // Google shares that copy with every attendee, description included,
    // so it leaves out the guest's phone, which additional attendees never
    // see (#261); the copies without attendees keep it for the host. A
    // claimant listed on it would get it on their Google Calendar from
    // Google, beside the copy calrs writes for them (#269): when they have
    // one, they are left off. Without one they stay, and rely on it.
    let attendee_ics = match &attendee_source {
        Some(_) => {
            let shared = crate::email::generate_ics_caldav(&crate::email::BookingDetails {
                guest_phone: None,
                ..details.clone()
            });
            let targets = booking_write_targets(pool, booking_uid, user_id).await;
            match claimant_with_own_copy(pool, booking_uid, &targets).await {
                Some((claimant, _)) => crate::google_calendar::without_attendees_named(
                    &shared,
                    &claimant_attendee_addresses(pool, booking_uid, &claimant).await,
                ),
                None => shared,
            }
        }
        None => String::new(),
    };
    let mut preserved_meet_href = false;

    for (
        source_id,
        url,
        username,
        password_enc,
        calendar_href,
        auth_type,
        access_token_enc,
        token_expires_at,
        provider_type,
        oauth2_provider,
        source_name,
    ) in sources
    {
        if crate::google_meet::should_skip_caldav_put(
            pool,
            booking_uid,
            user_id,
            source_id,
            auth_type,
            oauth2_provider.as_deref(),
        )
        .await
        {
            let stored_href: Option<Option<String>> =
                sqlx::query_scalar("SELECT caldav_calendar_href FROM bookings WHERE uid = ?")
                    .bind(booking_uid)
                    .fetch_optional(pool)
                    .await
                    .ok()
                    .flatten();
            // First confirm: `create_meet_for_booking` just PUT the event at
            // these times. Patching immediately is a wasted Calendar round-trip
            // and a spurious warning when the iCalUID index is still stale.
            // Reschedule / re-push already have an href and need the patch.
            if stored_href
                .flatten()
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .is_some()
            {
                if let Err(e) = crate::google_meet::patch_owner_event_times(
                    pool,
                    key,
                    user_id,
                    booking_uid,
                    details,
                    &crate::google_meet::LiveGoogleMeetApi,
                )
                .await
                {
                    tracing::warn!(
                        error = %e,
                        uid = %booking_uid,
                        "google meet: skipped CalDAV PUT but could not patch event times"
                    );
                    // Retries are exhausted by now. The booking, the guest's
                    // invite and the reminders all hold the new time while the
                    // host's calendar holds the old one, and this Google event
                    // is the only copy the host has -- a log line would leave
                    // them to discover it at the old time.
                    notify_host_meet_time_desync(pool, key, booking_uid, details, &e.to_string())
                        .await;
                } else {
                    tracing::info!(
                        uid = %booking_uid,
                        calendar_href = %calendar_href,
                        "google meet: patched event times, skipped CalDAV PUT to preserve conferenceData"
                    );
                }
            }

            // The event *is* on this calendar -- `create_meet_for_booking`
            // PUT it there before attaching the conference. Record the href
            // exactly as the PUT path below does, or the booking looks like
            // it was never pushed: cancellation skips the delete and leaves
            // a ghost event (with its Meet) blocking the host's own
            // availability, the pending-reschedule path never clears the old
            // time, and `cancel_orphaned_bookings` cannot see the booking at
            // all. Recorded even when the time patch failed: the patch says
            // nothing about whether the event exists.
            if owner == CopyOwner::Host {
                let _ = sqlx::query(
                    "UPDATE bookings SET caldav_calendar_href = ?, caldav_source_id = ? WHERE uid = ?",
                )
                .bind(calendar_href)
                .bind(source_id)
                .bind(booking_uid)
                .execute(pool)
                .await;
            }
            if track {
                record_copy(pool, booking_uid, user_id, source_id, calendar_href).await;
            }
            // Later PUTs to other calendars must not overwrite this: cancel
            // keys off one href, and that has to be the Meet-holding calendar.
            preserved_meet_href = true;
            // No PUT here, so nothing else would notice that minting the
            // Meet could not add the guests: check, and leave a failure
            // for the write-back retry when they are still missing.
            if attendee_source.as_deref() == Some(source_id.as_str()) {
                let ensured = match crate::oauth2_caldav::build_client_for_source(
                    pool,
                    key,
                    source_id,
                    url,
                    auth_type,
                    username,
                    password_enc.as_deref(),
                    access_token_enc.as_deref(),
                    token_expires_at.as_deref(),
                )
                .await
                {
                    Ok(client) => {
                        client
                            .ensure_google_attendees(calendar_href, booking_uid, &attendee_ics)
                            .await
                    }
                    Err(e) => Err(e),
                };
                if let Err(e) = ensured {
                    tracing::warn!(uid = %booking_uid, error = format!("{e:#}"), "google meet: the Meet event still lacks its attendees");
                    failures.push(PushFailure {
                        message: format!("{source_name}: {e:#}"),
                        refusal: None,
                    });
                }
            }
            continue;
        }

        let collection = (crate::resources::url_origin(url), calendar_href.clone());
        if skip.contains(&collection) {
            continue;
        }

        tracing::debug!(uid = %booking_uid, calendar_href = %calendar_href, provider = %provider_type, "pushing booking to calendar");

        let provider = match write_provider_for_source(
            pool,
            key,
            source_id,
            url,
            username,
            password_enc.as_deref(),
            auth_type,
            access_token_enc.as_deref(),
            token_expires_at.as_deref(),
            provider_type,
        )
        .await
        {
            Ok(Some(p)) => p,
            // Became idle since the query above: skipped, not failed.
            Ok(None) => continue,
            Err(e) => {
                tracing::error!(url = %url, error = format!("{e:#}"), "calendar write-back failed: could not build client");
                failures.push(PushFailure {
                    message: format!("{source_name}: {e:#}"),
                    refusal: None,
                });
                continue;
            }
        };

        // A 2xx on the write is not proof the host sees the event (#162):
        // read it back by UID before calling it written.
        let google = is_google(auth_type, oauth2_provider.as_deref());
        let source_ics = if !google {
            &ics
        } else if attendee_source.as_deref() == Some(source_id.as_str()) {
            &attendee_ics
        } else {
            &without_attendees
        };
        let written = crate::providers::put_and_verify(
            provider.as_ref(),
            calendar_href,
            booking_uid,
            source_ics,
        )
        .await;
        let event_may_exist = match &written {
            Ok(()) => true,
            Err(e) => e.event_may_exist(),
        };

        // Record which calendar href the booking was pushed to (last successful
        // one), unless a Meet skip already pinned the Meet-holding calendar.
        // Also recorded when the write was accepted but could not be read
        // back: the event may still be there, and cancellation keys its
        // delete off this href. A claimant's copy must not take the host's
        // place here, or the host's delete would look in the wrong calendar.
        // A write to a chosen destination that could not be verified does
        // not take the href from a copy known to exist (#47): the copy is
        // recorded below, so a cancellation still finds it either way.
        let keeps_known_copy = track
            && written.is_err()
            && sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM bookings WHERE uid = ? AND caldav_calendar_href IS NOT NULL",
            )
            .bind(booking_uid)
            .fetch_one(pool)
            .await
            .unwrap_or(0)
                > 0;
        if event_may_exist && !preserved_meet_href && owner == CopyOwner::Host && !keeps_known_copy
        {
            let _ = sqlx::query(
                "UPDATE bookings SET caldav_calendar_href = ?, caldav_source_id = ? WHERE uid = ?",
            )
            .bind(calendar_href)
            .bind(source_id)
            .bind(booking_uid)
            .execute(pool)
            .await;
        }
        // A copy written to a chosen destination is recorded, so a later
        // push, cancellation or reschedule finds it there (#47).
        if event_may_exist && track {
            record_copy(pool, booking_uid, user_id, source_id, calendar_href).await;
        }

        match written {
            Ok(()) => {
                tracing::info!(uid = %booking_uid, calendar_href = %calendar_href, "calendar write-back succeeded (verified)");
            }
            Err(e) => {
                tracing::error!(uid = %booking_uid, calendar_href = %calendar_href, error = %e, "calendar write-back failed");
                failures.push(PushFailure {
                    message: format!("{source_name}: {e}"),
                    refusal: e.permanent_refusal().map(str::to_string),
                });
            }
        }
    }
    failures
}

/// A calendar client for one of a user's sources, whatever its protocol:
/// Microsoft Graph through `build_provider_for_source` (token refresh), EWS
/// through the provider factory, CalDAV (basic or OAuth2) through
/// `build_client_for_source`. `Ok(None)` for a managed Exchange source with
/// nothing to do (see [`crate::ews::global::provider_for_source`]).
#[allow(clippy::too_many_arguments)]
pub(super) async fn write_provider_for_source(
    pool: &SqlitePool,
    key: &[u8; 32],
    source_id: &str,
    url: &str,
    username: &str,
    password_enc: Option<&str>,
    auth_type: &str,
    access_token_enc: Option<&str>,
    token_expires_at: Option<&str>,
    provider_type: &str,
) -> anyhow::Result<Option<Box<dyn crate::providers::CalendarProvider>>> {
    if provider_type == crate::providers::factory::kinds::MICROSOFT {
        return crate::microsoft_oauth::build_provider_for_source(pool, key, source_id)
            .await
            .map(Some);
    }
    if provider_type == crate::providers::factory::kinds::EWS {
        return crate::ews::global::provider_for_source(
            pool,
            key,
            source_id,
            url,
            username,
            password_enc,
        )
        .await;
    }
    let client = crate::oauth2_caldav::build_client_for_source(
        pool,
        key,
        source_id,
        url,
        auth_type,
        username,
        password_enc,
        access_token_enc,
        token_expires_at,
    )
    .await?;
    Ok(Some(Box::new(
        crate::providers::caldav::CaldavProvider::from_client(client),
    )))
}

/// Delete a booking from a user's calendars by uid: the write calendar of
/// each of their sources, and every copy recorded at a destination they
/// chose (#47). Used for collective bookings, where no single stored href
/// describes every member's copy, and as the fallback when the stored href
/// matches no source.
async fn caldav_delete_for_user(
    pool: &SqlitePool,
    key: &[u8; 32],
    user_id: &str,
    booking_uid: &str,
) {
    let tracked = delete_tracked_copies(pool, key, user_id, booking_uid).await;
    for row in default_write_sources(pool, user_id).await {
        // Same place already tried: the same href on another server is not.
        if tracked
            .iter()
            .any(|(sid, href)| *sid == row.0 && *href == row.4)
        {
            continue;
        }
        delete_from(pool, key, &row, booking_uid, DeleteKind::Ended).await;
    }

    // Remove cached event
    let _ = sqlx::query(
        "DELETE FROM events WHERE uid = ? AND calendar_id IN (
            SELECT c.id FROM calendars c
            JOIN caldav_sources cs ON cs.id = c.source_id
            JOIN accounts a ON a.id = cs.account_id
            WHERE a.user_id = ?
        )",
    )
    .bind(booking_uid)
    .bind(user_id)
    .execute(pool)
    .await;
}

/// The user who claimed a booking, when their copy of it is calrs's to
/// write and delete (#269): not when they own its Google Meet event, the
/// only copy of the meeting on their calendar.
pub(super) async fn booking_claimant(pool: &SqlitePool, booking_uid: &str) -> Option<String> {
    let claimant: String =
        sqlx::query_scalar("SELECT claimed_by_user_id FROM bookings WHERE uid = ?")
            .bind(booking_uid)
            .fetch_optional(pool)
            .await
            .unwrap_or(None)
            .flatten()?;
    if crate::google_meet::holds_the_meet(pool, booking_uid, &claimant).await {
        return None;
    }
    Some(claimant)
}

/// The claimant of a booking whose own copy calrs writes (#269): one who
/// is not a host and has a write collection no host writes to (a shared
/// collection already holds the host's event). Returned with the hosts'
/// collections, which their copy skips. `None` when no claimant copy is
/// written.
pub(super) async fn claimant_with_own_copy(
    pool: &SqlitePool,
    booking_uid: &str,
    targets: &[String],
) -> Option<(String, Vec<Collection>)> {
    let claimant = booking_claimant(pool, booking_uid)
        .await
        .filter(|c| !targets.contains(c))?;
    // Where the hosts' copies go, chosen destinations included (#47), as
    // the push computes it.
    let mut host_collections = Vec::new();
    for target in targets {
        let (rows, _) = host_write_sources(pool, booking_uid, target).await;
        host_collections.extend(rows.iter().map(row_collection));
    }
    // The claimant's copy goes to their source setting (`push_claimant_copy`).
    default_write_sources(pool, &claimant)
        .await
        .iter()
        .any(|row| !host_collections.contains(&row_collection(row)))
        .then_some((claimant, host_collections))
}

/// The addresses the claimant of a booking is listed under: the attendee
/// row their claim added (found by user, #350), and their current address.
async fn claimant_attendee_addresses(
    pool: &SqlitePool,
    booking_uid: &str,
    claimant: &str,
) -> Vec<String> {
    sqlx::query_scalar(
        "SELECT ba.email FROM booking_attendees ba
         JOIN bookings b ON b.id = ba.booking_id
         WHERE b.uid = ? AND ba.claimant_user_id = ?
         UNION
         SELECT COALESCE(booking_email, email) FROM users WHERE id = ?",
    )
    .bind(booking_uid)
    .bind(claimant)
    .bind(claimant)
    .fetch_all(pool)
    .await
    .unwrap_or_default()
}

/// A server origin (scheme, host, port) and a collection on it.
pub(super) type Collection = (Option<(String, String, Option<u16>)>, String);

/// Delete the copy of a booking a watcher's claim put on the claimant's own
/// calendar (#269), by uid from their current write sources, like the
/// host's delete when its stored href no longer matches one. A copy left in
/// a calendar the claimant has since stopped writing to stays there, as the
/// host's would. A no-op for a booking nobody claimed.
pub(crate) async fn claimant_delete_booking(pool: &SqlitePool, key: &[u8; 32], booking_uid: &str) {
    if let Some(claimant) = booking_claimant(pool, booking_uid).await {
        caldav_delete_for_user(pool, key, &claimant, booking_uid).await;
    }
}

/// Delete a booking from the host's CalDAV calendar, and the claimant's
/// copy if the booking was claimed.
pub(crate) async fn caldav_delete_booking(
    pool: &SqlitePool,
    key: &[u8; 32],
    user_id: &str,
    booking_uid: &str,
) {
    // First, and whatever happens below: the host may have no write
    // calendar while the claimant has one.
    claimant_delete_booking(pool, key, booking_uid).await;

    // Check if this booking was pushed to CalDAV
    let info: Option<(String, Option<String>)> = sqlx::query_as(
        "SELECT caldav_calendar_href, caldav_source_id FROM bookings WHERE uid = ? AND caldav_calendar_href IS NOT NULL",
    )
    .bind(booking_uid)
    .fetch_optional(pool)
    .await
    .unwrap_or(None);

    // Resolve whose calendar the event actually lives on (#147): the
    // assigned member's, every member's for collective, else the caller's.
    let targets = booking_write_targets(pool, booking_uid, user_id).await;

    // Copies written to a chosen destination are found where they were
    // written, whatever the destination is now (#47), including those of
    // members who no longer host the booking (removed, excluded, disabled).
    let mut tracked = Vec::new();
    let mut holders = targets.clone();
    for user in copy_users(pool, booking_uid).await {
        if !holders.contains(&user) {
            holders.push(user);
        }
    }
    for holder in &holders {
        tracked.extend(delete_tracked_copies(pool, key, holder, booking_uid).await);
    }

    let (calendar_href, stored_source) = match info {
        Some((href, source)) => (href, source),
        None => {
            tracing::debug!(uid = %booking_uid, "calendar delete skipped: booking was never pushed");
            return;
        }
    };

    if targets.len() > 1 {
        // Collective: one stored href cannot describe N member calendars;
        // delete by uid from every member's write sources instead.
        for target in &targets {
            caldav_delete_for_user(pool, key, target, booking_uid).await;
        }
        return;
    }
    let target = targets.first().map(String::as_str).unwrap_or(user_id);

    // Get the source credentials and provider type
    type SourceRow = (
        String,
        String,
        String,
        Option<String>,
        String,
        Option<String>,
        Option<String>,
        String,
    );
    /// The user's enabled source the href is on: the one stored with it
    /// (#47), else, for bookings written before, the source whose write
    /// calendar it is.
    async fn find_source(
        pool: &SqlitePool,
        user: &str,
        href: &str,
        source_id: Option<&str>,
    ) -> Option<SourceRow> {
        if let Some(source_id) = source_id {
            return sqlx::query_as(
                "SELECT cs.id, cs.url, cs.username, cs.password_enc, cs.auth_type, cs.access_token_enc, cs.token_expires_at, cs.provider_type
                 FROM caldav_sources cs
                 JOIN accounts a ON a.id = cs.account_id
                 WHERE a.user_id = ? AND cs.enabled = 1 AND cs.id = ?",
            )
            .bind(user)
            .bind(source_id)
            .fetch_optional(pool)
            .await
            .unwrap_or(None);
        }
        sqlx::query_as(
            "SELECT cs.id, cs.url, cs.username, cs.password_enc, cs.auth_type, cs.access_token_enc, cs.token_expires_at, cs.provider_type
             FROM caldav_sources cs
             JOIN accounts a ON a.id = cs.account_id
             WHERE a.user_id = ? AND cs.enabled = 1 AND cs.write_calendar_href = ?
             LIMIT 1",
        )
        .bind(user)
        .bind(href)
        .fetch_optional(pool)
        .await
        .unwrap_or(None)
    }

    let stored_source = stored_source.as_deref();
    let mut cache_user = target;
    let mut source = find_source(pool, target, &calendar_href, stored_source).await;
    if source.is_none() && target != user_id {
        // Pre-#147 bookings were pushed to the event type owner's
        // calendar; the stored href then belongs to them, not the
        // assigned member. Clean up where the event actually is.
        source = find_source(pool, user_id, &calendar_href, stored_source).await;
        if source.is_some() {
            cache_user = user_id;
        }
    }

    let (
        source_id,
        url,
        username,
        password_enc,
        auth_type,
        access_token_enc,
        token_expires_at,
        provider_type,
    ) = match source {
        Some(s) => s,
        None => {
            // The href no longer matches any write source (write calendar
            // changed since the push). Best effort: delete by uid from the
            // target's current write sources.
            if tracked.iter().any(|(_, href)| *href == calendar_href) {
                tracing::debug!(uid = %booking_uid, user_id = %target, href = %calendar_href, "calendar delete: the stored href was a recorded copy; deleting by uid from the source write calendars too");
            } else {
                tracing::warn!(uid = %booking_uid, user_id = %target, href = %calendar_href, "calendar delete: stored href matches no write source, falling back to delete-by-uid");
            }
            caldav_delete_for_user(pool, key, target, booking_uid).await;
            return;
        }
    };

    let delete_result = match write_provider_for_source(
        pool,
        key,
        &source_id,
        &url,
        &username,
        password_enc.as_deref(),
        &auth_type,
        access_token_enc.as_deref(),
        token_expires_at.as_deref(),
        &provider_type,
    )
    .await
    {
        Ok(Some(client)) => client.delete_event(&calendar_href, booking_uid).await,
        // A managed Exchange source with nothing to do.
        Ok(None) => return,
        Err(error) => Err(error),
    };
    if let Err(e) = delete_result {
        tracing::error!(uid = %booking_uid, error = %e, "calendar event delete failed");
    }
    // A push under the source setting writes the booking to the write
    // calendar of every source of the host, not only the one stored (a
    // CalDAV source and a Microsoft one, say), and those copies are not
    // recorded: delete them by uid too, skipping each place already tried
    // (the stored one, and the recorded copies deleted above), as
    // `caldav_delete_for_user` does.
    for row in default_write_sources(pool, cache_user).await {
        let tried = (row.0 == source_id && row.4 == calendar_href)
            || tracked
                .iter()
                .any(|(sid, href)| *sid == row.0 && *href == row.4);
        if !tried {
            delete_from(pool, key, &row, booking_uid, DeleteKind::Ended).await;
        }
    }

    // Also remove the cached event from local DB so it doesn't block availability
    let _ = sqlx::query(
        "DELETE FROM events WHERE uid = ? AND calendar_id IN (
            SELECT c.id FROM calendars c
            JOIN caldav_sources cs ON cs.id = c.source_id
            JOIN accounts a ON a.id = cs.account_id
            WHERE a.user_id = ?
        )",
    )
    .bind(booking_uid)
    .bind(cache_user)
    .execute(pool)
    .await;
}

// --- Shared resource reservation (write-back) ---

/// Resources a booking must reserve: its assigned resource in round-robin
/// mode, every attached resource in 'all' mode. Tuple: (resource_id,
/// caldav_url or derived, caldav_username, caldav_password_enc).
async fn booking_resources_to_reserve(
    pool: &SqlitePool,
    booking_uid: &str,
    for_release: bool,
) -> Vec<(String, Option<String>, Option<String>, Option<String>)> {
    let booking: Option<(String, Option<String>, String)> = sqlx::query_as(
        "SELECT b.event_type_id, b.assigned_resource_id, et.resource_scheduling_mode
         FROM bookings b JOIN event_types et ON et.id = b.event_type_id
         WHERE b.uid = ?",
    )
    .bind(booking_uid)
    .fetch_optional(pool)
    .await
    .unwrap_or(None);
    let Some((et_id, assigned, mode)) = booking else {
        return Vec::new();
    };

    // The booking's stored assignment is fetched directly, NOT through the
    // junction, so a resource that was detached from the event type after
    // the reservation was written still gets released. The junction list
    // covers 'all' mode.
    let mut rows: Vec<(
        String,
        String,
        Option<String>,
        Option<String>,
        Option<String>,
    )> = Vec::new();
    if let Some(rid) = &assigned {
        if let Some(row) = sqlx::query_as(
            "SELECT id, feed_url, caldav_url, caldav_username, caldav_password
             FROM resources WHERE id = ?",
        )
        .bind(rid)
        .fetch_optional(pool)
        .await
        .unwrap_or(None)
        {
            rows.push(row);
        }
    }
    // Push targets are mode-aware (round_robin reserves only the assigned
    // resource). Release targets ignore the CURRENT mode: it may have been
    // flipped since the reservation was written, and deleting a uid that
    // was never pushed to a resource is a tolerated no-op.
    if for_release || mode != "round_robin" {
        let attached: Vec<(
            String,
            String,
            Option<String>,
            Option<String>,
            Option<String>,
        )> = sqlx::query_as(
            "SELECT r.id, r.feed_url, r.caldav_url, r.caldav_username, r.caldav_password
             FROM resources r
             JOIN event_type_resources etr ON etr.resource_id = r.id
             WHERE etr.event_type_id = ?",
        )
        .bind(&et_id)
        .fetch_all(pool)
        .await
        .unwrap_or_default();
        for row in attached {
            if !rows.iter().any(|(id, _, _, _, _)| *id == row.0) {
                rows.push(row);
            }
        }
    }

    rows.into_iter()
        .map(|(id, feed_url, caldav_url, user, pw)| {
            let url = caldav_url
                .filter(|u| !u.is_empty())
                .or_else(|| crate::resources::derive_caldav_url(&feed_url));
            (id, url, user, pw)
        })
        .collect()
}

/// Credential candidates for writing to a resource calendar, in trust order:
/// the resource's own service account first, then members who opted in
/// (`users.lend_resource_write`) and have a CalDAV source on the same server
/// origin, preferring the booking's assigned host.
pub(super) async fn resource_write_candidates(
    pool: &SqlitePool,
    key: &[u8; 32],
    caldav_url: &str,
    service_username: Option<&str>,
    service_password_enc: Option<&str>,
    preferred_user_id: Option<&str>,
) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();

    if let (Some(u), Some(enc)) = (service_username, service_password_enc) {
        if !u.is_empty() {
            if let Ok(p) = crate::crypto::decrypt_password(key, enc) {
                out.push((u.to_string(), p));
            }
        }
    }

    let Some(origin) = crate::resources::url_origin(caldav_url) else {
        return out;
    };

    let mut members: Vec<(String, String, Option<String>, String)> = sqlx::query_as(
        "SELECT u.id, cs.username, cs.password_enc, cs.url
         FROM users u
         JOIN accounts a ON a.user_id = u.id
         JOIN caldav_sources cs ON cs.account_id = a.id
         WHERE u.lend_resource_write = 1 AND u.enabled = 1 AND cs.enabled = 1
           AND cs.password_enc IS NOT NULL",
    )
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    // Assigned host first, then stable order.
    members.sort_by_key(|(uid, _, _, _)| Some(uid.as_str()) != preferred_user_id);
    for (_, username, password_enc, source_url) in members {
        if crate::resources::url_origin(&source_url).as_ref() != Some(&origin) {
            continue;
        }
        if let Some(enc) = password_enc.as_deref() {
            if let Ok(p) = crate::crypto::decrypt_password(key, enc) {
                out.push((username, p));
            }
        }
    }
    out
}

/// Push the booking into each required resource calendar (reservation).
/// Uses the booking's own uid, so a re-push (reschedule) overwrites in
/// place. Failure is non-fatal: the booking stays confirmed and calrs' own
/// DB check keeps blocking the resource; the failure is logged for admins.
pub(super) async fn resource_push_booking(
    pool: &SqlitePool,
    key: &[u8; 32],
    booking_uid: &str,
    details: &crate::email::BookingDetails,
) {
    let targets = booking_resources_to_reserve(pool, booking_uid, false).await;
    if targets.is_empty() {
        return;
    }
    let preferred: Option<String> =
        sqlx::query_scalar("SELECT assigned_user_id FROM bookings WHERE uid = ?")
            .bind(booking_uid)
            .fetch_optional(pool)
            .await
            .unwrap_or(None)
            .flatten();
    // A resource calendar is shared, often published as a public feed: the
    // reservation carries the slot, never what the guest typed.
    let ics = crate::email::generate_ics_caldav(&crate::email::BookingDetails {
        notes: None,
        guest_phone: None,
        ..details.clone()
    });

    for (resource_id, caldav_url, service_user, service_pw) in &targets {
        let Some(url) = caldav_url else {
            tracing::debug!(resource_id = %resource_id, "resource reservation skipped: read-only (no CalDAV URL)");
            continue;
        };
        let candidates = resource_write_candidates(
            pool,
            key,
            url,
            service_user.as_deref(),
            service_pw.as_deref(),
            preferred.as_deref(),
        )
        .await;
        if candidates.is_empty() {
            tracing::warn!(resource_id = %resource_id, uid = %booking_uid, "resource reservation skipped: no write credentials available");
            continue;
        }
        let mut written = false;
        for (username, password) in &candidates {
            let client = crate::caldav::CaldavClient::new(url, username, password);
            match client.put_event(url, booking_uid, &ics).await {
                Ok(()) => {
                    tracing::info!(resource_id = %resource_id, uid = %booking_uid, writer = %username, "resource reservation written");
                    written = true;
                    break;
                }
                Err(e) => {
                    tracing::warn!(resource_id = %resource_id, writer = %username, error = %e, "resource reservation write failed, trying next credentials");
                }
            }
        }
        if !written {
            tracing::error!(resource_id = %resource_id, uid = %booking_uid, "resource reservation failed with all credentials; booking stays confirmed, resource calendar not updated");
        }
    }
}

/// Delete the booking's reservation from each required resource calendar.
/// Deletes by uid, so it works regardless of which credentials originally
/// wrote the event.
/// Human label of the resources reserved for a booking, for host-facing
/// emails: the assigned resource's name in round-robin mode, the attached
/// resource names in 'all' mode. None when the event type uses no
/// resources.
pub(super) async fn booking_resource_label(pool: &SqlitePool, booking_uid: &str) -> Option<String> {
    let booking: Option<(String, Option<String>, String)> = sqlx::query_as(
        "SELECT b.event_type_id, b.assigned_resource_id, et.resource_scheduling_mode
         FROM bookings b JOIN event_types et ON et.id = b.event_type_id
         WHERE b.uid = ?",
    )
    .bind(booking_uid)
    .fetch_optional(pool)
    .await
    .unwrap_or(None);
    let (et_id, assigned, mode) = booking?;
    let names: Vec<String> = if mode == "round_robin" {
        match assigned {
            Some(rid) => sqlx::query_scalar("SELECT name FROM resources WHERE id = ?")
                .bind(&rid)
                .fetch_optional(pool)
                .await
                .unwrap_or(None)
                .into_iter()
                .collect(),
            None => Vec::new(),
        }
    } else {
        sqlx::query_scalar(
            "SELECT r.name FROM resources r
             JOIN event_type_resources etr ON etr.resource_id = r.id
             WHERE etr.event_type_id = ? ORDER BY r.name",
        )
        .bind(&et_id)
        .fetch_all(pool)
        .await
        .unwrap_or_default()
    };
    if names.is_empty() {
        None
    } else {
        Some(names.join(", "))
    }
}

/// What the guest typed into the booking form that the host should keep:
/// the notes, and the phone number when the event type collects one (a phone
/// stored before the mode was switched off is not shown). Paths that rebuild
/// `BookingDetails` from the row (approval, reschedule, claim, reminders) use
/// this so the host emails and the calendar event carry what the booking
/// form's own emails did (#261). The write-back retry, the Meet path and the
/// pending sweep read them here too rather than from their own row queries,
/// whose copies had already drifted (#266).
pub(crate) async fn booking_guest_input(
    pool: &SqlitePool,
    booking_uid: &str,
) -> (Option<String>, Option<String>) {
    let row: Option<(Option<String>, Option<String>, String)> = sqlx::query_as(
        "SELECT b.notes, b.guest_phone, et.sms_phone_mode
         FROM bookings b JOIN event_types et ON et.id = b.event_type_id
         WHERE b.uid = ?",
    )
    .bind(booking_uid)
    .fetch_optional(pool)
    .await
    .unwrap_or(None);
    let Some((notes, phone, phone_mode)) = row else {
        return (None, None);
    };
    let notes = notes.filter(|n| !n.trim().is_empty());
    let phone = phone
        .filter(|p| !p.trim().is_empty())
        .filter(|_| crate::sms::collects_phone(&phone_mode));
    (notes, phone)
}

/// The additional attendees the guest invited, for paths that rebuild
/// `BookingDetails` from the row: without them an approved or rescheduled
/// booking's calendar event lists only the guest. One order for every push,
/// so a rewrite never reorders the ATTENDEE lines.
pub(crate) async fn booking_additional_attendees(
    pool: &SqlitePool,
    booking_uid: &str,
) -> Vec<String> {
    sqlx::query_scalar(
        "SELECT ba.email FROM booking_attendees ba
         JOIN bookings b ON b.id = ba.booking_id
         WHERE b.uid = ? ORDER BY ba.created_at, ba.email",
    )
    .bind(booking_uid)
    .fetch_all(pool)
    .await
    .unwrap_or_default()
}

/// The SEQUENCE of the booking's invite, bumped by each reschedule (#267).
/// Every path that rebuilds an invite, a cancellation or a calendar push
/// from the row reads it here, so a moved booking's copies update in place
/// and its cancellation is not ignored as older than the copy it cancels.
pub(crate) async fn booking_ics_sequence(pool: &SqlitePool, booking_uid: &str) -> u32 {
    sqlx::query_scalar::<_, i64>("SELECT ics_sequence FROM bookings WHERE uid = ?")
        .bind(booking_uid)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten()
        .and_then(|n| u32::try_from(n).ok())
        .unwrap_or(0)
}

/// The attendees a claim added (#267), each with the claimant's saved
/// language when it is a shipped one. The attendee emails a later reschedule
/// or cancellation sends them read it instead of the guest's, and carry no
/// guest footer, since they are staff, not a guest (#223). Empty when nobody
/// claimed the booking.
///
/// The rows are found by `added_by_claim`, so neither a booking email
/// changed after the claim (the row keeps the claim-time address, which is
/// where their copies go) nor the claimant's deleted account makes them a
/// guest's; a deleted claimant's row has no language and reads the
/// booking's. A claim with no marked row (its insert failed) falls back to
/// the claimant's current address.
///
/// `attendees` is the booking's additional attendees. When the lookup fails
/// every one of them is returned, without a language: who the claimant is
/// is unknown, so none of them is treated as a guest and no copy carries the
/// footer.
pub(crate) async fn booking_attendee_languages(
    pool: &SqlitePool,
    booking_uid: &str,
    attendees: &[String],
) -> Vec<(String, Option<String>)> {
    let found = sqlx::query_as::<_, (String, Option<String>)>(
        "SELECT ba.email, u.language FROM bookings b
         JOIN booking_attendees ba ON ba.booking_id = b.id AND ba.added_by_claim = 1
         LEFT JOIN users u ON u.id = ba.claimant_user_id
         WHERE b.uid = ?
         UNION ALL
         SELECT COALESCE(u.booking_email, u.email), u.language FROM bookings b
         JOIN users u ON u.id = b.claimed_by_user_id
         WHERE b.uid = ?
           AND NOT EXISTS (SELECT 1 FROM booking_attendees ba
                           WHERE ba.booking_id = b.id AND ba.added_by_claim = 1)",
    )
    .bind(booking_uid)
    .bind(booking_uid)
    .fetch_all(pool)
    .await;
    match found {
        // Kept without a language too: the claimant never gets the guest footer.
        Ok(rows) => rows
            .into_iter()
            .map(|(email, lang)| (email, crate::email::supported_language(lang)))
            .collect(),
        Err(e) => {
            tracing::error!(
                booking_uid = %booking_uid,
                error = %e,
                "claimant lookup failed, no attendee copy gets the guest footer"
            );
            attendees
                .iter()
                .map(|email| (email.clone(), None))
                .collect()
        }
    }
}

/// Fill in who holds the invite of a booking being cancelled, declined or
/// expired (#267): whether it was ever confirmed (`first_confirmed_at`), and
/// then its additional attendees with their languages. A booking never
/// confirmed sent no invite, so its attendees are left empty and get
/// nothing. `was_pending` says the booking ended while awaiting approval:
/// if it was confirmed before, that was a reschedule, and the times on the
/// row are the requested ones rather than those of the copies.
pub(crate) async fn with_invite_holders(
    pool: &SqlitePool,
    details: crate::email::CancellationDetails,
    was_pending: bool,
) -> crate::email::CancellationDetails {
    let invite_sent = sqlx::query_scalar::<_, bool>(
        "SELECT first_confirmed_at IS NOT NULL FROM bookings WHERE uid = ?",
    )
    .bind(&details.uid)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten()
    .unwrap_or(false);
    if !invite_sent {
        return crate::email::CancellationDetails {
            invite_sent: false,
            reschedule_pending: false,
            additional_attendees: Vec::new(),
            attendee_languages: Vec::new(),
            ..details
        };
    }
    let attendees = booking_additional_attendees(pool, &details.uid).await;
    crate::email::CancellationDetails {
        invite_sent: true,
        reschedule_pending: was_pending,
        attendee_languages: booking_attendee_languages(pool, &details.uid, &attendees).await,
        additional_attendees: attendees,
        ..details
    }
}

/// Release one specific resource's reservation for a booking, fetched by
/// id (used when a reschedule re-pick moves the booking off a resource the
/// row no longer references).
pub(super) async fn resource_release_one(
    pool: &SqlitePool,
    key: &[u8; 32],
    booking_uid: &str,
    resource_id: &str,
) {
    let row: Option<(String, Option<String>, Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT feed_url, caldav_url, caldav_username, caldav_password FROM resources WHERE id = ?",
    )
    .bind(resource_id)
    .fetch_optional(pool)
    .await
    .unwrap_or(None);
    let Some((feed_url, caldav_url, service_user, service_pw)) = row else {
        return;
    };
    let url = caldav_url
        .filter(|u| !u.is_empty())
        .or_else(|| crate::resources::derive_caldav_url(&feed_url));
    release_reservation_target(
        pool,
        key,
        booking_uid,
        resource_id,
        url.as_deref(),
        service_user.as_deref(),
        service_pw.as_deref(),
    )
    .await;
}

/// Shared per-resource release: remote DELETE with credential candidates,
/// then local cache cleanup so availability recovers immediately.
async fn release_reservation_target(
    pool: &SqlitePool,
    key: &[u8; 32],
    booking_uid: &str,
    resource_id: &str,
    caldav_url: Option<&str>,
    service_user: Option<&str>,
    service_pw: Option<&str>,
) {
    if let Some(url) = caldav_url {
        let candidates =
            resource_write_candidates(pool, key, url, service_user, service_pw, None).await;
        let mut deleted = false;
        for (username, password) in &candidates {
            let client = crate::caldav::CaldavClient::new(url, username, password);
            match client.delete_event(url, booking_uid).await {
                Ok(()) => {
                    deleted = true;
                    break;
                }
                Err(e) => {
                    tracing::warn!(resource_id = %resource_id, writer = %username, error = %e, "resource reservation delete failed, trying next credentials");
                }
            }
        }
        if candidates.is_empty() {
            tracing::error!(resource_id = %resource_id, uid = %booking_uid, "resource reservation release skipped: no write credentials available; remove the event manually from the resource calendar");
        } else if !deleted {
            tracing::error!(resource_id = %resource_id, uid = %booking_uid, "resource reservation could not be released; remove the event manually from the resource calendar");
        }
    }
    let _ = sqlx::query("DELETE FROM resource_events WHERE resource_id = ? AND uid = ?")
        .bind(resource_id)
        .bind(booking_uid)
        .execute(pool)
        .await;
}

pub(crate) async fn resource_delete_booking(pool: &SqlitePool, key: &[u8; 32], booking_uid: &str) {
    let targets = booking_resources_to_reserve(pool, booking_uid, true).await;
    for (resource_id, caldav_url, service_user, service_pw) in &targets {
        release_reservation_target(
            pool,
            key,
            booking_uid,
            resource_id,
            caldav_url.as_deref(),
            service_user.as_deref(),
            service_pw.as_deref(),
        )
        .await;
    }
}
