//! Who hosts a booking: host identity, collective members, reschedule and approval hosts.

use super::*;

/// The `(name, email, language)` the booking's calendar event was created
/// with: the assigned member for round-robin, every eligible member joined
/// (first one's address) for a collective team event type, otherwise the
/// event type owner. `language` is the saved language of the user whose
/// address is returned (normalised, `None` for English), so host emails are
/// written in it without a second lookup.
///
/// This mirrors how the booking flow fills `BookingDetails::host_name` /
/// `host_email`, and that symmetry is load-bearing: the ICS is written as
/// `ORGANIZER;CN={host_name}:mailto:{host_email}`, and Google CalDAV answers
/// `403 Forbidden` to a PUT that changes an existing event's organizer. The
/// reschedule handlers used to resolve the host through `accounts.user_id`,
/// so a team booking created with the assigned member as organizer was
/// re-pushed with the *owner* as organizer — Google refused the PUT, the error
/// was only logged, and the guest was still told the booking had moved.
///
/// Multi-host bookings (collective team, dynamic group) store the organizer
/// they were created with (#161), and that snapshot wins: recomputing it from
/// the current roster would let a member being added, removed, disabled or
/// renamed, or a weight change, move the organizer under an existing event.
/// Rows without a snapshot (single-host bookings, and multi-host bookings
/// made before migration 070) are resolved from the current state below.
///
/// Returns `None` only when the booking or its host row is gone, so callers
/// can keep whatever they resolved themselves.
pub(crate) async fn booking_host_identity(
    pool: &SqlitePool,
    booking_uid: &str,
) -> Option<(String, String, Option<String>)> {
    booking_host_identity_with_id(pool, booking_uid)
        .await
        .map(|(name, email, language, _)| (name, email, language))
}

/// `booking_host_identity` plus the user id behind the address, `None` when
/// the stored organizer's user is gone. A host email addressed to that
/// identity decides with it whether its recipient may read the booking's
/// host notes (#173): the organizer snapshot is kept for the calendar, but a
/// member who has left the team since must not be sent the notes.
pub(crate) async fn booking_host_identity_with_id(
    pool: &SqlitePool,
    booking_uid: &str,
) -> Option<(String, String, Option<String>, Option<String>)> {
    // accounts.user_id is nullable (ON DELETE SET NULL, and team event types
    // hang off a placeholder account), hence Option.
    #[allow(clippy::type_complexity)]
    let (
        organizer_name,
        organizer_email,
        organizer_user_id,
        assigned,
        team_id,
        mode,
        et_id,
        owner_id,
    ): (
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
        String,
        String,
        Option<String>,
    ) = sqlx::query_as(
        "SELECT b.organizer_name, b.organizer_email, b.organizer_user_id,
                b.assigned_user_id, et.team_id, et.scheduling_mode, et.id, a.user_id
         FROM bookings b
         JOIN event_types et ON et.id = b.event_type_id
         JOIN accounts a ON a.id = et.account_id
         WHERE b.uid = ?",
    )
    .bind(booking_uid)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten()?;

    // The snapshot holds even when its user was since disabled or deleted:
    // the event on the calendar still names that address. Only the language
    // is read live, and falls back to English once the user is gone.
    if let (Some(name), Some(email)) = (organizer_name, organizer_email) {
        let language = match &organizer_user_id {
            Some(id) => crate::email::user_language(pool, id).await,
            None => None,
        };
        return Some((name, email, language, organizer_user_id));
    }

    // Collective bookings stay unassigned and carry the whole roster. Same
    // eligibility and `ORDER BY u.name` as the booking flow, so legacy rows
    // without a snapshot keep the organizer they were created with as long
    // as the roster has not changed since.
    if assigned.is_none() && mode == "collective" {
        if let Some(team_id) = &team_id {
            let members: Vec<(String, String, Option<String>, String)> = sqlx::query_as(
                "SELECT u.name, COALESCE(u.booking_email, u.email), u.language, u.id FROM users u
                 JOIN team_members tm ON tm.user_id = u.id
                 LEFT JOIN event_type_member_weights etw
                   ON etw.user_id = u.id AND etw.event_type_id = ?
                 WHERE tm.team_id = ? AND u.enabled = 1 AND COALESCE(etw.weight, 1) > 0
                 ORDER BY u.name",
            )
            .bind(&et_id)
            .bind(team_id)
            .fetch_all(pool)
            .await
            .unwrap_or_default();
            if let Some((_, first_email, first_language, first_id)) = members.first() {
                let joined = members
                    .iter()
                    .map(|(name, _, _, _)| name.as_str())
                    .collect::<Vec<_>>()
                    .join(" & ");
                return Some((
                    joined,
                    first_email.clone(),
                    crate::email::supported_language(first_language.clone()),
                    Some(first_id.clone()),
                ));
            }
        }
    }

    let host = assigned.or(owner_id)?;
    host_contact(pool, &host)
        .await
        .map(|(name, email, language)| (name, email, language, Some(host)))
}

