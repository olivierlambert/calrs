//! Supplemental Google Calendar status-event sync. The Calendar API is the
//! documented interface for `outOfOffice`; don't infer absence from a title or
//! rely on a proprietary iCalendar extension being present in CalDAV.

use anyhow::{bail, Context, Result};
use chrono::{DateTime, Duration, NaiveDate, SecondsFormat, TimeZone, Utc};
use chrono_tz::Tz;
use serde::Deserialize;
use sqlx::SqlitePool;
use std::collections::HashSet;

/// A start or end as the Calendar API sends it: `dateTime` for a timed
/// event, `date` (end exclusive) for a whole-day one.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct EventTime {
    date_time: Option<String>,
    date: Option<String>,
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
    /// The calendar's own timezone, which whole-day events are in.
    time_zone: Option<String>,
}

/// Days an out-of-office snapshot keeps blocking without a successful
/// fetch. Past them its rows are dropped: an absence the host has since
/// deleted must not block forever because the Calendar API stopped
/// answering (disabled, out of quota).
pub(crate) const SNAPSHOT_MAX_AGE_DAYS: i64 = 7;

/// How long an out-of-office refresh may take. A sync can run inside a
/// slot page request (on-demand sync), which must not wait on Google.
pub(crate) const REFRESH_BUDGET: std::time::Duration = std::time::Duration::from_secs(10);

/// A snapshot refreshed this recently is not fetched again: syncs come
/// every minute from the background loop and on demand from slot pages.
const REFRESH_INTERVAL: std::time::Duration = std::time::Duration::from_secs(5 * 60);

/// When each calendar's snapshot was last refreshed successfully, by
/// Calendar API base and calendar id. In memory: after a restart the first
/// sync refreshes.
static LAST_REFRESH: std::sync::LazyLock<
    std::sync::Mutex<std::collections::HashMap<String, std::time::Instant>>,
> = std::sync::LazyLock::new(Default::default);

/// Refresh one calendar's out-of-office snapshot during a source sync,
/// within `budget`, unless it was refreshed in the last five minutes.
/// Returns the failure to report on the source (so the sync health, the
/// dashboard badge and the owner's alert apply), `None` on success, when
/// skipped, or when the Calendar API is simply not in use: not usable for
/// this project or token, with nothing cached that it could leave stale.
/// On any failure, rows older than [`SNAPSHOT_MAX_AGE_DAYS`] are dropped.
/// A failed refresh is tried again on the next sync.
pub(crate) async fn refresh_out_of_office(
    pool: &SqlitePool,
    calendar_id: &str,
    primary_calendar_href: &str,
    href: &str,
    api_base: &str,
    token: &str,
    budget: std::time::Duration,
) -> Option<String> {
    let key = format!("{api_base}|{calendar_id}");
    let fresh = LAST_REFRESH
        .lock()
        .ok()
        .and_then(|last| last.get(&key).copied())
        .is_some_and(|at| at.elapsed() < REFRESH_INTERVAL);
    if fresh {
        return None;
    }
    let refreshed = tokio::time::timeout(
        budget,
        sync_out_of_office(
            pool,
            calendar_id,
            primary_calendar_href,
            href,
            api_base,
            token,
        ),
    )
    .await
    .unwrap_or_else(|_| {
        Err(anyhow::anyhow!(
            "Google out-of-office fetch: no answer within {} s",
            budget.as_secs()
        ))
    });
    let error = match refreshed {
        Ok(count) => {
            tracing::debug!(calendar_id = %calendar_id, count, "Google out-of-office synced");
            if let Ok(mut last) = LAST_REFRESH.lock() {
                last.insert(key, std::time::Instant::now());
            }
            return None;
        }
        Err(e) => e,
    };
    let expired = sqlx::query(
        "DELETE FROM events WHERE calendar_id = ? AND google_out_of_office = 1
           AND synced_at < datetime('now', ?)",
    )
    .bind(calendar_id)
    .bind(format!("-{SNAPSHOT_MAX_AGE_DAYS} days"))
    .execute(pool)
    .await
    .map(|r| r.rows_affected())
    .unwrap_or(0);
    let cached: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM events WHERE calendar_id = ? AND google_out_of_office = 1",
    )
    .bind(calendar_id)
    .fetch_one(pool)
    .await
    .unwrap_or(0);
    let no_access = error
        .downcast_ref::<ApiFailure>()
        .is_some_and(|f| f.no_access);
    if no_access && cached == 0 && expired == 0 {
        tracing::debug!(calendar_id = %calendar_id, error = %error, "Calendar API not usable, Google out-of-office not read");
        return None;
    }
    tracing::warn!(calendar_id = %calendar_id, error = %error, cached, expired, "Google out-of-office sync failed");
    Some(format!("Google out-of-office: {error:#}"))
}

pub(crate) async fn sync_out_of_office(
    pool: &SqlitePool,
    calendar_id: &str,
    primary_calendar_href: &str,
    href: &str,
    api_base: &str,
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
        replace_snapshot(pool, calendar_id, &[], Tz::UTC).await?;
        return Ok(0);
    }
    let url = format!(
        "{}/calendars/{}/events",
        api_base.trim_end_matches('/'),
        urlencoding::encode(&remote_id)
    );
    // Expand recurrence at Google so moved/cancelled instances and DST are
    // authoritative.
    let (time_min, time_max) = query_window(Utc::now());
    let (events, zone) = fetch_out_of_office(&url, token, &time_min, &time_max).await?;
    let zone = zone.and_then(|z| z.parse::<Tz>().ok()).unwrap_or(Tz::UTC);
    replace_snapshot(pool, calendar_id, &events, zone).await
}

/// `timeMin` and `timeMax` of the out-of-office query: the booking
/// validator's 365-day limit plus a day for guest timezone boundaries, and
/// the same history as CalDAV sync. Whole seconds in UTC, the RFC 3339
/// shape the Calendar API documents (`to_rfc3339` would send nanoseconds).
fn query_window(now: DateTime<Utc>) -> (String, String) {
    (
        (now - Duration::days(90)).to_rfc3339_opts(SecondsFormat::Secs, true),
        (now + Duration::days(366)).to_rfc3339_opts(SecondsFormat::Secs, true),
    )
}

async fn fetch_out_of_office(
    url: &str,
    token: &str,
    time_min: &str,
    time_max: &str,
) -> Result<(Vec<StatusEvent>, Option<String>)> {
    let client = crate::net_guard::plain_builder()
        .timeout(std::time::Duration::from_secs(60))
        .redirect(reqwest::redirect::Policy::none())
        .build()?;
    let mut events = Vec::new();
    let mut zone = None;
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
            return Err(ApiFailure::refused("Google out-of-office fetch", response)
                .await
                .into());
        }
        let page: EventPage = response.json().await?;
        zone = zone.or(page.time_zone);
        events.extend(page.items);
        match page.next_page_token {
            Some(next) if !next.is_empty() => {
                if !seen_tokens.insert(next.clone()) || seen_tokens.len() > 100 {
                    bail!("Google out-of-office pagination did not terminate");
                }
                page_token = Some(next);
            }
            _ => return Ok((events, zone)),
        }
    }
}

/// The UTC instant of a start or end. A whole day starts at midnight in
/// `zone`, the calendar's timezone, or at the first instant that exists
/// there when a clock change skips midnight.
fn instant(time: &EventTime, zone: Tz) -> Option<DateTime<Utc>> {
    if let Some(date_time) = time.date_time.as_deref() {
        return DateTime::parse_from_rfc3339(date_time)
            .ok()
            .map(|t| t.with_timezone(&Utc));
    }
    let day = NaiveDate::parse_from_str(time.date.as_deref()?, "%Y-%m-%d").ok()?;
    (0..=2).find_map(|hour| {
        zone.from_local_datetime(&day.and_hms_opt(hour, 0, 0)?)
            .earliest()
            .map(|t| t.with_timezone(&Utc))
    })
}

/// The UTC interval an absence blocks, `None` when Google sent something
/// unusable (no start or end, an unparseable or empty interval).
fn absence_interval(event: &StatusEvent, zone: Tz) -> Option<(DateTime<Utc>, DateTime<Utc>)> {
    let start = instant(event.start.as_ref()?, zone)?;
    let end = instant(event.end.as_ref()?, zone)?;
    (end > start).then_some((start, end))
}

