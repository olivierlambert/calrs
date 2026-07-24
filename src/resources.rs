//! Shared bookable resources (demo lab, meeting room, …).
//!
//! A resource is an instance-level entity backed by a read-only ICS publish
//! feed (BlueMind "public/private calendar address", Nextcloud "public link",
//! …). Its events are cached in `resource_events` and merged into slot
//! availability for every event type the resource is attached to.
//!
//! Two scheduling modes per event type (`event_types.resource_scheduling_mode`):
//! - `all` (default): every attached resource must be free.
//! - `round_robin`: at least one resource must be free; one is picked and
//!   recorded on the booking (`bookings.assigned_resource_id`).
//!
//! calrs bookings themselves also block resources (independently of the feed,
//! which may be stale): a confirmed booking blocks its assigned resource, and
//! in `all` mode every resource of its event type. This is what prevents two
//! event types sharing a resource from double-booking it even without
//! write-back.

use chrono::NaiveDateTime;
use chrono_tz::Tz;
use sqlx::SqlitePool;
use uuid::Uuid;

use crate::utils::{
    convert_event_to_tz, extract_vevent_field, extract_vevent_tzid, parse_ical_datetime,
    split_vevents,
};

/// Re-sync a resource feed when older than this many minutes.
pub const SYNC_STALE_MINUTES: i64 = 5;

/// Serializes the resource availability check + booking insert
/// process-wide. SQLite gives us no cross-connection serialization here:
/// the busy reads run on other pool connections and cannot see an
/// uncommitted insert, so without this lock two concurrent bookings (even
/// on different event types sharing a resource) could both see the
/// resource as free. Bookings without resources never take this lock.
static BOOKING_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// Acquire the process-wide resource booking lock. Hold it from before
/// [`check_and_pick`] until after the booking row is committed, then drop
/// it before any network write-back.
pub async fn booking_lock() -> tokio::sync::MutexGuard<'static, ()> {
    BOOKING_LOCK.lock().await
}

#[derive(Debug, Clone)]
pub struct ResourceRef {
    pub id: String,
    pub name: String,
}

/// Outcome of the availability check at booking time.
#[derive(Debug, Clone, PartialEq)]
pub enum ResourceCheck {
    /// Event type has no resources attached.
    NoResources,
    /// Resources are available. `assigned` is the picked resource in
    /// round-robin mode, `None` in `all` mode.
    Free { assigned: Option<String> },
    /// Required resource(s) busy, the slot must be refused.
    Busy,
}

/// Fetch the raw ICS publish feed. An empty body with a calendar
/// content-type is a valid, empty calendar (BlueMind behaviour).
pub async fn fetch_feed(feed_url: &str) -> anyhow::Result<String> {
    // Re-validate on every fetch, not just at configuration time: the
    // stored URL's host may have started resolving to an internal address
    // since (and background re-syncs are triggered from public pages).
    crate::caldav::validate_caldav_url(feed_url)?;
    // No redirects: following one would let a validated public URL bounce
    // the request to an internal host (SSRF).
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .redirect(reqwest::redirect::Policy::none())
        .build()?;
    let mut resp = client.get(feed_url).send().await?;
    if !resp.status().is_success() {
        anyhow::bail!("feed returned HTTP {}", resp.status());
    }
    let is_calendar = resp
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .map(|ct| ct.contains("text/calendar"))
        .unwrap_or(false);
    // Bound the body: a hostile feed must not exhaust memory.
    const MAX_FEED_BYTES: usize = 10 * 1024 * 1024;
    let mut raw: Vec<u8> = Vec::new();
    while let Some(chunk) = resp.chunk().await? {
        raw.extend_from_slice(&chunk);
        if raw.len() > MAX_FEED_BYTES {
            anyhow::bail!("feed exceeds {} bytes", MAX_FEED_BYTES);
        }
    }
    let body = String::from_utf8_lossy(&raw).into_owned();
    if !is_calendar && !body.contains("BEGIN:VCALENDAR") {
        anyhow::bail!("URL did not return an ICS calendar");
    }
    Ok(body)
}

