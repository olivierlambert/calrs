//! Outgoing webhooks for booking events (#168).
//!
//! A user or a team admin points calrs at a URL; whenever a booking is made,
//! requested, approved, declined, expires, is cancelled or moves, calrs POSTs
//! a versioned JSON message there, signed with the endpoint's secret.
//!
//! ## Never in the way of a booking
//!
//! The handlers only call [`enqueue`], after their own change has committed:
//! it reads the booking back, writes one `webhook_deliveries` row per
//! matching endpoint and wakes the delivery task. Every error is logged and
//! swallowed. The HTTP request itself happens in [`run_webhook_loop`], with a
//! timeout, bounded retries and backoff, the way calendar write-back retries
//! work (`writeback.rs`). With no endpoint configured, `enqueue` is one
//! `SELECT EXISTS` and nothing else.
//!
//! ## Signing
//!
//! Same scheme as the meeting webhook and the SMS webhook adapter: the raw
//! body is signed with HMAC-SHA256 and sent as `X-Calrs-Signature:
//! sha256=<hex>`. The body carries a `timestamp` (Unix seconds, rewritten on
//! each attempt) and an `id` that stays the same across retries, so a
//! receiver can refuse replays and drop duplicates with nothing but the
//! signed body.
//!
//! ## SSRF posture
//!
//! Unlike the two admin-only webhooks, these URLs can be set by any user the
//! admin lets in, so they get the CalDAV / resource feed policy and more:
//! HTTPS only (plain HTTP only for a host on the operator's private-host
//! allowlist), no credentials in the URL, no private or reserved address
//! unless allowlisted, checked again at every delivery, the checked
//! addresses pinned for the request itself (so DNS rebinding between the
//! check and the connection does not work), no redirects, short timeouts,
//! and at most a short excerpt of an error response kept.

use chrono::{DateTime, Utc};
use chrono_tz::Tz;
use serde::Serialize;
use sqlx::SqlitePool;
use std::net::{IpAddr, SocketAddr};
use std::time::Duration;

/// Version of the JSON body. Bumped only for a change a receiver must handle;
/// new fields are added without a bump.
pub const PAYLOAD_VERSION: u32 = 1;
/// Endpoints per scope (one event type, or one user's personal event types).
pub const MAX_ENDPOINTS_PER_SCOPE: i64 = 10;
pub const SECRET_MIN_LEN: usize = 16;
pub const SECRET_MAX_LEN: usize = 256;
pub const URL_MAX_LEN: usize = 2048;

/// Minutes to wait after the n-th failed attempt. The first attempt is
/// immediate, so a delivery gets `MAX_ATTEMPTS` tries over about 17 hours.
const RETRY_DELAYS_MIN: [i64; 6] = [1, 5, 15, 60, 240, 720];
pub const MAX_ATTEMPTS: i64 = RETRY_DELAYS_MIN.len() as i64 + 1;

const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
/// How much of an error response is read, and how much of it is kept.
const ERROR_BODY_READ_BYTES: usize = 1024;
const ERROR_SNIPPET_CHARS: usize = 200;
/// Deliveries that ran out of attempts stay visible this long.
const FAILED_RETENTION_DAYS: i64 = 7;
/// Deliveries handled per pass of the delivery task.
const BATCH: i64 = 100;

/// SQL condition for an endpoint that may receive deliveries. Expects the
/// aliases `w` (webhooks) and `u` (the user who added it). The admin switch
/// is read live, so turning it off stops every non-admin endpoint at once,
/// queued deliveries included.
const ACTIVE_ENDPOINT: &str = "w.enabled = 1 AND u.enabled = 1 AND (u.role = 'admin' OR COALESCE((SELECT webhooks_allow_all_users FROM auth_config WHERE id = 'singleton'), 0) = 1)";

/// What happened to the booking.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    /// A booking was made and is confirmed.
    Created,
    /// A booking was requested and waits for the host's approval.
    Requested,
    /// The host approved a pending request (or a pending reschedule).
    Confirmed,
    /// The host declined a pending request.
    Declined,
    /// A pending request reached its deadline unanswered.
    Expired,
    /// The guest or the host cancelled the booking.
    Cancelled,
    /// The booking moved to another time (possibly waiting for approval).
    Rescheduled,
    /// The "Send test" button.
    Ping,
}

impl Event {
    pub fn name(self) -> &'static str {
        match self {
            Event::Created => "booking.created",
            Event::Requested => "booking.requested",
            Event::Confirmed => "booking.confirmed",
            Event::Declined => "booking.declined",
            Event::Expired => "booking.expired",
            Event::Cancelled => "booking.cancelled",
            Event::Rescheduled => "booking.rescheduled",
            Event::Ping => "ping",
        }
    }

    /// `Created` or `Requested`, for a booking that was just inserted.
    pub fn new_booking(pending: bool) -> Self {
        if pending {
            Event::Requested
        } else {
            Event::Created
        }
    }
}

