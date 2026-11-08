//! Tests for the multistatus reader: responses in the shapes the servers
//! calrs is used with actually send (captured ones where the repository has
//! them: Radicale #132, SOGo #15, Kerio Connect #312), the regressions the
//! string-based extractor collected over the years, and malformed input.

use super::*;

fn cals(xml: &str) -> Vec<CalendarInfo> {
    parse_calendar_list(xml).unwrap()
}

fn events(xml: &str) -> Vec<RawEvent> {
    parse_event_responses(xml).unwrap()
}

fn sync(xml: &str) -> SyncResult {
    parse_sync_response(xml).unwrap()
}

fn principal(xml: &str) -> Option<String> {
    current_user_principal(xml).unwrap()
}

fn home(xml: &str) -> Option<String> {
    calendar_home_set(xml).unwrap()
}

/// iCalendar lines joined with CRLF, as calendar data reads after
/// normalisation (the trailing line break is trimmed away).
fn ics(lines: &[&str]) -> String {
    lines.join("\r\n")
}

/// A one-calendar listing whose 200 propstat holds `props`.
fn listing_with_props(props: &str) -> String {
    format!(
        r#"<d:multistatus xmlns:d="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav" xmlns:CS="http://calendarserver.org/ns/" xmlns:A="http://apple.com/ns/ical/"><d:response><d:href>/cal/</d:href><d:propstat><d:prop><d:resourcetype><d:collection/><C:calendar/></d:resourcetype>{props}</d:prop><d:status>HTTP/1.1 200 OK</d:status></d:propstat></d:response></d:multistatus>"#
    )
}

fn display_name(props: &str) -> Option<String> {
    cals(&listing_with_props(props)).pop().unwrap().display_name
}

/// Whether a response with this resourcetype content is listed.
fn listed(resourcetype: &str) -> bool {
    let xml = format!(
        r#"<d:multistatus xmlns:d="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav" xmlns:CR="urn:ietf:params:xml:ns:carddav" xmlns:ICAL="http://apple.com/ns/ical/"><d:response><d:href>/x/</d:href><d:propstat><d:prop><d:resourcetype>{resourcetype}</d:resourcetype></d:prop><d:status>HTTP/1.1 200 OK</d:status></d:propstat></d:response></d:multistatus>"#
    );
    !cals(&xml).is_empty()
}

// --- Property values ---

#[test]
fn property_values_are_trimmed_and_empty_ones_absent() {
    assert_eq!(
        display_name("<d:displayname>  My Calendar  </d:displayname>").as_deref(),
        Some("My Calendar")
    );
    assert_eq!(display_name("<d:displayname></d:displayname>"), None);
    assert_eq!(display_name("<d:displayname/>"), None);
    assert_eq!(display_name("<d:displayname>\n   \n</d:displayname>"), None);
    assert_eq!(display_name(""), None);
}

#[test]
fn property_values_unwrap_cdata() {
    // Fastmail wraps display names in CDATA.
    assert_eq!(
        display_name("<d:displayname><![CDATA[Work]]></d:displayname>").as_deref(),
        Some("Work")
    );
    assert_eq!(
        display_name("<d:displayname><![CDATA[ R&D <team> ]]></d:displayname>").as_deref(),
        Some("R&D <team>")
    );
    assert_eq!(
        display_name("<d:displayname><![CDATA[]]></d:displayname>"),
        None
    );
    // A close tag inside CDATA does not end the element.
    assert_eq!(
        display_name("<d:displayname><![CDATA[a</d:displayname>b]]></d:displayname>").as_deref(),
        Some("a</d:displayname>b")
    );
    // Text and CDATA next to each other join.
    assert_eq!(
        display_name("<d:displayname>Q&amp;A <![CDATA[<live>]]> room</d:displayname>").as_deref(),
        Some("Q&A <live> room")
    );
}

#[test]
fn property_values_decode_entities() {
    assert_eq!(
        display_name("<d:displayname>Sales &amp; Marketing &#x2764;&#65;</d:displayname>")
            .as_deref(),
        Some("Sales & Marketing \u{2764}A")
    );
    // Unknown references and stray ampersands are left as written.
    assert_eq!(
        display_name("<d:displayname>a &lt; b &unknown; &amp; R & D</d:displayname>").as_deref(),
        Some("a < b &unknown; & R & D")
    );
}

#[test]
fn property_with_attributes_reads_its_text() {
    let cal = cals(&listing_with_props(
        r#"<A:calendar-color symbolic-color="custom">#1F6BFF</A:calendar-color>"#,
    ))
    .pop()
    .unwrap();
    assert_eq!(cal.color.as_deref(), Some("#1F6BFF"));
}

#[test]
fn sync_token_roundtrips_through_xml() {
    let xml = r#"<d:multistatus xmlns:d="DAV:"><d:sync-token>https://example.com/sync?a=1&amp;b=&lt;2&gt;</d:sync-token></d:multistatus>"#;
    let token = sync(xml).new_sync_token.unwrap();
    assert_eq!(token, "https://example.com/sync?a=1&b=<2>");
    assert_eq!(
        super::super::xml_escape(&token),
        "https://example.com/sync?a=1&amp;b=&lt;2&gt;"
    );
}

#[test]
fn calendar_data_decodes_entities_and_normalises_line_endings() {
    let xml = "<d:multistatus xmlns:d=\"DAV:\" xmlns:C=\"urn:ietf:params:xml:ns:caldav\"><d:response><d:href>/e.ics</d:href><d:propstat><d:prop>\
<C:calendar-data>BEGIN:VCALENDAR&#13;\nSUMMARY:a &lt; b &unknown;\nEND:VCALENDAR&#13;\n</C:calendar-data>\
</d:prop><d:status>HTTP/1.1 200 OK</d:status></d:propstat></d:response></d:multistatus>";
    assert_eq!(
        events(xml)[0].ical_data,
        ics(&[
            "BEGIN:VCALENDAR",
            "SUMMARY:a < b &unknown;",
            "END:VCALENDAR"
        ])
    );
}

// --- Namespaces and prefixes ---

#[test]
fn prefixes_are_read_by_namespace_not_spelling() {
    // The same listing under lowercase, uppercase, arbitrary and default
    // prefixes reads identically.
    let variants = [
        r#"<d:multistatus xmlns:d="DAV:" xmlns:c="urn:ietf:params:xml:ns:caldav" xmlns:cs="http://calendarserver.org/ns/"><d:response><d:href>/c/</d:href><d:propstat><d:prop><d:resourcetype><c:calendar/></d:resourcetype><d:displayname>N</d:displayname><cs:getctag>1</cs:getctag></d:prop><d:status>HTTP/1.1 200 OK</d:status></d:propstat></d:response></d:multistatus>"#,
        r#"<D:multistatus xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav" xmlns:CS="http://calendarserver.org/ns/"><D:response><D:href>/c/</D:href><D:propstat><D:prop><D:resourcetype><C:calendar/></D:resourcetype><D:displayname>N</D:displayname><CS:getctag>1</CS:getctag></D:prop><D:status>HTTP/1.1 200 OK</D:status></D:propstat></D:response></D:multistatus>"#,
        r#"<ns0:multistatus xmlns:ns0="DAV:" xmlns:ns1="urn:ietf:params:xml:ns:caldav" xmlns:my-ns="http://calendarserver.org/ns/"><ns0:response><ns0:href>/c/</ns0:href><ns0:propstat><ns0:prop><ns0:resourcetype><ns1:calendar/></ns0:resourcetype><ns0:displayname>N</ns0:displayname><my-ns:getctag>1</my-ns:getctag></ns0:prop><ns0:status>HTTP/1.1 200 OK</ns0:status></ns0:propstat></ns0:response></ns0:multistatus>"#,
        r#"<multistatus xmlns="DAV:"><response><href>/c/</href><propstat><prop><resourcetype><calendar xmlns="urn:ietf:params:xml:ns:caldav"/></resourcetype><displayname>N</displayname><getctag xmlns="http://calendarserver.org/ns/">1</getctag></prop><status>HTTP/1.1 200 OK</status></propstat></response></multistatus>"#,
    ];
    for xml in variants {
        let list = cals(xml);
        assert_eq!(list.len(), 1, "{xml}");
        assert_eq!(list[0].href, "/c/");
        assert_eq!(list[0].display_name.as_deref(), Some("N"));
        assert_eq!(list[0].ctag.as_deref(), Some("1"));
    }
}

#[test]
fn a_prefix_bound_to_another_namespace_does_not_match() {
    // A colour in another namespace is not Apple's calendar-color.
    let cal = cals(&listing_with_props(
        r#"<x:calendar-color xmlns:x="urn:other">#000000</x:calendar-color>"#,
    ))
    .pop()
    .unwrap();
    assert_eq!(cal.color, None);
}

#[test]
fn undeclared_prefixes_still_match_by_local_name() {
    // The tolerance the old extractor had: a server (or fixture) using a
    // prefix it never declared still reads.
    let xml = r#"
<d:multistatus>
  <d:response>
    <d:href>/dav/calendars/alice/</d:href>
    <d:propstat><d:prop>
      <d:resourcetype><d:collection/></d:resourcetype>
    </d:prop></d:propstat>
  </d:response>
  <d:response>
    <d:href>/dav/calendars/alice/default/</d:href>
    <d:propstat><d:prop>
      <d:resourcetype><d:collection/><cal:calendar/></d:resourcetype>
      <d:displayname>Personal</d:displayname>
    </d:prop></d:propstat>
  </d:response>
  <d:response>
    <d:href>/dav/calendars/alice/work/</d:href>
    <d:propstat><d:prop>
      <d:resourcetype><d:collection/><cal:calendar/></d:resourcetype>
      <d:displayname>Work</d:displayname>
      <aic:calendar-color>#FF0000</aic:calendar-color>
      <cso:getctag>ctag-123</cso:getctag>
    </d:prop></d:propstat>
  </d:response>
</d:multistatus>"#;
    let list = cals(xml);
    assert_eq!(list.len(), 2);
    assert_eq!(list[0].href, "/dav/calendars/alice/default/");
    assert_eq!(list[0].display_name.as_deref(), Some("Personal"));
    assert_eq!(list[0].color, None);
    assert_eq!(list[1].href, "/dav/calendars/alice/work/");
    assert_eq!(list[1].display_name.as_deref(), Some("Work"));
    assert_eq!(list[1].color.as_deref(), Some("#FF0000"));
    assert_eq!(list[1].ctag.as_deref(), Some("ctag-123"));
}

