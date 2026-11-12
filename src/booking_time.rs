//! Booking storage: version 0 is an untouched legacy wall clock; version 1 is
//! an RFC3339 UTC instant. The database enforces the UTC marker for version 1.
use chrono::{DateTime, Duration, NaiveDateTime, TimeZone, Utc};
use chrono_tz::Tz;
use sqlx::SqlitePool;

/// SQL condition selecting the bookings one user hosts. Expects the aliases
/// `b` (bookings), `et` (its event type) and `a` (that event type's account),
/// and binds the user id four times.
///
/// A user hosts a booking when it is assigned to them, or when it is
/// unassigned and they own the event type (personal bookings, and legacy
/// unassigned round-robin rows), or when it is an unassigned team booking and
/// they are a member of that team. An assigned booking must NOT mark the
/// event type owner busy: on a round-robin team the owner stays free when
/// another member took the booking (#146). An unassigned collective booking
/// is its roster's, as on the collective booking path: members of the team
/// not excluded from the event type (weight 0 or less in
/// `event_type_member_weights`, as the slot engine and `hosts.rs` read it).
/// Its owner hosts it only when on that roster, so an owner excluded from it
/// or no longer in the team is free (#375). A co-host of a dynamic group
/// booking (`booking_participants`, #46) hosts it too.
pub const USER_HOSTS_BOOKING: &str = "(b.assigned_user_id = ?
        OR (b.assigned_user_id IS NULL AND a.user_id = ?
            AND NOT (et.team_id IS NOT NULL AND et.scheduling_mode = 'collective'))
        OR (b.assigned_user_id IS NULL AND et.team_id IS NOT NULL AND EXISTS (
              SELECT 1 FROM team_members tm
              WHERE tm.team_id = et.team_id AND tm.user_id = ?
                AND NOT EXISTS (SELECT 1 FROM event_type_member_weights etw
                    WHERE etw.event_type_id = et.id AND etw.user_id = tm.user_id
                      AND etw.weight <= 0)))
        OR EXISTS (SELECT 1 FROM booking_participants bp
              WHERE bp.booking_id = b.id AND bp.user_id = ?))";

/// SQL condition selecting the bookings that make one user busy. Expects the
/// aliases `b` (bookings), `et` (its event type) and `a` (that event type's
/// account), and binds the user id five times.
///
/// The bookings the user hosts ([`USER_HOSTS_BOOKING`], four binds), plus the
/// confirmed bookings they claimed as a watcher (`claimed_by_user_id`, one
/// bind, #374): the claimant attends, so they are not free then either. A
/// claimant who also hosts the booking matches it once, as one row. Pending
/// requests count when their own event type holds the slot
/// (`pending_blocks_slot`, #234).
///
/// Shared by the web slot engine, the troubleshoot view and the CLI so the
/// three can never disagree about who is busy. `include_pending_holds` is
/// false only for the approval re-check, where other pending requests must
/// not block the one being approved.
pub fn user_busy_bookings(include_pending_holds: bool) -> String {
    let status = if include_pending_holds {
        "(b.status = 'confirmed' OR (b.status = 'pending' AND et.pending_blocks_slot = 1))"
    } else {
        "b.status = 'confirmed'"
    };
    format!(
        "({USER_HOSTS_BOOKING}
        OR (b.claimed_by_user_id = ? AND b.status = 'confirmed'))
   AND {status}"
    )
}

/// The buffers a booking holds around itself, its own event type's
/// `buffer_before` and `buffer_after` in minutes (#422), as two columns.
/// Expects the alias `et` for the booking's event type.
pub const BOOKING_BUFFERS: &str =
    "MAX(COALESCE(et.buffer_before, 0), 0), MAX(COALESCE(et.buffer_after, 0), 0)";