/// Who ended a booking.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Actor {
    Guest,
    Host,
}

impl Actor {
    fn as_str(self) -> &'static str {
        match self {
            Actor::Guest => "guest",
            Actor::Host => "host",
        }
    }
}

/// What the event adds to the booking itself.
#[derive(Clone, Debug, Default)]
pub struct Context {
    pub cancelled_by: Option<Actor>,
    /// The cancellation or decline reason, which the guest is shown too.
    pub reason: Option<String>,
    /// The times before a reschedule, as stored (`start_at`, `end_at`).
    pub previous: Option<(String, String)>,
}

impl Context {
    pub fn cancelled(by: Actor, reason: Option<String>) -> Self {
        Self {
            cancelled_by: Some(by),
            reason: reason.filter(|r| !r.trim().is_empty()),
            ..Self::default()
        }
    }

    pub fn declined(reason: Option<String>) -> Self {
        Self {
            reason: reason.filter(|r| !r.trim().is_empty()),
            ..Self::default()
        }
    }

    pub fn rescheduled(old_start: &str, old_end: &str) -> Self {
        Self {
            previous: Some((old_start.to_string(), old_end.to_string())),
            ..Self::default()
        }
    }
}

// --- Payload ---

#[derive(Serialize, Clone, Debug)]
struct Times {
    start: String,
    end: String,
}

#[derive(Serialize, Clone, Debug)]
struct Guest {
    name: String,
    email: String,
    phone: Option<String>,
    notes: Option<String>,
    timezone: String,
}

#[derive(Serialize, Clone, Debug)]
struct Host {
    name: String,
    email: String,
}

#[derive(Serialize, Clone, Debug)]
struct Location {
    #[serde(rename = "type")]
    kind: String,
    value: Option<String>,
}

#[derive(Serialize, Clone, Debug)]
struct BookingPayload {
    uid: String,
    status: String,
    start: String,
    end: String,
    language: Option<String>,
    created_at: String,
    guest: Guest,
    attendees: Vec<String>,
    hosts: Vec<Host>,
    location: Option<Location>,
    previous: Option<Times>,
    cancelled_by: Option<&'static str>,
    reason: Option<String>,
}

#[derive(Serialize, Clone, Debug)]
struct Team {
    id: String,
    slug: String,
    name: String,
}

#[derive(Serialize, Clone, Debug)]
struct EventTypePayload {
    id: String,
    slug: String,
    title: String,
    timezone: String,
    team: Option<Team>,
}

#[derive(Serialize, Debug)]
struct Envelope<'a> {
    version: u32,
    id: &'a str,
    event: &'static str,
    occurred_at: String,
    test: bool,
    booking: &'a BookingPayload,
    event_type: &'a EventTypePayload,
}

/// The booking as a webhook sees it, read back from the database once per
/// event and shared by every endpoint it goes to.
struct Snapshot {
    event_type_id: String,
    booking: BookingPayload,
    event_type: EventTypePayload,
    /// The event type's zone, to read legacy wall-clock `previous` times.
    tz: Tz,
}

#[derive(sqlx::FromRow)]
struct SnapshotRow {
    uid: String,
    status: String,
    start_at: String,
    end_at: String,
    guest_name: String,
    guest_email: String,
    guest_timezone: String,
    language: Option<String>,
    created_at: String,
    et_id: String,
    et_slug: String,
    et_title: String,
    location_type: Option<String>,
    location_value: Option<String>,
    team_id: Option<String>,
    team_slug: Option<String>,
    team_name: Option<String>,
    owner_id: Option<String>,
}

/// A stored booking time as a UTC instant (`2026-11-10T09:00:00Z`). Rows
/// written before UTC storage hold the wall clock of the event type's zone.
fn utc_iso(value: &str, tz: Tz) -> String {
    crate::booking_time::local(value, tz, Tz::UTC)
        .map(|t| t.format("%Y-%m-%dT%H:%M:%SZ").to_string())
        .unwrap_or_else(|| value.to_string())
}

fn iso(t: DateTime<Utc>) -> String {
    t.format("%Y-%m-%dT%H:%M:%SZ").to_string()
}

