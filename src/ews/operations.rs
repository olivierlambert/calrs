//! High-level EWS operations: list calendar folders, fetch / create / delete
//! calendar items, run an incremental sync.
//!
//! Each function builds a SOAP body, posts it via [`super::soap::post_soap`],
//! and parses the response with helpers from [`super::parse`]. Operations are
//! kept narrow on purpose — calrs only exercises a small slice of the EWS
//! surface.

use anyhow::{bail, Context, Result};

use super::parse::{
    parse_calendar_items_response, parse_create_item_response, parse_find_folder_response,
    parse_get_item_response, parse_sync_folder_items_response, EwsCalendarFolder, EwsCalendarItem,
    EwsSyncDelta,
};
use super::soap::{escape, post_soap};

/// Issue a `GetFolder` against `inbox` to confirm that the credentials are
/// valid and the endpoint speaks EWS. The folder is purely a convenience pick
/// — every Exchange mailbox has it.
pub async fn check_connection(endpoint: &str, username: &str, password: &str) -> Result<bool> {
    let body = r#"    <m:GetFolder>
      <m:FolderShape>
        <t:BaseShape>IdOnly</t:BaseShape>
      </m:FolderShape>
      <m:FolderIds>
        <t:DistinguishedFolderId Id="inbox" />
      </m:FolderIds>
    </m:GetFolder>
"#;
    let resp = post_soap(endpoint, username, password, body, false).await?;
    // post_soap already raises on SOAP faults; we only need to confirm the
    // response carries a real folder reference (or, as a softer fallback, the
    // standard Success class) before declaring the endpoint EWS-compatible.
    if resp.contains("FolderId") || resp.contains("ResponseClass=\"Success\"") {
        Ok(true)
    } else {
        bail!("EWS GetFolder returned no FolderId — server may not be EWS-compatible")
    }
}

/// Enumerate the user's calendar folders by walking down from
/// `msgfolderroot`. We restrict to `IPF.Appointment` containers.
pub async fn list_calendar_folders(
    endpoint: &str,
    username: &str,
    password: &str,
) -> Result<Vec<EwsCalendarFolder>> {
    let body = r#"    <m:FindFolder Traversal="Deep">
      <m:FolderShape>
        <t:BaseShape>Default</t:BaseShape>
        <t:AdditionalProperties>
          <t:FieldURI FieldURI="folder:FolderClass" />
          <t:FieldURI FieldURI="folder:DisplayName" />
        </t:AdditionalProperties>
      </m:FolderShape>
      <m:Restriction>
        <t:IsEqualTo>
          <t:FieldURI FieldURI="folder:FolderClass" />
          <t:FieldURIOrConstant>
            <t:Constant Value="IPF.Appointment" />
          </t:FieldURIOrConstant>
        </t:IsEqualTo>
      </m:Restriction>
      <m:ParentFolderIds>
        <t:DistinguishedFolderId Id="msgfolderroot" />
      </m:ParentFolderIds>
    </m:FindFolder>
"#;
    let resp = post_soap(endpoint, username, password, body, false).await?;
    parse_find_folder_response(&resp)
}

/// Fetch every calendar item in a folder. Uses paging
/// (`IndexedPageItemView`) with a max page size to avoid OOM on busy
/// mailboxes.
pub async fn list_items(
    endpoint: &str,
    username: &str,
    password: &str,
    folder_id: &str,
) -> Result<Vec<EwsCalendarItem>> {
    list_items_window(endpoint, username, password, folder_id, None, None).await
}

/// Fetch calendar items overlapping `[start, end)`. Both bounds are RFC 3339
/// UTC strings (e.g. `2026-05-06T00:00:00Z`). EWS exposes this as a
/// `CalendarView` query, which expands recurring series into their
/// occurrences in the window — handy for booking slot computation.
pub async fn list_items_in_window(
    endpoint: &str,
    username: &str,
    password: &str,
    folder_id: &str,
    start_utc: &str,
    end_utc: &str,
) -> Result<Vec<EwsCalendarItem>> {
    list_items_window(
        endpoint,
        username,
        password,
        folder_id,
        Some(start_utc),
        Some(end_utc),
    )
    .await
}

const PAGE_SIZE: u32 = 200;

