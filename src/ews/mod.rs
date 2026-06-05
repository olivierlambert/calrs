//! Microsoft Exchange Web Services (EWS) calendar provider.
//!
//! Targets on-prem **Exchange 2019** and earlier (2016, 2013) which all speak
//! the same SOAP protocol at `<host>/EWS/Exchange.asmx`. The implementation
//! intentionally keeps surface area minimal: we discover calendar folders,
//! fetch / write events, and run delta sync — nothing more. Anything more
//! exotic (free/busy of other users, room booking, delegate access) belongs
//! in a follow-up PR.
//!
//! ## Authentication
//!
//! HTTP Basic over TLS. NTLM and Kerberos are common in on-prem environments
//! but require additional crates (`reqwest` does not natively negotiate
//! either). For now, admins should either enable Basic on a service mailbox
//! or place a reverse proxy in front that handles the negotiate handshake.
//! See `docs/ews.md` (planned) for setup details.
//!
//! ## Layout
//!
//! - `autodiscover` — POX Autodiscover lookup so users can configure a source
//!   with just an email address.
//! - `soap` — envelope wrapping, basic auth, response parsing helpers.
//! - `operations` — typed wrappers for FindFolder, FindItem, GetItem,
//!   CreateItem, DeleteItem, SyncFolderItems.
//! - `parse` — XML response decoders.
//! - `ical` — synthesise an iCalendar block from EWS structured fields, used
//!   when MIME content is unavailable.
//!
//! The public surface is [`EwsProvider`], which implements
//! [`crate::providers::CalendarProvider`].

pub mod autodiscover;
pub mod ical;
pub mod operations;
pub mod parse;
pub mod soap;

use anyhow::Result;
use async_trait::async_trait;
use std::collections::HashMap;

use crate::providers::{CalendarProvider, DeltaResult, RawEvent, RemoteCalendar};

/// EWS-backed calendar provider. Constructed from the SOAP endpoint URL plus
/// credentials. Designed to be cheap to clone — no HTTP client cached on the
/// instance because each request rebuilds one with the appropriate timeout.
pub struct EwsProvider {
    endpoint: String,
    username: String,
    password: String,
}

impl EwsProvider {
    pub fn new(endpoint: &str, username: &str, password: &str) -> Self {
        Self {
            endpoint: endpoint.trim_end_matches('/').to_string(),
            username: username.to_string(),
            password: password.to_string(),
        }
    }

    /// Validate the URL the same way the CalDAV path does (HTTPS only,
    /// no SSRF-prone hostnames). Re-exported here so the source-add flow can
    /// validate before persisting.
    pub fn validate_url(url: &str) -> Result<()> {
        crate::caldav::validate_caldav_url(url)
    }
}

#[async_trait]
impl CalendarProvider for EwsProvider {
    async fn check_connection(&self) -> Result<bool> {
        operations::check_connection(&self.endpoint, &self.username, &self.password).await
    }

    async fn list_calendars(&self) -> Result<Vec<RemoteCalendar>> {
        let folders =
            operations::list_calendar_folders(&self.endpoint, &self.username, &self.password)
                .await?;
        Ok(folders
            .into_iter()
            .map(|f| RemoteCalendar {
                id: f.id,
                display_name: f.display_name,
                color: None,
                change_marker: f.change_key,
                sync_state: None,
            })
            .collect())
    }

    async fn fetch_events(&self, calendar_id: &str) -> Result<Vec<RawEvent>> {
        let items =
            operations::list_items(&self.endpoint, &self.username, &self.password, calendar_id)
                .await?;
        Ok(synth_raw_events(items))
    }

    async fn fetch_events_since(
        &self,
        calendar_id: &str,
        since_utc: &str,
    ) -> Result<Vec<RawEvent>> {
        // CalendarView wants both endpoints; pick a generous upper bound far
        // enough out to cover every booking horizon calrs supports today.
        // The 2-year window matches what the slot picker exposes — anything
        // beyond that is going to be replaced by a fresh sync long before it
        // becomes relevant.
        let end_utc = upper_bound_iso(since_utc);
        let items = operations::list_items_in_window(
            &self.endpoint,
            &self.username,
            &self.password,
            calendar_id,
            since_utc,
            &end_utc,
        )
        .await?;
        Ok(synth_raw_events(items))
    }