async fn snapshot(pool: &SqlitePool, booking_key: &str) -> Option<Snapshot> {
    let row: SnapshotRow = sqlx::query_as(
        "SELECT b.uid, b.status,
                CASE b.time_version WHEN 1 THEN b.start_at ELSE rtrim(b.start_at, 'Z') END AS start_at,
                CASE b.time_version WHEN 1 THEN b.end_at ELSE rtrim(b.end_at, 'Z') END AS end_at,
                b.guest_name, b.guest_email, COALESCE(b.guest_timezone, 'UTC') AS guest_timezone,
                NULLIF(TRIM(b.language), '') AS language, b.created_at,
                et.id AS et_id, et.slug AS et_slug, et.title AS et_title,
                NULLIF(et.location_type, '') AS location_type,
                COALESCE(NULLIF(b.meeting_url, ''), NULLIF(et.location_value, '')) AS location_value,
                t.id AS team_id, t.slug AS team_slug, t.name AS team_name, a.user_id AS owner_id
         FROM bookings b
         JOIN event_types et ON et.id = b.event_type_id
         JOIN accounts a ON a.id = et.account_id
         LEFT JOIN teams t ON t.id = et.team_id
         WHERE b.id = ?1 OR b.uid = ?1",
    )
    .bind(booking_key)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten()?;

    let tz = crate::booking_time::event_timezone(pool, &row.et_id)
        .await
        .unwrap_or(Tz::UTC);
    let (notes, phone) = super::booking_guest_input(pool, &row.uid).await;
    let attendees = super::booking_additional_attendees(pool, &row.uid).await;
    let fallback = row.owner_id.clone().unwrap_or_default();
    let mut hosts = Vec::new();
    for user_id in super::booking_write_targets(pool, &row.uid, &fallback).await {
        if let Some((name, email, _)) = super::host_contact(pool, &user_id).await {
            hosts.push(Host { name, email });
        }
    }
    let team = match (row.team_id, row.team_slug, row.team_name) {
        (Some(id), Some(slug), Some(name)) => Some(Team { id, slug, name }),
        _ => None,
    };
    Some(Snapshot {
        event_type_id: row.et_id.clone(),
        booking: BookingPayload {
            start: utc_iso(&row.start_at, tz),
            end: utc_iso(&row.end_at, tz),
            created_at: utc_iso(&row.created_at, Tz::UTC),
            uid: row.uid,
            status: row.status,
            language: row.language,
            guest: Guest {
                name: row.guest_name,
                email: row.guest_email,
                phone,
                notes,
                timezone: row.guest_timezone,
            },
            attendees,
            hosts,
            location: row.location_type.map(|kind| Location {
                kind,
                value: row.location_value,
            }),
            previous: None,
            cancelled_by: None,
            reason: None,
        },
        event_type: EventTypePayload {
            id: row.et_id,
            slug: row.et_slug,
            title: row.et_title,
            timezone: tz.name().to_string(),
            team,
        },
        tz,
    })
}

/// The JSON body of one delivery, without its per-attempt `timestamp`.
fn render(
    delivery_id: &str,
    event: Event,
    now: DateTime<Utc>,
    snap: &Snapshot,
    ctx: &Context,
) -> String {
    let mut booking = snap.booking.clone();
    booking.previous = ctx.previous.as_ref().map(|(start, end)| Times {
        start: utc_iso(start, snap.tz),
        end: utc_iso(end, snap.tz),
    });
    booking.cancelled_by = ctx.cancelled_by.map(Actor::as_str);
    booking.reason = ctx.reason.clone();
    let envelope = Envelope {
        version: PAYLOAD_VERSION,
        id: delivery_id,
        event: event.name(),
        occurred_at: iso(now),
        test: event == Event::Ping,
        booking: &booking,
        event_type: &snap.event_type,
    };
    serde_json::to_string(&envelope).unwrap_or_default()
}

/// The body of a "Send test" request: a made-up booking with every field
/// filled, so a receiver can be written against it.
fn ping_snapshot(event_type: Option<(String, String, String, Tz)>) -> Snapshot {
    let start = Utc::now() + chrono::Duration::days(1);
    let (id, slug, title, tz) = event_type.unwrap_or_else(|| {
        (
            "00000000-0000-0000-0000-000000000000".to_string(),
            "intro".to_string(),
            "Intro call".to_string(),
            Tz::UTC,
        )
    });
    Snapshot {
        event_type_id: id.clone(),
        booking: BookingPayload {
            uid: "test@calrs".to_string(),
            status: "confirmed".to_string(),
            start: iso(start),
            end: iso(start + chrono::Duration::minutes(30)),
            language: Some("en".to_string()),
            created_at: iso(Utc::now()),
            guest: Guest {
                name: "Test Guest".to_string(),
                email: "guest@example.com".to_string(),
                phone: Some("+15555550100".to_string()),
                notes: Some("This is a test delivery.".to_string()),
                timezone: "UTC".to_string(),
            },
            attendees: vec!["colleague@example.com".to_string()],
            hosts: vec![Host {
                name: "Test Host".to_string(),
                email: "host@example.com".to_string(),
            }],
            location: Some(Location {
                kind: "link".to_string(),
                value: Some("https://meet.example.com/test".to_string()),
            }),
            previous: None,
            cancelled_by: None,
            reason: None,
        },
        event_type: EventTypePayload {
            id,
            slug,
            title,
            timezone: tz.name().to_string(),
            team: None,
        },
        tz,
    }
}

// --- Queue ---

static KICK: tokio::sync::Notify = tokio::sync::Notify::const_new();