#[test]
fn calendar_resourcetype_distinguishes_lookalikes() {
    assert!(listed("<C:calendar />"));
    assert!(listed("<C:calendar/>"));
    assert!(listed("<C:calendar></C:calendar>"));
    assert!(listed(
        r#"<calendar xmlns="urn:ietf:params:xml:ns:caldav"/>"#
    ));
    assert!(listed(
        r#"<d:collection/><x:calendar xmlns:x="urn:ietf:params:xml:ns:caldav"/>"#
    ));
    assert!(!listed("<ICAL:calendar-color />"));
    assert!(!listed(
        "<C:calendar-data>BEGIN:VCALENDAR</C:calendar-data>"
    ));
    assert!(!listed("<C:supported-calendar-component-set />"));
    assert!(!listed("<CR:addressbook /><d:collection />"));
    assert!(!listed("<d:collection/>"));
    assert!(!listed("<C:schedule-inbox/><d:collection/>"));
    assert!(!listed(
        r#"<vevent-collection xmlns="http://groupdav.org/"/>"#
    ));
}

#[test]
fn calendar_and_calendar_data_are_read_outside_the_caldav_namespace() {
    // No other namespace defines these names, and servers that write them
    // in DAV: (or under a default namespace without declaring CalDAV's)
    // must keep their calendars listed and their events read.
    assert!(listed("<d:calendar/>"));
    assert!(listed("<d:collection/><calendar/>"));
    let xml = r#"<multistatus xmlns="DAV:"><response><href>/cal/</href><propstat><prop><resourcetype><collection/><calendar/></resourcetype><displayname>Work</displayname></prop><status>HTTP/1.1 200 OK</status></propstat></response></multistatus>"#;
    assert_eq!(cals(xml)[0].display_name.as_deref(), Some("Work"));
    let xml = r#"<multistatus xmlns="DAV:"><response><href>/cal/e.ics</href><propstat><prop><getetag>"1"</getetag><calendar-data>BEGIN:VCALENDAR</calendar-data></prop><status>HTTP/1.1 200 OK</status></propstat></response></multistatus>"#;
    assert_eq!(events(xml)[0].ical_data, "BEGIN:VCALENDAR");
}

#[test]
fn the_first_non_empty_property_value_wins() {
    // Kerio's status-less propstats: an empty placeholder, then the value.
    let xml = r#"<a:multistatus xmlns:a="DAV:" xmlns:g="urn:ietf:params:xml:ns:caldav"><a:response><a:href>/u/CAL/</a:href><a:propstat><a:prop><a:displayname/><g:calendar-data/></a:prop></a:propstat><a:propstat><a:prop><a:resourcetype><g:calendar/></a:resourcetype><a:displayname>Calendar</a:displayname><g:calendar-data>BEGIN:VCALENDAR</g:calendar-data></a:prop></a:propstat></a:response></a:multistatus>"#;
    assert_eq!(cals(xml)[0].display_name.as_deref(), Some("Calendar"));
    assert_eq!(events(xml)[0].ical_data, "BEGIN:VCALENDAR");
}

#[test]
fn a_calendar_element_outside_resourcetype_does_not_count() {
    let xml = r#"<d:multistatus xmlns:d="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav"><d:response><d:href>/x/</d:href><d:propstat><d:prop><d:resourcetype><d:collection/></d:resourcetype><x:wrapper xmlns:x="urn:x"><C:calendar/></x:wrapper></d:prop><d:status>HTTP/1.1 200 OK</d:status></d:propstat></d:response></d:multistatus>"#;
    assert!(cals(xml).is_empty());
}

// --- Propstat statuses ---

#[test]
fn only_granted_propstats_supply_properties() {
    // A 404 or 403 propstat names properties the server does not return,
    // even when (unusually) it writes a value in them.
    let xml = r#"<d:multistatus xmlns:d="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav" xmlns:A="http://apple.com/ns/ical/">
<d:response><d:href>/cal/</d:href>
  <d:propstat><d:prop><d:displayname>Ghost</d:displayname><A:calendar-color>#BAD</A:calendar-color></d:prop><d:status>HTTP/1.1 404 Not Found</d:status></d:propstat>
  <d:propstat><d:prop><d:sync-token>forbidden</d:sync-token></d:prop><d:status>HTTP/1.1 403 Forbidden</d:status></d:propstat>
  <d:propstat><d:prop><d:resourcetype><C:calendar/></d:resourcetype><d:displayname>Real</d:displayname></d:prop><d:status>
     HTTP/1.1 200 OK
  </d:status></d:propstat>
</d:response></d:multistatus>"#;
    let cal = cals(xml).pop().unwrap();
    assert_eq!(cal.display_name.as_deref(), Some("Real"));
    assert_eq!(cal.color, None);
    assert_eq!(cal.sync_token, None);

    // A resourcetype in a 404 propstat does not make a calendar.
    let xml = r#"<d:multistatus xmlns:d="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav"><d:response><d:href>/x/</d:href><d:propstat><d:prop><d:resourcetype><C:calendar/></d:resourcetype></d:prop><d:status>HTTP/1.1 404 Not Found</d:status></d:propstat></d:response></d:multistatus>"#;
    assert!(cals(xml).is_empty());
}

#[test]
fn propstat_without_status_and_prop_without_propstat_are_read() {
    // Kerio Connect sends propstats with no status at all.
    let xml = r#"<d:multistatus xmlns:d="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav"><d:response><d:href>/a/</d:href><d:propstat><d:prop><d:resourcetype><C:calendar/></d:resourcetype><d:displayname>A</d:displayname></d:prop></d:propstat></d:response><d:response><d:href>/b/</d:href><d:prop><d:resourcetype><C:calendar/></d:resourcetype><d:displayname>B</d:displayname></d:prop></d:response></d:multistatus>"#;
    let list = cals(xml);
    assert_eq!(list.len(), 2);
    assert_eq!(list[0].display_name.as_deref(), Some("A"));
    assert_eq!(list[1].display_name.as_deref(), Some("B"));
}

#[test]
fn status_codes_are_read_from_status_lines() {
    assert_eq!(status_code("HTTP/1.1 200 OK"), Some(200));
    assert_eq!(status_code("  HTTP/1.1   404   Not Found \n"), Some(404));
    assert_eq!(status_code("HTTP/2 410 Gone"), Some(410));
    assert_eq!(status_code("404"), Some(404));
    assert_eq!(status_code("HTTP/1.1 OK"), None);
    assert_eq!(status_code(""), None);
    assert_eq!(status_code("HTTP/1.1 2000 Odd"), None);
}

// --- Hrefs ---

#[test]
fn hrefs_are_kept_as_written() {
    let xml = r#"<d:multistatus xmlns:d="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav">
<d:response><d:href>
    /remote.php/dav/calendars/alice/team%20%26%20projects/
</d:href><d:propstat><d:prop><d:resourcetype><C:calendar/></d:resourcetype></d:prop><d:status>HTTP/1.1 200 OK</d:status></d:propstat></d:response>
<d:response><d:href>https://p42-caldav.icloud.com:443/123/calendars/work/</d:href><d:propstat><d:prop><d:resourcetype><C:calendar/></d:resourcetype></d:prop><d:status>HTTP/1.1 200 OK</d:status></d:propstat></d:response>
<d:response><d:href>/dav/calendars/__uids__/6A1F/calendar:Default:6A1F/</d:href><d:propstat><d:prop><d:resourcetype><C:calendar/></d:resourcetype></d:prop><d:status>HTTP/1.1 200 OK</d:status></d:propstat></d:response>
<d:response><d:href>/cal/a&amp;b%40c/</d:href><d:propstat><d:prop><d:resourcetype><C:calendar/></d:resourcetype></d:prop><d:status>HTTP/1.1 200 OK</d:status></d:propstat></d:response>
</d:multistatus>"#;
    let hrefs: Vec<String> = cals(xml).into_iter().map(|c| c.href).collect();
    assert_eq!(
        hrefs,
        [
            "/remote.php/dav/calendars/alice/team%20%26%20projects/",
            "https://p42-caldav.icloud.com:443/123/calendars/work/",
            "/dav/calendars/__uids__/6A1F/calendar:Default:6A1F/",
            "/cal/a&b%40c/",
        ]
    );
}

#[test]
fn the_response_href_wins_over_hrefs_inside_properties() {
    // An href inside a property (here an owner) that comes first in the
    // document must not be taken for the resource's own.
    let xml = r#"<d:multistatus xmlns:d="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav"><d:response><d:propstat><d:prop><d:owner><d:href>/principals/bob/</d:href></d:owner><d:resourcetype><C:calendar/></d:resourcetype></d:prop><d:status>HTTP/1.1 200 OK</d:status></d:propstat><d:href>/cal/bob/</d:href></d:response></d:multistatus>"#;
    assert_eq!(cals(xml)[0].href, "/cal/bob/");
}

#[test]
fn a_calendar_without_href_is_skipped() {
    let xml = r#"<d:multistatus xmlns:d="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav"><d:response><d:href> </d:href><d:propstat><d:prop><d:resourcetype><C:calendar/></d:resourcetype></d:prop></d:propstat></d:response></d:multistatus>"#;
    assert!(cals(xml).is_empty());
}

// --- Malformed and unexpected bodies ---

#[test]
fn an_empty_multistatus_holds_no_responses() {
    assert!(cals("<d:multistatus></d:multistatus>").is_empty());
    assert!(cals("<d:multistatus xmlns:d=\"DAV:\"/>").is_empty());
    let empty = sync("<multistatus xmlns=\"DAV:\"/>");
    assert!(empty.changed.is_empty() && empty.deleted_hrefs.is_empty());
    assert_eq!(empty.new_sync_token, None);
}

#[test]
fn a_plain_text_or_trailing_garbage_body() {
    // Text with no element is not a document.
    assert!(parse_event_responses("Internal Server Error").is_err());
    // A sabre/dav server with PHP `display_errors` on appends warnings after
    // the multistatus; the document itself still reads.
    let xml = "<?xml version=\"1.0\"?>\n<d:multistatus xmlns:d=\"DAV:\" xmlns:cal=\"urn:ietf:params:xml:ns:caldav\"><d:response><d:href>/dav.php/calendars/alice/default/e.ics</d:href><d:propstat><d:prop><cal:calendar-data>BEGIN:VCALENDAR</cal:calendar-data></d:prop><d:status>HTTP/1.1 200 OK</d:status></d:propstat></d:response></d:multistatus>\n<br />\n<b>Deprecated</b>:  Creation of dynamic property Sabre\\DAV\\Server::$debugExceptions is deprecated in <b>/var/www/baikal/vendor/sabre/dav/lib/DAV/Server.php</b> on line <b>233</b><br />\n";
    assert_eq!(events(xml).len(), 1);
}

#[test]
fn an_empty_body_is_an_error() {
    // Read as "no events", it would let the sync store the calendar's new
    // ctag without having read anything.
    for body in ["", "  \r\n", "<?xml version=\"1.0\"?>"] {
        let err = parse_event_responses(body).unwrap_err().to_string();
        assert!(err.contains("empty response"), "{body:?}: {err}");
        assert!(parse_sync_response(body).is_err());
        assert!(parse_calendar_list(body).is_err());
        assert!(calendar_home_set(body).is_err());
        assert_eq!(propfind_resource_exists(body), None);
    }
}