    async fn sync_delta(&self, calendar_id: &str, sync_state: Option<&str>) -> Result<DeltaResult> {
        // Cursor-seeding mode (see trait docs): the caller has already
        // populated the local cache via `fetch_events` and only wants a
        // starting cursor. EWS's `SyncFolderItems` without a state walks
        // the entire folder before returning one, which is prohibitively
        // costly on large mailboxes (and there's no smaller cursor-only
        // EWS call to swap in). For now, EWS sources rely on full fetches
        // via `fetch_events` — `stored_sync_state` stays `None` and every
        // sync re-fetches the folder. The follow-up is to swap in a
        // `CalendarView`-based incremental sync, see issue tracker.
        if sync_state.is_none() {
            return Ok(DeltaResult::default());
        }

        let delta = operations::sync_folder_items(
            &self.endpoint,
            &self.username,
            &self.password,
            calendar_id,
            sync_state,
        )
        .await?;

        // Real incremental sync: resolve added/changed items into iCal text.
        // We pull MIME so we get full RRULE / EXDATE fidelity.
        let ids: Vec<&str> = delta
            .added_or_changed
            .iter()
            .map(|(id, _uid)| id.as_str())
            .collect();
        let mime_pairs = if ids.is_empty() {
            Vec::new()
        } else {
            operations::get_items_mime(&self.endpoint, &self.username, &self.password, &ids).await?
        };
        // Index MIME by ItemId so the order matches.
        let mut mime_by_id: HashMap<String, String> = mime_pairs.into_iter().collect();

        let mut added_or_changed = Vec::with_capacity(delta.added_or_changed.len());
        for (id, _uid) in delta.added_or_changed {
            if let Some(ical) = mime_by_id.remove(&id) {
                added_or_changed.push(RawEvent {
                    remote_id: id,
                    ical,
                });
            }
        }

        // For deleted items, EWS gives us the ItemId; the iCalendar UID is
        // not surfaced in the Delete change. calrs's orphan sweep (driven
        // off the local `events` table) catches anything we miss here.
        let deleted_uids = delta.deleted_item_ids;

        Ok(DeltaResult {
            added_or_changed,
            deleted_uids,
            new_sync_state: delta.new_sync_state,
        })
    }

    async fn put_event(&self, calendar_id: &str, uid: &str, ics: &str) -> Result<()> {
        // EWS does not expose a true PUT-by-UID operation. The convention is
        // to look the UID up via FindItem, delete the existing entry (if
        // any), then create the new item.
        let existing = operations::find_items_by_uid(
            &self.endpoint,
            &self.username,
            &self.password,
            calendar_id,
            uid,
        )
        .await
        .unwrap_or_default();
        for item_id in &existing {
            if let Err(e) =
                operations::delete_item(&self.endpoint, &self.username, &self.password, item_id)
                    .await
            {
                tracing::warn!(uid = %uid, error = %e, "EWS could not delete prior copy before re-create; continuing");
            }
        }
        operations::create_item_from_ics(
            &self.endpoint,
            &self.username,
            &self.password,
            calendar_id,
            ics,
        )
        .await?;
        Ok(())
    }

    async fn delete_event(&self, calendar_id: &str, uid: &str) -> Result<()> {
        let existing = operations::find_items_by_uid(
            &self.endpoint,
            &self.username,
            &self.password,
            calendar_id,
            uid,
        )
        .await?;
        for item_id in &existing {
            operations::delete_item(&self.endpoint, &self.username, &self.password, item_id)
                .await?;
        }
        Ok(())
    }
}

/// Build a `RawEvent` for each item via [`ical::synth_vcalendar`].
///
/// We don't follow up with a MIME `GetItem` for recurring items: `CalendarView`
/// already expanded every occurrence in the requested window, and for those
/// virtual occurrence IDs Exchange frequently returns the metadata block
/// without `MimeContent` — which the parser then silently drops, losing the
/// entire series. Synthesising directly from the occurrence's own Start/End
/// keeps every one, and the `RECURRENCE-ID` emitted by `synth_vcalendar`
/// makes them addressable under their shared master UID.
fn synth_raw_events(items: Vec<parse::EwsCalendarItem>) -> Vec<RawEvent> {
    let mut out = Vec::with_capacity(items.len());
    for item in &items {
        if let Some(ics) = ical::synth_vcalendar(item) {
            out.push(RawEvent {
                remote_id: item.item_id.clone(),
                ical: ics,
            });
        }
    }
    out
}

/// Compute a far-enough upper bound for `CalendarView`. The input is the
/// caller's `since_utc` ISO 8601 string; we add roughly two years (the
/// horizon over which calrs ever needs free/busy data) and reformat as
/// RFC 3339 UTC. Anything we cannot parse falls back to "now + 2y".
fn upper_bound_iso(since_utc: &str) -> String {
    use chrono::{DateTime, Duration, Utc};

    if let Ok(parsed) = DateTime::parse_from_rfc3339(since_utc) {
        return (parsed + Duration::days(730))
            .with_timezone(&Utc)
            .to_rfc3339();
    }
    (Utc::now() + Duration::days(730)).to_rfc3339()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upper_bound_extends_two_years() {
        let bound = upper_bound_iso("2026-05-06T00:00:00Z");
        // The result must be ~2 years after the input — basic sanity that the math works.
        assert!(bound.starts_with("2028-"));
    }

    #[test]
    fn upper_bound_falls_back_for_garbage_input() {
        let bound = upper_bound_iso("not-a-date");
        // Must still be parseable as RFC 3339.
        assert!(chrono::DateTime::parse_from_rfc3339(&bound).is_ok());
    }

    #[test]
    fn ews_provider_trims_trailing_slash() {
        let p = EwsProvider::new("https://mail.example.com/EWS/Exchange.asmx/", "u", "p");
        assert_eq!(p.endpoint, "https://mail.example.com/EWS/Exchange.asmx");
    }
}
