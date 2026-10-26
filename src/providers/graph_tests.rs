use super::*;
use axum::{
    body::{to_bytes, Body},
    extract::State,
    http::{Request, Response},
    Router,
};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex as StdMutex};

fn ics(uid: &str, start: &str, end: &str) -> String {
    format!("BEGIN:VCALENDAR\r\nVERSION:2.0\r\nBEGIN:VEVENT\r\nUID:{uid}\r\nDTSTART{start}\r\nDTEND{end}\r\nSUMMARY:Booking\\, test\r\nDESCRIPTION:One\\nTwo\\; three\\\\four\r\nLOCATION:https://meet.example/room\r\nORGANIZER;CN=\"Host: Name\":mailto:host@example.com\r\nATTENDEE;SCHEDULE-AGENT=CLIENT:mailto:guest@example.com\r\nBEGIN:VALARM\r\nDESCRIPTION:Not the event description\r\nTRIGGER:-PT10M\r\nACTION:DISPLAY\r\nEND:VALARM\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n")
}

fn booking() -> String {
    ics("booking@calrs", ":20260329T100000Z", ":20260329T110000Z")
}

fn event() -> Value {
    json!({
        "id": "event-id", "iCalUId": "outlook-uid", "subject": "Busy, meeting",
        "start": { "dateTime": "2026-03-29T10:00:00.0000000", "timeZone": "UTC" },
        "end": { "dateTime": "2026-03-29T11:00:00.0000000", "timeZone": "UTC" },
        "showAs": "busy", "isCancelled": false, "isAllDay": false, "type": "singleInstance",
        "body": {"contentType": "text", "content": "Hello\nworld"}
    })
}

fn convert(value: Value) -> RawEvent {
    event_to_ical(serde_json::from_value(value).unwrap()).unwrap()
}

/// A booking whose guest timezone cannot be parsed makes the shared ICS
/// generator fall back to floating (no `Z`, no `TZID`) DTSTART/DTEND values.
/// Graph has no floating times, so `booking_body` refuses the event
/// explicitly instead of guessing an offset; the write-back failure
/// machinery then records, retries and alerts the host.
#[test]
fn booking_body_refuses_a_floating_ics_from_the_shared_generator() {
    let details = crate::email::BookingDetails {
        utc_times: None,
        event_title: "Planning".into(),
        date: "2026-03-29".into(),
        start_time: "10:00".into(),
        end_time: "11:00".into(),
        guest_name: "Guest".into(),
        guest_email: "guest@example.com".into(),
        guest_timezone: "Not/AZone".into(),
        host_name: "Host".into(),
        host_email: "host@example.com".into(),
        uid: "booking@calrs".into(),
        ..Default::default()
    };
    let ics = crate::email::generate_ics_caldav(&details);
    assert!(ics.contains("DTSTART:20260329T100000"));
    let error = booking_body("booking@calrs", &ics).unwrap_err();
    assert_eq!(
        error.downcast_ref::<GraphError>(),
        Some(&GraphError::UnsupportedIcalendar)
    );
}

#[test]
fn booking_unfolds_and_unescapes_without_provider_invitations() {
    let input = booking().replace("Booking\\, test", "Booking\\, \r\n test");
    let body = booking_body("booking@calrs", &input).unwrap();
    assert_eq!(body["subject"], "Booking, test");
    assert_eq!(body["body"]["content"], "One\nTwo; three\\four");
    assert_eq!(body["location"]["displayName"], "https://meet.example/room");
    assert_eq!(body["start"]["dateTime"], "2026-03-29T10:00:00");
    assert_eq!(body["start"]["timeZone"], "UTC");
    assert_eq!(body["responseRequested"], false);
    assert!(body.get("attendees").is_none());
    assert!(body.get("organizer").is_none());
    assert!(body.get("transactionId").is_none());
}

#[test]
fn booking_iana_timezone_observes_dst_and_resolves_ambiguous_times_early() {
    let input = ics(
        "uid",
        ";TZID=Europe/Paris:20260329T100000",
        ";TZID=Europe/Paris:20260329T110000",
    );
    let body = booking_body("uid", &input).unwrap();
    assert_eq!(body["start"]["dateTime"], "2026-03-29T08:00:00");
    assert_eq!(body["end"]["dateTime"], "2026-03-29T09:00:00");
    // 02:30 happens twice on 25 October in Paris: the earlier one, in CEST.
    let body = booking_body(
        "uid",
        &ics(
            "uid",
            ";TZID=Europe/Paris:20261025T023000",
            ":20261025T120000Z",
        ),
    )
    .unwrap();
    assert_eq!(body["start"]["dateTime"], "2026-10-25T00:30:00");
    for start in [
        ";TZID=Europe/Paris:20260329T023000",
        ":20260329T100000",
        ";TZID=Pacific Standard Time:20260329T100000",
    ] {
        assert!(booking_body("uid", &ics("uid", start, ":20261025T120000Z")).is_err());
    }
}

#[test]
fn booking_dates_use_exclusive_midnight_end() {
    let body = booking_body(
        "uid",
        &ics("uid", ";VALUE=DATE:20260329", ";VALUE=DATE:20260331"),
    )
    .unwrap();
    assert_eq!(body["isAllDay"], true);
    assert_eq!(body["start"]["dateTime"], "2026-03-29T00:00:00");
    assert_eq!(body["end"]["dateTime"], "2026-03-31T00:00:00");
    assert!(booking_body(
        "uid",
        &ics("uid", ";VALUE=DATE:20260329", ":20260330T000000Z")
    )
    .is_err());
    assert!(booking_body("uid", &ics("uid", ":20260329T110000Z", ":20260329T100000Z")).is_err());
}

