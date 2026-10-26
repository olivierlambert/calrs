//! Slot computation, busy times, frequency limits, timezones, and the troubleshoot view.

use super::*;

/// Parse availability windows from the form.
/// Supports new `avail_windows` format ("09:00-12:00,13:00-17:00") with fallback to
/// legacy single `avail_start`/`avail_end` pair. Returns at least one window.
pub(super) fn parse_avail_windows(
    windows_str: Option<&str>,
    legacy_start: Option<&str>,
    legacy_end: Option<&str>,
) -> Vec<(String, String)> {
    if let Some(ws) = windows_str.filter(|s| !s.trim().is_empty()) {
        let parsed: Vec<(String, String)> = ws
            .split(',')
            .filter_map(|w| {
                let parts: Vec<&str> = w.trim().splitn(2, '-').collect();
                if parts.len() == 2
                    && NaiveTime::parse_from_str(parts[0], "%H:%M").is_ok()
                    && NaiveTime::parse_from_str(parts[1], "%H:%M").is_ok()
                {
                    Some((parts[0].to_string(), parts[1].to_string()))
                } else {
                    None
                }
            })
            .collect();
        if !parsed.is_empty() {
            return parsed;
        }
    }
    // Fallback to legacy single window
    vec![(
        legacy_start.unwrap_or("09:00").to_string(),
        legacy_end.unwrap_or("17:00").to_string(),
    )]
}

/// Parse a single schedule string in the new format.
/// Format: "1:09:00-17:00;2:09:00-12:00,13:00-17:00" (day:windows;day:windows)
fn parse_schedule_string(s: &str) -> std::collections::BTreeMap<i32, Vec<(String, String)>> {
    let mut result = std::collections::BTreeMap::new();
    let s = s.trim();
    if s.is_empty() {
        return result;
    }
    for segment in s.split(';') {
        let segment = segment.trim();
        if segment.is_empty() {
            continue;
        }
        let parts: Vec<&str> = segment.splitn(2, ':').collect();
        if parts.len() != 2 {
            continue;
        }
        let day: i32 = match parts[0].trim().parse() {
            Ok(d) if (0..=6).contains(&d) => d,
            _ => continue,
        };
        let windows: Vec<(String, String)> = parts[1]
            .split(',')
            .filter_map(|w| {
                let times: Vec<&str> = w.trim().split('-').collect();
                if times.len() == 2 {
                    let s = times[0].trim().to_string();
                    let e = times[1].trim().to_string();
                    if !s.is_empty() && !e.is_empty() {
                        Some((s, e))
                    } else {
                        None
                    }
                } else {
                    None
                }
            })
            .collect();
        if !windows.is_empty() {
            result.insert(day, windows);
        }
    }
    result
}

/// Parse per-day availability schedule.
/// Resolution order: submitted `schedule` (new format) → `user_default` (new format,
/// e.g. the user's profile default availability) → legacy form fields → hardcoded
/// Mon–Fri 09:00–17:00. The `user_default` step is what stops an empty submission
/// from silently snapping back to the hardcoded default.
pub(super) fn parse_avail_schedule(
    schedule: Option<&str>,
    legacy_days: Option<&str>,
    legacy_windows: Option<&str>,
    legacy_start: Option<&str>,
    legacy_end: Option<&str>,
    user_default: Option<&str>,
) -> std::collections::BTreeMap<i32, Vec<(String, String)>> {
    if let Some(s) = schedule {
        let parsed = parse_schedule_string(s);
        if !parsed.is_empty() {
            return parsed;
        }
    }

    if let Some(s) = user_default {
        let parsed = parse_schedule_string(s);
        if !parsed.is_empty() {
            return parsed;
        }
    }

    // Fall back to legacy format
    let mut result = std::collections::BTreeMap::new();
    let days_str = legacy_days.unwrap_or("1,2,3,4,5");
    let windows = parse_avail_windows(legacy_windows, legacy_start, legacy_end);
    for day_str in days_str.split(',') {
        if let Ok(day) = day_str.trim().parse::<i32>() {
            if (0..=6).contains(&day) {
                result.insert(day, windows.clone());
            }
        }
    }
    result
}

/// Build an avail_schedule string from availability rules.
/// Output format: "1:09:00-17:00;2:09:00-12:00,13:00-17:00"
pub(super) fn build_avail_schedule(all_rules: &[(i32, String, String)]) -> String {
    let mut day_map: std::collections::BTreeMap<i32, Vec<String>> =
        std::collections::BTreeMap::new();
    for (day, start, end) in all_rules {
        day_map
            .entry(*day)
            .or_default()
            .push(format!("{}-{}", start, end));
    }
    day_map
        .iter()
        .map(|(day, ws)| format!("{}:{}", day, ws.join(",")))
        .collect::<Vec<_>>()
        .join(";")
}