/// Derive the CalDAV collection URL from a BlueMind publish URL.
///
/// `https://host/api/calendars/publish/calendar:UID/x-calendar-…` →
/// `https://host/dav/calendars/__uids__/UID/calendar:UID/`
/// Returns `None` for non-BlueMind feeds (the admin then enters the CalDAV
/// URL manually).
pub fn derive_caldav_url(feed_url: &str) -> Option<String> {
    let u = reqwest::Url::parse(feed_url).ok()?;
    let container = u
        .path()
        .split('/')
        .find(|s| s.starts_with("calendar:"))?
        .to_string();
    let uid = container.strip_prefix("calendar:")?;
    let host = u.host_str()?;
    let port = u.port().map(|p| format!(":{}", p)).unwrap_or_default();
    Some(format!(
        "{}://{}{}/dav/calendars/__uids__/{}/{}/",
        u.scheme(),
        host,
        port,
        uid,
        container
    ))
}

/// Scheme + host + effective port of a URL, for matching stored member
/// sources against a resource's CalDAV server.
pub fn url_origin(url: &str) -> Option<(String, String, Option<u16>)> {
    let u = reqwest::Url::parse(url).ok()?;
    Some((
        u.scheme().to_string(),
        u.host_str()?.to_string(),
        u.port_or_known_default(),
    ))
}

/// Extract `X-WR-CALNAME` from a feed body (used to auto-fill the name).
pub fn feed_calendar_name(body: &str) -> Option<String> {
    body.lines()
        .find(|l| l.starts_with("X-WR-CALNAME"))
        .and_then(|l| l.split_once(':').map(|(_, v)| v.trim().to_string()))
        .filter(|s| !s.is_empty())
}

/// Sync one resource's feed into `resource_events`. Returns the number of
/// cached VEVENTs. Orphans (events gone from the feed) are removed: the
/// feed is the full authoritative state, there is no delta protocol.
pub async fn sync_resource(
    pool: &SqlitePool,
    resource_id: &str,
    feed_url: &str,
) -> anyhow::Result<u32> {
    let body = fetch_feed(feed_url).await?;

    let mut vevents: Vec<String> = if body.contains("BEGIN:VEVENT") {
        split_vevents(&body)
    } else {
        Vec::new()
    };
    // A hostile or broken feed must not grow the cache without bound.
    const MAX_FEED_EVENTS: usize = 10_000;
    if vevents.len() > MAX_FEED_EVENTS {
        tracing::warn!(resource_id = %resource_id, count = vevents.len(), "resource feed truncated to {} events", MAX_FEED_EVENTS);
        vevents.truncate(MAX_FEED_EVENTS);
    }

    let mut seen: Vec<(String, String)> = Vec::new();
    let mut count = 0u32;
    for vevent in &vevents {
        let uid = extract_vevent_field(vevent, "UID").unwrap_or_else(|| Uuid::new_v4().to_string());
        let summary = extract_vevent_field(vevent, "SUMMARY");
        let start_at = extract_vevent_field(vevent, "DTSTART").unwrap_or_default();
        let end_at = extract_vevent_field(vevent, "DTEND").unwrap_or_default();
        let status = extract_vevent_field(vevent, "STATUS");
        let rrule = extract_vevent_field(vevent, "RRULE");
        let recurrence_id = extract_vevent_field(vevent, "RECURRENCE-ID");
        let transp = extract_vevent_field(vevent, "TRANSP");
        let timezone = extract_vevent_tzid(vevent, "DTSTART");
        // All-day events use VALUE=DATE ("20260730"), timed use full stamps.
        let all_day = start_at.len() == 8 && start_at.chars().all(|c| c.is_ascii_digit());

        if start_at.is_empty() {
            continue;
        }
        seen.push((uid.clone(), recurrence_id.clone().unwrap_or_default()));

        let id = Uuid::new_v4().to_string();
        let _ = sqlx::query(
            "INSERT INTO resource_events (id, resource_id, uid, recurrence_id, start_at, end_at, all_day, timezone, rrule, raw_ical, status, transp, summary)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(resource_id, uid, COALESCE(recurrence_id, '')) DO UPDATE SET
               start_at = excluded.start_at,
               end_at = excluded.end_at,
               all_day = excluded.all_day,
               timezone = excluded.timezone,
               rrule = excluded.rrule,
               raw_ical = excluded.raw_ical,
               status = excluded.status,
               transp = excluded.transp,
               summary = excluded.summary",
        )
        .bind(&id)
        .bind(resource_id)
        .bind(&uid)
        .bind(&recurrence_id)
        .bind(&start_at)
        .bind(&end_at)
        .bind(all_day as i32)
        .bind(&timezone)
        .bind(&rrule)
        .bind(vevent)
        .bind(&status)
        .bind(&transp)
        .bind(&summary)
        .execute(pool)
        .await
        .map(|_| count += 1)
        .ok();
    }

    // Remove events that vanished from the feed.
    let local: Vec<(String, String, Option<String>)> =
        sqlx::query_as("SELECT id, uid, recurrence_id FROM resource_events WHERE resource_id = ?")
            .bind(resource_id)
            .fetch_all(pool)
            .await
            .unwrap_or_default();
    for (row_id, uid, rec_id) in &local {
        let key = (uid.clone(), rec_id.clone().unwrap_or_default());
        if !seen.contains(&key) {
            let _ = sqlx::query("DELETE FROM resource_events WHERE id = ?")
                .bind(row_id)
                .execute(pool)
                .await;
        }
    }

    let _ = sqlx::query("UPDATE resources SET last_synced_at = datetime('now') WHERE id = ?")
        .bind(resource_id)
        .execute(pool)
        .await;

    Ok(count)
}