#[test]
fn booking_rejects_wrong_uid_recurrence_and_encoded_text() {
    assert!(booking_body("other", &booking()).is_err());
    for extra in [
        "RRULE:FREQ=DAILY",
        "DURATION:PT1H",
        "RECURRENCE-ID:20260329T100000Z",
        "DTSTART:20260329T100000Z",
    ] {
        let input = booking().replace("END:VEVENT", &format!("{extra}\r\nEND:VEVENT"));
        assert!(booking_body("booking@calrs", &input).is_err(), "{extra}");
    }
    let input = booking().replace("SUMMARY:", "SUMMARY;ENCODING=BASE64:");
    assert!(booking_body("booking@calrs", &input).is_err());
    let input = booking().replace(
        "END:VCALENDAR",
        "BEGIN:VEVENT\r\nEND:VEVENT\r\nEND:VCALENDAR",
    );
    assert!(booking_body("booking@calrs", &input).is_err());
}

#[test]
fn all_day_dates_block_whole_local_days_across_offsets_and_dst() {
    use crate::utils::{
        convert_event_to_tz, extract_vevent_field, extract_vevent_tzid, parse_ical_datetime,
    };

    // API-shaped midnight boundaries, not EWS-style converted UTC instants:
    // https://learn.microsoft.com/en-us/answers/questions/1373148/start-and-end-times-of-all-day-events-fetched-from
    for (host_zone, start, end, elapsed_hours) in [
        ("Asia/Karachi", "2023-09-30", "2023-10-01", 24),
        ("Europe/Paris", "2026-03-29", "2026-03-30", 23),
        ("America/New_York", "2026-03-08", "2026-03-09", 23),
        ("Europe/Paris", "2026-10-25", "2026-10-26", 25),
        ("America/New_York", "2026-11-01", "2026-11-02", 25),
        ("Pacific/Honolulu", "2026-03-29", "2026-03-31", 48),
    ] {
        for response_zone in ["UTC", "Central Standard Time"] {
            let mut value = event();
            value["isAllDay"] = json!(true);
            value["start"] =
                json!({"dateTime": format!("{start}T00:00:00.0000000"), "timeZone": response_zone});
            value["end"] =
                json!({"dateTime": format!("{end}T00:00:00.0000000"), "timeZone": response_zone});
            let raw = convert(value);
            assert!(raw
                .ical
                .contains(&format!("DTSTART;VALUE=DATE:{}", start.replace('-', ""))));
            assert!(raw
                .ical
                .contains(&format!("DTEND;VALUE=DATE:{}", end.replace('-', ""))));

            // The same parser and timezone conversion used by web/CLI busy-time
            // consumers must leave these as host-local midnight boundaries.
            let tz = extract_vevent_tzid(&raw.ical, "DTSTART");
            assert_eq!(tz, None);
            let host_tz: chrono_tz::Tz = host_zone.parse().unwrap();
            let busy_start = convert_event_to_tz(
                parse_ical_datetime(&extract_vevent_field(&raw.ical, "DTSTART").unwrap()).unwrap(),
                tz.as_deref(),
                host_tz,
            );
            let busy_end = convert_event_to_tz(
                parse_ical_datetime(&extract_vevent_field(&raw.ical, "DTEND").unwrap()).unwrap(),
                tz.as_deref(),
                host_tz,
            );
            let expected_start = NaiveDate::parse_from_str(start, "%Y-%m-%d")
                .unwrap()
                .and_hms_opt(0, 0, 0)
                .unwrap();
            let expected_end = NaiveDate::parse_from_str(end, "%Y-%m-%d")
                .unwrap()
                .and_hms_opt(0, 0, 0)
                .unwrap();
            assert_eq!(
                (busy_start, busy_end),
                (expected_start, expected_end),
                "{host_zone}"
            );
            let overlaps = |start, end| start < busy_end && end > busy_start;
            assert!(!overlaps(
                expected_start - Duration::minutes(30),
                expected_start
            ));
            let mut slot = expected_start;
            while slot < expected_end {
                assert!(
                    overlaps(slot, slot + Duration::minutes(30)),
                    "{host_zone}: {slot}"
                );
                slot += Duration::minutes(30);
            }
            assert!(!overlaps(
                expected_end,
                expected_end + Duration::minutes(30)
            ));
            assert_eq!(
                (host_tz.from_local_datetime(&busy_end).single().unwrap()
                    - host_tz.from_local_datetime(&busy_start).single().unwrap())
                .num_hours(),
                elapsed_hours,
            );
        }
    }
}

#[test]
fn all_day_rejects_non_midnight_or_invalid_exclusive_boundaries() {
    for (start, end, end_zone) in [
        (
            "2026-03-28T23:00:00.0000000",
            "2026-03-29T22:00:00.0000000",
            "UTC",
        ),
        (
            "2026-03-29T00:00:00.0000000",
            "2026-03-29T00:00:00.0000000",
            "UTC",
        ),
        (
            "2026-03-30T00:00:00.0000000",
            "2026-03-29T00:00:00.0000000",
            "UTC",
        ),
        (
            "2026-03-29T00:00:00.0000000",
            "2026-03-30T00:00:00.0000000",
            "Pacific Standard Time",
        ),
    ] {
        let mut value = event();
        value["isAllDay"] = json!(true);
        value["start"]["dateTime"] = json!(start);
        value["end"]["dateTime"] = json!(end);
        value["end"]["timeZone"] = json!(end_zone);
        assert!(event_to_ical(serde_json::from_value(value).unwrap()).is_err());
    }
}