/// Convert a user's default availability rules into "busy" times for hours OUTSIDE
/// their available windows. This lets us constrain dynamic group link participants
/// by their working hours without changing the slot computation engine.
/// Convert a user's default availability rules into "busy" times for hours OUTSIDE
/// their available windows. Times are converted from the participant's timezone to
/// the host's timezone so they integrate correctly with the slot computation.
pub(super) async fn user_avail_as_busy(
    pool: &SqlitePool,
    user_id: &str,
    window_start: NaiveDateTime,
    window_end: NaiveDateTime,
    host_tz: chrono_tz::Tz,
) -> Vec<(NaiveDateTime, NaiveDateTime)> {
    let rules: Vec<(i32, String, String)> = sqlx::query_as(
        "SELECT day_of_week, start_time, end_time FROM user_availability_rules WHERE user_id = ? ORDER BY day_of_week, start_time",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    // No rules = no constraint (user hasn't set default availability)
    if rules.is_empty() {
        return vec![];
    }

    // Get the participant's timezone
    let user_tz_str: String = sqlx::query_scalar("SELECT timezone FROM users WHERE id = ?")
        .bind(user_id)
        .fetch_one(pool)
        .await
        .unwrap_or_else(|_| "UTC".to_string());
    let user_tz: chrono_tz::Tz = user_tz_str.parse().unwrap_or(chrono_tz::Tz::UTC);

    // Work in the participant's timezone: convert window bounds from host TZ to user TZ
    let window_start_user = host_tz
        .from_local_datetime(&window_start)
        .earliest()
        .map(|dt| dt.with_timezone(&user_tz).naive_local())
        .unwrap_or(window_start);
    let window_end_user = host_tz
        .from_local_datetime(&window_end)
        .earliest()
        .map(|dt| dt.with_timezone(&user_tz).naive_local())
        .unwrap_or(window_end);

    let mut busy = Vec::new();
    let mut date = window_start_user.date();
    let end_date = window_end_user.date();
    while date <= end_date {
        let weekday = date.weekday().num_days_from_sunday() as i32;
        let mut windows: Vec<(NaiveTime, NaiveTime)> = rules
            .iter()
            .filter(|(d, _, _)| *d == weekday)
            .filter_map(|(_, s, e)| {
                let start = NaiveTime::parse_from_str(s, "%H:%M").ok()?;
                let end = NaiveTime::parse_from_str(e, "%H:%M").ok()?;
                Some((start, end))
            })
            .collect();
        windows.sort_by_key(|(s, _)| *s);

        if windows.is_empty() {
            // Entire day is unavailable
            let day_start = date.and_hms_opt(0, 0, 0).unwrap();
            let day_end = date.and_hms_opt(23, 59, 59).unwrap();
            busy.push((day_start, day_end));
        } else {
            // Block hours outside available windows
            let day_start = date.and_hms_opt(0, 0, 0).unwrap();
            let first_avail = date.and_time(windows[0].0);
            if first_avail > day_start {
                busy.push((day_start, first_avail));
            }
            for i in 0..windows.len() - 1 {
                let gap_start = date.and_time(windows[i].1);
                let gap_end = date.and_time(windows[i + 1].0);
                if gap_end > gap_start {
                    busy.push((gap_start, gap_end));
                }
            }
            let last_avail_end = date.and_time(windows.last().unwrap().1);
            let day_end = date.and_hms_opt(23, 59, 59).unwrap();
            if day_end > last_avail_end {
                busy.push((last_avail_end, day_end));
            }
        }
        date = date.succ_opt().unwrap_or(date);
    }

    // Convert busy times from participant's TZ back to host's TZ
    busy.into_iter()
        .filter_map(|(start, end)| {
            let start_host = user_tz
                .from_local_datetime(&start)
                .earliest()
                .map(|dt| dt.with_timezone(&host_tz).naive_local())?;
            let end_host = user_tz
                .from_local_datetime(&end)
                .earliest()
                .map(|dt| dt.with_timezone(&host_tz).naive_local())?;
            Some((start_host, end_host))
        })
        .collect()
}

/// Load a user's default availability rules as a serialized schedule string.
/// Format: "1:09:00-17:00;2:09:00-12:00,13:00-17:00" (day:start-end,start-end;...)
pub(super) async fn load_user_avail_schedule(pool: &SqlitePool, user_id: &str) -> String {
    let rules: Vec<(i32, String, String)> = sqlx::query_as(
        "SELECT day_of_week, start_time, end_time FROM user_availability_rules WHERE user_id = ? ORDER BY day_of_week, start_time",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    let mut parts = Vec::new();
    let mut current_day: Option<i32> = None;
    let mut windows = Vec::new();
    for (day, start, end) in &rules {
        if current_day != Some(*day) {
            if let Some(d) = current_day {
                parts.push(format!("{}:{}", d, windows.join(",")));
            }
            current_day = Some(*day);
            windows.clear();
        }
        windows.push(format!("{}-{}", start, end));
    }
    if let Some(d) = current_day {
        parts.push(format!("{}:{}", d, windows.join(",")));
    }
    parts.join(";")
}

/// Save a user's default availability rules from a serialized schedule string.
pub(super) async fn save_user_avail_schedule(pool: &SqlitePool, user_id: &str, schedule: &str) {
    // Delete existing rules
    let _ = sqlx::query("DELETE FROM user_availability_rules WHERE user_id = ?")
        .bind(user_id)
        .execute(pool)
        .await;

    // Parse and insert new rules
    for seg in schedule.split(';') {
        let seg = seg.trim();
        if seg.is_empty() {
            continue;
        }
        let parts: Vec<&str> = seg.splitn(2, ':').collect();
        if parts.len() < 2 {
            continue;
        }
        let day: i32 = match parts[0].parse() {
            Ok(d) if (0..=6).contains(&d) => d,
            _ => continue,
        };
        for window in parts[1].split(',') {
            let times: Vec<&str> = window.trim().split('-').collect();
            if times.len() != 2 {
                continue;
            }
            let id = uuid::Uuid::new_v4().to_string();
            let _ = sqlx::query(
                "INSERT INTO user_availability_rules (id, user_id, day_of_week, start_time, end_time) VALUES (?, ?, ?, ?, ?)",
            )
            .bind(&id)
            .bind(user_id)
            .bind(day)
            .bind(times[0].trim())
            .bind(times[1].trim())
            .execute(pool)
            .await;
        }
    }
}

/// Ensure a user has default availability rules. If none exist, insert Mon-Fri 9:00-17:00.
pub(super) async fn ensure_user_avail_seeded(pool: &SqlitePool, user_id: &str) {
    let count: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM user_availability_rules WHERE user_id = ?")
            .bind(user_id)
            .fetch_one(pool)
            .await
            .unwrap_or((0,));
    if count.0 > 0 {
        return;
    }
    // Seed Mon(1)-Fri(5) 09:00-17:00
    for day in 1..=5 {
        let id = uuid::Uuid::new_v4().to_string();
        let _ = sqlx::query(
            "INSERT INTO user_availability_rules (id, user_id, day_of_week, start_time, end_time) VALUES (?, ?, ?, '09:00', '17:00')",
        )
        .bind(&id)
        .bind(user_id)
        .bind(day)
        .execute(pool)
        .await;
    }
}

// --- Slot computation (shared with CLI) ---

/// Pick an available team member for a booking slot.
/// Returns (user_id, name, email) of the member with fewest recent bookings.
pub(super) async fn pick_group_member(
    pool: &SqlitePool,
    team_id: &str,
    event_type_id: &str,
    slot_start: NaiveDateTime,
    slot_end: NaiveDateTime,
    actual_start: NaiveDateTime,
    buffer_before: i32,
    buffer_after: i32,
    host_tz: Tz,
) -> Option<(String, String, String)> {
    let buf_start = slot_start - Duration::minutes(buffer_before as i64);
    let buf_end = slot_end + Duration::minutes(buffer_after as i64);

    // Fetch members with per-event-type weight (fallback to default 1)
    // weight=0 means excluded from this event type
    let members: Vec<(String, String, String, i64)> = sqlx::query_as(
        "SELECT u.id, u.name, COALESCE(u.booking_email, u.email), \
         COALESCE(etw.weight, 1) \
         FROM users u JOIN team_members tm ON tm.user_id = u.id \
         LEFT JOIN event_type_member_weights etw ON etw.user_id = u.id AND etw.event_type_id = ? \
         WHERE tm.team_id = ? AND u.enabled = 1 \
         AND COALESCE(etw.weight, 1) > 0",
    )
    .bind(event_type_id)
    .bind(team_id)
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    // Per-member booking-frequency caps. We exclude any member already at
    // (or over) their per-period cap so the picker doesn't pick them just to
    // have the submit-time check reject the booking.
    let per_member_limits: Result<Vec<(i32, String)>, sqlx::Error> = sqlx::query_as(
        "SELECT max_bookings, period FROM booking_frequency_limits \
         WHERE event_type_id = ? AND per_member = 1",
    )
    .bind(event_type_id)
    .fetch_all(pool)
    .await;
    let per_member_limits = match per_member_limits {
        Ok(limits) => limits,
        Err(error) => {
            tracing::error!(%error, %event_type_id, "cannot load member booking frequency limits");
            return None;
        }
    };

    let mut available_members = Vec::new();

    for (user_id, name, email, weight) in &members {
        let mut busy = fetch_busy_times_for_user(
            pool,
            user_id,
            buf_start,
            buf_end,
            host_tz,
            Some(event_type_id),
        )
        .await;
        // Also exclude members who are outside their own working hours for
        // this slot. Members without explicit user_availability_rules are
        // returned unconstrained by user_avail_as_busy, matching the slot
        // grid semantics in show_group_slots.
        busy.extend(user_avail_as_busy(pool, user_id, buf_start, buf_end, host_tz).await);
        if has_conflict(&busy, buf_start, buf_end) {
            continue;
        }

        // Pending bookings mark a member busy above only when their event
        // type has `pending_blocks_slot` on, but the per-member unique index
        // counts them always: re-picking a member with a pending same-slot assignment
        // would just bounce the INSERT (#146). Deliberately broader than
        // the index (any event type, buffered window): a member with an
        // overlapping pending commitment anywhere is a bad pick — if both
        // get approved they would be double-booked in real life.
        let assigned: Vec<(String, String)> = sqlx::query_as(
            "SELECT CASE time_version WHEN 1 THEN start_at ELSE rtrim(start_at, 'Z') END AS start_at, CASE time_version WHEN 1 THEN end_at ELSE rtrim(end_at, 'Z') END AS end_at FROM bookings WHERE assigned_user_id = ? AND status IN ('confirmed', 'pending') AND start_at < strftime('%Y-%m-%dT%H:%M:%S', ?, '+2 days') AND end_at > strftime('%Y-%m-%dT%H:%M:%S', ?, '-2 days')")
            .bind(user_id).bind(buf_end.to_string()).bind(buf_start.to_string())
            .fetch_all(pool).await.unwrap_or_default();
        if assigned.iter().any(
            |(s, e)| match crate::booking_time::busy_range(s, e, host_tz) {
                Some((s, e)) => s < buf_end && e > buf_start,
                _ => false,
            },
        ) {
            continue;
        }

        let mut at_per_member_cap = false;
        for (max, period) in &per_member_limits {
            let (rs, re) = frequency_period_range(actual_start, period);
            let counts = match crate::booking_time::period_counts(pool, event_type_id, rs, re).await
            {
                Ok(counts) => counts,
                Err(error) => {
                    tracing::error!(%error, %event_type_id, "cannot check member booking frequency");
                    return None;
                }
            };
            let count: i64 = counts
                .into_iter()
                .filter(|(uid, _)| uid.as_deref() == Some(user_id))
                .map(|(_, n)| n)
                .sum();
            if count >= *max as i64 {
                at_per_member_cap = true;
                break;
            }
        }
        if at_per_member_cap {
            continue;
        }

        available_members.push((user_id.clone(), name.clone(), email.clone(), *weight));
    }

    if available_members.is_empty() {
        return None;
    }

    // Among available members, pick by highest weight first, then fewest bookings in last 30 days
    let thirty_days_ago = (Utc::now() - Duration::days(30))
        .format("%Y-%m-%dT%H:%M:%S")
        .to_string();
    let mut best: Option<(String, String, String, i64, i64)> = None;

    for (user_id, name, email, weight) in &available_members {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM bookings WHERE assigned_user_id = ? AND created_at >= ?",
        )
        .bind(user_id)
        .bind(&thirty_days_ago)
        .fetch_one(pool)
        .await
        .unwrap_or(0);

        let is_better = match &best {
            None => true,
            Some((_, _, _, bw, bc)) => *weight > *bw || (*weight == *bw && count < *bc),
        };
        if is_better {
            best = Some((user_id.clone(), name.clone(), email.clone(), *weight, count));
        }
    }

    best.map(|(uid, name, email, _, _)| (uid, name, email))
}

pub(super) struct SlotDay {
    pub(super) date: String,
    pub(super) label: String,
    pub(super) slots: Vec<SlotTime>,
}

pub(super) struct SlotTime {
    pub(super) start: String,      // guest TZ display
    pub(super) end: String,        // guest TZ display
    pub(super) host_date: String,  // YYYY-MM-DD in host TZ (for booking)
    pub(super) host_time: String,  // HH:MM in host TZ (for booking)
    pub(super) guest_date: String, // YYYY-MM-DD in guest TZ (for grouping by day)
}

// --- Shared busy-time helpers ---

/// Expand recurring events into (start, end) pairs within a time window.
/// Tuples are (start_at, end_at, rrule, raw_ical, timezone).
pub(crate) fn expand_recurring_into_busy(
    recurring: &[(String, String, String, Option<String>, Option<String>)],
    window_start: NaiveDateTime,
    window_end: NaiveDateTime,
    host_tz: Tz,
) -> Vec<(NaiveDateTime, NaiveDateTime)> {
    let mut result = Vec::new();
    for (s, e, rrule_str, raw_ical, event_tz) in recurring {
        if let (Some(ev_start), Some(ev_end)) = (parse_ical_datetime(s), parse_ical_datetime(e)) {
            let exdates = raw_ical
                .as_deref()
                .map(|raw| crate::rrule::extract_exdates(raw, event_tz.as_deref()))
                .unwrap_or_default();
            // Expand RRULE in the event's own timezone (correct for DST)
            let occurrences = crate::rrule::expand_rrule(
                ev_start,
                ev_end,
                rrule_str,
                &exdates,
                window_start,
                window_end,
            );
            // Convert each occurrence to host timezone
            for (os, oe) in occurrences {
                let cs = convert_event_to_tz(os, event_tz.as_deref(), host_tz);
                let ce = convert_event_to_tz(oe, event_tz.as_deref(), host_tz);
                result.push((cs, ce));
            }
        }
    }
    result
}

/// Fetch busy times for a specific user (events from their calendars + their bookings).
/// Event times are converted from their stored timezone to `host_tz`.
pub(super) async fn fetch_busy_times_for_user(
    pool: &SqlitePool,
    user_id: &str,
    window_start: NaiveDateTime,
    window_end: NaiveDateTime,
    host_tz: Tz,
    event_type_id: Option<&str>,
) -> Vec<(NaiveDateTime, NaiveDateTime)> {
    fetch_busy_times_for_user_ex(
        pool,
        user_id,
        window_start,
        window_end,
        host_tz,
        event_type_id,
        None,
        None,
    )
    .await
}

pub(super) async fn fetch_busy_times_for_user_ex(
    pool: &SqlitePool,
    user_id: &str,
    window_start: NaiveDateTime,
    window_end: NaiveDateTime,
    host_tz: Tz,
    event_type_id: Option<&str>,
    exclude_booking_id: Option<&str>,
    exclude_booking_uid: Option<&str>,
) -> Vec<(NaiveDateTime, NaiveDateTime)> {
    fetch_busy_times_for_user_opts(
        pool,
        user_id,
        window_start,
        window_end,
        host_tz,
        event_type_id,
        exclude_booking_id,
        exclude_booking_uid,
        true,
    )
    .await
}

/// [`fetch_busy_times_for_user_ex`], choosing whether pending requests that
/// hold their slot count as busy. Slot pages and booking checks count them;
/// the approval re-check does not (see [`approval_hosts_free`]).
#[allow(clippy::too_many_arguments)]
pub(super) async fn fetch_busy_times_for_user_opts(
    pool: &SqlitePool,
    user_id: &str,
    window_start: NaiveDateTime,
    window_end: NaiveDateTime,
    host_tz: Tz,
    event_type_id: Option<&str>,
    exclude_booking_id: Option<&str>,
    exclude_booking_uid: Option<&str>,
    include_pending_holds: bool,
) -> Vec<(NaiveDateTime, NaiveDateTime)> {
    // Events are stored in compact iCal form ("YYYYMMDDTHHMMSS" for timed,
    // "YYYYMMDD" for all-day), so both overlap bounds must also be compact
    // with a time component. Using a date-only upper bound here ("YYYYMMDD")
    // is a bug: a timed event sorts lexically *after* its own date prefix
    // ("20260618T100000" > "20260618"), so a same-day window (window_end on the
    // same date as the event, e.g. the single-slot window in pick_group_member)
    // would silently miss the event and treat a busy member as free.
    let end_compact = window_end.format("%Y%m%dT%H%M%S").to_string();
    let start_compact = window_start.format("%Y%m%dT%H%M%S").to_string();
    // ISO bounds are for the bookings sub-query below (bookings store ISO times).
    let end_iso = window_end.format("%Y-%m-%dT%H:%M:%S").to_string();
    let start_iso = window_start.format("%Y-%m-%dT%H:%M:%S").to_string();

    // Empty string means NOT EXISTS is always true (no rows match), so all calendars pass
    let et_id_for_filter = event_type_id.unwrap_or("");
    // A booking pushed to CalDAV comes back through sync as an `events` row
    // with the same UID. Excluding only its `bookings` row would therefore
    // still make the booking conflict with its own calendar copy.
    let exclude_uid = exclude_booking_uid.unwrap_or("");

    let events: Vec<(String, String, Option<String>)> = sqlx::query_as(
        "SELECT e.start_at, e.end_at, e.timezone FROM events e
         JOIN calendars c ON c.id = e.calendar_id
         JOIN caldav_sources cs ON cs.id = c.source_id
         JOIN accounts a ON a.id = cs.account_id
         WHERE a.user_id = ?
           AND ((c.is_busy = 1 AND NOT EXISTS (SELECT 1 FROM event_type_calendars WHERE event_type_id = ?))
                OR c.id IN (SELECT calendar_id FROM event_type_calendars WHERE event_type_id = ?))
           AND (e.rrule IS NULL OR e.rrule = '')
           AND (e.status IS NULL OR e.status != 'CANCELLED')
           AND (e.transp IS NULL OR e.transp != 'TRANSPARENT')
           AND (? = '' OR e.uid != ?)
           AND e.start_at <= ? AND e.end_at >= ?",
    )
    .bind(user_id)
    .bind(et_id_for_filter)
    .bind(et_id_for_filter)
    .bind(exclude_uid)
    .bind(exclude_uid)
    .bind(&end_compact)
    .bind(&start_compact)
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    let mut busy: Vec<(NaiveDateTime, NaiveDateTime)> = events
        .iter()
        .filter_map(|(s, e, tz)| {
            let start = convert_event_to_tz(parse_ical_datetime(s)?, tz.as_deref(), host_tz);
            let end = convert_event_to_tz(parse_ical_datetime(e)?, tz.as_deref(), host_tz);
            Some((start, end))
        })
        .collect();

    let end_compact_rrule = window_end.format("%Y%m%dT235959").to_string();
    let recurring: Vec<(String, String, String, Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT e.start_at, e.end_at, e.rrule, e.raw_ical, e.timezone FROM events e
         JOIN calendars c ON c.id = e.calendar_id
         JOIN caldav_sources cs ON cs.id = c.source_id
         JOIN accounts a ON a.id = cs.account_id
         WHERE a.user_id = ?
           AND ((c.is_busy = 1 AND NOT EXISTS (SELECT 1 FROM event_type_calendars WHERE event_type_id = ?))
                OR c.id IN (SELECT calendar_id FROM event_type_calendars WHERE event_type_id = ?))
           AND (e.status IS NULL OR e.status != 'CANCELLED')
           AND (e.transp IS NULL OR e.transp != 'TRANSPARENT')
           AND (? = '' OR e.uid != ?)
           AND e.rrule IS NOT NULL AND e.rrule != '' AND (e.start_at <= ? OR e.start_at <= ?)",
    )
    .bind(user_id)
    .bind(et_id_for_filter)
    .bind(et_id_for_filter)
    .bind(exclude_uid)
    .bind(exclude_uid)
    .bind(&end_iso)
    .bind(&end_compact_rrule)
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    busy.extend(expand_recurring_into_busy(
        &recurring,
        window_start,
        window_end,
        host_tz,
    ));

    let exclude_id = exclude_booking_id.unwrap_or("");
    // A booking makes a user busy when it is assigned to them, or when it
    // is unassigned and they own the event type (personal bookings), or
    // when it is an unassigned team booking (collective mode) and they are
    // a member of that team. An assigned booking must NOT mark the event
    // type owner busy: on a round-robin team the owner stays free when
    // another member took the booking (#146).
    //
    // Pending requests count too when their own event type says so
    // (`pending_blocks_slot`, on by default): otherwise a second guest is
    // offered the slot, fills the form, and is bounced at submit, and an
    // overlapping slot on another event type could be requested as well (#234).
    let bookings: Vec<(String, String)> = sqlx::query_as(&format!(
        "SELECT CASE b.time_version WHEN 1 THEN b.start_at ELSE rtrim(b.start_at, 'Z') END AS start_at, CASE b.time_version WHEN 1 THEN b.end_at ELSE rtrim(b.end_at, 'Z') END AS end_at FROM bookings b
         JOIN event_types et ON et.id = b.event_type_id
         JOIN accounts a ON a.id = et.account_id
         WHERE {}
           AND b.start_at <= strftime('%Y-%m-%dT%H:%M:%S', ?, '+2 days') AND b.end_at >= strftime('%Y-%m-%dT%H:%M:%S', ?, '-2 days')
           AND (? = '' OR b.id != ?)",
        crate::booking_time::user_busy_bookings(include_pending_holds)
    ))
    .bind(user_id)
    .bind(user_id)
    .bind(user_id)
    .bind(&end_iso)
    .bind(&start_iso)
    .bind(exclude_id)
    .bind(exclude_id)
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    for (s, e) in &bookings {
        if let Some((start, end)) = crate::booking_time::busy_range(s, e, host_tz) {
            busy.push((start, end));
        }
    }

    busy
}

/// Check if any busy period overlaps with [buf_start, buf_end).
pub(super) fn has_conflict(
    busy: &[(NaiveDateTime, NaiveDateTime)],
    buf_start: NaiveDateTime,
    buf_end: NaiveDateTime,
) -> bool {
    busy.iter().any(|(s, e)| *s < buf_end && *e > buf_start)
}

// --- Busy source for unified slot computation ---

pub(super) enum BusySource {
    /// Flat list of busy times (individual event type)
    Individual(Vec<(NaiveDateTime, NaiveDateTime)>),
    /// Per-member busy times; slot is available if ANY member is free
    Group(HashMap<String, Vec<(NaiveDateTime, NaiveDateTime)>>),
    /// Per-member busy times; slot is available only if ALL members are free
    Team(HashMap<String, Vec<(NaiveDateTime, NaiveDateTime)>>),
}

/// Whether one slot (buffers already applied) is free under a [`BusySource`].
/// Shared by the slot grid and the submit-time re-checks so the two can never
/// disagree about what a busy source means.
pub(super) fn busy_source_is_free(
    busy: &BusySource,
    buf_start: NaiveDateTime,
    buf_end: NaiveDateTime,
) -> bool {
    match busy {
        BusySource::Individual(times) => !has_conflict(times, buf_start, buf_end),
        BusySource::Group(member_busy) => member_busy
            .values()
            .any(|times| !has_conflict(times, buf_start, buf_end)),
        BusySource::Team(member_busy) => {
            !member_busy.is_empty()
                && member_busy
                    .values()
                    .all(|times| !has_conflict(times, buf_start, buf_end))
        }
    }
}

/// Compute available slots for an event type.
/// Caller provides pre-fetched busy times via BusySource.
pub(super) async fn compute_slots(
    pool: &SqlitePool,
    et_id: &str,
    duration: i32,
    buffer_before: i32,
    buffer_after: i32,
    min_notice: i32,
    start_offset: i32,
    days_ahead: i32,
    booking_horizon_days: Option<i32>,
    host_tz: Tz,
    guest_tz: Tz,
    busy: BusySource,
    exclude_booking_id: Option<&str>,
) -> Vec<SlotDay> {
    // Shared-resource blocking: intervals during which the event type's
    // required resources are unavailable ('all' mode: any resource busy;
    // 'round_robin': every resource busy). Extending every member's busy
    // list blocks the slot under all three BusySource semantics.
    let now_host = chrono::Utc::now().with_timezone(&host_tz).naive_local();
    let resource_window_end =
        now_host + chrono::Duration::days((start_offset + days_ahead + 1) as i64);
    let resource_block = crate::resources::blocking_intervals_for_event_type(
        pool,
        et_id,
        now_host,
        resource_window_end,
        host_tz,
        exclude_booking_id,
    )
    .await;
    let busy = if resource_block.is_empty() {
        busy
    } else {
        match busy {
            BusySource::Individual(mut v) => {
                v.extend(resource_block.iter().cloned());
                BusySource::Individual(v)
            }
            BusySource::Group(mut m) => {
                for v in m.values_mut() {
                    v.extend(resource_block.iter().cloned());
                }
                BusySource::Group(m)
            }
            BusySource::Team(mut m) => {
                for v in m.values_mut() {
                    v.extend(resource_block.iter().cloned());
                }
                BusySource::Team(m)
            }
        }
    };

    let rules: Vec<(i32, String, String)> = sqlx::query_as(
        "SELECT day_of_week, start_time, end_time FROM availability_rules WHERE event_type_id = ?",
    )
    .bind(et_id)
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    // Fetch availability overrides for this event type
    let overrides: Vec<(String, Option<String>, Option<String>, i32)> = sqlx::query_as(
        "SELECT date, start_time, end_time, is_blocked FROM availability_overrides WHERE event_type_id = ? ORDER BY date, start_time",
    )
    .bind(et_id)
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    // slot_interval_min overrides the cursor step. NULL = use duration (legacy behavior).
    let slot_interval: Option<i32> =
        sqlx::query_scalar("SELECT slot_interval_min FROM event_types WHERE id = ?")
            .bind(et_id)
            .fetch_one(pool)
            .await
            .unwrap_or(None);
    let interval = slot_interval.filter(|v| *v > 0).unwrap_or(duration);

    let mut result = compute_slots_from_rules(
        &rules,
        duration,
        interval,
        buffer_before,
        buffer_after,
        min_notice,
        start_offset,
        days_ahead,
        booking_horizon_days,
        host_tz,
        guest_tz,
        busy,
        &overrides,
    );

    // Drop slots whose host-local period already meets/exceeds a configured
    // frequency cap. The submit-time check (would_exceed_frequency_limit)
    // stays as a backstop for the race where two guests grab the last slot
    // at the same instant.
    apply_frequency_limit_filter(pool, et_id, &mut result).await;

    // If first_slot_only is enabled, keep only the earliest slot per day
    let first_only: i32 =
        sqlx::query_scalar("SELECT first_slot_only FROM event_types WHERE id = ?")
            .bind(et_id)
            .fetch_one(pool)
            .await
            .unwrap_or(0);

    if first_only != 0 {
        for day in &mut result {
            day.slots.truncate(1);
        }
    }

    result
}

/// The span a submit-time [`is_offered_slot`] check recomputes slots over:
/// the target's host-local day, two days either side (a guest's calendar day
/// can straddle up to three host days, and `first_slot_only` keeps the first
/// slot per guest day), never before today. `busy_start`/`busy_end` is the
/// window the caller fetches busy times for, padded by a day so buffers at
/// the edges see the same events the slot page does.
pub(super) struct OfferedSlotWindow {
    pub(super) start_offset: i32,
    pub(super) days_ahead: i32,
    pub(super) busy_start: NaiveDateTime,
    pub(super) busy_end: NaiveDateTime,
}

pub(super) fn offered_slot_window(host_tz: Tz, slot_start: NaiveDateTime) -> OfferedSlotWindow {
    let today = Utc::now().with_timezone(&host_tz).naive_local().date();
    // Booking forms cap the date at a year ahead before this runs, so the
    // day count always fits.
    let target = (slot_start.date() - today).num_days().clamp(-1, 100_000) as i32;
    let start_offset = (target - 2).max(0);
    let end_offset = (target + 2).max(start_offset);
    let day = |offset: i32| today + Duration::days(offset as i64);
    OfferedSlotWindow {
        start_offset,
        days_ahead: end_offset - start_offset + 1,
        busy_start: day(start_offset - 1).and_time(NaiveTime::MIN),
        busy_end: day(end_offset + 1)
            .and_hms_opt(23, 59, 59)
            .unwrap_or_else(|| day(end_offset + 1).and_time(NaiveTime::MIN)),
    }
}

/// Whether `slot_start` (host-local) is a slot the booking page would offer,
/// recomputed with [`compute_slots`] itself: weekly rules, date overrides
/// (blocked days and custom hours), minimum notice, horizon, slot interval
/// alignment, frequency limits, resources, `first_slot_only`, and the busy
/// times in `busy` (fetched by the caller over `window`, with the same
/// [`BusySource`] shape as the slot page for that URL). A submitted time is
/// only ever a claim about what the page showed; this is what makes the
/// server hold the page to it.
#[allow(clippy::too_many_arguments)]
pub(super) async fn is_offered_slot(
    pool: &SqlitePool,
    et_id: &str,
    duration: i32,
    buffer_before: i32,
    buffer_after: i32,
    min_notice: i32,
    host_tz: Tz,
    guest_tz: Tz,
    window: &OfferedSlotWindow,
    busy: BusySource,
    exclude_booking_id: Option<&str>,
    slot_start: NaiveDateTime,
) -> bool {
    let horizon = get_booking_horizon(pool, et_id).await;
    let days = compute_slots(
        pool,
        et_id,
        duration,
        buffer_before,
        buffer_after,
        min_notice,
        window.start_offset,
        window.days_ahead,
        horizon,
        host_tz,
        guest_tz,
        busy,
        exclude_booking_id,
    )
    .await;
    let date = slot_start.format("%Y-%m-%d").to_string();
    let time = slot_start.format("%H:%M").to_string();
    days.iter()
        .flat_map(|d| &d.slots)
        .any(|s| s.host_date == date && s.host_time == time)
}

/// Busy times of a team event type's members over a window, as the team slot
/// page and the team booking check both see them: every enabled member not
/// excluded by a zero weight, constrained to their own working hours.
/// Collective needs every member free, round robin any one.
pub(super) async fn team_busy_source(
    pool: &SqlitePool,
    et_id: &str,
    team_id: &str,
    collective: bool,
    window_start: NaiveDateTime,
    window_end: NaiveDateTime,
    host_tz: Tz,
) -> BusySource {
    let mut member_busy = HashMap::new();
    for user_id in team_member_ids(pool, et_id, team_id).await {
        let mut busy = fetch_busy_times_for_user(
            pool,
            &user_id,
            window_start,
            window_end,
            host_tz,
            Some(et_id),
        )
        .await;
        // Members without explicit hours in user_availability_rules come back
        // unconstrained (user_avail_as_busy short-circuits to an empty Vec),
        // so this never plants a surprise 9-17 default.
        busy.extend(user_avail_as_busy(pool, &user_id, window_start, window_end, host_tz).await);
        member_busy.insert(user_id, busy);
    }
    if collective {
        BusySource::Team(member_busy)
    } else {
        BusySource::Group(member_busy)
    }
}

/// The members a team event type can be booked with: enabled, and not
/// excluded from this event type by a zero weight.
pub(super) async fn team_member_ids(pool: &SqlitePool, et_id: &str, team_id: &str) -> Vec<String> {
    sqlx::query_scalar(
        "SELECT u.id FROM users u JOIN team_members tm ON tm.user_id = u.id \
         LEFT JOIN event_type_member_weights etw ON etw.user_id = u.id AND etw.event_type_id = ? \
         WHERE tm.team_id = ? AND u.enabled = 1 \
         AND COALESCE(etw.weight, 1) > 0",
    )
    .bind(et_id)
    .bind(team_id)
    .fetch_all(pool)
    .await
    .unwrap_or_default()
}

/// Busy times of a dynamic group link (`/u/alice+bob/slug`) over a window.
/// The first user owns the event type, so only their calendars are filtered
/// by its calendar selection; the others are constrained to their default
/// working hours. Everyone must be free.
pub(super) async fn dynamic_group_busy_source(
    pool: &SqlitePool,
    et_id: &str,
    user_ids: &[String],
    window_start: NaiveDateTime,
    window_end: NaiveDateTime,
    host_tz: Tz,
) -> BusySource {
    let mut member_busy = HashMap::new();
    for (i, user_id) in user_ids.iter().enumerate() {
        let et_filter = if i == 0 { Some(et_id) } else { None };
        let mut busy =
            fetch_busy_times_for_user(pool, user_id, window_start, window_end, host_tz, et_filter)
                .await;
        if i > 0 {
            ensure_user_avail_seeded(pool, user_id).await;
            busy.extend(user_avail_as_busy(pool, user_id, window_start, window_end, host_tz).await);
        }
        member_busy.insert(user_id.clone(), busy);
    }
    BusySource::Team(member_busy)
}

/// Remove slots that fall inside a host-local period already at/over a
/// configured booking-frequency cap, so the picker doesn't surface times
/// that the submit-time check would reject.
///
/// Per-member limits hide a slot only when *every* eligible team member is
/// already at their cap for the slot's period; if any one of them still has
/// headroom, the slot stays available and the round-robin picker will route
/// to that member. On personal event types (no team), per-member limits
/// degrade to event-type-wide behaviour. Query failures hide the slots and
/// are logged, so unavailable counts never look like spare capacity.
pub(super) async fn apply_frequency_limit_filter(
    pool: &SqlitePool,
    et_id: &str,
    days: &mut [SlotDay],
) {
    let limits: Result<Vec<(i32, String, i32)>, sqlx::Error> = sqlx::query_as(
        "SELECT max_bookings, period, per_member FROM booking_frequency_limits WHERE event_type_id = ?",
    )
    .bind(et_id)
    .fetch_all(pool)
    .await;
    let limits = match limits {
        Ok(limits) => limits,
        Err(error) => {
            tracing::error!(%error, "cannot load booking frequency limits");
            for day in days {
                day.slots.clear();
            }
            return;
        }
    };

    if limits.is_empty() {
        return;
    }

    let has_per_member = limits.iter().any(|(_, _, m)| *m != 0);
    let team_members: Vec<String> = if has_per_member {
        sqlx::query_scalar(
            "SELECT u.id FROM users u \
             JOIN team_members tm ON tm.user_id = u.id \
             JOIN event_types et ON et.team_id = tm.team_id \
             LEFT JOIN event_type_member_weights etw ON etw.user_id = u.id AND etw.event_type_id = et.id \
             WHERE et.id = ? AND u.enabled = 1 AND COALESCE(etw.weight, 1) > 0",
        )
        .bind(et_id)
        .fetch_all(pool)
        .await
        .unwrap_or_default()
    } else {
        Vec::new()
    };

    // Gather unique (period, period_start) pairs the visible slots touch, and
    // resolve each one to (event-type-wide count, per-member counts).
    let mut wide_counts: HashMap<(String, NaiveDateTime), i64> = HashMap::new();
    let mut member_counts: HashMap<(String, NaiveDateTime, String), i64> = HashMap::new();

    for day in days.iter() {
        for slot in &day.slots {
            let Ok(d) = NaiveDate::parse_from_str(&slot.host_date, "%Y-%m-%d") else {
                continue;
            };
            let dt = d.and_hms_opt(12, 0, 0).unwrap();
            for (_, period, _) in &limits {
                let (ps, _) = frequency_period_range(dt, period);
                wide_counts.entry((period.clone(), ps)).or_insert(0);
            }
        }
    }

    for ((period, ps), wide) in wide_counts.iter_mut() {
        let (rs, re) = frequency_period_range(*ps, period);
        let counts = match crate::booking_time::period_counts(pool, et_id, rs, re).await {
            Ok(counts) => counts,
            Err(error) => {
                tracing::error!(%error, %et_id, "cannot check slot booking frequency");
                for day in days {
                    day.slots.clear();
                }
                return;
            }
        };
        *wide = counts.iter().map(|(_, n)| n).sum();
        for (uid, n) in counts {
            if let Some(uid) = uid {
                member_counts.insert((period.clone(), *ps, uid), n);
            }
        }
    }

    for day in days.iter_mut() {
        day.slots.retain(|slot| {
            let Ok(d) = NaiveDate::parse_from_str(&slot.host_date, "%Y-%m-%d") else {
                return true;
            };
            let dt = d.and_hms_opt(12, 0, 0).unwrap();
            for (max, period, per_member) in &limits {
                let (ps, _) = frequency_period_range(dt, period);
                let max_i64 = *max as i64;
                if *per_member != 0 && !team_members.is_empty() {
                    // Hide only if every eligible member is at cap.
                    let all_capped = team_members.iter().all(|uid| {
                        let count = member_counts
                            .get(&(period.clone(), ps, uid.clone()))
                            .copied()
                            .unwrap_or(0);
                        count >= max_i64
                    });
                    if all_capped {
                        return false;
                    }
                } else {
                    let count = wide_counts.get(&(period.clone(), ps)).copied().unwrap_or(0);
                    if count >= max_i64 {
                        return false;
                    }
                }
            }
            true
        });
    }
}

/// Save booking frequency limits from the serialized form field.
/// Format per row: "count:period" or "count:period:m" (the trailing ":m"
/// marks the row as per-member; absence means event-type-wide). Rows are
/// comma-separated.
pub(super) async fn save_frequency_limits(
    pool: &SqlitePool,
    event_type_id: &str,
    limits_str: &str,
) {
    if limits_str.is_empty() {
        return;
    }
    for part in limits_str.split(',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        let mut bits = part.splitn(3, ':');
        let Some(count_str) = bits.next() else {
            continue;
        };
        let Some(period) = bits.next() else {
            continue;
        };
        let per_member = matches!(bits.next(), Some("m"));
        let count: i32 = count_str.parse().unwrap_or(0);
        if count > 0 && ["day", "week", "month", "year"].contains(&period) {
            let limit_id = uuid::Uuid::new_v4().to_string();
            let _ = sqlx::query(
                "INSERT INTO booking_frequency_limits (id, event_type_id, max_bookings, period, per_member) VALUES (?, ?, ?, ?, ?)",
            )
            .bind(&limit_id)
            .bind(event_type_id)
            .bind(count)
            .bind(period)
            .bind(per_member as i32)
            .execute(pool)
            .await;
        }
    }
}

/// Compute the start/end of a calendar period containing the given datetime.
fn frequency_period_range(dt: NaiveDateTime, period: &str) -> (NaiveDateTime, NaiveDateTime) {
    let date = dt.date();
    match period {
        "day" => {
            let start = date.and_hms_opt(0, 0, 0).unwrap();
            let end = (date + Duration::days(1)).and_hms_opt(0, 0, 0).unwrap();
            (start, end)
        }
        "week" => {
            let weekday = date.weekday().num_days_from_monday();
            let week_start = date - Duration::days(weekday as i64);
            let start = week_start.and_hms_opt(0, 0, 0).unwrap();
            let end = (week_start + Duration::days(7))
                .and_hms_opt(0, 0, 0)
                .unwrap();
            (start, end)
        }
        "month" => {
            let start = NaiveDate::from_ymd_opt(date.year(), date.month(), 1)
                .unwrap()
                .and_hms_opt(0, 0, 0)
                .unwrap();
            let end = if date.month() == 12 {
                NaiveDate::from_ymd_opt(date.year() + 1, 1, 1).unwrap()
            } else {
                NaiveDate::from_ymd_opt(date.year(), date.month() + 1, 1).unwrap()
            }
            .and_hms_opt(0, 0, 0)
            .unwrap();
            (start, end)
        }
        "year" => {
            let start = NaiveDate::from_ymd_opt(date.year(), 1, 1)
                .unwrap()
                .and_hms_opt(0, 0, 0)
                .unwrap();
            let end = NaiveDate::from_ymd_opt(date.year() + 1, 1, 1)
                .unwrap()
                .and_hms_opt(0, 0, 0)
                .unwrap();
            (start, end)
        }
        _ => (dt, dt),
    }
}

/// Check if a booking at the given datetime would exceed any frequency limit
/// configured on the event type. `assigned_user_id` is the team member the
/// booking would land on — required for per-member caps; ignored by
/// event-type-wide caps. Pass `None` for personal event types (no assignee).
/// Returns true on query failure to reject the booking; the error is logged.
pub(super) async fn would_exceed_frequency_limit(
    pool: &SqlitePool,
    event_type_id: &str,
    proposed_start: NaiveDateTime,
    assigned_user_id: Option<&str>,
) -> bool {
    let limits: Result<Vec<(i32, String, i32)>, sqlx::Error> = sqlx::query_as(
        "SELECT max_bookings, period, per_member FROM booking_frequency_limits WHERE event_type_id = ?",
    )
    .bind(event_type_id)
    .fetch_all(pool)
    .await;
    let limits = match limits {
        Ok(limits) => limits,
        Err(error) => {
            tracing::error!(%error, "cannot load booking frequency limits");
            return true;
        }
    };

    if limits.is_empty() {
        return false;
    }

    for (max_bookings, period, per_member) in &limits {
        let (range_start, range_end) = frequency_period_range(proposed_start, period);
        let counts = match crate::booking_time::period_counts(
            pool,
            event_type_id,
            range_start,
            range_end,
        )
        .await
        {
            Ok(counts) => counts,
            Err(error) => {
                tracing::error!(%error, %event_type_id, "cannot check booking frequency; rejecting booking");
                return true;
            }
        };
        let count = (counts
            .into_iter()
            .filter(|(uid, _)| {
                *per_member == 0 || assigned_user_id.is_none() || uid.as_deref() == assigned_user_id
            })
            .map(|(_, n)| n)
            .sum::<i64>(),);

        if count.0 >= *max_bookings as i64 {
            return true;
        }
    }
    false
}

/// Core slot computation from pre-fetched rules.
pub(super) fn compute_slots_from_rules(
    rules: &[(i32, String, String)],
    duration: i32,
    interval: i32,
    buffer_before: i32,
    buffer_after: i32,
    min_notice: i32,
    start_offset: i32,
    days_ahead: i32,
    booking_horizon_days: Option<i32>,
    host_tz: Tz,
    guest_tz: Tz,
    busy: BusySource,
    overrides: &[(String, Option<String>, Option<String>, i32)],
) -> Vec<SlotDay> {
    let now_host = Utc::now().with_timezone(&host_tz).naive_local();
    let min_start = Utc::now() + Duration::minutes(min_notice as i64);

    let slot_duration = Duration::minutes(duration as i64);
    let slot_step = Duration::minutes(interval.max(1) as i64);
    let mut result = Vec::new();

    for day_offset in start_offset..(start_offset + days_ahead) {
        // Rolling booking horizon. `day_offset` counts days from today in the
        // host timezone, which is exactly what the horizon bounds, so it can
        // be compared directly. Offsets only increase, so stop rather than skip.
        if booking_horizon_days.is_some_and(|days| day_offset > days) {
            break;
        }

        let date = now_host.date() + Duration::days(day_offset as i64);
        let date_str = date.format("%Y-%m-%d").to_string();

        // Check availability overrides for this date
        let day_overrides: Vec<&(String, Option<String>, Option<String>, i32)> = overrides
            .iter()
            .filter(|(d, _, _, _)| *d == date_str)
            .collect();

        // If any override blocks this day, skip entirely
        if day_overrides.iter().any(|(_, _, _, blocked)| *blocked != 0) {
            continue;
        }

        // Build time windows: use custom hours if overrides exist, else weekly rules
        let windows: Vec<(String, String)> = if !day_overrides.is_empty() {
            // Custom hours overrides replace weekly rules
            day_overrides
                .iter()
                .filter_map(|(_, s, e, _)| match (s, e) {
                    (Some(start), Some(end)) => Some((start.clone(), end.clone())),
                    _ => None,
                })
                .collect()
        } else {
            let weekday = date.weekday().num_days_from_sunday() as i32;
            rules
                .iter()
                .filter(|(d, _, _)| *d == weekday)
                .map(|(_, s, e)| (s.clone(), e.clone()))
                .collect()
        };

        if windows.is_empty() {
            continue;
        }

        let mut day_slots = Vec::new();

        for (start_str, end_str) in &windows {
            let window_start_time = match NaiveTime::parse_from_str(start_str, "%H:%M") {
                Ok(t) => t,
                Err(_) => continue,
            };
            let window_end_time = match NaiveTime::parse_from_str(end_str, "%H:%M") {
                Ok(t) => t,
                Err(_) => continue,
            };

            // Walk the cursor as a NaiveDateTime, not a NaiveTime: NaiveTime +
            // Duration wraps at 24h, which turned a window ending at 23:00 with
            // a 60-minute slot duration into an infinite loop (23:00 + 60m =
            // 00:00, still <= 23:00 as a time-of-day, so the loop emitted a
            // slot every step forever until OOM).
            let window_end = date.and_time(window_end_time);
            let mut cursor = date.and_time(window_start_time);
            while cursor < window_end {
                let Some(slot_start_utc) = host_tz
                    .from_local_datetime(&cursor)
                    .single()
                    .map(|t| t.with_timezone(&Utc))
                else {
                    cursor += slot_step;
                    continue;
                };
                let slot_end_utc = slot_start_utc + slot_duration;
                let guest_start = slot_start_utc.with_timezone(&guest_tz);
                let guest_end = slot_end_utc.with_timezone(&guest_tz);
                let (check_start, check_end) = crate::booking_time::busy_range(
                    &slot_start_utc.to_rfc3339(),
                    &slot_end_utc.to_rfc3339(),
                    host_tz,
                )
                .unwrap();
                // The submission carries wall time only: do not advertise a
                // guest time that cannot identify a unique instant on POST.
                if slot_start_utc < min_start
                    || check_start < date.and_time(window_start_time)
                    || check_end > window_end
                    || guest_tz
                        .from_local_datetime(&guest_start.naive_local())
                        .single()
                        .is_none()
                {
                    cursor += slot_step;
                    continue;
                }
                let buf_start = check_start - Duration::minutes(buffer_before as i64);
                let buf_end = check_end + Duration::minutes(buffer_after as i64);

                if busy_source_is_free(&busy, buf_start, buf_end) {
                    day_slots.push(SlotTime {
                        start: guest_start.format("%H:%M").to_string(),
                        end: guest_end.format("%H:%M").to_string(),
                        host_date: date.format("%Y-%m-%d").to_string(),
                        host_time: cursor.format("%H:%M").to_string(),
                        guest_date: guest_start.format("%Y-%m-%d").to_string(),
                    });
                }

                cursor += slot_step;
            }
        }

        if !day_slots.is_empty() {
            let mut guest_days: std::collections::BTreeMap<String, Vec<SlotTime>> =
                std::collections::BTreeMap::new();
            for slot in day_slots {
                guest_days
                    .entry(slot.guest_date.clone())
                    .or_default()
                    .push(slot);
            }
            for (guest_date_str, slots) in guest_days {
                if let Ok(gd) = NaiveDate::parse_from_str(&guest_date_str, "%Y-%m-%d") {
                    if !result.iter().any(|d: &SlotDay| d.date == guest_date_str) {
                        result.push(SlotDay {
                            date: guest_date_str,
                            label: gd.format("%A, %B %-d").to_string(),
                            slots,
                        });
                    } else if let Some(existing) =
                        result.iter_mut().find(|d| d.date == guest_date_str)
                    {
                        existing.slots.extend(slots);
                    }
                }
            }
        }
    }

    result.sort_by(|a, b| a.date.cmp(&b.date));
    for day in &mut result {
        day.slots.sort_by(|a, b| a.start.cmp(&b.start));
    }
    result
}

/// Parse a guest timezone, using UTC when none is supplied.
pub(super) fn parse_guest_tz(tz: Option<&str>) -> Tz {
    tz.and_then(|s| s.parse().ok()).unwrap_or(Tz::UTC)
}

/// Normalize a timezone value submitted via an event-type form. Accepts the
/// input only if it parses as a valid IANA timezone, otherwise returns the
/// fallback (typically the submitting user's timezone).
pub(super) fn normalize_event_type_tz(input: Option<&str>, fallback: &str) -> String {
    input
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .filter(|s| s.parse::<Tz>().is_ok())
        .map(str::to_string)
        .unwrap_or_else(|| fallback.to_string())
}

/// Timezone to store when a form that already holds one is saved (#265).
/// A value posted back unchanged is kept as it is, even one chrono-tz cannot
/// parse (an old alias or a name from a newer tzdb), so a save that did not
/// touch the field never changes it. Any other value must parse; when it
/// does not, the stored one is kept.
pub(super) fn resolve_saved_timezone(input: Option<&str>, stored: &str) -> String {
    match input.map(str::trim) {
        Some(s) if s == stored => stored.to_string(),
        Some(s) if !s.is_empty() && s.parse::<Tz>().is_ok() => s.to_string(),
        _ => stored.to_string(),
    }
}

/// Timezone to store when an existing event type is saved. `stored` is the
/// column (`None` when NULL) and `follows` the timezone a NULL row follows,
/// its owner's profile one (see `get_host_tz`). A NULL row stays NULL when
/// the form posts back that timezone, which is what the edit form shows for
/// it, so editing something else does not pin the event type to today's
/// profile timezone (#265).
pub(super) fn resolve_event_type_tz(
    input: Option<&str>,
    stored: Option<&str>,
    follows: &str,
) -> Option<String> {
    match stored.filter(|s| !s.is_empty()) {
        Some(stored) => Some(resolve_saved_timezone(input, stored)),
        None => {
            let s = input.map(str::trim).unwrap_or_default();
            (s != follows && s.parse::<Tz>().is_ok()).then(|| s.to_string())
        }
    }
}

/// An event type's own timezone column, `None` when NULL or empty.
pub(super) async fn stored_event_type_tz(pool: &SqlitePool, et_id: &str) -> Option<String> {
    sqlx::query_scalar::<_, Option<String>>("SELECT timezone FROM event_types WHERE id = ?")
        .bind(et_id)
        .fetch_optional(pool)
        .await
        .unwrap_or(None)
        .flatten()
        .filter(|s| !s.is_empty())
}

/// The timezone an event type with no timezone of its own follows: its
/// owner's profile timezone, as `get_host_tz` resolves it, else UTC.
pub(super) async fn event_type_followed_tz(pool: &SqlitePool, et_id: &str) -> String {
    sqlx::query_scalar::<_, String>(
        "SELECT u.timezone
         FROM event_types et
         JOIN accounts a ON a.id = et.account_id
         JOIN users u ON u.id = a.user_id
         WHERE et.id = ?",
    )
    .bind(et_id)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten()
    .filter(|tz| tz.parse::<Tz>().is_ok())
    .unwrap_or_else(|| server_tz().name().to_string())
}

/// Get the host's timezone from the event type owner's profile.
/// Falls back to UTC when no valid timezone is configured.
/// Convert a naive datetime in the guest's timezone to the equivalent naive
/// datetime in the host's timezone. Used when accepting a booking: the URL
/// carries the time the guest clicked (their local time), but availability
/// checks, storage, and the existing display code all assume host-local.
pub(super) fn guest_to_host_local(
    guest_local: NaiveDateTime,
    guest_tz: Tz,
    host_tz: Tz,
) -> NaiveDateTime {
    use chrono::TimeZone;
    let utc = guest_tz
        .from_local_datetime(&guest_local)
        .earliest()
        .unwrap_or_else(|| guest_tz.from_utc_datetime(&guest_local))
        .with_timezone(&Utc);
    utc.with_timezone(&host_tz).naive_local()
}

/// Get the rolling booking horizon for an event type, in days.
/// `None` (the stored NULL) means a guest may book arbitrarily far ahead.
pub(super) async fn get_booking_horizon(pool: &SqlitePool, et_id: &str) -> Option<i32> {
    sqlx::query_scalar::<_, Option<i32>>(
        "SELECT booking_horizon_days FROM event_types WHERE id = ?",
    )
    .bind(et_id)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten()
    .flatten()
}

/// The last host-local date a guest may book, for a given horizon.
/// `None` horizon means unlimited, so no date is ever past it. A horizon that
/// runs off the end of the representable calendar is unlimited for the same
/// reason, so `checked_add_signed` degrades to `None` rather than panicking
/// (`NaiveDate + TimeDelta` panics on overflow). The day loop in
/// `compute_slots_from_rules` treats such a horizon as unreachable too.
///
/// A negative horizon is only reachable by writing the column directly, since
/// `parse_optional_day_count` rejects one. It has to mean "nothing bookable",
/// so it is clamped to yesterday. Left to the arithmetic, a large negative
/// overflows to `None` and reads as unlimited, which is the wrong direction
/// to fail in.
pub(super) fn horizon_last_date(
    now_host: NaiveDateTime,
    horizon: Option<i32>,
) -> Option<NaiveDate> {
    horizon.and_then(|days| {
        let date = now_host.date();
        if days < 0 {
            return date.pred_opt();
        }
        date.checked_add_signed(Duration::days(days as i64))
    })
}

pub(super) async fn get_host_tz(pool: &SqlitePool, et_id: &str) -> Tz {
    if !et_id.is_empty() {
        // Prefer the explicit event-type timezone (migration 046). Falls back
        // to the account owner's timezone for rows where it is still NULL.
        if let Some(tz_str) = sqlx::query_scalar::<_, String>(
            "SELECT COALESCE(NULLIF(et.timezone, ''), u.timezone)
             FROM event_types et
             JOIN accounts a ON a.id = et.account_id
             JOIN users u ON u.id = a.user_id
             WHERE et.id = ?",
        )
        .bind(et_id)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten()
        {
            if let Ok(tz) = tz_str.parse::<Tz>() {
                return tz;
            }
        }
    }
    server_tz()
}

/// Get a user's timezone from their profile. Falls back to UTC.
async fn get_user_tz(pool: &SqlitePool, user_id: &str) -> Tz {
    if let Some(tz_str) = sqlx::query_scalar::<_, String>("SELECT timezone FROM users WHERE id = ?")
        .bind(user_id)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten()
    {
        if let Ok(tz) = tz_str.parse::<Tz>() {
            return tz;
        }
    }
    server_tz()
}

/// Deterministic default for missing timezone configuration.
fn server_tz() -> Tz {
    Tz::UTC
}

/// Common IANA timezones, shown first in every timezone picker with a
/// friendly label.
const COMMON_TIMEZONES: &[(&str, &str)] = &[
    ("Pacific/Midway", "Midway"),
    ("Pacific/Honolulu", "Hawaii"),
    ("America/Anchorage", "Alaska"),
    ("America/Los_Angeles", "Los Angeles"),
    ("America/Denver", "Denver"),
    ("America/Chicago", "Chicago"),
    ("America/New_York", "New York"),
    ("America/Sao_Paulo", "São Paulo"),
    ("Atlantic/Cape_Verde", "Cape Verde"),
    ("UTC", "UTC"),
    ("Europe/London", "London"),
    ("Europe/Paris", "Paris, Brussels"),
    ("Europe/Helsinki", "Helsinki, Kyiv"),
    ("Europe/Moscow", "Moscow"),
    ("Asia/Dubai", "Dubai"),
    ("Asia/Kolkata", "India"),
    ("Asia/Bangkok", "Bangkok"),
    ("Asia/Shanghai", "Shanghai"),
    ("Asia/Tokyo", "Tokyo"),
    ("Australia/Sydney", "Sydney"),
    ("Pacific/Auckland", "Auckland"),
];

/// IANA areas listed in the full picker, one `<optgroup>` each. Ids outside
/// them (`US/Eastern`, `Etc/GMT+3`, `EST5EDT`) are legacy aliases.
const TIMEZONE_AREAS: &[&str] = &[
    "Africa",
    "America",
    "Antarctica",
    "Arctic",
    "Asia",
    "Atlantic",
    "Australia",
    "Europe",
    "Indian",
    "Pacific",
];

/// Old names and spellings of a place that is also listed under its current
/// name, taken from tzdb's `backward` file: the pre-1993 Australian state
/// names, the two-part names renamed in 1995, the "alternate names" section
/// (including a second continent for Istanbul and Nicosia, whose canonical
/// ids are `Europe/Istanbul` and `Asia/Nicosia`) and Porto Acre, the old name
/// of Rio Branco. Zones tzdb merged into another one but that are places in
/// their own right (Amsterdam, Oslo, Montreal, Yellowknife) stay listed,
/// since people look for their own city.
pub(super) const LEGACY_TIMEZONE_NAMES: &[&str] = &[
    "Africa/Asmera",
    "America/Buenos_Aires",
    "America/Catamarca",
    "America/Cordoba",
    "America/Godthab",
    "America/Indianapolis",
    "America/Jujuy",
    "America/Knox_IN",
    "America/Louisville",
    "America/Mendoza",
    "America/Porto_Acre",
    "America/Virgin",
    "Asia/Ashkhabad",
    "Asia/Calcutta",
    "Asia/Chungking",
    "Asia/Dacca",
    "Asia/Istanbul",
    "Asia/Katmandu",
    "Asia/Macao",
    "Asia/Rangoon",
    "Asia/Saigon",
    "Asia/Thimbu",
    "Asia/Ujung_Pandang",
    "Asia/Ulan_Bator",
    "Atlantic/Faeroe",
    "Australia/ACT",
    "Australia/LHI",
    "Australia/NSW",
    "Australia/North",
    "Australia/Queensland",
    "Australia/South",
    "Australia/Tasmania",
    "Australia/Victoria",
    "Australia/West",
    "Australia/Yancowinna",
    "Europe/Kiev",
    "Europe/Nicosia",
    "Pacific/Ponape",
    "Pacific/Samoa",
    "Pacific/Truk",
];

/// One `<option>` of a timezone picker.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct TzOption {
    pub value: String,
    pub label: String,
    pub selected: bool,
}