/// Name, booking address and normalised saved language of one user: the
/// single query behind `booking_host_identity`'s non-collective case and the
/// fallbacks its callers take when it resolves nothing.
pub(super) async fn host_contact(
    pool: &SqlitePool,
    user_id: &str,
) -> Option<(String, String, Option<String>)> {
    let (name, email, language): (String, String, Option<String>) = sqlx::query_as(
        "SELECT name, COALESCE(booking_email, email), language FROM users WHERE id = ?",
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten()?;
    Some((name, email, crate::email::supported_language(language)))
}

/// Eligible members of the collective team booking identified by `booking_uid`.
/// Unlike [`booking_write_targets`], this deliberately preserves an empty
/// roster so availability callers can reject a slot when nobody can host it.
async fn eligible_collective_booking_members(
    pool: &SqlitePool,
    booking_uid: &str,
    team_id: &str,
) -> Vec<String> {
    try_eligible_collective_booking_members(pool, booking_uid, team_id)
        .await
        .unwrap_or_default()
}

/// [`eligible_collective_booking_members`], with an unreadable roster as an
/// error rather than an empty one.
async fn try_eligible_collective_booking_members(
    pool: &SqlitePool,
    booking_uid: &str,
    team_id: &str,
) -> Result<Vec<String>, sqlx::Error> {
    let members: Vec<(String,)> = sqlx::query_as(
        "SELECT u.id FROM users u
         JOIN team_members tm ON tm.user_id = u.id
         LEFT JOIN event_type_member_weights etw
           ON etw.user_id = u.id
          AND etw.event_type_id = (SELECT event_type_id FROM bookings WHERE uid = ?)
         WHERE tm.team_id = ? AND u.enabled = 1
           AND COALESCE(etw.weight, 1) > 0",
    )
    .bind(booking_uid)
    .bind(team_id)
    .fetch_all(pool)
    .await?;
    Ok(members.into_iter().map(|(id,)| id).collect())
}

/// Users whose calendars this booking's write-back targets (#147): the
/// assigned team member when set, every eligible team member for a
/// collective team event type, otherwise the caller's fallback user
/// (personal host / event type owner). Callers historically passed the
/// event type owner everywhere, which put team bookings on the owner's
/// calendar instead of the assigned member's.
pub(super) async fn booking_write_targets(
    pool: &SqlitePool,
    booking_uid: &str,
    fallback_user_id: &str,
) -> Vec<String> {
    try_booking_write_targets(pool, booking_uid, fallback_user_id)
        .await
        .unwrap_or_else(|error| {
            tracing::error!(%error, uid = %booking_uid, "cannot resolve the booking's hosts; using the fallback user");
            vec![fallback_user_id.to_string()]
        })
}

/// [`booking_write_targets`], with a failed read as an error rather than
/// the fallback user: the reschedule and approval checks must not check
/// the wrong person.
async fn try_booking_write_targets(
    pool: &SqlitePool,
    booking_uid: &str,
    fallback_user_id: &str,
) -> Result<Vec<String>, sqlx::Error> {
    let row: Option<(Option<String>, Option<String>, String, Option<String>)> = sqlx::query_as(
        "SELECT b.assigned_user_id, et.team_id, et.scheduling_mode, a.user_id
         FROM bookings b
         JOIN event_types et ON et.id = b.event_type_id
         JOIN accounts a ON a.id = et.account_id
         WHERE b.uid = ?",
    )
    .bind(booking_uid)
    .fetch_optional(pool)
    .await?;
    Ok(match row {
        Some((Some(assigned), _, _, _)) => vec![assigned],
        Some((None, Some(team_id), mode, owner)) if mode == "collective" => {
            // Same eligibility as the slot grid: enabled members with a
            // non-zero per-event-type weight.
            let members =
                try_eligible_collective_booking_members(pool, booking_uid, &team_id).await?;
            if members.is_empty() {
                vec![owner.unwrap_or_else(|| fallback_user_id.to_string())]
            } else {
                members
            }
        }
        // Unassigned round-robin (and personal): same person `elect_meet_owner`
        // uses — the event type owner — not the caller. A team admin pushing
        // must not mint Meet on the owner then write ICS elsewhere.
        Some((None, _, _, Some(owner))) => vec![owner],
        _ => vec![fallback_user_id.to_string()],
    })
}

/// Who a reschedule has to be free for (#166). The guest reschedule flow used
/// to resolve the host through `accounts.user_id` — the event type owner — and
/// check availability against them, so on a round-robin team event type a guest
/// could move a booking onto a slot the assigned member was already busy for,
/// and was shown a slot grid built from the wrong person's calendar.
///
/// The member list comes from [`booking_write_targets`], which already encodes
/// the resolution order: the assigned member, every eligible member for a
/// collective event type, the owner otherwise.
pub(super) struct RescheduleHosts {
    /// Users whose calendars gate the new slot. Personal and round-robin
    /// bookings fall back to the event type owner; a collective booking keeps
    /// an empty roster so no slot can be offered without an eligible host.
    pub(super) user_ids: Vec<String>,
    /// The team the event type belongs to, `None` for personal event types.
    pub(super) team_id: Option<String>,
    /// Unassigned collective booking: it belongs to the whole roster, so every
    /// member must be free and the page shows the team instead of one person.
    pub(super) collective: bool,
    /// The other hosts of a dynamic group booking (`/u/alice+bob/slug`),
    /// enabled ones only, besides the owner in `user_ids`. They must be free
    /// too, but only their busy times are read: the booking stays the owner's
    /// for organizer, write-back and emails.
    pub(super) participants: Vec<String>,
    /// The watcher who claimed the booking (#374), when they are not one of
    /// the hosts above. A claim survives a reschedule, so the new time must
    /// be free for them too: their busy times (all their calendars, as for a
    /// co-host, but not their working hours, since they chose to join) are
    /// read with the booking's own row and calendar copy excluded. The
    /// booking stays the hosts' for organizer, write-back and emails.
    pub(super) claimant: Option<String>,
}

/// The enabled co-hosts stored for a dynamic group booking.
pub(super) async fn booking_participants(pool: &SqlitePool, booking_uid: &str) -> Vec<String> {
    try_booking_participants(pool, booking_uid)
        .await
        .unwrap_or_default()
}

/// [`booking_participants`], with an unreadable list as an error rather than
/// "no co-hosts".
async fn try_booking_participants(
    pool: &SqlitePool,
    booking_uid: &str,
) -> Result<Vec<String>, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT bp.user_id FROM booking_participants bp
         JOIN bookings b ON b.id = bp.booking_id
         JOIN users u ON u.id = bp.user_id
         WHERE b.uid = ? AND u.enabled = 1
         ORDER BY u.id",
    )
    .bind(booking_uid)
    .fetch_all(pool)
    .await
}