async fn list_items_window(
    endpoint: &str,
    username: &str,
    password: &str,
    folder_id: &str,
    start_utc: Option<&str>,
    end_utc: Option<&str>,
) -> Result<Vec<EwsCalendarItem>> {
    let mut all = Vec::new();
    let mut offset: u32 = 0;
    loop {
        let view = if let (Some(s), Some(e)) = (start_utc, end_utc) {
            // CalendarView expands recurrences; no offset paging is needed
            // because EWS returns up to 1000 items in one shot for a window.
            format!(
                r#"<m:CalendarView MaxEntriesReturned="1000" StartDate="{s}" EndDate="{e}" />"#,
                s = escape(s),
                e = escape(e),
            )
        } else {
            format!(
                r#"<m:IndexedPageItemView MaxEntriesReturned="{PAGE_SIZE}" Offset="{offset}" BasePoint="Beginning" />"#,
            )
        };

        let body = format!(
            r#"    <m:FindItem Traversal="Shallow">
      <m:ItemShape>
        <t:BaseShape>IdOnly</t:BaseShape>
        <t:AdditionalProperties>
          <t:FieldURI FieldURI="item:Subject" />
          <t:FieldURI FieldURI="calendar:Start" />
          <t:FieldURI FieldURI="calendar:End" />
          <t:FieldURI FieldURI="calendar:Location" />
          <t:FieldURI FieldURI="calendar:UID" />
          <t:FieldURI FieldURI="calendar:LegacyFreeBusyStatus" />
          <t:FieldURI FieldURI="calendar:IsAllDayEvent" />
          <t:FieldURI FieldURI="calendar:IsCancelled" />
          <t:FieldURI FieldURI="calendar:Recurrence" />
          <t:FieldURI FieldURI="calendar:CalendarItemType" />
        </t:AdditionalProperties>
      </m:ItemShape>
      {view}
      <m:ParentFolderIds>
        <t:FolderId Id="{folder}" />
      </m:ParentFolderIds>
    </m:FindItem>
"#,
            folder = escape(folder_id),
        );
        let resp = post_soap(endpoint, username, password, &body, true).await?;
        let mut page = parse_calendar_items_response(&resp)?;

        let included = page.included_count;
        let total = page.total;
        all.append(&mut page.items);

        // CalendarView returns everything in one shot.
        if start_utc.is_some() {
            break;
        }
        if included == 0 {
            break;
        }
        offset += included as u32;
        if let Some(t) = total {
            if offset >= t as u32 {
                break;
            }
        } else if (included as u32) < PAGE_SIZE {
            break;
        }
    }
    tracing::debug!(folder = %folder_id, count = all.len(), "EWS FindItem complete");
    Ok(all)
}

/// Fetch the MIME (RFC 5322 + iCalendar) representation of one or more items.
/// Returns the items in the same order, dropping any that the server failed
/// to retrieve.
pub async fn get_items_mime(
    endpoint: &str,
    username: &str,
    password: &str,
    item_ids: &[&str],
) -> Result<Vec<(String, String)>> {
    if item_ids.is_empty() {
        return Ok(Vec::new());
    }
    let mut id_xml = String::new();
    for id in item_ids {
        id_xml.push_str(&format!(
            r#"        <t:ItemId Id="{}" />
"#,
            escape(id)
        ));
    }
    let body = format!(
        r#"    <m:GetItem>
      <m:ItemShape>
        <t:BaseShape>IdOnly</t:BaseShape>
        <t:IncludeMimeContent>true</t:IncludeMimeContent>
        <t:AdditionalProperties>
          <t:FieldURI FieldURI="calendar:UID" />
        </t:AdditionalProperties>
      </m:ItemShape>
      <m:ItemIds>
{id_xml}      </m:ItemIds>
    </m:GetItem>
"#,
    );
    let resp = post_soap(endpoint, username, password, &body, true).await?;
    parse_get_item_response(&resp)
}

/// Create a calendar item from an iCalendar payload. Exchange will store the
/// MIME blob and surface the event natively via OWA / Outlook.
///
/// `SendMeetingInvitations="SendToNone"` keeps the call non-disruptive — calrs
/// drives invitations through SMTP separately and does not want EWS to fire
/// off duplicate invites on every booking.
pub async fn create_item_from_ics(
    endpoint: &str,
    username: &str,
    password: &str,
    folder_id: &str,
    ics: &str,
) -> Result<String> {
    use base64::Engine;
    let mime = base64::engine::general_purpose::STANDARD.encode(ics.as_bytes());

    let body = format!(
        r#"    <m:CreateItem MessageDisposition="SaveOnly" SendMeetingInvitations="SendToNone">
      <m:SavedItemFolderId>
        <t:FolderId Id="{folder}" />
      </m:SavedItemFolderId>
      <m:Items>
        <t:CalendarItem>
          <t:MimeContent CharacterSet="UTF-8">{mime}</t:MimeContent>
        </t:CalendarItem>
      </m:Items>
    </m:CreateItem>
"#,
        folder = escape(folder_id),
    );
    let resp = post_soap(endpoint, username, password, &body, true).await?;
    let item_id = parse_create_item_response(&resp)?;
    Ok(item_id)
}