/// One `<optgroup>` of a timezone picker: the common zones (`area: None`),
/// then one group per IANA area.
#[derive(Debug, Clone)]
pub(super) struct TzGroup {
    pub area: Option<&'static str>,
    pub options: Vec<TzOption>,
}

/// Today's UTC offset of a zone, as shown next to its name: "UTC+3",
/// "UTC+5:30", "UTC-3".
pub(super) fn utc_offset_label(tz: Tz, now: chrono::DateTime<chrono::Utc>) -> String {
    let offset = now.with_timezone(&tz).offset().fix().local_minus_utc();
    let sign = if offset < 0 { '-' } else { '+' };
    let h = offset.abs() / 3600;
    let m = (offset.abs() % 3600) / 60;
    if m != 0 {
        format!("UTC{}{}:{:02}", sign, h, m)
    } else {
        format!("UTC{}{}", sign, h)
    }
}

/// Readable name of a zone inside its area: `Europe/Bucharest` reads
/// "Bucharest", `America/Argentina/Buenos_Aires` reads "Buenos Aires,
/// Argentina", so the city comes first and native type-ahead finds it. The
/// few ids whose spelling does not read as the place name once the
/// underscores are gone are spelled out.
pub(super) fn timezone_location_label(name: &str) -> String {
    let mut parts: Vec<String> = name
        .split('/')
        .skip(1)
        .map(|p| match p {
            "DumontDUrville" => "Dumont d'Urville".to_string(),
            "ComodRivadavia" => "Comodoro Rivadavia".to_string(),
            _ => p.replace('_', " "),
        })
        .collect();
    parts.reverse();
    parts.join(", ")
}