/// Queue `event` for every endpoint watching the booking (`booking_key` is
/// its id or uid). Call it after the change has committed. Never fails: a
/// webhook must not cost anyone a booking.
pub async fn enqueue(pool: &SqlitePool, booking_key: &str, event: Event, ctx: Context) {
    // The whole cost of the feature on an instance that does not use it.
    let any: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM webhooks WHERE enabled = 1)")
        .fetch_one(pool)
        .await
        .unwrap_or(false);
    if !any {
        return;
    }
    let Some(snap) = snapshot(pool, booking_key).await else {
        tracing::warn!(booking = %booking_key, event = event.name(), "webhook: booking not found, nothing queued");
        return;
    };
    let endpoints: Vec<String> = sqlx::query_scalar(&format!(
        "SELECT w.id FROM webhooks w JOIN users u ON u.id = w.user_id
         WHERE {ACTIVE_ENDPOINT}
           AND (w.event_type_id = ?1
                OR (w.event_type_id IS NULL AND EXISTS (
                    SELECT 1 FROM event_types et JOIN accounts a ON a.id = et.account_id
                    WHERE et.id = ?1 AND et.team_id IS NULL AND a.user_id = w.user_id)))
         ORDER BY w.created_at, w.id"
    ))
    .bind(&snap.event_type_id)
    .fetch_all(pool)
    .await
    .unwrap_or_default();
    if endpoints.is_empty() {
        return;
    }
    let now = Utc::now();
    for webhook_id in &endpoints {
        let id = uuid::Uuid::new_v4().to_string();
        let payload = render(&id, event, now, &snap, &ctx);
        let inserted = sqlx::query(
            "INSERT INTO webhook_deliveries (id, webhook_id, booking_uid, event, payload, next_attempt_at)
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(webhook_id)
        .bind(&snap.booking.uid)
        .bind(event.name())
        .bind(&payload)
        .bind(iso(now))
        .execute(pool)
        .await;
        if let Err(e) = inserted {
            tracing::warn!(error = %e, webhook = %webhook_id, "webhook: delivery not queued");
        }
    }
    tracing::debug!(booking = %snap.booking.uid, event = event.name(), endpoints = endpoints.len(), "webhook deliveries queued");
    KICK.notify_one();
}

// --- Target checks ---

/// Why a URL cannot be used. `key()` is the message shown on the form.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TargetError {
    Invalid,
    HttpsRequired,
    Credentials,
    Private(String),
    Unresolvable(String),
}

impl TargetError {
    pub fn key(&self) -> &'static str {
        match self {
            TargetError::Invalid => "webhooks-error-url",
            TargetError::HttpsRequired => "webhooks-error-https",
            TargetError::Credentials => "webhooks-error-credentials",
            TargetError::Private(_) => "webhooks-error-private",
            TargetError::Unresolvable(_) => "webhooks-error-resolve",
        }
    }
}

impl std::fmt::Display for TargetError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TargetError::Invalid => write!(f, "not a valid URL"),
            TargetError::HttpsRequired => write!(f, "only https URLs are allowed"),
            TargetError::Credentials => write!(f, "credentials in the URL are not allowed"),
            TargetError::Private(ip) => {
                write!(f, "resolves to a private or reserved address ({ip})")
            }
            TargetError::Unresolvable(e) => write!(f, "cannot resolve the host: {e}"),
        }
    }
}

/// A URL that passed [`check_target`], with the addresses it was checked
/// against when they have to be pinned.
#[derive(Debug)]
pub struct Target {
    url: reqwest::Url,
    pinned: Option<(String, Vec<SocketAddr>)>,
}

/// The shape checks that need no network: absolute https URL (http only for
/// an allowlisted host), no credentials, a host. Returns the parsed URL and
/// whether its host is on the operator's private-host allowlist.
pub fn check_url_shape(
    raw: &str,
    allowlist: &[String],
) -> Result<(reqwest::Url, bool), TargetError> {
    let raw = raw.trim();
    if raw.is_empty() || raw.len() > URL_MAX_LEN {
        return Err(TargetError::Invalid);
    }
    let url = reqwest::Url::parse(raw).map_err(|_| TargetError::Invalid)?;
    let host = url
        .host_str()
        .filter(|h| !h.is_empty())
        .ok_or(TargetError::Invalid)?
        .trim_start_matches('[')
        .trim_end_matches(']')
        .to_ascii_lowercase();
    let allowlisted = allowlist.contains(&host);
    match url.scheme() {
        "https" => {}
        "http" if allowlisted => {}
        "http" => return Err(TargetError::HttpsRequired),
        _ => return Err(TargetError::Invalid),
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(TargetError::Credentials);
    }
    Ok((url, allowlisted))
}

