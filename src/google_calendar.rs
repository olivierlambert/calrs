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

// --- Booking writes that Google does not announce (#237) ---
//
// calrs emails the guest and the additional attendees its own invitation
// and ICS. Google's CalDAV endpoint ignores `SCHEDULE-AGENT=CLIENT` (RFC
// 6638) and mails attendees itself, so every booking used to reach them
// twice. The guests still belong on the event (Google Meet lets listed
// attendees in without the "ask to join" lobby), so on Google the attendee
// list goes through the Calendar API, whose `sendUpdates=none` is the
// documented way to change an event without mailing anyone.

/// One ATTENDEE moved from the ICS to the Calendar API.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Attendee {
    pub email: String,
    pub display_name: Option<String>,
}

/// The ICS without its ATTENDEE and ORGANIZER lines (refolded), and the
/// attendees it named, deduplicated by address. The organizer is dropped
/// rather than moved: Google makes the calendar's own account the organizer,
/// which is also what lets that account mint the Meet.
pub(crate) fn split_scheduling(ics: &str) -> (String, Vec<Attendee>) {
    let unfolded = crate::utils::unfold_ical(ics);
    let mut kept: Vec<&str> = Vec::new();
    let mut attendees: Vec<Attendee> = Vec::new();
    for raw in unfolded.split('\n') {
        let line = raw.strip_suffix('\r').unwrap_or(raw);
        let parsed = crate::utils::parse_content_line(line);
        let name = parsed.as_ref().map(|p| p.name).unwrap_or("");
        if name.eq_ignore_ascii_case("ORGANIZER") {
            continue;
        }
        if !name.eq_ignore_ascii_case("ATTENDEE") {
            kept.push(line);
            continue;
        }
        let Some(parsed) = parsed else { continue };
        let value = parsed.value.trim();
        let email = if value.len() >= 7 && value[..7].eq_ignore_ascii_case("mailto:") {
            value[7..].trim()
        } else {
            value
        };
        if email.is_empty()
            || attendees
                .iter()
                .any(|a| a.email.eq_ignore_ascii_case(email))
        {
            continue;
        }
        let display_name = parsed
            .param("CN")
            .map(|cn| {
                // RFC 6868 caret escapes, as `sanitize_ics_param` writes them.
                cn.replace("^n", " ").replace("^'", "\"").replace("^^", "^")
            })
            .filter(|cn| !cn.trim().is_empty());
        attendees.push(Attendee {
            email: email.to_string(),
            display_name,
        });
    }
    (crate::email::fold_ics(&kept.join("\r\n")), attendees)
}

/// One event found by iCalUID.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FoundEvent {
    pub id: String,
    pub has_attendees: bool,
}

#[derive(Deserialize)]
struct FoundPage {
    #[serde(default)]
    items: Vec<FoundItem>,
}

#[derive(Deserialize)]
struct FoundItem {
    id: Option<String>,
    #[serde(default)]
    attendees: Vec<serde_json::Value>,
}

/// The Calendar API `events` collection of one calendar. Errors carry the
/// HTTP status only, never the response body (it can quote event content).
pub(crate) struct EventsApi<'a> {
    base: &'a str,
    token: &'a str,
    calendar_id: &'a str,
    /// Between lookups of an event just written over CalDAV, which can take
    /// a moment to appear in the iCalUID index.
    pub retry_delay: std::time::Duration,
    pub lookup_attempts: u32,
}

impl<'a> EventsApi<'a> {
    pub(crate) fn new(base: &'a str, token: &'a str, calendar_id: &'a str) -> Self {
        Self {
            base: base.trim_end_matches('/'),
            token,
            calendar_id,
            retry_delay: std::time::Duration::from_millis(300),
            lookup_attempts: 5,
        }
    }