/// Every timezone for a picker: the common zones first, then every IANA
/// zone grouped by area and sorted by name, each labelled with today's UTC
/// offset. A common zone is not repeated in its area, so whatever is
/// selected appears exactly once. `selected` is always offered, even a
/// legacy alias the full list leaves out (`US/Eastern`, `Asia/Calcutta`),
/// so the form posts the saved value back as it is, and the save handlers
/// keep a value posted back unchanged (`resolve_saved_timezone`).
pub(super) fn timezone_groups(selected: &str, now: chrono::DateTime<chrono::Utc>) -> Vec<TzGroup> {
    let is_common = |name: &str| COMMON_TIMEZONES.iter().any(|(iana, _)| *iana == name);
    let mut common: Vec<TzOption> = COMMON_TIMEZONES
        .iter()
        .map(|(iana, city)| TzOption {
            value: iana.to_string(),
            label: match iana.parse::<Tz>() {
                Ok(tz) if *iana != "UTC" => format!("{} ({})", city, utc_offset_label(tz, now)),
                _ => city.to_string(),
            },
            selected: *iana == selected,
        })
        .collect();
    let mut groups: Vec<TzGroup> = TIMEZONE_AREAS
        .iter()
        .map(|area| TzGroup {
            area: Some(area),
            options: Vec::new(),
        })
        .collect();
    let mut found = selected.is_empty() || is_common(selected);

    for tz in chrono_tz::TZ_VARIANTS.iter() {
        let name = tz.name();
        let is_selected = name == selected;
        if is_common(name) || (LEGACY_TIMEZONE_NAMES.contains(&name) && !is_selected) {
            continue;
        }
        let Some((area, _)) = name.split_once('/') else {
            continue;
        };
        if let Some(group) = groups.iter_mut().find(|g| g.area == Some(area)) {
            group.options.push(TzOption {
                value: name.to_string(),
                label: format!(
                    "{} ({})",
                    timezone_location_label(name),
                    utc_offset_label(*tz, now)
                ),
                selected: is_selected,
            });
            found |= is_selected;
        }
    }
    for group in &mut groups {
        group
            .options
            .sort_by_cached_key(|o| (o.label.to_lowercase(), o.value.clone()));
    }

    // A selected id outside the areas (`US/Eastern`, `Etc/GMT+3`) joins the
    // common zones, at its place by UTC offset. One chrono-tz cannot parse
    // is kept as it is, so saving the form does not replace it either.
    if !found {
        let offset_of = |tz: Tz| now.with_timezone(&tz).offset().fix().local_minus_utc();
        let parsed = selected.parse::<Tz>().ok();
        let label = match parsed {
            Some(tz) => format!("{} ({})", selected, utc_offset_label(tz, now)),
            None => selected.to_string(),
        };
        let pos = parsed
            .and_then(|tz| {
                common.iter().position(|o| {
                    o.value
                        .parse::<Tz>()
                        .is_ok_and(|t| offset_of(t) > offset_of(tz))
                })
            })
            .unwrap_or(common.len());
        common.insert(
            pos,
            TzOption {
                value: selected.to_string(),
                label,
                selected: true,
            },
        );
    }

    let mut result = vec![TzGroup {
        area: None,
        options: common,
    }];
    result.extend(groups.into_iter().filter(|g| !g.options.is_empty()));
    result
}