#[test]
fn malformed_bodies_are_errors_not_empty_results() {
    let truncated = r#"<d:multistatus xmlns:d="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav"><d:response><d:href>/e.ics</d:href><d:propstat><d:prop><C:calendar-data>BEGIN:VCALENDAR"#;
    let mismatched =
        r#"<d:multistatus xmlns:d="DAV:"><d:response><d:href>/e.ics</d:response></d:multistatus>"#;
    let html = "<html><head><title>502 Bad Gateway</title></head><body><hr><center>nginx</center></body></html>";
    for body in [truncated, mismatched, html] {
        assert!(parse_event_responses(body).is_err(), "{body}");
        assert!(parse_sync_response(body).is_err(), "{body}");
        assert!(parse_calendar_list(body).is_err(), "{body}");
        assert!(current_user_principal(body).is_err(), "{body}");
        assert_eq!(propfind_resource_exists(body), None, "{body}");
    }
}

#[test]
fn a_document_that_is_not_a_multistatus_is_an_error() {
    // A well-formed error page, or a WebDAV precondition error, is not an
    // empty calendar: reading it as one would let the sync drop the cache.
    for body in [
        "<html><body><p>Service unavailable</p></body></html>",
        r#"<d:error xmlns:d="DAV:"><d:valid-sync-token/></d:error>"#,
        r#"<multistatus xmlns="urn:not-dav"/>"#,
    ] {
        let err = parse_event_responses(body).unwrap_err().to_string();
        assert!(err.contains("expected a WebDAV multistatus"), "{err}");
        assert_eq!(propfind_resource_exists(body), None);
    }
}

#[test]
fn a_byte_order_mark_is_skipped() {
    let xml = "\u{feff}<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<d:multistatus xmlns:d=\"DAV:\"><d:response><d:href>/</d:href><d:propstat><d:prop><d:current-user-principal><d:href>/p/</d:href></d:current-user-principal></d:prop><d:status>HTTP/1.1 200 OK</d:status></d:propstat></d:response></d:multistatus>";
    assert_eq!(principal(xml).as_deref(), Some("/p/"));
}

#[test]
fn entity_expansion_attacks_do_nothing() {
    let xml = r#"<?xml version="1.0"?>
<!DOCTYPE d:multistatus [
 <!ENTITY a "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa">
 <!ENTITY b "&a;&a;&a;&a;&a;&a;&a;&a;&a;&a;&a;&a;&a;&a;&a;&a;">
 <!ENTITY c "&b;&b;&b;&b;&b;&b;&b;&b;&b;&b;&b;&b;&b;&b;&b;&b;">
 <!ENTITY xxe SYSTEM "file:///etc/passwd">
]>
<d:multistatus xmlns:d="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav"><d:response><d:href>/cal/</d:href><d:propstat><d:prop><d:resourcetype><C:calendar/></d:resourcetype><d:displayname>&c;&xxe;</d:displayname></d:prop><d:status>HTTP/1.1 200 OK</d:status></d:propstat></d:response></d:multistatus>"#;
    assert_eq!(cals(xml)[0].display_name.as_deref(), Some("&c;&xxe;"));
}

// --- Principal and calendar home discovery ---

#[test]
fn principal_and_home_with_uppercase_and_unprefixed_names() {
    let xml = r#"<D:multistatus xmlns:D="DAV:"><D:response><D:href>/</D:href><D:propstat><D:prop><D:current-user-principal><D:href>/principals/users/alice/</D:href></D:current-user-principal></D:prop><D:status>HTTP/1.1 200 OK</D:status></D:propstat></D:response></D:multistatus>"#;
    assert_eq!(principal(xml).as_deref(), Some("/principals/users/alice/"));
    let xml = r#"<multistatus xmlns="DAV:"><response><href>/p/</href><propstat><prop><C:calendar-home-set xmlns:C="urn:ietf:params:xml:ns:caldav"><href>/cal/home/</href></C:calendar-home-set></prop><status>HTTP/1.1 200 OK</status></propstat></response></multistatus>"#;
    assert_eq!(home(xml).as_deref(), Some("/cal/home/"));
}

#[test]
fn sogo_calendar_home_under_an_arbitrary_prefix() {
    // Real SOGo response from issue #15: CalDAV bound to `a:`, and DAV:
    // declared again on the inner href.
    let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<D:multistatus xmlns:D="DAV:" xmlns:a="urn:ietf:params:xml:ns:caldav"><D:response><D:href>/SOGo/dav/guess.who@echirolles.fr/</D:href><D:propstat><D:status>HTTP/1.1 200 OK</D:status><D:prop><a:calendar-home-set><D:href xmlns:D="DAV:">/SOGo/dav/guess.who@echirolles.fr/Calendar/</D:href></a:calendar-home-set></D:prop></D:propstat></D:response></D:multistatus>"#;
    assert_eq!(
        home(xml).as_deref(),
        Some("/SOGo/dav/guess.who@echirolles.fr/Calendar/")
    );
}

#[test]
fn principal_missing_or_unauthenticated_is_none() {
    let unauthenticated = r#"<d:multistatus xmlns:d="DAV:"><d:response><d:href>/</d:href><d:propstat><d:prop><d:current-user-principal><d:unauthenticated/></d:current-user-principal></d:prop><d:status>HTTP/1.1 200 OK</d:status></d:propstat></d:response></d:multistatus>"#;
    assert_eq!(principal(unauthenticated), None);
    let not_found = r#"<d:multistatus xmlns:d="DAV:"><d:response><d:href>/</d:href><d:propstat><d:prop><d:current-user-principal/></d:prop><d:status>HTTP/1.1 404 Not Found</d:status></d:propstat></d:response></d:multistatus>"#;
    assert_eq!(principal(not_found), None);
    // A calendar-home-set in another namespace is not CalDAV's.
    let wrong_ns = r#"<d:multistatus xmlns:d="DAV:"><d:response><d:href>/p/</d:href><d:propstat><d:prop><x:calendar-home-set xmlns:x="urn:other"><d:href>/nope/</d:href></x:calendar-home-set></d:prop><d:status>HTTP/1.1 200 OK</d:status></d:propstat></d:response></d:multistatus>"#;
    assert_eq!(home(wrong_ns), None);
}

// --- Event REPORTs ---

#[test]
fn events_read_calendar_data_under_any_prefix() {
    let xml = r#"
<d:multistatus>
  <d:response>
    <d:href>/cal/event1.ics</d:href>
    <d:propstat><d:prop>
      <cal:calendar-data>BEGIN:VCALENDAR
BEGIN:VEVENT
UID:ev1
END:VEVENT
END:VCALENDAR</cal:calendar-data>
    </d:prop></d:propstat>
  </d:response>
  <d:response>
    <d:href>/cal/event2.ics</d:href>
    <d:propstat><d:prop>
      <c:calendar-data>BEGIN:VCALENDAR
BEGIN:VEVENT
UID:ev2
END:VEVENT
END:VCALENDAR</c:calendar-data>
    </d:prop></d:propstat>
  </d:response>
</d:multistatus>"#;
    let list = events(xml);
    assert_eq!(list.len(), 2);
    assert_eq!(list[0].href, "/cal/event1.ics");
    assert!(list[0].ical_data.contains("UID:ev1"));
    assert_eq!(list[1].href, "/cal/event2.ics");
    assert!(list[1].ical_data.contains("UID:ev2"));
}

#[test]
fn events_without_data_are_skipped() {
    let xml = r#"<d:multistatus xmlns:d="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav">
  <d:response><d:href>/cal/no-data.ics</d:href><d:propstat><d:prop></d:prop></d:propstat></d:response>
  <d:response><d:href>/cal/gone.ics</d:href><d:status>HTTP/1.1 404 Not Found</d:status></d:response>
  <d:response><d:href>/cal/refused.ics</d:href><d:propstat><d:prop><C:calendar-data/></d:prop><d:status>HTTP/1.1 404 Not Found</d:status></d:propstat></d:response>
</d:multistatus>"#;
    assert!(events(xml).is_empty());
}

#[test]
fn two_responses_under_uppercase_prefixes() {
    let xml = r#"<D:multistatus xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav"><D:response><D:href>/a</D:href><D:propstat><D:prop><C:calendar-data>A</C:calendar-data></D:prop></D:propstat></D:response><D:response><D:href>/b</D:href><D:propstat><D:prop><C:calendar-data>B</C:calendar-data></D:prop></D:propstat></D:response></D:multistatus>"#;
    let list = events(xml);
    assert_eq!(list.len(), 2);
    assert_eq!(
        (list[0].href.as_str(), list[0].ical_data.as_str()),
        ("/a", "A")
    );
    assert_eq!(
        (list[1].href.as_str(), list[1].ical_data.as_str()),
        ("/b", "B")
    );
}

#[test]
fn a_large_report_parses_every_event() {
    // BlueMind calendars with 4000+ events are synced in one REPORT.
    let mut xml = String::from(
        r#"<?xml version="1.0" encoding="UTF-8"?><d:multistatus xmlns:d="DAV:" xmlns:cal="urn:ietf:params:xml:ns:caldav">"#,
    );
    for i in 0..5000 {
        xml.push_str(&format!(
            "<d:response><d:href>/dav/calendars/__uids__/U/calendar:Default:U/{i}.ics</d:href>\
<d:propstat><d:prop><d:getetag>\"{i}\"</d:getetag><cal:calendar-data>BEGIN:VCALENDAR\r\nBEGIN:VEVENT\r\n\
UID:bm-{i}\r\nDTSTART:20261110T090000Z\r\nDTEND:20261110T100000Z\r\nSUMMARY:Meeting {i}\r\n\
END:VEVENT\r\nEND:VCALENDAR\r\n</cal:calendar-data></d:prop><d:status>HTTP/1.1 200 OK</d:status>\
</d:propstat></d:response>"
        ));
    }
    xml.push_str("</d:multistatus>");
    let list = events(&xml);
    assert_eq!(list.len(), 5000);
    assert!(list[4999].href.ends_with("/4999.ics"));
    assert!(list[4999].ical_data.contains("UID:bm-4999\r\n"));
}

// --- sync-collection ---