/// Replace only API-owned rows, atomically, after every page has been fetched.
/// An empty successful snapshot clears deleted absences; a fetch or DB
/// failure leaves the previous snapshot intact. Ordinary CalDAV rows survive.
/// An item that cannot be read is skipped and logged rather than failing
/// the snapshot: one odd item must not freeze every other absence, deleted
/// ones included. A repeated id is stored once.
async fn replace_snapshot(
    pool: &SqlitePool,
    calendar_id: &str,
    events: &[StatusEvent],
    zone: Tz,
) -> Result<usize> {
    let mut tx = pool.begin().await?;
    sqlx::query("DELETE FROM events WHERE calendar_id = ? AND google_out_of_office = 1")
        .bind(calendar_id)
        .execute(&mut *tx)
        .await?;
    let mut count = 0;
    let mut seen = HashSet::new();
    for event in events {
        if event.event_type != "outOfOffice" || event.status.as_deref() == Some("cancelled") {
            continue;
        }
        let interval = absence_interval(event, zone).filter(|_| !event.id.is_empty());
        let Some((start, end)) = interval else {
            tracing::warn!(calendar_id = %calendar_id, "Google out-of-office item without a usable interval, skipped");
            continue;
        };
        if !seen.insert(event.id.as_str()) {
            continue;
        }
        // Use a distinct UID namespace: CalDAV can expose the same absence,
        // but its incremental/deletion lifecycle must not own this API copy.
        sqlx::query(
            "INSERT OR IGNORE INTO events (id, calendar_id, uid, summary, start_at, end_at,
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
/// The address of an ATTENDEE value, without its `mailto:`.
fn attendee_email(value: &str) -> &str {
    let value = value.trim();
    // `get`: a value whose seventh byte falls inside a character is no
    // `mailto:` one, and must not panic.
    match value.get(..7) {
        Some(scheme) if scheme.eq_ignore_ascii_case("mailto:") => value[7..].trim(),
        _ => value,
    }
}

/// The ICS without the ATTENDEE lines of `emails` (case blind), refolded.
/// Everything else is kept as it is.
pub(crate) fn without_attendees_named(ics: &str, emails: &[String]) -> String {
    if emails.is_empty() {
        return ics.to_string();
    }
    let unfolded = crate::utils::unfold_ical(ics);
    let kept: Vec<&str> = unfolded
        .split('\n')
        .map(|raw| raw.strip_suffix('\r').unwrap_or(raw))
        .filter(|line| {
            let Some(parsed) = crate::utils::parse_content_line(line) else {
                return true;
            };
            if !parsed.name.eq_ignore_ascii_case("ATTENDEE") {
                return true;
            }
            let email = attendee_email(parsed.value);
            !emails.iter().any(|e| e.trim().eq_ignore_ascii_case(email))
        })
        .collect();
    crate::email::fold_ics(&kept.join("\r\n"))
}

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
        let email = attendee_email(parsed.value);
        if email.is_empty()
            || attendees
                .iter()
                .any(|a| a.email.eq_ignore_ascii_case(email))
        {
            continue;
        }
        let display_name = parsed
            .param("CN")
            .map(decode_param_value)
            .filter(|cn| !cn.trim().is_empty());
        attendees.push(Attendee {
            email: email.to_string(),
            display_name,
        });
    }
    (crate::email::fold_ics(&kept.join("\r\n")), attendees)
}

/// Undo RFC 6868 caret escapes, as `sanitize_ics_param` writes them, in
/// one left-to-right pass so `^^n` is a caret then `n`, not a newline. A
/// newline becomes a space: this is a display name.
fn decode_param_value(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut chars = value.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '^' {
            out.push(c);
            continue;
        }
        match chars.peek() {
            Some('n') => out.push(' '),
            Some('\'') => out.push('"'),
            Some('^') => out.push('^'),
            _ => {
                out.push('^');
                continue;
            }
        }
        chars.next();
    }
    out
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

/// A Calendar API call that failed. It carries the HTTP status and whether
/// Google said the API is not usable at all, never the response body (it
/// can quote event content).
#[derive(Debug)]
pub(crate) struct ApiFailure {
    what: &'static str,
    /// `None` when there was no usable answer: a timeout, a network error,
    /// a body that does not parse.
    pub status: Option<u16>,
    /// The project has the Calendar API disabled, or the token lacks the
    /// scope: a persistent refusal, unlike a 429, a 5xx or a timeout.
    pub no_access: bool,
}

impl std::fmt::Display for ApiFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.status {
            Some(status) => write!(f, "{}: HTTP {}", self.what, status),
            None => write!(f, "{}: no usable answer", self.what),
        }
    }
}

impl std::error::Error for ApiFailure {}

/// Google's error reasons that mean the Calendar API cannot be used with
/// this project or token, whatever is retried: `accessNotConfigured` and
/// `SERVICE_DISABLED` (API not enabled), `insufficientPermissions` and
/// `ACCESS_TOKEN_SCOPE_INSUFFICIENT` (token without the scope).
const NO_ACCESS_REASONS: [&str; 4] = [
    "accessNotConfigured",
    "SERVICE_DISABLED",
    "insufficientPermissions",
    "ACCESS_TOKEN_SCOPE_INSUFFICIENT",
];

/// Whether a 403 body names one of [`NO_ACCESS_REASONS`], in either of the
/// shapes Google uses (`error.errors[].reason`, `error.details[].reason`).
pub(crate) fn says_no_access(body: &str) -> bool {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(body) else {
        return false;
    };
    ["errors", "details"].iter().any(|list| {
        value["error"][list].as_array().is_some_and(|items| {
            items.iter().any(|item| {
                item["reason"]
                    .as_str()
                    .is_some_and(|reason| NO_ACCESS_REASONS.contains(&reason))
            })
        })
    })
}

impl ApiFailure {
    fn transport(what: &'static str) -> Self {
        Self {
            what,
            status: None,
            no_access: false,
        }
    }

    /// The failure for a non-2xx answer. Reads the body for Google's reason
    /// and drops it.
    pub(crate) async fn refused(what: &'static str, response: reqwest::Response) -> Self {
        let status = response.status().as_u16();
        let body = response.text().await.unwrap_or_default();
        Self {
            what,
            status: Some(status),
            no_access: status == 403 && says_no_access(&body),
        }
    }
}

/// Total time the Calendar API part of one booking write may take, on top
/// of the CalDAV request itself, half before the PUT and half after; a
/// delete's lookup and its delete get it each. Past it, a write gives up
/// and is retried by the write-back retries; it runs inside the booking
/// request, which must not wait on Google for minutes.
pub(crate) const API_DEADLINE: std::time::Duration = std::time::Duration::from_secs(30);

/// The Calendar API `events` collection of one calendar.
pub(crate) struct EventsApi<'a> {
    base: &'a str,
    token: &'a str,
    calendar_id: &'a str,
    /// Between lookups of an event just written over CalDAV, which can take
    /// a moment to appear in the iCalUID index.
    pub retry_delay: std::time::Duration,
    pub lookup_attempts: u32,
    /// See [`API_DEADLINE`].
    pub deadline: std::time::Duration,
}

impl<'a> EventsApi<'a> {
    pub(crate) fn new(base: &'a str, token: &'a str, calendar_id: &'a str) -> Self {
        Self {
            base: base.trim_end_matches('/'),
            token,
            calendar_id,
            retry_delay: std::time::Duration::from_millis(300),
            lookup_attempts: 5,
            deadline: API_DEADLINE,
        }
    }

    fn http() -> std::result::Result<reqwest::Client, ApiFailure> {
        crate::net_guard::plain_builder()
            .timeout(std::time::Duration::from_secs(20))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|_| ApiFailure::transport("Google Calendar client"))
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

    pub(crate) async fn find(
        &self,
        ical_uid: &str,
    ) -> std::result::Result<Option<FoundEvent>, ApiFailure> {
        const WHAT: &str = "Google Calendar events lookup";
        let response = Self::http()?
            .get(self.events_url())
            .bearer_auth(self.token)
            .query(&[("iCalUID", ical_uid), ("showDeleted", "false")])
            .send()
            .await
            .map_err(|_| ApiFailure::transport(WHAT))?;
        if !response.status().is_success() {
            return Err(ApiFailure::refused(WHAT, response).await);
        }
        let page: FoundPage = response
            .json()
            .await
            .map_err(|_| ApiFailure::transport(WHAT))?;
        Ok(page.items.into_iter().find_map(|item| {
            let id = item.id.filter(|id| !id.is_empty())?;
            Some(FoundEvent {
                id,
                has_attendees: !item.attendees.is_empty(),
            })
        }))
    }