#[test]
fn timed_events_keep_utc_and_all_day_exceptions_keep_original_identity() {
    let timed = convert(event());
    assert!(timed.ical.contains("DTSTART:20260329T100000Z"));
    assert!(timed.ical.contains("DTEND:20260329T110000Z"));
    assert!(!timed.ical.contains("VALUE=DATE"));

    let mut value = event();
    value["isAllDay"] = json!(true);
    value["type"] = json!("exception");
    value["originalStart"] = json!("2026-03-28T23:00:00Z");
    value["start"]["dateTime"] = json!("2026-04-02T00:00:00.0000000");
    value["end"]["dateTime"] = json!("2026-04-03T00:00:00.0000000");
    let raw = convert(value);
    assert!(raw.ical.contains("UID:outlook-uid"));
    assert!(raw.ical.contains("RECURRENCE-ID:20260328T230000Z"));
    assert!(raw.ical.contains("DTSTART;VALUE=DATE:20260402"));
    assert!(raw.ical.contains("DTEND;VALUE=DATE:20260403"));
}

#[test]
fn recurrence_uses_original_start_for_rescheduled_exceptions() {
    let mut first = event();
    first["type"] = json!("occurrence");
    first["originalStart"] = json!("2026-03-29T10:00:00Z");
    let mut second = first.clone();
    second["id"] = json!("exception-id");
    second["type"] = json!("exception");
    // Rescheduled to exactly the same time as another occurrence: original
    // start, not DTSTART, remains the cache's distinct recurrence key.
    second["originalStart"] = json!("2026-03-30T10:00:00Z");
    let first = convert(first);
    let second = convert(second);
    assert!(first.ical.contains("RECURRENCE-ID:20260329T100000Z"));
    assert!(second.ical.contains("RECURRENCE-ID:20260330T100000Z"));
    assert!(first.ical.contains("UID:outlook-uid"));
    assert!(second.ical.contains("UID:outlook-uid"));
}

#[test]
fn free_tentative_busy_and_cancelled_mapping() {
    for (show_as, transp, status) in [
        ("free", "TRANSPARENT", "CONFIRMED"),
        ("tentative", "TRANSPARENT", "TENTATIVE"),
        ("workingElsewhere", "OPAQUE", "CONFIRMED"),
        ("busy", "OPAQUE", "CONFIRMED"),
        ("oof", "OPAQUE", "CONFIRMED"),
        ("unknown", "OPAQUE", "CONFIRMED"),
    ] {
        let mut value = event();
        value["showAs"] = json!(show_as);
        let raw = convert(value);
        assert!(raw.ical.contains(&format!("TRANSP:{transp}")));
        assert!(raw.ical.contains(&format!("STATUS:{status}")));
    }
    let mut value = event();
    value["isCancelled"] = json!(true);
    assert!(convert(value).ical.contains("STATUS:CANCELLED"));
}

#[test]
fn graph_dates_reject_non_utc_and_uid_prefers_our_property() {
    let mut value = event();
    value["start"]["timeZone"] = json!("Pacific Standard Time");
    assert!(event_to_ical(serde_json::from_value(value).unwrap()).is_err());
    let mut value = event();
    value["singleValueExtendedProperties"] =
        json!([{ "id": UID_PROPERTY, "value": "booking@calrs" }]);
    assert!(convert(value).ical.contains("UID:booking@calrs"));
}

#[test]
fn pagination_and_opaque_identifier_boundary() {
    let provider = GraphProvider::new("test-token").unwrap();
    for bad in [
        "https://attacker.example/v1.0/me/calendars",
        "http://graph.microsoft.com/v1.0/me/calendars",
        "https://graph.microsoft.com:444/v1.0/me/calendars",
        "https://graph.microsoft.com.evil.example/v1.0/me/calendars",
        "https://user:secret@graph.microsoft.com/v1.0/me/calendars",
        "https://graph.microsoft.com/beta/me/calendars",
        "https://graph.microsoft.com/v1.0/me/calendars#fragment",
    ] {
        assert!(
            provider.validate_url(&Url::parse(bad).unwrap()).is_err(),
            "{bad}"
        );
    }
    let url = provider
        .endpoint(&["me", "calendars", "id/with+?=#%", "events", "item/opaque"])
        .unwrap();
    assert_eq!(
        url.path(),
        "/v1.0/me/calendars/id%2Fwith+%3F=%23%25/events/item%2Fopaque"
    );
    assert!(url.query().is_none());
    assert!(provider.endpoint(&["me", "calendars", ".."]).is_err());
    assert_eq!(odata_string("a'b'"), "a''b''");
    let body = json!({"start": {"dateTime": "2026-03-31T10:00:00.0000000"}, "end": {"dateTime": "2026-03-31T11:00:00.0000000"}});
    assert_eq!(
        transaction_id("cal", "uid", 0, &body),
        transaction_id("cal", "uid", 0, &body)
    );
    assert_ne!(
        transaction_id("cal", "uid", 0, &body),
        transaction_id("other", "uid", 0, &body)
    );
    assert_ne!(
        transaction_id("cal", "uid", 0, &body),
        transaction_id("cal", "UID", 0, &body)
    );
    let moved = json!({"start": {"dateTime": "2026-04-07T10:00:00.0000000"}, "end": {"dateTime": "2026-04-07T11:00:00.0000000"}});
    assert_ne!(
        transaction_id("cal", "uid", 0, &body),
        transaction_id("cal", "uid", 0, &moved)
    );
    // A, then B (a pending reschedule deletes it), then back to A: the
    // SEQUENCE bumped by each reschedule keeps the third create apart.
    assert_ne!(
        transaction_id("cal", "uid", 0, &body),
        transaction_id("cal", "uid", 2, &body)
    );
}

#[derive(Clone)]
struct MockState {
    replies: Arc<StdMutex<VecDeque<(u16, Value, Option<String>)>>>,
    requests: Arc<StdMutex<Vec<(String, String, Value, String)>>>,
}

