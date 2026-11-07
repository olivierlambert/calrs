//! Reading WebDAV multistatus answers (RFC 4918 section 13) for the CalDAV
//! client, on the namespace-aware tree of `crate::xml` (#325).
//!
//! Elements are matched by namespace URI and local name, so any prefix, an
//! uppercase one, a default namespace, or a namespace declared on the
//! element itself (Kerio Connect declares CalDAV's on each `<response>`) all
//! read the same. Properties are only taken from a `<propstat>` whose status
//! is 2xx: a property a server reports as 404 is absent, not empty. The
//! resource-level `<status>`, a direct child of `<response>`, is kept apart
//! from the property-level ones inside each propstat, which is what tells a
//! deleted event from one that merely lacks a property (#107).
//!
//! Lenient where servers are known to cut corners, strict where reading
//! wrong would cost data: a propstat with no status, or a `<prop>` directly
//! under `<response>`, still counts; a name under a prefix that was never
//! declared matches on its local name (see `xml::Element::is`); but
//! malformed markup, or a body that is not a multistatus at all (an HTML
//! error page), is an error rather than "no events", since an empty event
//! list would let the sync delete the cached calendar.

use anyhow::{bail, Result};

use super::{CalendarInfo, RawEvent, SyncResult};
use crate::xml::{self, Element};

const DAV: &str = "DAV:";
const CALDAV: &str = "urn:ietf:params:xml:ns:caldav";
const CALENDARSERVER: &str = "http://calendarserver.org/ns/";
const APPLE_ICAL: &str = "http://apple.com/ns/ical/";

/// A parsed multistatus body. An empty body holds no responses, as a
/// server answering an empty calendar with nothing at all is common enough.
struct Multistatus(Option<Element>);

impl Multistatus {
    fn parse(body: &str) -> Result<Self> {
        let root = xml::parse(body)?;
        if let Some(root) = &root {
            if !root.is(DAV, "multistatus") {
                bail!(
                    "expected a WebDAV multistatus, got a <{}> document",
                    root.local_name()
                );
            }
        }
        Ok(Self(root))
    }

    fn responses(&self) -> impl Iterator<Item = Response<'_>> {
        self.0
            .iter()
            .flat_map(|root| root.children(DAV, "response"))
            .map(Response)
    }

    /// A direct child of `<multistatus>`, such as a sync-collection's
    /// `<sync-token>`.
    fn child(&self, ns: &str, name: &str) -> Option<&Element> {
        self.0.as_ref()?.child(ns, name)
    }
}

/// One `<response>` of a multistatus.
#[derive(Clone, Copy)]
struct Response<'a>(&'a Element);

impl<'a> Response<'a> {
    /// The resource's href exactly as the server wrote it (entities decoded,
    /// percent-encoding and absolute URLs kept), or empty when missing.
    fn href(self) -> String {
        self.0
            .child(DAV, "href")
            .and_then(Element::trimmed_text)
            .unwrap_or_default()
    }

    /// The resource-level status code: the `<status>` directly under
    /// `<response>`, never one inside a `<propstat>`.
    fn status_code(self) -> Option<Option<u16>> {
        self.0
            .child(DAV, "status")
            .map(|status| status_code(&status.text()))
    }

    fn propstats(self) -> impl Iterator<Item = &'a Element> {
        self.0.children(DAV, "propstat")
    }

    /// Every property the server granted: the children of each `<prop>` in
    /// a 2xx (or status-less) propstat, then of any `<prop>` placed directly
    /// under `<response>`.
    fn props(self) -> impl Iterator<Item = &'a Element> {
        let granted = self
            .propstats()
            .filter(|propstat| propstat_granted(propstat))
            .flat_map(|propstat| propstat.children(DAV, "prop"));
        granted
            .chain(self.0.children(DAV, "prop"))
            .flat_map(Element::elements)
    }

    fn prop(self, ns: &str, name: &str) -> Option<&'a Element> {
        self.props().find(|prop| prop.is(ns, name))
    }

    fn prop_text(self, ns: &str, name: &str) -> Option<String> {
        self.prop(ns, name).and_then(Element::trimmed_text)
    }

    /// The event's iCalendar data, line endings normalised to CRLF.
    fn calendar_data(self) -> Option<String> {
        self.prop_text(CALDAV, "calendar-data")
            .map(|ics| crate::utils::normalize_line_endings(&ics).into_owned())
    }

    /// The href inside a property such as `<current-user-principal>`.
    fn prop_href(self, ns: &str, name: &str) -> Option<String> {
        self.prop(ns, name)?
            .find(DAV, "href")
            .and_then(Element::trimmed_text)
    }
}

/// Whether a propstat's properties were returned: its status is 2xx, or it
/// has none (invalid, but read the way the old extractor did).
fn propstat_granted(propstat: &Element) -> bool {
    match propstat.child(DAV, "status") {
        None => true,
        Some(status) => status_code(&status.text()).is_some_and(|code| (200..300).contains(&code)),
    }
}