#[test]
fn sync_changes_and_deletions() {
    let xml = r#"
<d:multistatus xmlns:d="DAV:" xmlns:cal="urn:ietf:params:xml:ns:caldav">
  <d:response>
    <d:href>/cal/event1.ics</d:href>
    <d:propstat>
      <d:prop>
        <d:getetag>"etag1"</d:getetag>
        <cal:calendar-data>BEGIN:VCALENDAR
BEGIN:VEVENT
UID:ev1
SUMMARY:Changed event
END:VEVENT
END:VCALENDAR</cal:calendar-data>
      </d:prop>
      <d:status>HTTP/1.1 200 OK</d:status>
    </d:propstat>
  </d:response>
  <d:response>
    <d:href>/cal/deleted-event.ics</d:href>
    <d:status>HTTP/1.1 404 Not Found</d:status>
  </d:response>
  <d:sync-token>https://example.com/sync/new-token-456</d:sync-token>
</d:multistatus>"#;
    let result = sync(xml);
    assert_eq!(result.changed.len(), 1);
    assert_eq!(result.changed[0].href, "/cal/event1.ics");
    assert!(result.changed[0].ical_data.contains("UID:ev1"));
    assert_eq!(result.deleted_hrefs, ["/cal/deleted-event.ics"]);
    assert_eq!(
        result.new_sync_token.as_deref(),
        Some("https://example.com/sync/new-token-456")
    );
}

#[test]
fn sync_empty_delta() {
    let xml = r#"
<d:multistatus xmlns:d="DAV:">
  <d:sync-token>https://example.com/sync/token-789</d:sync-token>
</d:multistatus>"#;
    let result = sync(xml);
    assert!(result.changed.is_empty());
    assert!(result.deleted_hrefs.is_empty());
    assert_eq!(
        result.new_sync_token.as_deref(),
        Some("https://example.com/sync/token-789")
    );
}

#[test]
fn sync_multiple_deletions_and_410() {
    let xml = r#"
<d:multistatus xmlns:d="DAV:">
  <d:response><d:href>/cal/a.ics</d:href><d:status>HTTP/1.1 404 Not Found</d:status></d:response>
  <d:response><d:href>/cal/b.ics</d:href><d:status>HTTP/1.1 404 Not Found</d:status></d:response>
  <d:response><d:href>/cal/gone.ics</d:href><d:status>HTTP/1.1 410 Gone</d:status></d:response>
  <d:sync-token>token-new</d:sync-token>
</d:multistatus>"#;
    let result = sync(xml);
    assert!(result.changed.is_empty());
    assert_eq!(
        result.deleted_hrefs,
        ["/cal/a.ics", "/cal/b.ics", "/cal/gone.ics"]
    );
}

/// Regression test for issue #107 / 2026-05-14 production incident: a 404
/// inside a propstat is a missing property on a live resource, not a
/// deletion, even when that propstat comes first.
#[test]
fn sync_ignores_property_level_404_in_propstat() {
    let xml = r#"
<d:multistatus xmlns:d="DAV:" xmlns:cal="urn:ietf:params:xml:ns:caldav">
  <d:response>
    <d:href>/cal/live-event.ics</d:href>
    <d:propstat>
      <d:prop><d:getetag/></d:prop>
      <d:status>HTTP/1.1 404 Not Found</d:status>
    </d:propstat>
    <d:propstat>
      <d:prop>
        <d:getetag>"abc-123"</d:getetag>
        <cal:calendar-data>BEGIN:VCALENDAR
BEGIN:VEVENT
UID:still-here
END:VEVENT
END:VCALENDAR</cal:calendar-data>
      </d:prop>
      <d:status>HTTP/1.1 200 OK</d:status>
    </d:propstat>
  </d:response>
  <d:sync-token>https://example.com/sync/t1</d:sync-token>
</d:multistatus>"#;
    let result = sync(xml);
    assert!(
        result.deleted_hrefs.is_empty(),
        "a 404 propstat must NOT be a deletion; got {:?}",
        result.deleted_hrefs
    );
    assert_eq!(result.changed.len(), 1);
    assert_eq!(result.changed[0].href, "/cal/live-event.ics");
    assert!(result.changed[0].ical_data.contains("UID:still-here"));
}

#[test]
fn sync_property_404_under_any_prefix_is_not_a_deletion() {
    // The same shape as above under uppercase, unprefixed and arbitrary
    // prefixes, the cases the string-based propstat stripping had to chase.
    for xml in [
        r#"<D:multistatus xmlns:D="DAV:"><D:response><D:href>/b</D:href><D:propstat><D:prop><D:getetag/></D:prop><D:status>HTTP/1.1 404 Not Found</D:status></D:propstat></D:response></D:multistatus>"#,
        r#"<multistatus xmlns="DAV:"><response><href>/c</href><propstat><prop><getetag/></prop><status>HTTP/1.1 404 Not Found</status></propstat></response></multistatus>"#,
        r#"<my-ns:multistatus xmlns:my-ns="DAV:"><my-ns:response><my-ns:href>/e</my-ns:href><my-ns:propstat><my-ns:prop><my-ns:getetag/></my-ns:prop><my-ns:status>HTTP/1.1 404 Not Found</my-ns:status></my-ns:propstat></my-ns:response></my-ns:multistatus>"#,
        r#"<a:multistatus xmlns:a="DAV:" xmlns:b="DAV:"><a:response><a:href>/f</a:href><b:propstat><a:prop><a:getetag/></a:prop><b:status>HTTP/1.1 404 Not Found</b:status></b:propstat></a:response></a:multistatus>"#,
    ] {
        assert!(sync(xml).deleted_hrefs.is_empty(), "{xml}");
    }
}

#[test]
fn sync_mixed_real_deletion_and_propstat_404() {
    let xml = r#"
<d:multistatus xmlns:d="DAV:" xmlns:cal="urn:ietf:params:xml:ns:caldav">
  <d:response>
    <d:href>/cal/deleted.ics</d:href>
    <d:status>HTTP/1.1 404 Not Found</d:status>
  </d:response>
  <d:response>
    <d:href>/cal/has-propstat-404.ics</d:href>
    <d:propstat>
      <d:prop><d:displayname/></d:prop>
      <d:status>HTTP/1.1 404 Not Found</d:status>
    </d:propstat>
    <d:propstat>
      <d:prop>
        <d:getetag>"e1"</d:getetag>
        <cal:calendar-data>BEGIN:VCALENDAR
BEGIN:VEVENT
UID:withpropstat404
END:VEVENT
END:VCALENDAR</cal:calendar-data>
      </d:prop>
      <d:status>HTTP/1.1 200 OK</d:status>
    </d:propstat>
  </d:response>
  <d:response>
    <d:href>/cal/normal.ics</d:href>
    <d:propstat>
      <d:prop>
        <d:getetag>"e2"</d:getetag>
        <cal:calendar-data>BEGIN:VCALENDAR
BEGIN:VEVENT
UID:normal
END:VEVENT
END:VCALENDAR</cal:calendar-data>
      </d:prop>
      <d:status>HTTP/1.1 200 OK</d:status>
    </d:propstat>
  </d:response>
</d:multistatus>"#;
    let result = sync(xml);
    assert_eq!(result.deleted_hrefs, ["/cal/deleted.ics"]);
    let hrefs: Vec<&str> = result.changed.iter().map(|e| e.href.as_str()).collect();
    assert_eq!(hrefs, ["/cal/has-propstat-404.ics", "/cal/normal.ics"]);
}

#[test]
fn sync_token_is_the_multistatus_one() {
    // A sync-token property inside a response is not the new collection
    // token, wherever it appears.
    let xml = r#"<d:multistatus xmlns:d="DAV:"><d:response><d:href>/x</d:href><d:propstat><d:prop><d:sync-token>inner</d:sync-token></d:prop><d:status>HTTP/1.1 200 OK</d:status></d:propstat></d:response><d:sync-token>outer</d:sync-token></d:multistatus>"#;
    assert_eq!(sync(xml).new_sync_token.as_deref(), Some("outer"));
}

// --- propfind_resource_exists ---

#[test]
fn propfind_resource_level_404_means_gone() {
    let xml = r#"<d:multistatus xmlns:d="DAV:"><d:response><d:href>/cal/x.ics</d:href>
<d:status>HTTP/1.1 404 Not Found</d:status></d:response></d:multistatus>"#;
    assert_eq!(propfind_resource_exists(xml), Some(false));
    let gone = xml.replace("404 Not Found", "410 Gone");
    assert_eq!(propfind_resource_exists(&gone), Some(false));
    let ok = xml.replace("404 Not Found", "200 OK");
    assert_eq!(propfind_resource_exists(&ok), Some(true));
}

#[test]
fn propfind_propstat_means_present_even_with_a_missing_property() {
    let found = r#"<D:multistatus xmlns:D="DAV:"><D:response><D:href>/cal/x.ics</D:href>
<D:propstat><D:prop><D:getetag>"1"</D:getetag></D:prop><D:status>HTTP/1.1 200 OK</D:status></D:propstat>
</D:response></D:multistatus>"#;
    assert_eq!(propfind_resource_exists(found), Some(true));
    // getetag not returned: a property-level 404 on a resource that exists.
    let missing_prop = r#"<multistatus xmlns="DAV:"><response><href>/cal/x.ics</href>
<propstat><prop><getetag/></prop><status>HTTP/1.1 404 Not Found</status></propstat>
</response></multistatus>"#;
    assert_eq!(propfind_resource_exists(missing_prop), Some(true));
}

#[test]
fn propfind_missing_property_under_any_prefix_is_not_gone() {
    // A property 404 under an unusual prefix must not read as the event
    // being gone: the orphan sweep would cancel a live booking on it.
    let inner = "<P:prop><P:getetag/></P:prop><P:status>HTTP/1.1 404 Not Found</P:status>";
    for prefix in ["a", "my-ns", "abcdefghijk", "é", "e\u{301}", "ns.1"] {
        let xml = format!(
            "<{prefix}:multistatus xmlns:{prefix}=\"DAV:\"><{prefix}:response><{prefix}:href>/x</{prefix}:href>\
<{prefix}:propstat>{}</{prefix}:propstat></{prefix}:response></{prefix}:multistatus>",
            inner.replace("P:", &format!("{prefix}:"))
        );
        assert_eq!(propfind_resource_exists(&xml), Some(true), "{prefix}");
    }
}

#[test]
fn propfind_reads_the_resource_status_whatever_the_prefix() {
    // A resource-level 404 under a prefix bound to DAV: is a 404, whatever
    // the prefix is (the string extractor could not read `my-ns:`).
    let xml = r#"<my-ns:multistatus xmlns:my-ns="DAV:"><my-ns:response><my-ns:href>/x.ics</my-ns:href><my-ns:status>HTTP/1.1 404 Not Found</my-ns:status></my-ns:response></my-ns:multistatus>"#;
    assert_eq!(propfind_resource_exists(xml), Some(false));
    // But a status element in another namespace is not WebDAV's.
    let xml = r#"<d:multistatus xmlns:d="DAV:"><d:response><d:href>/x.ics</d:href><x:status xmlns:x="urn:x">HTTP/1.1 404 Not Found</x:status><d:propstat><d:prop><d:getetag>"1"</d:getetag></d:prop><d:status>HTTP/1.1 200 OK</d:status></d:propstat></d:response></d:multistatus>"#;
    assert_eq!(propfind_resource_exists(xml), Some(true));
}