struct Mock {
    provider: GraphProvider,
    state: MockState,
    server: tokio::task::JoinHandle<()>,
}

impl Drop for Mock {
    fn drop(&mut self) {
        self.server.abort();
    }
}

async fn handler(State(state): State<MockState>, request: Request<Body>) -> Response<Body> {
    let (parts, body) = request.into_parts();
    let body = to_bytes(body, 1024 * 1024).await.unwrap();
    let body = if body.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&body).unwrap()
    };
    let prefer = parts
        .headers
        .get("prefer")
        .unwrap()
        .to_str()
        .unwrap()
        .to_owned();
    assert_eq!(
        parts.headers.get("authorization").unwrap(),
        "Bearer test-token"
    );
    state.requests.lock().unwrap().push((
        parts.method.to_string(),
        parts.uri.to_string(),
        body,
        prefer,
    ));
    let (status, body, location) = state
        .replies
        .lock()
        .unwrap()
        .pop_front()
        .expect("unexpected Graph request");
    let mut response = Response::builder()
        .status(status)
        .header("content-type", "application/json");
    if let Some(location) = location {
        response = response.header("location", location);
    }
    if matches!(status, 429 | 503 | 504) {
        response = response.header("retry-after", "0");
    }
    response
        .body(if status == 204 {
            Body::empty()
        } else {
            Body::from(body.to_string())
        })
        .unwrap()
}

async fn mock(replies: Vec<(u16, Value, Option<String>)>) -> Mock {
    let state = MockState {
        replies: Arc::new(StdMutex::new(replies.into())),
        requests: Arc::new(StdMutex::new(Vec::new())),
    };
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let root = Url::parse(&format!("http://{}/v1.0/", listener.local_addr().unwrap())).unwrap();
    let app = Router::new().fallback(handler).with_state(state.clone());
    let server = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    Mock {
        provider: GraphProvider::with_root("test-token", root).unwrap(),
        state,
        server,
    }
}

#[tokio::test]
async fn calendar_view_fetches_original_identity_of_moved_occurrence() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let root = Url::parse(&format!("http://{}/v1.0/", listener.local_addr().unwrap())).unwrap();
    let app = Router::new().fallback(|request: Request<Body>| async move {
        let url = Url::parse(&format!("http://localhost{}", request.uri())).unwrap();
        let mut value = event();
        value["type"] = json!("exception");
        value["start"]["dateTime"] = json!("2026-03-31T10:00:00.0000000");
        value["end"]["dateTime"] = json!("2026-03-31T11:00:00.0000000");
        // Graph only returns originalStart when explicitly selected.
        if url.query_pairs().any(|(key, fields)| {
            key == "$select" && fields.split(',').any(|f| f == "originalStart")
        }) {
            value["originalStart"] = json!("2026-03-29T10:00:00Z");
        }
        axum::Json(json!({"value": [value]}))
    });
    let server = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    let provider = GraphProvider::with_root("test-token", root).unwrap();
    let result = provider
        .fetch_events_since("cal", "2026-03-01T00:00:00Z")
        .await;
    server.abort();
    let events = result.unwrap();
    assert!(events[0].ical.contains("DTSTART:20260331T100000Z"));
    assert!(events[0].ical.contains("DTEND:20260331T110000Z"));
    assert!(events[0].ical.contains("RECURRENCE-ID:20260329T100000Z"));
    assert!(events[0].ical.contains("UID:outlook-uid"));
}

fn ok(value: Value) -> (u16, Value, Option<String>) {
    (200, value, None)
}
fn found(uid: &str, id: &str) -> Value {
    json!({"value": [{"id": id, "singleValueExtendedProperties": [{"id": UID_PROPERTY, "value": uid}]}]})
}

#[tokio::test]
async fn idempotent_create_update_readback_and_delete_use_extended_uid() {
    let mock = mock(vec![
        ok(json!({"value": []})),
        (201, json!({"id": "item/opaque"}), None),
        ok(found("booking@calrs", "item/opaque")),
        ok(json!({"id": "item/opaque"})),
        ok(found("booking@calrs", "item/opaque")),
        ok(found("booking@calrs", "item/opaque")),
        (204, Value::Null, None),
        ok(json!({"value": []})),
        ok(json!({"value": []})),
    ])
    .await;
    mock.provider
        .put_event("cal/opaque", "booking@calrs", &booking())
        .await
        .unwrap();
    mock.provider
        .put_event(
            "cal/opaque",
            "booking@calrs",
            &booking().replace("Booking", "Updated"),
        )
        .await
        .unwrap();
    assert!(mock
        .provider
        .event_exists("cal/opaque", "booking@calrs")
        .await
        .unwrap());
    mock.provider
        .delete_event("cal/opaque", "booking@calrs")
        .await
        .unwrap();
    mock.provider
        .delete_event("cal/opaque", "booking@calrs")
        .await
        .unwrap();
    assert!(!mock
        .provider
        .event_exists("cal/opaque", "booking@calrs")
        .await
        .unwrap());
    let requests = mock.state.requests.lock().unwrap();
    assert_eq!(requests.iter().filter(|r| r.0 == "POST").count(), 1);
    assert_eq!(
        requests[1].2["singleValueExtendedProperties"][0]["value"],
        "booking@calrs"
    );
    assert_eq!(
        requests[1].2["transactionId"],
        transaction_id("cal/opaque", "booking@calrs", 0, &requests[1].2)
    );
    assert_eq!(requests[3].0, "PATCH");
    assert!(requests[3].1.ends_with("/events/item%2Fopaque"));
    assert!(requests[3].2.get("transactionId").is_none());
    assert!(requests[3].2.get("singleValueExtendedProperties").is_none());
    assert_eq!(requests[3].2["subject"], "Updated, test");
    assert_eq!(requests[6].0, "DELETE");
    for request in requests.iter() {
        assert_eq!(request.3, PREFER);
        assert!(request.2.get("attendees").is_none());
    }
    assert!(mock.state.replies.lock().unwrap().is_empty());
}