/// Sync every listed resource whose cache is older than
/// [`SYNC_STALE_MINUTES`]. Failures are logged and skipped: a dead feed must
/// not break the booking page (the cache keeps serving the last state).
pub async fn sync_if_stale(pool: &SqlitePool, resource_ids: &[String]) {
    for rid in resource_ids {
        let row: Option<(String, Option<String>)> =
            sqlx::query_as("SELECT feed_url, last_synced_at FROM resources WHERE id = ?")
                .bind(rid)
                .fetch_optional(pool)
                .await
                .unwrap_or(None);
        let Some((feed_url, last)) = row else {
            continue;
        };
        let stale = match last
            .and_then(|l| chrono::NaiveDateTime::parse_from_str(&l, "%Y-%m-%d %H:%M:%S").ok())
        {
            Some(t) => {
                chrono::Utc::now().naive_utc() - t > chrono::Duration::minutes(SYNC_STALE_MINUTES)
            }
            None => true,
        };
        if stale {
            if let Err(e) = sync_resource(pool, rid, &feed_url).await {
                tracing::warn!(resource_id = %rid, error = %e, "resource feed sync failed");
                // Stamp the attempt anyway: a dead feed must back off for
                // SYNC_STALE_MINUTES instead of re-fetching (with a 30s
                // timeout) on every slot view and booking.
                let _ = sqlx::query(
                    "UPDATE resources SET last_synced_at = datetime('now') WHERE id = ?",
                )
                .bind(rid)
                .execute(pool)
                .await;
            }
        }
    }
}

/// Resources attached to an event type (empty = feature unused).
pub async fn resources_for_event_type(pool: &SqlitePool, event_type_id: &str) -> Vec<ResourceRef> {
    let rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT r.id, r.name FROM resources r
         JOIN event_type_resources etr ON etr.resource_id = r.id
         WHERE etr.event_type_id = ? ORDER BY r.name",
    )
    .bind(event_type_id)
    .fetch_all(pool)
    .await
    .unwrap_or_default();
    rows.into_iter()
        .map(|(id, name)| ResourceRef { id, name })
        .collect()
}