/// Full check of a webhook URL, run when it is saved and again before every
/// delivery: [`check_url_shape`], then every address the host resolves to
/// must be public unless the host is allowlisted. The checked addresses are
/// returned for the request to connect to, so a second DNS answer cannot
/// point it somewhere else.
pub async fn check_target(raw: &str, allowlist: &[String]) -> Result<Target, TargetError> {
    let (url, allowlisted) = check_url_shape(raw, allowlist)?;
    if allowlisted {
        return Ok(Target { url, pinned: None });
    }
    let host = url.host_str().unwrap_or_default().to_string();
    let bare = host.trim_start_matches('[').trim_end_matches(']');
    if let Ok(ip) = bare.parse::<IpAddr>() {
        if crate::caldav::is_private_ip(&ip) {
            return Err(TargetError::Private(ip.to_string()));
        }
        return Ok(Target { url, pinned: None });
    }
    let port = url.port_or_known_default().unwrap_or(443);
    let addrs: Vec<SocketAddr> = tokio::time::timeout(
        CONNECT_TIMEOUT,
        tokio::net::lookup_host((host.as_str(), port)),
    )
    .await
    .map_err(|_| TargetError::Unresolvable("timed out".to_string()))?
    .map_err(|e| TargetError::Unresolvable(e.to_string()))?
    .collect();
    if addrs.is_empty() {
        return Err(TargetError::Unresolvable("no address".to_string()));
    }
    if let Some(bad) = addrs.iter().find(|a| crate::caldav::is_private_ip(&a.ip())) {
        return Err(TargetError::Private(bad.ip().to_string()));
    }
    Ok(Target {
        url,
        pinned: Some((host, addrs)),
    })
}

// --- Delivery ---

/// Outcome of one HTTP attempt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attempt {
    /// The HTTP status, `None` when no response came back.
    pub status: Option<u16>,
    /// `None` on a 2xx answer.
    pub error: Option<String>,
}

impl Attempt {
    pub fn ok(&self) -> bool {
        self.error.is_none()
    }

    fn failed(status: Option<u16>, error: impl Into<String>) -> Self {
        Self {
            status,
            error: Some(error.into()),
        }
    }
}

/// Keep printable text only, cut to [`ERROR_SNIPPET_CHARS`].
fn snippet(text: &str) -> String {
    let clean: String = text
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    let clean = clean.trim();
    if clean.chars().count() > ERROR_SNIPPET_CHARS {
        let cut: String = clean.chars().take(ERROR_SNIPPET_CHARS).collect();
        format!("{cut}...")
    } else {
        clean.to_string()
    }
}

/// Add the per-attempt `timestamp` to a stored body.
fn stamped_body(payload: &str, now: DateTime<Utc>) -> Option<Vec<u8>> {
    let mut value: serde_json::Value = serde_json::from_str(payload).ok()?;
    value
        .as_object_mut()?
        .insert("timestamp".to_string(), now.timestamp().into());
    serde_json::to_vec(&value).ok()
}

/// One signed POST. Checks the target first, every time.
async fn post(
    url: &str,
    secret: &str,
    delivery_id: &str,
    event: &str,
    body: Vec<u8>,
    allowlist: &[String],
) -> Attempt {
    let target = match check_target(url, allowlist).await {
        Ok(t) => t,
        Err(e) => return Attempt::failed(None, format!("refused: {e}")),
    };
    let mut builder = reqwest::Client::builder()
        .timeout(REQUEST_TIMEOUT)
        .connect_timeout(CONNECT_TIMEOUT)
        .redirect(reqwest::redirect::Policy::none());
    if let Some((host, addrs)) = &target.pinned {
        builder = builder.resolve_to_addrs(host, addrs);
    }
    let client = match builder.build() {
        Ok(c) => c,
        Err(e) => return Attempt::failed(None, format!("client: {e}")),
    };
    let signature = super::meeting::sign_hmac_sha256(secret.as_bytes(), &body);
    let response = client
        .post(target.url)
        .header("content-type", "application/json")
        .header(
            "user-agent",
            concat!("calrs-webhook/", env!("CARGO_PKG_VERSION")),
        )
        .header("X-Calrs-Event", event)
        .header("X-Calrs-Delivery", delivery_id)
        .header("X-Calrs-Signature", format!("sha256={signature}"))
        .body(body)
        .send()
        .await;
    let mut response = match response {
        Ok(r) => r,
        Err(e) => {
            let what = if e.is_timeout() {
                "timed out".to_string()
            } else if e.is_connect() {
                "connection failed".to_string()
            } else {
                e.to_string()
            };
            return Attempt::failed(None, snippet(&what));
        }
    };
    let status = response.status();
    if status.is_success() {
        return Attempt {
            status: Some(status.as_u16()),
            error: None,
        };
    }
    // Read a little of the answer for the error line, never the whole body.
    let mut raw = Vec::new();
    while raw.len() < ERROR_BODY_READ_BYTES {
        match tokio::time::timeout(Duration::from_secs(2), response.chunk()).await {
            Ok(Ok(Some(chunk))) => raw.extend_from_slice(&chunk),
            _ => break,
        }
    }
    raw.truncate(ERROR_BODY_READ_BYTES);
    let text = snippet(&String::from_utf8_lossy(&raw));
    let message = if status.is_redirection() {
        format!("HTTP {} (redirects are not followed)", status.as_u16())
    } else if text.is_empty() {
        format!("HTTP {}", status.as_u16())
    } else {
        format!("HTTP {}: {}", status.as_u16(), text)
    };
    Attempt::failed(Some(status.as_u16()), message)
}

