//! Booking storage: version 0 is an untouched legacy wall clock; version 1 is
//! an RFC3339 UTC instant. The database enforces the UTC marker for version 1.
use chrono::{DateTime, Duration, NaiveDateTime, TimeZone, Utc};
use chrono_tz::Tz;
use sqlx::SqlitePool;

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
    Some(
        legacy_tz
            .from_local_datetime(&wall)
            .earliest()
            .unwrap_or_else(|| legacy_tz.from_utc_datetime(&wall))
            .with_timezone(&target)
            .naive_local(),
    )
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

pub async fn event_timezone(pool: &SqlitePool, id: &str) -> Tz {
    sqlx::query_scalar::<_, Option<String>>("SELECT COALESCE(NULLIF(et.timezone, ''), u.timezone) FROM event_types et JOIN accounts a ON a.id = et.account_id LEFT JOIN users u ON u.id = a.user_id WHERE et.id = ?")
        .bind(id).fetch_optional(pool).await.ok().flatten().flatten()
        .and_then(|tz| tz.parse().ok()).unwrap_or(Tz::UTC)
}

/// Frequency limits are calendar periods in the event timezone, not UTC days.
/// Widen the indexed SQL candidate window to cover every UTC offset, then
/// perform the exact comparison after conversion (including DST boundaries).
pub async fn period_counts(
    pool: &SqlitePool,
    event: &str,
    start: NaiveDateTime,
    end: NaiveDateTime,
) -> Vec<(Option<String>, i64)> {
    let tz = event_timezone(pool, event).await;
    let rows: Vec<(String, Option<String>)> = sqlx::query_as("SELECT CASE time_version WHEN 1 THEN start_at ELSE rtrim(start_at, 'Z') END AS start_at, assigned_user_id FROM bookings WHERE event_type_id = ? AND status IN ('confirmed', 'pending') AND start_at >= ? AND start_at < ?")
        .bind(event).bind((start - Duration::days(2)).to_string()).bind((end + Duration::days(2)).to_string())
        .fetch_all(pool).await.unwrap_or_default();
    let mut counts = std::collections::BTreeMap::new();
    for (value, member) in rows {
        if local(&value, tz, tz).is_some_and(|v| v >= start && v < end) {
            *counts.entry(member).or_insert(0) += 1;
        }
    }
    counts.into_iter().collect()
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
    let tz = event_timezone(pool, event).await;
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