/// Busy intervals for one resource in `[window_start, window_end]`,
/// converted to `host_tz`. Sources: cached feed events (single + recurring)
/// and calrs' own confirmed bookings that block this resource.
pub async fn busy_for_resource(
    pool: &SqlitePool,
    resource_id: &str,
    window_start: NaiveDateTime,
    window_end: NaiveDateTime,
    host_tz: Tz,
    exclude_booking_id: Option<&str>,
) -> Vec<(NaiveDateTime, NaiveDateTime)> {
    let end_compact = window_end.format("%Y%m%dT%H%M%S").to_string();
    let start_compact = window_start.format("%Y%m%dT%H%M%S").to_string();
    let end_iso = window_end.format("%Y-%m-%dT%H:%M:%S").to_string();
    let start_iso = window_start.format("%Y-%m-%dT%H:%M:%S").to_string();

    // The two uid exclusions below handle our own write-back reservations
    // once they come back through the feed: the rescheduled booking's own
    // reservation must not block its new slot, and a reservation whose
    // booking was cancelled (release may have failed) must not keep
    // blocking until someone cleans the remote calendar by hand.
    let exclude_id = exclude_booking_id.unwrap_or("");
    let events: Vec<(String, String, Option<String>)> = sqlx::query_as(
        "SELECT start_at, end_at, timezone FROM resource_events
         WHERE resource_id = ?
           AND (rrule IS NULL OR rrule = '')
           AND (status IS NULL OR status != 'CANCELLED')
           AND (transp IS NULL OR transp != 'TRANSPARENT')
           AND (? = '' OR uid NOT IN (SELECT uid FROM bookings WHERE id = ?))
           AND uid NOT IN (SELECT uid FROM bookings WHERE status IN ('cancelled', 'declined'))
           AND start_at <= ? AND end_at >= ?",
    )
    .bind(resource_id)
    .bind(exclude_id)
    .bind(exclude_id)
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

    let recurring: Vec<(String, String, String, Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT start_at, end_at, rrule, raw_ical, timezone FROM resource_events
         WHERE resource_id = ?
           AND (status IS NULL OR status != 'CANCELLED')
           AND (transp IS NULL OR transp != 'TRANSPARENT')
           AND (? = '' OR uid NOT IN (SELECT uid FROM bookings WHERE id = ?))
           AND uid NOT IN (SELECT uid FROM bookings WHERE status IN ('cancelled', 'declined'))
           AND rrule IS NOT NULL AND rrule != '' AND start_at <= ?",
    )
    .bind(resource_id)
    .bind(exclude_id)
    .bind(exclude_id)
    .bind(&end_compact)
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    busy.extend(crate::web::expand_recurring_into_busy(
        &recurring,
        window_start,
        window_end,
        host_tz,
    ));

    // calrs' own bookings: an assigned booking always blocks its resource;
    // in `all` mode every confirmed booking of an event type using this
    // resource blocks it.
    let bookings: Vec<(String, String)> = sqlx::query_as(
        "SELECT b.start_at, b.end_at FROM bookings b
         JOIN event_types et ON et.id = b.event_type_id
         WHERE b.status = 'confirmed'
           AND (b.assigned_resource_id = ?
                OR (et.resource_scheduling_mode = 'all' AND EXISTS (
                      SELECT 1 FROM event_type_resources etr
                      WHERE etr.event_type_id = et.id AND etr.resource_id = ?)))
           AND b.start_at <= ? AND b.end_at >= ?
           AND (? = '' OR b.id != ?)",
    )
    .bind(resource_id)
    .bind(resource_id)
    .bind(&end_iso)
    .bind(&start_iso)
    .bind(exclude_id)
    .bind(exclude_id)
    .fetch_all(pool)
    .await
    .unwrap_or_default();
    for (s, e) in &bookings {
        if let (Some(start), Some(end)) = (parse_ical_datetime(s), parse_ical_datetime(e)) {
            busy.push((start, end));
        }
    }

    busy
}

/// Sort intervals and merge overlapping/adjacent ones.
fn normalize(
    mut intervals: Vec<(NaiveDateTime, NaiveDateTime)>,
) -> Vec<(NaiveDateTime, NaiveDateTime)> {
    intervals.retain(|(s, e)| s < e);
    intervals.sort();
    let mut out: Vec<(NaiveDateTime, NaiveDateTime)> = Vec::new();
    for (s, e) in intervals {
        match out.last_mut() {
            Some(last) if s <= last.1 => {
                if e > last.1 {
                    last.1 = e;
                }
            }
            _ => out.push((s, e)),
        }
    }
    out
}

/// Intersect two normalized interval lists (two-pointer sweep).
fn intersect(
    a: &[(NaiveDateTime, NaiveDateTime)],
    b: &[(NaiveDateTime, NaiveDateTime)],
) -> Vec<(NaiveDateTime, NaiveDateTime)> {
    let (mut i, mut j) = (0, 0);
    let mut out = Vec::new();
    while i < a.len() && j < b.len() {
        let start = a[i].0.max(b[j].0);
        let end = a[i].1.min(b[j].1);
        if start < end {
            out.push((start, end));
        }
        if a[i].1 <= b[j].1 {
            i += 1;
        } else {
            j += 1;
        }
    }
    out
}

/// Merge per-resource busy lists according to the scheduling mode into the
/// intervals during which the slot must be considered blocked.
///
/// - `all`: any resource busy blocks → union of all busy intervals.
/// - `round_robin`: blocked only when EVERY resource is busy → intersection.
pub fn merge_mode_busy(
    per_resource: &[Vec<(NaiveDateTime, NaiveDateTime)>],
    mode: &str,
) -> Vec<(NaiveDateTime, NaiveDateTime)> {
    if per_resource.is_empty() {
        return Vec::new();
    }
    if mode == "round_robin" {
        let mut acc = normalize(per_resource[0].clone());
        for list in &per_resource[1..] {
            if acc.is_empty() {
                return Vec::new();
            }
            acc = intersect(&acc, &normalize(list.clone()));
        }
        acc
    } else {
        normalize(per_resource.concat())
    }
}

