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

/// Push a confirmed booking to ONE user's CalDAV calendar (every source
/// with a write_calendar_href set for them). Most callers want
/// [`caldav_push_booking`], which resolves the right user(s) first and
/// writes the claimant's copy too.
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
    let mut failures = Vec::new();
    // Find all sources with write_calendar_href configured for this user
    let sources: Vec<(
        String,
        String,
        String,
        Option<String>,
        String,
        String,
        Option<String>,
        Option<String>,
        String,
        Option<String>,
        String,
    )> = sqlx::query_as(
        "SELECT cs.id, cs.url, cs.username, cs.password_enc, cs.write_calendar_href, cs.auth_type, cs.access_token_enc, cs.token_expires_at, cs.provider_type, cs.oauth2_provider, cs.name
         FROM caldav_sources cs
         JOIN accounts a ON a.id = cs.account_id
         WHERE a.user_id = ? AND cs.enabled = 1 AND cs.write_calendar_href IS NOT NULL",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    if sources.is_empty() {
        // Distinguish "user has no sources" (debug) from "user has sources but none
        // are write-configured" (warn). The second case is almost always a misconfig.
        let unconfigured: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM caldav_sources cs
             JOIN accounts a ON a.id = cs.account_id
             WHERE a.user_id = ? AND cs.enabled = 1 AND cs.write_calendar_href IS NULL",
        )
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
    ) in &sources
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
                let _ = sqlx::query("UPDATE bookings SET caldav_calendar_href = ? WHERE uid = ?")
                    .bind(calendar_href)
                    .bind(booking_uid)
                    .execute(pool)
                    .await;
            }
            // Later PUTs to other calendars must not overwrite this: cancel
            // keys off one href, and that has to be the Meet-holding calendar.
            preserved_meet_href = true;
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
            Ok(p) => p,
            Err(e) => {
                tracing::error!(url = %url, error = format!("{e:#}"), "calendar write-back failed: could not build client");
                failures.push(format!("{source_name}: {e:#}"));
                continue;
            }
        };

        // A 2xx on the write is not proof the host sees the event (#162):
        // read it back by UID before calling it written.
        let written =
            crate::providers::put_and_verify(provider.as_ref(), calendar_href, booking_uid, &ics)
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
        if event_may_exist && !preserved_meet_href && owner == CopyOwner::Host {
            let _ = sqlx::query("UPDATE bookings SET caldav_calendar_href = ? WHERE uid = ?")
                .bind(calendar_href)
                .bind(booking_uid)
                .execute(pool)
                .await;
        }

        match written {
            Ok(()) => {
                tracing::info!(uid = %booking_uid, calendar_href = %calendar_href, "calendar write-back succeeded (verified)");
            }
            Err(e) => {
                tracing::error!(uid = %booking_uid, calendar_href = %calendar_href, error = %e, "calendar write-back failed");
                failures.push(format!("{source_name}: {e}"));
            }
        }
    }
    failures
}