/// Store the latest outcome on the endpoint, for its row on the page.
async fn record_endpoint(pool: &SqlitePool, webhook_id: &str, attempt: &Attempt, now: &str) {
    let _ = sqlx::query(
        "UPDATE webhooks SET last_attempt_at = ?1, last_status = ?2, last_error = ?3,
                last_success_at = CASE WHEN ?3 IS NULL THEN ?1 ELSE last_success_at END
         WHERE id = ?4",
    )
    .bind(now)
    .bind(attempt.status.map(i64::from))
    .bind(&attempt.error)
    .bind(webhook_id)
    .execute(pool)
    .await;
}

/// When to try again after `attempts` failed attempts, `None` once they are
/// used up.
fn next_attempt_at(attempts: i64, now: DateTime<Utc>) -> Option<DateTime<Utc>> {
    let idx = usize::try_from(attempts.checked_sub(1)?).ok()?;
    RETRY_DELAYS_MIN
        .get(idx)
        .map(|m| now + chrono::Duration::minutes(*m))
}

#[derive(sqlx::FromRow)]
struct Due {
    id: String,
    webhook_id: String,
    event: String,
    payload: String,
    attempts: i64,
    url: String,
    secret_enc: String,
}

async fn deliver_one(
    pool: &SqlitePool,
    key: &[u8; 32],
    due: &Due,
    now: DateTime<Utc>,
    allowlist: &[String],
) -> bool {
    let attempt = match (
        crate::crypto::decrypt_value(key, &due.secret_enc),
        stamped_body(&due.payload, now),
    ) {
        (Ok(secret), Some(body)) => {
            post(&due.url, &secret, &due.id, &due.event, body, allowlist).await
        }
        (Err(_), _) => Attempt::failed(None, "the signing secret cannot be read"),
        (_, None) => Attempt::failed(None, "stored payload is unreadable"),
    };
    let now_s = iso(now);
    record_endpoint(pool, &due.webhook_id, &attempt, &now_s).await;
    if attempt.ok() {
        let _ = sqlx::query("DELETE FROM webhook_deliveries WHERE id = ?")
            .bind(&due.id)
            .execute(pool)
            .await;
        tracing::debug!(delivery = %due.id, event = %due.event, "webhook delivered");
        return true;
    }
    let attempts = due.attempts + 1;
    let error = attempt.error.unwrap_or_default();
    match next_attempt_at(attempts, now) {
        Some(next) => {
            let _ = sqlx::query(
                "UPDATE webhook_deliveries SET attempts = ?, next_attempt_at = ?, last_error = ? WHERE id = ?",
            )
            .bind(attempts)
            .bind(iso(next))
            .bind(&error)
            .bind(&due.id)
            .execute(pool)
            .await;
        }
        None => {
            let _ = sqlx::query(
                "UPDATE webhook_deliveries SET state = 'failed', attempts = ?, last_error = ? WHERE id = ?",
            )
            .bind(attempts)
            .bind(&error)
            .bind(&due.id)
            .execute(pool)
            .await;
            tracing::warn!(delivery = %due.id, webhook = %due.webhook_id, error = %error, "webhook delivery gave up");
        }
    }
    tracing::info!(delivery = %due.id, webhook = %due.webhook_id, attempts, error = %error, "webhook delivery failed");
    false
}

/// Deliver everything that is due. Endpoints are served concurrently, each
/// one's deliveries in order; an endpoint that fails is left alone for the
/// rest of the pass. A booking's later events wait for its earlier ones (a
/// receiver never sees "cancelled" before "created"), until those succeed
/// or run out of attempts. Returns the number delivered.
pub(crate) async fn deliver_due_with(
    pool: &SqlitePool,
    key: &[u8; 32],
    now: DateTime<Utc>,
    allowlist: &[String],
) -> usize {
    // A success can release the next event of the same booking, held back
    // by the ordering rule: go again until a pass delivers nothing. Every
    // success deletes a row, so this ends.
    let mut total = 0;
    loop {
        let delivered = deliver_pass(pool, key, now, allowlist).await;
        if delivered == 0 {
            return total;
        }
        total += delivered;
    }
}