/// SQL condition keeping the bookings whose buffered interval can touch a
/// window, given as ISO local times (binds its end, then its start). The two
/// days on each side cover stored UTC times; the booking's own buffers
/// widen it further. Expects the aliases `b` and `et`.
pub const BUFFERED_BOOKING_WINDOW: &str = "b.start_at <= strftime('%Y-%m-%dT%H:%M:%S', ?, '+2 days', printf('%+d minutes', MAX(COALESCE(et.buffer_before, 0), 0)))
           AND b.end_at >= strftime('%Y-%m-%dT%H:%M:%S', ?, '-2 days', printf('%+d minutes', -MAX(COALESCE(et.buffer_after, 0), 0)))";

/// Pad instants before taking their local envelope. Adding minutes to a
/// local wall clock loses or invents elapsed time at a DST change (#422).
fn buffered_local_range(
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    target: Tz,
    before: i64,
    after: i64,
) -> (NaiveDateTime, NaiveDateTime) {
    let minutes = |m: i64| Duration::minutes(m.clamp(0, i32::MAX.into()));
    crate::time_off::local_range(
        start.checked_sub_signed(minutes(before)).unwrap_or(start),
        end.checked_add_signed(minutes(after)).unwrap_or(end),
        target,
    )
}

pub fn naive(value: &str) -> Option<NaiveDateTime> {
    NaiveDateTime::parse_from_str(value.trim_end_matches('Z'), "%Y-%m-%dT%H:%M:%S")
        .or_else(|_| NaiveDateTime::parse_from_str(value, "%Y-%m-%d %H:%M:%S"))
        .ok()
}

/// UTC records ignore subsequent timezone-setting changes. Legacy records
/// keep their previous interpretation; this never rewrites their timestamps.
pub fn local(value: &str, legacy_tz: Tz, target: Tz) -> Option<NaiveDateTime> {
    if let Ok(instant) = DateTime::parse_from_rfc3339(value) {
        return Some(instant.with_timezone(&target).naive_local());
    }
    let wall = naive(value)?;
    if legacy_tz == target {
        return Some(wall);
    }
    // A wall clock in a skipped hour moves to the first minute after the
    // gap, as time off does; it was read as UTC, hours away.
    Some(
        crate::time_off::to_utc(legacy_tz, wall, false)?
            .with_timezone(&target)
            .naive_local(),
    )
}

/// Wall-clock envelope of a stored booking for the availability engine,
/// which works in local times. Local time runs backwards where the clocks go
/// back, so a booking at the repeated hour can read reversed or short
/// (00:45Z to 01:15Z in Paris on 25 October 2026 reads 02:45 to 02:15).
/// The range is the hull of every local time the booking covers, as for
/// time off ([`crate::time_off::local_range`], #366): an end is moved out
/// only when it lies inside the repeated hour, so a booking that merely
/// spans the change keeps its own ends (#377). Legacy rows retain their
/// historical wall-clock interpretation.
pub fn busy_range(start: &str, end: &str, tz: Tz) -> Option<(NaiveDateTime, NaiveDateTime)> {
    let first = local(start, tz, tz)?;
    let last = local(end, tz, tz)?;
    if let (Ok(start), Ok(end)) = (
        DateTime::parse_from_rfc3339(start),
        DateTime::parse_from_rfc3339(end),
    ) {
        return Some(crate::time_off::local_range(
            start.with_timezone(&Utc),
            end.with_timezone(&Utc),
            tz,
        ));
    }
    Some((first, last))
}

/// A candidate or booking with buffers measured in elapsed minutes. Legacy
/// unmarked timestamps are interpreted in `tz`; callers reading versioned
/// storage use `stored_buffered_busy_range` instead. With no padding, retain
/// `busy_range`'s historical handling of unmarked wall clocks.
pub fn buffered_busy_range(
    start: &str,
    end: &str,
    tz: Tz,
    before: i64,
    after: i64,
) -> Option<(NaiveDateTime, NaiveDateTime)> {
    if before <= 0 && after <= 0 {
        return busy_range(start, end, tz);
    }
    let instant = |value: &str, latest: bool| {
        if let Ok(t) = DateTime::parse_from_rfc3339(value) {
            return Some(t.with_timezone(&Utc));
        }
        crate::time_off::to_utc(tz, naive(value)?, latest)
    };
    Some(buffered_local_range(
        instant(start, false)?,
        instant(end, true)?,
        tz,
        before,
        after,
    ))
}