/// Busy intervals that block slots of this event type, given its attached
/// resources and mode. Empty when no resources are attached.
pub async fn blocking_intervals_for_event_type(
    pool: &SqlitePool,
    event_type_id: &str,
    window_start: NaiveDateTime,
    window_end: NaiveDateTime,
    host_tz: Tz,
    exclude_booking_id: Option<&str>,
) -> Vec<(NaiveDateTime, NaiveDateTime)> {
    let refs = resources_for_event_type(pool, event_type_id).await;
    if refs.is_empty() {
        return Vec::new();
    }
    let ids: Vec<String> = refs.iter().map(|r| r.id.clone()).collect();
    sync_if_stale(pool, &ids).await;
    let mode: String =
        sqlx::query_scalar("SELECT resource_scheduling_mode FROM event_types WHERE id = ?")
            .bind(event_type_id)
            .fetch_optional(pool)
            .await
            .unwrap_or(None)
            .unwrap_or_else(|| "all".to_string());

    let mut per_resource = Vec::with_capacity(refs.len());
    for r in &refs {
        per_resource.push(
            busy_for_resource(
                pool,
                &r.id,
                window_start,
                window_end,
                host_tz,
                exclude_booking_id,
            )
            .await,
        );
    }
    merge_mode_busy(&per_resource, &mode)
}

