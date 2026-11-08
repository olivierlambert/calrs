//! Response parsers for the EWS operations defined in [`super::operations`].
//!
//! Each parser reads the tree [`super::soap::post_soap`] returns, parsed by
//! the namespace-aware reader in `src/xml.rs` (#369): elements are matched by
//! namespace and local name, so the prefixes a server writes do not matter.
//! A reply missing what its operation must carry (its response message, a
//! successful `ResponseClass`, the `RootFolder`, an item's `ItemId`) is an
//! error, never an empty result: an empty calendar reads as "free".

use anyhow::{anyhow, bail, Result};
use base64::Engine;

use super::soap::{ews_child, ews_text, is_ews, message_code, message_failure};
use crate::xml::{quoted, Element};

/// One calendar folder discovered via `FindFolder`.
#[derive(Debug, Clone)]
pub struct EwsCalendarFolder {
    pub id: String,
    pub change_key: Option<String>,
    pub display_name: Option<String>,
}

/// Calendar item metadata returned by `FindItem` (no MIME content yet — that
/// requires a follow-up `GetItem`).
#[derive(Debug, Clone)]
pub struct EwsCalendarItem {
    pub item_id: String,
    pub change_key: Option<String>,
    pub uid: Option<String>,
    pub subject: Option<String>,
    pub start: Option<String>,
    pub end: Option<String>,
    pub location: Option<String>,
    pub is_all_day: bool,
    pub is_cancelled: bool,
    pub free_busy_status: Option<String>,
    pub has_recurrence: bool,
}

/// Page of calendar items + total count + how many were included in this
/// response, so the caller can drive offset paging.
#[derive(Debug, Clone, Default)]
pub struct EwsItemPage {
    pub items: Vec<EwsCalendarItem>,
    pub total: Option<u64>,
    pub included_count: usize,
    /// `IncludesLastItemInRange`, when the server sent it. `Some(false)` on a
    /// CalendarView means the server cut the window short.
    pub includes_last: Option<bool>,
}

/// Outcome of `SyncFolderItems` — added / changed items (with iCal text), and
/// deleted item ids/UIDs.
#[derive(Debug, Clone, Default)]
pub struct EwsSyncDelta {
    pub added_or_changed: Vec<(String, String)>,
    pub deleted_uids: Vec<String>,
    pub deleted_item_ids: Vec<String>,
    pub new_sync_state: Option<String>,
    pub includes_last: bool,
}

/// `doc` and every element below it, in document order. Parsers take the
/// whole envelope, or in tests a bare fragment whose root is the message.
fn self_and_descendants(doc: &Element) -> impl Iterator<Item = &Element> {
    std::iter::once(doc).chain(doc.descendants())
}

/// The EWS response message `name` of a reply, which must have succeeded.
/// `post_soap` already turned a failed message into an error, so a message
/// missing or not marked `Success` is a reply calrs cannot read.
fn success_message<'a>(doc: &'a Element, name: &str, op: &str) -> Result<&'a Element> {
    let message = self_and_descendants(doc)
        .find(|el| is_ews(el, name))
        .ok_or_else(|| anyhow!("EWS {op} reply holds no {name}"))?;
    check_success(message, op)?;
    Ok(message)
}

/// A message succeeded when its `ResponseClass` is `Success`, or `Warning`
/// with nothing failed (`NoError`: processed, with a note), the same test
/// [`super::soap::post_soap`] applies.
fn check_success(message: &Element, op: &str) -> Result<()> {
    match message.attr("ResponseClass") {
        Some("Success") | Some("Warning") => match message_failure(message) {
            None => Ok(()),
            Some(failure) => bail!("EWS {op} did not succeed: {}", quoted(&failure)),
        },
        Some(class) => match message_failure(message) {
            Some(failure) => bail!("EWS {op} did not succeed: {}", quoted(&failure)),
            None => bail!("EWS {op} did not succeed (ResponseClass {})", quoted(class)),
        },
        None => bail!("EWS {op} did not succeed (no ResponseClass)"),
    }
}

/// An `xs:boolean`: `true`, `false`, `1` or `0` (case aside for the words).
/// Anything else is an error rather than a guess.
fn parse_bool(value: &str, what: &str) -> Result<bool> {
    let value = value.trim();
    if value.eq_ignore_ascii_case("true") || value == "1" {
        Ok(true)
    } else if value.eq_ignore_ascii_case("false") || value == "0" {
        Ok(false)
    } else {
        bail!("EWS {what} is not a boolean: {}", quoted(value))
    }
}

/// The boolean attribute `name` of `el`, `None` when absent.
fn bool_attr(el: &Element, name: &str) -> Result<Option<bool>> {
    el.attr(name).map(|v| parse_bool(v, name)).transpose()
}

/// The boolean EWS child `name` of `el`, `false` when absent.
fn bool_child(el: &Element, name: &str) -> Result<bool> {
    match ews_text(el, name) {
        Some(value) => parse_bool(&value, name),
        None => Ok(false),
    }
}

/// The `Id` (and `ChangeKey`) of the id child `id_name` (`ItemId`,
/// `FolderId`) of `el`. Missing or empty is an error: an item or folder
/// calrs cannot address is not one it can skip without losing it.
fn required_id(el: &Element, id_name: &str) -> Result<(String, Option<String>)> {
    let tag = ews_child(el, id_name)
        .ok_or_else(|| anyhow!("EWS {} without an {id_name}", quoted(el.local_name())))?;
    match tag.attr("Id") {
        Some(id) if !id.is_empty() => {
            Ok((id.to_string(), tag.attr("ChangeKey").map(str::to_string)))
        }
        _ => bail!("EWS {id_name} without an Id"),
    }
}

/// Parse one page of a `FindFolderResponse`: its calendar folders, and the
/// `RootFolder`'s `IncludesLastItemInRange` when the server sent it
/// (`Some(false)`: more pages follow).
///
/// The answer must be a successful `FindFolderResponseMessage` with a
/// `RootFolder`: anything else is an error, never an empty list, since the
/// sync drops calendars a listing no longer shows.
pub fn parse_find_folder_page(doc: &Element) -> Result<(Vec<EwsCalendarFolder>, Option<bool>)> {
    let message = success_message(doc, "FindFolderResponseMessage", "FindFolder")?;
    let root = ews_child(message, "RootFolder")
        .ok_or_else(|| anyhow!("EWS FindFolder reply holds no RootFolder"))?;
    let includes_last = bool_attr(root, "IncludesLastItemInRange")?;
    let folders = ews_child(root, "Folders")
        .ok_or_else(|| anyhow!("EWS FindFolder reply holds no Folders"))?;
    let mut out = Vec::new();
    // The restriction asks for IPF.Appointment folders, which Exchange
    // lists as CalendarFolder; any other folder kind is not a calendar.
    for folder in folders.elements().filter(|el| is_ews(el, "CalendarFolder")) {
        let (id, change_key) = required_id(folder, "FolderId")?;
        out.push(EwsCalendarFolder {
            id,
            change_key,
            display_name: ews_text(folder, "DisplayName"),
        });
    }
    Ok((out, includes_last))
}