/// Heading of a timezone `<optgroup>` in the page language.
fn timezone_group_label(lang: &str, area: Option<&str>) -> String {
    match area {
        None => crate::i18n::translate(lang, "tz-group-common", None),
        Some("Africa") => crate::i18n::translate(lang, "tz-region-africa", None),
        Some("America") => crate::i18n::translate(lang, "tz-region-america", None),
        Some("Antarctica") => crate::i18n::translate(lang, "tz-region-antarctica", None),
        Some("Arctic") => crate::i18n::translate(lang, "tz-region-arctic", None),
        Some("Asia") => crate::i18n::translate(lang, "tz-region-asia", None),
        Some("Atlantic") => crate::i18n::translate(lang, "tz-region-atlantic", None),
        Some("Australia") => crate::i18n::translate(lang, "tz-region-australia", None),
        Some("Europe") => crate::i18n::translate(lang, "tz-region-europe", None),
        Some("Indian") => crate::i18n::translate(lang, "tz-region-indian", None),
        Some("Pacific") => crate::i18n::translate(lang, "tz-region-pacific", None),
        Some(other) => other.to_string(),
    }
}

/// Template context for a timezone `<select>`: one entry per `<optgroup>`,
/// each with a translated `label` and its `options` (`value`, `label`,
/// `selected`). Used by every timezone picker (#265).
pub(super) fn timezone_options(selected: &str, lang: &str) -> Vec<minijinja::Value> {
    timezone_groups(selected, chrono::Utc::now())
        .into_iter()
        .map(|g| {
            let options: Vec<minijinja::Value> = g
                .options
                .iter()
                .map(|o| context! { value => o.value, label => o.label, selected => o.selected })
                .collect();
            context! { label => timezone_group_label(lang, g.area), options => options }
        })
        .collect()
}