/// Find calendar items that match a specific iCalendar UID. EWS exposes
/// `calendar:UID` as a searchable field; the result is usually 0 or 1 item.
pub async fn find_items_by_uid(
    endpoint: &str,
    username: &str,
    password: &str,
    folder_id: &str,
    uid: &str,
) -> Result<Vec<String>> {
    let body = format!(
        r#"    <m:FindItem Traversal="Shallow">
      <m:ItemShape>
        <t:BaseShape>IdOnly</t:BaseShape>
      </m:ItemShape>
      <m:Restriction>
        <t:IsEqualTo>
          <t:FieldURI FieldURI="calendar:UID" />
          <t:FieldURIOrConstant>
            <t:Constant Value="{uid}" />
          </t:FieldURIOrConstant>
        </t:IsEqualTo>
      </m:Restriction>
      <m:ParentFolderIds>
        <t:FolderId Id="{folder}" />
      </m:ParentFolderIds>
    </m:FindItem>
"#,
        uid = escape(uid),
        folder = escape(folder_id),
    );
    let resp = post_soap(endpoint, username, password, &body, false).await?;
    let parsed = parse_calendar_items_response(&resp)?;
    Ok(parsed.items.into_iter().map(|i| i.item_id).collect())
}

/// Permanently delete an item by id. We use `HardDelete` because the
/// alternative (`MoveToDeletedItems`) would leave a tombstone in Trash that
/// can confuse free/busy on shared calendars.
pub async fn delete_item(
    endpoint: &str,
    username: &str,
    password: &str,
    item_id: &str,
) -> Result<()> {
    let body = format!(
        r#"    <m:DeleteItem DeleteType="HardDelete" SendMeetingCancellations="SendToNone">
      <m:ItemIds>
        <t:ItemId Id="{id}" />
      </m:ItemIds>
    </m:DeleteItem>
"#,
        id = escape(item_id),
    );
    let _ = post_soap(endpoint, username, password, &body, false).await?;
    Ok(())
}

/// Run a delta sync (`SyncFolderItems`).
///
/// `sync_state` is the opaque cursor returned by the previous call (or
/// `None` for the initial sync). We request `MaxChangesReturned=512` and
/// loop until the server reports `IncludesLastItemInRange=true`, with a
/// hard cap of `MAX_SYNC_PAGES` iterations as insurance against a server
/// that never sets the terminator (200 pages × 512 changes = 102 400
/// items, far above any realistic mailbox).
pub async fn sync_folder_items(
    endpoint: &str,
    username: &str,
    password: &str,
    folder_id: &str,
    sync_state: Option<&str>,
) -> Result<EwsSyncDelta> {
    const MAX_SYNC_PAGES: usize = 200;
    let mut state = sync_state.map(str::to_string);
    let mut all = EwsSyncDelta::default();
    for _ in 0..MAX_SYNC_PAGES {
        let state_xml = state
            .as_deref()
            .map(|s| format!("      <m:SyncState>{}</m:SyncState>\n", escape(s)))
            .unwrap_or_default();
        let body = format!(
            r#"    <m:SyncFolderItems>
      <m:ItemShape>
        <t:BaseShape>IdOnly</t:BaseShape>
        <t:AdditionalProperties>
          <t:FieldURI FieldURI="calendar:UID" />
        </t:AdditionalProperties>
      </m:ItemShape>
      <m:SyncFolderId>
        <t:FolderId Id="{folder}" />
      </m:SyncFolderId>
{state_xml}      <m:MaxChangesReturned>512</m:MaxChangesReturned>
      <m:SyncScope>NormalItems</m:SyncScope>
    </m:SyncFolderItems>
"#,
            folder = escape(folder_id),
        );
        let resp = post_soap(endpoint, username, password, &body, true).await?;
        let page = parse_sync_folder_items_response(&resp)
            .context("failed to parse SyncFolderItems response")?;

        all.added_or_changed.extend(page.added_or_changed);
        all.deleted_uids.extend(page.deleted_uids);
        all.deleted_item_ids.extend(page.deleted_item_ids);
        all.new_sync_state = page.new_sync_state;
        state = all.new_sync_state.clone();

        if page.includes_last {
            return Ok(all);
        }
    }
    // Hit the safety cap without seeing IncludesLastItemInRange=true: either
    // the server is broken or the mailbox is genuinely enormous. Return what
    // we have plus the latest cursor so the next sync can resume.
    tracing::warn!(
        folder_id = %folder_id,
        pages = MAX_SYNC_PAGES,
        "SyncFolderItems hit the safety cap without IncludesLastItemInRange=true; \
         returning partial result, next sync will resume from cursor"
    );
    Ok(all)
}