#[test]
fn propfind_reads_valid_but_unusual_markup() {
    let inner = "<d:prop><d:getetag/></d:prop><d:status>HTTP/1.1 404 Not Found</d:status>";
    for xml in [
        // A `/>` inside an attribute value does not make the tag self-closing.
        format!("<d:propstat x=\"/>\">{inner}</d:propstat>"),
        // Whitespace before the `>` of the close tag.
        format!("<d:propstat>{inner}</d:propstat\n>"),
        // A comment that looks like an open tag.
        format!("<!-- <propstat isn't --><d:propstat>{inner}</d:propstat>"),
        // A `propstats-wrapper` is not a propstat, and the real one is read.
        format!(
            "<d:propstats-wrapper>keep me</d:propstats-wrapper><d:propstat>{inner}</d:propstat>"
        ),
    ] {
        let doc = format!(
            "<d:multistatus xmlns:d=\"DAV:\"><d:response><d:href>/x.ics</d:href>{xml}</d:response></d:multistatus>"
        );
        assert_eq!(propfind_resource_exists(&doc), Some(true), "{xml}");
    }
}

#[test]
fn propfind_without_a_usable_status_is_unknown() {
    assert_eq!(propfind_resource_exists(""), None);
    assert_eq!(propfind_resource_exists("<d:multistatus/>"), None);
    let forbidden = r#"<d:multistatus><d:response><d:href>/cal/x.ics</d:href>
<d:status>HTTP/1.1 403 Forbidden</d:status></d:response></d:multistatus>"#;
    assert_eq!(propfind_resource_exists(forbidden), None);
    let unreadable = forbidden.replace("HTTP/1.1 403 Forbidden", "who knows");
    assert_eq!(propfind_resource_exists(&unreadable), None);
    let bare =
        "<d:multistatus><d:response><d:href>/cal/x.ics</d:href></d:response></d:multistatus>";
    assert_eq!(propfind_resource_exists(bare), None);
    // An empty `<propstat/>` carries no status and proves nothing.
    let empty = "<d:multistatus><d:response><d:href>/cal/x.ics</d:href><d:propstat/>\
</d:response></d:multistatus>";
    assert_eq!(propfind_resource_exists(empty), None);
}

#[test]
fn propfind_tolerates_a_less_than_in_an_attribute_value() {
    // Invalid XML, but seen: attribute values are never read, so a `<` in
    // one does not hide the propstat after it (#296).
    let xml = "<d:multistatus xmlns:d=\"DAV:\"><d:response><d:href>/x</d:href><d:propstat a=\"x<y\"><d:prop><d:getetag/></d:prop><d:status>HTTP/1.1 404 Not Found</d:status></d:propstat></d:response></d:multistatus>";
    assert_eq!(propfind_resource_exists(xml), Some(true));
}

// --- Server families ---

/// Nextcloud and Baikal both run sabre/dav: `d:`/`cal:`/`cs:` prefixes,
/// Apple's namespace declared on the colour itself, a 404 propstat for every
/// property a collection lacks, scheduling and trash collections next to
/// the calendars, and CR written as `&#13;` in calendar data.
#[test]
fn nextcloud_sabre_dav_discovery_listing_report_and_delta() {
    let principal_xml = r#"<?xml version="1.0"?>
<d:multistatus xmlns:d="DAV:" xmlns:s="http://sabredav.org/ns" xmlns:cal="urn:ietf:params:xml:ns:caldav" xmlns:cs="http://calendarserver.org/ns/" xmlns:oc="http://owncloud.org/ns" xmlns:nc="http://nextcloud.org/ns">
 <d:response>
  <d:href>/remote.php/dav/</d:href>
  <d:propstat>
   <d:prop>
    <d:current-user-principal>
     <d:href>/remote.php/dav/principals/users/alice/</d:href>
    </d:current-user-principal>
   </d:prop>
   <d:status>HTTP/1.1 200 OK</d:status>
  </d:propstat>
 </d:response>
</d:multistatus>"#;
    assert_eq!(
        principal(principal_xml).as_deref(),
        Some("/remote.php/dav/principals/users/alice/")
    );

    let home_xml = r#"<?xml version="1.0"?>
<d:multistatus xmlns:d="DAV:" xmlns:s="http://sabredav.org/ns" xmlns:cal="urn:ietf:params:xml:ns:caldav" xmlns:cs="http://calendarserver.org/ns/" xmlns:oc="http://owncloud.org/ns" xmlns:nc="http://nextcloud.org/ns">
 <d:response>
  <d:href>/remote.php/dav/principals/users/alice/</d:href>
  <d:propstat>
   <d:prop>
    <cal:calendar-home-set>
     <d:href>/remote.php/dav/calendars/alice/</d:href>
    </cal:calendar-home-set>
   </d:prop>
   <d:status>HTTP/1.1 200 OK</d:status>
  </d:propstat>
 </d:response>
</d:multistatus>"#;
    assert_eq!(
        home(home_xml).as_deref(),
        Some("/remote.php/dav/calendars/alice/")
    );

    let listing = r#"<?xml version="1.0"?>
<d:multistatus xmlns:d="DAV:" xmlns:s="http://sabredav.org/ns" xmlns:cal="urn:ietf:params:xml:ns:caldav" xmlns:cs="http://calendarserver.org/ns/" xmlns:oc="http://owncloud.org/ns" xmlns:nc="http://nextcloud.org/ns">
 <d:response>
  <d:href>/remote.php/dav/calendars/alice/</d:href>
  <d:propstat>
   <d:prop><d:resourcetype><d:collection/></d:resourcetype></d:prop>
   <d:status>HTTP/1.1 200 OK</d:status>
  </d:propstat>
  <d:propstat>
   <d:prop><d:displayname/><x1:calendar-color xmlns:x1="http://apple.com/ns/ical/"/><cs:getctag/><d:sync-token/></d:prop>
   <d:status>HTTP/1.1 404 Not Found</d:status>
  </d:propstat>
 </d:response>
 <d:response>
  <d:href>/remote.php/dav/calendars/alice/personal/</d:href>
  <d:propstat>
   <d:prop>
    <d:resourcetype><d:collection/><cal:calendar/></d:resourcetype>
    <d:displayname>Personal</d:displayname>
    <x1:calendar-color xmlns:x1="http://apple.com/ns/ical/">#0082c9</x1:calendar-color>
    <cs:getctag>http://sabre.io/ns/sync/42</cs:getctag>
    <d:sync-token>http://sabre.io/ns/sync/42</d:sync-token>
   </d:prop>
   <d:status>HTTP/1.1 200 OK</d:status>
  </d:propstat>
 </d:response>
 <d:response>
  <d:href>/remote.php/dav/calendars/alice/team%20%26%20projects/</d:href>
  <d:propstat>
   <d:prop>
    <d:resourcetype><d:collection/><cal:calendar/><cs:shared-owner/></d:resourcetype>
    <d:displayname>Team &amp; projects</d:displayname>
    <cs:getctag>http://sabre.io/ns/sync/7</cs:getctag>
    <d:sync-token>http://sabre.io/ns/sync/7</d:sync-token>
   </d:prop>
   <d:status>HTTP/1.1 200 OK</d:status>
  </d:propstat>
  <d:propstat>
   <d:prop><x1:calendar-color xmlns:x1="http://apple.com/ns/ical/"/></d:prop>
   <d:status>HTTP/1.1 404 Not Found</d:status>
  </d:propstat>
 </d:response>
 <d:response>
  <d:href>/remote.php/dav/calendars/alice/inbox/</d:href>
  <d:propstat>
   <d:prop><d:resourcetype><d:collection/><cal:schedule-inbox/></d:resourcetype></d:prop>
   <d:status>HTTP/1.1 200 OK</d:status>
  </d:propstat>
 </d:response>
 <d:response>
  <d:href>/remote.php/dav/calendars/alice/outbox/</d:href>
  <d:propstat>
   <d:prop><d:resourcetype><d:collection/><cal:schedule-outbox/></d:resourcetype></d:prop>
   <d:status>HTTP/1.1 200 OK</d:status>
  </d:propstat>
 </d:response>
 <d:response>
  <d:href>/remote.php/dav/calendars/alice/trashbin/</d:href>
  <d:propstat>
   <d:prop><d:resourcetype><d:collection/><nc:trash-bin/></d:resourcetype></d:prop>
   <d:status>HTTP/1.1 200 OK</d:status>
  </d:propstat>
 </d:response>
</d:multistatus>"#;
    let list = cals(listing);
    assert_eq!(list.len(), 2);
    assert_eq!(list[0].href, "/remote.php/dav/calendars/alice/personal/");
    assert_eq!(list[0].display_name.as_deref(), Some("Personal"));
    assert_eq!(list[0].color.as_deref(), Some("#0082c9"));
    assert_eq!(list[0].ctag.as_deref(), Some("http://sabre.io/ns/sync/42"));
    assert_eq!(
        list[0].sync_token.as_deref(),
        Some("http://sabre.io/ns/sync/42")
    );
    assert_eq!(
        list[1].href,
        "/remote.php/dav/calendars/alice/team%20%26%20projects/"
    );
    assert_eq!(list[1].display_name.as_deref(), Some("Team & projects"));
    assert_eq!(list[1].color, None);

    let report = r#"<?xml version="1.0"?>
<d:multistatus xmlns:d="DAV:" xmlns:s="http://sabredav.org/ns" xmlns:cal="urn:ietf:params:xml:ns:caldav" xmlns:cs="http://calendarserver.org/ns/" xmlns:oc="http://owncloud.org/ns" xmlns:nc="http://nextcloud.org/ns">
 <d:response>
  <d:href>/remote.php/dav/calendars/alice/personal/0A1B2C3D-standup.ics</d:href>
  <d:propstat>
   <d:prop>
    <d:getetag>&quot;8c5f0e4b1a2d3c4e5f60718293a4b5c6&quot;</d:getetag>
    <cal:calendar-data>BEGIN:VCALENDAR&#13;
VERSION:2.0&#13;
PRODID:-//Sabre//Sabre VObject 4.5.4//EN&#13;
BEGIN:VEVENT&#13;
UID:0A1B2C3D-standup&#13;
DTSTART;TZID=Europe/Paris:20261110T093000&#13;
DTEND;TZID=Europe/Paris:20261110T094500&#13;
SUMMARY:Stand-up &amp; planning &lt;daily&gt;&#13;
END:VEVENT&#13;
END:VCALENDAR&#13;
</cal:calendar-data>
   </d:prop>
   <d:status>HTTP/1.1 200 OK</d:status>
  </d:propstat>
 </d:response>
</d:multistatus>"#;
    let list = events(report);
    assert_eq!(list.len(), 1);
    assert_eq!(
        list[0].href,
        "/remote.php/dav/calendars/alice/personal/0A1B2C3D-standup.ics"
    );
    assert_eq!(
        list[0].ical_data,
        ics(&[
            "BEGIN:VCALENDAR",
            "VERSION:2.0",
            "PRODID:-//Sabre//Sabre VObject 4.5.4//EN",
            "BEGIN:VEVENT",
            "UID:0A1B2C3D-standup",
            "DTSTART;TZID=Europe/Paris:20261110T093000",
            "DTEND;TZID=Europe/Paris:20261110T094500",
            "SUMMARY:Stand-up & planning <daily>",
            "END:VEVENT",
            "END:VCALENDAR",
        ])
    );

    let delta = r#"<?xml version="1.0"?>
<d:multistatus xmlns:d="DAV:" xmlns:s="http://sabredav.org/ns" xmlns:cal="urn:ietf:params:xml:ns:caldav" xmlns:cs="http://calendarserver.org/ns/" xmlns:oc="http://owncloud.org/ns" xmlns:nc="http://nextcloud.org/ns">
 <d:response>
  <d:href>/remote.php/dav/calendars/alice/personal/new-event.ics</d:href>
  <d:propstat>
   <d:prop>
    <d:getetag>&quot;e1&quot;</d:getetag>
    <cal:calendar-data>BEGIN:VCALENDAR&#13;
BEGIN:VEVENT&#13;
UID:new-event&#13;
END:VEVENT&#13;
END:VCALENDAR&#13;
</cal:calendar-data>
   </d:prop>
   <d:status>HTTP/1.1 200 OK</d:status>
  </d:propstat>
 </d:response>
 <d:response>
  <d:href>/remote.php/dav/calendars/alice/personal/removed.ics</d:href>
  <d:status>HTTP/1.1 404 Not Found</d:status>
 </d:response>
 <d:sync-token>http://sabre.io/ns/sync/43</d:sync-token>
</d:multistatus>"#;
    let result = sync(delta);
    assert_eq!(result.changed.len(), 1);
    assert_eq!(
        result.changed[0].ical_data,
        ics(&[
            "BEGIN:VCALENDAR",
            "BEGIN:VEVENT",
            "UID:new-event",
            "END:VEVENT",
            "END:VCALENDAR"
        ])
    );
    assert_eq!(
        result.deleted_hrefs,
        ["/remote.php/dav/calendars/alice/personal/removed.ics"]
    );
    assert_eq!(
        result.new_sync_token.as_deref(),
        Some("http://sabre.io/ns/sync/43")
    );
}

