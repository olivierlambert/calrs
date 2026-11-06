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
pub mod graph;

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
    /// The account cannot write to it, so it is never offered for write-back.
    /// Graph: `canEdit` is false. CalDAV and EWS: not reported.
    pub read_only: bool,
    /// Owned by someone else and shared with the account. Graph: the owner
    /// differs from the default calendar's. CalDAV and EWS: not reported.
    pub shared: bool,
}

/// A raw calendar event as iCalendar (RFC 5545) text plus its remote handle.
///
/// `remote_id` is whatever the provider uses to address the item:
/// CalDAV: the resource href (`/calendars/alice/work/UID.ics`).
/// EWS:    the ItemId.
///
/// An empty `ical` marks an item the provider listed but could not convert
/// (see [`RawEvent::unreadable`]): the sync skips it, and removes nothing
/// from that calendar's cache until a snapshot without one.
#[derive(Debug, Clone)]
pub struct RawEvent {
    pub remote_id: String,
    pub ical: String,
}

impl RawEvent {
    /// An item the provider listed but could not convert to iCalendar.
    pub fn unreadable(remote_id: String) -> Self {
        Self {
            remote_id,
            ical: String::new(),
        }
    }
}

/// A listed calendar the granted access cannot read (a colleague's or a
/// room's calendar shared with the user, say). The sync skips it, unless it
/// is the source's write calendar, rather than failing the whole source.
#[derive(Debug)]
pub struct CalendarUnreadable;

impl std::fmt::Display for CalendarUnreadable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("this calendar cannot be read with the access granted to calrs")
    }
}

impl std::error::Error for CalendarUnreadable {}

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

    /// Whether an event with this UID can be read back from the calendar.
    ///
    /// `Ok(true)` when the server confirms it is there, `Ok(false)` when the
    /// server confirms it is not, `Err` when it cannot tell (network error,
    /// 5xx, auth failure). Used to verify a write-back (#162): a 2xx on the
    /// write alone is not proof the host will see the event.
    async fn event_exists(&self, calendar_id: &str, uid: &str) -> Result<bool>;
}

/// Why a calendar write-back did not land (#162).
#[derive(Debug)]
pub enum WriteBackError {
    /// The write itself was refused or never reached the server.
    Put(anyhow::Error),
    /// The server accepted the write, but the event is not there when read
    /// back by UID.
    NotReadable,
    /// The server accepted the write, and the read-back could not tell
    /// whether the event is there.
    Verify(anyhow::Error),
}

impl WriteBackError {
    /// Whether the event may be on the calendar despite the failure, so a
    /// later cancellation should still try to delete it.
    pub fn event_may_exist(&self) -> bool {
        !matches!(self, WriteBackError::Put(_))
    }
}

impl std::fmt::Display for WriteBackError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WriteBackError::Put(e) => write!(f, "{e:#}"),
            WriteBackError::NotReadable => {
                write!(
                    f,
                    "the server accepted the event but it is not readable back"
                )
            }
            WriteBackError::Verify(e) => {
                write!(f, "the event was written but could not be verified: {e:#}")
            }
        }
    }
}

/// Write an event and read it back by UID. The write is a create-or-replace
/// keyed on the UID, so calling this again for the same booking (a retry)
/// replaces the event rather than adding a second one.
pub async fn put_and_verify(
    provider: &dyn CalendarProvider,
    calendar_id: &str,
    uid: &str,
    ics: &str,
) -> std::result::Result<(), WriteBackError> {
    provider
        .put_event(calendar_id, uid, ics)
        .await
        .map_err(WriteBackError::Put)?;
    match provider.event_exists(calendar_id, uid).await {
        Ok(true) => Ok(()),
        Ok(false) => Err(WriteBackError::NotReadable),
        Err(e) => Err(WriteBackError::Verify(e)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// In-memory provider: `put_fails` refuses writes, `drop_writes`
    /// accepts them and keeps nothing, `exists_fails` makes the read-back
    /// error out.
    #[derive(Default)]
    struct FakeProvider {
        put_fails: bool,
        drop_writes: bool,
        exists_fails: bool,
        stored: Mutex<Vec<String>>,
    }

    #[async_trait]
    impl CalendarProvider for FakeProvider {
        async fn check_connection(&self) -> Result<bool> {
            Ok(true)
        }
        async fn list_calendars(&self) -> Result<Vec<RemoteCalendar>> {
            Ok(vec![])
        }
        async fn fetch_events(&self, _: &str) -> Result<Vec<RawEvent>> {
            Ok(vec![])
        }
        async fn fetch_events_since(&self, _: &str, _: &str) -> Result<Vec<RawEvent>> {
            Ok(vec![])
        }
        async fn sync_delta(&self, _: &str, _: Option<&str>) -> Result<DeltaResult> {
            Ok(DeltaResult::default())
        }
        async fn put_event(&self, _: &str, uid: &str, _: &str) -> Result<()> {
            if self.put_fails {
                anyhow::bail!("PUT returned 400 Bad Request");
            }
            if !self.drop_writes {
                let mut stored = self.stored.lock().unwrap();
                if !stored.iter().any(|u| u == uid) {
                    stored.push(uid.to_string());
                }
            }
            Ok(())
        }
        async fn delete_event(&self, _: &str, uid: &str) -> Result<()> {
            self.stored.lock().unwrap().retain(|u| u != uid);
            Ok(())
        }
        async fn event_exists(&self, _: &str, uid: &str) -> Result<bool> {
            if self.exists_fails {
                anyhow::bail!("HEAD returned 503");
            }
            Ok(self.stored.lock().unwrap().iter().any(|u| u == uid))
        }
    }

    #[tokio::test]
    async fn verified_write_succeeds_and_is_idempotent() {
        let p = FakeProvider::default();
        put_and_verify(&p, "/cal/", "uid-1", "ics").await.unwrap();
        put_and_verify(&p, "/cal/", "uid-1", "ics").await.unwrap();
        assert_eq!(
            p.stored.lock().unwrap().len(),
            1,
            "a retry must not duplicate"
        );
    }

    #[tokio::test]
    async fn refused_write_is_a_put_failure() {
        let p = FakeProvider {
            put_fails: true,
            ..Default::default()
        };
        let err = put_and_verify(&p, "/cal/", "uid-1", "ics")
            .await
            .unwrap_err();
        assert!(matches!(err, WriteBackError::Put(_)));
        assert!(!err.event_may_exist());
        assert!(err.to_string().contains("400"));
    }

    /// The #162 case: the server says 2xx and the event is not there.
    #[tokio::test]
    async fn accepted_but_missing_write_fails_verification() {
        let p = FakeProvider {
            drop_writes: true,
            ..Default::default()
        };
        let err = put_and_verify(&p, "/cal/", "uid-1", "ics")
            .await
            .unwrap_err();
        assert!(matches!(err, WriteBackError::NotReadable));
        assert!(err.event_may_exist());
    }

    #[tokio::test]
    async fn unverifiable_write_counts_as_failed() {
        let p = FakeProvider {
            exists_fails: true,
            ..Default::default()
        };
        let err = put_and_verify(&p, "/cal/", "uid-1", "ics")
            .await
            .unwrap_err();
        assert!(matches!(err, WriteBackError::Verify(_)));
        assert!(err.to_string().contains("503"));
    }
}