    fn http() -> Result<reqwest::Client> {
        Ok(reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(20))
            .redirect(reqwest::redirect::Policy::none())
            .build()?)
    }

    fn events_url(&self) -> String {
        format!(
            "{}/calendars/{}/events",
            self.base,
            urlencoding::encode(self.calendar_id)
        )
    }

    fn event_url(&self, event_id: &str) -> String {
        format!("{}/{}", self.events_url(), urlencoding::encode(event_id))
    }

    pub(crate) async fn find(&self, ical_uid: &str) -> Result<Option<FoundEvent>> {
        let response = Self::http()?
            .get(self.events_url())
            .bearer_auth(self.token)
            .query(&[("iCalUID", ical_uid), ("showDeleted", "false")])
            .send()
            .await?;
        if !response.status().is_success() {
            bail!("Google Calendar events lookup: HTTP {}", response.status());
        }
        let page: FoundPage = response.json().await?;
        Ok(page.items.into_iter().find_map(|item| {
            let id = item.id.filter(|id| !id.is_empty())?;
            Some(FoundEvent {
                id,
                has_attendees: !item.attendees.is_empty(),
            })
        }))
    }

    /// `find`, retried while the event is not visible yet.
    async fn find_written(&self, ical_uid: &str) -> Result<Option<FoundEvent>> {
        let attempts = self.lookup_attempts.max(1);
        for attempt in 0..attempts {
            if let Some(found) = self.find(ical_uid).await? {
                return Ok(Some(found));
            }
            if attempt + 1 < attempts && !self.retry_delay.is_zero() {
                tokio::time::sleep(self.retry_delay).await;
            }
        }
        Ok(None)
    }

    /// Replace the attendee list without notifying anyone.
    pub(crate) async fn set_attendees(&self, event_id: &str, attendees: &[Attendee]) -> Result<()> {
        let body = serde_json::json!({ "attendees": attendees_json(attendees) });
        let response = Self::http()?
            .patch(self.event_url(event_id))
            .bearer_auth(self.token)
            .query(&[("sendUpdates", "none")])
            .json(&body)
            .send()
            .await?;
        if !response.status().is_success() {
            bail!(
                "Google Calendar attendee update: HTTP {}",
                response.status()
            );
        }
        Ok(())
    }

    /// Delete the event without notifying anyone. Already gone is success.
    pub(crate) async fn delete(&self, event_id: &str) -> Result<()> {
        let response = Self::http()?
            .delete(self.event_url(event_id))
            .bearer_auth(self.token)
            .query(&[("sendUpdates", "none")])
            .send()
            .await?;
        let status = response.status();
        if !status.is_success() && status.as_u16() != 404 && status.as_u16() != 410 {
            bail!("Google Calendar event delete: HTTP {}", status);
        }
        Ok(())
    }
}

pub(crate) fn attendees_json(attendees: &[Attendee]) -> serde_json::Value {
    serde_json::Value::Array(
        attendees
            .iter()
            .map(|a| match &a.display_name {
                Some(name) => serde_json::json!({ "email": a.email, "displayName": name }),
                None => serde_json::json!({ "email": a.email }),
            })
            .collect(),
    )
}

/// Write a booking to a Google calendar with its guests as attendees and
/// no mail from Google:
///
/// 1. an earlier copy that carries attendees (a reschedule, a retry, or an
///    event written before this change) loses them first, silently, or the
///    CalDAV PUT below, which names none, would read as removing them;
/// 2. the event is PUT over CalDAV without ATTENDEE or ORGANIZER;
/// 3. the attendees are added back through the Calendar API.
///
/// Without the Calendar API (not enabled in the Google Cloud project, a
/// transient failure), the event is still written, host-only, and the guests
/// rely on calrs's own invitation. A failure of step 3 is logged and not
/// returned: the event is on the calendar, and reporting the write as failed
/// would keep a cancellation from deleting it.
pub(crate) async fn put_event_without_invitations(
    client: &crate::caldav::CaldavClient,
    api: &EventsApi<'_>,
    calendar_href: &str,
    uid: &str,
    ics: &str,
) -> Result<()> {
    let (host_only, attendees) = split_scheduling(ics);
    let existing = match api.find(uid).await {
        Ok(found) => found,
        Err(e) => {
            tracing::warn!(uid = %uid, error = %e, "google calendar: Calendar API unavailable, writing the event without attendees");
            return client.put_event_raw(calendar_href, uid, &host_only).await;
        }
    };
    if let Some(found) = existing.as_ref().filter(|f| f.has_attendees) {
        if let Err(e) = api.set_attendees(&found.id, &[]).await {
            // Leave the event as it is rather than let a PUT drop its
            // attendees in a way Google announces. The write-back retries.
            tracing::warn!(uid = %uid, error = %e, "google calendar: could not clear attendees before rewriting the event");
            return Err(e.context("Google Calendar attendee update before rewrite"));
        }
    }
    client.put_event_raw(calendar_href, uid, &host_only).await?;
    if attendees.is_empty() {
        return Ok(());
    }
    // Looked up again rather than reusing `existing`: the PUT may have
    // replaced the event under a new id.
    let event_id = api.find_written(uid).await.ok().flatten().map(|f| f.id);
    let result = match event_id {
        Some(id) => api.set_attendees(&id, &attendees).await,
        None => Err(anyhow::anyhow!(
            "event not visible through the Calendar API"
        )),
    };
    if let Err(e) = result {
        tracing::warn!(uid = %uid, error = %e, "google calendar: event written, but the guests could not be added as attendees");
    }
    Ok(())
}

