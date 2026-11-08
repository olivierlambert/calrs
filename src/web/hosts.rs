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
    .await
    .unwrap_or_default();
    members.into_iter().map(|(id,)| id).collect()
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
    let row: Option<(Option<String>, Option<String>, String, Option<String>)> = sqlx::query_as(
        "SELECT b.assigned_user_id, et.team_id, et.scheduling_mode, a.user_id
         FROM bookings b
         JOIN event_types et ON et.id = b.event_type_id
         JOIN accounts a ON a.id = et.account_id
         WHERE b.uid = ?",
    )
    .bind(booking_uid)
    .fetch_optional(pool)
    .await
    .unwrap_or(None);
    match row {
        Some((Some(assigned), _, _, _)) => vec![assigned],
        Some((None, Some(team_id), mode, owner)) if mode == "collective" => {
            // Same eligibility as the slot grid: enabled members with a
            // non-zero per-event-type weight.
            let members = eligible_collective_booking_members(pool, booking_uid, &team_id).await;
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
    }
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
}

/// The enabled co-hosts stored for a dynamic group booking.
pub(super) async fn booking_participants(pool: &SqlitePool, booking_uid: &str) -> Vec<String> {
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
    .unwrap_or_default()
}

pub(super) async fn reschedule_hosts(
    pool: &SqlitePool,
    booking_uid: &str,
    owner_user_id: &str,
) -> RescheduleHosts {
    let row: Option<(Option<String>, Option<String>, String)> = sqlx::query_as(
        "SELECT b.assigned_user_id, et.team_id, et.scheduling_mode
         FROM bookings b JOIN event_types et ON et.id = b.event_type_id
         WHERE b.uid = ?",
    )
    .bind(booking_uid)
    .fetch_optional(pool)
    .await
    .unwrap_or(None);
    // No row means the caller's own lookup raced a delete; treat it as a
    // personal booking on the owner, which is what happened before #166.
    let (assigned, team_id, mode) = row.unwrap_or_default();
    // An assigned booking stays that member's even if the event type has since
    // been switched to collective.
    let collective = assigned.is_none() && team_id.is_some() && mode == "collective";
    let user_ids = if collective {
        // Do not inherit booking_write_targets' CalDAV fallback here. An empty
        // collective roster must produce BusySource::Team(empty), which makes
        // every slot unavailable rather than treating the owner as the host.
        eligible_collective_booking_members(
            pool,
            booking_uid,
            team_id.as_deref().unwrap_or_default(),
        )
        .await
    } else {
        booking_write_targets(pool, booking_uid, owner_user_id).await
    };
    let participants = if team_id.is_none() {
        booking_participants(pool, booking_uid).await
    } else {
        Vec::new()
    };
    RescheduleHosts {
        user_ids,
        collective,
        team_id,
        participants,
    }
}