// --- Troubleshoot ---

#[derive(Deserialize)]
pub(super) struct TroubleshootQuery {
    date: Option<String>,
    event_type: Option<String>,
}

pub(super) async fn troubleshoot(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
    Query(params): Query<TroubleshootQuery>,
) -> impl IntoResponse {
    let user = &auth_user.user;

    // Always sync before troubleshooting to ensure fresh data
    crate::commands::sync::sync_if_stale(&state.pool, &state.secret_key, &user.id).await;

    let host_tz = get_user_tz(&state.pool, &user.id).await;
    let now_host = Utc::now().with_timezone(&host_tz).naive_local();

    let target_date = params
        .date
        .as_deref()
        .and_then(|d| NaiveDate::parse_from_str(d, "%Y-%m-%d").ok())
        .unwrap_or(now_host.date());

    // Fetch user's event types for the selector
    let event_types: Vec<(String, String, i32, i32, i32, i32)> = sqlx::query_as(
        "SELECT et.slug, et.title, et.duration_min, et.buffer_before, et.buffer_after, et.min_notice_min
         FROM event_types et
         JOIN accounts a ON a.id = et.account_id
         WHERE a.user_id = ? AND et.team_id IS NULL AND et.enabled = 1
         ORDER BY et.created_at",
    )
    .bind(&user.id)
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();

    if event_types.is_empty() {
        let tmpl = match state.templates.get_template("troubleshoot.html") {
            Ok(t) => t,
            Err(e) => return internal_error_html("template render", &e),
        };
        let (impersonating, impersonating_name, _) = impersonation_ctx(&auth_user);
        return Html(
            tmpl.render(context! {
                user_name => &user.name,
                no_event_types => true,
                sidebar => sidebar_context(&auth_user, "troubleshoot"),
                lang => auth_user.lang,
                impersonating => impersonating,
                impersonating_name => impersonating_name,
            })
            .unwrap_or_default(),
        );
    }

    let selected_slug = params.event_type.as_deref().unwrap_or(&event_types[0].0);
    let selected_et = event_types
        .iter()
        .find(|et| et.0 == selected_slug)
        .unwrap_or(&event_types[0]);
    let (ref et_slug, ref et_title, duration, buf_before, buf_after, min_notice) = *selected_et;

    // Get event type ID
    let et_id: Option<(String,)> = sqlx::query_as(
        "SELECT et.id FROM event_types et JOIN accounts a ON a.id = et.account_id WHERE a.user_id = ? AND et.slug = ?",
    )
    .bind(&user.id)
    .bind(et_slug)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    let et_id = match et_id {
        Some((id,)) => id,
        None => {
            return Html(crate::i18n::translate(
                auth_user.lang,
                "error-event-type-not-found",
                None,
            ))
        }
    };

    let booking_horizon = get_booking_horizon(&state.pool, &et_id).await;

    // Check availability overrides for this date
    let target_date_str = target_date.format("%Y-%m-%d").to_string();
    let day_overrides: Vec<(Option<String>, Option<String>, i32)> = sqlx::query_as(
        "SELECT start_time, end_time, is_blocked FROM availability_overrides WHERE event_type_id = ? AND date = ? ORDER BY start_time",
    )
    .bind(&et_id)
    .bind(&target_date_str)
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();

    let date_is_blocked = day_overrides.iter().any(|(_, _, b)| *b != 0);

    // Availability rules: use overrides if present, else weekly rules
    let rules: Vec<(String, String)> = if date_is_blocked {
        vec![] // blocked day — no availability
    } else if !day_overrides.is_empty() {
        // Custom hours replace weekly rules
        day_overrides
            .iter()
            .filter_map(|(s, e, _)| match (s, e) {
                (Some(start), Some(end)) => Some((start.clone(), end.clone())),
                _ => None,
            })
            .collect()
    } else {
        let weekday = target_date.weekday().num_days_from_sunday() as i32;
        sqlx::query_as(
            "SELECT start_time, end_time FROM availability_rules WHERE event_type_id = ? AND day_of_week = ? ORDER BY start_time",
        )
        .bind(&et_id)
        .bind(weekday)
        .fetch_all(&state.pool)
        .await
        .unwrap_or_default()
    };

    // Busy events for this date — enriched with title + calendar name
    let day_start_compact = target_date.format("%Y%m%d").to_string();
    let day_end_compact = (target_date + Duration::days(1))
        .format("%Y%m%d")
        .to_string();
    let day_start_iso = target_date.format("%Y-%m-%dT00:00:00").to_string();
    let day_end_iso = target_date.format("%Y-%m-%dT23:59:59").to_string();

    // Non-recurring busy events for this date
    let raw_busy_events: Vec<(
        String,
        String,
        Option<String>,
        Option<String>,
        Option<String>,
    )> = sqlx::query_as(
        "SELECT e.start_at, e.end_at, e.summary, c.display_name, e.timezone
         FROM events e
         JOIN calendars c ON c.id = e.calendar_id
         JOIN caldav_sources cs ON cs.id = c.source_id
         JOIN accounts a ON a.id = cs.account_id
         WHERE a.user_id = ?
           AND ((c.is_busy = 1 AND NOT EXISTS (SELECT 1 FROM event_type_calendars WHERE event_type_id = ?))
                OR c.id IN (SELECT calendar_id FROM event_type_calendars WHERE event_type_id = ?))
           AND (e.rrule IS NULL OR e.rrule = '')
           AND (e.status IS NULL OR e.status != 'CANCELLED')
           AND (e.transp IS NULL OR e.transp != 'TRANSPARENT')
           AND ((e.start_at < ? AND e.end_at > ?) OR (e.start_at < ? AND e.end_at > ?))
         ORDER BY e.start_at",
    )
    .bind(&user.id)
    .bind(&et_id)
    .bind(&et_id)
    .bind(&day_end_compact)
    .bind(&day_start_compact)
    .bind(&day_end_iso)
    .bind(&day_start_iso)
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();

    let mut busy_events: Vec<(String, String, Option<String>, Option<String>)> = raw_busy_events
        .iter()
        .filter_map(|(s, e, summary, cal_name, event_tz)| {
            let start = convert_event_to_tz(parse_ical_datetime(s)?, event_tz.as_deref(), host_tz);
            let end = convert_event_to_tz(parse_ical_datetime(e)?, event_tz.as_deref(), host_tz);
            Some((
                start.format("%Y-%m-%dT%H:%M:%S").to_string(),
                end.format("%Y-%m-%dT%H:%M:%S").to_string(),
                summary.clone(),
                cal_name.clone(),
            ))
        })
        .collect();

    // Recurring busy events — expand into this date
    let recurring_events: Vec<(
        String,
        String,
        String,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
    )> = sqlx::query_as(
        "SELECT e.start_at, e.end_at, e.rrule, e.raw_ical, e.summary, c.display_name, e.timezone
         FROM events e
         JOIN calendars c ON c.id = e.calendar_id
         JOIN caldav_sources cs ON cs.id = c.source_id
         JOIN accounts a ON a.id = cs.account_id
         WHERE a.user_id = ?
           AND ((c.is_busy = 1 AND NOT EXISTS (SELECT 1 FROM event_type_calendars WHERE event_type_id = ?))
                OR c.id IN (SELECT calendar_id FROM event_type_calendars WHERE event_type_id = ?))
           AND (e.status IS NULL OR e.status != 'CANCELLED')
           AND (e.transp IS NULL OR e.transp != 'TRANSPARENT')
           AND e.rrule IS NOT NULL AND e.rrule != ''
           AND (e.start_at <= ? OR e.start_at <= ?)",
    )
    .bind(&user.id)
    .bind(&et_id)
    .bind(&et_id)
    .bind(&day_end_iso)
    .bind(&day_end_compact)
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();

    let ts_window_start = target_date
        .and_hms_opt(0, 0, 0)
        .unwrap_or(target_date.and_time(NaiveTime::MIN));
    let ts_window_end = target_date
        .and_hms_opt(23, 59, 59)
        .unwrap_or(target_date.and_time(NaiveTime::MIN));
    for (s, e, rrule_str, raw_ical, summary, cal_name, event_tz) in &recurring_events {
        if let (Some(ev_start), Some(ev_end)) = (parse_ical_datetime(s), parse_ical_datetime(e)) {
            let exdates = raw_ical
                .as_deref()
                .map(|raw| crate::rrule::extract_exdates(raw, event_tz.as_deref()))
                .unwrap_or_default();
            let occurrences = crate::rrule::expand_rrule(
                ev_start,
                ev_end,
                rrule_str,
                &exdates,
                ts_window_start,
                ts_window_end,
            );
            for (os, oe) in occurrences {
                let cs = convert_event_to_tz(os, event_tz.as_deref(), host_tz);
                let ce = convert_event_to_tz(oe, event_tz.as_deref(), host_tz);
                busy_events.push((
                    cs.format("%Y-%m-%dT%H:%M:%S").to_string(),
                    ce.format("%Y-%m-%dT%H:%M:%S").to_string(),
                    summary.clone(),
                    cal_name.clone(),
                ));
            }
        }
    }

    // Bookings for this date: the same bookings that make this user busy on
    // the slot pages (their own, the ones assigned to them on someone else's
    // team event type, unassigned collective ones, and held pending requests).
    let bookings: Vec<(String, String, String, String, String)> = sqlx::query_as(&format!(
        "SELECT CASE b.time_version WHEN 1 THEN b.start_at ELSE rtrim(b.start_at, 'Z') END AS start_at, CASE b.time_version WHEN 1 THEN b.end_at ELSE rtrim(b.end_at, 'Z') END AS end_at, b.guest_name, et.title, b.status
         FROM bookings b
         JOIN event_types et ON et.id = b.event_type_id
         JOIN accounts a ON a.id = et.account_id
         WHERE {}
           AND b.start_at < strftime('%Y-%m-%dT%H:%M:%S', ?, '+2 days') AND b.end_at > strftime('%Y-%m-%dT%H:%M:%S', ?, '-2 days')
         ORDER BY b.start_at",
        crate::booking_time::user_busy_bookings(true)
    ))
    .bind(&user.id)
    .bind(&user.id)
    .bind(&user.id)
    .bind(&day_end_iso)
    .bind(&day_start_iso)
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();

    // Build timeline: scan 15-min ticks from display_start to display_end
    let display_start_hour: u32 = rules
        .iter()
        .filter_map(|(s, _)| NaiveTime::parse_from_str(s, "%H:%M").ok())
        .map(|t| t.hour())
        .min()
        .unwrap_or(8)
        .saturating_sub(1);
    let display_end_hour: u32 = rules
        .iter()
        .filter_map(|(_, e)| NaiveTime::parse_from_str(e, "%H:%M").ok())
        .map(|t| {
            if t.minute() > 0 {
                t.hour() + 1
            } else {
                t.hour()
            }
        })
        .max()
        .unwrap_or(18)
        .min(23)
        + 1;

    let display_start = NaiveTime::from_hms_opt(display_start_hour, 0, 0).unwrap_or(NaiveTime::MIN);
    let display_end = NaiveTime::from_hms_opt(display_end_hour, 0, 0).unwrap_or(NaiveTime::MIN);
    let total_minutes = (display_end - display_start).num_minutes() as f64;

    let min_start = now_host + Duration::minutes(min_notice as i64);

    // This view reimplements availability for a single date and shares no code
    // with compute_slots, so the rolling booking horizon is checked separately.
    // The whole target date is either inside the window or outside it.
    let beyond_horizon =
        horizon_last_date(now_host, booking_horizon).is_some_and(|last| target_date > last);

    // Parse availability windows
    let avail_windows: Vec<(NaiveTime, NaiveTime)> = rules
        .iter()
        .filter_map(|(s, e)| {
            let st = NaiveTime::parse_from_str(s, "%H:%M").ok()?;
            let en = NaiveTime::parse_from_str(e, "%H:%M").ok()?;
            Some((st, en))
        })
        .collect();

    // Parse busy events into (start_dt, end_dt, label, detail)
    let busy_parsed: Vec<(NaiveDateTime, NaiveDateTime, String, String)> = busy_events
        .iter()
        .filter_map(|(s, e, summary, cal)| {
            let start = parse_ical_datetime(s)?;
            let end = parse_ical_datetime(e)?;
            let label = summary.clone().unwrap_or_else(|| "Busy".to_string());
            let detail = cal.clone().unwrap_or_default();
            Some((start, end, label, detail))
        })
        .collect();

    // Parse bookings into (start_dt, end_dt, label, detail)
    // A pending request is shown as such, so the host can tell a held slot
    // from a confirmed meeting.
    let bookings_parsed: Vec<(NaiveDateTime, NaiveDateTime, String, String)> = bookings
        .iter()
        .filter_map(|(s, e, guest, et_title, status)| {
            let (start, end) = crate::booking_time::busy_range(s, e, host_tz)?;
            let detail = if status == "pending" {
                tr1(
                    auth_user.lang,
                    "troubleshoot-detail-pending-request",
                    "title",
                    et_title,
                )
            } else {
                et_title.clone()
            };
            Some((start, end, guest.clone(), detail))
        })
        .collect();

    // Intervals where required shared resources block this event type
    // (mode-aware: union in 'all', intersection in 'round_robin').
    let resource_names: Vec<String> =
        crate::resources::resources_for_event_type(&state.pool, &et_id)
            .await
            .into_iter()
            .map(|r| r.name)
            .collect();
    let resource_blocked: Vec<(NaiveDateTime, NaiveDateTime)> =
        crate::resources::blocking_intervals_for_event_type(
            &state.pool,
            &et_id,
            ts_window_start,
            ts_window_end,
            host_tz,
            None,
        )
        .await;
    let lang = auth_user.lang;
    let resource_label = tr1(
        lang,
        "troubleshoot-label-resource-busy",
        "names",
        &resource_names.join(", "),
    );

    // Scan in 15-min increments and classify each tick
    struct Tick {
        time: NaiveTime,
        status: String, // "available", "outside", "busy_event", "booking", "buffer", "min_notice", "beyond_horizon"
        label: String,
        detail: String,
    }

    let mut ticks: Vec<Tick> = Vec::new();
    let mut cursor = display_start;
    let tick_size = Duration::minutes(15);

    while cursor < display_end {
        let tick_dt = target_date.and_time(cursor);
        let tick_end = tick_dt + tick_size;

        // 1. Check if within availability window
        let in_avail = avail_windows
            .iter()
            .any(|(ws, we)| cursor >= *ws && cursor < *we);

        if !in_avail {
            ticks.push(Tick {
                time: cursor,
                status: "outside".to_string(),
                label: crate::i18n::translate(lang, "troubleshoot-label-outside", None),
                detail: String::new(),
            });
            cursor = (tick_dt + tick_size).time();
            continue;
        }

        // 2. Check minimum notice
        if tick_dt < min_start {
            ticks.push(Tick {
                time: cursor,
                status: "min_notice".to_string(),
                label: tr1(
                    lang,
                    "troubleshoot-label-min-notice",
                    "minutes",
                    &min_notice.to_string(),
                ),
                detail: String::new(),
            });
            cursor = (tick_dt + tick_size).time();
            continue;
        }

        // 2b. Check the rolling booking horizon
        if beyond_horizon {
            ticks.push(Tick {
                time: cursor,
                status: "beyond_horizon".to_string(),
                label: tr1(
                    lang,
                    "troubleshoot-label-beyond-horizon",
                    "days",
                    &booking_horizon.unwrap_or(0).to_string(),
                ),
                detail: String::new(),
            });
            cursor = (tick_dt + tick_size).time();
            continue;
        }

        // 3. Check calendar events (with buffers)
        let event_conflict = busy_parsed.iter().find(|(s, e, _, _)| {
            let buf_s = *s - Duration::minutes(buf_before as i64);
            let buf_e = *e + Duration::minutes(buf_after as i64);
            tick_dt < buf_e && tick_end > buf_s
        });

        if let Some((ev_s, ev_e, ev_label, ev_detail)) = event_conflict {
            // Is it the event itself or just the buffer zone?
            let in_event = tick_dt < *ev_e && tick_end > *ev_s;
            if in_event {
                ticks.push(Tick {
                    time: cursor,
                    status: "busy_event".to_string(),
                    label: ev_label.clone(),
                    detail: ev_detail.clone(),
                });
            } else {
                ticks.push(Tick {
                    time: cursor,
                    status: "buffer".to_string(),
                    label: tr1(
                        lang,
                        "troubleshoot-label-buffer",
                        "minutes",
                        &if tick_dt < *ev_s {
                            buf_before
                        } else {
                            buf_after
                        }
                        .to_string(),
                    ),
                    detail: tr1(lang, "troubleshoot-detail-around", "label", ev_label),
                });
            }
            cursor = (tick_dt + tick_size).time();
            continue;
        }

        // 4. Check bookings (with buffers)
        let booking_conflict = bookings_parsed.iter().find(|(s, e, _, _)| {
            let buf_s = *s - Duration::minutes(buf_before as i64);
            let buf_e = *e + Duration::minutes(buf_after as i64);
            tick_dt < buf_e && tick_end > buf_s
        });

        if let Some((bk_s, bk_e, bk_guest, bk_et)) = booking_conflict {
            let in_booking = tick_dt < *bk_e && tick_end > *bk_s;
            if in_booking {
                ticks.push(Tick {
                    time: cursor,
                    status: "booking".to_string(),
                    label: bk_guest.clone(),
                    detail: bk_et.clone(),
                });
            } else {
                ticks.push(Tick {
                    time: cursor,
                    status: "buffer".to_string(),
                    label: tr1(
                        lang,
                        "troubleshoot-label-buffer",
                        "minutes",
                        &if tick_dt < *bk_s {
                            buf_before
                        } else {
                            buf_after
                        }
                        .to_string(),
                    ),
                    detail: tr1(
                        lang,
                        "troubleshoot-detail-around-booking",
                        "guest",
                        bk_guest,
                    ),
                });
            }
            cursor = (tick_dt + tick_size).time();
            continue;
        }

        // 5. Check shared resources (no buffers: a resource needs none)
        if resource_blocked
            .iter()
            .any(|(s, e)| tick_dt < *e && tick_end > *s)
        {
            ticks.push(Tick {
                time: cursor,
                status: "resource_busy".to_string(),
                label: resource_label.clone(),
                detail: String::new(),
            });
            cursor = (tick_dt + tick_size).time();
            continue;
        }

        // 6. Available!
        ticks.push(Tick {
            time: cursor,
            status: "available".to_string(),
            label: crate::i18n::translate(lang, "troubleshoot-label-available", None),
            detail: String::new(),
        });
        cursor = (tick_dt + tick_size).time();
    }

    // Merge consecutive ticks with same status+label into blocks
    struct Block {
        start: NaiveTime,
        end: NaiveTime,
        status: String,
        label: String,
        detail: String,
        left_pct: f64,
        width_pct: f64,
    }

    let mut blocks: Vec<Block> = Vec::new();
    for tick in &ticks {
        let tick_end_time = (target_date.and_time(tick.time) + tick_size).time();
        if let Some(last) = blocks.last_mut() {
            if last.status == tick.status && last.label == tick.label {
                last.end = tick_end_time;
                let start_min = (last.start - display_start).num_minutes() as f64;
                let dur_min = (last.end - last.start).num_minutes() as f64;
                last.left_pct = start_min / total_minutes * 100.0;
                last.width_pct = dur_min / total_minutes * 100.0;
                continue;
            }
        }
        let start_min = (tick.time - display_start).num_minutes() as f64;
        let dur_min = tick_size.num_minutes() as f64;
        blocks.push(Block {
            start: tick.time,
            end: tick_end_time,
            status: tick.status.clone(),
            label: tick.label.clone(),
            detail: tick.detail.clone(),
            left_pct: start_min / total_minutes * 100.0,
            width_pct: dur_min / total_minutes * 100.0,
        });
    }

    // Build template data
    let blocks_ctx: Vec<minijinja::Value> = blocks
        .iter()
        .map(|b| {
            context! {
                start => b.start.format("%H:%M").to_string(),
                end => b.end.format("%H:%M").to_string(),
                status => &b.status,
                label => &b.label,
                detail => &b.detail,
                left_pct => format!("{:.2}", b.left_pct),
                width_pct => format!("{:.2}", b.width_pct),
            }
        })
        .collect();

    // Hour markers for the timeline
    let mut hour_markers: Vec<minijinja::Value> = Vec::new();
    let mut h = display_start_hour;
    while h <= display_end_hour {
        let min_offset = (h - display_start_hour) as f64 * 60.0;
        let left_pct = min_offset / total_minutes * 100.0;
        hour_markers.push(context! {
            label => format!("{:02}:00", h),
            left_pct => format!("{:.2}", left_pct),
        });
        h += 1;
    }

    // Breakdown: only non-available blocks
    let breakdown_ctx: Vec<minijinja::Value> = blocks
        .iter()
        .filter(|b| b.status != "available" && b.status != "outside")
        .map(|b| {
            let reason = match b.status.as_str() {
                "busy_event" => tr1(
                    lang,
                    "troubleshoot-reason-calendar-event",
                    "label",
                    &b.label,
                ),
                "booking" => tr1(lang, "troubleshoot-reason-booking", "label", &b.label),
                "buffer" => b.label.clone(),
                "min_notice" => b.label.clone(),
                "beyond_horizon" => b.label.clone(),
                "resource_busy" => b.label.clone(),
                _ => b.status.clone(),
            };
            context! {
                start => b.start.format("%H:%M").to_string(),
                end => b.end.format("%H:%M").to_string(),
                status => &b.status,
                reason => reason,
                detail => &b.detail,
            }
        })
        .collect();

    let et_options: Vec<minijinja::Value> = event_types
        .iter()
        .map(|et| {
            context! {
                slug => &et.0,
                title => &et.1,
                selected => et.0 == *et_slug,
            }
        })
        .collect();

    let prev_date = (target_date - Duration::days(1))
        .format("%Y-%m-%d")
        .to_string();
    let next_date = (target_date + Duration::days(1))
        .format("%Y-%m-%d")
        .to_string();
    // Troubleshoot is a dashboard page; keep English until dashboard is translated.
    let date_label = crate::i18n::format_long_date(target_date, "en");

    let tmpl = match state.templates.get_template("troubleshoot.html") {
        Ok(t) => t,
        Err(e) => return internal_error_html("template render", &e),
    };

    let (impersonating, impersonating_name, _impersonating_admin) = impersonation_ctx(&auth_user);
    Html(
        tmpl.render(context! {
            user_name => &user.name,
            no_event_types => false,
            event_types => et_options,
            selected_slug => et_slug,
            selected_date => target_date.format("%Y-%m-%d").to_string(),
            date_label => date_label,
            prev_date => prev_date,
            next_date => next_date,
            has_rules => !rules.is_empty(),
            date_is_blocked => date_is_blocked,
            has_custom_hours => !day_overrides.is_empty() && !date_is_blocked,
            blocks => blocks_ctx,
            hour_markers => hour_markers,
            breakdown => breakdown_ctx,
            et_title => et_title,
            duration => duration,
            buf_before => buf_before,
            buf_after => buf_after,
            min_notice => min_notice,
            sidebar => sidebar_context(&auth_user, "troubleshoot"),
            lang => auth_user.lang,
            impersonating => impersonating,
            impersonating_name => impersonating_name,
        })
        .unwrap_or_default(),
    )
}