/// The code of an HTTP status line such as `HTTP/1.1 404 Not Found`: its
/// first token of exactly three digits, so a bare `404` reads too.
fn status_code(line: &str) -> Option<u16> {
    line.split_whitespace()
        .find(|token| token.len() == 3 && token.bytes().all(|b| b.is_ascii_digit()))
        .and_then(|code| code.parse().ok())
}

fn is_gone(code: Option<u16>) -> bool {
    matches!(code, Some(404 | 410))
}

/// The calendar collections in the answer to a depth:1 PROPFIND on a
/// calendar home: responses whose resourcetype holds CalDAV's `calendar`,
/// which skips the home itself, plain collections, principals and
/// addressbooks.
pub(super) fn parse_calendar_list(body: &str) -> Result<Vec<CalendarInfo>> {
    let ms = Multistatus::parse(body)?;
    let mut calendars = Vec::new();
    for response in ms.responses() {
        let is_calendar = response
            .prop(DAV, "resourcetype")
            .is_some_and(|types| types.child(CALDAV, "calendar").is_some());
        if !is_calendar {
            continue;
        }
        let href = response.href();
        if href.is_empty() {
            continue;
        }
        calendars.push(CalendarInfo {
            href,
            display_name: response.prop_text(DAV, "displayname"),
            color: response.prop_text(APPLE_ICAL, "calendar-color"),
            ctag: response.prop_text(CALENDARSERVER, "getctag"),
            sync_token: response.prop_text(DAV, "sync-token"),
        });
    }
    Ok(calendars)
}

/// The events of a calendar-query REPORT: every response carrying
/// calendar data. A response without any (a 404, or a resource the server
/// would not render) is skipped.
pub(super) fn parse_event_responses(body: &str) -> Result<Vec<RawEvent>> {
    let ms = Multistatus::parse(body)?;
    Ok(ms
        .responses()
        .filter_map(|response| {
            let ical_data = response.calendar_data()?;
            Some(RawEvent {
                href: response.href(),
                ical_data,
            })
        })
        .collect())
}

/// A sync-collection REPORT (RFC 6578): the new sync token, the changed
/// events, and the hrefs whose resource-level status is 404 or 410.
pub(super) fn parse_sync_response(body: &str) -> Result<SyncResult> {
    let ms = Multistatus::parse(body)?;
    let mut changed = Vec::new();
    let mut deleted_hrefs = Vec::new();
    for response in ms.responses() {
        let href = response.href();
        if href.is_empty() {
            continue;
        }
        // Only the resource-level status says the resource is gone. A 404
        // inside a propstat means one property is missing on a resource
        // that exists; reading it as a deletion once cancelled live
        // bookings in production (#107).
        if response.status_code().is_some_and(is_gone) {
            deleted_hrefs.push(href);
            continue;
        }
        if let Some(ical_data) = response.calendar_data() {
            changed.push(RawEvent { href, ical_data });
        }
    }
    Ok(SyncResult {
        new_sync_token: ms.child(DAV, "sync-token").and_then(Element::trimmed_text),
        changed,
        deleted_hrefs,
    })
}

/// Read the multistatus answer to a depth:0 PROPFIND on one resource. Depth 0
/// returns a single `<response>`, so the first one is the resource asked about.
///
/// - `Some(false)`: its resource-level `<status>` is 404 or 410.
/// - `Some(true)`: its resource-level status is 2xx, or it has no
///   resource-level status but carries a `<propstat>` with content. A
///   propstat 404 means a property is missing on a resource that exists,
///   not that the resource is gone (the misreading `parse_sync_response`
///   guards against).
/// - `None`: no response, another resource-level status, or a body that is
///   not a well-formed multistatus; the caller can't tell and must not act
///   on it.
pub(super) fn propfind_resource_exists(body: &str) -> Option<bool> {
    let ms = Multistatus::parse(body).ok()?;
    let response = ms.responses().next()?;
    if let Some(code) = response.status_code() {
        if is_gone(code) {
            return Some(false);
        }
        return code
            .is_some_and(|code| (200..300).contains(&code))
            .then_some(true);
    }
    let has_propstat = response.propstats().any(Element::has_children);
    has_propstat.then_some(true)
}

/// The `current-user-principal` href of a PROPFIND answer, `None` when the
/// server sent none (or `<unauthenticated/>` instead).
pub(super) fn current_user_principal(body: &str) -> Result<Option<String>> {
    first_prop_href(body, DAV, "current-user-principal")
}

/// The first `calendar-home-set` href of a PROPFIND answer.
pub(super) fn calendar_home_set(body: &str) -> Result<Option<String>> {
    first_prop_href(body, CALDAV, "calendar-home-set")
}

fn first_prop_href(body: &str, ns: &str, name: &str) -> Result<Option<String>> {
    let ms = Multistatus::parse(body)?;
    let href = ms
        .responses()
        .find_map(|response| response.prop_href(ns, name));
    Ok(href)
}

#[cfg(test)]
mod tests;