#[test]
fn baikal_listing_skips_addressbooks_and_scheduling_collections() {
    let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<d:multistatus xmlns:d="DAV:" xmlns:s="http://sabredav.org/ns" xmlns:cal="urn:ietf:params:xml:ns:caldav" xmlns:cs="http://calendarserver.org/ns/" xmlns:card="urn:ietf:params:xml:ns:carddav"><d:response><d:href>/dav.php/calendars/alice/</d:href><d:propstat><d:prop><d:resourcetype><d:collection/></d:resourcetype></d:prop><d:status>HTTP/1.1 200 OK</d:status></d:propstat></d:response><d:response><d:href>/dav.php/calendars/alice/default/</d:href><d:propstat><d:prop><d:resourcetype><d:collection/><cal:calendar/></d:resourcetype><d:displayname>Default calendar</d:displayname><x1:calendar-color xmlns:x1="http://apple.com/ns/ical/">#8E44ADFF</x1:calendar-color><cs:getctag>http://sabre.io/ns/sync/12</cs:getctag><d:sync-token>http://sabre.io/ns/sync/12</d:sync-token></d:prop><d:status>HTTP/1.1 200 OK</d:status></d:propstat></d:response><d:response><d:href>/dav.php/calendars/alice/inbox/</d:href><d:propstat><d:prop><d:resourcetype><d:collection/><cal:schedule-inbox/></d:resourcetype></d:prop><d:status>HTTP/1.1 200 OK</d:status></d:propstat><d:propstat><d:prop><d:displayname/><cs:getctag/></d:prop><d:status>HTTP/1.1 404 Not Found</d:status></d:propstat></d:response><d:response><d:href>/dav.php/addressbooks/alice/default/</d:href><d:propstat><d:prop><d:resourcetype><d:collection/><card:addressbook/></d:resourcetype><d:displayname>Contacts</d:displayname></d:prop><d:status>HTTP/1.1 200 OK</d:status></d:propstat></d:response></d:multistatus>"#;
    let list = cals(xml);
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].href, "/dav.php/calendars/alice/default/");
    assert_eq!(list[0].display_name.as_deref(), Some("Default calendar"));
    assert_eq!(list[0].color.as_deref(), Some("#8E44ADFF"));
    assert_eq!(list[0].ctag.as_deref(), Some("http://sabre.io/ns/sync/12"));
}

/// iCloud answers in the default DAV namespace and declares every other
/// namespace on the property itself; its calendar home is an absolute URL
/// on another host and port.
#[test]
fn icloud_default_namespace_and_absolute_home() {
    let principal_xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<multistatus xmlns="DAV:"><response><href>/</href><propstat><prop><current-user-principal><href>/123456789/principal/</href></current-user-principal></prop><status>HTTP/1.1 200 OK</status></propstat></response></multistatus>"#;
    assert_eq!(
        principal(principal_xml).as_deref(),
        Some("/123456789/principal/")
    );

    let home_xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<multistatus xmlns="DAV:"><response><href>/123456789/principal/</href><propstat><prop><calendar-home-set xmlns="urn:ietf:params:xml:ns:caldav"><href xmlns="DAV:">https://p42-caldav.icloud.com:443/123456789/calendars/</href></calendar-home-set></prop><status>HTTP/1.1 200 OK</status></propstat></response></multistatus>"#;
    assert_eq!(
        home(home_xml).as_deref(),
        Some("https://p42-caldav.icloud.com:443/123456789/calendars/")
    );

    let listing = r#"<?xml version="1.0" encoding="UTF-8"?>
<multistatus xmlns="DAV:"><response><href>/123456789/calendars/</href><propstat><prop><resourcetype><collection/></resourcetype></prop><status>HTTP/1.1 200 OK</status></propstat><propstat><prop><displayname/><calendar-color xmlns="http://apple.com/ns/ical/"/><getctag xmlns="http://calendarserver.org/ns/"/><sync-token/></prop><status>HTTP/1.1 404 Not Found</status></propstat></response><response><href>/123456789/calendars/home/</href><propstat><prop><resourcetype><collection/><calendar xmlns="urn:ietf:params:xml:ns:caldav"/></resourcetype><displayname>Home</displayname><calendar-color xmlns="http://apple.com/ns/ical/" symbolic-color="custom">#1BADF8FF</calendar-color><getctag xmlns="http://calendarserver.org/ns/">HwoQEgwAAKs8xBbNAAAAABgCGAAiFQjVlrjAkbmZ+wEQ4b2dt8/q9Y8BKAA=</getctag><sync-token>https://p42-caldav.icloud.com/mm/1/HwoQEgwAAKs8xBbN</sync-token></prop><status>HTTP/1.1 200 OK</status></propstat></response><response><href>/123456789/calendars/inbox/</href><propstat><prop><resourcetype><collection/><schedule-inbox xmlns="urn:ietf:params:xml:ns:caldav"/></resourcetype></prop><status>HTTP/1.1 200 OK</status></propstat></response><response><href>/123456789/calendars/notification/</href><propstat><prop><resourcetype><collection/><notification xmlns="http://calendarserver.org/ns/"/></resourcetype></prop><status>HTTP/1.1 200 OK</status></propstat></response></multistatus>"#;
    let list = cals(listing);
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].href, "/123456789/calendars/home/");
    assert_eq!(list[0].display_name.as_deref(), Some("Home"));
    assert_eq!(list[0].color.as_deref(), Some("#1BADF8FF"));
    assert_eq!(
        list[0].ctag.as_deref(),
        Some("HwoQEgwAAKs8xBbNAAAAABgCGAAiFQjVlrjAkbmZ+wEQ4b2dt8/q9Y8BKAA=")
    );
    assert_eq!(
        list[0].sync_token.as_deref(),
        Some("https://p42-caldav.icloud.com/mm/1/HwoQEgwAAKs8xBbN")
    );

    let delta = r#"<?xml version="1.0" encoding="UTF-8"?>
<multistatus xmlns="DAV:"><response><href>/123456789/calendars/home/A1B2.ics</href><propstat><prop><getetag>"lr2xg5p1"</getetag><calendar-data xmlns="urn:ietf:params:xml:ns:caldav">BEGIN:VCALENDAR
VERSION:2.0
PRODID:-//Apple Inc.//iPhone OS 18.0//EN
BEGIN:VEVENT
UID:A1B2
DTSTART:20261112T140000Z
DTEND:20261112T150000Z
END:VEVENT
END:VCALENDAR
</calendar-data></prop><status>HTTP/1.1 200 OK</status></propstat></response><response><href>/123456789/calendars/home/C3D4.ics</href><status>HTTP/1.1 404 Not Found</status></response><sync-token>https://p42-caldav.icloud.com/mm/1/HwoQEgwAAKs9</sync-token></multistatus>"#;
    let result = sync(delta);
    assert_eq!(result.changed.len(), 1);
    assert!(result.changed[0]
        .ical_data
        .contains("UID:A1B2\r\nDTSTART:20261112T140000Z\r\n"));
    assert_eq!(result.deleted_hrefs, ["/123456789/calendars/home/C3D4.ics"]);
    assert_eq!(
        result.new_sync_token.as_deref(),
        Some("https://p42-caldav.icloud.com/mm/1/HwoQEgwAAKs9")
    );
}