#[tokio::test]
async fn property_lookup_escapes_odata_and_checks_uid_case() {
    let mock = mock(vec![ok(found("UID'O", "wrong-case"))]).await;
    assert!(!mock.provider.event_exists("cal", "uid'o").await.unwrap());
    let requests = mock.state.requests.lock().unwrap();
    let url = Url::parse(&format!("http://localhost{}", requests[0].1)).unwrap();
    let filter = url
        .query_pairs()
        .find(|(key, _)| key == "$filter")
        .unwrap()
        .1
        .into_owned();
    assert!(filter.contains("ep/value eq 'uid''o'"));
}

#[tokio::test]
async fn duplicate_uid_is_not_arbitrarily_mutated() {
    let mock = mock(vec![ok(json!({"value": [
        {"id": "one", "singleValueExtendedProperties": [{"id": UID_PROPERTY, "value": "booking@calrs"}]},
        {"id": "two", "singleValueExtendedProperties": [{"id": UID_PROPERTY, "value": "booking@calrs"}]}
    ]}))]).await;
    let error = mock
        .provider
        .put_event("cal", "booking@calrs", &booking())
        .await
        .unwrap_err();
    assert_eq!(
        error.downcast_ref::<GraphError>(),
        Some(&GraphError::DuplicateUid)
    );
    assert_eq!(mock.state.requests.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn failed_create_retry_has_same_transaction_id() {
    let mock = mock(vec![
        ok(json!({"value": []})),
        (500, json!({"secret": "NEVER LOG THIS"}), None),
        ok(json!({"value": []})),
        (201, json!({"id": "created"}), None),
    ])
    .await;
    let error = mock
        .provider
        .put_event("cal", "booking@calrs", &booking())
        .await
        .unwrap_err();
    assert_eq!(
        error.downcast_ref::<GraphError>(),
        Some(&GraphError::Http(500))
    );
    assert!(!format!("{error:?}").contains("NEVER LOG"));
    mock.provider
        .put_event("cal", "booking@calrs", &booking())
        .await
        .unwrap();
    let requests = mock.state.requests.lock().unwrap();
    assert_eq!(
        requests[1].2["transactionId"],
        requests[3].2["transactionId"]
    );
}

#[tokio::test]
async fn pagination_consumes_every_page_and_refuses_off_origin_links() {
    let mock = mock(Vec::new()).await;
    let next = mock
        .provider
        .endpoint(&["me", "calendars"])
        .unwrap()
        .to_string()
        + "?$skiptoken=opaque";
    mock.state.replies.lock().unwrap().extend([
        ok(json!({"value": [{"id": "first"}], "@odata.nextLink": next})),
        ok(json!({"value": [{"id": "second"}]})),
        ok(json!({"value": [], "@odata.nextLink": "https://attacker.example/steal"})),
    ]);
    let calendars = mock.provider.list_calendars().await.unwrap();
    assert_eq!(calendars.len(), 2);
    assert_eq!(calendars[1].id, "second");
    let error = mock.provider.list_calendars().await.unwrap_err();
    assert_eq!(
        error.downcast_ref::<GraphError>(),
        Some(&GraphError::UnsafeUrl)
    );
    assert_eq!(mock.state.requests.lock().unwrap().len(), 3);
}

#[tokio::test]
async fn calendar_view_pages_expand_recurrence_without_delta_cursor() {
    let mock = mock(Vec::new()).await;
    let next = mock
        .provider
        .endpoint(&["me", "calendars", "cal", "calendarView"])
        .unwrap()
        .to_string()
        + "?$skiptoken=two";
    let mut second = event();
    second["id"] = json!("second");
    second["type"] = json!("exception");
    second["originalStart"] = json!("2026-03-30T10:00:00Z");
    mock.state.replies.lock().unwrap().extend([
        ok(json!({"value": [event()], "@odata.nextLink": next})),
        ok(json!({"value": [second]})),
    ]);
    let events = mock
        .provider
        .fetch_events_since("cal", "2026-01-01T00:00:00Z")
        .await
        .unwrap();
    assert_eq!(events.len(), 2);
    assert!(events[1].ical.contains("RECURRENCE-ID:20260330T100000Z"));
    {
        let requests = mock.state.requests.lock().unwrap();
        let url = Url::parse(&format!("http://localhost{}", requests[0].1)).unwrap();
        assert!(url
            .query_pairs()
            .any(|(key, value)| key == "startDateTime" && value == "2026-01-01T00:00:00+00:00"));
    }
    assert!(mock
        .provider
        .sync_delta("cal", None)
        .await
        .unwrap()
        .new_sync_state
        .is_none());
    assert!(mock
        .provider
        .sync_delta("cal", Some("unsupported-cursor"))
        .await
        .is_err());
}

#[tokio::test]
async fn pagination_loops_fail_and_redirects_never_follow() {
    let mock = mock(Vec::new()).await;
    let next = mock
        .provider
        .endpoint(&["me", "calendars"])
        .unwrap()
        .to_string()
        + "?$skiptoken=loop";
    mock.state.replies.lock().unwrap().extend([
        ok(json!({"value": [], "@odata.nextLink": next})),
        ok(json!({"value": [], "@odata.nextLink": next})),
        (302, json!({}), Some("http://127.0.0.1:1/steal".to_owned())),
    ]);
    let error = mock.provider.list_calendars().await.unwrap_err();
    assert_eq!(
        error.downcast_ref::<GraphError>(),
        Some(&GraphError::PaginationLoop)
    );
    let error = mock.provider.check_connection().await.unwrap_err();
    assert_eq!(
        error.downcast_ref::<GraphError>(),
        Some(&GraphError::Http(302))
    );
    assert_eq!(mock.state.requests.lock().unwrap().len(), 3);
}

#[tokio::test]
async fn delete_item_404_is_idempotent_but_lookup_auth_failure_is_not_absence() {
    let mock = mock(vec![
        ok(found("uid", "item")),
        (404, json!({}), None),
        (401, json!({"error": "private"}), None),
    ])
    .await;
    mock.provider.delete_event("cal", "uid").await.unwrap();
    let error = mock.provider.event_exists("cal", "uid").await.unwrap_err();
    assert_eq!(
        error.downcast_ref::<GraphError>(),
        Some(&GraphError::Http(401))
    );
}

#[tokio::test]
async fn uid_lookup_follows_pages_and_rejects_missing_identity_expansion() {
    let mock = mock(Vec::new()).await;
    let next = mock
        .provider
        .endpoint(&["me", "calendars", "cal", "events"])
        .unwrap()
        .to_string()
        + "?$skiptoken=two";
    mock.state.replies.lock().unwrap().extend([
        ok(json!({"value": [], "@odata.nextLink": next})),
        ok(found("uid", "second-page-item")),
        ok(json!({"value": [{"id": "missing-expanded-property"}]})),
    ]);
    assert!(mock.provider.event_exists("cal", "uid").await.unwrap());
    let error = mock.provider.event_exists("cal", "uid").await.unwrap_err();
    assert_eq!(
        error.downcast_ref::<GraphError>(),
        Some(&GraphError::InvalidResponse)
    );
    assert_eq!(mock.state.requests.lock().unwrap().len(), 3);
}

#[tokio::test]
async fn patch_404_does_not_recreate_a_concurrently_deleted_booking() {
    let mock = mock(vec![
        ok(found("booking@calrs", "deleted-item")),
        (404, json!({}), None),
    ])
    .await;
    let error = mock
        .provider
        .put_event("cal", "booking@calrs", &booking())
        .await
        .unwrap_err();
    assert_eq!(
        error.downcast_ref::<GraphError>(),
        Some(&GraphError::Http(404))
    );
    let requests = mock.state.requests.lock().unwrap();
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[1].0, "PATCH");
}

#[tokio::test]
async fn pagination_accepts_a_reencoded_identifier_but_not_another_version() {
    let mock = mock(Vec::new()).await;
    let first = mock
        .provider
        .endpoint(&["me", "calendars", "id/with+", "calendarView"])
        .unwrap();
    assert!(first.path().contains("id%2Fwith+"));
    // Graph may write the identifier with lower-case escapes or `+` encoded.
    let next = first.to_string().replace("id%2Fwith+", "id%2fwith%2B") + "?$skiptoken=two";
    let beta = next.replace("/v1.0/", "/beta/");
    mock.state.replies.lock().unwrap().extend([
        ok(json!({"value": [event()], "@odata.nextLink": next})),
        ok(json!({"value": []})),
        ok(json!({"value": [], "@odata.nextLink": beta})),
    ]);
    let events = mock
        .provider
        .fetch_events_since("id/with+", "2026-01-01T00:00:00Z")
        .await
        .unwrap();
    assert_eq!(events.len(), 1);
    let error = mock
        .provider
        .fetch_events_since("id/with+", "2026-01-01T00:00:00Z")
        .await
        .unwrap_err();
    assert_eq!(
        error.downcast_ref::<GraphError>(),
        Some(&GraphError::UnsafeUrl)
    );
    assert_eq!(mock.state.requests.lock().unwrap().len(), 3);
}

#[tokio::test]
async fn throttled_requests_are_retried_a_bounded_number_of_times() {
    let mock = mock(vec![
        (429, json!({}), None),
        ok(json!({"value": [event()]})),
        (504, json!({}), None),
        ok(found("uid", "item")),
        (503, json!({}), None),
        (503, json!({}), None),
        (503, json!({}), None),
    ])
    .await;
    let events = mock
        .provider
        .fetch_events_since("cal", "2026-01-01T00:00:00Z")
        .await
        .unwrap();
    assert_eq!(events.len(), 1);
    assert!(mock.provider.event_exists("cal", "uid").await.unwrap());
    let error = mock
        .provider
        .fetch_events_since("cal", "2026-01-01T00:00:00Z")
        .await
        .unwrap_err();
    assert_eq!(
        error.downcast_ref::<GraphError>(),
        Some(&GraphError::Http(503))
    );
    assert_eq!(mock.state.requests.lock().unwrap().len(), 7);
}

#[tokio::test]
async fn one_unreadable_event_does_not_hide_the_others() {
    let mut odd = event();
    odd["id"] = json!("odd");
    odd["type"] = json!("somethingNew");
    let mock = mock(vec![ok(json!({"value": [event(), odd]}))]).await;
    let events = mock
        .provider
        .fetch_events_since("cal", "2026-01-01T00:00:00Z")
        .await
        .unwrap();
    assert_eq!(events.len(), 2);
    assert!(events[0].ical.contains("UID:outlook-uid"));
    assert_eq!(events[1].remote_id, "odd");
    assert!(events[1].ical.is_empty(), "marked unreadable");
}

#[tokio::test]
async fn a_calendar_without_read_access_is_reported_as_unreadable() {
    let mock = mock(vec![
        (403, json!({"error": "private"}), None),
        (404, json!({}), None),
        (401, json!({}), None),
    ])
    .await;
    for _ in 0..2 {
        let error = mock
            .provider
            .fetch_events_since("shared", "2026-01-01T00:00:00Z")
            .await
            .unwrap_err();
        assert!(error.downcast_ref::<CalendarUnreadable>().is_some());
    }
    // An expired or revoked token is not a calendar to skip.
    let error = mock
        .provider
        .fetch_events_since("own", "2026-01-01T00:00:00Z")
        .await
        .unwrap_err();
    assert_eq!(
        error.downcast_ref::<GraphError>(),
        Some(&GraphError::Http(401))
    );
}

#[tokio::test]
async fn a_booking_moved_back_to_its_first_time_gets_a_new_transaction_id() {
    let mock = mock(vec![
        ok(json!({"value": []})),
        (201, json!({"id": "first"}), None),
        ok(json!({"value": []})),
        (201, json!({"id": "again"}), None),
    ])
    .await;
    let with_sequence = |sequence: u32| {
        booking().replace(
            "UID:booking@calrs\r\n",
            &format!("UID:booking@calrs\r\nSEQUENCE:{sequence}\r\n"),
        )
    };
    mock.provider
        .put_event("cal", "booking@calrs", &with_sequence(0))
        .await
        .unwrap();
    mock.provider
        .put_event("cal", "booking@calrs", &with_sequence(2))
        .await
        .unwrap();
    let requests = mock.state.requests.lock().unwrap();
    assert_eq!(requests[1].2["start"], requests[3].2["start"]);
    assert_ne!(
        requests[1].2["transactionId"],
        requests[3].2["transactionId"]
    );
}

/// `/me/calendars` as Graph lists it once the user accepted a colleague's
/// share in Outlook: the colleague's calendar sits among the user's own, with
/// its owner and `canEdit: false` the only difference. A holiday calendar is
/// the user's own but not writable either.
#[tokio::test]
async fn listing_marks_colleagues_calendars_shared_and_read_only_ones() {
    let me = json!({"name": "Host", "address": "host@example.com"});
    let colleague = json!({"name": "Colleague", "address": "colleague@example.com"});
    let mock = mock(vec![
        ok(json!({"value": [
            {"id": "own", "name": "Calendar", "hexColor": "", "owner": me, "canEdit": true, "isDefaultCalendar": true},
            {"id": "holidays", "name": "Holidays", "hexColor": "", "owner": me, "canEdit": false, "isDefaultCalendar": false},
            {"id": "colleague", "name": "Colleague", "hexColor": "", "owner": colleague, "canEdit": false, "isDefaultCalendar": false},
            {"id": "upper", "name": "Mine too", "hexColor": "", "owner": {"address": "HOST@example.com"}, "canEdit": true}
        ]})),
        // No default calendar listed: nothing to compare owners against.
        ok(json!({"value": [{"id": "colleague", "owner": colleague}]})),
    ])
    .await;
    let calendars = mock.provider.list_calendars().await.unwrap();
    let flags: Vec<_> = calendars
        .iter()
        .map(|c| (c.id.as_str(), c.shared, c.read_only))
        .collect();
    assert_eq!(
        flags,
        [
            ("own", false, false),
            ("holidays", false, true),
            ("colleague", true, true),
            ("upper", false, false),
        ]
    );
    let calendars = mock.provider.list_calendars().await.unwrap();
    assert!(!calendars[0].shared && !calendars[0].read_only);
    let requests = mock.state.requests.lock().unwrap();
    assert!(requests[0]
        .1
        .contains("%24select=id%2Cname%2ChexColor%2Cowner%2CcanEdit%2CisDefaultCalendar"));
}

/// The nextLink Graph returned for a calendar holding more than 1000
/// occurrences: the calendar segment rewritten in OData key syntax, the query
/// reordered and re-encoded with lower-case escapes, and `$skip` appended.
#[tokio::test]
async fn pagination_follows_graphs_key_syntax_next_link() {
    let mock = mock(Vec::new()).await;
    let root = mock.provider.root.as_str().to_owned();
    let next = format!(
        "{root}me/calendars('AAkALgAAAAAAHYQDEapmEc2byACqAC-EWg0AAAAA--gAA')/calendarView\
         ?startDateTime=2026-07-28T08%3a12%3a49.537396%2b00%3a00\
         &endDateTime=2028-10-25T08%3a12%3a49.537417%2b00%3a00\
         &%24select=id%2ciCalUId%2csubject%2clocation%2cstart%2cend%2cshowAs%2cisCancelled%2cisAllDay%2ctype%2coriginalStart\
         &%24expand=singleValueExtendedProperties(%24filter%3did+eq+%27String+%7b4f742d1c-593e-4a5f-9552-d819c3db47c2%7d+Name+CalrsUid%27)\
         &%24top=1000&%24skip=1000"
    );
    let mut second = event();
    second["id"] = json!("second-page");
    mock.state.replies.lock().unwrap().extend([
        ok(json!({"value": [event()], "@odata.nextLink": next})),
        ok(json!({"value": [second]})),
    ]);
    let events = mock
        .provider
        .fetch_events_since(
            "AAkALgAAAAAAHYQDEapmEc2byACqAC-EWg0AAAAA--gAA",
            "2026-07-28T08:12:49Z",
        )
        .await
        .unwrap();
    assert_eq!(events.len(), 2);
    let requests = mock.state.requests.lock().unwrap();
    assert!(requests[0]
        .1
        .contains("/me/calendars/AAkALgAAAAAAHYQDEapmEc2byACqAC-EWg0AAAAA--gAA/calendarView"));
    // The link is followed as given, not rebuilt from the first request.
    assert!(requests[1]
        .1
        .contains("/me/calendars('AAkALgAAAAAAHYQDEapmEc2byACqAC-EWg0AAAAA--gAA')/calendarView"));
    assert!(requests[1].1.ends_with("&%24top=1000&%24skip=1000"));
}

/// Occurrences as Graph returns them: every occurrence carries its own
/// `iCalUId` (the series GlobalObjectId with the instance date in bytes
/// 16-19), `originalStart` has no fractional seconds, and an empty location
/// still comes back as an object.
#[tokio::test]
async fn occurrences_carry_distinct_ical_uids_and_original_starts() {
    let occurrence = |day: u32, uid_date: &str| {
        json!({
            "id": format!("AAkBOQAICN8y{day}"),
            "iCalUId": format!("040000008200E00074C5B7101A82E00807EA0A{uid_date}00000000000000000000000000000000000000000010000000AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"),
            "subject": "Standup",
            "originalStart": format!("2026-10-{day}T09:00:00Z"),
            "isAllDay": false, "isCancelled": false, "showAs": "busy", "type": "occurrence",
            "seriesMasterId": "AAkALgAAseries",
            "start": {"dateTime": format!("2026-10-{day}T09:00:00.0000000"), "timeZone": "UTC"},
            "end": {"dateTime": format!("2026-10-{day}T09:30:00.0000000"), "timeZone": "UTC"},
            "location": {"displayName": "", "locationType": "default", "uniqueIdType": "unknown", "address": {}, "coordinates": {}}
        })
    };
    let mut moved = occurrence(27, "1B");
    moved["type"] = json!("exception");
    moved["start"]["dateTime"] = json!("2026-10-27T13:00:00.0000000");
    moved["end"]["dateTime"] = json!("2026-10-27T13:45:00.0000000");
    let mock = mock(vec![ok(json!({"value": [
        occurrence(25, "19"),
        occurrence(26, "1A"),
        moved,
    ]}))])
    .await;
    let events = mock
        .provider
        .fetch_events_since("cal", "2026-10-01T00:00:00Z")
        .await
        .unwrap();
    let uids: HashSet<_> = events
        .iter()
        .map(|e| crate::utils::extract_vevent_field(&e.ical, "UID").unwrap())
        .collect();
    assert_eq!(uids.len(), 3, "one UID per occurrence, as Graph sends them");
    assert!(!events[0].ical.contains("LOCATION"));
    assert!(events[0].ical.contains("RECURRENCE-ID:20261025T090000Z"));
    assert!(events[2].ical.contains("DTSTART:20261027T130000Z"));
    assert!(events[2].ical.contains("RECURRENCE-ID:20261027T090000Z"));
}

/// A meeting the organizer cancelled stays in the attendee's calendar as
/// `isCancelled`, shown as free and with a "Canceled: " subject, until the
/// attendee removes it. An unanswered invitation is `tentative`.
#[test]
fn cancelled_and_unanswered_meeting_copies_do_not_block() {
    let mut cancelled = event();
    cancelled["subject"] = json!("Canceled: Planning");
    cancelled["isCancelled"] = json!(true);
    cancelled["showAs"] = json!("free");
    let raw = convert(cancelled);
    assert!(raw.ical.contains("STATUS:CANCELLED"));
    assert!(raw.ical.contains("TRANSP:TRANSPARENT"));

    let mut unanswered = event();
    unanswered["showAs"] = json!("tentative");
    unanswered["responseStatus"] =
        json!({"response": "notResponded", "time": "0001-01-01T00:00:00Z"});
    assert!(convert(unanswered).ical.contains("TRANSP:TRANSPARENT"));
}

/// A private item on a calendar shared without private access keeps its time
/// and busy status; Exchange replaces only the subject.
#[test]
fn private_item_on_a_shared_calendar_still_blocks() {
    let mut value = event();
    value["subject"] = json!("Private Appointment");
    value["sensitivity"] = json!("private");
    let raw = convert(value);
    assert!(raw.ical.contains("TRANSP:OPAQUE"));
    assert!(raw.ical.contains("DTSTART:20260329T100000Z"));
}

/// Writing to a calendar someone shared read-only is refused with
/// `RemoteCalendarAccessDenied`: a write failure, not a calendar to skip.
#[tokio::test]
async fn writing_to_a_read_only_shared_calendar_is_a_write_failure() {
    let mock = mock(vec![
        ok(json!({"value": []})),
        (
            403,
            json!({"error": {"code": "RemoteCalendarAccessDenied", "message": "Access is denied. Check credentials and try again."}}),
            None,
        ),
    ])
    .await;
    let error = mock
        .provider
        .put_event("shared", "booking@calrs", &booking())
        .await
        .unwrap_err();
    assert_eq!(
        error.downcast_ref::<GraphError>(),
        Some(&GraphError::Http(403))
    );
    assert!(error.downcast_ref::<CalendarUnreadable>().is_none());
}

/// Graph's schema lets `showAs`, `isCancelled` and other fields be null. An
/// item calrs cannot even parse is marked unreadable like any other, rather
/// than failing the whole page and, with it, the calendar.
#[tokio::test]
async fn an_item_with_a_null_field_does_not_fail_the_page() {
    let mut null_show_as = event();
    null_show_as["id"] = json!("null-show-as");
    null_show_as["showAs"] = Value::Null;
    let mut no_cancelled = event();
    no_cancelled["id"] = json!("no-is-cancelled");
    no_cancelled.as_object_mut().unwrap().remove("isCancelled");
    let mock = mock(vec![ok(
        json!({"value": [null_show_as, event(), no_cancelled]}),
    )])
    .await;
    let events = mock
        .provider
        .fetch_events_since("cal", "2026-01-01T00:00:00Z")
        .await
        .unwrap();
    let unreadable: Vec<_> = events
        .iter()
        .filter(|e| e.ical.is_empty())
        .map(|e| e.remote_id.as_str())
        .collect();
    assert_eq!(unreadable, ["null-show-as", "no-is-cancelled"]);
    assert!(events[1].ical.contains("UID:outlook-uid"));
}
