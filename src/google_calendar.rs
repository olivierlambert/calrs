//! Supplemental Google Calendar status-event sync. The Calendar API is the
//! documented interface for `outOfOffice`; don't infer absence from a title or
//! rely on a proprietary iCalendar extension being present in CalDAV.

use anyhow::{bail, Context, Result};
use chrono::{DateTime, Duration, Utc};
use serde::Deserialize;
use sqlx::SqlitePool;
use std::collections::HashSet;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct EventTime {
    date_time: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StatusEvent {
    id: String,
    event_type: String,
    status: Option<String>,
    summary: Option<String>,
    start: Option<EventTime>,
    end: Option<EventTime>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct EventPage {
    #[serde(default)]
    items: Vec<StatusEvent>,
    next_page_token: Option<String>,
}

pub(crate) async fn sync_out_of_office(
    pool: &SqlitePool,
    calendar_id: &str,
    primary_calendar_href: &str,
    href: &str,
    token: &str,
) -> Result<usize> {
    let remote_id = crate::google_meet::calendar_id_from_caldav_href(href)
        .context("Invalid Google calendar collection href")?;
    let primary_id = crate::google_meet::calendar_id_from_caldav_href(primary_calendar_href)
        .context("Invalid Google primary calendar href")?;
    // Google supports status events only on primary calendars. Subscribed
    // holidays and other secondary collections can return 404 for this query.
    if !remote_id.eq_ignore_ascii_case(&primary_id) {
        tracing::debug!("skipping Google out-of-office query for secondary calendar");
        replace_snapshot(pool, calendar_id, &[]).await?;
        return Ok(0);
    }
    let url = format!(
        "https://www.googleapis.com/calendar/v3/calendars/{}/events",
        urlencoding::encode(&remote_id)
    );
    let now = Utc::now();
    // Expand recurrence at Google so moved/cancelled instances and DST are
    // authoritative. Cover the booking validator's 365-day limit, plus a day
    // for guest timezone boundaries; retain the same history as CalDAV sync.
    let events = fetch_out_of_office(
        &url,
        token,
        &(now - Duration::days(90)).to_rfc3339(),
        &(now + Duration::days(366)).to_rfc3339(),
    )
    .await?;
    replace_snapshot(pool, calendar_id, &events).await
}

async fn fetch_out_of_office(
    url: &str,
    token: &str,
    time_min: &str,
    time_max: &str,
) -> Result<Vec<StatusEvent>> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .redirect(reqwest::redirect::Policy::none())
        .build()?;
    let mut events = Vec::new();
    let mut page_token = None;
    let mut seen_tokens = HashSet::new();
    loop {
        let mut request = client.get(url).bearer_auth(token).query(&[
            ("eventTypes", "outOfOffice"),
            ("singleEvents", "true"),
            ("showDeleted", "false"),
            ("maxResults", "2500"),
            ("timeMin", time_min),
            ("timeMax", time_max),
        ]);
        if let Some(ref page_token) = page_token {
            request = request.query(&[("pageToken", page_token)]);
        }
        let response = request.send().await?;
        if !response.status().is_success() {
            let status = response.status();
            bail!("Google out-of-office fetch: HTTP {}", status);
        }
        let page: EventPage = response.json().await?;
        events.extend(page.items);
        match page.next_page_token {
            Some(next) if !next.is_empty() => {
                if !seen_tokens.insert(next.clone()) || seen_tokens.len() > 100 {
                    bail!("Google out-of-office pagination did not terminate");
                }
                page_token = Some(next);
            }
            _ => return Ok(events),
        }
    }
}

/// Replace only API-owned rows, atomically, after every page has been fetched.
/// An empty successful snapshot clears deleted absences; any fetch, parse or DB
/// failure leaves the previous snapshot intact. Ordinary CalDAV rows survive.
async fn replace_snapshot(
    pool: &SqlitePool,
    calendar_id: &str,
    events: &[StatusEvent],
) -> Result<usize> {
    let mut tx = pool.begin().await?;
    sqlx::query("DELETE FROM events WHERE calendar_id = ? AND google_out_of_office = 1")
        .bind(calendar_id)
        .execute(&mut *tx)
        .await?;
    let mut count = 0;
    for event in events {
        if event.event_type != "outOfOffice" || event.status.as_deref() == Some("cancelled") {
            continue;
        }
        let start = DateTime::parse_from_rfc3339(
            &event
                .start
                .as_ref()
                .context("Missing out-of-office start")?
                .date_time,
        )?
        .with_timezone(&Utc);
        let end = DateTime::parse_from_rfc3339(
            &event
                .end
                .as_ref()
                .context("Missing out-of-office end")?
                .date_time,
        )?
        .with_timezone(&Utc);
        if event.id.is_empty() || end <= start {
            bail!("Invalid Google out-of-office interval");
        }
        // Use a distinct UID namespace: CalDAV can expose the same absence,
        // but its incremental/deletion lifecycle must not own this API copy.
        sqlx::query(
            "INSERT INTO events (id, calendar_id, uid, summary, start_at, end_at,
                 timezone, status, transp, google_out_of_office)
             VALUES (?, ?, ?, ?, ?, ?, 'UTC', 'CONFIRMED', 'OPAQUE', 1)",
        )
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(calendar_id)
        .bind(format!("google-out-of-office:{}", event.id))
        .bind(&event.summary)
        .bind(start.format("%Y%m%dT%H%M%SZ").to_string())
        .bind(end.format("%Y%m%dT%H%M%SZ").to_string())
        .execute(&mut *tx)
        .await?;
        count += 1;
    }
    tx.commit().await?;
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{extract::Query, http::HeaderMap, routing::get, Json, Router};
    use serde_json::json;
    use std::collections::HashMap;

    fn absence(id: &str, start: &str, end: &str) -> StatusEvent {
        serde_json::from_value(json!({
            "id": id, "eventType": "outOfOffice", "status": "confirmed",
            "summary": "Vacation", "start": {"dateTime": start}, "end": {"dateTime": end}
        }))
        .unwrap()
    }

    async fn database() -> SqlitePool {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        crate::db::migrate(&pool).await.unwrap();
        sqlx::query(
            "INSERT INTO accounts (id, name, email) VALUES ('a', 'Host', 'host@example.com')",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query("INSERT INTO caldav_sources (id, account_id, name, url, username) VALUES ('s', 'a', 'Google', 'https://apidata.googleusercontent.com', 'host@example.com')")
            .execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO calendars (id, source_id, href) VALUES ('c', 's', '/caldav/v2/host%40example.com/events'), ('other', 's', '/other')")
            .execute(&pool).await.unwrap();
        pool
    }

    #[tokio::test]
    async fn snapshot_normalizes_whole_day_and_timed_absences_and_clears_deletions() {
        let pool = database().await;
        let whole_day = absence(
            "day",
            "2026-10-25T00:00:00+02:00",
            "2026-10-26T00:00:00+01:00",
        );
        let timed = absence(
            "series_20261026",
            "2026-10-26T09:00:00+01:00",
            "2026-10-26T12:00:00+01:00",
        );
        sqlx::query("INSERT INTO events (id, calendar_id, uid, start_at, end_at) VALUES ('normal', 'c', 'normal', '20261026T130000', '20261026T140000')")
            .execute(&pool).await.unwrap();
        assert_eq!(
            replace_snapshot(&pool, "c", &[whole_day, timed])
                .await
                .unwrap(),
            2
        );
        replace_snapshot(
            &pool,
            "other",
            &[absence(
                "other",
                "2026-10-26T09:00:00Z",
                "2026-10-26T10:00:00Z",
            )],
        )
        .await
        .unwrap();
        let rows: Vec<(String, String, String, String)> = sqlx::query_as(
            "SELECT start_at, end_at, timezone, transp FROM events WHERE calendar_id = 'c' AND google_out_of_office = 1 ORDER BY start_at",
        ).fetch_all(&pool).await.unwrap();
        assert_eq!(
            rows[0],
            (
                "20261024T220000Z".into(),
                "20261025T230000Z".into(),
                "UTC".into(),
                "OPAQUE".into()
            )
        );
        assert_eq!(rows[1].0, "20261026T080000Z");
        assert_eq!(rows[1].1, "20261026T110000Z");
        replace_snapshot(&pool, "c", &[]).await.unwrap();
        let remaining: Vec<String> = sqlx::query_scalar("SELECT uid FROM events ORDER BY uid")
            .fetch_all(&pool)
            .await
            .unwrap();
        assert_eq!(remaining, vec!["google-out-of-office:other", "normal"]);
    }

    #[tokio::test]
    async fn invalid_snapshot_preserves_previous_blocks() {
        let pool = database().await;
        replace_snapshot(
            &pool,
            "c",
            &[absence(
                "old",
                "2026-10-26T09:00:00Z",
                "2026-10-26T10:00:00Z",
            )],
        )
        .await
        .unwrap();
        assert!(replace_snapshot(
            &pool,
            "c",
            &[absence("bad", "invalid", "2026-10-26T10:00:00Z")]
        )
        .await
        .is_err());
        let uid: String = sqlx::query_scalar("SELECT uid FROM events")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(uid, "google-out-of-office:old");
    }

    #[tokio::test]
    async fn cancelled_absences_and_working_locations_do_not_block() {
        let pool = database().await;
        let mut cancelled = absence("cancelled", "2026-10-26T09:00:00Z", "2026-10-26T10:00:00Z");
        cancelled.status = Some("cancelled".into());
        let mut location = absence("location", "2026-10-26T09:00:00Z", "2026-10-26T10:00:00Z");
        location.event_type = "workingLocation".into();
        assert_eq!(
            replace_snapshot(&pool, "c", &[cancelled, location])
                .await
                .unwrap(),
            0
        );
    }

    #[tokio::test]
    async fn secondary_calendar_skips_api_call_and_clears_any_old_snapshot() {
        let pool = database().await;
        replace_snapshot(
            &pool,
            "other",
            &[absence(
                "stale",
                "2026-10-26T09:00:00Z",
                "2026-10-26T10:00:00Z",
            )],
        )
        .await
        .unwrap();
        let count = sync_out_of_office(
            &pool,
            "other",
            "https://apidata.googleusercontent.com/caldav/v2/host%40example.com/user",
            "https://apidata.googleusercontent.com/caldav/v2/en.usa%23holiday%40group.v.calendar.google.com/events",
            "must-not-be-used",
        )
        .await
        .unwrap();
        assert_eq!(count, 0);
        let remaining: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM events WHERE calendar_id = 'other' AND google_out_of_office = 1",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(remaining, 0);
    }

    #[tokio::test]
    async fn fetch_paginates_expanded_status_events_and_rejects_partial_results() {
        async fn page(
            headers: HeaderMap,
            Query(query): Query<HashMap<String, String>>,
        ) -> axum::response::Response {
            use axum::response::IntoResponse;
            assert_eq!(headers["authorization"], "Bearer test-token");
            assert_eq!(query["eventTypes"], "outOfOffice");
            assert_eq!(query["singleEvents"], "true");
            assert_eq!(query["showDeleted"], "false");
            assert_eq!(query["timeMax"], "2027-10-01T00:00:00Z");
            if query.contains_key("pageToken") {
                if query["timeMin"] == "fail" {
                    return (
                        axum::http::StatusCode::NOT_FOUND,
                        Json(json!({"error": {"message": "Requested entity was not found."}})),
                    )
                        .into_response();
                }
                return Json(json!({"items": [{"id": "second", "eventType": "outOfOffice"}]}))
                    .into_response();
            }
            Json(json!({"items": [{"id": "first", "eventType": "outOfOffice"}], "nextPageToken": "next"})).into_response()
        }
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/events", listener.local_addr().unwrap());
        let server = tokio::spawn(async move {
            axum::serve(listener, Router::new().route("/events", get(page)))
                .await
                .unwrap();
        });
        let events = fetch_out_of_office(
            &url,
            "test-token",
            "2026-10-01T00:00:00Z",
            "2027-10-01T00:00:00Z",
        )
        .await
        .unwrap();
        assert_eq!(events.len(), 2);
        assert_eq!(events[1].id, "second");
        let error = fetch_out_of_office(&url, "test-token", "fail", "2027-10-01T00:00:00Z")
            .await
            .unwrap_err();
        assert!(error.to_string().contains("HTTP 404"));
        assert!(!error.to_string().contains("Requested entity was not found"));
        server.abort();
    }

    #[test]
    fn secondary_google_calendars_are_not_status_event_sources() {
        let primary = "https://apidata.googleusercontent.com/caldav/v2/alice%40example.com/events";
        let holidays = "https://apidata.googleusercontent.com/caldav/v2/en.usa%23holiday%40group.v.calendar.google.com/events";
        let same_primary =
            "https://apidata.googleusercontent.com/caldav/v2/ALICE%40EXAMPLE.COM/events";
        assert!(crate::google_meet::calendar_id_from_caldav_href(primary)
            .unwrap()
            .eq_ignore_ascii_case(
                &crate::google_meet::calendar_id_from_caldav_href(same_primary).unwrap()
            ));
        assert_ne!(
            crate::google_meet::calendar_id_from_caldav_href(primary),
            crate::google_meet::calendar_id_from_caldav_href(holidays)
        );
    }
}