/// Read a stored booking in a different calendar timezone. Legacy values,
/// including historical values with a misleading Z suffix, are wall clocks
/// in their event's zone. Keep the full possible interval at a repeated hour.
pub fn stored_buffered_busy_range(
    start: &str,
    end: &str,
    time_version: i64,
    legacy_tz: Tz,
    target: Tz,
    (before, after): (i64, i64),
) -> Option<(NaiveDateTime, NaiveDateTime)> {
    if time_version == 1 {
        return buffered_busy_range(start, end, target, before, after);
    }
    let start_wall = naive(start)?;
    let end_wall = naive(end)?;
    let start = crate::time_off::to_utc(legacy_tz, start_wall, false)?;
    let end = crate::time_off::to_utc(legacy_tz, end_wall, true)?;
    Some(buffered_local_range(start, end, target, before, after))
}

/// A new booking must identify one instant. Reject nonexistent/ambiguous local
/// times rather than silently moving a meeting across a daylight-saving change.
pub fn encode(start: NaiveDateTime, tz: Tz, minutes: i32) -> Option<(String, String)> {
    let start = tz.from_local_datetime(&start).single()?.with_timezone(&Utc);
    let end = start.checked_add_signed(Duration::minutes(minutes.into()))?;
    Some((
        start.format("%Y-%m-%dT%H:%M:%SZ").to_string(),
        end.format("%Y-%m-%dT%H:%M:%SZ").to_string(),
    ))
}

/// Resolve the configured zone; database failures must not masquerade as UTC.
pub async fn event_timezone(pool: &SqlitePool, id: &str) -> Result<Tz, sqlx::Error> {
    let zone = sqlx::query_scalar::<_, Option<String>>("SELECT COALESCE(NULLIF(et.timezone, ''), u.timezone) FROM event_types et JOIN accounts a ON a.id = et.account_id LEFT JOIN users u ON u.id = a.user_id WHERE et.id = ?")
        .bind(id).fetch_optional(pool).await?.flatten();
    Ok(zone.and_then(|tz| tz.parse().ok()).unwrap_or(Tz::UTC))
}

/// Frequency limits are calendar periods in the event timezone, not UTC days.
/// Widen the indexed SQL candidate window to cover every UTC offset, then
/// perform the exact comparison after conversion (including DST boundaries).
pub async fn period_counts(
    pool: &SqlitePool,
    event: &str,
    start: NaiveDateTime,
    end: NaiveDateTime,
) -> Result<Vec<(Option<String>, i64)>, sqlx::Error> {
    let tz = event_timezone(pool, event).await?;
    let rows: Vec<(String, Option<String>)> = sqlx::query_as("SELECT CASE time_version WHEN 1 THEN start_at ELSE rtrim(start_at, 'Z') END AS start_at, assigned_user_id FROM bookings WHERE event_type_id = ? AND status IN ('confirmed', 'pending') AND start_at >= ? AND start_at < ?")
        .bind(event).bind((start - Duration::days(2)).to_string()).bind((end + Duration::days(2)).to_string())
        .fetch_all(pool).await?;
    let mut counts = std::collections::BTreeMap::new();
    for (value, member) in rows {
        if local(&value, tz, tz).is_some_and(|v| v >= start && v < end) {
            *counts.entry(member).or_insert(0) += 1;
        }
    }
    Ok(counts.into_iter().collect())
}

/// Local wall-clock strings for presentation only. Unmarked legacy values
/// remain unchanged at call sites that historically displayed them directly.
pub fn wall_strings(start: &str, end: &str, target: Tz) -> (String, String) {
    let render = |v: &str| {
        local(v, target, target)
            .map(|d| d.format("%Y-%m-%dT%H:%M:%S").to_string())
            .unwrap_or_else(|| v.to_owned())
    };
    (render(start), render(end))
}

