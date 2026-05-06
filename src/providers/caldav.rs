//! CalDAV adapter that exposes [`crate::caldav::CaldavClient`] through the
//! generic `CalendarProvider` trait. It is purely a translation layer — all
//! networking and parsing live in `crate::caldav`.

use anyhow::Result;
use async_trait::async_trait;

use super::{CalendarProvider, DeltaResult, RawEvent, RemoteCalendar};
use crate::caldav::CaldavClient;

pub struct CaldavProvider {
    client: CaldavClient,
}

impl CaldavProvider {
    pub fn new(base_url: &str, username: &str, password: &str) -> Self {
        Self {
            client: CaldavClient::new(base_url, username, password),
        }
    }

    /// Wrap an already-built `CaldavClient` (e.g. a bearer-authenticated Google
    /// CalDAV client) so the rest of the codebase can talk to it through the
    /// generic provider trait.
    pub fn from_client(client: CaldavClient) -> Self {
        Self { client }
    }
}

#[async_trait]
impl CalendarProvider for CaldavProvider {
    async fn check_connection(&self) -> Result<bool> {
        self.client.check_connection().await
    }

    async fn list_calendars(&self) -> Result<Vec<RemoteCalendar>> {
        let principal = self.client.discover_principal().await?;
        let home = self.client.discover_calendar_home(&principal).await?;
        let cals = self.client.list_calendars(&home).await?;
        Ok(cals
            .into_iter()
            .map(|c| RemoteCalendar {
                id: c.href,
                display_name: c.display_name,
                color: c.color,
                change_marker: c.ctag,
                sync_state: c.sync_token,
            })
            .collect())
    }

    async fn fetch_events(&self, calendar_id: &str) -> Result<Vec<RawEvent>> {
        let raws = self.client.fetch_events(calendar_id).await?;
        Ok(raws
            .into_iter()
            .map(|r| RawEvent {
                remote_id: r.href,
                ical: r.ical_data,
            })
            .collect())
    }

    async fn fetch_events_since(
        &self,
        calendar_id: &str,
        since_utc: &str,
    ) -> Result<Vec<RawEvent>> {
        let raws = self
            .client
            .fetch_events_since(calendar_id, since_utc)
            .await?;
        Ok(raws
            .into_iter()
            .map(|r| RawEvent {
                remote_id: r.href,
                ical: r.ical_data,
            })
            .collect())
    }

    async fn sync_delta(
        &self,
        calendar_id: &str,
        sync_state: Option<&str>,
    ) -> Result<DeltaResult> {
        let result = self.client.sync_collection(calendar_id, sync_state).await?;
        // CalDAV reports deletions as 404 hrefs. The href ends with `{uid}.ics`,
        // so we extract the UID — the rest of calrs keys events by UID.
        let deleted_uids = result
            .deleted_hrefs
            .iter()
            .filter_map(|href| {
                href.rsplit('/')
                    .next()
                    .map(|s| s.trim_end_matches(".ics").to_string())
            })
            .filter(|s| !s.is_empty())
            .collect();
        Ok(DeltaResult {
            added_or_changed: result
                .changed
                .into_iter()
                .map(|r| RawEvent {
                    remote_id: r.href,
                    ical: r.ical_data,
                })
                .collect(),
            deleted_uids,
            new_sync_state: result.new_sync_token,
        })
    }

    async fn put_event(&self, calendar_id: &str, uid: &str, ics: &str) -> Result<()> {
        self.client.put_event(calendar_id, uid, ics).await
    }

    async fn delete_event(&self, calendar_id: &str, uid: &str) -> Result<()> {
        self.client.delete_event(calendar_id, uid).await
    }
}