/// Google CalDAV: `D:` and `caldav:` prefixes, percent-encoded addresses in
/// every href (`%40`, `%23`).
#[test]
fn google_percent_encoded_hrefs() {
    let home_xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<D:multistatus xmlns:D="DAV:" xmlns:caldav="urn:ietf:params:xml:ns:caldav" xmlns:cs="http://calendarserver.org/ns/" xmlns:ical="http://apple.com/ns/ical/"><D:response><D:href>/caldav/v2/alice%40gmail.com/user/</D:href><D:propstat><D:status>HTTP/1.1 200 OK</D:status><D:prop><caldav:calendar-home-set><D:href>/caldav/v2/alice%40gmail.com/</D:href></caldav:calendar-home-set></D:prop></D:propstat></D:response></D:multistatus>"#;
    assert_eq!(
        home(home_xml).as_deref(),
        Some("/caldav/v2/alice%40gmail.com/")
    );

    let listing = r#"<?xml version="1.0" encoding="UTF-8"?>
<D:multistatus xmlns:D="DAV:" xmlns:caldav="urn:ietf:params:xml:ns:caldav" xmlns:cs="http://calendarserver.org/ns/" xmlns:ical="http://apple.com/ns/ical/"><D:response><D:href>/caldav/v2/alice%40gmail.com/user/</D:href><D:propstat><D:status>HTTP/1.1 200 OK</D:status><D:prop><D:resourcetype><D:principal/></D:resourcetype><D:displayname>Alice</D:displayname></D:prop></D:propstat></D:response><D:response><D:href>/caldav/v2/alice%40gmail.com/events/</D:href><D:propstat><D:status>HTTP/1.1 200 OK</D:status><D:prop><D:resourcetype><D:collection/><caldav:calendar/></D:resourcetype><D:displayname>alice@gmail.com</D:displayname><ical:calendar-color>#9FC6E7</ical:calendar-color><cs:getctag>63901234567</cs:getctag><D:sync-token>https://www.google.com/calendar/dav/alice%40gmail.com/events/sync/CPjLr9mUvokDEPjLr9mUvokDGAU=</D:sync-token></D:prop></D:propstat></D:response><D:response><D:href>/caldav/v2/en.french%23holiday%40group.v.calendar.google.com/events/</D:href><D:propstat><D:status>HTTP/1.1 200 OK</D:status><D:prop><D:resourcetype><D:collection/><caldav:calendar/></D:resourcetype><D:displayname>Holidays in France</D:displayname><ical:calendar-color>#16A765</ical:calendar-color><cs:getctag>63900000000</cs:getctag></D:prop></D:propstat></D:response></D:multistatus>"#;
    let list = cals(listing);
    assert_eq!(list.len(), 2);
    assert_eq!(list[0].href, "/caldav/v2/alice%40gmail.com/events/");
    assert_eq!(list[0].display_name.as_deref(), Some("alice@gmail.com"));
    assert_eq!(list[0].color.as_deref(), Some("#9FC6E7"));
    assert_eq!(list[0].ctag.as_deref(), Some("63901234567"));
    assert_eq!(
        list[1].href,
        "/caldav/v2/en.french%23holiday%40group.v.calendar.google.com/events/"
    );
    assert_eq!(list[1].sync_token, None);

    let report = r#"<?xml version="1.0" encoding="UTF-8"?>
<D:multistatus xmlns:D="DAV:" xmlns:caldav="urn:ietf:params:xml:ns:caldav"><D:response><D:href>/caldav/v2/alice%40gmail.com/events/abc123def456%40google.com.ics</D:href><D:propstat><D:status>HTTP/1.1 200 OK</D:status><D:prop><D:getetag>"63901234567000"</D:getetag><caldav:calendar-data>BEGIN:VCALENDAR
PRODID:-//Google Inc//Google Calendar 70.9054//EN
VERSION:2.0
BEGIN:VEVENT
DTSTART:20261113T100000Z
DTEND:20261113T110000Z
UID:abc123def456@google.com
SUMMARY:Lunch
END:VEVENT
END:VCALENDAR
</caldav:calendar-data></D:prop></D:propstat></D:response></D:multistatus>"#;
    let list = events(report);
    assert_eq!(list.len(), 1);
    assert_eq!(
        list[0].href,
        "/caldav/v2/alice%40gmail.com/events/abc123def456%40google.com.ics"
    );
    assert!(list[0]
        .ical_data
        .contains("UID:abc123def456@google.com\r\nSUMMARY:Lunch\r\n"));
}

/// Fastmail (Cyrus IMAP) wraps display names, colours and calendar data in
/// CDATA.
#[test]
fn fastmail_cyrus_cdata_everywhere() {
    let listing = r#"<?xml version="1.0" encoding="utf-8"?>
<D:multistatus xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav" xmlns:CS="http://calendarserver.org/ns/" xmlns:A="http://apple.com/ns/ical/"><D:response><D:href>/dav/calendars/user/alice@fastmail.com/</D:href><D:propstat><D:prop><D:resourcetype><D:collection/></D:resourcetype></D:prop><D:status>HTTP/1.1 200 OK</D:status></D:propstat><D:propstat><D:prop><D:displayname/><A:calendar-color/><CS:getctag/></D:prop><D:status>HTTP/1.1 404 Not Found</D:status></D:propstat></D:response><D:response><D:href>/dav/calendars/user/alice@fastmail.com/Default/</D:href><D:propstat><D:prop><D:resourcetype><D:collection/><C:calendar/></D:resourcetype><D:displayname><![CDATA[Personal]]></D:displayname><A:calendar-color><![CDATA[#3a429c]]></A:calendar-color><CS:getctag>1730000000-12</CS:getctag><D:sync-token>http://cyrusimap.org/ns/sync/12</D:sync-token></D:prop><D:status>HTTP/1.1 200 OK</D:status></D:propstat></D:response><D:response><D:href>/dav/calendars/user/alice@fastmail.com/6b1c3f2e-9d1a-4c3e-8f7b-2a5d9e0c1b4a/</D:href><D:propstat><D:prop><D:resourcetype><D:collection/><C:calendar/></D:resourcetype><D:displayname><![CDATA[R&D <team>]]></D:displayname><CS:getctag>1730000000-3</CS:getctag></D:prop><D:status>HTTP/1.1 200 OK</D:status></D:propstat><D:propstat><D:prop><A:calendar-color/></D:prop><D:status>HTTP/1.1 404 Not Found</D:status></D:propstat></D:response></D:multistatus>"#;
    let list = cals(listing);
    assert_eq!(list.len(), 2);
    assert_eq!(list[0].display_name.as_deref(), Some("Personal"));
    assert_eq!(list[0].color.as_deref(), Some("#3a429c"));
    assert_eq!(
        list[0].sync_token.as_deref(),
        Some("http://cyrusimap.org/ns/sync/12")
    );
    assert_eq!(list[1].display_name.as_deref(), Some("R&D <team>"));
    assert_eq!(list[1].color, None);

    let report = format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
<D:multistatus xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav"><D:response><D:href>/dav/calendars/user/alice@fastmail.com/Default/f00d.ics</D:href><D:propstat><D:prop><D:getetag>"9a8b7c"</D:getetag><C:calendar-data><![CDATA[{}]]></C:calendar-data></D:prop><D:status>HTTP/1.1 200 OK</D:status></D:propstat></D:response></D:multistatus>"#,
        "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nBEGIN:VEVENT\r\nUID:f00d\r\nSUMMARY:Q&A <b>\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n"
    );
    let list = events(&report);
    assert_eq!(
        list[0].ical_data,
        ics(&[
            "BEGIN:VCALENDAR",
            "VERSION:2.0",
            "BEGIN:VEVENT",
            "UID:f00d",
            "SUMMARY:Q&A <b>",
            "END:VEVENT",
            "END:VCALENDAR"
        ])
    );
}

/// Zimbra declares each namespace on the element that uses it, and reuses
/// the prefix `C:` for CalDAV and CardDAV in different responses.
#[test]
fn zimbra_inline_namespaces_and_a_reused_prefix() {
    let principal_xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<D:multistatus xmlns:D="DAV:"><D:response><D:href>/dav/</D:href><D:propstat><D:prop><D:current-user-principal><D:href>/principals/users/alice@example.com/</D:href></D:current-user-principal></D:prop><D:status>HTTP/1.1 200 OK</D:status></D:propstat></D:response></D:multistatus>"#;
    assert_eq!(
        principal(principal_xml).as_deref(),
        Some("/principals/users/alice@example.com/")
    );

    let listing = r#"<?xml version="1.0" encoding="UTF-8"?>
<D:multistatus xmlns:D="DAV:"><D:response><D:href>/dav/alice@example.com/Calendar/</D:href><D:propstat><D:prop><D:resourcetype><C:calendar xmlns:C="urn:ietf:params:xml:ns:caldav"/><D:collection/></D:resourcetype><D:displayname>Calendar</D:displayname><A:calendar-color xmlns:A="http://apple.com/ns/ical/">#0252D4FF</A:calendar-color><CS:getctag xmlns:CS="http://calendarserver.org/ns/">2567</CS:getctag></D:prop><D:status>HTTP/1.1 200 OK</D:status></D:propstat></D:response><D:response><D:href>/dav/alice@example.com/Contacts/</D:href><D:propstat><D:prop><D:resourcetype><C:addressbook xmlns:C="urn:ietf:params:xml:ns:carddav"/><D:collection/></D:resourcetype><D:displayname>Contacts</D:displayname></D:prop><D:status>HTTP/1.1 200 OK</D:status></D:propstat></D:response><D:response><D:href>/dav/alice@example.com/Inbox/</D:href><D:propstat><D:prop><D:resourcetype><D:collection/></D:resourcetype><D:displayname>Inbox</D:displayname></D:prop><D:status>HTTP/1.1 200 OK</D:status></D:propstat></D:response></D:multistatus>"#;
    let list = cals(listing);
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].href, "/dav/alice@example.com/Calendar/");
    assert_eq!(list[0].color.as_deref(), Some("#0252D4FF"));
    assert_eq!(list[0].ctag.as_deref(), Some("2567"));
}

/// BlueMind binds Apple's namespace to `x1:` and Calendar Server's to `cs:`,
/// and its hrefs carry colons.
#[test]
fn bluemind_x1_and_cs_prefixes() {
    let declared = r#"<?xml version="1.0" encoding="UTF-8"?>
<d:multistatus xmlns:d="DAV:" xmlns:cal="urn:ietf:params:xml:ns:caldav" xmlns:cs="http://calendarserver.org/ns/" xmlns:x1="http://apple.com/ns/ical/"><d:response><d:href>/dav/calendars/__uids__/6A1F4C2B/calendar:Default:6A1F4C2B/</d:href><d:propstat><d:prop><d:resourcetype><d:collection/><cal:calendar/></d:resourcetype><d:displayname>BlueMind Cal</d:displayname><x1:calendar-color>#00FF00</x1:calendar-color><cs:getctag>ctag-bm</cs:getctag></d:prop><d:status>HTTP/1.1 200 OK</d:status></d:propstat></d:response></d:multistatus>"#;
    // The same listing with the prefixes left undeclared, as the old test
    // fixture had it.
    let undeclared = r#"
<d:multistatus>
  <d:response>
    <d:href>/dav/calendars/__uids__/6A1F4C2B/calendar:Default:6A1F4C2B/</d:href>
    <d:propstat><d:prop>
      <d:resourcetype><d:collection/><cal:calendar/></d:resourcetype>
      <d:displayname>BlueMind Cal</d:displayname>
      <x1:calendar-color>#00FF00</x1:calendar-color>
      <cs:getctag>ctag-bm</cs:getctag>
    </d:prop></d:propstat>
  </d:response>
</d:multistatus>"#;
    for xml in [declared, undeclared] {
        let list = cals(xml);
        assert_eq!(list.len(), 1);
        assert_eq!(
            list[0].href,
            "/dav/calendars/__uids__/6A1F4C2B/calendar:Default:6A1F4C2B/"
        );
        assert_eq!(list[0].color.as_deref(), Some("#00FF00"));
        assert_eq!(list[0].ctag.as_deref(), Some("ctag-bm"));
    }
}