/// Delete a booking from a Google calendar without a cancellation from
/// Google. An event with attendees goes through the Calendar API with
/// `sendUpdates=none`; one without (or not visible to the API) through
/// CalDAV, as for any server. When the API cannot be reached the CalDAV
/// DELETE still runs: a meeting left on the host's calendar after the guest
/// was told it is cancelled is worse than Google's duplicate notice.
pub(crate) async fn delete_event_without_cancellations(
    client: &crate::caldav::CaldavClient,
    api: &EventsApi<'_>,
    calendar_href: &str,
    uid: &str,
) -> Result<()> {
    match api.find(uid).await {
        Ok(Some(found)) if found.has_attendees => api.delete(&found.id).await,
        Ok(_) => client.delete_event_raw(calendar_href, uid).await,
        Err(e) => {
            tracing::warn!(uid = %uid, error = %e, "google calendar: Calendar API unavailable, deleting over CalDAV; Google may notify the attendees");
            client.delete_event_raw(calendar_href, uid).await
        }
    }
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

    // --- #237: booking writes Google does not announce ---

    /// A fake Google: CalDAV under `/caldav/v2/`, the Calendar API under
    /// `/api/`. Every request is logged as "METHOD path?query body".
    #[derive(Default)]
    struct FakeGoogle {
        log: std::sync::Mutex<Vec<String>>,
        /// iCalUID -> (event id, attendees as the API holds them).
        events: std::sync::Mutex<HashMap<String, (String, Vec<serde_json::Value>)>>,
        api_down: bool,
        next_id: std::sync::atomic::AtomicU32,
    }

    async fn fake_google(
        axum::extract::State(fake): axum::extract::State<std::sync::Arc<FakeGoogle>>,
        method: axum::http::Method,
        uri: axum::http::Uri,
        body: String,
    ) -> axum::response::Response {
        use axum::response::IntoResponse;
        use std::sync::atomic::Ordering;
        let path = uri.path().to_string();
        let query = uri.query().unwrap_or("").to_string();
        fake.log
            .lock()
            .unwrap()
            .push(format!("{method} {path}?{query} {body}"));
        if let Some(rest) = path.strip_prefix("/caldav/v2/host%40example.com/events/") {
            let uid = rest.trim_end_matches(".ics").to_string();
            let mut events = fake.events.lock().unwrap();
            return match method.as_str() {
                "PUT" => {
                    // A CalDAV PUT replaces the event, attendees included.
                    let id = events.get(&uid).map(|e| e.0.clone()).unwrap_or_else(|| {
                        format!("ev{}", fake.next_id.fetch_add(1, Ordering::SeqCst))
                    });
                    events.insert(uid, (id, Vec::new()));
                    axum::http::StatusCode::CREATED.into_response()
                }
                "DELETE" => {
                    events.remove(&uid);
                    axum::http::StatusCode::NO_CONTENT.into_response()
                }
                _ => axum::http::StatusCode::METHOD_NOT_ALLOWED.into_response(),
            };
        }
        if fake.api_down {
            return axum::http::StatusCode::FORBIDDEN.into_response();
        }
        let Some(rest) = path.strip_prefix("/api/calendars/host%40example.com/events") else {
            return axum::http::StatusCode::NOT_FOUND.into_response();
        };
        let mut events = fake.events.lock().unwrap();
        if rest.is_empty() && method == axum::http::Method::GET {
            let uid = query
                .split('&')
                .find_map(|kv| kv.strip_prefix("iCalUID="))
                .map(|v| urlencoding::decode(v).unwrap().into_owned())
                .unwrap_or_default();
            let items: Vec<_> = events
                .get(&uid)
                .map(|(id, attendees)| json!({"id": id, "attendees": attendees}))
                .into_iter()
                .collect();
            return Json(json!({ "items": items })).into_response();
        }
        let id = rest.trim_start_matches('/');
        let Some(uid) = events
            .iter()
            .find(|(_, (eid, _))| eid == id)
            .map(|(uid, _)| uid.clone())
        else {
            return axum::http::StatusCode::NOT_FOUND.into_response();
        };
        match method.as_str() {
            "PATCH" => {
                let patch: serde_json::Value = serde_json::from_str(&body).unwrap();
                let attendees = patch["attendees"].as_array().cloned().unwrap_or_default();
                events.get_mut(&uid).unwrap().1 = attendees;
                Json(json!({"id": id})).into_response()
            }
            "DELETE" => {
                events.remove(&uid);
                axum::http::StatusCode::NO_CONTENT.into_response()
            }
            _ => axum::http::StatusCode::METHOD_NOT_ALLOWED.into_response(),
        }
    }

    const HREF: &str = "/caldav/v2/host%40example.com/events";

    async fn serve_fake(
        fake: std::sync::Arc<FakeGoogle>,
    ) -> (crate::caldav::CaldavClient, tokio::task::JoinHandle<()>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let app = Router::new().fallback(fake_google).with_state(fake);
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        let client = crate::caldav::CaldavClient::with_bearer(&base, "test-token")
            .with_google_api_base(&format!("{base}/api"));
        (client, server)
    }

    fn booking_ics(uid: &str) -> String {
        crate::email::generate_ics_caldav(&crate::email::BookingDetails {
            event_title: "Intro Call".into(),
            date: "2026-03-10".into(),
            start_time: "14:00".into(),
            end_time: "14:30".into(),
            guest_name: "Doe, Jane".into(),
            guest_email: "jane@example.com".into(),
            guest_timezone: "UTC".into(),
            host_name: "Alice".into(),
            host_email: "alias@example.com".into(),
            uid: uid.into(),
            notes: Some("Discuss plans".into()),
            additional_attendees: vec!["bob@example.com".into()],
            ..Default::default()
        })
    }

    fn log(fake: &FakeGoogle) -> Vec<String> {
        fake.log.lock().unwrap().clone()
    }

    /// Every Calendar API write carries `sendUpdates=none`, and no CalDAV
    /// write names an attendee or an organizer: the two ways Google mails.
    fn assert_nothing_announced(log: &[String]) {
        for entry in log {
            if entry.starts_with("PATCH /api/") || entry.starts_with("DELETE /api/") {
                assert!(entry.contains("sendUpdates=none"), "{entry}");
            }
            if entry.starts_with("PUT /caldav/") {
                assert!(!entry.contains("ATTENDEE"), "{entry}");
                assert!(!entry.contains("ORGANIZER"), "{entry}");
            }
        }
    }

    #[test]
    fn split_scheduling_moves_attendees_and_drops_the_organizer() {
        let ics = booking_ics("split@calrs");
        assert!(ics.contains("ATTENDEE;SCHEDULE-AGENT=CLIENT"));
        let (host_only, attendees) = split_scheduling(&ics);
        assert_eq!(
            attendees,
            vec![
                Attendee {
                    email: "jane@example.com".into(),
                    display_name: Some("Doe, Jane".into()),
                },
                Attendee {
                    email: "bob@example.com".into(),
                    display_name: None,
                },
            ]
        );
        assert!(!host_only.contains("ATTENDEE"), "{host_only}");
        assert!(!host_only.contains("ORGANIZER"), "{host_only}");
        for kept in [
            "UID:split@calrs\r\n",
            "SUMMARY:",
            "DESCRIPTION:",
            "END:VCALENDAR\r\n",
        ] {
            assert!(host_only.contains(kept), "{kept} in {host_only}");
        }
        assert!(host_only.split("\r\n").all(|line| line.len() <= 75));

        // Folded lines, a quoted CN, an uppercase scheme and a repeat.
        let folded = "BEGIN:VEVENT\r\nATTENDEE;CN=\"Long Name ^'Q^' With: Colon\";RSVP=TRU\r\n E:MAILTO:x@example.com\r\nATTENDEE:mailto:X@example.com\r\nEND:VEVENT\r\n";
        let (rest, attendees) = split_scheduling(folded);
        assert_eq!(rest, "BEGIN:VEVENT\r\nEND:VEVENT\r\n");
        assert_eq!(
            attendees,
            vec![Attendee {
                email: "x@example.com".into(),
                display_name: Some("Long Name \"Q\" With: Colon".into()),
            }]
        );
    }

    #[tokio::test]
    async fn new_google_event_gets_its_attendees_without_a_google_invitation() {
        let fake = std::sync::Arc::new(FakeGoogle::default());
        let (client, server) = serve_fake(fake.clone()).await;
        client
            .put_event(HREF, "new@calrs", &booking_ics("new@calrs"))
            .await
            .unwrap();
        server.abort();
        let log = log(&fake);
        assert_nothing_announced(&log);
        assert!(log.iter().any(|e| e.starts_with("PUT /caldav/")));
        let events = fake.events.lock().unwrap();
        let (_, attendees) = &events["new@calrs"];
        assert_eq!(
            attendees,
            &vec![
                json!({"email": "jane@example.com", "displayName": "Doe, Jane"}),
                json!({"email": "bob@example.com"}),
            ]
        );
    }

    #[tokio::test]
    async fn rewriting_a_google_event_clears_its_attendees_before_the_put() {
        let fake = std::sync::Arc::new(FakeGoogle::default());
        fake.events.lock().unwrap().insert(
            "moved@calrs".into(),
            ("ev-old".into(), vec![json!({"email": "jane@example.com"})]),
        );
        let (client, server) = serve_fake(fake.clone()).await;
        client
            .put_event(HREF, "moved@calrs", &booking_ics("moved@calrs"))
            .await
            .unwrap();
        server.abort();
        let log = log(&fake);
        assert_nothing_announced(&log);
        let writes: Vec<&String> = log.iter().filter(|e| !e.starts_with("GET ")).collect();
        assert_eq!(writes.len(), 3, "{log:#?}");
        assert!(writes[0].starts_with("PATCH /api/calendars/host%40example.com/events/ev-old?"));
        assert!(writes[0].ends_with(r#"{"attendees":[]}"#), "{}", writes[0]);
        assert!(writes[1].starts_with("PUT /caldav/"));
        assert!(writes[2].starts_with("PATCH /api/") && writes[2].contains("jane@example.com"));
        assert_eq!(fake.events.lock().unwrap()["moved@calrs"].1.len(), 2);
    }

    #[tokio::test]
    async fn google_event_is_still_written_host_only_without_the_calendar_api() {
        let fake = std::sync::Arc::new(FakeGoogle {
            api_down: true,
            ..Default::default()
        });
        let (client, server) = serve_fake(fake.clone()).await;
        client
            .put_event(HREF, "noapi@calrs", &booking_ics("noapi@calrs"))
            .await
            .unwrap();
        server.abort();
        let log = log(&fake);
        assert_nothing_announced(&log);
        assert!(fake.events.lock().unwrap().contains_key("noapi@calrs"));
        assert!(!log.iter().any(|e| e.starts_with("PATCH ")));
    }

    #[tokio::test]
    async fn google_event_with_attendees_is_deleted_silently_through_the_api() {
        let fake = std::sync::Arc::new(FakeGoogle::default());
        {
            let mut events = fake.events.lock().unwrap();
            events.insert(
                "guests@calrs".into(),
                ("ev1".into(), vec![json!({"email": "jane@example.com"})]),
            );
            events.insert("alone@calrs".into(), ("ev2".into(), Vec::new()));
        }
        let (client, server) = serve_fake(fake.clone()).await;
        client.delete_event(HREF, "guests@calrs").await.unwrap();
        client.delete_event(HREF, "alone@calrs").await.unwrap();
        server.abort();
        let log = log(&fake);
        assert_nothing_announced(&log);
        assert!(log
            .iter()
            .any(|e| e.starts_with("DELETE /api/calendars/host%40example.com/events/ev1?")));
        assert!(log
            .iter()
            .any(|e| e.starts_with("DELETE /caldav/v2/host%40example.com/events/alone@calrs.ics")));
        assert!(!log.iter().any(|e| e.contains("/events/guests@calrs.ics")));
        assert!(fake.events.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn google_delete_falls_back_to_caldav_without_the_calendar_api() {
        let fake = std::sync::Arc::new(FakeGoogle {
            api_down: true,
            ..Default::default()
        });
        fake.events.lock().unwrap().insert(
            "gone@calrs".into(),
            ("ev1".into(), vec![json!({"email": "jane@example.com"})]),
        );
        let (client, server) = serve_fake(fake.clone()).await;
        client.delete_event(HREF, "gone@calrs").await.unwrap();
        server.abort();
        assert!(fake.events.lock().unwrap().is_empty());
    }

    /// Any other CalDAV server keeps the ATTENDEE lines (with
    /// SCHEDULE-AGENT=CLIENT) and sees no Calendar API call.
    #[tokio::test]
    async fn other_caldav_servers_keep_attendees_in_the_ics() {
        let fake = std::sync::Arc::new(FakeGoogle::default());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let app = Router::new().fallback(fake_google).with_state(fake.clone());
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        let client = crate::caldav::CaldavClient::with_bearer(&base, "test-token");
        client
            .put_event(HREF, "plain@calrs", &booking_ics("plain@calrs"))
            .await
            .unwrap();
        server.abort();
        let log = log(&fake);
        assert_eq!(log.len(), 1, "{log:#?}");
        assert!(
            log[0].starts_with("PUT /caldav/") && log[0].contains("ATTENDEE;SCHEDULE-AGENT=CLIENT")
        );
    }
}