pub async fn guest_wall_strings(
    pool: &SqlitePool,
    key: &str,
    start: &str,
    end: &str,
) -> (String, String) {
    if !start.ends_with('Z') {
        return (start.to_owned(), end.to_owned());
    }
    let zone: Option<String> = sqlx::query_scalar("SELECT guest_timezone FROM bookings WHERE id = ?1 OR uid = ?1 OR cancel_token = ?1 OR confirm_token = ?1 OR reschedule_token = ?1")
        .bind(key).fetch_optional(pool).await.ok().flatten();
    wall_strings(
        start,
        end,
        zone.and_then(|z| z.parse().ok()).unwrap_or(Tz::UTC),
    )
}

/// Preserve both absolute endpoints in calendar attachments, including meetings
/// spanning midnight or a clock change. Legacy email conversion is unchanged.
pub fn ics_times(start: &str, end: &str) -> Option<(String, String)> {
    let start = DateTime::parse_from_rfc3339(start)
        .ok()?
        .with_timezone(&Utc);
    let end = DateTime::parse_from_rfc3339(end).ok()?.with_timezone(&Utc);
    Some((
        start.format("%Y%m%dT%H%M%SZ").to_string(),
        end.format("%Y%m%dT%H%M%SZ").to_string(),
    ))
}