/// Booking-time check: verify resource availability for `[start, end)` and,
/// in round-robin mode, pick the free resource with the fewest upcoming
/// assignments (stable tie-break by name via the resource listing order).
pub async fn check_and_pick(
    pool: &SqlitePool,
    event_type_id: &str,
    start: NaiveDateTime,
    end: NaiveDateTime,
    host_tz: Tz,
    exclude_booking_id: Option<&str>,
) -> ResourceCheck {
    let refs = resources_for_event_type(pool, event_type_id).await;
    if refs.is_empty() {
        return ResourceCheck::NoResources;
    }
    let mode: String =
        sqlx::query_scalar("SELECT resource_scheduling_mode FROM event_types WHERE id = ?")
            .bind(event_type_id)
            .fetch_optional(pool)
            .await
            .unwrap_or(None)
            .unwrap_or_else(|| "all".to_string());

    let mut free: Vec<&ResourceRef> = Vec::new();
    for r in &refs {
        let busy = busy_for_resource(pool, &r.id, start, end, host_tz, exclude_booking_id).await;
        let overlaps = busy.iter().any(|(bs, be)| *bs < end && *be > start);
        if !overlaps {
            free.push(r);
        } else if mode != "round_robin" {
            return ResourceCheck::Busy;
        }
    }

    if mode == "round_robin" {
        if free.is_empty() {
            return ResourceCheck::Busy;
        }
        // Least-loaded pick: fewest upcoming confirmed assignments.
        let mut best: Option<(i64, &ResourceRef)> = None;
        let now_iso = chrono::Utc::now()
            .naive_utc()
            .format("%Y-%m-%dT%H:%M:%S")
            .to_string();
        for r in &free {
            let load: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM bookings
                 WHERE assigned_resource_id = ? AND status = 'confirmed' AND start_at >= ?",
            )
            .bind(&r.id)
            .bind(&now_iso)
            .fetch_one(pool)
            .await
            .unwrap_or(0);
            if best.is_none() || load < best.as_ref().unwrap().0 {
                best = Some((load, r));
            }
        }
        ResourceCheck::Free {
            assigned: best.map(|(_, r)| r.id.clone()),
        }
    } else {
        ResourceCheck::Free { assigned: None }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dt(s: &str) -> NaiveDateTime {
        NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M").unwrap()
    }

    #[test]
    fn normalize_merges_overlaps() {
        let out = normalize(vec![
            (dt("2026-07-30T10:00"), dt("2026-07-30T11:00")),
            (dt("2026-07-30T10:30"), dt("2026-07-30T12:00")),
            (dt("2026-07-30T14:00"), dt("2026-07-30T15:00")),
        ]);
        assert_eq!(
            out,
            vec![
                (dt("2026-07-30T10:00"), dt("2026-07-30T12:00")),
                (dt("2026-07-30T14:00"), dt("2026-07-30T15:00")),
            ]
        );
    }

    #[test]
    fn normalize_drops_empty_intervals() {
        let out = normalize(vec![(dt("2026-07-30T10:00"), dt("2026-07-30T10:00"))]);
        assert!(out.is_empty());
    }

    #[test]
    fn intersect_two_lists() {
        let a = vec![(dt("2026-07-30T10:00"), dt("2026-07-30T12:00"))];
        let b = vec![
            (dt("2026-07-30T09:00"), dt("2026-07-30T10:30")),
            (dt("2026-07-30T11:00"), dt("2026-07-30T13:00")),
        ];
        let out = intersect(&a, &b);
        assert_eq!(
            out,
            vec![
                (dt("2026-07-30T10:00"), dt("2026-07-30T10:30")),
                (dt("2026-07-30T11:00"), dt("2026-07-30T12:00")),
            ]
        );
    }

    #[test]
    fn merge_all_mode_is_union() {
        let per = vec![
            vec![(dt("2026-07-30T10:00"), dt("2026-07-30T11:00"))],
            vec![(dt("2026-07-30T14:00"), dt("2026-07-30T15:00"))],
        ];
        let out = merge_mode_busy(&per, "all");
        assert_eq!(out.len(), 2);
    }

    #[test]
    fn merge_round_robin_is_intersection() {
        // Resource 1 busy 10-12, resource 2 busy 11-13: only 11-12 has
        // NO free resource, so only 11-12 blocks.
        let per = vec![
            vec![(dt("2026-07-30T10:00"), dt("2026-07-30T12:00"))],
            vec![(dt("2026-07-30T11:00"), dt("2026-07-30T13:00"))],
        ];
        let out = merge_mode_busy(&per, "round_robin");
        assert_eq!(out, vec![(dt("2026-07-30T11:00"), dt("2026-07-30T12:00"))]);
    }

    #[test]
    fn merge_round_robin_one_free_resource_never_blocks() {
        let per = vec![
            vec![(dt("2026-07-30T10:00"), dt("2026-07-30T12:00"))],
            vec![],
        ];
        let out = merge_mode_busy(&per, "round_robin");
        assert!(out.is_empty());
    }

    #[tokio::test]
    async fn busy_for_resource_all_day_exclusive_dtend_spans_full_days() {
        use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
        use std::str::FromStr;
        let pool = SqlitePoolOptions::new()
            .max_connections(2)
            .connect_with(
                SqliteConnectOptions::from_str("sqlite::memory:")
                    .unwrap()
                    .foreign_keys(true),
            )
            .await
            .unwrap();
        crate::db::migrate(&pool).await.unwrap();

        let resource_id = Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO resources (id, name, feed_url, last_synced_at) \
             VALUES (?, 'Lab', 'https://feed.invalid/cal.ics', datetime('now'))",
        )
        .bind(&resource_id)
        .execute(&pool)
        .await
        .unwrap();
        // All-day event, exclusive DTEND: July 30 + July 31, ends Aug 1 00:00.
        sqlx::query(
            "INSERT INTO resource_events (id, resource_id, uid, start_at, end_at, all_day) \
             VALUES (?, ?, 'uid-1', '20260730', '20260801', 1)",
        )
        .bind(Uuid::new_v4().to_string())
        .bind(&resource_id)
        .execute(&pool)
        .await
        .unwrap();

        let busy = busy_for_resource(
            &pool,
            &resource_id,
            dt("2026-07-25T00:00"),
            dt("2026-08-05T00:00"),
            chrono_tz::Tz::UTC,
            None,
        )
        .await;
        assert_eq!(
            busy,
            vec![(dt("2026-07-30T00:00"), dt("2026-08-01T00:00"))],
            "all-day event must span July 30 00:00 to August 1 00:00"
        );
    }

    #[test]
    fn derive_caldav_url_bluemind() {
        let feed = "https://mail.example.com/api/calendars/publish/calendar:6A8EB8E1-7FD8/x-calendar-public-abc";
        assert_eq!(
            derive_caldav_url(feed).as_deref(),
            Some("https://mail.example.com/dav/calendars/__uids__/6A8EB8E1-7FD8/calendar:6A8EB8E1-7FD8/")
        );
        assert_eq!(derive_caldav_url("https://example.com/some/feed.ics"), None);
    }

    #[test]
    fn feed_calendar_name_extracts() {
        let body = "BEGIN:VCALENDAR\r\nX-WR-CALNAME:Vates Demo Lab 1\r\nEND:VCALENDAR\r\n";
        assert_eq!(
            feed_calendar_name(body).as_deref(),
            Some("Vates Demo Lab 1")
        );
        assert_eq!(feed_calendar_name("BEGIN:VCALENDAR\r\n"), None);
    }
}