/// The [`RescheduleHosts`] of a booking. Any read that fails is an error:
/// falling back would check the owner instead of the assigned member, or
/// skip a dynamic group booking's co-hosts.
pub(super) async fn reschedule_hosts(
    pool: &SqlitePool,
    booking_uid: &str,
    owner_user_id: &str,
) -> Result<RescheduleHosts, sqlx::Error> {
    let row: Option<(Option<String>, Option<String>, String, Option<String>)> = sqlx::query_as(
        "SELECT b.assigned_user_id, et.team_id, et.scheduling_mode, b.claimed_by_user_id
         FROM bookings b JOIN event_types et ON et.id = b.event_type_id
         WHERE b.uid = ?",
    )
    .bind(booking_uid)
    .fetch_optional(pool)
    .await?;
    // No row means the caller's own lookup raced a delete; treat it as a
    // personal booking on the owner, which is what happened before #166.
    let (assigned, team_id, mode, claimed_by) = row.unwrap_or_default();
    // An assigned booking stays that member's even if the event type has since
    // been switched to collective.
    let collective = assigned.is_none() && team_id.is_some() && mode == "collective";
    let user_ids = if collective {
        // Do not inherit booking_write_targets' CalDAV fallback here. An empty
        // collective roster must produce BusySource::Team(empty), which makes
        // every slot unavailable rather than treating the owner as the host.
        try_eligible_collective_booking_members(
            pool,
            booking_uid,
            team_id.as_deref().unwrap_or_default(),
        )
        .await?
    } else {
        try_booking_write_targets(pool, booking_uid, owner_user_id).await?
    };
    let participants = if team_id.is_none() {
        try_booking_participants(pool, booking_uid).await?
    } else {
        Vec::new()
    };
    let claimant = claimed_by.filter(|c| !user_ids.contains(c) && !participants.contains(c));
    Ok(RescheduleHosts {
        user_ids,
        collective,
        team_id,
        participants,
        claimant,
    })
}

