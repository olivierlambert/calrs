//! Native delegated Microsoft Graph calendar access (global cloud only).
//!
//! CalendarView expands recurrence server-side; full snapshots also remove
//! cancelled occurrences which Graph omits. Timed events use UTC; all-day
//! events retain calendar dates and their exclusive end, independent of the
//! response timezone label. No delta cursor is advertised.
//!
//! Booking identity is a named single-value legacy extended property, NOT the
//! read-only Graph iCalUId. Deterministic transactionId protects initial POST
//! retries; PATCH retains the Graph item identity. In-process mutations on
//! the same calendar are serialized, and duplicate identity matches fail closed rather than choosing
//! or deleting an arbitrary event. Graph does not offer an atomic UID upsert or
//! a documented unlimited transactionId retention period across processes.
//!
//! Protocol references:
//! https://learn.microsoft.com/en-us/graph/api/calendar-list-calendarview
//! https://learn.microsoft.com/en-us/graph/api/resources/event
//! https://learn.microsoft.com/en-us/graph/api/singlevaluelegacyextendedproperty-get

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::sync::{Arc, OnceLock};
use std::time::Duration as StdDuration;

use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, Duration, NaiveDate, NaiveDateTime, TimeZone, Utc};
use reqwest::{header, Method, StatusCode, Url};
use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tokio::sync::Mutex;

use super::{CalendarProvider, CalendarUnreadable, DeltaResult, RawEvent, RemoteCalendar};

const GRAPH_ROOT: &str = "https://graph.microsoft.com/v1.0/";
// Private property-set GUID: keep stable across releases and installations.
const UID_PROPERTY: &str = "String {4f742d1c-593e-4a5f-9552-d819c3db47c2} Name CalrsUid";
const PREFER: &str = "outlook.timezone=\"UTC\", IdType=\"ImmutableId\"";
/// Attempts for a request Graph throttles (429) or cannot serve right now
/// (503, 504), and the longest `Retry-After` honoured before giving up.
const MAX_ATTEMPTS: u32 = 3;
const MAX_RETRY_AFTER: StdDuration = StdDuration::from_secs(30);
/// Width of the calendarView window, from now: the horizon over which calrs
/// ever needs free/busy data. The sync bounds orphan removal with it.
pub(crate) const SYNC_WINDOW_DAYS: i64 = 730;
// One lock per calendar: mutations of a single calendar stay serialized
// without letting one throttled mailbox block writes to every other one.
static WRITES: OnceLock<Mutex<HashMap<String, Arc<Mutex<()>>>>> = OnceLock::new();

async fn write_lock(calendar_id: &str) -> Arc<Mutex<()>> {
    let mut map = WRITES
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .await;
    map.entry(calendar_id.to_owned()).or_default().clone()
}

/// Safe to display: deliberately excludes response bodies, request URLs and
/// reqwest source errors (which can include URLs with private query values).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GraphError {
    Http(u16),
    Transport,
    InvalidResponse,
    UnsafeUrl,
    PaginationLoop,
    InvalidCalendarId,
    InvalidDateTime,
    InvalidIcalendar,
    UnsupportedIcalendar,
    UnsupportedDelta,
    DuplicateUid,
}

impl fmt::Display for GraphError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Http(status) => write!(f, "Microsoft Graph request failed (HTTP {status})"),
            Self::Transport => f.write_str("Microsoft Graph transport failed"),
            Self::InvalidResponse => f.write_str("Microsoft Graph returned an invalid response"),
            Self::UnsafeUrl => f.write_str("Microsoft Graph URL is outside the allowed endpoint"),
            Self::PaginationLoop => f.write_str("Microsoft Graph pagination did not terminate"),
            Self::InvalidCalendarId => f.write_str("Invalid Microsoft Graph resource identifier"),
            Self::InvalidDateTime => f.write_str("Unsupported or invalid calendar date/time"),
            Self::InvalidIcalendar => f.write_str("Invalid booking iCalendar data"),
            Self::UnsupportedIcalendar => f.write_str("Unsupported booking iCalendar format"),
            Self::UnsupportedDelta => {
                f.write_str("Microsoft Graph provider requires a full snapshot")
            }
            Self::DuplicateUid => f.write_str(
                "Several Microsoft calendar events carry this booking's calrs identity, \
                 usually because the event was copied in Outlook; delete the copy",
            ),
        }
    }
}

