//! Google Meet auto-links (issue #45 phase 3).
//!
//! A confirmed booking with `location_type = google_meet` gets a Meet conference
//! attached to the **host's** Google Calendar event via Calendar API v3
//! `conferenceData.createRequest`. Tokens come from the existing Google OAuth2
//! CalDAV source (`https://www.googleapis.com/auth/calendar`); no extra scope
//! and no reconnect.
//!
//! Meet REST API `spaces.create` is intentionally not used: it needs another
//! OAuth scope and is not available for consumer Gmail accounts.
//!
//! The conference is bound to the real write-back event (CalDAV PUT, then
//! Calendar API PATCH) so the host sees a native "Join with Google Meet"
//! button. A second CalDAV PUT of the same ICS would strip `conferenceData`,
//! which is why write-back skips only the Meet-holding Google source, and
//! only when `meeting_url` is actually a Meet link.

use std::sync::OnceLock;

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use chrono::{NaiveDateTime, TimeZone};
use chrono_tz::Tz;
use fluent_bundle::{FluentArgs, FluentValue};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::SqlitePool;

const CALENDAR_API_BASE: &str = "https://www.googleapis.com/calendar/v3";
const GOOGLE_CALDAV_MARKER: &str = "/caldav/v2/";
const HANGOUTS_MEET: &str = "hangoutsMeet";

/// `event_types.location_type` value for auto-generated Google Meet links.
pub const LOCATION_TYPE: &str = "google_meet";

/// Default attempts when Google reports `conferenceData.status = pending`.
pub const DEFAULT_PENDING_ATTEMPTS: u32 = 5;
const DEFAULT_PENDING_DELAY: std::time::Duration = std::time::Duration::from_millis(300);
/// Cap list/patch/get retries so a slow Calendar API cannot stall confirm.
/// Combined with the CalDAV PUT (10s timeout), worst-case guest POST is
/// about 30s. A reverse proxy with a 30s gateway timeout may return an
/// error for a booking that actually succeeded.
const ATTACH_DEADLINE: std::time::Duration = std::time::Duration::from_secs(20);

/// Query parameter that must be `1` or Google silently ignores conferenceData.
pub const CONFERENCE_DATA_VERSION: &str = "1";

#[derive(Debug, Clone)]
pub struct AttachConfig {
    pub max_attempts: u32,
    pub retry_delay: std::time::Duration,
}