/// Refresh the calendars of every user a reschedule depends on. `sync_if_stale`
/// takes a per-source mutex (skipping a source another task is syncing) and
/// re-checks staleness inside it, so even with this fan-out at most one
/// CalDAV fetch per source is in flight.
pub(super) async fn sync_reschedule_hosts(state: &Arc<AppState>, hosts: &RescheduleHosts) {
    let mut sync_tasks = tokio::task::JoinSet::new();
    for user_id in hosts
        .user_ids
        .iter()
        .chain(&hosts.participants)
        .chain(&hosts.claimant)
    {
        let pool = state.pool.clone();
        let key = state.secret_key;
        let user_id = user_id.clone();
        sync_tasks.spawn(async move {
            crate::commands::sync::sync_if_stale(&pool, &key, &user_id).await;
        });
    }
    while sync_tasks.join_next().await.is_some() {}
}

/// Busy times for every user a reschedule has to be free for, with the booking
/// flow's semantics: team hosts go through [`BusySource::Team`], so a
/// collective booking needs the whole roster free and a round-robin booking —
/// which resolves to exactly one member — needs that member free. Personal
/// event types keep the plain single-user path.
///
/// `strict` is for the submit-time check: a read that fails is then an
/// error rather than skipped rows, so it can never make a host look free.
/// The reschedule slot page passes `false` and never gets an error.
#[allow(clippy::too_many_arguments)]
pub(super) async fn reschedule_busy_source(
    pool: &SqlitePool,
    hosts: &RescheduleHosts,
    et_id: &str,
    window_start: NaiveDateTime,
    window_end: NaiveDateTime,
    host_tz: Tz,
    exclude_booking_id: &str,
    exclude_booking_uid: &str,
    strict: bool,
) -> Result<BusySource, sqlx::Error> {
    let calendars = |user_id: String, et_filter: Option<String>| async move {
        reschedule_user_busy(
            pool,
            &user_id,
            window_start,
            window_end,
            host_tz,
            et_filter.as_deref(),
            exclude_booking_id,
            exclude_booking_uid,
            strict,
        )
        .await
    };
    let hours = |user_id: String| async move {
        if strict {
            try_user_avail_as_busy(pool, &user_id, window_start, window_end, host_tz).await
        } else {
            Ok(user_avail_as_busy(pool, &user_id, window_start, window_end, host_tz).await)
        }
    };
    let et = Some(et_id.to_string());
    // The claimant's calendars, all of them (the event type's calendar
    // selection is the hosts'), with this booking and its copy excluded.
    let claimant_busy = match &hosts.claimant {
        Some(claimant) => Some((claimant.clone(), calendars(claimant.clone(), None).await?)),
        None => None,
    };
    if hosts.team_id.is_none() {
        let owner = hosts.user_ids.first().cloned().unwrap_or_default();
        let mut owner_busy = calendars(owner.clone(), et.clone()).await?;
        if hosts.participants.is_empty() {
            // One person must be free: the claimant's busy times simply
            // join the host's.
            if let Some((_, busy)) = claimant_busy {
                owner_busy.extend(busy);
            }
            return Ok(BusySource::Individual(owner_busy));
        }
        // A dynamic group booking: everyone must be free, each co-host
        // within their own working hours and with all of their calendars,
        // as on the dynamic group slot page.
        let mut member_busy = HashMap::new();
        member_busy.insert(owner, owner_busy);
        for user_id in &hosts.participants {
            let mut busy = calendars(user_id.clone(), None).await?;
            ensure_user_avail_seeded(pool, user_id).await;
            busy.extend(hours(user_id.clone()).await?);
            member_busy.insert(user_id.clone(), busy);
        }
        member_busy.extend(claimant_busy);
        return Ok(BusySource::Team(member_busy));
    }
    let mut member_busy = HashMap::new();
    for user_id in &hosts.user_ids {
        let mut busy = calendars(user_id.clone(), et.clone()).await?;
        // Same personal-working-hours constraint the team slot grid applies.
        // Members without explicit hours come back unconstrained, so this never
        // plants a surprise 9-17 default.
        busy.extend(hours(user_id.clone()).await?);
        member_busy.insert(user_id.clone(), busy);
    }
    // An empty collective roster stays `Team(empty)`, never bookable: the
    // claimant alone does not make a slot free.
    if !member_busy.is_empty() {
        member_busy.extend(claimant_busy);
    }
    Ok(BusySource::Team(member_busy))
}