async fn deliver_pass(
    pool: &SqlitePool,
    key: &[u8; 32],
    now: DateTime<Utc>,
    allowlist: &[String],
) -> usize {
    let due: Vec<Due> = sqlx::query_as(&format!(
        "SELECT d.id, d.webhook_id, d.event, d.payload, d.attempts, w.url, w.secret_enc
         FROM webhook_deliveries d
         JOIN webhooks w ON w.id = d.webhook_id
         JOIN users u ON u.id = w.user_id
         WHERE d.state = 'pending' AND d.next_attempt_at <= ? AND {ACTIVE_ENDPOINT}
           AND NOT EXISTS (
               SELECT 1 FROM webhook_deliveries o
               WHERE o.webhook_id = d.webhook_id AND o.booking_uid = d.booking_uid
                 AND o.state = 'pending' AND o.seq < d.seq)
         ORDER BY d.seq
         LIMIT ?"
    ))
    .bind(iso(now))
    .bind(BATCH)
    .fetch_all(pool)
    .await
    .unwrap_or_default();
    if due.is_empty() {
        return 0;
    }
    let mut by_endpoint: Vec<(String, Vec<Due>)> = Vec::new();
    for d in due {
        match by_endpoint.iter_mut().find(|(w, _)| *w == d.webhook_id) {
            Some((_, list)) => list.push(d),
            None => by_endpoint.push((d.webhook_id.clone(), vec![d])),
        }
    }
    let mut tasks = tokio::task::JoinSet::new();
    for (_, list) in by_endpoint {
        let (pool, key, allowlist) = (pool.clone(), *key, allowlist.to_vec());
        tasks.spawn(async move {
            let mut delivered = 0;
            for d in &list {
                if !deliver_one(&pool, &key, d, now, &allowlist).await {
                    break;
                }
                delivered += 1;
            }
            delivered
        });
    }
    let mut delivered = 0;
    while let Some(result) = tasks.join_next().await {
        delivered += result.unwrap_or(0);
    }
    delivered
}

/// Drop deliveries that ran out of attempts a week ago.
pub(crate) async fn prune(pool: &SqlitePool, now: DateTime<Utc>) {
    let cutoff = (now - chrono::Duration::days(FAILED_RETENTION_DAYS))
        .format("%Y-%m-%d %H:%M:%S")
        .to_string();
    let _ = sqlx::query("DELETE FROM webhook_deliveries WHERE state = 'failed' AND created_at < ?")
        .bind(cutoff)
        .execute(pool)
        .await;
}

/// The delivery task spawned by `calrs serve`. Wakes when a delivery is
/// queued, and every 30 seconds for the retries. Deliveries queued by the
/// CLI are picked up by that tick.
pub async fn run_webhook_loop(pool: SqlitePool, key: [u8; 32]) {
    let mut last_prune = std::time::Instant::now();
    loop {
        tokio::select! {
            _ = KICK.notified() => {}
            _ = tokio::time::sleep(Duration::from_secs(30)) => {}
        }
        let allowlist = crate::settings::private_host_allowlist();
        deliver_due_with(&pool, &key, Utc::now(), &allowlist).await;
        if last_prune.elapsed() >= Duration::from_secs(3600) {
            prune(&pool, Utc::now()).await;
            last_prune = std::time::Instant::now();
        }
    }
}