/// The existing unique index protects UTC-vs-UTC inserts. Also preserve its
/// whole-slot exclusion for legacy rows (notably pending bookings), whose text
/// representation differs from a new UTC timestamp for the same instant.
pub async fn legacy_slot_taken(
    pool: &SqlitePool,
    event: &str,
    member: Option<&str>,
    utc_start: &str,
    exclude: &str,
) -> anyhow::Result<bool> {
    let tz = event_timezone(pool, event).await?;
    let target = local(utc_start, tz, tz);
    let starts: Vec<String> = sqlx::query_scalar("SELECT CASE time_version WHEN 1 THEN start_at ELSE rtrim(start_at, 'Z') END AS start_at FROM bookings WHERE event_type_id = ? AND time_version = 0 AND status IN ('confirmed','pending') AND COALESCE(assigned_user_id,'') = ? AND id != ?")
        .bind(event).bind(member.unwrap_or("")).bind(exclude).fetch_all(pool).await?;
    Ok(starts.iter().any(|s| local(s, tz, tz) == target))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seasonal_offsets_and_legacy_values() {
        let paris: Tz = "Europe/Paris".parse().unwrap();
        for (wall, utc) in [
            ("2026-07-01T14:00:00", "2026-07-01T12:00:00Z"),
            ("2026-01-01T14:00:00", "2026-01-01T13:00:00Z"),
        ] {
            let (start, _) = encode(naive(wall).unwrap(), paris, 30).unwrap();
            assert_eq!(start, utc);
            assert_eq!(local(&start, Tz::UTC, paris), naive(wall));
            assert_eq!(local(wall, paris, paris), naive(wall));
        }
    }

    #[test]
    fn ambiguous_or_nonexistent_start_is_rejected() {
        let paris = "Europe/Paris".parse().unwrap();
        assert!(encode(naive("2026-03-29T02:30:00").unwrap(), paris, 30).is_none());
        assert!(encode(naive("2026-10-25T02:30:00").unwrap(), paris, 30).is_none());
    }

    #[test]
    fn calendar_endpoints_survive_midnight_and_dst() {
        let paris = "Europe/Paris".parse().unwrap();
        for (wall, minutes, expected_start, expected_end) in [
            (
                "2026-03-29T01:30:00",
                120,
                "20260329T003000Z",
                "20260329T023000Z",
            ),
            (
                "2026-10-25T01:30:00",
                180,
                "20261024T233000Z",
                "20261025T023000Z",
            ),
            (
                "2026-07-01T23:45:00",
                60,
                "20260701T214500Z",
                "20260701T224500Z",
            ),
        ] {
            let (s, e) = encode(naive(wall).unwrap(), paris, minutes).unwrap();
            let details = crate::email::BookingDetails {
                utc_times: ics_times(&s, &e),
                date: "deliberately-unused".into(),
                guest_timezone: "Pacific/Honolulu".into(),
                ..Default::default()
            };
            let ics = crate::email::generate_ics(&details, "REQUEST");
            assert!(ics.contains(&format!("DTSTART:{expected_start}")));
            assert!(ics.contains(&format!("DTEND:{expected_end}")));
        }
    }

    /// One zone's 2026 clock changes: the fall-back instant, the repeated
    /// wall-clock span it creates, and the spring-forward instant.
    struct Changes {
        zone: &'static str,
        fall_back: &'static str,
        repeated: (&'static str, &'static str),
        spring_forward: &'static str,
    }

    const CHANGES: [Changes; 4] = [
        Changes {
            zone: "Europe/Paris",
            fall_back: "2026-10-25T01:00:00Z",
            repeated: ("2026-10-25T02:00:00", "2026-10-25T03:00:00"),
            spring_forward: "2026-03-29T01:00:00Z",
        },
        Changes {
            zone: "America/New_York",
            fall_back: "2026-11-01T06:00:00Z",
            repeated: ("2026-11-01T01:00:00", "2026-11-01T02:00:00"),
            spring_forward: "2026-03-08T07:00:00Z",
        },
        Changes {
            zone: "Australia/Sydney",
            fall_back: "2026-04-04T16:00:00Z",
            repeated: ("2026-04-05T02:00:00", "2026-04-05T03:00:00"),
            spring_forward: "2026-10-03T16:00:00Z",
        },
        // A 30-minute change: +11:00 back to +10:30, +10:30 on to +11:00.
        Changes {
            zone: "Australia/Lord_Howe",
            fall_back: "2026-04-04T15:00:00Z",
            repeated: ("2026-04-05T01:30:00", "2026-04-05T02:00:00"),
            spring_forward: "2026-10-03T15:30:00Z",
        },
    ];

    fn instant(value: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(value)
            .unwrap()
            .with_timezone(&Utc)
    }

    fn range(start: DateTime<Utc>, end: DateTime<Utc>, tz: Tz) -> (NaiveDateTime, NaiveDateTime) {
        let rfc = |t: DateTime<Utc>| t.format("%Y-%m-%dT%H:%M:%SZ").to_string();
        busy_range(&rfc(start), &rfc(end), tz).unwrap()
    }

    #[test]
    fn busy_range_widens_only_an_end_inside_the_repeated_hour() {
        for c in &CHANGES {
            let tz: Tz = c.zone.parse().unwrap();
            let change = instant(c.fall_back);
            let wall = |t: DateTime<Utc>| t.with_timezone(&tz).naive_local();
            let (repeat_low, repeat_high) =
                (naive(c.repeated.0).unwrap(), naive(c.repeated.1).unwrap());
            let shift = repeat_high - repeat_low;
            assert_eq!(wall(change), repeat_low, "{}: the change", c.zone);
            assert_eq!(
                wall(change - Duration::seconds(1)) + Duration::seconds(1),
                repeat_high,
                "{}: the instant before the change",
                c.zone
            );
            let hour = Duration::hours(2); // more than any shift

            // Across the change, both ends outside the repeated hour: the
            // booking's own ends, nothing outside it is blocked.
            assert_eq!(
                range(change - hour, change + hour, tz),
                (wall(change - hour), wall(change + hour)),
                "{}: spanning the change",
                c.zone
            );
            // From the first occurrence into the second: the whole repeated
            // hour, no more, although the plain ends read the same.
            let half = shift / 2;
            assert_eq!(wall(change + half), wall(change - half), "{}", c.zone);
            assert_eq!(
                range(change - half, change + half, tz),
                (repeat_low, repeat_high),
                "{}: inside the repeated hour",
                c.zone
            );
            // Starting in the first occurrence: only the start moves back.
            assert_eq!(
                range(change - half, change + hour, tz),
                (repeat_low, wall(change + hour)),
                "{}: start in the first occurrence",
                c.zone
            );
            // Ending in the second occurrence: only the end moves out.
            assert_eq!(
                range(change - hour, change + half, tz),
                (wall(change - hour), repeat_high),
                "{}: end in the second occurrence",
                c.zone
            );
            // Ending before the change (in the first occurrence): untouched.
            assert_eq!(
                range(change - hour, change - half, tz),
                (wall(change - hour), wall(change - half)),
                "{}: before the change",
                c.zone
            );
            // Starting after the change (in the second occurrence): untouched.
            assert_eq!(
                range(change + half, change + hour, tz),
                (wall(change + half), wall(change + hour)),
                "{}: after the change",
                c.zone
            );
        }
    }

    #[test]
    fn busy_range_adds_nothing_at_a_spring_forward() {
        for c in &CHANGES {
            let tz: Tz = c.zone.parse().unwrap();
            let change = instant(c.spring_forward);
            let wall = |t: DateTime<Utc>| t.with_timezone(&tz).naive_local();
            let before = change - Duration::seconds(1);
            assert!(
                wall(change) - wall(before) > Duration::seconds(1),
                "{}: the clocks jump at the change",
                c.zone
            );
            for (start, end) in [
                (change - Duration::hours(2), change + Duration::hours(2)),
                (
                    change - Duration::minutes(30),
                    change + Duration::minutes(15),
                ),
                (change, change + Duration::hours(1)),
                (change - Duration::hours(1), change),
            ] {
                assert_eq!(
                    range(start, end, tz),
                    (wall(start), wall(end)),
                    "{}: {start} to {end}",
                    c.zone
                );
            }
        }
    }

    #[test]
    fn busy_range_keeps_legacy_wall_clocks() {
        let paris: Tz = "Europe/Paris".parse().unwrap();
        assert_eq!(
            busy_range("2026-10-25T02:30:00", "2026-10-25T03:30:00", paris),
            Some((
                naive("2026-10-25T02:30:00").unwrap(),
                naive("2026-10-25T03:30:00").unwrap()
            ))
        );
    }

    #[test]
    fn booking_buffers_measure_elapsed_minutes_at_clock_changes() {
        for (zone, start, end, before, after, expected_start, expected_end) in [
            (
                "Europe/Paris",
                "2027-03-28T00:00:00Z",
                "2027-03-28T00:30:00Z",
                0,
                120,
                "2027-03-28T01:00:00",
                "2027-03-28T04:30:00",
            ),
            (
                "Europe/Paris",
                "2027-03-28T01:30:00Z",
                "2027-03-28T02:00:00Z",
                120,
                0,
                "2027-03-28T00:30:00",
                "2027-03-28T04:00:00",
            ),
            (
                "Europe/Paris",
                "2027-10-31T00:00:00Z",
                "2027-10-31T00:30:00Z",
                0,
                120,
                "2027-10-31T02:00:00",
                "2027-10-31T03:30:00",
            ),
            // The before-buffer starts in the repeated hour: retain its
            // conservative local envelope, but do not invent another hour.
            (
                "Europe/Paris",
                "2027-10-31T02:30:00Z",
                "2027-10-31T03:00:00Z",
                120,
                0,
                "2027-10-31T02:00:00",
                "2027-10-31T04:00:00",
            ),
            // Lord Howe jumps by half an hour, not a whole hour.
            (
                "Australia/Lord_Howe",
                "2026-10-03T15:00:00Z",
                "2026-10-03T15:15:00Z",
                0,
                60,
                "2026-10-04T01:30:00",
                "2026-10-04T03:15:00",
            ),
        ] {
            assert_eq!(
                buffered_busy_range(start, end, zone.parse().unwrap(), before, after),
                Some((naive(expected_start).unwrap(), naive(expected_end).unwrap())),
                "{zone}, {start}, before={before}, after={after}",
            );
        }
    }

    /// A legacy wall clock that does not exist (inside the spring gap) is
    /// read as the first minute after the gap, as time off reads it, not as
    /// UTC, which moved it by the zone's offset once buffers were applied.
    #[test]
    fn a_legacy_time_in_the_spring_gap_moves_past_the_gap() {
        let paris: Tz = "Europe/Paris".parse().unwrap();
        assert_eq!(
            buffered_busy_range("2027-03-28T02:30:00", "2027-03-28T03:30:00", paris, 15, 0),
            Some((
                naive("2027-03-28T01:45:00").unwrap(),
                naive("2027-03-28T03:30:00").unwrap()
            )),
        );
        assert_eq!(
            stored_buffered_busy_range(
                "2027-03-28T02:30:00",
                "2027-03-28T03:30:00",
                0,
                paris,
                Tz::UTC,
                (0, 0),
            ),
            Some((
                naive("2027-03-28T01:00:00").unwrap(),
                naive("2027-03-28T01:30:00").unwrap()
            )),
        );
        assert_eq!(
            local("2027-03-28T02:30:00", paris, Tz::UTC),
            naive("2027-03-28T01:00:00")
        );
    }

    #[test]
    fn booking_buffers_preserve_legacy_timezone_and_time_version() {
        let paris = "Europe/Paris".parse().unwrap();
        let expected = Some((
            naive("2027-03-28T00:00:00").unwrap(),
            naive("2027-03-28T02:30:00").unwrap(),
        ));
        for suffix in ["", "Z"] {
            // A legacy Z is a misleading marker, not an actual UTC instant.
            assert_eq!(
                stored_buffered_busy_range(
                    &format!("2027-03-28T01:00:00{suffix}"),
                    &format!("2027-03-28T01:30:00{suffix}"),
                    0,
                    paris,
                    Tz::UTC,
                    (0, 120),
                ),
                expected,
            );
        }
        assert_eq!(
            stored_buffered_busy_range(
                "2027-03-28T00:00:00Z",
                "2027-03-28T00:30:00Z",
                1,
                paris,
                Tz::UTC,
                (0, 120),
            ),
            expected,
        );
        assert_eq!(
            buffered_busy_range("2027-03-28T01:00:00", "2027-03-28T01:30:00", paris, 0, 120),
            Some((
                naive("2027-03-28T01:00:00").unwrap(),
                naive("2027-03-28T04:30:00").unwrap()
            )),
        );
    }

    #[tokio::test]
    async fn upgrade_preserves_existing_timestamps_and_marks_only_new_writes() {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::raw_sql("CREATE TABLE bookings(id TEXT PRIMARY KEY, start_at TEXT, end_at TEXT); INSERT INTO bookings VALUES ('old','2026-07-01T14:00:00','2026-07-01T14:30:00');").execute(&pool).await.unwrap();
        sqlx::raw_sql(include_str!("../migrations/064_booking_time_version.sql"))
            .execute(&pool)
            .await
            .unwrap();
        let old: (String, String, i64) =
            sqlx::query_as("SELECT start_at,end_at,time_version FROM bookings WHERE id='old'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(
            old,
            (
                "2026-07-01T14:00:00".into(),
                "2026-07-01T14:30:00".into(),
                0
            )
        );
        assert!(sqlx::query(
            "INSERT INTO bookings VALUES ('bad','2026-07-01T14:00:00','2026-07-01T14:30:00',1)"
        )
        .execute(&pool)
        .await
        .is_err());
        sqlx::query(
            "INSERT INTO bookings VALUES ('new','2026-07-01T12:00:00Z','2026-07-01T12:30:00Z',1)",
        )
        .execute(&pool)
        .await
        .unwrap();
        assert!(
            sqlx::query("UPDATE bookings SET start_at='2026-07-01T14:00:00' WHERE id='new'")
                .execute(&pool)
                .await
                .is_err()
        );
    }
}