/// One user's busy times for [`reschedule_busy_source`], excluding the
/// booking being moved and its own calendar copy.
#[allow(clippy::too_many_arguments)]
async fn reschedule_user_busy(
    pool: &SqlitePool,
    user_id: &str,
    window_start: NaiveDateTime,
    window_end: NaiveDateTime,
    host_tz: Tz,
    et_filter: Option<&str>,
    exclude_booking_id: &str,
    exclude_booking_uid: &str,
    strict: bool,
) -> Result<Vec<(NaiveDateTime, NaiveDateTime)>, sqlx::Error> {
    if strict {
        try_fetch_busy_times_for_user_opts(
            pool,
            user_id,
            window_start,
            window_end,
            host_tz,
            et_filter,
            Some(exclude_booking_id),
            Some(exclude_booking_uid),
            true,
        )
        .await
    } else {
        Ok(fetch_busy_times_for_user_ex(
            pool,
            user_id,
            window_start,
            window_end,
            host_tz,
            et_filter,
            Some(exclude_booking_id),
            Some(exclude_booking_uid),
        )
        .await)
    }
}

/// Upper bound on the calendar refresh an approval does before re-checking
/// the hosts. The token path is unauthenticated, so a slow or dead CalDAV
/// server must not hold the request (or the approval) open indefinitely.
const APPROVAL_SYNC_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

/// The hosts an approval has to re-check (same resolution as a reschedule:
/// the assigned member, every eligible member of a collective booking, the
/// owner otherwise), with their calendars refreshed when stale. The refresh
/// is bounded by [`APPROVAL_SYNC_TIMEOUT`] overall; past it the check uses
/// the cached events, which is what it would have done before the refresh.
pub(super) async fn prepare_approval_hosts(
    state: &Arc<AppState>,
    booking_uid: &str,
    owner_user_id: &str,
) -> Result<RescheduleHosts, sqlx::Error> {
    let hosts = reschedule_hosts(&state.pool, booking_uid, owner_user_id).await?;
    if tokio::time::timeout(APPROVAL_SYNC_TIMEOUT, sync_reschedule_hosts(state, &hosts))
        .await
        .is_err()
    {
        tracing::warn!(
            uid = %booking_uid,
            "calendar refresh before approval timed out; checking cached events"
        );
    }
    Ok(hosts)
}

/// Whether a host an approval needs has a Microsoft calendar that has not
/// finished its first sync: [`approval_hosts_free`] then refuses, and the
/// refusal should say why rather than call the host busy.
pub(super) async fn approval_hosts_awaiting_first_sync(
    pool: &SqlitePool,
    hosts: &RescheduleHosts,
) -> bool {
    for user_id in hosts
        .user_ids
        .iter()
        .chain(hosts.participants.iter())
        .chain(hosts.claimant.iter())
    {
        if crate::commands::sync::microsoft_coverage_gap(pool, user_id, chrono::Utc::now()).await
            == Some(crate::commands::sync::CoverageGap::NotYetSynced)
        {
            return true;
        }
    }
    false
}