impl Default for AttachConfig {
    fn default() -> Self {
        Self {
            max_attempts: DEFAULT_PENDING_ATTEMPTS,
            retry_delay: DEFAULT_PENDING_DELAY,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct MeetEvent {
    pub id: Option<String>,
    pub hangout_link: Option<String>,
    pub conference_status: Option<String>,
    pub video_uri: Option<String>,
}

impl MeetEvent {
    pub fn meet_url(&self) -> Option<String> {
        self.hangout_link
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .or_else(|| {
                self.video_uri
                    .as_deref()
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_string)
            })
    }

    pub fn is_pending(&self) -> bool {
        self.conference_status
            .as_deref()
            .is_some_and(|s| s.eq_ignore_ascii_case("pending"))
    }
}

/// Calendar API operations needed to mint and update a Meet conference.
/// Production uses [`LiveGoogleMeetApi`]; tests inject a fake.
#[async_trait]
pub trait GoogleMeetApi: Send + Sync {
    async fn put_ics(
        &self,
        source_url: &str,
        access_token: &str,
        calendar_href: &str,
        uid: &str,
        ics: &str,
    ) -> Result<()>;

    async fn find_event_id(
        &self,
        access_token: &str,
        calendar_id: &str,
        ical_uid: &str,
    ) -> Result<Option<String>>;

    async fn patch_conference(
        &self,
        access_token: &str,
        calendar_id: &str,
        event_id: &str,
        request_id: &str,
    ) -> Result<MeetEvent>;

    async fn get_event(
        &self,
        access_token: &str,
        calendar_id: &str,
        event_id: &str,
    ) -> Result<MeetEvent>;

    async fn patch_times(
        &self,
        access_token: &str,
        calendar_id: &str,
        event_id: &str,
        start_rfc3339: &str,
        end_rfc3339: &str,
    ) -> Result<()>;
}

/// Live Calendar API + CalDAV PUT client.
pub struct LiveGoogleMeetApi;

#[async_trait]
impl GoogleMeetApi for LiveGoogleMeetApi {
    async fn put_ics(
        &self,
        source_url: &str,
        access_token: &str,
        calendar_href: &str,
        uid: &str,
        ics: &str,
    ) -> Result<()> {
        let client = crate::caldav::CaldavClient::with_bearer(source_url, access_token);
        client.put_event(calendar_href, uid, ics).await
    }

    async fn find_event_id(
        &self,
        access_token: &str,
        calendar_id: &str,
        ical_uid: &str,
    ) -> Result<Option<String>> {
        let url = events_list_by_ical_uid_url(calendar_id, ical_uid);
        let resp = calendar_api_get(access_token, &url).await?;
        let parsed: EventListResponse = serde_json::from_value(resp)
            .map_err(|e| anyhow!("Google events list parse failed: {}", e))?;
        Ok(parsed
            .items
            .into_iter()
            .flatten()
            .find_map(|item| item.id.filter(|id| !id.is_empty())))
    }

    async fn patch_conference(
        &self,
        access_token: &str,
        calendar_id: &str,
        event_id: &str,
        request_id: &str,
    ) -> Result<MeetEvent> {
        let url = event_patch_url(calendar_id, event_id);
        let body = conference_patch_body(request_id);
        let resp = calendar_api_patch(access_token, &url, &body).await?;
        Ok(meet_event_from_json(&resp))
    }

    async fn get_event(
        &self,
        access_token: &str,
        calendar_id: &str,
        event_id: &str,
    ) -> Result<MeetEvent> {
        let url = event_get_url(calendar_id, event_id);
        let resp = calendar_api_get(access_token, &url).await?;
        Ok(meet_event_from_json(&resp))
    }

    async fn patch_times(
        &self,
        access_token: &str,
        calendar_id: &str,
        event_id: &str,
        start_rfc3339: &str,
        end_rfc3339: &str,
    ) -> Result<()> {
        let url = event_patch_url(calendar_id, event_id);
        let body = times_patch_body(start_rfc3339, end_rfc3339);
        let _ = calendar_api_patch(access_token, &url, &body).await?;
        Ok(())
    }
}

#[derive(Deserialize)]
struct EventListResponse {
    items: Option<Vec<EventListItem>>,
}

#[derive(Deserialize)]
struct EventListItem {
    id: Option<String>,
}

/// Build the Calendar API `calendarId` from a Google CalDAV collection href.
///
/// Google's collection URL is `/caldav/v2/{calendarId}/events`. The calendar
/// id is URL-encoded in the path (emails contain `@` and sometimes `+`).
/// Returns `None` for non-Google hrefs so callers can refuse rather than
/// guess `primary`.
pub fn calendar_id_from_caldav_href(href: &str) -> Option<String> {
    let lower = href.to_ascii_lowercase();
    let idx = lower.find(GOOGLE_CALDAV_MARKER)?;
    let after = &href[idx + GOOGLE_CALDAV_MARKER.len()..];
    let segment = after.split('/').next().unwrap_or("").trim();
    if segment.is_empty() {
        return None;
    }
    let decoded = urlencoding::decode(segment).ok()?;
    let id = decoded.trim();
    if id.is_empty() {
        return None;
    }
    Some(id.to_string())
}

/// JSON body for `events.patch` with `conferenceData.createRequest`.
/// `conferenceDataVersion=1` is a query param, not a body field.
pub fn conference_patch_body(request_id: &str) -> Value {
    json!({
        "conferenceData": {
            "createRequest": {
                "requestId": request_id,
                "conferenceSolutionKey": { "type": HANGOUTS_MEET }
            }
        }
    })
}

pub fn times_patch_body(start_rfc3339: &str, end_rfc3339: &str) -> Value {
    json!({
        "start": { "dateTime": start_rfc3339 },
        "end": { "dateTime": end_rfc3339 }
    })
}

pub fn events_list_by_ical_uid_url(calendar_id: &str, ical_uid: &str) -> String {
    format!(
        "{}/calendars/{}/events?iCalUID={}",
        CALENDAR_API_BASE,
        urlencoding::encode(calendar_id),
        urlencoding::encode(ical_uid)
    )
}

pub fn event_patch_url(calendar_id: &str, event_id: &str) -> String {
    format!(
        "{}/calendars/{}/events/{}?conferenceDataVersion={}",
        CALENDAR_API_BASE,
        urlencoding::encode(calendar_id),
        urlencoding::encode(event_id),
        CONFERENCE_DATA_VERSION
    )
}

pub fn event_get_url(calendar_id: &str, event_id: &str) -> String {
    format!(
        "{}/calendars/{}/events/{}",
        CALENDAR_API_BASE,
        urlencoding::encode(calendar_id),
        urlencoding::encode(event_id)
    )
}

pub fn meet_event_from_json(value: &Value) -> MeetEvent {
    let hangout_link = value
        .get("hangoutLink")
        .and_then(|v| v.as_str())
        .map(str::to_string);
    let conference = value.get("conferenceData");
    let conference_status = conference
        .and_then(|c| c.get("status"))
        .and_then(|s| s.get("statusCode"))
        .and_then(|s| s.as_str())
        .map(str::to_string);
    let video_uri = conference
        .and_then(|c| c.get("entryPoints"))
        .and_then(|e| e.as_array())
        .and_then(|arr| {
            arr.iter().find_map(|ep| {
                let kind = ep.get("entryPointType").and_then(|t| t.as_str())?;
                if kind == "video" {
                    ep.get("uri").and_then(|u| u.as_str()).map(str::to_string)
                } else {
                    None
                }
            })
        });
    MeetEvent {
        id: value.get("id").and_then(|v| v.as_str()).map(str::to_string),
        hangout_link,
        conference_status,
        video_uri,
    }
}

/// Whether this write-back source is the one holding the Meet conference.
/// A second ICS PUT on that calendar would strip `conferenceData`. Other
/// Google sources for the same host still get a copy. A leftover Jitsi or
/// webhook URL after switching the event type to `google_meet` is not a
/// Meet, so those bookings are not skipped.
pub async fn should_skip_caldav_put(
    pool: &SqlitePool,
    booking_uid: &str,
    user_id: &str,
    source_id: &str,
    auth_type: &str,
    oauth2_provider: Option<&str>,
) -> bool {
    if auth_type != "oauth2" || oauth2_provider != Some("google") {
        return false;
    }
    let row: Option<(String, Option<String>, String)> = sqlx::query_as(
        "SELECT et.location_type, b.meeting_url, b.id
         FROM bookings b
         JOIN event_types et ON et.id = b.event_type_id
         WHERE b.uid = ?",
    )
    .bind(booking_uid)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten();
    let Some((location_type, meeting_url, booking_id)) = row else {
        return false;
    };
    if location_type != LOCATION_TYPE {
        return false;
    }
    if !meeting_url.as_deref().is_some_and(is_google_meet_url) {
        return false;
    }
    match elect_meet_owner(pool, &booking_id).await {
        Some(owner) if owner == user_id => {}
        _ => return false,
    }
    match google_write_source_for_user(pool, user_id).await {
        Some(source) => source.id == source_id,
        None => false,
    }
}

/// Elect the user whose Google account owns the Meet conference.
///
/// Matches write-back host identity so the ORGANIZER and the Meet token
/// holder are the same person (Google 403s an organizer mismatch).
pub async fn elect_meet_owner(pool: &SqlitePool, booking_id: &str) -> Option<String> {
    let row: Option<(
        Option<String>,
        Option<String>,
        String,
        String,
        Option<String>,
    )> = sqlx::query_as(
        "SELECT b.assigned_user_id, et.team_id, et.scheduling_mode, et.id, a.user_id
         FROM bookings b
         JOIN event_types et ON et.id = b.event_type_id
         JOIN accounts a ON a.id = et.account_id
         WHERE b.id = ?",
    )
    .bind(booking_id)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten();
    let (assigned, team_id, mode, et_id, owner_id) = row?;

    if let Some(assigned) = assigned {
        return user_has_google_writeback(pool, &assigned)
            .await
            .then_some(assigned);
    }

    if let Some(team_id) = team_id {
        if mode == "collective" {
            // Same person as ORGANIZER (`ORDER BY u.name`). Do not skip to
            // the next member if the organizer later disconnects Google:
            // that would mint a Meet on someone else's calendar.
            let members = eligible_team_member_ids(pool, &team_id, Some(&et_id)).await;
            let first = members.into_iter().next()?;
            return user_has_google_writeback(pool, &first)
                .await
                .then_some(first);
        }
    }

    let owner = owner_id?;
    user_has_google_writeback(pool, &owner)
        .await
        .then_some(owner)
}

pub async fn user_has_google_writeback(pool: &SqlitePool, user_id: &str) -> bool {
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM caldav_sources cs
         JOIN accounts a ON a.id = cs.account_id
         WHERE a.user_id = ?
           AND cs.enabled = 1
           AND cs.auth_type = 'oauth2'
           AND cs.oauth2_provider = 'google'
           AND cs.write_calendar_href IS NOT NULL
           AND TRIM(cs.write_calendar_href) != ''",
    )
    .bind(user_id)
    .fetch_one(pool)
    .await
    .unwrap_or(0);
    count > 0
}

async fn eligible_team_member_ids(
    pool: &SqlitePool,
    team_id: &str,
    event_type_id: Option<&str>,
) -> Vec<String> {
    let rows: Vec<(String,)> = if let Some(et_id) = event_type_id {
        sqlx::query_as(
            "SELECT u.id FROM users u
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
        .unwrap_or_default()
    } else {
        sqlx::query_as(
            "SELECT u.id FROM users u
             JOIN team_members tm ON tm.user_id = u.id
             WHERE tm.team_id = ? AND u.enabled = 1
             ORDER BY u.name",
        )
        .bind(team_id)
        .fetch_all(pool)
        .await
        .unwrap_or_default()
    };
    rows.into_iter().map(|(id,)| id).collect()
}

/// Names of users who do not yet have a Google OAuth2 source with write-back.
/// One query for the whole set, preserving `user_ids` order.
pub async fn users_missing_google_writeback(pool: &SqlitePool, user_ids: &[String]) -> Vec<String> {
    if user_ids.is_empty() {
        return Vec::new();
    }
    let placeholders = vec!["?"; user_ids.len()].join(",");
    let sql = format!(
        "SELECT u.id, u.name FROM users u
         WHERE u.id IN ({placeholders})
           AND NOT EXISTS (
             SELECT 1 FROM caldav_sources cs
             JOIN accounts a ON a.id = cs.account_id
             WHERE a.user_id = u.id
               AND cs.enabled = 1
               AND cs.auth_type = 'oauth2'
               AND cs.oauth2_provider = 'google'
               AND cs.write_calendar_href IS NOT NULL
               AND TRIM(cs.write_calendar_href) != ''
           )"
    );
    let mut query = sqlx::query_as::<_, (String, String)>(&sql);
    for id in user_ids {
        query = query.bind(id);
    }
    let rows = query.fetch_all(pool).await.unwrap_or_default();
    user_ids
        .iter()
        .filter_map(|id| {
            rows.iter()
                .find(|(row_id, _)| row_id == id)
                .map(|(_, name)| {
                    if name.trim().is_empty() {
                        id.clone()
                    } else {
                        name.clone()
                    }
                })
        })
        .collect()
}

/// Guest-facing location when Meet minting failed. Not stored on
/// `bookings.meeting_url` — confirmation and email still show a label
/// instead of omitting the location row.
pub fn failed_mint_location_label(lang: &str) -> String {
    crate::i18n::translate(lang, "slots-location-google-meet", None)
}

/// Error message if Google Meet cannot be selected for this event type.
///
/// `personal_user_id` is the current user for a personal event type.
/// `team_id` (and optional `event_type_id` for per-member weights) covers
/// team event types: every eligible member must have Google write-back.
pub async fn google_meet_prereq_error(
    pool: &SqlitePool,
    personal_user_id: Option<&str>,
    team_id: Option<&str>,
    event_type_id: Option<&str>,
) -> Option<String> {
    let ids = if let Some(tid) = team_id {
        eligible_team_member_ids(pool, tid, event_type_id).await
    } else if let Some(uid) = personal_user_id {
        vec![uid.to_string()]
    } else {
        return Some(crate::i18n::translate(
            "en",
            "google-meet-prereq-no-host",
            None,
        ));
    };
    if ids.is_empty() {
        return Some(crate::i18n::translate(
            "en",
            "google-meet-prereq-no-eligible",
            None,
        ));
    }
    let missing = users_missing_google_writeback(pool, &ids).await;
    if missing.is_empty() {
        return None;
    }
    let names = missing.join(", ");
    let mut args = FluentArgs::new();
    args.set("names", FluentValue::from(names.as_str()));
    Some(crate::i18n::translate(
        "en",
        "google-meet-prereq-missing",
        Some(&args),
    ))
}

struct GoogleWriteSource {
    id: String,
    url: String,
    write_calendar_href: String,
    access_token_enc: String,
    token_expires_at: Option<String>,
}

async fn google_write_source_for_user(
    pool: &SqlitePool,
    user_id: &str,
) -> Option<GoogleWriteSource> {
    sqlx::query_as::<_, (String, String, String, Option<String>, Option<String>)>(
        "SELECT cs.id, cs.url, cs.write_calendar_href, cs.access_token_enc, cs.token_expires_at
         FROM caldav_sources cs
         JOIN accounts a ON a.id = cs.account_id
         WHERE a.user_id = ?
           AND cs.enabled = 1
           AND cs.auth_type = 'oauth2'
           AND cs.oauth2_provider = 'google'
           AND cs.write_calendar_href IS NOT NULL
           AND TRIM(cs.write_calendar_href) != ''
         ORDER BY cs.id
         LIMIT 1",
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten()
    .and_then(
        |(id, url, write_calendar_href, access_token_enc, token_expires_at)| {
            let access_token_enc = access_token_enc.filter(|s| !s.is_empty())?;
            Some(GoogleWriteSource {
                id,
                url,
                write_calendar_href,
                access_token_enc,
                token_expires_at,
            })
        },
    )
}

/// PUT the booking ICS onto the owner's Google calendar, then attach Meet.
pub async fn create_meet_for_booking(
    pool: &SqlitePool,
    key: &[u8; 32],
    booking_id: &str,
    api: &dyn GoogleMeetApi,
) -> Option<String> {
    create_meet_for_booking_with_config(pool, key, booking_id, api, &AttachConfig::default()).await
}

pub async fn create_meet_for_booking_with_config(
    pool: &SqlitePool,
    key: &[u8; 32],
    booking_id: &str,
    api: &dyn GoogleMeetApi,
    config: &AttachConfig,
) -> Option<String> {
    let owner_id = elect_meet_owner(pool, booking_id).await?;
    let source = google_write_source_for_user(pool, &owner_id).await?;
    let calendar_id = match calendar_id_from_caldav_href(&source.write_calendar_href) {
        Some(id) => id,
        None => {
            tracing::warn!(
                href = %source.write_calendar_href,
                "google meet: could not parse calendarId from write_calendar_href"
            );
            return None;
        }
    };

    let access_token = match crate::oauth2_caldav::get_valid_access_token(
        pool,
        key,
        &source.id,
        &source.access_token_enc,
        source.token_expires_at.as_deref(),
    )
    .await
    {
        Ok(t) => t,
        Err(e) => {
            tracing::warn!(error = %e, "google meet: could not get access token");
            return None;
        }
    };

    let details = booking_details_for_meet(pool, booking_id, &owner_id).await?;
    let ics = crate::email::generate_ics_caldav(&details);

    if let Err(e) = api
        .put_ics(
            &source.url,
            &access_token,
            &source.write_calendar_href,
            &details.uid,
            &ics,
        )
        .await
    {
        tracing::warn!(error = %e, uid = %details.uid, "google meet: CalDAV PUT failed");
        return None;
    }

    match tokio::time::timeout(
        ATTACH_DEADLINE,
        attach_meet(
            api,
            &access_token,
            &calendar_id,
            &details.uid,
            booking_id,
            config,
        ),
    )
    .await
    {
        Ok(url) => url,
        Err(_) => {
            tracing::warn!(
                booking_id = %booking_id,
                "google meet: attach_meet timed out"
            );
            None
        }
    }
}

/// Look up the CalDAV-created event by iCalUID, PATCH conferenceData, retry
/// while Google reports `pending`.
pub async fn attach_meet(
    api: &dyn GoogleMeetApi,
    access_token: &str,
    calendar_id: &str,
    ical_uid: &str,
    request_id: &str,
    config: &AttachConfig,
) -> Option<String> {
    let mut event_id = None;
    for attempt in 0..config.max_attempts {
        match api.find_event_id(access_token, calendar_id, ical_uid).await {
            Ok(Some(id)) => {
                event_id = Some(id);
                break;
            }
            Ok(None) => {
                tracing::debug!(
                    attempt,
                    ical_uid,
                    "google meet: event not yet visible via Calendar API"
                );
            }
            Err(e) => {
                tracing::warn!(error = %e, ical_uid, "google meet: events list failed");
                return None;
            }
        }
        if attempt + 1 < config.max_attempts && !config.retry_delay.is_zero() {
            tokio::time::sleep(config.retry_delay).await;
        }
    }
    let event_id = event_id?;

    let patched = match api
        .patch_conference(access_token, calendar_id, &event_id, request_id)
        .await
    {
        Ok(ev) => ev,
        Err(e) => {
            tracing::warn!(error = %e, event_id = %event_id, "google meet: conference patch failed");
            return None;
        }
    };

    let mut current = patched;
    for attempt in 0..config.max_attempts {
        if let Some(url) = current.meet_url() {
            if is_http_url(&url) {
                return Some(url);
            }
            tracing::warn!(url = %url, "google meet: hangoutLink is not http(s)");
            return None;
        }
        // Re-read while Google reports `pending`, and also when `statusCode` is
        // absent: live createRequest responses omit `conferenceData.status`
        // even while the conference is still materializing. Breaking on
        // `!is_pending()` alone would skip `get_event` and lose the Meet.
        if !conference_awaiting_link(&current) {
            break;
        }
        match api.get_event(access_token, calendar_id, &event_id).await {
            Ok(ev) => current = ev,
            Err(e) => {
                tracing::warn!(error = %e, "google meet: get event after pending failed");
                return None;
            }
        }
        if attempt + 1 < config.max_attempts
            && conference_awaiting_link(&current)
            && !config.retry_delay.is_zero()
        {
            tokio::time::sleep(config.retry_delay).await;
        }
    }

    match current.meet_url() {
        Some(url) if is_http_url(&url) => Some(url),
        _ => {
            tracing::warn!(event_id = %event_id, "google meet: no hangoutLink after retries");
            None
        }
    }
}

/// Whether another Calendar API GET might still produce a Meet URL.
///
/// `pending` is the documented async case. `None` covers responses that omit
/// `conferenceData.status` entirely (observed on a live Google account).
fn conference_awaiting_link(ev: &MeetEvent) -> bool {
    if ev.meet_url().is_some() {
        return false;
    }
    match ev.conference_status.as_deref() {
        None => true,
        Some(s) if s.eq_ignore_ascii_case("pending") => true,
        Some(_) => false,
    }
}

/// Patch start/end on the owner's Google event without touching conferenceData.
pub async fn patch_owner_event_times(
    pool: &SqlitePool,
    key: &[u8; 32],
    user_id: &str,
    booking_uid: &str,
    details: &crate::email::BookingDetails,
    api: &dyn GoogleMeetApi,
) -> Result<()> {
    let source = google_write_source_for_user(pool, user_id)
        .await
        .ok_or_else(|| anyhow!("no Google write source for meet owner"))?;
    let calendar_id = calendar_id_from_caldav_href(&source.write_calendar_href)
        .ok_or_else(|| anyhow!("cannot parse calendarId from write_calendar_href"))?;
    let access_token = crate::oauth2_caldav::get_valid_access_token(
        pool,
        key,
        &source.id,
        &source.access_token_enc,
        source.token_expires_at.as_deref(),
    )
    .await?;
    let event_id = api
        .find_event_id(&access_token, &calendar_id, booking_uid)
        .await?
        .ok_or_else(|| anyhow!("Google event not found for iCalUID {}", booking_uid))?;

    // bookings.start_at / end_at are naive host-zone datetimes and each has
    // its own date, so they survive midnight wrap and mixed guest/host tz in
    // BookingDetails (claim_booking builds details in guest-local wall clock).
    let stored: Option<(String, String, Option<String>)> = sqlx::query_as(
        "SELECT b.start_at, b.end_at, et.timezone
         FROM bookings b
         JOIN event_types et ON et.id = b.event_type_id
         WHERE b.uid = ?",
    )
    .bind(booking_uid)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten();

    let (start, end) = if let Some((start_at, end_at, et_tz)) = stored {
        let tz = et_tz
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or("UTC");
        rfc3339_from_stored(&start_at, &end_at, tz)
    } else {
        let tz = if !details.host_timezone.trim().is_empty() {
            details.host_timezone.as_str()
        } else {
            details.guest_timezone.as_str()
        };
        rfc3339_range(&details.date, &details.start_time, &details.end_time, tz)
    }
    .ok_or_else(|| anyhow!("could not convert booking times to RFC3339"))?;
    api.patch_times(&access_token, &calendar_id, &event_id, &start, &end)
        .await
}

fn naive_range_to_rfc3339(
    start_naive: NaiveDateTime,
    mut end_naive: NaiveDateTime,
    timezone: &str,
) -> Option<(String, String)> {
    let tz: Tz = timezone.parse().ok()?;
    if end_naive <= start_naive {
        end_naive += chrono::Duration::days(1);
    }
    let start_utc = tz.from_local_datetime(&start_naive).earliest()?.to_utc();
    let end_utc = tz.from_local_datetime(&end_naive).earliest()?.to_utc();
    Some((
        start_utc.to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        end_utc.to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
    ))
}

pub fn rfc3339_range(
    date: &str,
    start_time: &str,
    end_time: &str,
    timezone: &str,
) -> Option<(String, String)> {
    let start_naive =
        NaiveDateTime::parse_from_str(&format!("{} {}:00", date, start_time), "%Y-%m-%d %H:%M:%S")
            .ok()?;
    let end_naive =
        NaiveDateTime::parse_from_str(&format!("{} {}:00", date, end_time), "%Y-%m-%d %H:%M:%S")
            .ok()?;
    naive_range_to_rfc3339(start_naive, end_naive, timezone)
}

fn parse_stored_naive(value: &str) -> Option<NaiveDateTime> {
    NaiveDateTime::parse_from_str(value, "%Y-%m-%dT%H:%M:%S")
        .ok()
        .or_else(|| {
            let (d, t) = split_stored_datetime(value)?;
            NaiveDateTime::parse_from_str(&format!("{} {}:00", d, t), "%Y-%m-%d %H:%M:%S").ok()
        })
}

fn rfc3339_from_stored(start_at: &str, end_at: &str, timezone: &str) -> Option<(String, String)> {
    naive_range_to_rfc3339(
        parse_stored_naive(start_at)?,
        parse_stored_naive(end_at)?,
        timezone,
    )
}

async fn booking_details_for_meet(
    pool: &SqlitePool,
    booking_id: &str,
    owner_id: &str,
) -> Option<crate::email::BookingDetails> {
    let booking: Option<(
        String,
        String,
        String,
        String,
        String,
        Option<String>,
        String,
        Option<String>,
        Option<i32>,
    )> = sqlx::query_as(
        "SELECT b.uid, b.guest_name, b.guest_email, b.start_at, b.end_at,
                b.notes, et.title, et.timezone, et.reminder_minutes
         FROM bookings b
         JOIN event_types et ON et.id = b.event_type_id
         WHERE b.id = ?",
    )
    .bind(booking_id)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten();
    let (
        uid,
        guest_name,
        guest_email,
        start_at,
        end_at,
        notes,
        title,
        et_timezone,
        reminder_minutes,
    ) = booking?;

    let host: Option<(String, String)> =
        sqlx::query_as("SELECT name, COALESCE(booking_email, email) FROM users WHERE id = ?")
            .bind(owner_id)
            .fetch_optional(pool)
            .await
            .ok()
            .flatten();
    let (host_name, host_email) = host?;

    // bookings.start_at is naive in the event type timezone. Convert to UTC
    // for ICS by treating those wall-clock times as host-zone, not guest-zone.
    let host_tz = et_timezone
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("UTC");
    let (date, start_time) = split_stored_datetime(&start_at)?;
    let (_, end_time) = split_stored_datetime(&end_at)?;

    let attendees: Vec<(String,)> =
        sqlx::query_as("SELECT email FROM booking_attendees WHERE booking_id = ?")
            .bind(booking_id)
            .fetch_all(pool)
            .await
            .unwrap_or_default();

    Some(crate::email::BookingDetails {
        event_title: title,
        date,
        start_time,
        end_time,
        guest_name,
        guest_email,
        guest_timezone: host_tz.to_string(),
        host_name,
        host_email,
        uid,
        notes,
        location: None,
        reminder_minutes,
        additional_attendees: attendees.into_iter().map(|(e,)| e).collect(),
        host_timezone: host_tz.to_string(),
        ..Default::default()
    })
}

fn split_stored_datetime(value: &str) -> Option<(String, String)> {
    if !value.is_ascii() {
        return None;
    }
    let date = value.get(0..10)?.to_string();
    let time = value.get(11..16)?.to_string();
    if date.as_bytes().get(4) != Some(&b'-') {
        return None;
    }
    Some((date, time))
}

fn is_http_url(url: &str) -> bool {
    let lc = url.to_ascii_lowercase();
    lc.starts_with("http://") || lc.starts_with("https://")
}

/// True when `url` is a Google Meet conference link (`meet.google.com/...`).
/// Used so a leftover Jitsi/webhook URL is not treated as a conference to
/// protect from a second CalDAV PUT.
pub fn is_google_meet_url(url: &str) -> bool {
    let lc = url.trim().to_ascii_lowercase();
    let rest = lc
        .strip_prefix("https://")
        .or_else(|| lc.strip_prefix("http://"))
        .unwrap_or(lc.as_str());
    let rest = rest.strip_prefix("www.").unwrap_or(rest);
    rest.starts_with("meet.google.com/") && rest.len() > "meet.google.com/".len()
}

fn http_client() -> reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT
        .get_or_init(|| {
            reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(10))
                .user_agent("calrs-google-meet/1")
                .build()
                .unwrap_or_default()
        })
        .clone()
}

async fn calendar_api_get(access_token: &str, url: &str) -> Result<Value> {
    let resp = http_client()
        .get(url)
        .bearer_auth(access_token)
        .send()
        .await?;
    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(anyhow!("Google Calendar GET {} : {}", status, body));
    }
    Ok(resp.json().await?)
}