/// Parse a `GetFolderResponse` for one folder and return its id, `None` when
/// the successful message lists no folder.
pub fn parse_get_folder_response(doc: &Element) -> Result<Option<String>> {
    let message = success_message(doc, "GetFolderResponseMessage", "GetFolder")?;
    let folders = ews_child(message, "Folders")
        .ok_or_else(|| anyhow!("EWS GetFolder reply holds no Folders"))?;
    match folders.elements().next() {
        Some(folder) => Ok(Some(required_id(folder, "FolderId")?.0)),
        None => Ok(None),
    }
}

/// Parse a `FindItemResponse` for calendar items.
pub fn parse_calendar_items_response(doc: &Element) -> Result<EwsItemPage> {
    let message = success_message(doc, "FindItemResponseMessage", "FindItem")?;
    let root = ews_child(message, "RootFolder")
        .ok_or_else(|| anyhow!("EWS FindItem reply holds no RootFolder"))?;
    let total = root
        .attr("TotalItemsInView")
        .map(|v| {
            v.trim()
                .parse::<u64>()
                .map_err(|_| anyhow!("EWS TotalItemsInView is not a number: {}", quoted(v)))
        })
        .transpose()?;
    let includes_last = bool_attr(root, "IncludesLastItemInRange")?;
    let items =
        ews_child(root, "Items").ok_or_else(|| anyhow!("EWS FindItem reply holds no Items"))?;

    let mut page = EwsItemPage {
        total,
        includes_last,
        // Every entry counts towards the server's cap and the paging offset,
        // whatever its kind.
        included_count: items.elements().count(),
        ..Default::default()
    };
    for item in items.elements().filter(|el| is_ews(el, "CalendarItem")) {
        page.items.push(parse_calendar_item(item)?);
    }
    Ok(page)
}

fn parse_calendar_item(item: &Element) -> Result<EwsCalendarItem> {
    let (item_id, change_key) = required_id(item, "ItemId")?;
    // `has_recurrence` here narrowly means "needs RECURRENCE-ID to
    // disambiguate siblings sharing the master UID", i.e. an expanded
    // `Occurrence` or `Exception` returned by `CalendarView`. A raw
    // `RecurringMaster` (only surfaced by the non-windowed `fetch_events`
    // path, which production sync does not use) must NOT get a
    // `RECURRENCE-ID`: masters are a single logical item and emitting one
    // would store an iCal value with `RECURRENCE-ID` but no `RRULE`, which is
    // meaningless.
    let calendar_item_type = ews_text(item, "CalendarItemType");
    let has_recurrence = matches!(
        calendar_item_type.as_deref(),
        Some(t) if t.eq_ignore_ascii_case("Occurrence") || t.eq_ignore_ascii_case("Exception")
    );
    let uid = ews_text(item, "UID").map(|uid| safe_uid(uid, &item_id));
    let start = ews_text(item, "Start")
        .map(|v| checked_datetime(v, "Start", &item_id))
        .transpose()?;
    let end = ews_text(item, "End")
        .map(|v| checked_datetime(v, "End", &item_id))
        .transpose()?;
    Ok(EwsCalendarItem {
        item_id,
        change_key,
        uid,
        subject: ews_text(item, "Subject"),
        start,
        end,
        location: ews_text(item, "Location"),
        is_all_day: bool_child(item, "IsAllDayEvent")?,
        is_cancelled: bool_child(item, "IsCancelled")?,
        free_busy_status: ews_text(item, "LegacyFreeBusyStatus"),
        has_recurrence,
    })
}

/// Whether `value` holds a control character (CR, LF, NUL, ...). Such a
/// value would break the lines of the VCALENDAR made up from the item
/// (`ical::synth_vcalendar`): a UID carrying CRLF could add properties, or a
/// second VEVENT reusing a real UID marked TRANSPARENT, turning busy time
/// into free.
pub fn has_control(value: &str) -> bool {
    value.chars().any(char::is_control)
}

/// The UID calrs stores for an item: the one Exchange sent, or, when it
/// holds control characters, `ews-` and the hex SHA-256 of its bytes.
///
/// A UID is the organizer's to choose, so anyone who can invite the host
/// can put a CRLF in one. Refusing the answer would let them freeze the
/// host's sync; writing it as is would break the made-up VCALENDAR's lines.
/// The item is kept (it is still busy time) under a UID derived the same
/// way on every read, so the orphan sweep and later lookups see one stable
/// value. calrs's own UIDs never hold control characters, so a derived UID
/// never matches a booking.
pub fn safe_uid(uid: String, item_id: &str) -> String {
    if !has_control(&uid) {
        return uid;
    }
    use sha2::{Digest, Sha256};
    tracing::warn!(item_id = %item_id, "EWS item with control characters in its UID, storing it under a derived UID");
    format!("ews-{}", hex::encode(Sha256::digest(uid.as_bytes())))
}

/// Whether `value` reads as an `xs:dateTime` as EWS writes it
/// (`2026-05-06T09:00:00Z`, possibly with fractions or an offset): digits
/// and `-:T.Z+` only.
pub fn is_ews_datetime(value: &str) -> bool {
    !value.is_empty()
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || matches!(b, b'-' | b':' | b'T' | b'.' | b'Z' | b'+'))
}

/// An item's `Start` or `End`, refused when it is not a date-time: it is
/// written as is into the VCALENDAR made up from the item.
fn checked_datetime(value: String, what: &str, item_id: &str) -> Result<String> {
    if !is_ews_datetime(&value) {
        tracing::warn!(item_id = %item_id, "EWS item with an unreadable {what}, refusing the answer");
        bail!(
            "EWS item {} has an unreadable {what}: {}",
            quoted(item_id),
            quoted(&value)
        );
    }
    Ok(value)
}

/// The item kinds whose MIME content `parse_get_item_response` reads.
const MIME_ITEM_KINDS: [&str; 4] = [
    "CalendarItem",
    "MeetingRequest",
    "MeetingResponse",
    "MeetingCancellation",
];

/// The per-message code of an item deleted since it was listed: a GetItem
/// batch drops that item rather than failing (see
/// [`super::operations::get_items_mime`]).
pub const ITEM_GONE: &str = "ErrorItemNotFound";