impl std::error::Error for GraphError {}

pub struct GraphProvider {
    client: reqwest::Client,
    root: Url,
}

#[derive(Deserialize)]
struct Page<T> {
    value: Vec<T>,
    #[serde(rename = "@odata.nextLink")]
    next: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Calendar {
    id: String,
    name: Option<String>,
    hex_color: Option<String>,
    owner: Option<EmailAddress>,
    can_edit: Option<bool>,
    #[serde(default)]
    is_default_calendar: bool,
}

#[derive(Deserialize)]
struct EmailAddress {
    address: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GraphDateTime {
    date_time: String,
    time_zone: String,
}

#[derive(Deserialize)]
struct ExtendedProperty {
    id: String,
    value: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Event {
    id: String,
    #[serde(rename = "iCalUId")]
    ical_uid: Option<String>,
    subject: Option<String>,
    location: Option<Location>,
    start: GraphDateTime,
    end: GraphDateTime,
    show_as: String,
    is_cancelled: bool,
    is_all_day: bool,
    #[serde(rename = "type")]
    kind: String,
    original_start: Option<String>,
    #[serde(default)]
    single_value_extended_properties: Vec<ExtendedProperty>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Location {
    display_name: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct IdentityMatch {
    id: String,
    #[serde(default)]
    single_value_extended_properties: Vec<ExtendedProperty>,
}

impl GraphProvider {
    pub fn new(access_token: &str) -> Result<Self> {
        Self::with_root(
            access_token,
            Url::parse(GRAPH_ROOT).expect("constant Graph URL"),
        )
    }

    fn with_root(access_token: &str, root: Url) -> Result<Self> {
        let mut auth = header::HeaderValue::from_str(&format!("Bearer {access_token}"))
            .map_err(|_| GraphError::Transport)?;
        auth.set_sensitive(true);
        let mut headers = header::HeaderMap::new();
        headers.insert(header::AUTHORIZATION, auth);
        headers.insert("prefer", header::HeaderValue::from_static(PREFER));
        let client = reqwest::Client::builder()
            .default_headers(headers)
            .redirect(reqwest::redirect::Policy::none())
            .timeout(StdDuration::from_secs(30))
            .build()
            .map_err(|_| GraphError::Transport)?;
        Ok(Self { client, root })
    }

    fn endpoint(&self, segments: &[&str]) -> Result<Url> {
        let mut url = self.root.clone();
        {
            let mut path = url.path_segments_mut().map_err(|_| GraphError::UnsafeUrl)?;
            path.pop_if_empty();
            for segment in segments {
                if segment.is_empty() || matches!(*segment, "." | "..") {
                    return Err(GraphError::InvalidCalendarId.into());
                }
                // push percent-encodes opaque identifiers, including '/', '?', '#'.
                path.push(segment);
            }
        }
        Ok(url)
    }

    fn validate_url(&self, url: &Url) -> Result<()> {
        if url.origin() != self.root.origin()
            || !url.username().is_empty()
            || url.password().is_some()
            || url.fragment().is_some()
            || !url.path().starts_with(self.root.path())
        {
            return Err(GraphError::UnsafeUrl.into());
        }
        Ok(())
    }

    /// Sends one request, retrying a throttled (429) or temporarily
    /// unavailable (503, 504) one up to [`MAX_ATTEMPTS`] times. A POST is safe
    /// to resend: it carries a transactionId.
    async fn request(
        &self,
        method: Method,
        url: Url,
        body: Option<&Value>,
    ) -> Result<reqwest::Response> {
        self.validate_url(&url)?;
        let mut attempt = 1;
        loop {
            let mut request = self.client.request(method.clone(), url.clone());
            if let Some(body) = body {
                request = request.json(body);
            }
            let response = request.send().await.map_err(|_| GraphError::Transport)?;
            let retryable = matches!(
                response.status(),
                StatusCode::TOO_MANY_REQUESTS
                    | StatusCode::SERVICE_UNAVAILABLE
                    | StatusCode::GATEWAY_TIMEOUT
            );
            if !retryable || attempt >= MAX_ATTEMPTS {
                return Ok(response);
            }
            let wait = retry_after(&response)
                .unwrap_or_else(|| StdDuration::from_secs(u64::from(attempt)));
            if wait > MAX_RETRY_AFTER {
                return Ok(response);
            }
            tokio::time::sleep(wait).await;
            attempt += 1;
        }
    }

    async fn json<T: DeserializeOwned>(response: reqwest::Response) -> Result<T> {
        if !response.status().is_success() {
            return Err(GraphError::Http(response.status().as_u16()).into());
        }
        response
            .json()
            .await
            .map_err(|_| GraphError::InvalidResponse.into())
    }

    async fn pages<T: DeserializeOwned>(&self, mut url: Url) -> Result<Vec<T>> {
        let mut seen = HashSet::new();
        let mut values = Vec::new();
        loop {
            // A nextLink must stay on the Graph origin under /v1.0/. Its path
            // is not compared with the first page's: Graph may encode an
            // opaque identifier differently from how calrs sent it.
            self.validate_url(&url)?;
            if seen.len() >= 10_000 || !seen.insert(url.as_str().to_owned()) {
                return Err(GraphError::PaginationLoop.into());
            }
            let page: Page<T> = Self::json(self.request(Method::GET, url, None).await?).await?;
            values.extend(page.value);
            match page.next {
                Some(next) => url = Url::parse(&next).map_err(|_| GraphError::UnsafeUrl)?,
                None => return Ok(values),
            }
        }
    }

    async fn find_uid(&self, calendar_id: &str, uid: &str) -> Result<Option<String>> {
        if uid.is_empty() {
            return Err(GraphError::InvalidIcalendar.into());
        }
        let mut url = self.endpoint(&["me", "calendars", calendar_id, "events"])?;
        let filter = format!(
            "singleValueExtendedProperties/Any(ep: ep/id eq '{}' and ep/value eq '{}')",
            UID_PROPERTY,
            odata_string(uid)
        );
        url.query_pairs_mut()
            .append_pair("$filter", &filter)
            .append_pair("$expand", &uid_expand())
            .append_pair("$select", "id")
            .append_pair("$top", "100");
        let matches: Vec<IdentityMatch> = self.pages(url).await?;
        let mut ids = HashSet::new();
        for item in matches {
            // Graph's string-property filter is case-insensitive. The iCalendar
            // UID is not: expand and check the exact stored value before mutation.
            let property = item
                .single_value_extended_properties
                .iter()
                .find(|p| p.id == UID_PROPERTY)
                .ok_or(GraphError::InvalidResponse)?;
            if property.value == uid {
                if item.id.is_empty() {
                    return Err(GraphError::InvalidResponse.into());
                }
                ids.insert(item.id);
            }
        }
        if ids.len() > 1 {
            return Err(GraphError::DuplicateUid.into());
        }
        Ok(ids.into_iter().next())
    }
}

#[async_trait]
impl CalendarProvider for GraphProvider {
    async fn check_connection(&self) -> Result<bool> {
        let mut url = self.endpoint(&["me", "calendars"])?;
        url.query_pairs_mut()
            .append_pair("$top", "1")
            .append_pair("$select", "id");
        let _: Page<Calendar> = Self::json(self.request(Method::GET, url, None).await?).await?;
        Ok(true)
    }

    async fn list_calendars(&self) -> Result<Vec<RemoteCalendar>> {
        let mut url = self.endpoint(&["me", "calendars"])?;
        url.query_pairs_mut()
            .append_pair(
                "$select",
                "id,name,hexColor,owner,canEdit,isDefaultCalendar",
            )
            .append_pair("$top", "100");
        let calendars: Vec<Calendar> = self.pages(url).await?;
        // A calendar someone shared and the user accepted in Outlook is listed,
        // and readable, as one of the user's own. Its owner tells it apart.
        let me = calendars
            .iter()
            .find(|c| c.is_default_calendar)
            .and_then(|c| c.owner.as_ref()?.address.clone());
        calendars
            .into_iter()
            .map(|c| {
                if c.id.is_empty() {
                    return Err(GraphError::InvalidResponse.into());
                }
                let owner = c.owner.and_then(|o| o.address);
                Ok(RemoteCalendar {
                    id: c.id,
                    display_name: c.name,
                    color: c.hex_color.filter(|v| !v.is_empty()),
                    change_marker: None,
                    sync_state: None,
                    read_only: c.can_edit == Some(false),
                    shared: matches!((&me, &owner), (Some(me), Some(owner)) if !me.eq_ignore_ascii_case(owner)),
                })
            })
            .collect()
    }

    async fn fetch_events(&self, calendar_id: &str) -> Result<Vec<RawEvent>> {
        let lookback = Duration::days(crate::commands::sync::FULL_FETCH_LOOKBACK_DAYS);
        self.fetch_events_since(calendar_id, &(Utc::now() - lookback).to_rfc3339())
            .await
    }

    async fn fetch_events_since(
        &self,
        calendar_id: &str,
        since_utc: &str,
    ) -> Result<Vec<RawEvent>> {
        let since =
            DateTime::parse_from_rfc3339(since_utc).map_err(|_| GraphError::InvalidDateTime)?;
        let end = Utc::now() + Duration::days(SYNC_WINDOW_DAYS);
        if since >= end {
            return Err(GraphError::InvalidDateTime.into());
        }
        let mut url = self.endpoint(&["me", "calendars", calendar_id, "calendarView"])?;
        url.query_pairs_mut()
            .append_pair("startDateTime", &since.to_rfc3339())
            .append_pair("endDateTime", &end.to_rfc3339())
            .append_pair("$top", "1000")
            // Graph omits originalStart by default, even for recurring instances.
            .append_pair(
                "$select",
                "id,iCalUId,subject,location,start,end,showAs,isCancelled,isAllDay,type,originalStart",
            )
            .append_pair("$expand", &uid_expand());
        // Items are parsed one by one: Graph's schema lets fields such as
        // showAs or isCancelled be null, and one such item must not fail the
        // whole page, and with it the calendar.
        let items: Vec<Value> = self.pages(url).await.map_err(|error| {
            // Listed but refused. A share accepted in Outlook is served as
            // one of the user's own calendars and read with
            // Calendars.ReadWrite, so this is a fallback for other cases.
            match error.downcast_ref::<GraphError>() {
                Some(GraphError::Http(403 | 404)) => CalendarUnreadable.into(),
                _ => error,
            }
        })?;
        Ok(items
            .into_iter()
            .map(|item| {
                let remote_id = item["id"].as_str().unwrap_or_default().to_owned();
                serde_json::from_value::<Event>(item)
                    .map_err(|_| anyhow::Error::from(GraphError::InvalidResponse))
                    .and_then(event_to_ical)
                    .unwrap_or_else(|error| {
                        tracing::warn!(%error, "Microsoft Graph event calrs cannot read");
                        RawEvent::unreadable(remote_id)
                    })
            })
            .collect())
    }

    async fn sync_delta(
        &self,
        _calendar_id: &str,
        sync_state: Option<&str>,
    ) -> Result<DeltaResult> {
        if sync_state.is_some_and(|state| !state.is_empty()) {
            return Err(GraphError::UnsupportedDelta.into());
        }
        Ok(DeltaResult::default())
    }

    async fn put_event(&self, calendar_id: &str, uid: &str, ics: &str) -> Result<()> {
        let mut body = booking_body(uid, ics)?;
        let lock = write_lock(calendar_id).await;
        let _guard = lock.lock().await;
        if let Some(id) = self.find_uid(calendar_id, uid).await? {
            let url = self.endpoint(&["me", "calendars", calendar_id, "events", &id])?;
            let response = self.request(Method::PATCH, url, Some(&body)).await?;
            if !response.status().is_success() {
                // A concurrent deletion (404) fails this write rather than
                // re-creating the event at once. The write-back retry that
                // follows finds no event and creates a fresh one, as for any
                // event the host deleted while the booking stands.
                return Err(GraphError::Http(response.status().as_u16()).into());
            }
        } else {
            body["transactionId"] =
                Value::String(transaction_id(calendar_id, uid, ics_sequence(ics), &body));
            body["singleValueExtendedProperties"] = json!([{ "id": UID_PROPERTY, "value": uid }]);
            let url = self.endpoint(&["me", "calendars", calendar_id, "events"])?;
            let response = self.request(Method::POST, url, Some(&body)).await?;
            if !response.status().is_success() {
                return Err(GraphError::Http(response.status().as_u16()).into());
            }
        }
        Ok(())
    }

    async fn delete_event(&self, calendar_id: &str, uid: &str) -> Result<()> {
        let lock = write_lock(calendar_id).await;
        let _guard = lock.lock().await;
        let Some(id) = self.find_uid(calendar_id, uid).await? else {
            return Ok(());
        };
        let url = self.endpoint(&["me", "calendars", calendar_id, "events", &id])?;
        let response = self.request(Method::DELETE, url, None).await?;
        if !response.status().is_success() && response.status() != StatusCode::NOT_FOUND {
            return Err(GraphError::Http(response.status().as_u16()).into());
        }
        Ok(())
    }

    async fn event_exists(&self, calendar_id: &str, uid: &str) -> Result<bool> {
        Ok(self.find_uid(calendar_id, uid).await?.is_some())
    }
}

/// `Retry-After` in seconds; the HTTP-date form falls back to the default.
fn retry_after(response: &reqwest::Response) -> Option<StdDuration> {
    response
        .headers()
        .get(header::RETRY_AFTER)?
        .to_str()
        .ok()?
        .trim()
        .parse()
        .ok()
        .map(StdDuration::from_secs)
}

fn odata_string(value: &str) -> String {
    value.replace('\'', "''")
}

fn uid_expand() -> String {
    format!("singleValueExtendedProperties($filter=id eq '{UID_PROPERTY}')")
}

/// The invite's SEQUENCE, bumped by every reschedule (0 when absent).
fn ics_sequence(ics: &str) -> u32 {
    crate::utils::extract_vevent_field(&crate::utils::unfold_ical(ics), "SEQUENCE")
        .and_then(|value| value.trim().parse().ok())
        .unwrap_or(0)
}

fn transaction_id(calendar_id: &str, uid: &str, sequence: u32, body: &Value) -> String {
    let mut hash = Sha256::new();
    hash.update(b"calrs-graph-booking-v3\0");
    hash.update(calendar_id.as_bytes());
    hash.update([0]);
    hash.update(uid.as_bytes());
    // A booking deleted and re-created under the same UID must not collide
    // with a transactionId Graph may still remember from the first create,
    // even when it moves back to the same time (A, B, A): every reschedule
    // bumps the SEQUENCE. A retry of the same write keeps the same id.
    hash.update([0]);
    hash.update(sequence.to_be_bytes());
    hash.update([0]);
    hash.update(
        body["start"]["dateTime"]
            .as_str()
            .unwrap_or_default()
            .as_bytes(),
    );
    hash.update([0]);
    hash.update(
        body["end"]["dateTime"]
            .as_str()
            .unwrap_or_default()
            .as_bytes(),
    );
    // UUID-shaped deterministic identifier for this booking content.
    let digest = hash.finalize();
    let mut bytes = [0u8; 16];
    bytes.copy_from_slice(&digest[..16]);
    bytes[6] = (bytes[6] & 0x0f) | 0x50;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    uuid::Uuid::from_bytes(bytes).to_string()
}

fn graph_datetime(value: &GraphDateTime) -> Result<DateTime<Utc>> {
    if !matches!(value.time_zone.as_str(), "UTC" | "Etc/UTC") {
        return Err(GraphError::InvalidDateTime.into());
    }
    if let Ok(date) = DateTime::parse_from_rfc3339(&value.date_time) {
        return Ok(date.with_timezone(&Utc));
    }
    NaiveDateTime::parse_from_str(&value.date_time, "%Y-%m-%dT%H:%M:%S%.f")
        .map(|date| Utc.from_utc_datetime(&date))
        .map_err(|_| GraphError::InvalidDateTime.into())
}

fn graph_date(value: &GraphDateTime) -> Result<NaiveDate> {
    // Graph all-day boundaries are wall-clock midnights, even when Prefer UTC
    // labels them UTC. Converting them as instants shifts the blocked day.
    // Windows timezone names need no mapping for a calendar date.
    let date = NaiveDateTime::parse_from_str(&value.date_time, "%Y-%m-%dT%H:%M:%S%.f")
        .map_err(|_| GraphError::InvalidDateTime)?;
    if date.time() != chrono::NaiveTime::MIN {
        return Err(GraphError::InvalidDateTime.into());
    }
    Ok(date.date())
}

fn event_to_ical(event: Event) -> Result<RawEvent> {
    if event.id.is_empty() || event.kind == "seriesMaster" {
        return Err(GraphError::InvalidResponse.into());
    }
    let (start_line, end_line) = if event.is_all_day {
        if event.start.time_zone.is_empty() || event.start.time_zone != event.end.time_zone {
            return Err(GraphError::InvalidDateTime.into());
        }
        let start = graph_date(&event.start)?;
        let end = graph_date(&event.end)?;
        if end <= start {
            return Err(GraphError::InvalidDateTime.into());
        }
        (
            format!("DTSTART;VALUE=DATE:{}", start.format("%Y%m%d")),
            format!("DTEND;VALUE=DATE:{}", end.format("%Y%m%d")),
        )
    } else {
        let start = graph_datetime(&event.start)?;
        let end = graph_datetime(&event.end)?;
        if end < start {
            return Err(GraphError::InvalidDateTime.into());
        }
        (
            format!("DTSTART:{}", start.format("%Y%m%dT%H%M%SZ")),
            format!("DTEND:{}", end.format("%Y%m%dT%H%M%SZ")),
        )
    };
    let uid = event
        .single_value_extended_properties
        .iter()
        .find(|p| p.id == UID_PROPERTY && !p.value.is_empty())
        .map(|p| p.value.as_str())
        .or(event.ical_uid.as_deref().filter(|v| !v.is_empty()))
        .unwrap_or(&event.id);
    let mut lines = vec![
        "BEGIN:VCALENDAR".to_owned(),
        "VERSION:2.0".to_owned(),
        "PRODID:-//calrs//Microsoft Graph//EN".to_owned(),
        "BEGIN:VEVENT".to_owned(),
        format!("UID:{}", escape_text(uid)),
        format!("DTSTAMP:{}", Utc::now().format("%Y%m%dT%H%M%SZ")),
        start_line,
        end_line,
    ];
    if matches!(event.kind.as_str(), "occurrence" | "exception") {
        let original = event
            .original_start
            .as_deref()
            .ok_or(GraphError::InvalidResponse)?;
        let original =
            DateTime::parse_from_rfc3339(original).map_err(|_| GraphError::InvalidDateTime)?;
        lines.push(format!(
            "RECURRENCE-ID:{}",
            original.with_timezone(&Utc).format("%Y%m%dT%H%M%SZ")
        ));
    } else if event.kind != "singleInstance" {
        return Err(GraphError::InvalidResponse.into());
    }
    if let Some(subject) = event.subject {
        lines.push(format!("SUMMARY:{}", escape_text(&subject)));
    }
    // Graph sends an empty display name for an event without a location.
    if let Some(location) = event
        .location
        .and_then(|l| l.display_name)
        .filter(|l| !l.is_empty())
    {
        lines.push(format!("LOCATION:{}", escape_text(&location)));
    }
    // Matches the EWS availability policy: free and tentative do not block;
    // working elsewhere does, since the host is still working.
    let transparent = matches!(event.show_as.as_str(), "free" | "tentative");
    lines.push(format!(
        "TRANSP:{}",
        if transparent { "TRANSPARENT" } else { "OPAQUE" }
    ));
    lines.push(format!(
        "STATUS:{}",
        if event.is_cancelled {
            "CANCELLED"
        } else if event.show_as == "tentative" {
            "TENTATIVE"
        } else {
            "CONFIRMED"
        }
    ));
    lines.extend(["END:VEVENT".to_owned(), "END:VCALENDAR".to_owned()]);
    Ok(RawEvent {
        remote_id: event.id,
        ical: lines.join("\r\n") + "\r\n",
    })
}

fn escape_text(value: &str) -> String {
    let mut text = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '\\' => text.push_str("\\\\"),
            ',' => text.push_str("\\,"),
            ';' => text.push_str("\\;"),
            '\n' => text.push_str("\\n"),
            '\r' => {}
            _ => text.push(ch),
        }
    }
    text
}

fn unescape_text(value: &str) -> Result<String> {
    let mut text = String::with_capacity(value.len());
    let mut chars = value.chars();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            text.push(match chars.next() {
                Some('n' | 'N') => '\n',
                Some(ch @ ('\\' | ',' | ';')) => ch,
                _ => return Err(GraphError::InvalidIcalendar.into()),
            });
        } else {
            text.push(ch);
        }
    }
    Ok(text)
}

// Split the content line at a colon outside quoted parameter values (an
// ORGANIZER CN may itself contain a colon). Values may contain arbitrary colons.
fn property(line: &str) -> Result<(&str, &str)> {
    let mut quoted = false;
    for (index, ch) in line.char_indices() {
        match ch {
            '"' => quoted = !quoted,
            ':' if !quoted => return Ok((&line[..index], &line[index + 1..])),
            _ => {}
        }
    }
    Err(GraphError::InvalidIcalendar.into())
}

fn booking_body(uid: &str, ics: &str) -> Result<Value> {
    let unfolded = crate::utils::unfold_ical(ics);
    let mut stack: Vec<String> = Vec::new();
    let mut fields: HashMap<String, (&str, &str)> = HashMap::new();
    let mut event_count = 0;
    let mut calendar_count = 0;
    for line in unfolded.lines().filter(|line| !line.is_empty()) {
        let (key, value) = property(line)?;
        if key.eq_ignore_ascii_case("BEGIN") {
            let component = value.to_ascii_uppercase();
            match component.as_str() {
                "VCALENDAR" if stack.is_empty() => calendar_count += 1,
                "VEVENT" if stack.as_slice() == ["VCALENDAR"] => event_count += 1,
                "VALARM" if stack.as_slice() == ["VCALENDAR", "VEVENT"] => {}
                _ => return Err(GraphError::UnsupportedIcalendar.into()),
            }
            stack.push(component);
        } else if key.eq_ignore_ascii_case("END") {
            if stack.pop().as_deref() != Some(value.to_ascii_uppercase().as_str()) {
                return Err(GraphError::InvalidIcalendar.into());
            }
        } else if stack.as_slice() == ["VCALENDAR", "VEVENT"] {
            let name = key
                .split(';')
                .next()
                .unwrap_or_default()
                .to_ascii_uppercase();
            if matches!(
                name.as_str(),
                "RRULE" | "RDATE" | "EXDATE" | "EXRULE" | "RECURRENCE-ID" | "DURATION"
            ) {
                return Err(GraphError::UnsupportedIcalendar.into());
            }
            if matches!(
                name.as_str(),
                "UID"
                    | "DTSTART"
                    | "DTEND"
                    | "SUMMARY"
                    | "DESCRIPTION"
                    | "LOCATION"
                    | "TRANSP"
                    | "STATUS"
            ) && fields.insert(name, (key, value)).is_some()
            {
                return Err(GraphError::InvalidIcalendar.into());
            }
        } else if stack.is_empty() {
            return Err(GraphError::InvalidIcalendar.into());
        }
    }
    if !stack.is_empty() || event_count != 1 || calendar_count != 1 || uid.is_empty() {
        return Err(GraphError::InvalidIcalendar.into());
    }
    let field = |name: &str| {
        fields
            .get(name)
            .copied()
            .ok_or(GraphError::InvalidIcalendar)
    };
    if unescape_text(field("UID")?.1)? != uid {
        return Err(GraphError::InvalidIcalendar.into());
    }
    let (start, start_all_day) = booking_datetime(field("DTSTART")?)?;
    let (end, end_all_day) = booking_datetime(field("DTEND")?)?;
    if start_all_day != end_all_day || end <= start {
        return Err(GraphError::InvalidDateTime.into());
    }
    let text = |name: &str| -> Result<String> {
        match fields.get(name) {
            Some((key, value)) => {
                // Do not interpret encoded/binary text as plain UTF-8.
                for param in key.split(';').skip(1) {
                    let (name, value) =
                        param.split_once('=').ok_or(GraphError::InvalidIcalendar)?;
                    let plain_text = name.eq_ignore_ascii_case("LANGUAGE")
                        || (name.eq_ignore_ascii_case("VALUE")
                            && value.eq_ignore_ascii_case("TEXT"));
                    if !plain_text {
                        return Err(GraphError::UnsupportedIcalendar.into());
                    }
                }
                unescape_text(value)
            }
            None => Ok(String::new()),
        }
    };
    let status = fields.get("STATUS").map(|(_, v)| *v).unwrap_or("CONFIRMED");
    if !matches!(status, "CONFIRMED" | "TENTATIVE") {
        return Err(GraphError::UnsupportedIcalendar.into());
    }
    let transp = fields.get("TRANSP").map(|(_, v)| *v).unwrap_or("OPAQUE");
    if !matches!(transp, "OPAQUE" | "TRANSPARENT") {
        return Err(GraphError::UnsupportedIcalendar.into());
    }
    // No attendees/organizer: Cal.rs sends its own invitation emails. Graph
    // appointment writes must not generate a second set of invitations.
    Ok(json!({
        "subject": text("SUMMARY")?,
        "body": { "contentType": "text", "content": text("DESCRIPTION")? },
        "location": { "displayName": text("LOCATION")? },
        "start": { "dateTime": start.format("%Y-%m-%dT%H:%M:%S").to_string(), "timeZone": "UTC" },
        "end": { "dateTime": end.format("%Y-%m-%dT%H:%M:%S").to_string(), "timeZone": "UTC" },
        "isAllDay": start_all_day,
        "showAs": if transp == "TRANSPARENT" { "free" } else if status == "TENTATIVE" { "tentative" } else { "busy" },
        "responseRequested": false
    }))
}

fn booking_datetime((key, value): (&str, &str)) -> Result<(DateTime<Utc>, bool)> {
    let mut tzid = None;
    let mut date_only = false;
    let mut value_seen = false;
    for param in key.split(';').skip(1) {
        let (name, val) = param.split_once('=').ok_or(GraphError::InvalidIcalendar)?;
        let val = val
            .strip_prefix('"')
            .and_then(|v| v.strip_suffix('"'))
            .unwrap_or(val);
        if name.eq_ignore_ascii_case("TZID") && tzid.is_none() {
            tzid = Some(val);
        } else if name.eq_ignore_ascii_case("VALUE") && !value_seen {
            value_seen = true;
            if val.eq_ignore_ascii_case("DATE") {
                date_only = true;
            } else if !val.eq_ignore_ascii_case("DATE-TIME") {
                return Err(GraphError::UnsupportedIcalendar.into());
            }
        } else {
            return Err(GraphError::UnsupportedIcalendar.into());
        }
    }
    if date_only {
        if tzid.is_some() || value.len() != 8 {
            return Err(GraphError::InvalidDateTime.into());
        }
        let date =
            NaiveDate::parse_from_str(value, "%Y%m%d").map_err(|_| GraphError::InvalidDateTime)?;
        return Ok((
            Utc.from_utc_datetime(
                &date
                    .and_hms_opt(0, 0, 0)
                    .ok_or(GraphError::InvalidDateTime)?,
            ),
            true,
        ));
    }
    let is_utc = value.ends_with('Z');
    let naive = if is_utc {
        &value[..value.len() - 1]
    } else {
        value
    };
    if naive.len() != 15 || (is_utc && tzid.is_some()) {
        return Err(GraphError::InvalidDateTime.into());
    }
    let naive = NaiveDateTime::parse_from_str(naive, "%Y%m%dT%H%M%S")
        .map_err(|_| GraphError::InvalidDateTime)?;
    if is_utc {
        return Ok((Utc.from_utc_datetime(&naive), false));
    }
    let tz: chrono_tz::Tz = tzid
        .ok_or(GraphError::UnsupportedIcalendar)?
        .parse()
        .map_err(|_| GraphError::UnsupportedIcalendar)?;
    // Floating dates and Windows TZIDs must not be silently interpreted in
    // the server's local timezone, and a DST gap has no instant at all. An
    // ambiguous fold takes the earlier instant, as the shared ICS code does.
    let date = tz
        .from_local_datetime(&naive)
        .earliest()
        .ok_or(GraphError::InvalidDateTime)?;
    Ok((date.with_timezone(&Utc), false))
}

#[cfg(test)]
#[path = "graph_tests.rs"]
mod tests;