    /// `find`, retried while the event is not visible yet.
    async fn find_written(
        &self,
        ical_uid: &str,
    ) -> std::result::Result<Option<FoundEvent>, ApiFailure> {
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
    pub(crate) async fn set_attendees(
        &self,
        event_id: &str,
        attendees: &[Attendee],
    ) -> std::result::Result<(), ApiFailure> {
        const WHAT: &str = "Google Calendar attendee update";
        let body = serde_json::json!({ "attendees": attendees_json(attendees) });
        let response = Self::http()?
            .patch(self.event_url(event_id))
            .bearer_auth(self.token)
            .query(&[("sendUpdates", "none")])
            .json(&body)
            .send()
            .await
            .map_err(|_| ApiFailure::transport(WHAT))?;
        if !response.status().is_success() {
            return Err(ApiFailure::refused(WHAT, response).await);
        }
        Ok(())
    }

    /// Delete the event without notifying anyone. Already gone is success.
    pub(crate) async fn delete(&self, event_id: &str) -> std::result::Result<(), ApiFailure> {
        const WHAT: &str = "Google Calendar event delete";
        let response = Self::http()?
            .delete(self.event_url(event_id))
            .bearer_auth(self.token)
            .query(&[("sendUpdates", "none")])
            .send()
            .await
            .map_err(|_| ApiFailure::transport(WHAT))?;
        let status = response.status();
        if !status.is_success() && status.as_u16() != 404 && status.as_u16() != 410 {
            return Err(ApiFailure::refused(WHAT, response).await);
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
/// When step 1 cannot run, nothing is written: a 429, a 5xx, a timeout or
/// [`API_DEADLINE`] returns an error, which the write-back retries, since a
/// PUT over an event that has attendees would make Google announce their
/// removal. Only a Calendar API that cannot be used at all (not enabled in
/// the Google Cloud project, a token without the scope) writes the event
/// without attendees, and only when CalDAV shows it is new or has none.
/// Steps 1 and 3 each get half of [`API_DEADLINE`]. A failure of step 3
/// returns `providers::WrittenIncomplete`: the write-back records it as an
/// event that is there but incomplete, keeps the href so a cancellation
/// still deletes it, and its retry clears and re-adds the attendees. That
/// rewrite resets the attendees' RSVP on the Google event, silently.
pub(crate) async fn put_event_without_invitations(
    client: &crate::caldav::CaldavClient,
    api: &EventsApi<'_>,
    calendar_href: &str,
    uid: &str,
    ics: &str,
) -> Result<()> {
    let (host_only, attendees) = split_scheduling(ics);
    // Each Calendar API step gets half the deadline, so a slow lookup
    // before the write cannot leave none for the attendees after it.
    let share = api.deadline / 2;
    let cleared = tokio::time::timeout(share, async {
        let existing = api.find(uid).await?;
        if let Some(found) = existing.filter(|f| f.has_attendees) {
            api.set_attendees(&found.id, &[]).await?;
        }
        Ok::<(), ApiFailure>(())
    })
    .await;
    match cleared {
        Ok(Ok(())) => {}
        // The events collection itself is not found: the calendar was
        // deleted (a chosen write destination, #47). A refusal for good, as
        // the CalDAV PUT's 404 is, so the write-back falls back to the
        // source setting instead of retrying a calendar that is gone.
        Ok(Err(e)) if e.status == Some(404) => {
            return Err(anyhow::Error::new(crate::providers::PermanentRefusal {
                summary: "404 Not Found".to_string(),
                message: format!("{e}: the calendar is not found"),
            }));
        }
        Ok(Err(e)) if e.no_access => {
            // Without the API nothing can clear attendees, so only an event
            // that has none (or does not exist yet) may be rewritten.
            let current = client.get_event_raw(calendar_href, uid).await?;
            if current.is_some_and(|ics| lists_guests(&ics, api.calendar_id)) {
                bail!("{e}; the event has attendees, so it is left as it is");
            }
            tracing::warn!(uid = %uid, error = %e, "google calendar: Calendar API not usable, writing the event without attendees");
            return client.put_event_raw(calendar_href, uid, &host_only).await;
        }
        Ok(Err(e)) => {
            tracing::warn!(uid = %uid, error = %e, "google calendar: Calendar API failed before the write, leaving the event for the write-back retry");
            return Err(anyhow::Error::new(e).context("Google Calendar API before the write"));
        }
        Err(_) => {
            tracing::warn!(uid = %uid, "google calendar: Calendar API too slow before the write, leaving the event for the write-back retry");
            bail!(
                "Google Calendar API: no answer within {} s",
                share.as_secs()
            );
        }
    }
    client.put_event_raw(calendar_href, uid, &host_only).await?;
    if attendees.is_empty() {
        return Ok(());
    }
    // Looked up again: the PUT may have replaced the event under a new id.
    // A failure here is returned as `WrittenIncomplete`: the event is on the
    // calendar, so a cancellation must still delete it, and the write-back
    // retry clears and re-adds the attendees.
    add_attendees(api, uid, &attendees, share, false)
        .await
        .map_err(|problem| {
            tracing::warn!(uid = %uid, error = %problem, "google calendar: event written, but the guests could not be added as attendees");
            anyhow::Error::new(crate::providers::WrittenIncomplete(format!(
                "Google Calendar: event written, but the guests could not be added as attendees: {problem}"
            )))
        })
}

/// Set `attendees` on the event `uid` within `budget`, the guest alone when
/// Google refuses the whole list (one address it rejects fails all of
/// them, and the guest matters most: Meet admits listed attendees without
/// the lobby). `only_if_missing` leaves an event that lists attendees, or
/// that is not there, alone: the check on a Meet copy calrs never rewrites.
async fn add_attendees(
    api: &EventsApi<'_>,
    uid: &str,
    attendees: &[Attendee],
    budget: std::time::Duration,
    only_if_missing: bool,
) -> std::result::Result<(), String> {
    if attendees.is_empty() {
        return Ok(());
    }
    let added = tokio::time::timeout(budget, async {
        let found = if only_if_missing {
            api.find(uid).await?
        } else {
            api.find_written(uid).await?
        };
        let Some(found) = found else {
            return Ok(only_if_missing);
        };
        if only_if_missing && found.has_attendees {
            return Ok(true);
        }
        match api.set_attendees(&found.id, attendees).await {
            Err(e) if e.status == Some(400) && attendees.len() > 1 => {
                tracing::warn!(uid = %uid, error = %e, "google calendar: attendee list refused, adding the guest alone");
                api.set_attendees(&found.id, &attendees[..1])
                    .await
                    .map(|_| true)
            }
            other => other.map(|_| true),
        }
    })
    .await;
    match added {
        Ok(Ok(true)) => Ok(()),
        Ok(Ok(false)) => Err("event not visible through the Calendar API".to_string()),
        Ok(Err(e)) => Err(e.to_string()),
        Err(_) => Err(format!("no answer within {} s", budget.as_secs())),
    }
}

/// Make sure the Google event `uid` lists the attendees of `ics`, without
/// rewriting it: for the Meet copy, which a CalDAV PUT would strip of its
/// conference, when an earlier write could not add them. An event that
/// already lists attendees is left alone.
pub(crate) async fn ensure_attendees(api: &EventsApi<'_>, uid: &str, ics: &str) -> Result<()> {
    let (_, attendees) = split_scheduling(ics);
    add_attendees(api, uid, &attendees, api.deadline, true)
        .await
        .map_err(|problem| {
            anyhow::anyhow!(
                "Google Calendar: the guests could not be added as attendees: {problem}"
            )
        })
}

/// Whether an event as Google's CalDAV shows it lists anyone but its own
/// people: Google can list the organizer and the calendar's owner as
/// attendees of an event that has no guests.
fn lists_guests(ics: &str, calendar_owner: &str) -> bool {
    let unfolded = crate::utils::unfold_ical(ics);
    let organizers: Vec<String> = unfolded
        .split('\n')
        .filter_map(|raw| crate::utils::parse_content_line(raw.strip_suffix('\r').unwrap_or(raw)))
        .filter(|line| line.name.eq_ignore_ascii_case("ORGANIZER"))
        .map(|line| attendee_email(line.value).to_string())
        .collect();
    split_scheduling(ics).1.iter().any(|a| {
        !a.email.eq_ignore_ascii_case(calendar_owner)
            && !organizers.iter().any(|o| o.eq_ignore_ascii_case(&a.email))
    })
}

/// Whose copy of a booking is the one Google copy that carries its guests
/// as attendees. Google puts an event on the Google calendar of each of its
/// attendees by itself, so attendees on two copies (two collective members,
/// a claimant's copy) would show a guest the meeting twice. Every other
/// Google copy is written without attendees.
///
/// That copy is the Meet owner's when the booking has a Meet, otherwise the
/// booking's primary host's: the stored organizer, else the assigned member,
/// else the first collective member, else the event type owner. Both are the
/// same person, `google_meet::elect_meet_owner`. Of that user's Google
/// copies of the booking (one on a chosen write destination, #47, or one per
/// source otherwise), the writer picks the one on the lowest source id, the
/// source a Meet is minted on. `None` when that host writes to no Google
/// calendar: then no Google copy carries attendees.
pub(crate) async fn attendee_copy_holder(pool: &SqlitePool, booking_uid: &str) -> Option<String> {
    let booking_id: String = sqlx::query_scalar("SELECT id FROM bookings WHERE uid = ?")
        .bind(booking_uid)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten()?;
    crate::google_meet::elect_meet_owner(pool, &booking_id).await
}

/// What a Google delete does when the Calendar API cannot delete an event
/// that lists attendees.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DeleteFallback {
    /// Delete over CalDAV anyway: the final delete of a cancellation, which
    /// nothing retries. A meeting left on the host's calendar after the
    /// guest was told it is cancelled is worse than Google's own notice.
    CalDav,
    /// Return the error: a copy released because the booking moved (#47)
    /// stays recorded and is deleted again later, silently.
    Retry,
}

/// Delete a booking from a Google calendar without a cancellation from
/// Google. An event with attendees goes through the Calendar API with
/// `sendUpdates=none`; one without through CalDAV, as for any server. When
/// the API cannot be reached, cannot tell or cannot delete, `fallback`
/// decides between the CalDAV DELETE and an error.
pub(crate) async fn delete_event_without_cancellations(
    client: &crate::caldav::CaldavClient,
    api: &EventsApi<'_>,
    calendar_href: &str,
    uid: &str,
    fallback: DeleteFallback,
) -> Result<()> {
    let problem = match tokio::time::timeout(api.deadline, api.find(uid)).await {
        Ok(Ok(Some(found))) if found.has_attendees => {
            match tokio::time::timeout(api.deadline, api.delete(&found.id)).await {
                Ok(Ok(())) => return Ok(()),
                Ok(Err(e)) => format!("Calendar API delete failed: {e}"),
                Err(_) => "Calendar API delete too slow".to_string(),
            }
        }
        Ok(Ok(_)) => return client.delete_event_raw(calendar_href, uid).await,
        Ok(Err(e)) => format!("Calendar API unavailable: {e}"),
        Err(_) => "Calendar API too slow".to_string(),
    };
    match fallback {
        DeleteFallback::CalDav => {
            tracing::warn!(uid = %uid, error = %problem, "google calendar: deleting over CalDAV; Google may notify the attendees");
            client.delete_event_raw(calendar_href, uid).await
        }
        DeleteFallback::Retry => {
            tracing::warn!(uid = %uid, error = %problem, "google calendar: copy not deleted, kept for a later silent delete");
            bail!("Google Calendar: {problem}")
        }
    }
}

/// Forget when `calendar_id`'s out-of-office snapshot was last refreshed,
/// so the next sync fetches it (the calendar's events were just dropped).
pub(crate) fn forget_refresh(calendar_id: &str) {
    let suffix = format!("|{calendar_id}");
    if let Ok(mut last) = LAST_REFRESH.lock() {
        last.retain(|key, _| !key.ends_with(&suffix));
    }
}

/// Record a refresh of `calendar_id` now, as a successful fetch does.
#[cfg(test)]
pub(crate) fn mark_refreshed(api_base: &str, calendar_id: &str) {
    if let Ok(mut last) = LAST_REFRESH.lock() {
        last.insert(
            format!("{api_base}|{calendar_id}"),
            std::time::Instant::now(),
        );
    }
}

/// Whether some Calendar API base refreshed `calendar_id` in the last five
/// minutes, so the next sync would skip it.
#[cfg(test)]
pub(crate) fn refreshed_recently(calendar_id: &str) -> bool {
    let suffix = format!("|{calendar_id}");
    LAST_REFRESH.lock().is_ok_and(|last| {
        last.iter()
            .any(|(key, at)| key.ends_with(&suffix) && at.elapsed() < REFRESH_INTERVAL)
    })
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
            replace_snapshot(&pool, "c", &[whole_day, timed], Tz::UTC)
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
            Tz::UTC,
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
        replace_snapshot(&pool, "c", &[], Tz::UTC).await.unwrap();
        let remaining: Vec<String> = sqlx::query_scalar("SELECT uid FROM events ORDER BY uid")
            .fetch_all(&pool)
            .await
            .unwrap();
        assert_eq!(remaining, vec!["google-out-of-office:other", "normal"]);
    }

    /// An item that cannot be read (no dateTime, a bad one, an empty
    /// interval, no id) is skipped, not fatal: the rest of the snapshot is
    /// stored, and a deleted absence stops blocking. A repeated id once.
    #[tokio::test]
    async fn unusable_absences_are_skipped_and_repeats_stored_once() {
        let pool = database().await;
        replace_snapshot(
            &pool,
            "c",
            &[absence(
                "old",
                "2026-10-26T09:00:00Z",
                "2026-10-26T10:00:00Z",
            )],
            Tz::UTC,
        )
        .await
        .unwrap();
        let no_times: StatusEvent =
            serde_json::from_value(json!({"id": "bare", "eventType": "outOfOffice"})).unwrap();
        let events = [
            absence("bad", "invalid", "2026-10-26T10:00:00Z"),
            absence("empty", "2026-10-26T10:00:00Z", "2026-10-26T10:00:00Z"),
            absence("", "2026-10-26T09:00:00Z", "2026-10-26T10:00:00Z"),
            no_times,
            absence("good", "2026-10-27T09:00:00Z", "2026-10-27T10:00:00Z"),
            absence("good", "2026-10-27T09:00:00Z", "2026-10-27T10:00:00Z"),
        ];
        assert_eq!(
            replace_snapshot(&pool, "c", &events, Tz::UTC)
                .await
                .unwrap(),
            1
        );
        let uids: Vec<String> = sqlx::query_scalar("SELECT uid FROM events")
            .fetch_all(&pool)
            .await
            .unwrap();
        assert_eq!(uids, vec!["google-out-of-office:good"]);
    }

    /// A whole-day absence (`date`, end exclusive) blocks its days in the
    /// calendar's timezone, including a 25-hour day at the end of DST.
    #[tokio::test]
    async fn whole_day_absences_block_their_days_in_the_calendar_timezone() {
        let pool = database().await;
        let page: EventPage = serde_json::from_value(json!({
            "timeZone": "Europe/Paris",
            "items": [{
                "id": "allday", "eventType": "outOfOffice", "status": "confirmed",
                "start": {"date": "2026-10-25"}, "end": {"date": "2026-10-26"}
            }]
        }))
        .unwrap();
        let zone: Tz = page.time_zone.as_deref().unwrap().parse().unwrap();
        assert_eq!(
            replace_snapshot(&pool, "c", &page.items, zone)
                .await
                .unwrap(),
            1
        );
        let (start, end): (String, String) =
            sqlx::query_as("SELECT start_at, end_at FROM events WHERE calendar_id = 'c'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(start, "20261024T220000Z");
        assert_eq!(end, "20261025T230000Z");
    }

    #[test]
    fn query_window_is_whole_seconds_in_utc() {
        let now = Utc.with_ymd_and_hms(2026, 11, 7, 12, 30, 45).unwrap()
            + Duration::nanoseconds(123_456_789);
        let (time_min, time_max) = query_window(now);
        assert_eq!(time_min, "2026-08-09T12:30:45Z");
        assert_eq!(time_max, "2027-11-08T12:30:45Z");
    }

    /// The refresh throttle is process-wide and keyed by API base and
    /// calendar id. These tests all use calendar `c` on fake servers whose
    /// ports the OS hands out again once a server is dropped, so one test's
    /// successful refresh could read as "fresh" in another, or
    /// `forget_refresh("c")` clear another's entry. They take this lock and
    /// start from an empty throttle for `c`.
    static REFRESH_TESTS: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

    /// A failed out-of-office fetch is reported on the source, so sync
    /// health and the owner's alert see it, and a snapshot that has not
    /// been refreshed for a week stops blocking. A Calendar API that is
    /// simply not enabled, with nothing cached, is no failure.
    #[tokio::test]
    async fn out_of_office_failures_are_reported_and_stale_blocks_expire() {
        let _serial = REFRESH_TESTS.lock().await;
        forget_refresh("c");
        const PRIMARY: &str = "http://x/caldav/v2/host%40example.com/user";
        const CAL_HREF: &str = "/caldav/v2/host%40example.com/events";
        async fn cached(pool: &SqlitePool) -> i64 {
            sqlx::query_scalar("SELECT COUNT(*) FROM events WHERE google_out_of_office = 1")
                .fetch_one(pool)
                .await
                .unwrap()
        }
        let pool = database().await;

        // Not enabled, nothing cached: not a failure.
        let fake = std::sync::Arc::new(FakeGoogle::without_api());
        let (base, server) = super::fake::serve(fake).await;
        let api = format!("{base}/api");
        assert_eq!(
            refresh_out_of_office(&pool, "c", PRIMARY, CAL_HREF, &api, "t", REFRESH_BUDGET).await,
            None
        );
        server.abort();

        // A fresh snapshot survives a failed fetch, which is reported.
        replace_snapshot(
            &pool,
            "c",
            &[absence("a", "2026-10-26T09:00:00Z", "2026-10-26T10:00:00Z")],
            Tz::UTC,
        )
        .await
        .unwrap();
        for fake in [FakeGoogle::api_failing(503), FakeGoogle::without_api()] {
            let (base, server) = super::fake::serve(std::sync::Arc::new(fake)).await;
            let api = format!("{base}/api");
            forget_refresh("c");
            let failure =
                refresh_out_of_office(&pool, "c", PRIMARY, CAL_HREF, &api, "t", REFRESH_BUDGET)
                    .await
                    .unwrap();
            assert!(failure.starts_with("Google out-of-office"), "{failure}");
            assert!(!failure.contains("Backend Error"), "{failure}");
            server.abort();
            assert_eq!(cached(&pool).await, 1);
        }

        // A week without a successful fetch: the blocks are dropped.
        sqlx::query("UPDATE events SET synced_at = datetime('now', '-8 days')")
            .execute(&pool)
            .await
            .unwrap();
        let (base, server) =
            super::fake::serve(std::sync::Arc::new(FakeGoogle::api_failing(503))).await;
        let api = format!("{base}/api");
        assert!(
            refresh_out_of_office(&pool, "c", PRIMARY, CAL_HREF, &api, "t", REFRESH_BUDGET)
                .await
                .is_some()
        );
        assert_eq!(cached(&pool).await, 0);

        // Success: nothing to report.
        server.abort();
        let (base, server) = super::fake::serve(std::sync::Arc::new(FakeGoogle::default())).await;
        let api = format!("{base}/api");
        assert_eq!(
            refresh_out_of_office(&pool, "c", PRIMARY, CAL_HREF, &api, "t", REFRESH_BUDGET).await,
            None
        );
        server.abort();
    }

    /// A snapshot refreshed in the last five minutes is not fetched again,
    /// and a refresh that does not answer within its budget is a failure
    /// (tried again on the next sync) instead of holding a slot page.
    #[tokio::test]
    async fn out_of_office_refreshes_are_spaced_and_bounded() {
        let _serial = REFRESH_TESTS.lock().await;
        forget_refresh("c");
        const PRIMARY: &str = "http://x/caldav/v2/host%40example.com/user";
        const CAL_HREF: &str = "/caldav/v2/host%40example.com/events";
        let pool = database().await;
        let fake = std::sync::Arc::new(FakeGoogle::default());
        let (base, server) = super::fake::serve(fake.clone()).await;
        let api = format!("{base}/api");
        for _ in 0..2 {
            assert_eq!(
                refresh_out_of_office(&pool, "c", PRIMARY, CAL_HREF, &api, "t", REFRESH_BUDGET)
                    .await,
                None
            );
        }
        server.abort();
        let fetches = fake
            .log()
            .iter()
            .filter(|e| e.starts_with("GET /api/"))
            .count();
        assert_eq!(fetches, 1);

        let slow = std::sync::Arc::new(FakeGoogle::slow(std::time::Duration::from_secs(5)));
        let (base, server) = super::fake::serve(slow).await;
        let api = format!("{base}/api");
        let started = std::time::Instant::now();
        let failure = refresh_out_of_office(
            &pool,
            "c",
            PRIMARY,
            CAL_HREF,
            &api,
            "t",
            std::time::Duration::from_millis(200),
        )
        .await
        .unwrap();
        server.abort();
        assert!(started.elapsed() < std::time::Duration::from_secs(3));
        assert!(failure.contains("no answer within"), "{failure}");
    }

    /// A calendar whose events were dropped is fetched again on the next
    /// sync, even within five minutes of the last refresh.
    #[tokio::test]
    async fn a_forgotten_calendar_refreshes_again() {
        let _serial = REFRESH_TESTS.lock().await;
        forget_refresh("c");
        const PRIMARY: &str = "http://x/caldav/v2/host%40example.com/user";
        const CAL_HREF: &str = "/caldav/v2/host%40example.com/events";
        let pool = database().await;
        let fake = std::sync::Arc::new(FakeGoogle::default());
        let (base, server) = super::fake::serve(fake.clone()).await;
        let api = format!("{base}/api");
        let refresh =
            || refresh_out_of_office(&pool, "c", PRIMARY, CAL_HREF, &api, "t", REFRESH_BUDGET);
        assert_eq!(refresh().await, None);
        forget_refresh("c");
        assert_eq!(refresh().await, None);
        server.abort();
        let fetches = fake
            .log()
            .iter()
            .filter(|e| e.starts_with("GET /api/"))
            .count();
        assert_eq!(fetches, 2);
    }

    #[tokio::test]
    async fn cancelled_absences_and_working_locations_do_not_block() {
        let pool = database().await;
        let mut cancelled = absence("cancelled", "2026-10-26T09:00:00Z", "2026-10-26T10:00:00Z");
        cancelled.status = Some("cancelled".into());
        let mut location = absence("location", "2026-10-26T09:00:00Z", "2026-10-26T10:00:00Z");
        location.event_type = "workingLocation".into();
        assert_eq!(
            replace_snapshot(&pool, "c", &[cancelled, location], Tz::UTC)
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
            Tz::UTC,
        )
        .await
        .unwrap();
        let count = sync_out_of_office(
            &pool,
            "other",
            "https://apidata.googleusercontent.com/caldav/v2/host%40example.com/user",
            "https://apidata.googleusercontent.com/caldav/v2/en.usa%23holiday%40group.v.calendar.google.com/events",
            "http://127.0.0.1:9",
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
        let (events, _) = fetch_out_of_office(
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

    use super::fake::{assert_nothing_announced, FakeGoogle};

    const HREF: &str = "/caldav/v2/host%40example.com/events";
    const CAL: &str = "host@example.com";

    async fn serve_fake(
        fake: std::sync::Arc<FakeGoogle>,
    ) -> (crate::caldav::CaldavClient, tokio::task::JoinHandle<()>) {
        let (base, server) = super::fake::serve(fake).await;
        (
            crate::caldav::CaldavClient::with_bearer(&base, "test-token"),
            server,
        )
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
        fake.log()
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
        assert_eq!(
            fake.attendees(CAL, "new@calrs").unwrap(),
            vec![
                json!({"email": "jane@example.com", "displayName": "Doe, Jane"}),
                json!({"email": "bob@example.com"}),
            ]
        );
    }

    #[tokio::test]
    async fn rewriting_a_google_event_clears_its_attendees_before_the_put() {
        let fake = std::sync::Arc::new(FakeGoogle::default());
        fake.insert(
            CAL,
            "moved@calrs",
            "ev-old",
            vec![json!({"email": "jane@example.com"})],
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
        assert_eq!(fake.attendees(CAL, "moved@calrs").unwrap().len(), 2);
    }

    #[tokio::test]
    async fn google_event_is_still_written_host_only_without_the_calendar_api() {
        let fake = std::sync::Arc::new(FakeGoogle::without_api());
        let (client, server) = serve_fake(fake.clone()).await;
        client
            .put_event(HREF, "noapi@calrs", &booking_ics("noapi@calrs"))
            .await
            .unwrap();
        server.abort();
        let log = log(&fake);
        assert_nothing_announced(&log);
        assert!(fake.attendees(CAL, "noapi@calrs").is_some());
        assert!(!log.iter().any(|e| e.starts_with("PATCH ")));
    }

    #[tokio::test]
    async fn google_event_with_attendees_is_deleted_silently_through_the_api() {
        let fake = std::sync::Arc::new(FakeGoogle::default());
        fake.insert(
            CAL,
            "guests@calrs",
            "ev1",
            vec![json!({"email": "jane@example.com"})],
        );
        fake.insert(CAL, "alone@calrs", "ev2", Vec::new());
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
        assert!(fake.is_empty());
    }

    #[tokio::test]
    async fn google_delete_falls_back_to_caldav_without_the_calendar_api() {
        let fake = std::sync::Arc::new(FakeGoogle::without_api());
        fake.insert(
            CAL,
            "gone@calrs",
            "ev1",
            vec![json!({"email": "jane@example.com"})],
        );
        let (client, server) = serve_fake(fake.clone()).await;
        client.delete_event(HREF, "gone@calrs").await.unwrap();
        server.abort();
        assert!(fake.is_empty());
    }

    #[test]
    fn only_a_disabled_api_or_a_missing_scope_reads_as_no_access() {
        assert!(says_no_access(super::fake::ACCESS_NOT_CONFIGURED));
        assert!(says_no_access(
            r#"{"error": {"code": 403, "errors": [{"reason": "insufficientPermissions"}]}}"#
        ));
        assert!(says_no_access(
            r#"{"error": {"details": [{"reason": "ACCESS_TOKEN_SCOPE_INSUFFICIENT"}]}}"#
        ));
        for body in [
            r#"{"error": {"code": 403, "errors": [{"reason": "rateLimitExceeded"}]}}"#,
            r#"{"error": {"message": "accessNotConfigured"}}"#,
            "",
            "<html>Forbidden</html>",
        ] {
            assert!(!says_no_access(body), "{body}");
        }
    }

    /// A 429, a 5xx: nothing is written, since a PUT over an event with
    /// attendees would make Google announce their removal. The error goes
    /// back to the write-back, which retries.
    #[tokio::test]
    async fn a_transient_api_failure_writes_nothing() {
        for status in [429, 500, 503] {
            let fake = std::sync::Arc::new(FakeGoogle::api_failing(status));
            fake.insert(
                CAL,
                "moved@calrs",
                "ev1",
                vec![json!({"email": "jane@example.com"})],
            );
            let (client, server) = serve_fake(fake.clone()).await;
            for uid in ["moved@calrs", "new@calrs"] {
                let error = client
                    .put_event(HREF, uid, &booking_ics(uid))
                    .await
                    .unwrap_err();
                assert!(
                    format!("{error:#}").contains(&format!("HTTP {status}")),
                    "{error:#}"
                );
            }
            server.abort();
            let log = log(&fake);
            assert!(!log.iter().any(|e| e.starts_with("PUT ")), "{log:#?}");
            assert_eq!(fake.attendees(CAL, "moved@calrs").unwrap().len(), 1);
            assert!(fake.attendees(CAL, "new@calrs").is_none());
        }
    }

    /// A Calendar API 403 that is no "API not usable" refusal (a rate
    /// limit) is transient: the write fails for the retry, never as a
    /// refusal for good that would send the booking to another calendar.
    #[tokio::test]
    async fn a_transient_api_403_is_no_permanent_refusal() {
        let fake = std::sync::Arc::new(FakeGoogle::api_failing(403));
        let (client, server) = serve_fake(fake.clone()).await;
        let provider = crate::providers::caldav::CaldavProvider::from_client(client);
        let error = crate::providers::put_and_verify(
            &provider,
            HREF,
            "busy@calrs",
            &booking_ics("busy@calrs"),
        )
        .await
        .unwrap_err();
        server.abort();
        assert_eq!(error.permanent_refusal(), None, "{error}");
        assert!(!error.event_may_exist());
    }

    /// A Google calendar that is gone (a deleted write destination) is a
    /// refusal for good, as a CalDAV 404 is, so the write-back falls back
    /// to the source setting instead of retrying it.
    #[tokio::test]
    async fn a_deleted_google_calendar_is_a_permanent_refusal() {
        let fake = std::sync::Arc::new(FakeGoogle::default());
        fake.remove_calendar(CAL);
        let (client, server) = serve_fake(fake.clone()).await;
        let provider = crate::providers::caldav::CaldavProvider::from_client(client);
        let error = crate::providers::put_and_verify(
            &provider,
            HREF,
            "gone@calrs",
            &booking_ics("gone@calrs"),
        )
        .await
        .unwrap_err();
        server.abort();
        assert_eq!(error.permanent_refusal(), Some("404 Not Found"), "{error}");
    }

    /// A release (#47) never falls back to the CalDAV delete that would
    /// have Google mail a cancellation: the error is returned, for a later
    /// quiet try. A cancellation's final delete still falls back.
    #[tokio::test]
    async fn a_release_keeps_a_copy_the_api_cannot_delete() {
        let fake = std::sync::Arc::new(FakeGoogle::delete_failing(503));
        fake.insert(
            CAL,
            "moved@calrs",
            "ev1",
            vec![json!({"email": "jane@example.com"})],
        );
        let (client, server) = serve_fake(fake.clone()).await;
        assert!(client.release_event(HREF, "moved@calrs").await.is_err());
        assert!(!log(&fake).iter().any(|e| e.starts_with("DELETE /caldav/")));
        assert!(fake.attendees(CAL, "moved@calrs").is_some());
        client.delete_event(HREF, "moved@calrs").await.unwrap();
        server.abort();
        assert!(fake.attendees(CAL, "moved@calrs").is_none());
    }

    /// Without the Calendar API, an event that already has attendees (the
    /// API was disabled after it was written) is left alone: nothing could
    /// clear them silently. One without attendees is rewritten.
    #[tokio::test]
    async fn without_the_api_an_event_with_attendees_is_not_rewritten() {
        let fake = std::sync::Arc::new(FakeGoogle::without_api());
        fake.insert(
            CAL,
            "guests@calrs",
            "ev1",
            vec![json!({"email": "jane@example.com"})],
        );
        fake.insert(CAL, "alone@calrs", "ev2", Vec::new());
        let (client, server) = serve_fake(fake.clone()).await;
        assert!(client
            .put_event(HREF, "guests@calrs", &booking_ics("guests@calrs"))
            .await
            .is_err());
        client
            .put_event(HREF, "alone@calrs", &booking_ics("alone@calrs"))
            .await
            .unwrap();
        server.abort();
        let log = log(&fake);
        assert_nothing_announced(&log);
        assert!(!log
            .iter()
            .any(|e| e.starts_with("PUT ") && e.contains("guests@calrs")));
        assert!(log
            .iter()
            .any(|e| e.starts_with("PUT ") && e.contains("alone@calrs")));
        assert_eq!(fake.attendees(CAL, "guests@calrs").unwrap().len(), 1);
    }

    /// The Calendar API part of a write has one deadline: past it, the
    /// write gives up before its PUT instead of holding the booking request.
    #[tokio::test]
    async fn a_slow_calendar_api_gives_up_before_the_write() {
        let fake = std::sync::Arc::new(FakeGoogle::slow(std::time::Duration::from_secs(5)));
        let (base, server) = super::fake::serve(fake.clone()).await;
        let client = crate::caldav::CaldavClient::with_bearer(&base, "test-token");
        let api_base = format!("{base}/api");
        let mut api = EventsApi::new(&api_base, "test-token", CAL);
        api.deadline = std::time::Duration::from_millis(200);
        let started = std::time::Instant::now();
        let error = put_event_without_invitations(
            &client,
            &api,
            HREF,
            "slow@calrs",
            &booking_ics("slow@calrs"),
        )
        .await
        .unwrap_err();
        assert!(started.elapsed() < std::time::Duration::from_secs(3));
        assert!(error.to_string().contains("no answer within"), "{error}");
        server.abort();
        assert!(!log(&fake).iter().any(|e| e.starts_with("PUT ")));
    }

    /// A Calendar API delete that fails falls back to CalDAV: nothing
    /// retries a delete, and the cancelled meeting must not stay on the
    /// host's calendar.
    #[tokio::test]
    async fn a_failed_api_delete_still_deletes_the_event() {
        let fake = std::sync::Arc::new(FakeGoogle::delete_failing(503));
        fake.insert(
            CAL,
            "guests@calrs",
            "ev1",
            vec![json!({"email": "jane@example.com"})],
        );
        let (client, server) = serve_fake(fake.clone()).await;
        client.delete_event(HREF, "guests@calrs").await.unwrap();
        server.abort();
        let log = log(&fake);
        assert!(log
            .iter()
            .any(|e| e.starts_with("DELETE /api/calendars/host%40example.com/events/ev1?")));
        assert!(
            log.iter()
                .any(|e| e
                    .starts_with("DELETE /caldav/v2/host%40example.com/events/guests@calrs.ics"))
        );
        assert!(fake.is_empty());
    }

    /// A Google collection whose href carries no calendar id cannot use the
    /// Calendar API: the event is written without attendees rather than
    /// with ATTENDEE lines Google would mail.
    #[tokio::test]
    async fn a_google_href_without_a_calendar_id_gets_no_attendees() {
        let fake = std::sync::Arc::new(FakeGoogle::default());
        let (client, server) = serve_fake(fake.clone()).await;
        client
            .put_event("/dav/team", "odd@calrs", &booking_ics("odd@calrs"))
            .await
            .unwrap();
        server.abort();
        let log = log(&fake);
        assert_eq!(log.len(), 1, "{log:#?}");
        assert!(
            log[0].starts_with("PUT /dav/team/odd@calrs.ics"),
            "{}",
            log[0]
        );
        assert_nothing_announced(&log);
        assert_eq!(fake.attendees("/dav/team", "odd@calrs"), Some(Vec::new()));
    }

    /// Clearing worked and the PUT landed, but adding the guests back
    /// failed: the write reports an incomplete event (so the write-back
    /// keeps the href and retries), and the retry restores them.
    #[tokio::test]
    async fn a_failed_attendee_add_is_reported_and_the_retry_restores_them() {
        let fake = std::sync::Arc::new(FakeGoogle::failing_adds(1));
        fake.insert(
            CAL,
            "moved@calrs",
            "ev1",
            vec![json!({"email": "jane@example.com"})],
        );
        let (client, server) = serve_fake(fake.clone()).await;
        let error = client
            .put_event(HREF, "moved@calrs", &booking_ics("moved@calrs"))
            .await
            .unwrap_err();
        assert!(error
            .downcast_ref::<crate::providers::WrittenIncomplete>()
            .is_some());
        assert_eq!(fake.attendees(CAL, "moved@calrs"), Some(Vec::new()));
        client
            .put_event(HREF, "moved@calrs", &booking_ics("moved@calrs"))
            .await
            .unwrap();
        server.abort();
        assert_nothing_announced(&log(&fake));
        assert_eq!(fake.attendees(CAL, "moved@calrs").unwrap().len(), 2);
    }

    /// A Calendar API delete that does not answer within the deadline falls
    /// back to CalDAV like a failed one.
    #[tokio::test]
    async fn a_slow_api_delete_falls_back_to_caldav() {
        let fake = std::sync::Arc::new(FakeGoogle::slow_deletes(std::time::Duration::from_secs(5)));
        fake.insert(
            CAL,
            "guests@calrs",
            "ev1",
            vec![json!({"email": "jane@example.com"})],
        );
        let (base, server) = super::fake::serve(fake.clone()).await;
        let client = crate::caldav::CaldavClient::with_bearer(&base, "test-token");
        let api_base = format!("{base}/api");
        let mut api = EventsApi::new(&api_base, "test-token", CAL);
        api.deadline = std::time::Duration::from_millis(300);
        let started = std::time::Instant::now();
        delete_event_without_cancellations(
            &client,
            &api,
            HREF,
            "guests@calrs",
            DeleteFallback::CalDav,
        )
        .await
        .unwrap();
        assert!(started.elapsed() < std::time::Duration::from_secs(3));
        server.abort();
        assert!(
            log(&fake)
                .iter()
                .any(|e| e
                    .starts_with("DELETE /caldav/v2/host%40example.com/events/guests@calrs.ics"))
        );
    }

    /// Without the API, an event that lists only the calendar's owner or
    /// its organizer has no guests to lose, and is rewritten.
    #[tokio::test]
    async fn without_the_api_the_owner_alone_is_no_guest() {
        let fake = std::sync::Arc::new(FakeGoogle::without_api());
        fake.insert(
            CAL,
            "owner@calrs",
            "ev1",
            vec![json!({"email": "Host@Example.com"})],
        );
        let (client, server) = serve_fake(fake.clone()).await;
        client
            .put_event(HREF, "owner@calrs", &booking_ics("owner@calrs"))
            .await
            .unwrap();
        server.abort();
        assert!(log(&fake)
            .iter()
            .any(|e| e.starts_with("PUT ") && e.contains("owner@calrs")));
        let ics = "BEGIN:VEVENT\r\nORGANIZER:mailto:boss@example.com\r\nATTENDEE:mailto:boss@example.com\r\nATTENDEE:mailto:host@example.com\r\nEND:VEVENT\r\n";
        assert!(!lists_guests(ics, CAL));
        let with_guest = ics.replace(
            "END:VEVENT",
            "ATTENDEE:mailto:jane@example.com\r\nEND:VEVENT",
        );
        assert!(lists_guests(&with_guest, CAL));
    }

    /// One address Google refuses does not cost the guest their place on
    /// the event: they are added alone.
    #[tokio::test]
    async fn a_refused_attendee_list_still_lists_the_guest() {
        let fake = std::sync::Arc::new(FakeGoogle::rejecting("bob@example.com"));
        let (client, server) = serve_fake(fake.clone()).await;
        client
            .put_event(HREF, "picky@calrs", &booking_ics("picky@calrs"))
            .await
            .unwrap();
        server.abort();
        assert_nothing_announced(&log(&fake));
        assert_eq!(
            fake.attendees(CAL, "picky@calrs").unwrap(),
            vec![json!({"email": "jane@example.com", "displayName": "Doe, Jane"})]
        );
    }

    #[test]
    fn attendee_addresses_never_split_a_character() {
        assert_eq!(attendee_email("MAILTO:x@example.com"), "x@example.com");
        assert_eq!(attendee_email("abc\u{2014}def"), "abc\u{2014}def");
        assert_eq!(
            attendee_email("j\u{fc}rgen@example.com"),
            "j\u{fc}rgen@example.com"
        );
        let ics = "BEGIN:VEVENT\r\nSUMMARY:Intro \u{2014} call\r\nATTENDEE:mailto:Carol@example.com\r\nATTENDEE:mailto:jane@example.com\r\nEND:VEVENT\r\n";
        assert_eq!(
            without_attendees_named(ics, &["carol@example.com".to_string()]),
            "BEGIN:VEVENT\r\nSUMMARY:Intro \u{2014} call\r\nATTENDEE:mailto:jane@example.com\r\nEND:VEVENT\r\n"
        );
    }

    #[test]
    fn caret_escapes_decode_in_one_pass() {
        assert_eq!(decode_param_value("a^^nb"), "a^nb");
        assert_eq!(decode_param_value("a^nb"), "a b");
        assert_eq!(decode_param_value("^'Q^'"), "\"Q\"");
        assert_eq!(decode_param_value("^^^'"), "^\"");
        assert_eq!(decode_param_value("a^xb^"), "a^xb^");
    }

    /// Any other CalDAV server keeps the ATTENDEE lines (with
    /// SCHEDULE-AGENT=CLIENT) and sees no Calendar API call.
    #[tokio::test]
    async fn other_caldav_servers_keep_attendees_in_the_ics() {
        let fake = std::sync::Arc::new(FakeGoogle::default());
        let (base, server) = super::fake::serve_plain(fake.clone()).await;
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

/// A fake Google for tests: CalDAV under `/caldav/v2/{calendar}/events/`,
/// the Calendar API under `/api/calendars/{calendar}/events`, any number of
/// calendars. Every request is logged as "METHOD path?query body".
#[cfg(test)]
pub(crate) mod fake {
    use axum::{response::IntoResponse, Json, Router};
    use serde_json::{json, Value};
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::{Arc, Mutex};

    type Key = (String, String);

    #[derive(Default)]
    pub(crate) struct FakeGoogle {
        log: Mutex<Vec<String>>,
        /// (calendar id, iCalUID) -> (event id, attendees as the API holds them).
        events: Mutex<HashMap<Key, (String, Vec<Value>)>>,
        next_id: AtomicU32,
        /// Every Calendar API call answers this status and body.
        api_failure: Option<(u16, &'static str)>,
        /// Every Calendar API call waits this long first.
        api_delay: Option<std::time::Duration>,
        /// Calendar API deletes (only) answer this status (0: they work).
        api_delete_failure: std::sync::atomic::AtomicU16,
        /// Calendars that do not exist: every request on them answers 404.
        missing: Mutex<Vec<String>>,
        /// An attendee update naming this address answers 400.
        rejected_attendee: Option<&'static str>,
        /// This many updates that add attendees answer 503 first.
        failing_adds: AtomicU32,
        /// Calendar API deletes (only) wait this long first.
        delete_delay: Option<std::time::Duration>,
    }

    /// Decrements `counter` if it is above zero, saying whether it did: one
    /// more injected failure to hand out. A compare-exchange loop rather than
    /// `fetch_update`, which newer toolchains deprecate under another name.
    fn take_one(counter: &AtomicU32) -> bool {
        let mut n = counter.load(Ordering::SeqCst);
        while n > 0 {
            match counter.compare_exchange(n, n - 1, Ordering::SeqCst, Ordering::SeqCst) {
                Ok(_) => return true,
                Err(current) => n = current,
            }
        }
        false
    }

    /// Google's answer when the Calendar API is not enabled in the project.
    pub(crate) const ACCESS_NOT_CONFIGURED: &str = r#"{"error": {"code": 403, "message": "Google Calendar API has not been used in project 123 before or it is disabled.", "errors": [{"domain": "usageLimits", "reason": "accessNotConfigured"}], "status": "PERMISSION_DENIED", "details": [{"@type": "type.googleapis.com/google.rpc.ErrorInfo", "reason": "SERVICE_DISABLED"}]}}"#;

    impl FakeGoogle {
        /// A Google whose Calendar API is not enabled.
        pub(crate) fn without_api() -> Self {
            Self {
                api_failure: Some((403, ACCESS_NOT_CONFIGURED)),
                ..Default::default()
            }
        }

        /// A Google whose Calendar API takes `delay` to answer anything.
        pub(crate) fn slow(delay: std::time::Duration) -> Self {
            Self {
                api_delay: Some(delay),
                ..Default::default()
            }
        }

        /// A Google whose Calendar API deletes take `delay`.
        pub(crate) fn slow_deletes(delay: std::time::Duration) -> Self {
            Self {
                delete_delay: Some(delay),
                ..Default::default()
            }
        }

        /// A Google whose next `n` attendee additions fail with a 503.
        pub(crate) fn failing_adds(n: u32) -> Self {
            Self {
                failing_adds: AtomicU32::new(n),
                ..Default::default()
            }
        }

        /// A Google that refuses any attendee list naming `email`.
        pub(crate) fn rejecting(email: &'static str) -> Self {
            Self {
                rejected_attendee: Some(email),
                ..Default::default()
            }
        }

        /// A Google whose Calendar API answers `status` to every delete.
        pub(crate) fn delete_failing(status: u16) -> Self {
            let fake = Self::default();
            fake.fail_deletes(Some(status));
            fake
        }

        /// From now on, Calendar API deletes answer `status`, or work.
        pub(crate) fn fail_deletes(&self, status: Option<u16>) {
            self.api_delete_failure
                .store(status.unwrap_or(0), Ordering::SeqCst);
        }

        /// From now on, `calendar` does not exist.
        pub(crate) fn remove_calendar(&self, calendar: &str) {
            self.missing.lock().unwrap().push(calendar.to_string());
        }

        /// A Google whose Calendar API answers `status` to every call.
        pub(crate) fn api_failing(status: u16) -> Self {
            Self {
                api_failure: Some((status, r#"{"error": {"message": "Backend Error"}}"#)),
                ..Default::default()
            }
        }

        pub(crate) fn log(&self) -> Vec<String> {
            self.log.lock().unwrap().clone()
        }

        /// An event already on `calendar`, as an earlier write left it.
        pub(crate) fn insert(&self, calendar: &str, uid: &str, id: &str, attendees: Vec<Value>) {
            self.events.lock().unwrap().insert(
                (calendar.to_string(), uid.to_string()),
                (id.to_string(), attendees),
            );
        }

        /// The attendees of the event, `None` when it is not on `calendar`.
        pub(crate) fn attendees(&self, calendar: &str, uid: &str) -> Option<Vec<Value>> {
            self.events
                .lock()
                .unwrap()
                .get(&(calendar.to_string(), uid.to_string()))
                .map(|(_, attendees)| attendees.clone())
        }

        pub(crate) fn is_empty(&self) -> bool {
            self.events.lock().unwrap().is_empty()
        }
    }

    fn decode(segment: &str) -> String {
        urlencoding::decode(segment)
            .map(|s| s.into_owned())
            .unwrap_or_else(|_| segment.to_string())
    }

    async fn handle(
        axum::extract::State(fake): axum::extract::State<Arc<FakeGoogle>>,
        method: axum::http::Method,
        uri: axum::http::Uri,
        body: String,
    ) -> axum::response::Response {
        use axum::http::StatusCode;
        let path = uri.path().to_string();
        let query = uri.query().unwrap_or("").to_string();
        fake.log
            .lock()
            .unwrap()
            .push(format!("{method} {path}?{query} {body}"));
        if !path.starts_with("/api/") {
            // CalDAV. A `/caldav/v2/{calendar}/events/` collection is keyed
            // by its calendar id, any other collection by its path.
            let Some((collection, file)) = path.rsplit_once('/') else {
                return StatusCode::NOT_FOUND.into_response();
            };
            let calendar = collection
                .strip_prefix("/caldav/v2/")
                .and_then(|rest| rest.strip_suffix("/events"))
                .map(decode)
                .unwrap_or_else(|| collection.to_string());
            if fake.missing.lock().unwrap().contains(&calendar) {
                return StatusCode::NOT_FOUND.into_response();
            }
            let key = (calendar, decode(file.trim_end_matches(".ics")));
            let mut events = fake.events.lock().unwrap();
            return match method.as_str() {
                "PUT" => {
                    // A CalDAV PUT replaces the event, attendees included.
                    let id = events.get(&key).map(|e| e.0.clone()).unwrap_or_else(|| {
                        format!("ev{}", fake.next_id.fetch_add(1, Ordering::SeqCst))
                    });
                    events.insert(key, (id, Vec::new()));
                    StatusCode::CREATED.into_response()
                }
                "GET" => match events.get(&key) {
                    // What Google's CalDAV shows: the attendees as ATTENDEE lines.
                    Some((_, attendees)) => {
                        let mut ics =
                            format!("BEGIN:VCALENDAR\r\nBEGIN:VEVENT\r\nUID:{}\r\n", key.1);
                        for a in attendees {
                            ics.push_str(&format!(
                                "ATTENDEE:mailto:{}\r\n",
                                a["email"].as_str().unwrap_or("")
                            ));
                        }
                        ics.push_str("END:VEVENT\r\nEND:VCALENDAR\r\n");
                        ics.into_response()
                    }
                    None => StatusCode::NOT_FOUND.into_response(),
                },
                "HEAD" if events.contains_key(&key) => StatusCode::OK.into_response(),
                "HEAD" => StatusCode::NOT_FOUND.into_response(),
                "DELETE" => {
                    events.remove(&key);
                    StatusCode::NO_CONTENT.into_response()
                }
                _ => StatusCode::METHOD_NOT_ALLOWED.into_response(),
            };
        }
        if let Some(delay) = fake.api_delay {
            tokio::time::sleep(delay).await;
        }
        if let (Some(delay), "DELETE") = (fake.delete_delay, method.as_str()) {
            tokio::time::sleep(delay).await;
        }
        if let Some((status, body)) = fake.api_failure {
            return (StatusCode::from_u16(status).unwrap(), body).into_response();
        }
        let Some(rest) = path.strip_prefix("/api/calendars/") else {
            return StatusCode::NOT_FOUND.into_response();
        };
        let Some((calendar, rest)) = rest.split_once("/events") else {
            return StatusCode::NOT_FOUND.into_response();
        };
        let calendar = decode(calendar);
        if fake.missing.lock().unwrap().contains(&calendar) {
            return StatusCode::NOT_FOUND.into_response();
        }
        let mut events = fake.events.lock().unwrap();
        if rest.is_empty() && method == axum::http::Method::GET {
            let uid = query
                .split('&')
                .find_map(|kv| kv.strip_prefix("iCalUID="))
                .map(decode)
                .unwrap_or_default();
            let items: Vec<_> = events
                .get(&(calendar, uid))
                .map(|(id, attendees)| json!({"id": id, "attendees": attendees}))
                .into_iter()
                .collect();
            return Json(json!({ "items": items })).into_response();
        }
        let id = decode(rest.trim_start_matches('/'));
        let Some(key) = events
            .iter()
            .find(|((cal, _), (eid, _))| *cal == calendar && *eid == id)
            .map(|(key, _)| key.clone())
        else {
            return StatusCode::NOT_FOUND.into_response();
        };
        match method.as_str() {
            "PATCH" => {
                if fake
                    .rejected_attendee
                    .is_some_and(|email| body.contains(email))
                {
                    return StatusCode::BAD_REQUEST.into_response();
                }
                if body.contains("\"email\"") && take_one(&fake.failing_adds) {
                    return StatusCode::SERVICE_UNAVAILABLE.into_response();
                }
                let patch: Value = serde_json::from_str(&body).unwrap();
                let attendees = patch["attendees"].as_array().cloned().unwrap_or_default();
                events.get_mut(&key).unwrap().1 = attendees;
                Json(json!({ "id": id })).into_response()
            }
            "DELETE" => {
                let failure = fake.api_delete_failure.load(Ordering::SeqCst);
                if failure != 0 {
                    return StatusCode::from_u16(failure).unwrap().into_response();
                }
                events.remove(&key);
                StatusCode::NO_CONTENT.into_response()
            }
            _ => StatusCode::METHOD_NOT_ALLOWED.into_response(),
        }
    }

    /// Serve `fake` on loopback without telling `CaldavClient` it is Google:
    /// what any other CalDAV server sees. Returns its base URL.
    pub(crate) async fn serve_plain(
        fake: Arc<FakeGoogle>,
    ) -> (String, tokio::task::JoinHandle<()>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let app = Router::new().fallback(handle).with_state(fake);
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        (base, server)
    }

    /// Serve `fake` as Google: every `CaldavClient` built for the returned
    /// base URL takes the Calendar API detour, against the fake's `/api`.
    pub(crate) async fn serve(fake: Arc<FakeGoogle>) -> (String, tokio::task::JoinHandle<()>) {
        let (base, server) = serve_plain(fake).await;
        crate::caldav::CaldavClient::treat_as_google(&base, &format!("{base}/api"));
        (base, server)
    }

    /// Every Calendar API write carries `sendUpdates=none`, and no CalDAV
    /// write names an attendee or an organizer: the two ways Google mails.
    pub(crate) fn assert_nothing_announced(log: &[String]) {
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
}