/// Parse a `GetItemResponse`. Returns one (item_id, ical) pair per item
/// retrieved, across every response message of the batch. A message failing
/// with [`ITEM_GONE`] is dropped; any other failed message is an error. An
/// item without MIME content is skipped (the server occasionally returns a
/// placeholder for unsupported types), as is MIME content that holds no
/// calendar.
pub fn parse_get_item_response(doc: &Element) -> Result<Vec<(String, String)>> {
    let mut out = Vec::new();
    let mut messages = 0;
    for message in self_and_descendants(doc).filter(|el| is_ews(el, "GetItemResponseMessage")) {
        messages += 1;
        if message.attr("ResponseClass") == Some("Error")
            && message_code(message).as_deref() == Some(ITEM_GONE)
        {
            tracing::debug!("EWS GetItem: an item of the batch is gone, dropping it");
            continue;
        }
        check_success(message, "GetItem")?;
        let items = ews_child(message, "Items")
            .ok_or_else(|| anyhow!("EWS GetItem reply holds no Items"))?;
        for item in items
            .elements()
            .filter(|el| MIME_ITEM_KINDS.iter().any(|kind| is_ews(el, kind)))
        {
            let (item_id, _) = required_id(item, "ItemId")?;
            let Some(mime_b64) = ews_text(item, "MimeContent") else {
                continue;
            };
            // Base64 may be wrapped over several lines.
            let compact: String = mime_b64
                .chars()
                .filter(|c| !c.is_ascii_whitespace())
                .collect();
            let decoded = match base64::engine::general_purpose::STANDARD.decode(compact) {
                Ok(bytes) => bytes,
                Err(e) => {
                    tracing::warn!(error = %e, "EWS MimeContent base64 decode failed; skipping item");
                    continue;
                }
            };
            let mime_text = match String::from_utf8(decoded) {
                Ok(t) => t,
                Err(e) => {
                    tracing::warn!(error = %e, "EWS MimeContent not valid UTF-8; skipping item");
                    continue;
                }
            };
            if let Some(ical) = extract_vcalendar(&mime_text) {
                out.push((item_id, ical));
            }
        }
    }
    if messages == 0 {
        bail!("EWS GetItem reply holds no GetItemResponseMessage");
    }
    Ok(out)
}

/// Pull the BEGIN:VCALENDAR…END:VCALENDAR block out of a MIME message body.
/// EWS returns the full RFC 5322 envelope plus calendar attachment; we only
/// care about the iCalendar portion.
pub fn extract_vcalendar(mime: &str) -> Option<String> {
    let begin = mime.find("BEGIN:VCALENDAR")?;
    let end = mime.find("END:VCALENDAR")?;
    if end <= begin {
        return None;
    }
    let close = end + "END:VCALENDAR".len();
    let mut block = mime[begin..close].to_string();
    // MIME line endings are CRLF; iCal already requires CRLF, so we leave it
    // alone. RFC 5545 §3.1 long-content lines are folded with a CRLF followed
    // by a single space or tab; unfold them here so downstream parsers see
    // logical lines rather than the wire format.
    block = block.replace("\r\n ", "");
    block = block.replace("\r\n\t", "");
    Some(block)
}

/// Parse the response of `CreateItem` and return the new ItemId.
pub fn parse_create_item_response(doc: &Element) -> Result<String> {
    let message = success_message(doc, "CreateItemResponseMessage", "CreateItem")?;
    let item = ews_child(message, "Items")
        .and_then(|items| items.elements().next())
        .ok_or_else(|| anyhow!("CreateItem response did not include an ItemId"))?;
    Ok(required_id(item, "ItemId")?.0)
}

/// Check the response of `DeleteItem`.
pub fn parse_delete_item_response(doc: &Element) -> Result<()> {
    success_message(doc, "DeleteItemResponseMessage", "DeleteItem").map(|_| ())
}