#[test]
fn sogo_uppercase_prefixes() {
    let xml = r#"
<D:multistatus xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav">
  <D:response>
    <D:href>/SOGo/dav/user/Calendar/personal/</D:href>
    <D:propstat><D:prop>
      <D:resourcetype><D:collection/><C:calendar/></D:resourcetype>
      <D:displayname>Personal Calendar</D:displayname>
    </D:prop></D:propstat>
  </D:response>
  <D:response>
    <D:href>/SOGo/dav/user/Calendar/</D:href>
    <D:propstat><D:prop>
      <D:resourcetype><D:collection/></D:resourcetype>
    </D:prop></D:propstat>
  </D:response>
</D:multistatus>"#;
    let list = cals(xml);
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].href, "/SOGo/dav/user/Calendar/personal/");
    assert_eq!(list[0].display_name.as_deref(), Some("Personal Calendar"));
}

#[test]
fn sogo_unprefixed_calendar_with_its_own_xmlns() {
    // SOGo writes <calendar xmlns="urn:ietf:params:xml:ns:caldav"/>, next
    // to GroupDAV collection types that must not count.
    let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<D:multistatus xmlns:D="DAV:" xmlns:b="http://calendarserver.org/ns/" xmlns:a="http://apple.com/ns/ical/"><D:response><D:href>/SOGo/dav/user@example.com/Calendar/</D:href><D:propstat><D:status>HTTP/1.1 200 OK</D:status><D:prop><D:resourcetype><D:collection/></D:resourcetype><D:displayname>Calendar</D:displayname></D:prop></D:propstat></D:response><D:response><D:href>/SOGo/dav/user@example.com/Calendar/personal/</D:href><D:propstat><D:status>HTTP/1.1 200 OK</D:status><D:prop><D:resourcetype><D:collection/><calendar xmlns="urn:ietf:params:xml:ns:caldav"/><vevent-collection xmlns="http://groupdav.org/"/><vtodo-collection xmlns="http://groupdav.org/"/><schedule-outbox xmlns="urn:ietf:params:xml:ns:caldav"/></D:resourcetype><D:displayname>Personal Calendar</D:displayname><a:calendar-color>#AAAAAAFF</a:calendar-color><b:getctag>-1</b:getctag><D:sync-token>-1</D:sync-token></D:prop></D:propstat></D:response></D:multistatus>"#;
    let list = cals(xml);
    assert_eq!(list.len(), 1);
    assert_eq!(
        list[0].href,
        "/SOGo/dav/user@example.com/Calendar/personal/"
    );
    assert_eq!(list[0].display_name.as_deref(), Some("Personal Calendar"));
    assert_eq!(list[0].color.as_deref(), Some("#AAAAAAFF"));
    assert_eq!(list[0].ctag.as_deref(), Some("-1"));
    assert_eq!(list[0].sync_token.as_deref(), Some("-1"));
}

#[test]
fn radicale_default_namespace_principal_and_addressbook() {
    // Radicale 3.7.3 (issue #132): default DAV namespace, `C:` for CalDAV,
    // a space before the self-closing slash, a CardDAV addressbook and the
    // principal collection itself, neither of which is a calendar.
    let xml = r#"<?xml version='1.0' encoding='utf-8'?>
<multistatus xmlns="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav" xmlns:CR="urn:ietf:params:xml:ns:carddav" xmlns:CS="http://calendarserver.org/ns/" xmlns:ICAL="http://apple.com/ns/ical/"><response><href>/greg/</href><propstat><prop><resourcetype><principal /><collection /></resourcetype></prop><status>HTTP/1.1 200 OK</status></propstat><propstat><prop><displayname /><ICAL:calendar-color /><CS:getctag /><sync-token /></prop><status>HTTP/1.1 404 Not Found</status></propstat></response><response><href>/greg/0ff70afa/</href><propstat><prop><resourcetype><C:calendar /><collection /></resourcetype><displayname>Calendrier Pro</displayname><ICAL:calendar-color>#f79f78ff</ICAL:calendar-color><CS:getctag>"ctag-pro"</CS:getctag><sync-token>http://radicale.org/ns/sync/pro</sync-token></prop><status>HTTP/1.1 200 OK</status></propstat></response><response><href>/greg/7d2395d4/</href><propstat><prop><resourcetype><CR:addressbook /><collection /></resourcetype><displayname>Contacts Pros</displayname><CS:getctag>"ctag-contacts"</CS:getctag><sync-token>http://radicale.org/ns/sync/contacts</sync-token></prop><status>HTTP/1.1 200 OK</status></propstat><propstat><prop><ICAL:calendar-color /></prop><status>HTTP/1.1 404 Not Found</status></propstat></response><response><href>/greg/a56013c9/</href><propstat><prop><resourcetype><C:calendar /><collection /></resourcetype><displayname>Calendrier Perso</displayname><ICAL:calendar-color>#74bda7ff</ICAL:calendar-color><CS:getctag>"ctag-perso"</CS:getctag><sync-token>http://radicale.org/ns/sync/perso</sync-token></prop><status>HTTP/1.1 200 OK</status></propstat></response></multistatus>"#;
    let list = cals(xml);
    assert_eq!(list.len(), 2, "two calendars, no principal or addressbook");
    assert_eq!(list[0].href, "/greg/0ff70afa/");
    assert_eq!(list[0].display_name.as_deref(), Some("Calendrier Pro"));
    assert_eq!(list[0].color.as_deref(), Some("#f79f78ff"));
    assert_eq!(list[0].ctag.as_deref(), Some("\"ctag-pro\""));
    assert_eq!(
        list[0].sync_token.as_deref(),
        Some("http://radicale.org/ns/sync/pro")
    );
    assert_eq!(list[1].href, "/greg/a56013c9/");
    assert_eq!(list[1].display_name.as_deref(), Some("Calendrier Perso"));
    assert_eq!(list[1].color.as_deref(), Some("#74bda7ff"));
}

#[test]
fn radicale_sync_delta_with_token_first() {
    let xml = "<?xml version='1.0' encoding='utf-8'?>\n<multistatus xmlns=\"DAV:\" xmlns:C=\"urn:ietf:params:xml:ns:caldav\"><sync-token>http://radicale.org/ns/sync/abc</sync-token><response><href>/greg/0ff70afa/new.ics</href><propstat><prop><getetag>\"e2\"</getetag><C:calendar-data>BEGIN:VCALENDAR\r\nBEGIN:VEVENT\r\nUID:new\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n</C:calendar-data></prop><status>HTTP/1.1 200 OK</status></propstat></response><response><href>/greg/0ff70afa/old.ics</href><status>HTTP/1.1 404 Not Found</status></response></multistatus>";
    let result = sync(xml);
    assert_eq!(
        result.new_sync_token.as_deref(),
        Some("http://radicale.org/ns/sync/abc")
    );
    assert_eq!(result.changed[0].href, "/greg/0ff70afa/new.ics");
    assert_eq!(result.deleted_hrefs, ["/greg/0ff70afa/old.ics"]);
}

#[test]
fn kerio_namespace_on_the_response_tag() {
    // Kerio Connect (#312) declares the CalDAV namespace on the <response>
    // tag of calendar entries only; plain mail folders get a bare one.
    let xml = r#"<a:multistatus xmlns:a="DAV:"><a:response><a:href>/u/MAIL/</a:href><a:propstat><a:prop><a:resourcetype><a:collection/></a:resourcetype><a:displayname>NonCalendarDir</a:displayname></a:prop></a:propstat></a:response><a:response xmlns:g="urn:ietf:params:xml:ns:caldav"><a:href>/u/CAL/</a:href><a:propstat><a:prop><a:resourcetype><a:collection/><g:calendar/></a:resourcetype><a:displayname>Calendar</a:displayname></a:prop></a:propstat></a:response></a:multistatus>"#;
    let list = cals(xml);
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].href, "/u/CAL/");
    assert_eq!(list[0].display_name.as_deref(), Some("Calendar"));
}

#[test]
fn mixed_prefixes_bound_to_dav_with_attributes() {
    // Both prefixes are bound to DAV:. The first response carries an
    // attribute; each response keeps its own href and properties.
    let xml = r#"<d:multistatus xmlns:d="DAV:" xmlns:a="DAV:" xmlns:c="urn:ietf:params:xml:ns:caldav"><d:response xmlns:x="urn:x"><d:href>/u/MAIL/</d:href><d:propstat><d:prop><d:resourcetype><d:collection/></d:resourcetype><d:displayname>Mail</d:displayname></d:prop></d:propstat></d:response><a:response><a:href>/u/CAL/</a:href><a:propstat><a:prop><a:resourcetype><a:collection/><c:calendar/></a:resourcetype><a:displayname>Calendar</a:displayname></a:prop></a:propstat></a:response></d:multistatus>"#;
    let list = cals(xml);
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].href, "/u/CAL/");
    assert_eq!(list[0].display_name.as_deref(), Some("Calendar"));
}

#[test]
fn kerio_bare_cr_line_endings() {
    // Kerio Connect separates iCalendar lines with a bare CR.
    let xml = "<a:multistatus xmlns:a=\"DAV:\" xmlns:c=\"urn:ietf:params:xml:ns:caldav\">\
<a:response><a:href>/full-calendars/d.com/u/CAL/x.eml</a:href>\
<a:propstat><a:status>HTTP/1.1 200 OK</a:status><a:prop>\
<c:calendar-data>BEGIN:VCALENDAR\rVERSION:2.0\rBEGIN:VEVENT\r\
UID:kerio-1\rRRULE:FREQ=WEEKLY;UNTIL=20270131T225900Z;INTERVAL=2;BYDAY=TU\r\
DTSTART;TZID=Europe/Amsterdam:20260519T133000\r\
DTEND;TZID=Europe/Amsterdam:20260519T140000\r\
TRANSP:OPAQUE\rEND:VEVENT\rEND:VCALENDAR\r\n\
</c:calendar-data></a:prop></a:propstat></a:response></a:multistatus>";
    let list = events(xml);
    assert_eq!(list.len(), 1);
    let vevent = &crate::utils::split_vevents(&list[0].ical_data)[0];
    let field = |name| crate::utils::extract_vevent_field(vevent, name);
    assert_eq!(field("UID").as_deref(), Some("kerio-1"));
    assert_eq!(field("DTSTART").as_deref(), Some("20260519T133000"));
    assert_eq!(field("DTEND").as_deref(), Some("20260519T140000"));
    assert!(field("RRULE").is_some());
}