async fn calendar_api_patch(access_token: &str, url: &str, body: &Value) -> Result<Value> {
    let resp = http_client()
        .patch(url)
        .bearer_auth(access_token)
        .header("content-type", "application/json")
        .json(body)
        .send()
        .await?;
    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(anyhow!("Google Calendar PATCH {} : {}", status, body));
    }
    Ok(resp.json().await?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
    use std::collections::HashMap;
    use std::str::FromStr;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Mutex;

    #[test]
    fn calendar_id_from_primary_email_href() {
        let href = "https://apidata.googleusercontent.com/caldav/v2/alice%40gmail.com/events";
        assert_eq!(
            calendar_id_from_caldav_href(href).as_deref(),
            Some("alice@gmail.com")
        );
    }

    #[test]
    fn calendar_id_from_href_decodes_plus_in_email() {
        let href =
            "https://apidata.googleusercontent.com/caldav/v2/alice%2Btest%40gmail.com/events/";
        assert_eq!(
            calendar_id_from_caldav_href(href).as_deref(),
            Some("alice+test@gmail.com")
        );
    }

    #[test]
    fn calendar_id_from_secondary_calendar() {
        let href = "https://apidata.googleusercontent.com/caldav/v2/en.usa%23holiday%40group.v.calendar.google.com/events";
        assert_eq!(
            calendar_id_from_caldav_href(href).as_deref(),
            Some("en.usa#holiday@group.v.calendar.google.com")
        );
    }

    #[test]
    fn calendar_id_from_relative_href() {
        let href = "/caldav/v2/alice%40gmail.com/events/";
        assert_eq!(
            calendar_id_from_caldav_href(href).as_deref(),
            Some("alice@gmail.com")
        );
    }

    #[test]
    fn is_google_meet_url_accepts_conference_links_only() {
        assert!(is_google_meet_url("https://meet.google.com/aaa-bbbb-ccc"));
        assert!(is_google_meet_url(
            " http://www.meet.google.com/lookup/xyz "
        ));
        assert!(!is_google_meet_url("https://meet.jit.si/intro-abc"));
        assert!(!is_google_meet_url("https://example.com/webhook"));
        assert!(!is_google_meet_url("https://meet.google.com/"));
        assert!(!is_google_meet_url(""));
    }

    #[test]
    fn failed_mint_location_label_uses_fluent_key() {
        assert_eq!(failed_mint_location_label("en"), "Google Meet");
    }

    #[test]
    fn calendar_id_rejects_non_google_href() {
        assert_eq!(
            calendar_id_from_caldav_href(
                "https://nextcloud.example.com/remote.php/dav/calendars/alice/work/"
            ),
            None
        );
        assert_eq!(calendar_id_from_caldav_href(""), None);
        assert_eq!(
            calendar_id_from_caldav_href("https://apidata.googleusercontent.com/caldav/v2/"),
            None
        );
    }

    #[test]
    fn conference_patch_body_shape() {
        let body = conference_patch_body("booking-1");
        assert_eq!(
            body["conferenceData"]["createRequest"]["requestId"],
            "booking-1"
        );
        assert_eq!(
            body["conferenceData"]["createRequest"]["conferenceSolutionKey"]["type"],
            HANGOUTS_MEET
        );
        assert!(body.get("conferenceDataVersion").is_none());
    }

    #[test]
    fn event_patch_url_puts_version_in_query() {
        let url = event_patch_url("alice@gmail.com", "abc123");
        assert!(url.contains("conferenceDataVersion=1"));
        assert!(url.contains("calendars/alice%40gmail.com/"));
        assert!(url.contains("/events/abc123?"));
    }

    #[test]
    fn events_list_url_encodes_ical_uid() {
        let url = events_list_by_ical_uid_url("primary", "uid+special@calrs");
        assert!(url.contains("iCalUID=uid%2Bspecial%40calrs"));
    }

    #[test]
    fn meet_event_from_json_prefers_hangout_link() {
        let v = json!({
            "id": "ev1",
            "hangoutLink": "https://meet.google.com/aaa-bbbb-ccc",
            "conferenceData": {
                "status": { "statusCode": "success" },
                "entryPoints": [
                    { "entryPointType": "video", "uri": "https://meet.google.com/other" }
                ]
            }
        });
        let ev = meet_event_from_json(&v);
        assert_eq!(
            ev.meet_url().as_deref(),
            Some("https://meet.google.com/aaa-bbbb-ccc")
        );
        assert!(!ev.is_pending());
    }

    #[test]
    fn meet_event_from_json_falls_back_to_video_entrypoint() {
        let v = json!({
            "id": "ev1",
            "conferenceData": {
                "status": { "statusCode": "success" },
                "entryPoints": [
                    { "entryPointType": "phone", "uri": "tel:+123" },
                    { "entryPointType": "video", "uri": "https://meet.google.com/xyz-uvwx-rst" }
                ]
            }
        });
        let ev = meet_event_from_json(&v);
        assert_eq!(
            ev.meet_url().as_deref(),
            Some("https://meet.google.com/xyz-uvwx-rst")
        );
    }

    #[test]
    fn meet_event_pending_has_no_url_yet() {
        let v = json!({
            "id": "ev1",
            "conferenceData": { "status": { "statusCode": "pending" } }
        });
        let ev = meet_event_from_json(&v);
        assert!(ev.is_pending());
        assert!(ev.meet_url().is_none());
    }

    #[test]
    fn rfc3339_range_utc() {
        let (start, end) = rfc3339_range("2026-06-05", "10:00", "10:30", "UTC").unwrap();
        assert_eq!(start, "2026-06-05T10:00:00Z");
        assert_eq!(end, "2026-06-05T10:30:00Z");
    }

    #[test]
    fn rfc3339_range_crosses_midnight() {
        let (start, end) = rfc3339_range("2026-06-05", "23:30", "00:15", "UTC").unwrap();
        assert_eq!(start, "2026-06-05T23:30:00Z");
        assert_eq!(end, "2026-06-06T00:15:00Z");
    }

    #[test]
    fn rfc3339_range_non_utc() {
        let (start, end) = rfc3339_range("2026-06-05", "10:00", "10:30", "Europe/Paris").unwrap();
        assert_eq!(start, "2026-06-05T08:00:00Z");
        assert_eq!(end, "2026-06-05T08:30:00Z");
    }

    #[test]
    fn rfc3339_from_stored_uses_each_datetime() {
        let (start, end) =
            rfc3339_from_stored("2026-06-05T23:30:00", "2026-06-06T00:15:00", "UTC").unwrap();
        assert_eq!(start, "2026-06-05T23:30:00Z");
        assert_eq!(end, "2026-06-06T00:15:00Z");
    }

    #[test]
    fn split_stored_datetime_rejects_non_ascii() {
        assert!(split_stored_datetime("2026-06-05T1é:00:00").is_none());
        assert_eq!(
            split_stored_datetime("2026-06-05T10:00:00"),
            Some(("2026-06-05".to_string(), "10:00".to_string()))
        );
    }

    struct FakeApi {
        put_count: AtomicU32,
        conference_patches: AtomicU32,
        get_count: AtomicU32,
        fail_patch: bool,
        pending_then_success: bool,
        /// PATCH returns neither hangoutLink nor statusCode; GET then yields the URL.
        omit_status_on_patch: bool,
        hangout: String,
        events: Mutex<HashMap<String, String>>,
    }

    impl FakeApi {
        fn new(hangout: &str) -> Self {
            Self {
                put_count: AtomicU32::new(0),
                conference_patches: AtomicU32::new(0),
                get_count: AtomicU32::new(0),
                fail_patch: false,
                pending_then_success: false,
                omit_status_on_patch: false,
                hangout: hangout.to_string(),
                events: Mutex::new(HashMap::new()),
            }
        }
    }

    #[async_trait]
    impl GoogleMeetApi for FakeApi {
        async fn put_ics(
            &self,
            _source_url: &str,
            _access_token: &str,
            _calendar_href: &str,
            uid: &str,
            _ics: &str,
        ) -> Result<()> {
            self.put_count.fetch_add(1, Ordering::SeqCst);
            self.events
                .lock()
                .unwrap()
                .insert(uid.to_string(), format!("ev-{}", uid));
            Ok(())
        }

        async fn find_event_id(
            &self,
            _access_token: &str,
            _calendar_id: &str,
            ical_uid: &str,
        ) -> Result<Option<String>> {
            Ok(self.events.lock().unwrap().get(ical_uid).cloned())
        }

        async fn patch_conference(
            &self,
            _access_token: &str,
            _calendar_id: &str,
            event_id: &str,
            _request_id: &str,
        ) -> Result<MeetEvent> {
            self.conference_patches.fetch_add(1, Ordering::SeqCst);
            if self.fail_patch {
                return Err(anyhow!("403 forbidden"));
            }
            if self.pending_then_success {
                return Ok(MeetEvent {
                    id: Some(event_id.to_string()),
                    conference_status: Some("pending".to_string()),
                    ..Default::default()
                });
            }
            if self.omit_status_on_patch {
                return Ok(MeetEvent {
                    id: Some(event_id.to_string()),
                    ..Default::default()
                });
            }
            Ok(MeetEvent {
                id: Some(event_id.to_string()),
                hangout_link: Some(self.hangout.clone()),
                conference_status: Some("success".to_string()),
                ..Default::default()
            })
        }

        async fn get_event(
            &self,
            _access_token: &str,
            _calendar_id: &str,
            event_id: &str,
        ) -> Result<MeetEvent> {
            self.get_count.fetch_add(1, Ordering::SeqCst);
            Ok(MeetEvent {
                id: Some(event_id.to_string()),
                hangout_link: Some(self.hangout.clone()),
                conference_status: Some("success".to_string()),
                ..Default::default()
            })
        }

        async fn patch_times(
            &self,
            _access_token: &str,
            _calendar_id: &str,
            _event_id: &str,
            _start_rfc3339: &str,
            _end_rfc3339: &str,
        ) -> Result<()> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn attach_meet_immediate_hangout_link() {
        let api = FakeApi::new("https://meet.google.com/aaa-bbbb-ccc");
        api.events
            .lock()
            .unwrap()
            .insert("uid-1".into(), "ev-1".into());
        let url = attach_meet(
            &api,
            "token",
            "primary",
            "uid-1",
            "req-1",
            &AttachConfig {
                max_attempts: 3,
                retry_delay: std::time::Duration::ZERO,
            },
        )
        .await;
        assert_eq!(url.as_deref(), Some("https://meet.google.com/aaa-bbbb-ccc"));
        assert_eq!(api.conference_patches.load(Ordering::SeqCst), 1);
        assert_eq!(api.get_count.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn attach_meet_retries_pending_then_succeeds() {
        let mut api = FakeApi::new("https://meet.google.com/pending-ok");
        api.pending_then_success = true;
        api.events
            .lock()
            .unwrap()
            .insert("uid-2".into(), "ev-2".into());
        let url = attach_meet(
            &api,
            "token",
            "primary",
            "uid-2",
            "req-2",
            &AttachConfig {
                max_attempts: 3,
                retry_delay: std::time::Duration::ZERO,
            },
        )
        .await;
        assert_eq!(url.as_deref(), Some("https://meet.google.com/pending-ok"));
        assert!(api.get_count.load(Ordering::SeqCst) >= 1);
    }

    #[tokio::test]
    async fn attach_meet_rereads_when_patch_omits_status_and_url() {
        // Live Google: createRequest PATCH can return neither hangoutLink nor
        // status.statusCode. The old loop treated that as "not pending" and
        // never called get_event.
        let mut api = FakeApi::new("https://meet.google.com/reread-ok");
        api.omit_status_on_patch = true;
        api.events
            .lock()
            .unwrap()
            .insert("uid-omit".into(), "ev-omit".into());
        let url = attach_meet(
            &api,
            "token",
            "primary",
            "uid-omit",
            "req-omit",
            &AttachConfig {
                max_attempts: 3,
                retry_delay: std::time::Duration::ZERO,
            },
        )
        .await;
        assert_eq!(url.as_deref(), Some("https://meet.google.com/reread-ok"));
        assert_eq!(api.get_count.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn attach_meet_patch_error_returns_none() {
        let mut api = FakeApi::new("https://meet.google.com/nope");
        api.fail_patch = true;
        api.events
            .lock()
            .unwrap()
            .insert("uid-3".into(), "ev-3".into());
        let url = attach_meet(
            &api,
            "token",
            "primary",
            "uid-3",
            "req-3",
            &AttachConfig {
                max_attempts: 2,
                retry_delay: std::time::Duration::ZERO,
            },
        )
        .await;
        assert!(url.is_none());
    }

    #[test]
    fn conference_awaiting_link_treats_missing_status_like_pending() {
        assert!(conference_awaiting_link(&MeetEvent::default()));
        assert!(conference_awaiting_link(&MeetEvent {
            conference_status: Some("pending".into()),
            ..Default::default()
        }));
        assert!(!conference_awaiting_link(&MeetEvent {
            conference_status: Some("success".into()),
            ..Default::default()
        }));
        assert!(!conference_awaiting_link(&MeetEvent {
            hangout_link: Some("https://meet.google.com/x".into()),
            conference_status: None,
            ..Default::default()
        }));
    }

    async fn memory_pool() -> SqlitePool {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(
                SqliteConnectOptions::from_str("sqlite::memory:")
                    .unwrap()
                    .foreign_keys(true),
            )
            .await
            .unwrap();
        crate::db::migrate(&pool).await.unwrap();
        pool
    }

    async fn insert_user(pool: &SqlitePool, email: &str, name: &str) -> String {
        let id = uuid::Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO users (id, email, name, role, auth_provider, username, enabled)
             VALUES (?, ?, ?, 'user', 'local', ?, 1)",
        )
        .bind(&id)
        .bind(email)
        .bind(name)
        .bind(email.split('@').next().unwrap())
        .execute(pool)
        .await
        .unwrap();
        let account_id = uuid::Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO accounts (id, user_id, name, email, timezone)
             VALUES (?, ?, ?, ?, 'UTC')",
        )
        .bind(&account_id)
        .bind(&id)
        .bind(name)
        .bind(email)
        .execute(pool)
        .await
        .unwrap();
        id
    }

    async fn insert_google_source(pool: &SqlitePool, user_id: &str, with_write: bool) -> String {
        let account_id: String =
            sqlx::query_scalar("SELECT id FROM accounts WHERE user_id = ? LIMIT 1")
                .bind(user_id)
                .fetch_one(pool)
                .await
                .unwrap();
        let href = if with_write {
            Some("https://apidata.googleusercontent.com/caldav/v2/alice%40gmail.com/events/")
        } else {
            None
        };
        let source_id = uuid::Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO caldav_sources (id, account_id, name, url, username, auth_type, oauth2_provider, access_token_enc, write_calendar_href, enabled)
             VALUES (?, ?, 'Google', 'https://apidata.googleusercontent.com/caldav/v2/alice%40gmail.com/user', 'alice@gmail.com', 'oauth2', 'google', 'tok', ?, 1)",
        )
        .bind(&source_id)
        .bind(&account_id)
        .bind(href)
        .execute(pool)
        .await
        .unwrap();
        source_id
    }

    async fn insert_event_type(
        pool: &SqlitePool,
        owner_id: &str,
        team_id: Option<&str>,
        mode: &str,
        location_type: &str,
    ) -> String {
        let account_id: String =
            sqlx::query_scalar("SELECT id FROM accounts WHERE user_id = ? LIMIT 1")
                .bind(owner_id)
                .fetch_one(pool)
                .await
                .unwrap();
        let et_id = uuid::Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO event_types (id, account_id, slug, title, duration_min, location_type, team_id, created_by_user_id, scheduling_mode, timezone)
             VALUES (?, ?, ?, 'Meet', 30, ?, ?, ?, ?, 'UTC')",
        )
        .bind(&et_id)
        .bind(&account_id)
        .bind(format!("et-{}", &et_id[..8]))
        .bind(location_type)
        .bind(team_id)
        .bind(owner_id)
        .bind(mode)
        .execute(pool)
        .await
        .unwrap();
        et_id
    }

    async fn insert_booking(
        pool: &SqlitePool,
        et_id: &str,
        assigned: Option<&str>,
        meeting_url: Option<&str>,
    ) -> (String, String) {
        let bid = uuid::Uuid::new_v4().to_string();
        let uid = format!("{}@calrs", bid);
        sqlx::query(
            "INSERT INTO bookings (id, event_type_id, uid, guest_name, guest_email, guest_timezone, start_at, end_at, status, cancel_token, reschedule_token, assigned_user_id, meeting_url)
             VALUES (?, ?, ?, 'Bob', 'bob@example.com', 'UTC', '2026-06-05T10:00:00', '2026-06-05T10:30:00', 'confirmed', ?, ?, ?, ?)",
        )
        .bind(&bid)
        .bind(et_id)
        .bind(&uid)
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(assigned)
        .bind(meeting_url)
        .execute(pool)
        .await
        .unwrap();
        (bid, uid)
    }

    async fn insert_team(pool: &SqlitePool, members: &[(&str, &str)]) -> String {
        let team_id = uuid::Uuid::new_v4().to_string();
        let creator = members[0].0;
        sqlx::query(
            "INSERT INTO teams (id, name, slug, visibility, created_by)
             VALUES (?, 'T', ?, 'public', ?)",
        )
        .bind(&team_id)
        .bind(format!("t-{}", &team_id[..8]))
        .bind(creator)
        .execute(pool)
        .await
        .unwrap();
        for (uid, role) in members {
            sqlx::query(
                "INSERT INTO team_members (team_id, user_id, role, source)
                 VALUES (?, ?, ?, 'direct')",
            )
            .bind(&team_id)
            .bind(uid)
            .bind(role)
            .execute(pool)
            .await
            .unwrap();
        }
        team_id
    }

    #[tokio::test]
    async fn elect_owner_personal() {
        let pool = memory_pool().await;
        let alice = insert_user(&pool, "alice@example.com", "Alice").await;
        insert_google_source(&pool, &alice, true).await;
        let et = insert_event_type(&pool, &alice, None, "round_robin", "google_meet").await;
        let (bid, _) = insert_booking(&pool, &et, None, None).await;
        assert_eq!(
            elect_meet_owner(&pool, &bid).await.as_deref(),
            Some(alice.as_str())
        );
    }

    #[tokio::test]
    async fn elect_owner_round_robin_assigned() {
        let pool = memory_pool().await;
        let alice = insert_user(&pool, "alice@rr.example.com", "Alice").await;
        let bob = insert_user(&pool, "bob@rr.example.com", "Bob").await;
        insert_google_source(&pool, &alice, true).await;
        insert_google_source(&pool, &bob, true).await;
        let team = insert_team(&pool, &[(&alice, "admin"), (&bob, "member")]).await;
        let et = insert_event_type(&pool, &alice, Some(&team), "round_robin", "google_meet").await;
        let (bid, _) = insert_booking(&pool, &et, Some(&bob), None).await;
        assert_eq!(
            elect_meet_owner(&pool, &bid).await.as_deref(),
            Some(bob.as_str())
        );
    }

    #[tokio::test]
    async fn elect_owner_collective_first_by_name() {
        let pool = memory_pool().await;
        let alice = insert_user(&pool, "alice@col.example.com", "Alice").await;
        let carol = insert_user(&pool, "carol@col.example.com", "Carol").await;
        insert_google_source(&pool, &alice, true).await;
        insert_google_source(&pool, &carol, true).await;
        let team = insert_team(&pool, &[(&carol, "admin"), (&alice, "member")]).await;
        let et = insert_event_type(&pool, &carol, Some(&team), "collective", "google_meet").await;
        let (bid, _) = insert_booking(&pool, &et, None, None).await;
        // ORDER BY u.name → Alice before Carol
        assert_eq!(
            elect_meet_owner(&pool, &bid).await.as_deref(),
            Some(alice.as_str())
        );
    }

    #[tokio::test]
    async fn elect_owner_collective_does_not_skip_organizer_without_google() {
        let pool = memory_pool().await;
        let alice = insert_user(&pool, "alice@colmiss.example.com", "Alice").await;
        let carol = insert_user(&pool, "carol@colmiss.example.com", "Carol").await;
        insert_google_source(&pool, &carol, true).await;
        let team = insert_team(&pool, &[(&carol, "admin"), (&alice, "member")]).await;
        let et = insert_event_type(&pool, &carol, Some(&team), "collective", "google_meet").await;
        let (bid, _) = insert_booking(&pool, &et, None, None).await;
        assert!(
            elect_meet_owner(&pool, &bid).await.is_none(),
            "must not mint Meet on Carol when Alice is ORGANIZER"
        );
    }

    #[tokio::test]
    async fn elect_owner_missing_google_returns_none() {
        let pool = memory_pool().await;
        let alice = insert_user(&pool, "alice@nogoogle.example.com", "Alice").await;
        let et = insert_event_type(&pool, &alice, None, "round_robin", "google_meet").await;
        let (bid, _) = insert_booking(&pool, &et, None, None).await;
        assert!(elect_meet_owner(&pool, &bid).await.is_none());
    }

    #[tokio::test]
    async fn prereq_rejects_team_member_without_google() {
        let pool = memory_pool().await;
        let alice = insert_user(&pool, "alice@gate.example.com", "Alice").await;
        let bob = insert_user(&pool, "bob@gate.example.com", "Bob").await;
        insert_google_source(&pool, &alice, true).await;
        let team = insert_team(&pool, &[(&alice, "admin"), (&bob, "member")]).await;
        let err = google_meet_prereq_error(&pool, None, Some(&team), None).await;
        assert!(err.as_ref().unwrap().contains("Bob"), "{:?}", err);
        assert!(!err.as_ref().unwrap().contains("Alice"), "{:?}", err);
    }

    #[tokio::test]
    async fn prereq_accepts_when_every_member_has_google() {
        let pool = memory_pool().await;
        let alice = insert_user(&pool, "alice@ok.example.com", "Alice").await;
        let bob = insert_user(&pool, "bob@ok.example.com", "Bob").await;
        insert_google_source(&pool, &alice, true).await;
        insert_google_source(&pool, &bob, true).await;
        let team = insert_team(&pool, &[(&alice, "admin"), (&bob, "member")]).await;
        assert!(google_meet_prereq_error(&pool, None, Some(&team), None)
            .await
            .is_none());
    }

    #[tokio::test]
    async fn prereq_personal_requires_writeback_href() {
        let pool = memory_pool().await;
        let alice = insert_user(&pool, "alice@href.example.com", "Alice").await;
        insert_google_source(&pool, &alice, false).await;
        let err = google_meet_prereq_error(&pool, Some(&alice), None, None).await;
        assert!(err.unwrap().contains("Alice"));
    }

    #[tokio::test]
    async fn should_skip_caldav_put_only_for_owner_google_source() {
        let pool = memory_pool().await;
        let alice = insert_user(&pool, "alice@skip.example.com", "Alice").await;
        let bob = insert_user(&pool, "bob@skip.example.com", "Bob").await;
        insert_google_source(&pool, &alice, true).await;
        let bob_source = insert_google_source(&pool, &bob, true).await;
        let team = insert_team(&pool, &[(&alice, "admin"), (&bob, "member")]).await;
        let et = insert_event_type(&pool, &alice, Some(&team), "round_robin", "google_meet").await;
        let (bid, uid) = insert_booking(
            &pool,
            &et,
            Some(&bob),
            Some("https://meet.google.com/aaa-bbbb-ccc"),
        )
        .await;
        let _ = bid;
        assert!(
            should_skip_caldav_put(&pool, &uid, &bob, &bob_source, "oauth2", Some("google")).await,
            "assigned Meet owner Google source must be skipped"
        );
        assert!(
            !should_skip_caldav_put(&pool, &uid, &alice, &bob_source, "oauth2", Some("google"))
                .await,
            "other members still get a CalDAV copy"
        );
        assert!(
            !should_skip_caldav_put(&pool, &uid, &bob, &bob_source, "basic", None).await,
            "non-Google sources are never skipped"
        );
    }

    #[tokio::test]
    async fn should_skip_caldav_put_ignores_non_meet_urls() {
        let pool = memory_pool().await;
        let alice = insert_user(&pool, "alice@jitsi.example.com", "Alice").await;
        let source = insert_google_source(&pool, &alice, true).await;
        let et = insert_event_type(&pool, &alice, None, "round_robin", "google_meet").await;
        let (_, uid) =
            insert_booking(&pool, &et, None, Some("https://meet.jit.si/leftover-room")).await;
        assert!(
            !should_skip_caldav_put(&pool, &uid, &alice, &source, "oauth2", Some("google")).await,
            "a leftover Jitsi URL is not a conference to protect"
        );
    }

    #[tokio::test]
    async fn should_skip_caldav_put_only_the_ordered_google_source() {
        let pool = memory_pool().await;
        let alice = insert_user(&pool, "alice@two.example.com", "Alice").await;
        let account_id: String =
            sqlx::query_scalar("SELECT id FROM accounts WHERE user_id = ? LIMIT 1")
                .bind(&alice)
                .fetch_one(&pool)
                .await
                .unwrap();
        let first = "00000000-0000-4000-8000-000000000001";
        let second = "ffffffff-ffff-4fff-bfff-ffffffffffff";
        for (id, name) in [(first, "Google A"), (second, "Google B")] {
            sqlx::query(
                "INSERT INTO caldav_sources (id, account_id, name, url, username, auth_type, \
                 oauth2_provider, access_token_enc, write_calendar_href, enabled) \
                 VALUES (?, ?, ?, 'https://apidata.googleusercontent.com/caldav/v2/x/user', \
                 'alice@gmail.com', 'oauth2', 'google', 'tok', \
                 'https://apidata.googleusercontent.com/caldav/v2/alice%40gmail.com/events/', 1)",
            )
            .bind(id)
            .bind(&account_id)
            .bind(name)
            .execute(&pool)
            .await
            .unwrap();
        }
        let et = insert_event_type(&pool, &alice, None, "round_robin", "google_meet").await;
        let (_, uid) = insert_booking(
            &pool,
            &et,
            None,
            Some("https://meet.google.com/aaa-bbbb-ccc"),
        )
        .await;
        assert!(
            should_skip_caldav_put(&pool, &uid, &alice, first, "oauth2", Some("google")).await,
            "ORDER BY cs.id picks the first source as the Meet calendar"
        );
        assert!(
            !should_skip_caldav_put(&pool, &uid, &alice, second, "oauth2", Some("google")).await,
            "the host's second Google calendar still gets a CalDAV copy"
        );
    }

    #[tokio::test]
    async fn create_meet_for_booking_puts_then_patches() {
        let pool = memory_pool().await;
        let alice = insert_user(&pool, "alice@create.example.com", "Alice").await;
        let source_id = insert_google_source(&pool, &alice, true).await;
        let et = insert_event_type(&pool, &alice, None, "round_robin", "google_meet").await;
        let (bid, _) = insert_booking(&pool, &et, None, None).await;
        let api = FakeApi::new("https://meet.google.com/created-room");
        let key = [0u8; 32];
        // access_token_enc is plaintext "tok"; get_valid_access_token will try
        // decrypt and fail, then we cannot proceed. Seed an encrypted token.
        let enc = crate::crypto::encrypt_password(&key, "access-token").unwrap();
        sqlx::query(
            "UPDATE caldav_sources SET access_token_enc = ?, token_expires_at = ? WHERE id = ?",
        )
        .bind(&enc)
        .bind((chrono::Utc::now() + chrono::Duration::hours(1)).to_rfc3339())
        .bind(&source_id)
        .execute(&pool)
        .await
        .unwrap();

        let url = create_meet_for_booking_with_config(
            &pool,
            &key,
            &bid,
            &api,
            &AttachConfig {
                max_attempts: 2,
                retry_delay: std::time::Duration::ZERO,
            },
        )
        .await;
        assert_eq!(url.as_deref(), Some("https://meet.google.com/created-room"));
        assert_eq!(api.put_count.load(Ordering::SeqCst), 1);
        assert_eq!(api.conference_patches.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn generate_and_persist_returns_existing_meet_url() {
        let pool = memory_pool().await;
        let alice = insert_user(&pool, "alice@idem.example.com", "Alice").await;
        insert_google_source(&pool, &alice, true).await;
        let et = insert_event_type(&pool, &alice, None, "round_robin", "google_meet").await;
        let existing = "https://meet.google.com/already-there";
        let (bid, _) = insert_booking(&pool, &et, None, Some(existing)).await;
        let url = crate::web::meeting::generate_and_persist(
            &pool,
            &[0u8; 32],
            &bid,
            &et,
            Some(&alice),
            "Bob",
            "bob@example.com",
        )
        .await;
        assert_eq!(url.as_deref(), Some(existing));
    }
}