/// Parse a `SyncFolderItemsResponse` (single page). Caller loops until
/// `includes_last` is true.
pub fn parse_sync_folder_items_response(doc: &Element) -> Result<EwsSyncDelta> {
    let message = success_message(doc, "SyncFolderItemsResponseMessage", "SyncFolderItems")?;
    let new_sync_state = ews_text(message, "SyncState")
        .ok_or_else(|| anyhow!("EWS SyncFolderItems reply holds no SyncState"))?;
    let includes_last = ews_text(message, "IncludesLastItemInRange")
        .ok_or_else(|| anyhow!("EWS SyncFolderItems reply holds no IncludesLastItemInRange"))?;
    let changes = ews_child(message, "Changes")
        .ok_or_else(|| anyhow!("EWS SyncFolderItems reply holds no Changes"))?;

    let mut delta = EwsSyncDelta {
        new_sync_state: Some(new_sync_state),
        includes_last: parse_bool(&includes_last, "IncludesLastItemInRange")?,
        ..Default::default()
    };

    // Changes come as <t:Create><t:CalendarItem>...</t:CalendarItem></t:Create>
    // or <t:Update> with the same shape, and <t:Delete><t:ItemId/></t:Delete>.
    // ReadFlagChange does not matter for calendars and is ignored.
    for change in changes.elements() {
        if is_ews(change, "Create") || is_ews(change, "Update") {
            let item = change
                .elements()
                .next()
                .ok_or_else(|| anyhow!("EWS SyncFolderItems change without an item"))?;
            let (id, _) = required_id(item, "ItemId")?;
            let uid = ews_text(item, "UID")
                .map(|uid| safe_uid(uid, &id))
                .unwrap_or_default();
            delta.added_or_changed.push((id, uid));
        } else if is_ews(change, "Delete") {
            delta
                .deleted_item_ids
                .push(required_id(change, "ItemId")?.0);
        }
    }
    Ok(delta)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ews::fixtures;

    /// The tree of `xml`, a whole reply or a bare fragment.
    fn doc(xml: &str) -> Element {
        crate::xml::parse(xml).unwrap().unwrap()
    }

    /// A whole reply, parsed without the fault check.
    fn doc_of(xml: &str) -> Element {
        crate::ews::soap::parse_envelope(xml).unwrap()
    }

    /// A whole reply, read the way `post_soap` reads it.
    fn reply(xml: &str) -> Element {
        crate::ews::soap::read_reply(reqwest::StatusCode::OK, xml, "e").unwrap()
    }

    #[test]
    fn find_item_page_reports_whether_the_view_is_complete() {
        let page = |flag: &str| {
            parse_calendar_items_response(&doc(&format!(
                r#"<m:FindItemResponseMessage ResponseClass="Success"><m:RootFolder TotalItemsInView="1" {flag}><t:Items></t:Items></m:RootFolder></m:FindItemResponseMessage>"#
            )))
            .unwrap()
            .includes_last
        };
        assert_eq!(page(r#"IncludesLastItemInRange="true""#), Some(true));
        assert_eq!(page(r#"IncludesLastItemInRange="false""#), Some(false));
        assert_eq!(page(r#"IncludesLastItemInRange="1""#), Some(true));
        assert_eq!(page(""), None);
    }

    /// A reply that is not a successful FindFolder (an empty body, a
    /// sign-in page, an error class, a message without its RootFolder) is an
    /// error, never "no calendars".
    #[test]
    fn find_folder_refuses_anything_but_a_successful_listing() {
        for xml in [
            "<s:Envelope><s:Body/></s:Envelope>",
            "<html><body>Sign in</body></html>",
            r#"<m:FindFolderResponseMessage ResponseClass="Error"><m:RootFolder/></m:FindFolderResponseMessage>"#,
            r#"<m:FindFolderResponseMessage><m:RootFolder><t:Folders/></m:RootFolder></m:FindFolderResponseMessage>"#,
            r#"<m:FindFolderResponseMessage ResponseClass="Success"></m:FindFolderResponseMessage>"#,
            r#"<m:FindFolderResponseMessage ResponseClass="Success"><m:RootFolder/></m:FindFolderResponseMessage>"#,
            // A folder calrs cannot address is not skipped.
            r#"<m:FindFolderResponseMessage ResponseClass="Success"><m:RootFolder><t:Folders><t:CalendarFolder><t:DisplayName>X</t:DisplayName></t:CalendarFolder></t:Folders></m:RootFolder></m:FindFolderResponseMessage>"#,
            r#"<m:FindFolderResponseMessage ResponseClass="Success"><m:RootFolder><t:Folders><t:CalendarFolder><t:FolderId Id=""/></t:CalendarFolder></t:Folders></m:RootFolder></m:FindFolderResponseMessage>"#,
            // Paging flags that are not booleans are not guessed at.
            r#"<m:FindFolderResponseMessage ResponseClass="Success"><m:RootFolder IncludesLastItemInRange="maybe"><t:Folders/></m:RootFolder></m:FindFolderResponseMessage>"#,
            // Elements of another vocabulary are not EWS's.
            r#"<x:FindFolderResponseMessage xmlns:x="urn:other" ResponseClass="Success"><x:RootFolder><x:Folders/></x:RootFolder></x:FindFolderResponseMessage>"#,
        ] {
            assert!(parse_find_folder_page(&doc(xml)).is_err(), "{xml:?}");
        }
        let empty = r#"<m:FindFolderResponseMessage ResponseClass="Success"><m:RootFolder TotalItemsInView="0"><t:Folders/></m:RootFolder></m:FindFolderResponseMessage>"#;
        assert!(parse_find_folder_page(&doc(empty)).unwrap().0.is_empty());
    }

    /// A paged listing says whether its last folder is in; a truncated
    /// page (`IncludesLastItemInRange="false"`) is not the whole listing.
    #[test]
    fn find_folder_pages_say_whether_they_are_complete() {
        let page = |attr: &str| {
            doc(&format!(
                r#"<m:FindFolderResponseMessage ResponseClass="Success"><m:RootFolder {attr}><t:Folders><t:CalendarFolder><t:FolderId Id="F1"/></t:CalendarFolder></t:Folders></m:RootFolder></m:FindFolderResponseMessage>"#
            ))
        };
        let (folders, last) =
            parse_find_folder_page(&page(r#"IncludesLastItemInRange="false""#)).unwrap();
        assert_eq!((folders.len(), last), (1, Some(false)));
        assert_eq!(
            parse_find_folder_page(&page(r#"IncludesLastItemInRange="true""#))
                .unwrap()
                .1,
            Some(true)
        );
        assert_eq!(parse_find_folder_page(&page("")).unwrap().1, None);
    }

    #[test]
    fn parses_find_folder_response() {
        let xml = r#"<m:FindFolderResponseMessage ResponseClass="Success">
  <m:RootFolder TotalItemsInView="1">
    <t:Folders>
      <t:CalendarFolder>
        <t:FolderId Id="AAMkADUw" ChangeKey="CK1" />
        <t:DisplayName>Calendar</t:DisplayName>
      </t:CalendarFolder>
      <t:CalendarFolder>
        <t:FolderId Id="AAMkADUx" />
        <t:DisplayName>Birthdays</t:DisplayName>
      </t:CalendarFolder>
    </t:Folders>
  </m:RootFolder>
</m:FindFolderResponseMessage>"#;
        let (folders, _) = parse_find_folder_page(&doc(xml)).unwrap();
        assert_eq!(folders.len(), 2);
        assert_eq!(folders[0].id, "AAMkADUw");
        assert_eq!(folders[0].change_key, Some("CK1".into()));
        assert_eq!(folders[0].display_name.as_deref(), Some("Calendar"));
        assert_eq!(folders[1].id, "AAMkADUx");
        assert_eq!(folders[1].change_key, None);
        assert_eq!(folders[1].display_name.as_deref(), Some("Birthdays"));
    }

    /// One listing written with Exchange's prefixes, with default namespaces
    /// and with prefixes nobody uses reads the same, entities and CDATA
    /// decoded.
    #[test]
    fn find_folder_prefixes_do_not_matter() {
        for xml in [
            fixtures::FIND_FOLDER_SOAP_PREFIX,
            fixtures::FIND_FOLDER_DEFAULT_NAMESPACES,
            fixtures::FIND_FOLDER_ODD_PREFIXES,
        ] {
            let (folders, last) = parse_find_folder_page(&reply(xml)).unwrap();
            assert_eq!(last, Some(true), "{xml}");
            let read: Vec<_> = folders
                .iter()
                .map(|f| {
                    (
                        f.id.as_str(),
                        f.change_key.as_deref(),
                        f.display_name.as_deref(),
                    )
                })
                .collect();
            assert_eq!(
                read,
                [
                    (
                        "AAMkADUwNjA3YTQ5LTEzZDktNGQ2Yy1iZDk3LTJiNzRkZGFiMmJmZgAuAAAAAAA+/b8xAQ==",
                        Some("AgAAABYAAAD/"),
                        Some("Calendar")
                    ),
                    (
                        "AAMkADUwNjA3YTQ5LTEzZDktNGQ2Yy1iZDk3LTJiNzRkZGFiMmJmZgAuAAAAAAA+/b8xAg==",
                        Some("AgAAABYAAAEA"),
                        Some("Sales & Marketing")
                    ),
                ],
                "{xml}"
            );
        }
    }

    #[test]
    fn parses_find_item_calendar_view() {
        let xml = r#"<m:FindItemResponseMessage ResponseClass="Success">
  <m:RootFolder TotalItemsInView="2" IncludesLastItemInRange="true">
    <t:Items>
      <t:CalendarItem>
        <t:ItemId Id="AB1" ChangeKey="CK1" />
        <t:Subject>Sync sync</t:Subject>
        <t:Start>2026-05-06T09:00:00Z</t:Start>
        <t:End>2026-05-06T09:30:00Z</t:End>
        <t:Location>Room 1</t:Location>
        <t:IsAllDayEvent>false</t:IsAllDayEvent>
        <t:IsCancelled>false</t:IsCancelled>
        <t:LegacyFreeBusyStatus>Busy</t:LegacyFreeBusyStatus>
        <t:UID>uid-1</t:UID>
      </t:CalendarItem>
      <t:CalendarItem>
        <t:ItemId Id="AB2" />
        <t:Subject>All-day off</t:Subject>
        <t:Start>2026-05-08T00:00:00Z</t:Start>
        <t:End>2026-05-09T00:00:00Z</t:End>
        <t:IsAllDayEvent>true</t:IsAllDayEvent>
        <t:LegacyFreeBusyStatus>OOF</t:LegacyFreeBusyStatus>
        <t:UID>uid-2</t:UID>
      </t:CalendarItem>
    </t:Items>
  </m:RootFolder>
</m:FindItemResponseMessage>"#;
        let page = parse_calendar_items_response(&doc(xml)).unwrap();
        assert_eq!(page.total, Some(2));
        assert_eq!(page.items.len(), 2);
        assert_eq!(page.items[0].item_id, "AB1");
        assert_eq!(page.items[0].uid.as_deref(), Some("uid-1"));
        assert_eq!(page.items[0].subject.as_deref(), Some("Sync sync"));
        assert_eq!(page.items[0].start.as_deref(), Some("2026-05-06T09:00:00Z"));
        assert!(!page.items[0].is_all_day);
        assert!(page.items[1].is_all_day);
        assert_eq!(page.items[1].free_busy_status.as_deref(), Some("OOF"));
    }

    /// The Exchange 2016 CalendarView fixture, every field calrs reads.
    #[test]
    fn parses_an_exchange_calendar_view() {
        let page =
            parse_calendar_items_response(&reply(fixtures::FIND_ITEM_CALENDAR_VIEW)).unwrap();
        assert_eq!(page.total, Some(3));
        assert_eq!(page.includes_last, Some(true));
        assert_eq!(page.included_count, 3);
        let single = &page.items[0];
        assert_eq!(single.item_id, "AAMkADUwNjA3YTQ5AEYAAAAAAD+/b8xAAA=");
        assert_eq!(single.change_key.as_deref(), Some("DwAAABYAAAD/"));
        assert_eq!(
            single.subject.as_deref(),
            Some("Q3 review: R&D <budget> \u{2013} draft")
        );
        assert_eq!(single.location.as_deref(), Some("Room \"Turing\" & Teams"));
        assert_eq!(single.start.as_deref(), Some("2026-05-06T09:00:00Z"));
        assert_eq!(single.end.as_deref(), Some("2026-05-06T09:30:00Z"));
        assert_eq!(
            single.uid.as_deref(),
            Some(
                "040000008200E00074C5B7101A82E00800000000A1B2C3D4E1DC0100000000000000001000000011"
            )
        );
        assert_eq!(single.free_busy_status.as_deref(), Some("Busy"));
        assert!(!single.is_all_day && !single.is_cancelled && !single.has_recurrence);

        let absence = &page.items[1];
        assert!(absence.is_all_day);
        assert_eq!(absence.free_busy_status.as_deref(), Some("OOF"));
        assert_eq!(absence.location, None);

        let occurrence = &page.items[2];
        assert!(occurrence.has_recurrence && occurrence.is_cancelled);
        assert_eq!(occurrence.free_busy_status.as_deref(), Some("Tentative"));
        assert_eq!(
            occurrence.uid.as_deref(),
            Some("series-master@corp.example")
        );
    }

    /// The same items with default namespaces and other prefixes read the
    /// same.
    #[test]
    fn find_item_prefixes_do_not_matter() {
        let base = fixtures::FIND_ITEM_CALENDAR_VIEW;
        let renamed = base
            .replace("xmlns:m=", "xmlns:msg=")
            .replace("xmlns:t=", "xmlns:typ=")
            .replace("<m:", "<msg:")
            .replace("</m:", "</msg:")
            .replace("<t:", "<typ:")
            .replace("</t:", "</typ:")
            .replace("xmlns:s=", "xmlns:soap=")
            .replace("<s:", "<soap:")
            .replace("</s:", "</soap:");
        let defaults = base
            .replace(
                "<m:FindItemResponse xmlns:m=\"http://schemas.microsoft.com/exchange/services/2006/messages\" xmlns:t=\"http://schemas.microsoft.com/exchange/services/2006/types\">",
                "<FindItemResponse xmlns=\"http://schemas.microsoft.com/exchange/services/2006/messages\">",
            )
            .replace("<t:Items>", "<Items xmlns=\"http://schemas.microsoft.com/exchange/services/2006/types\">")
            .replace("<m:", "<")
            .replace("</m:", "</")
            .replace("<t:", "<")
            .replace("</t:", "</");
        let want = parse_calendar_items_response(&reply(base)).unwrap();
        for xml in [renamed, defaults] {
            let got = parse_calendar_items_response(&reply(&xml)).unwrap();
            assert_eq!(format!("{got:?}"), format!("{want:?}"), "{xml}");
        }
    }

    /// Text that quotes SOAP markup stays text.
    #[test]
    fn find_item_text_that_looks_like_markup_is_text() {
        let page = parse_calendar_items_response(&reply(fixtures::FIND_ITEM_FAULT_LOOKALIKE_TEXT))
            .unwrap();
        assert_eq!(page.items.len(), 1);
        assert_eq!(
            page.items[0].subject.as_deref(),
            Some("<s:Fault><faultstring>postmortem</faultstring></s:Fault>")
        );
        assert_eq!(
            page.items[0].location.as_deref(),
            Some("<m:ResponseCode>ErrorAccessDenied</m:ResponseCode>")
        );
    }

    /// A FindItem that is not a complete, successful answer is an error: an
    /// empty page would read as a free calendar.
    #[test]
    fn find_item_refuses_what_it_cannot_read() {
        let message = |root: &str| {
            format!(
                r#"<m:FindItemResponseMessage ResponseClass="Success">{root}</m:FindItemResponseMessage>"#
            )
        };
        for xml in [
            "<s:Envelope><s:Body/></s:Envelope>".to_string(),
            r#"<m:FindItemResponse><m:RootFolder><t:Items/></m:RootFolder></m:FindItemResponse>"#.to_string(),
            r#"<m:FindItemResponseMessage ResponseClass="Error"><m:RootFolder><t:Items/></m:RootFolder></m:FindItemResponseMessage>"#.to_string(),
            message(""),
            message("<m:RootFolder/>"),
            message(r#"<m:RootFolder TotalItemsInView="lots"><t:Items/></m:RootFolder>"#),
            message(r#"<m:RootFolder IncludesLastItemInRange="yes"><t:Items/></m:RootFolder>"#),
            message("<m:RootFolder><t:Items><t:CalendarItem><t:Subject>no id</t:Subject></t:CalendarItem></t:Items></m:RootFolder>"),
            message(r#"<m:RootFolder><t:Items><t:CalendarItem><t:ItemId Id="A"/><t:IsAllDayEvent>sometimes</t:IsAllDayEvent></t:CalendarItem></t:Items></m:RootFolder>"#),
            message(r#"<m:RootFolder><t:Items><t:CalendarItem><t:ItemId Id="A"/><t:IsCancelled>2</t:IsCancelled></t:CalendarItem></t:Items></m:RootFolder>"#),
        ] {
            assert!(parse_calendar_items_response(&doc(&xml)).is_err(), "{xml}");
        }
    }

    /// A Start or End that is not a date-time is a server problem and is
    /// refused: both are written as is into the made-up VCALENDAR.
    #[test]
    fn find_item_refuses_values_that_would_break_the_ical() {
        let item = |fields: &str| {
            doc(&format!(
                r#"<m:FindItemResponseMessage ResponseClass="Success"><m:RootFolder><t:Items><t:CalendarItem><t:ItemId Id="A"/>{fields}</t:CalendarItem></t:Items></m:RootFolder></m:FindItemResponseMessage>"#
            ))
        };
        for fields in [
            "<t:Start>2026-05-06T09:00:00Z&#xD;&#xA;END:VEVENT</t:Start>",
            "<t:End>2026-05-06T09:30:00Z;X=1</t:End>",
            "<t:Start>tomorrow</t:Start>",
        ] {
            let err = parse_calendar_items_response(&item(fields)).unwrap_err();
            assert!(err.to_string().contains("EWS item"), "{fields}: {err}");
        }
        // Ordinary values read as before, offsets and fractions included.
        let page = parse_calendar_items_response(&item(
            "<t:UID>a b@corp</t:UID><t:Start>2026-05-06T09:00:00.000+02:00</t:Start><t:End>2026-05-06T09:30:00Z</t:End>",
        ))
        .unwrap();
        assert_eq!(page.items[0].uid.as_deref(), Some("a b@corp"));
    }

    /// An organizer-chosen UID with control characters does not stop the
    /// sync: the item is kept, busy, under a derived UID that is the same on
    /// every read, and nothing it carried reaches the made-up VCALENDAR.
    #[test]
    fn hostile_uid_is_kept_under_a_derived_uid() {
        let xml = fixtures::FIND_ITEM_CALENDAR_VIEW.replace(
            "<t:UID>uid-2@corp.example</t:UID>",
            "<t:UID>040000008200E00074C5B7101A82E00800000000A1B2C3D4E1DC0100000000000000001000000011&#xD;&#xA;TRANSP:TRANSPARENT&#xD;&#xA;END:VEVENT&#xD;&#xA;BEGIN:VEVENT&#xD;&#xA;UID:victim\nX-INJECTED:1</t:UID>",
        );
        assert_ne!(xml, fixtures::FIND_ITEM_CALENDAR_VIEW);
        let page = parse_calendar_items_response(&reply(&xml)).unwrap();
        assert_eq!(page.items.len(), 3, "every item is kept");
        let hostile = &page.items[1];
        let uid = hostile.uid.clone().unwrap();
        assert!(uid.starts_with("ews-") && uid.len() == 4 + 64, "{uid}");
        // Derived the same way on every read.
        let again = parse_calendar_items_response(&reply(&xml)).unwrap();
        assert_eq!(again.items[1].uid.as_deref(), Some(uid.as_str()));
        // The normal item next to it is untouched.
        assert_eq!(
            page.items[0].uid.as_deref(),
            Some(
                "040000008200E00074C5B7101A82E00800000000A1B2C3D4E1DC0100000000000000001000000011"
            )
        );

        // Both items make a VCALENDAR; the hostile one stays busy and adds
        // nothing.
        for item in &page.items[..2] {
            let ics = crate::ews::ical::synth_vcalendar(item).expect("written");
            assert_eq!(ics.matches("BEGIN:VEVENT").count(), 1, "{ics}");
            assert_eq!(ics.matches("UID:").count(), 1, "{ics}");
            assert!(
                !ics.contains("X-INJECTED") && !ics.contains("victim"),
                "{ics}"
            );
            assert!(ics.contains("TRANSP:OPAQUE\r\n"), "{ics}");
            assert_eq!(ics.matches("TRANSP:").count(), 1, "{ics}");
        }
        let ics = crate::ews::ical::synth_vcalendar(hostile).unwrap();
        assert!(ics.contains(&format!("UID:{uid}\r\n")), "{ics}");
        assert!(ics.contains("DTSTART:20260508T000000Z\r\n"), "{ics}");

        // SyncFolderItems derives the same UID.
        let sync = fixtures::SYNC_FOLDER_ITEMS.replace(
            "<t:UID>uid-new@corp.example</t:UID>",
            "<t:UID>a&#xA;b</t:UID>",
        );
        let d = parse_sync_folder_items_response(&reply(&sync)).unwrap();
        assert_eq!(d.added_or_changed[0].1, safe_uid("a\nb".to_string(), "x"));
        assert_eq!(safe_uid("plain@corp".to_string(), "x"), "plain@corp");
    }

    /// A GetItem batch drops an item the server reports gone and fails on
    /// any other per-message error.
    #[test]
    fn get_item_batch_drops_gone_items_only() {
        let skipping = |xml: &str| {
            crate::ews::soap::read_reply_skipping(reqwest::StatusCode::OK, xml, "e", &[ITEM_GONE])
        };
        let doc = skipping(fixtures::GET_ITEM_SECOND_MESSAGE_FAILS).unwrap();
        assert!(parse_get_item_response(&doc).unwrap().is_empty());
        // The gone item next to a readable one keeps the readable one.
        let mixed = fixtures::GET_ITEM_MIME.replacen(
            r#"<m:GetItemResponseMessage ResponseClass="Success">
          <m:ResponseCode>NoError</m:ResponseCode>
          <m:Items>
            <t:MeetingRequest>"#,
            r#"<m:GetItemResponseMessage ResponseClass="Error"><m:MessageText>gone</m:MessageText><m:ResponseCode>ErrorItemNotFound</m:ResponseCode><m:Items/></m:GetItemResponseMessage>
        <m:GetItemResponseMessage ResponseClass="Success">
          <m:ResponseCode>NoError</m:ResponseCode>
          <m:Items>
            <t:MeetingRequest>"#,
            1,
        );
        assert_ne!(mixed, fixtures::GET_ITEM_MIME);
        let items = parse_get_item_response(&skipping(&mixed).unwrap()).unwrap();
        assert_eq!(items.len(), 2);
        // Another error is not dropped, by post_soap or by the parser.
        let denied = fixtures::GET_ITEM_SECOND_MESSAGE_FAILS
            .replace("ErrorItemNotFound", "ErrorAccessDenied");
        let err = skipping(&denied).unwrap_err().to_string();
        assert!(err.contains("ErrorAccessDenied"), "{err}");
        let err = parse_get_item_response(&doc_of(&denied))
            .unwrap_err()
            .to_string();
        assert!(err.contains("ErrorAccessDenied"), "{err}");
        // Without the skip list, a gone item is a fault like any other.
        assert!(crate::ews::soap::read_reply(
            reqwest::StatusCode::OK,
            fixtures::GET_ITEM_SECOND_MESSAGE_FAILS,
            "e"
        )
        .is_err());
    }

    /// Warning with NoError is a success with a note, for the parsers as for
    /// `post_soap`; Warning with an error code is a failure for both.
    #[test]
    fn warning_with_no_error_is_a_success_everywhere() {
        let warning = fixtures::CREATE_ITEM.replace("\"Success\"", "\"Warning\"");
        assert_eq!(
            parse_create_item_response(&reply(&warning)).unwrap(),
            "AAMkADUwNjA3YTQ5AEYAAAAAAD+/b8xNEW="
        );
        let failed = warning.replace(">NoError<", ">ErrorBatchProcessingStopped<");
        assert!(crate::ews::soap::read_reply(reqwest::StatusCode::OK, &failed, "e").is_err());
        let err = parse_create_item_response(&doc_of(&failed))
            .unwrap_err()
            .to_string();
        assert!(err.contains("ErrorBatchProcessingStopped"), "{err}");
    }

    /// Every entry counts towards the cap and the paging offset, but only
    /// calendar items are returned.
    #[test]
    fn find_item_counts_every_entry() {
        let xml = r#"<m:FindItemResponseMessage ResponseClass="Success"><m:RootFolder><t:Items><t:CalendarItem><t:ItemId Id="A"/></t:CalendarItem><t:Message><t:ItemId Id="B"/></t:Message></t:Items></m:RootFolder></m:FindItemResponseMessage>"#;
        let page = parse_calendar_items_response(&doc(xml)).unwrap();
        assert_eq!(page.included_count, 2);
        assert_eq!(page.items.len(), 1);
        assert_eq!(page.total, None);
    }

    /// Regression: Exchange 2019 omits `<t:Recurrence>` on expanded occurrences
    /// returned by `CalendarView`, signaling recurrence via `CalendarItemType`
    /// instead. The parser must flag these as `has_recurrence=true` so that
    /// `synth_vcalendar` emits a `RECURRENCE-ID` and the upsert keys siblings
    /// uniquely.
    #[test]
    fn occurrence_calendar_item_type_marks_has_recurrence() {
        let xml = r#"<m:FindItemResponseMessage ResponseClass="Success">
  <m:RootFolder TotalItemsInView="1">
    <t:Items>
      <t:CalendarItem>
        <t:ItemId Id="OC1" />
        <t:Subject>Daily standup</t:Subject>
        <t:Start>2026-06-10T07:45:00Z</t:Start>
        <t:End>2026-06-10T08:00:00Z</t:End>
        <t:CalendarItemType>Occurrence</t:CalendarItemType>
        <t:IsAllDayEvent>false</t:IsAllDayEvent>
        <t:UID>shared-master-uid</t:UID>
      </t:CalendarItem>
    </t:Items>
  </m:RootFolder>
</m:FindItemResponseMessage>"#;
        let page = parse_calendar_items_response(&doc(xml)).unwrap();
        assert_eq!(page.items.len(), 1);
        assert!(
            page.items[0].has_recurrence,
            "Occurrence items must be flagged as recurring"
        );
    }

    #[test]
    fn single_calendar_item_type_does_not_mark_has_recurrence() {
        let xml = r#"<m:FindItemResponseMessage ResponseClass="Success">
  <m:RootFolder TotalItemsInView="1">
    <t:Items>
      <t:CalendarItem>
        <t:ItemId Id="SI1" />
        <t:Subject>One-off</t:Subject>
        <t:Start>2026-06-10T09:00:00Z</t:Start>
        <t:End>2026-06-10T09:30:00Z</t:End>
        <t:CalendarItemType>Single</t:CalendarItemType>
        <t:IsAllDayEvent>false</t:IsAllDayEvent>
        <t:UID>standalone-uid</t:UID>
      </t:CalendarItem>
    </t:Items>
  </m:RootFolder>
</m:FindItemResponseMessage>"#;
        let page = parse_calendar_items_response(&doc(xml)).unwrap();
        assert!(!page.items[0].has_recurrence);
    }

    #[test]
    fn parses_get_folder_response() {
        assert_eq!(
            parse_get_folder_response(&reply(fixtures::GET_FOLDER_CALENDAR)).unwrap(),
            Some(
                "AAMkADUwNjA3YTQ5LTEzZDktNGQ2Yy1iZDk3LTJiNzRkZGFiMmJmZgAuAAAAAAA+/b8xAQ=="
                    .to_string()
            )
        );
        let none = r#"<m:GetFolderResponseMessage ResponseClass="Success"><m:Folders/></m:GetFolderResponseMessage>"#;
        assert_eq!(parse_get_folder_response(&doc(none)).unwrap(), None);
        for xml in [
            "<s:Envelope><s:Body/></s:Envelope>",
            r#"<m:GetFolderResponseMessage ResponseClass="Success"/>"#,
            r#"<m:GetFolderResponseMessage ResponseClass="Success"><m:Folders><t:Folder/></m:Folders></m:GetFolderResponseMessage>"#,
        ] {
            assert!(parse_get_folder_response(&doc(xml)).is_err(), "{xml}");
        }
    }

    #[test]
    fn parses_create_item_response_returns_id() {
        let xml = r#"<m:CreateItemResponseMessage ResponseClass="Success">
  <m:Items>
    <t:CalendarItem>
      <t:ItemId Id="NEW123" ChangeKey="CK99" />
    </t:CalendarItem>
  </m:Items>
</m:CreateItemResponseMessage>"#;
        assert_eq!(parse_create_item_response(&doc(xml)).unwrap(), "NEW123");
        assert_eq!(
            parse_create_item_response(&reply(fixtures::CREATE_ITEM)).unwrap(),
            "AAMkADUwNjA3YTQ5AEYAAAAAAD+/b8xNEW="
        );
        for xml in [
            "<s:Envelope><s:Body/></s:Envelope>",
            r#"<m:CreateItemResponseMessage ResponseClass="Success"><m:Items/></m:CreateItemResponseMessage>"#,
            r#"<m:CreateItemResponseMessage ResponseClass="Success"><m:Items><t:CalendarItem/></m:Items></m:CreateItemResponseMessage>"#,
        ] {
            assert!(parse_create_item_response(&doc(xml)).is_err(), "{xml}");
        }
    }

    #[test]
    fn delete_item_needs_a_successful_message() {
        assert!(parse_delete_item_response(&reply(fixtures::DELETE_ITEM)).is_ok());
        assert!(parse_delete_item_response(&doc("<s:Envelope><s:Body/></s:Envelope>")).is_err());
    }

    #[test]
    fn parses_get_item_mime_content() {
        let items = parse_get_item_response(&reply(fixtures::GET_ITEM_MIME)).unwrap();
        assert_eq!(items.len(), 2, "a calendar item and a meeting request");
        assert_eq!(items[0].0, "AAMkADUwNjA3YTQ5AEYAAAAAAD+/b8xAAA=");
        assert_eq!(items[1].0, "AAMkADUwNjA3YTQ5AEYAAAAAAD+/b8xAAB=");
        for (_, ical) in &items {
            assert!(ical.starts_with("BEGIN:VCALENDAR"), "{ical}");
            assert!(ical.ends_with("END:VCALENDAR"), "{ical}");
            assert!(ical.contains("SUMMARY;LANGUAGE=en-US:Board review & budget"));
            assert!(ical.contains("RRULE:FREQ=WEEKLY;COUNT=4"));
        }
        // An item without MIME content is skipped, one without an id is not.
        let skipped = r#"<m:GetItemResponseMessage ResponseClass="Success"><m:Items><t:CalendarItem><t:ItemId Id="A"/></t:CalendarItem></m:Items></m:GetItemResponseMessage>"#;
        assert!(parse_get_item_response(&doc(skipped)).unwrap().is_empty());
        for xml in [
            "<s:Envelope><s:Body/></s:Envelope>".to_string(),
            r#"<m:GetItemResponseMessage ResponseClass="Success"/>"#.to_string(),
            format!(
                r#"<m:GetItemResponseMessage ResponseClass="Success"><m:Items><t:CalendarItem><t:MimeContent>{}</t:MimeContent></t:CalendarItem></m:Items></m:GetItemResponseMessage>"#,
                fixtures::GET_ITEM_MIME_B64
            ),
        ] {
            assert!(parse_get_item_response(&doc(&xml)).is_err(), "{xml}");
        }
    }

    #[test]
    fn extract_vcalendar_from_mime() {
        let mime = "MIME-Version: 1.0\r\nContent-Type: text/calendar\r\n\r\nBEGIN:VCALENDAR\r\nVERSION:2.0\r\nBEGIN:VEVENT\r\nUID:abc@example.com\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";
        let block = extract_vcalendar(mime).unwrap();
        assert!(block.starts_with("BEGIN:VCALENDAR"));
        assert!(block.ends_with("END:VCALENDAR"));
        assert!(block.contains("UID:abc@example.com"));
    }

    #[test]
    fn parses_sync_delta_with_create_update_delete() {
        let xml = r#"<m:SyncFolderItemsResponseMessage ResponseClass="Success">
  <m:SyncState>STATE2</m:SyncState>
  <m:IncludesLastItemInRange>true</m:IncludesLastItemInRange>
  <m:Changes>
    <t:Create>
      <t:CalendarItem>
        <t:ItemId Id="N1" ChangeKey="CK1" />
        <t:UID>uid-new</t:UID>
      </t:CalendarItem>
    </t:Create>
    <t:Update>
      <t:CalendarItem>
        <t:ItemId Id="N2" ChangeKey="CK2" />
        <t:UID>uid-upd</t:UID>
      </t:CalendarItem>
    </t:Update>
    <t:Delete>
      <t:ItemId Id="GONE" />
    </t:Delete>
  </m:Changes>
</m:SyncFolderItemsResponseMessage>"#;
        let d = parse_sync_folder_items_response(&doc(xml)).unwrap();
        assert_eq!(d.new_sync_state.as_deref(), Some("STATE2"));
        assert!(d.includes_last);
        assert_eq!(d.added_or_changed.len(), 2);
        assert_eq!(d.added_or_changed[0].0, "N1");
        assert_eq!(d.added_or_changed[0].1, "uid-new");
        assert_eq!(d.deleted_item_ids, vec!["GONE"]);
    }

    #[test]
    fn parses_an_exchange_sync_delta() {
        let d = parse_sync_folder_items_response(&reply(fixtures::SYNC_FOLDER_ITEMS)).unwrap();
        assert!(d.new_sync_state.unwrap().starts_with("H4sIAAAA"));
        assert!(d.includes_last);
        assert_eq!(
            d.added_or_changed,
            [
                ("N1=".to_string(), "uid-new@corp.example".to_string()),
                ("N2=".to_string(), "uid-upd&co@corp.example".to_string()),
            ]
        );
        assert_eq!(d.deleted_item_ids, ["GONE="]);
    }

    #[test]
    fn sync_delta_refuses_what_it_cannot_read() {
        let message = |inner: &str| {
            format!(
                r#"<m:SyncFolderItemsResponseMessage ResponseClass="Success">{inner}</m:SyncFolderItemsResponseMessage>"#
            )
        };
        let state = "<m:SyncState>S</m:SyncState><m:IncludesLastItemInRange>true</m:IncludesLastItemInRange>";
        for xml in [
            "<s:Envelope><s:Body/></s:Envelope>".to_string(),
            message("<m:IncludesLastItemInRange>true</m:IncludesLastItemInRange><m:Changes/>"),
            message("<m:SyncState>S</m:SyncState><m:Changes/>"),
            message("<m:SyncState>S</m:SyncState><m:IncludesLastItemInRange>perhaps</m:IncludesLastItemInRange><m:Changes/>"),
            message(state),
            message(&format!("{state}<m:Changes><t:Create/></m:Changes>")),
            message(&format!("{state}<m:Changes><t:Update><t:CalendarItem><t:UID>u</t:UID></t:CalendarItem></t:Update></m:Changes>")),
            message(&format!("{state}<m:Changes><t:Delete/></m:Changes>")),
        ] {
            assert!(parse_sync_folder_items_response(&doc(&xml)).is_err(), "{xml}");
        }
    }
}