/// Whether everyone a pending booking needs is still free for it, checked
/// under the booking lock right before an approval flips it to confirmed.
///
/// Busy means a calendar event, a confirmed booking or time off (#46),
/// excluding the booking itself and its own calendar copy, for every host and
/// for the watcher who claimed the booking. Other pending requests are not counted:
/// they hold slots against new guests, but between two waiting requests the
/// host decides, and counting them would let two overlapping requests block
/// each other's approval forever. Once one is approved it is confirmed, and
/// the other is then refused. Working hours are not re-applied: the host is
/// explicitly accepting this time. An empty collective roster is never free,
/// as in [`reschedule_hosts`]. A busy time that cannot be read is an error,
/// which the callers turn into an internal error page: never an approval.
pub(super) async fn approval_hosts_free(
    pool: &SqlitePool,
    hosts: &RescheduleHosts,
    booking_id: &str,
    booking_uid: &str,
    et_id: &str,
    start_at: &str,
    end_at: &str,
) -> Result<bool, sqlx::Error> {
    if hosts.user_ids.is_empty() {
        return Ok(false);
    }
    let host_tz = get_host_tz(pool, et_id).await;
    let Some((start, end)) = crate::booking_time::busy_range(start_at, end_at, host_tz) else {
        return Ok(true);
    };
    // The co-hosts of a dynamic group booking read all their calendars, as
    // on the dynamic group slot page (the calendar selection is the owner's),
    // and so does the watcher who claimed the booking (#374): a claim is kept
    // through a reschedule, so its approval needs them free as well.
    let everyone = hosts
        .user_ids
        .iter()
        .map(|u| (u, Some(et_id)))
        .chain(hosts.participants.iter().map(|u| (u, None)))
        .chain(hosts.claimant.iter().map(|u| (u, None)));
    for (user_id, et_filter) in everyone {
        let busy = try_fetch_busy_times_for_user_opts(
            pool,
            user_id,
            start,
            end,
            host_tz,
            et_filter,
            Some(booking_id),
            Some(booking_uid),
            false,
        )
        .await?;
        if has_conflict(&busy, start, end) {
            tracing::info!(
                uid = %booking_uid,
                %user_id,
                co_host = et_filter.is_none(),
                "approval refused: a host is busy at the requested time"
            );
            return Ok(false);
        }
    }
    Ok(true)
}

/// The host profile the reschedule page shows: the person the booking is with
/// (the assigned member, or the owner of a personal event type), or the team
/// for a collective booking that belongs to the whole roster.
pub(super) struct RescheduleHostProfile {
    /// Sidebar heading — the host's name, or the team's when `team` is set.
    pub(super) name: String,
    pub(super) title: Option<String>,
    /// The single host to render an avatar for; `None` when `team` is set.
    pub(super) user_id: Option<String>,
    pub(super) has_avatar: bool,
    pub(super) team: Option<RescheduleTeamProfile>,
}

pub(super) struct RescheduleTeamProfile {
    pub(super) id: String,
    /// (user id, name, avatar path) for the eligible roster.
    pub(super) members: Vec<(String, String, Option<String>)>,
}

pub(super) async fn reschedule_host_profile(
    pool: &SqlitePool,
    hosts: &RescheduleHosts,
    et_id: &str,
) -> RescheduleHostProfile {
    if hosts.collective {
        if let Some(team_id) = &hosts.team_id {
            let team: Option<(String, Option<String>)> =
                sqlx::query_as("SELECT name, avatar_path FROM teams WHERE id = ?")
                    .bind(team_id)
                    .fetch_optional(pool)
                    .await
                    .unwrap_or(None);
            if let Some((team_name, team_avatar)) = team {
                // Same eligibility as the availability check above, so the
                // roster shown is the one the slot grid was computed from.
                let members: Vec<(String, String, Option<String>)> = sqlx::query_as(
                    "SELECT u.id, u.name, u.avatar_path FROM users u
                     JOIN team_members tm ON tm.user_id = u.id
                     LEFT JOIN event_type_member_weights etw
                       ON etw.user_id = u.id AND etw.event_type_id = ?
                     WHERE tm.team_id = ? AND u.enabled = 1 AND COALESCE(etw.weight, 1) > 0
                     ORDER BY u.name",
                )
                .bind(et_id)
                .bind(team_id)
                .fetch_all(pool)
                .await
                .unwrap_or_default();
                return RescheduleHostProfile {
                    name: team_name,
                    title: None,
                    user_id: None,
                    has_avatar: team_avatar.is_some(),
                    team: Some(RescheduleTeamProfile {
                        id: team_id.clone(),
                        members,
                    }),
                };
            }
        }
    }
    let host_id = hosts.user_ids.first().map(String::as_str).unwrap_or("");
    let row: Option<(String, Option<String>, Option<String>)> =
        sqlx::query_as("SELECT name, title, avatar_path FROM users WHERE id = ?")
            .bind(host_id)
            .fetch_optional(pool)
            .await
            .unwrap_or(None);
    let (name, title, avatar_path) = row.unwrap_or_default();
    RescheduleHostProfile {
        name,
        title,
        user_id: Some(host_id.to_string()),
        has_avatar: avatar_path.is_some(),
        team: None,
    }
}