/// The "Send test" button: one signed `ping` to the endpoint, sent now and
/// not queued, its outcome recorded on the endpoint like a delivery's.
pub(crate) async fn send_test_with(
    pool: &SqlitePool,
    key: &[u8; 32],
    webhook_id: &str,
    allowlist: &[String],
) -> Option<Attempt> {
    let (url, secret_enc, event_type_id): (String, String, Option<String>) =
        sqlx::query_as("SELECT url, secret_enc, event_type_id FROM webhooks WHERE id = ?")
            .bind(webhook_id)
            .fetch_optional(pool)
            .await
            .ok()
            .flatten()?;
    let event_type = match event_type_id {
        Some(id) => {
            let row: Option<(String, String)> =
                sqlx::query_as("SELECT slug, title FROM event_types WHERE id = ?")
                    .bind(&id)
                    .fetch_optional(pool)
                    .await
                    .ok()
                    .flatten();
            let tz = crate::booking_time::event_timezone(pool, &id)
                .await
                .unwrap_or(Tz::UTC);
            row.map(|(slug, title)| (id, slug, title, tz))
        }
        None => None,
    };
    let now = Utc::now();
    let id = uuid::Uuid::new_v4().to_string();
    let payload = render(
        &id,
        Event::Ping,
        now,
        &ping_snapshot(event_type),
        &Context::default(),
    );
    let attempt = match (
        crate::crypto::decrypt_value(key, &secret_enc),
        stamped_body(&payload, now),
    ) {
        (Ok(secret), Some(body)) => {
            post(&url, &secret, &id, Event::Ping.name(), body, allowlist).await
        }
        _ => Attempt::failed(None, "the signing secret cannot be read"),
    };
    record_endpoint(pool, webhook_id, &attempt, &iso(now)).await;
    Some(attempt)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_names_are_stable() {
        let names: Vec<&str> = [
            Event::Created,
            Event::Requested,
            Event::Confirmed,
            Event::Declined,
            Event::Expired,
            Event::Cancelled,
            Event::Rescheduled,
            Event::Ping,
        ]
        .iter()
        .map(|e| e.name())
        .collect();
        assert_eq!(
            names,
            [
                "booking.created",
                "booking.requested",
                "booking.confirmed",
                "booking.declined",
                "booking.expired",
                "booking.cancelled",
                "booking.rescheduled",
                "ping"
            ]
        );
    }

    #[test]
    fn retries_are_bounded() {
        let now = Utc::now();
        assert_eq!(
            next_attempt_at(1, now),
            Some(now + chrono::Duration::minutes(1))
        );
        assert_eq!(
            next_attempt_at(6, now),
            Some(now + chrono::Duration::minutes(720))
        );
        assert_eq!(next_attempt_at(MAX_ATTEMPTS, now), None);
        assert_eq!(next_attempt_at(0, now), None);
    }

    #[test]
    fn url_shape_requires_https_unless_allowlisted() {
        let none: Vec<String> = Vec::new();
        assert!(check_url_shape("https://hooks.example.com/x", &none).is_ok());
        assert_eq!(
            check_url_shape("http://hooks.example.com/x", &none).unwrap_err(),
            TargetError::HttpsRequired
        );
        let allow = vec!["n8n".to_string()];
        assert!(check_url_shape("http://n8n:5678/webhook/abc", &allow).is_ok());
        assert_eq!(
            check_url_shape("ftp://hooks.example.com/x", &allow).unwrap_err(),
            TargetError::Invalid
        );
        assert_eq!(
            check_url_shape("https://user:pw@hooks.example.com/x", &none).unwrap_err(),
            TargetError::Credentials
        );
        assert_eq!(
            check_url_shape("not a url", &none).unwrap_err(),
            TargetError::Invalid
        );
        let long = format!("https://hooks.example.com/{}", "a".repeat(URL_MAX_LEN));
        assert_eq!(
            check_url_shape(&long, &none).unwrap_err(),
            TargetError::Invalid
        );
    }

    #[tokio::test]
    async fn private_addresses_are_refused_unless_allowlisted() {
        let none: Vec<String> = Vec::new();
        for url in [
            "https://127.0.0.1/hook",
            "https://10.0.0.5/hook",
            "https://169.254.169.254/latest",
            "https://[::1]/hook",
            "https://localhost/hook",
        ] {
            match check_target(url, &none).await {
                Err(TargetError::Private(_)) => {}
                other => panic!("{url} should be refused as private, got {other:?}"),
            }
        }
        assert!(check_target("https://93.184.215.14/hook", &none)
            .await
            .is_ok());
        let allow = vec!["127.0.0.1".to_string()];
        assert!(check_target("http://127.0.0.1:8080/hook", &allow)
            .await
            .is_ok());
    }

    #[test]
    fn utc_iso_reads_both_storage_formats() {
        let paris: Tz = "Europe/Paris".parse().unwrap();
        assert_eq!(
            utc_iso("2026-07-01T14:00:00Z", paris),
            "2026-07-01T14:00:00Z"
        );
        // Legacy rows hold the event type's wall clock.
        assert_eq!(
            utc_iso("2026-07-01T14:00:00", paris),
            "2026-07-01T12:00:00Z"
        );
        assert_eq!(
            utc_iso("2026-07-01 14:00:00", Tz::UTC),
            "2026-07-01T14:00:00Z"
        );
    }

    #[test]
    fn snippet_strips_control_characters_and_truncates() {
        assert_eq!(snippet(" a\nb\tc "), "a b c");
        let long = "x".repeat(500);
        let cut = snippet(&long);
        assert!(cut.ends_with("..."));
        assert_eq!(cut.chars().count(), ERROR_SNIPPET_CHARS + 3);
    }

    #[test]
    fn stamped_body_adds_a_timestamp() {
        let now = Utc::now();
        let body = stamped_body(r#"{"id":"x","version":1}"#, now).unwrap();
        let value: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(value["timestamp"], now.timestamp());
        assert_eq!(value["id"], "x");
        assert!(stamped_body("not json", now).is_none());
    }

    #[test]
    fn ping_payload_carries_every_field() {
        let snap = ping_snapshot(None);
        let body = render("d-1", Event::Ping, Utc::now(), &snap, &Context::default());
        let v: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(v["version"], 1);
        assert_eq!(v["id"], "d-1");
        assert_eq!(v["event"], "ping");
        assert_eq!(v["test"], true);
        for field in [
            "uid",
            "status",
            "start",
            "end",
            "language",
            "created_at",
            "guest",
            "attendees",
            "hosts",
            "location",
            "previous",
            "cancelled_by",
            "reason",
        ] {
            assert!(v["booking"].get(field).is_some(), "booking.{field} missing");
        }
        for field in ["id", "slug", "title", "timezone", "team"] {
            assert!(
                v["event_type"].get(field).is_some(),
                "event_type.{field} missing"
            );
        }
    }
}