/// Refresh the calendars of every user a reschedule depends on. `sync_if_stale`
/// holds a per-source mutex and re-checks staleness inside it, so even with
/// this fan-out at most one CalDAV fetch per source is in flight.
pub(super) async fn sync_reschedule_hosts(state: &Arc<AppState>, hosts: &RescheduleHosts) {
    let mut sync_tasks = tokio::task::JoinSet::new();
    for user_id in hosts.user_ids.iter().chain(&hosts.participants) {
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
pub(super) async fn reschedule_busy_source(
    pool: &SqlitePool,
    hosts: &RescheduleHosts,
    et_id: &str,
    window_start: NaiveDateTime,
    window_end: NaiveDateTime,
    host_tz: Tz,
    exclude_booking_id: &str,
    exclude_booking_uid: &str,
) -> BusySource {
    if hosts.team_id.is_none() {
        let owner = hosts.user_ids.first().map(String::as_str).unwrap_or("");
        let owner_busy = fetch_busy_times_for_user_ex(
            pool,
            owner,
            window_start,
            window_end,
            host_tz,
            Some(et_id),
            Some(exclude_booking_id),
            Some(exclude_booking_uid),
        )
        .await;
        if hosts.participants.is_empty() {
            return BusySource::Individual(owner_busy);
        }
        // A dynamic group booking: everyone must be free, each co-host
        // within their own working hours and with all of their calendars,
        // as on the dynamic group slot page.
        let mut member_busy = HashMap::new();
        member_busy.insert(owner.to_string(), owner_busy);
        for user_id in &hosts.participants {
            let mut busy = fetch_busy_times_for_user_ex(
                pool,
                user_id,
                window_start,
                window_end,
                host_tz,
                None,
                Some(exclude_booking_id),
                Some(exclude_booking_uid),
            )
            .await;
            ensure_user_avail_seeded(pool, user_id).await;
            busy.extend(user_avail_as_busy(pool, user_id, window_start, window_end, host_tz).await);
            member_busy.insert(user_id.clone(), busy);
        }
        return BusySource::Team(member_busy);
    }
    let mut member_busy = HashMap::new();
    for user_id in &hosts.user_ids {
        let mut busy = fetch_busy_times_for_user_ex(
            pool,
            user_id,
            window_start,
            window_end,
            host_tz,
            Some(et_id),
            Some(exclude_booking_id),
            Some(exclude_booking_uid),
        )
        .await;
        // Same personal-working-hours constraint the team slot grid applies.
        // Members without explicit hours come back unconstrained, so this never
        // plants a surprise 9-17 default.
        busy.extend(user_avail_as_busy(pool, user_id, window_start, window_end, host_tz).await);
        member_busy.insert(user_id.clone(), busy);
    }
    BusySource::Team(member_busy)
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
) -> RescheduleHosts {
    let hosts = reschedule_hosts(&state.pool, booking_uid, owner_user_id).await;
    if tokio::time::timeout(APPROVAL_SYNC_TIMEOUT, sync_reschedule_hosts(state, &hosts))
        .await
        .is_err()
    {
        tracing::warn!(
            uid = %booking_uid,
            "calendar refresh before approval timed out; checking cached events"
        );
    }
    hosts
}

/// Whether everyone a pending booking needs is still free for it, checked
/// under the booking lock right before an approval flips it to confirmed.
///
/// Busy means a calendar event, a confirmed booking or time off (#46),
/// excluding the booking itself and its own calendar copy. Other pending requests are not counted:
/// they hold slots against new guests, but between two waiting requests the
/// host decides, and counting them would let two overlapping requests block
/// each other's approval forever. Once one is approved it is confirmed, and
/// the other is then refused. Working hours are not re-applied: the host is
/// explicitly accepting this time. An empty collective roster is never free,
/// as in [`reschedule_hosts`].
pub(super) async fn approval_hosts_free(
    pool: &SqlitePool,
    hosts: &RescheduleHosts,
    booking_id: &str,
    booking_uid: &str,
    et_id: &str,
    start_at: &str,
    end_at: &str,
) -> bool {
    if hosts.user_ids.is_empty() {
        return false;
    }
    let host_tz = get_host_tz(pool, et_id).await;
    let Some((start, end)) = crate::booking_time::busy_range(start_at, end_at, host_tz) else {
        return true;
    };
    // The co-hosts of a dynamic group booking read all their calendars, as
    // on the dynamic group slot page (the calendar selection is the owner's).
    let everyone = hosts
        .user_ids
        .iter()
        .map(|u| (u, Some(et_id)))
        .chain(hosts.participants.iter().map(|u| (u, None)));
    for (user_id, et_filter) in everyone {
        let busy = fetch_busy_times_for_user_opts(
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
        .await;
        if has_conflict(&busy, start, end) {
            tracing::info!(
                uid = %booking_uid,
                %user_id,
                co_host = et_filter.is_none(),
                "approval refused: a host is busy at the requested time"
            );
            return false;
        }
    }
    true
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
