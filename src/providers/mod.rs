//! Calendar provider abstraction.
//!
//! `calrs` historically only spoke CalDAV. This module introduces a thin
//! `CalendarProvider` trait so other back-ends (EWS, Microsoft Graph, …) can
//! plug in without touching sync, booking, or write-back code.
//!
//! Each provider hides its own protocol details behind operations expressed in
//! generic terms (a calendar identifier is an opaque string, an event is raw
//! iCalendar text). Sync state and change markers are also opaque, so each
//! provider can use whatever incremental-sync mechanism it has natively
//! (`sync-token` for CalDAV, `SyncState` for EWS).

use anyhow::Result;
use async_trait::async_trait;

pub mod caldav;
pub mod factory;

pub use factory::build_provider;

/// A calendar discovered on a remote provider.
///
/// `id` is the opaque identifier the provider uses to address the calendar:
/// for CalDAV it is the HTTP href (e.g. `/calendars/alice/work/`), for EWS it
/// is the folder Id (a long Base64-ish string). Callers must not parse it.
#[derive(Debug, Clone)]
pub struct RemoteCalendar {
    pub id: String,
    pub display_name: Option<String>,
    pub color: Option<String>,
    /// Provider-specific change marker for fast skip-if-unchanged.
    /// CalDAV: ctag. EWS: not used.
    pub change_marker: Option<String>,
    /// Sync state cursor for incremental sync. Treat as opaque.
    /// CalDAV: WebDAV sync-token (RFC 6578). EWS: SyncFolderItems sync state.
    pub sync_state: Option<String>,
}

/// A raw calendar event as iCalendar (RFC 5545) text plus its remote handle.
///
/// `remote_id` is whatever the provider uses to address the item:
/// CalDAV: the resource href (`/calendars/alice/work/UID.ics`).
/// EWS:    the ItemId.
#[derive(Debug, Clone)]
pub struct RawEvent {
    pub remote_id: String,
    pub ical: String,
}

/// Outcome of a delta sync (incremental fetch).
#[derive(Debug, Clone, Default)]
pub struct DeltaResult {
    pub added_or_changed: Vec<RawEvent>,
    /// UIDs of events deleted on the remote server (best-effort: providers that
    /// can't surface the iCal UID for deletions return remote ids instead — see
    /// the CalDAV adapter for details).
    pub deleted_uids: Vec<String>,
    pub new_sync_state: Option<String>,
}

/// Common operations every calendar back-end must support.
///
/// All methods are best-effort: a provider that genuinely cannot honour an
/// operation (e.g. delta sync on a server that lacks it) should fall back to a
/// sane default (full fetch, empty delta, …) rather than fail loudly.
#[async_trait]
pub trait CalendarProvider: Send + Sync {
    /// Verify the provider can be reached and the credentials are accepted.
    /// Returns `Ok(true)` when calendar features are explicitly advertised,
    /// `Ok(false)` when the connection succeeded but support is uncertain.
    async fn check_connection(&self) -> Result<bool>;

    /// Discover calendars/folders the authenticated user has access to.
    async fn list_calendars(&self) -> Result<Vec<RemoteCalendar>>;

    /// Fetch every event in a calendar (full snapshot).
    async fn fetch_events(&self, calendar_id: &str) -> Result<Vec<RawEvent>>;

    /// Fetch events with start time at or after `since_utc` (RFC 3339).
    /// Implementations that can't filter by time fall back to `fetch_events`.
    async fn fetch_events_since(&self, calendar_id: &str, since_utc: &str)
        -> Result<Vec<RawEvent>>;

    /// Incremental sync from a previous sync state.
    ///
    /// `sync_state = None` means **seed the cursor**: callers use this to
    /// obtain a starting sync state after a full fetch, *not* to retrieve
    /// items. Implementations may therefore return an empty
    /// `added_or_changed` when `sync_state` is `None` — the caller has
    /// already populated the local cache through `fetch_events`.
    async fn sync_delta(&self, calendar_id: &str, sync_state: Option<&str>) -> Result<DeltaResult>;

    /// Create-or-replace an event. `uid` is the iCalendar UID, `ics` is the
    /// full VCALENDAR/VEVENT block.
    async fn put_event(&self, calendar_id: &str, uid: &str, ics: &str) -> Result<()>;

    /// Delete an event by UID.
    async fn delete_event(&self, calendar_id: &str, uid: &str) -> Result<()>;
}