/// A calendar client for one of a user's sources, whatever its protocol:
/// EWS through the provider factory, CalDAV (basic or OAuth2) through
/// `build_client_for_source`.
#[allow(clippy::too_many_arguments)]
async fn write_provider_for_source(
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
) -> anyhow::Result<Box<dyn crate::providers::CalendarProvider>> {
    if provider_type == crate::providers::factory::kinds::EWS {
        let enc = password_enc.ok_or_else(|| anyhow::anyhow!("EWS source missing password"))?;
        let password = crate::crypto::decrypt_password(key, enc)
            .map_err(|e| anyhow::anyhow!("could not decrypt credentials: {e}"))?;
        return crate::providers::build_provider(provider_type, url, username, &password);
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
    Ok(Box::new(
        crate::providers::caldav::CaldavProvider::from_client(client),
    ))
}

/// Delete a booking from a user's CalDAV calendar by looking up their write-enabled source directly.
/// Used for team bookings where we don't track per-booking caldav_calendar_href.
async fn caldav_delete_for_user(
    pool: &SqlitePool,
    key: &[u8; 32],
    user_id: &str,
    booking_uid: &str,
) {
    let sources: Vec<(
        String,
        String,
        String,
        Option<String>,
        String,
        String,
        Option<String>,
        Option<String>,
        String,
    )> = sqlx::query_as(
        "SELECT cs.id, cs.url, cs.username, cs.password_enc, cs.write_calendar_href, cs.auth_type, cs.access_token_enc, cs.token_expires_at, cs.provider_type
         FROM caldav_sources cs
         JOIN accounts a ON a.id = cs.account_id
         WHERE a.user_id = ? AND cs.enabled = 1 AND cs.write_calendar_href IS NOT NULL",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
    .unwrap_or_default();

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
    ) in &sources
    {
        let delete_result = if provider_type == crate::providers::factory::kinds::EWS {
            let enc = match password_enc.as_deref() {
                Some(e) => e,
                None => continue,
            };
            let password = match crate::crypto::decrypt_password(key, enc) {
                Ok(p) => p,
                Err(_) => continue,
            };
            let client =
                match crate::providers::build_provider(provider_type, url, username, &password) {
                    Ok(c) => c,
                    Err(_) => continue,
                };
            client.delete_event(calendar_href, booking_uid).await
        } else {
            let client = match crate::oauth2_caldav::build_client_for_source(
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
                Ok(c) => c,
                Err(_) => continue,
            };
            client.delete_event(calendar_href, booking_uid).await
        };
        if let Err(e) = delete_result {
            tracing::error!(uid = %booking_uid, user = %user_id, calendar = %calendar_href, error = %e, "calendar event delete failed");
        }
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

/// A server origin (scheme, host, port) and a collection on it.
pub(super) type Collection = (Option<(String, String, Option<u16>)>, String);

/// The collections a user writes bookings to.
pub(super) async fn write_collections(pool: &SqlitePool, user_id: &str) -> Vec<Collection> {
    let rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT cs.url, cs.write_calendar_href FROM caldav_sources cs
         JOIN accounts a ON a.id = cs.account_id
         WHERE a.user_id = ? AND cs.enabled = 1 AND cs.write_calendar_href IS NOT NULL",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
    .unwrap_or_default();
    rows.into_iter()
        .map(|(url, href)| (crate::resources::url_origin(&url), href))
        .collect()
}

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
    let info: Option<(String,)> = sqlx::query_as(
        "SELECT caldav_calendar_href FROM bookings WHERE uid = ? AND caldav_calendar_href IS NOT NULL",
    )
    .bind(booking_uid)
    .fetch_optional(pool)
    .await
    .unwrap_or(None);

    let calendar_href = match info {
        Some((href,)) => href,
        None => {
            tracing::debug!(uid = %booking_uid, "calendar delete skipped: booking was never pushed");
            return;
        }
    };

    // Resolve whose calendar the event actually lives on (#147): the
    // assigned member's, every member's for collective, else the caller's.
    let targets = booking_write_targets(pool, booking_uid, user_id).await;
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
    async fn find_source(pool: &SqlitePool, user: &str, href: &str) -> Option<SourceRow> {
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

    let mut cache_user = target;
    let mut source = find_source(pool, target, &calendar_href).await;
    if source.is_none() && target != user_id {
        // Pre-#147 bookings were pushed to the event type owner's
        // calendar; the stored href then belongs to them, not the
        // assigned member. Clean up where the event actually is.
        source = find_source(pool, user_id, &calendar_href).await;
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
            tracing::warn!(uid = %booking_uid, user_id = %target, href = %calendar_href, "calendar delete: stored href matches no write source, falling back to delete-by-uid");
            caldav_delete_for_user(pool, key, target, booking_uid).await;
            return;
        }
    };

    let delete_result = if provider_type == crate::providers::factory::kinds::EWS {
        let enc = match password_enc.as_deref() {
            Some(e) => e,
            None => return,
        };
        let password = match crate::crypto::decrypt_password(key, enc) {
            Ok(p) => p,
            Err(_) => return,
        };
        let client =
            match crate::providers::build_provider(&provider_type, &url, &username, &password) {
                Ok(c) => c,
                Err(_) => return,
            };
        client.delete_event(&calendar_href, booking_uid).await
    } else {
        let client = match crate::oauth2_caldav::build_client_for_source(
            pool,
            key,
            &source_id,
            &url,
            &auth_type,
            &username,
            password_enc.as_deref(),
            access_token_enc.as_deref(),
            token_expires_at.as_deref(),
        )
        .await
        {
            Ok(c) => c,
            Err(_) => return,
        };
        client.delete_event(&calendar_href, booking_uid).await
    };
    if let Err(e) = delete_result {
        tracing::error!(uid = %booking_uid, error = %e, "calendar event delete failed");
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

/// Saved language of the watcher who claimed the booking, under the address
/// the claim listed them as an attendee with (#267). The attendee emails a
/// later reschedule or cancellation sends them read it instead of the
/// guest's. Empty when nobody claimed it or the claimant has no saved one.
pub(crate) async fn booking_attendee_languages(
    pool: &SqlitePool,
    booking_uid: &str,
) -> Vec<(String, String)> {
    sqlx::query_as::<_, (String, Option<String>)>(
        "SELECT COALESCE(u.booking_email, u.email), u.language FROM bookings b
         JOIN users u ON u.id = b.claimed_by_user_id WHERE b.uid = ?",
    )
    .bind(booking_uid)
    .fetch_all(pool)
    .await
    .unwrap_or_default()
    .into_iter()
    .filter_map(|(email, lang)| crate::email::supported_language(lang).map(|l| (email, l)))
    .collect()
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
    crate::email::CancellationDetails {
        invite_sent: true,
        reschedule_pending: was_pending,
        additional_attendees: booking_additional_attendees(pool, &details.uid).await,
        attendee_languages: booking_attendee_languages(pool, &details.uid).await,
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
