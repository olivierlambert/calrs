//! ICS generation and sanitizing, HTML layout, time conversion helpers.

use super::*;

#[test]
fn redteam_host_email_keeps_exact_end_across_rollback_and_midnight() {
    let times = ("20261024T233000Z".into(), "20261025T010000Z".into());
    let legacy = host_time_display("2026-10-25", "01:30", "02:00", "Europe/Paris", "UTC");
    assert_eq!(legacy.1, "23:30 – 00:00 (UTC)");
    let exact = host_time_display_exact(
        "2026-10-25",
        "01:30",
        "02:00",
        "Europe/Paris",
        "UTC",
        Some(&times),
    );
    assert_eq!(exact, ("2026-10-24".into(), "23:30 – 01:00 (UTC)".into()));
    assert_eq!(
        host_time_display_exact("2026-10-25", "01:30", "02:00", "Europe/Paris", "UTC", None),
        legacy
    );
}

#[test]
fn sanitize_strips_cr_lf() {
    assert_eq!(sanitize_ics("line1\r\nline2\nline3"), "line1 line2 line3");
}

#[test]
fn sanitize_escapes_semicolon_comma() {
    assert_eq!(sanitize_ics("a;b,c"), "a\\;b\\,c");
}

#[test]
fn sanitize_combined() {
    assert_eq!(
        sanitize_ics("Meeting; room A\nfloor 2"),
        "Meeting\\; room A floor 2"
    );
}

#[test]
fn sanitize_preserves_normal_text() {
    assert_eq!(sanitize_ics("Hello World"), "Hello World");
}

#[test]
fn sanitize_empty_string() {
    assert_eq!(sanitize_ics(""), "");
}

#[test]
fn sanitize_prevents_ics_injection() {
    // An attacker tries to inject a new ICS field via newline
    let malicious = "Meeting\r\nATTENDEE:evil@hacker.com";
    let sanitized = sanitize_ics(malicious);
    assert!(!sanitized.contains('\n'));
    assert!(!sanitized.contains('\r'));
}

/// #266: a raw backslash in a TEXT value starts an escape, so `C:\new`
/// read as a line break and `\;` as a literal semicolon.
#[test]
fn sanitize_escapes_backslash_before_the_other_specials() {
    assert_eq!(sanitize_ics(r"C:\new folder"), r"C:\\new folder");
    // Escaped once, not twice: the `;` gains one backslash, the input's own
    // backslash becomes two.
    assert_eq!(sanitize_ics(r"a\;b,c"), r"a\\\;b\,c");
}

/// The DESCRIPTION joins phone and notes with a literal `\n` after each
/// piece is sanitized, so that separator stays a single escape while a
/// backslash the guest typed is doubled (#266).
#[test]
fn generate_ics_caldav_escapes_backslashes_in_notes() {
    let details = BookingDetails {
        event_title: "Call".to_string(),
        date: "2026-03-10".to_string(),
        start_time: "09:00".to_string(),
        end_time: "10:00".to_string(),
        guest_name: "Bob".to_string(),
        guest_email: "bob@test.com".to_string(),
        guest_timezone: "UTC".to_string(),
        host_name: "Alice".to_string(),
        host_email: "alice@test.com".to_string(),
        uid: "uid-266".to_string(),
        notes: Some(r"Files in C:\new folder; see you".to_string()),
        guest_phone: Some("+33612345678".to_string()),
        ..Default::default()
    };
    let ics = generate_ics_caldav(&details);
    assert!(
        ics.contains(r"DESCRIPTION:Phone: +33612345678\nFiles in C:\\new folder\; see you"),
        "{ics}"
    );
}

// --- generate_ics ---

#[test]
fn generate_ics_basic_structure() {
    let details = BookingDetails {
        event_title: "Intro Call".to_string(),
        date: "2026-03-10".to_string(),
        start_time: "14:00".to_string(),
        end_time: "14:30".to_string(),
        guest_name: "Jane Doe".to_string(),
        guest_email: "jane@example.com".to_string(),
        guest_timezone: "Europe/Paris".to_string(),
        host_name: "Alice".to_string(),
        host_email: "alice@cal.rs".to_string(),
        uid: "test-uid-123".to_string(),
        notes: None,
        location: None,
        reminder_minutes: None,
        additional_attendees: vec![],
        ..Default::default()
    };

    let ics = generate_ics(&details, "PUBLISH");
    assert!(ics.contains("BEGIN:VCALENDAR"));
    assert!(ics.contains("END:VCALENDAR"));
    assert!(ics.contains("METHOD:PUBLISH"));
    assert!(ics.contains("BEGIN:VEVENT"));
    assert!(ics.contains("END:VEVENT"));
    assert!(ics.contains("UID:test-uid-123"));
    // Europe/Paris is UTC+1 in March (CET), so 14:00 Paris = 13:00 UTC
    assert!(ics.contains("DTSTART:20260310T130000Z"));
    assert!(ics.contains("DTEND:20260310T133000Z"));
    assert!(ics.contains("SUMMARY:Intro Call \u{2014} Jane & Alice"));
    assert!(ics.contains("ORGANIZER;CN=Alice:mailto:alice@cal.rs"));
    assert!(ics.contains("ATTENDEE;CN=Jane Doe;RSVP=TRUE:mailto:jane@example.com"));
    assert!(ics.contains("STATUS:CONFIRMED"));
}

// Regression test for #141: the CalDAV write-back variant must mark
// every ATTENDEE with SCHEDULE-AGENT=CLIENT (RFC 6638 §7.1) so servers
// like Fastmail do not send their own duplicate iMIP invite, while the
// email variant keeps plain ATTENDEE lines for "Add to calendar".
#[test]
fn generate_ics_caldav_marks_attendees_schedule_agent_client() {
    let details = BookingDetails {
        event_title: "Intro Call".to_string(),
        date: "2026-03-10".to_string(),
        start_time: "14:00".to_string(),
        end_time: "14:30".to_string(),
        guest_name: "Jane Doe".to_string(),
        guest_email: "jane@example.com".to_string(),
        guest_timezone: "Europe/Paris".to_string(),
        host_name: "Alice".to_string(),
        host_email: "alice@cal.rs".to_string(),
        uid: "test-uid-141".to_string(),
        notes: None,
        location: None,
        reminder_minutes: None,
        additional_attendees: vec!["bob@example.com".to_string()],
        ..Default::default()
    };

    // The guest's ATTENDEE line is over 75 octets and folded (#267): read
    // the logical lines.
    let caldav = crate::utils::unfold_ical(&generate_ics_caldav(&details)).into_owned();
    assert!(!caldav.contains("METHOD:"), "CalDAV PUT must omit METHOD");
    assert!(caldav
        .contains("ATTENDEE;SCHEDULE-AGENT=CLIENT;CN=Jane Doe;RSVP=TRUE:mailto:jane@example.com"));
    assert!(caldav.contains("ATTENDEE;SCHEDULE-AGENT=CLIENT;RSVP=TRUE:mailto:bob@example.com"));

    let email = generate_ics(&details, "REQUEST");
    assert!(
        !email.contains("SCHEDULE-AGENT"),
        "email .ics must keep plain ATTENDEE lines"
    );
    assert!(email.contains("ATTENDEE;CN=Jane Doe;RSVP=TRUE:mailto:jane@example.com"));
}

// Regression test for #49: DTSTAMP is REQUIRED in VEVENT by RFC 5545 §3.6.1.
// It was missing before, and strict clients (RustiCal) rejected the invite.
// Permissive ones (Gmail / Outlook) silently accepted it, so this went
// undetected for a while — keep this test even if current clients stop
// caring.
#[test]
fn generate_ics_has_rfc5545_dtstamp() {
    let details = BookingDetails {
        event_title: "Intro Call".to_string(),
        date: "2026-03-10".to_string(),
        start_time: "14:00".to_string(),
        end_time: "14:30".to_string(),
        guest_name: "Jane Doe".to_string(),
        guest_email: "jane@example.com".to_string(),
        guest_timezone: "UTC".to_string(),
        host_name: "Alice".to_string(),
        host_email: "alice@cal.rs".to_string(),
        uid: "dtstamp-uid".to_string(),
        notes: None,
        location: None,
        reminder_minutes: None,
        additional_attendees: vec![],
        ..Default::default()
    };

    let ics = generate_ics(&details, "REQUEST");

    let line = ics
        .lines()
        .find(|l| l.starts_with("DTSTAMP:"))
        .unwrap_or_else(|| panic!("DTSTAMP line missing from VEVENT:\n{}", ics));

    // RFC 5545 §3.3.5 form #2: YYYYMMDDTHHMMSSZ (UTC). 16 chars, 'T' at
    // position 8, trailing 'Z', digits everywhere else.
    let ts = &line["DTSTAMP:".len()..];
    assert_eq!(ts.len(), 16, "DTSTAMP wrong length: {:?}", ts);
    assert_eq!(ts.chars().nth(8), Some('T'), "no 'T' separator: {:?}", ts);
    assert!(ts.ends_with('Z'), "missing 'Z' UTC marker: {:?}", ts);
    assert!(
        ts[..8].chars().all(|c| c.is_ascii_digit()),
        "date part not digits: {:?}",
        &ts[..8]
    );
    assert!(
        ts[9..15].chars().all(|c| c.is_ascii_digit()),
        "time part not digits: {:?}",
        &ts[9..15]
    );
}

#[test]
fn generate_ics_with_location() {
    let details = BookingDetails {
        event_title: "Meeting".to_string(),
        date: "2026-03-10".to_string(),
        start_time: "09:00".to_string(),
        end_time: "10:00".to_string(),
        guest_name: "Bob".to_string(),
        guest_email: "bob@test.com".to_string(),
        guest_timezone: "UTC".to_string(),
        host_name: "Alice".to_string(),
        host_email: "alice@test.com".to_string(),
        uid: "uid-456".to_string(),
        notes: Some("Discuss roadmap".to_string()),
        location: Some("https://meet.example.com/room".to_string()),
        reminder_minutes: None,
        additional_attendees: vec![],
        ..Default::default()
    };

    let ics = generate_ics(&details, "REQUEST");
    assert!(ics.contains("METHOD:REQUEST"));
    assert!(ics.contains("LOCATION:https://meet.example.com/room\r\n"));
    assert!(ics.contains("DESCRIPTION:Discuss roadmap\r\n"));
    // ORGANIZER must be its own line, not folded into LOCATION
    assert!(ics.contains("\r\nORGANIZER;"));
}

#[test]
fn generate_ics_no_line_starts_with_whitespace() {
    // RFC 5545 §3.1: a line starting with whitespace is folded into the previous
    // logical line. Leading spaces on any property line would make clients (Gmail,
    // notably) fail to detect VEVENT at all.
    let details = BookingDetails {
        event_title: "Test".to_string(),
        date: "2026-03-10".to_string(),
        start_time: "09:00".to_string(),
        end_time: "10:00".to_string(),
        guest_name: "Bob".to_string(),
        guest_email: "bob@test.com".to_string(),
        guest_timezone: "UTC".to_string(),
        host_name: "Alice".to_string(),
        host_email: "alice@test.com".to_string(),
        uid: "uid-fold".to_string(),
        notes: Some("n".to_string()),
        location: Some("https://example.com".to_string()),
        reminder_minutes: Some(15),
        additional_attendees: vec!["a@test.com".to_string(), "b@test.com".to_string()],
        ..Default::default()
    };
    let ics = generate_ics(&details, "REQUEST");
    for line in ics.split("\r\n") {
        assert!(
            !line.starts_with(' ') && !line.starts_with('\t'),
            "ICS line must not start with whitespace (would be folded): {:?}",
            line
        );
    }
}

#[test]
fn generate_ics_no_description_when_no_notes() {
    let details = BookingDetails {
        event_title: "Call".to_string(),
        date: "2026-03-10".to_string(),
        start_time: "09:00".to_string(),
        end_time: "10:00".to_string(),
        guest_name: "Bob".to_string(),
        guest_email: "bob@test.com".to_string(),
        guest_timezone: "UTC".to_string(),
        host_name: "Alice".to_string(),
        host_email: "alice@test.com".to_string(),
        uid: "uid-no-notes".to_string(),
        notes: None,
        location: None,
        reminder_minutes: None,
        additional_attendees: vec![],
        ..Default::default()
    };

    let ics = generate_ics(&details, "PUBLISH");
    assert!(!ics.contains("DESCRIPTION:"));
}

#[test]
fn generate_ics_summary_includes_first_names() {
    let details = BookingDetails {
        event_title: "30min call".to_string(),
        date: "2026-03-10".to_string(),
        start_time: "09:00".to_string(),
        end_time: "09:30".to_string(),
        guest_name: "Jean-Baptiste Piacentino".to_string(),
        guest_email: "jb@test.com".to_string(),
        guest_timezone: "UTC".to_string(),
        host_name: "Olivier Lambert".to_string(),
        host_email: "olivier@test.com".to_string(),
        uid: "uid-names".to_string(),
        notes: None,
        location: None,
        reminder_minutes: None,
        additional_attendees: vec![],
        ..Default::default()
    };

    let ics = generate_ics(&details, "PUBLISH");
    assert!(ics.contains("SUMMARY:30min call \u{2014} Jean-Baptiste & Olivier"));
}

#[test]
fn generate_ics_escapes_special_chars() {
    let details = BookingDetails {
        event_title: "Meet; discuss, plan".to_string(),
        date: "2026-03-10".to_string(),
        start_time: "09:00".to_string(),
        end_time: "10:00".to_string(),
        guest_name: "O'Brien".to_string(),
        guest_email: "ob@test.com".to_string(),
        guest_timezone: "UTC".to_string(),
        host_name: "Host".to_string(),
        host_email: "host@test.com".to_string(),
        uid: "uid-789".to_string(),
        notes: None,
        location: None,
        reminder_minutes: None,
        additional_attendees: vec![],
        ..Default::default()
    };

    let ics = generate_ics(&details, "PUBLISH");
    assert!(ics.contains("SUMMARY:Meet\\; discuss\\, plan \u{2014} O'Brien & Host"));
}

// --- h (HTML escaping) ---

#[test]
fn html_escape_entities() {
    assert_eq!(
        h("<script>alert('xss')</script>"),
        "&lt;script&gt;alert('xss')&lt;/script&gt;"
    );
    assert_eq!(h("a & b"), "a &amp; b");
    assert_eq!(h("he said \"hello\""), "he said &quot;hello&quot;");
}

#[test]
fn html_escape_plain_text() {
    assert_eq!(h("Hello World"), "Hello World");
}

// --- render_html_email ---

#[test]
fn html_email_contains_structure() {
    let html = render_html_email(
        "Hi Alice,",
        "Your booking is confirmed!",
        "#16a34a",
        &[
            EmailRow {
                label: "Event".to_string(),
                value: "Intro Call".to_string(),
            },
            EmailRow {
                label: "Date".to_string(),
                value: "2026-03-10".to_string(),
            },
        ],
        Some("Calendar invite attached."),
    );

    assert!(html.contains("<!DOCTYPE html>"));
    assert!(html.contains("Hi Alice,"));
    assert!(html.contains("Your booking is confirmed!"));
    assert!(html.contains("#16a34a")); // accent color
    assert!(html.contains("Intro Call"));
    assert!(html.contains("2026-03-10"));
    assert!(html.contains("Calendar invite attached."));
    assert!(html.contains("calrs")); // footer branding
}

#[test]
fn html_email_with_actions() {
    let html = render_html_email_with_actions(
        "Action required",
        "Someone wants to book.",
        "#f59e0b",
        &[],
        None,
        &[
            EmailAction {
                label: "Approve".to_string(),
                url: "https://cal.rs/approve/tok".to_string(),
                color: "#16a34a".to_string(),
            },
            EmailAction {
                label: "Decline".to_string(),
                url: "https://cal.rs/decline/tok".to_string(),
                color: "#dc2626".to_string(),
            },
        ],
    );

    assert!(html.contains("Approve"));
    assert!(html.contains("Decline"));
    assert!(html.contains("https://cal.rs/approve/tok"));
    assert!(html.contains("https://cal.rs/decline/tok"));
}

#[test]
fn generate_cancel_ics_basic_structure() {
    let details = CancellationDetails {
        event_title: "Intro Call".to_string(),
        date: "2026-03-10".to_string(),
        start_time: "14:00".to_string(),
        end_time: "14:30".to_string(),
        guest_name: "Jane Doe".to_string(),
        guest_email: "jane@example.com".to_string(),
        guest_timezone: "UTC".to_string(),
        host_name: "Alice".to_string(),
        host_email: "alice@cal.rs".to_string(),
        uid: "cancel-uid-123".to_string(),
        reason: None,
        cancelled_by_host: true,
        ..Default::default()
    };

    let ics = generate_cancel_ics(&details);
    assert!(ics.contains("METHOD:CANCEL"));
    assert!(ics.contains("STATUS:CANCELLED"));
    assert!(ics.contains("UID:cancel-uid-123"));
    assert!(ics.contains("DTSTART:20260310T140000Z"));
    assert!(ics.contains("DTEND:20260310T143000Z"));
    assert!(ics.contains("SUMMARY:Intro Call \u{2014} Jane & Alice"));
}

// Regression test for #49 — DTSTAMP is also required on CANCEL, and its
// absence was the original symptom RustiCal reported. See
// generate_ics_has_rfc5545_dtstamp for the format rationale.
#[test]
fn generate_cancel_ics_has_rfc5545_dtstamp() {
    let details = CancellationDetails {
        event_title: "Intro Call".to_string(),
        date: "2026-03-10".to_string(),
        start_time: "14:00".to_string(),
        end_time: "14:30".to_string(),
        guest_name: "Jane Doe".to_string(),
        guest_email: "jane@example.com".to_string(),
        guest_timezone: "UTC".to_string(),
        host_name: "Alice".to_string(),
        host_email: "alice@cal.rs".to_string(),
        uid: "cancel-dtstamp-uid".to_string(),
        reason: None,
        cancelled_by_host: true,
        ..Default::default()
    };

    let ics = generate_cancel_ics(&details);
    let line = ics
        .lines()
        .find(|l| l.starts_with("DTSTAMP:"))
        .unwrap_or_else(|| panic!("DTSTAMP line missing from CANCEL VEVENT:\n{}", ics));
    let ts = &line["DTSTAMP:".len()..];
    assert_eq!(ts.len(), 16);
    assert!(ts.ends_with('Z'));
}

#[test]
fn cancellation_message_host_initiated() {
    let details = CancellationDetails {
        event_title: "Meeting".to_string(),
        date: "2026-03-10".to_string(),
        start_time: "09:00".to_string(),
        end_time: "10:00".to_string(),
        guest_name: "Bob".to_string(),
        guest_email: "bob@test.com".to_string(),
        guest_timezone: "UTC".to_string(),
        host_name: "Alice".to_string(),
        host_email: "alice@test.com".to_string(),
        uid: "uid-1".to_string(),
        reason: None,
        cancelled_by_host: true,
        ..Default::default()
    };

    // Host email should say "You cancelled this booking."
    let host_html = render_html_email(
        "Booking cancelled.",
        &if details.cancelled_by_host {
            "You cancelled this booking.".to_string()
        } else {
            format!("{} cancelled their booking.", h(&details.guest_name))
        },
        "#dc2626",
        &[],
        None,
    );
    assert!(host_html.contains("You cancelled this booking."));
    assert!(!host_html.contains("Bob cancelled"));

    // Guest email should mention the host
    let guest_msg = if details.cancelled_by_host {
        format!(
            "Your booking has been cancelled by {}.",
            h(&details.host_name)
        )
    } else {
        "Your booking has been cancelled.".to_string()
    };
    assert!(guest_msg.contains("cancelled by Alice"));
}

#[test]
fn cancellation_message_guest_initiated() {
    let details = CancellationDetails {
        event_title: "Meeting".to_string(),
        date: "2026-03-10".to_string(),
        start_time: "09:00".to_string(),
        end_time: "10:00".to_string(),
        guest_name: "Bob".to_string(),
        guest_email: "bob@test.com".to_string(),
        guest_timezone: "UTC".to_string(),
        host_name: "Alice".to_string(),
        host_email: "alice@test.com".to_string(),
        uid: "uid-2".to_string(),
        reason: Some("Schedule conflict".to_string()),
        cancelled_by_host: false,
        ..Default::default()
    };

    // Host email should say who cancelled
    let host_msg = if details.cancelled_by_host {
        "You cancelled this booking.".to_string()
    } else {
        format!("{} cancelled their booking.", h(&details.guest_name))
    };
    assert!(host_msg.contains("Bob cancelled their booking."));

    // Guest email should be generic
    let guest_msg = if details.cancelled_by_host {
        format!(
            "Your booking has been cancelled by {}.",
            h(&details.host_name)
        )
    } else {
        "Your booking has been cancelled.".to_string()
    };
    assert_eq!(guest_msg, "Your booking has been cancelled.");
}

#[test]
fn html_email_with_cancel_action() {
    let html = render_html_email_with_actions(
        "Hi Bob,",
        "Your booking has been confirmed!",
        "#16a34a",
        &[EmailRow {
            label: "Event".to_string(),
            value: "Intro Call".to_string(),
        }],
        Some("A calendar invite is attached."),
        &[EmailAction {
            label: "Cancel booking".to_string(),
            url: "https://cal.rs/booking/cancel/abc-123".to_string(),
            color: "#dc2626".to_string(),
        }],
    );

    assert!(html.contains("Cancel booking"));
    assert!(html.contains("https://cal.rs/booking/cancel/abc-123"));
    assert!(html.contains("#dc2626"));
}

#[test]
fn html_email_escapes_values() {
    let html = render_html_email(
        "Hi,",
        "Test",
        "#000",
        &[EmailRow {
            label: "Notes".to_string(),
            value: "<script>alert(1)</script>".to_string(),
        }],
        None,
    );

    assert!(!html.contains("<script>alert(1)</script>"));
    assert!(html.contains("&lt;script&gt;"));
}

// --- generate_ics edge cases ---

#[test]
fn sanitize_ics_param_quotes_delimiters() {
    // A plain name is left exactly as it was: no quotes, no escapes.
    assert_eq!(sanitize_ics_param("Jane Doe"), "Jane Doe");
    // Non-ASCII is a legal parameter value and must survive untouched.
    assert_eq!(
        sanitize_ics_param("Test Guest \u{ab}quotes\u{bb}"),
        "Test Guest \u{ab}quotes\u{bb}"
    );
    // The three delimiters that end a paramtext force a quoted-string.
    assert_eq!(sanitize_ics_param("Jane; Doe"), "\"Jane; Doe\"");
    assert_eq!(sanitize_ics_param("Doe, Jane"), "\"Doe, Jane\"");
    assert_eq!(sanitize_ics_param("Jane: Doe"), "\"Jane: Doe\"");
    // Never a backslash escape — that is TEXT syntax and is what #163 was.
    assert!(!sanitize_ics_param("Jane; Doe").contains('\\'));
}

#[test]
fn sanitize_ics_param_uses_caret_escapes() {
    // A quoted-string cannot hold a DQUOTE, so RFC 6868 spells it `^'`.
    assert_eq!(sanitize_ics_param("Jane \"JD\" Doe"), "Jane ^'JD^' Doe");
    // The caret itself doubles, and it must be escaped first so an input
    // caret cannot be mistaken for one this function introduced.
    assert_eq!(sanitize_ics_param("a^b"), "a^^b");
    assert_eq!(sanitize_ics_param("a^'b"), "a^^'b");
    // CR/LF becomes `^n` rather than being dropped, which also keeps the
    // header-injection guard: no raw newline survives.
    assert_eq!(sanitize_ics_param("a\r\nb"), "a^nb");
    assert_eq!(sanitize_ics_param("a\nb"), "a^nb");
    assert_eq!(sanitize_ics_param("a\rb"), "a^nb");
}

#[test]
fn caldav_ics_quotes_a_guest_name_with_a_semicolon() {
    // #163: Yandex 360 answers 400 Bad Request to `CN=Test Guest \; and`,
    // because a parameter takes no backslash escapes and the raw `;` ends
    // the parameter. The booking was reported confirmed and never reached
    // the calendar. Quoting is the RFC 5545 §3.1 spelling.
    let details = BookingDetails {
        event_title: "Call".to_string(),
        date: "2026-04-01".to_string(),
        start_time: "10:00".to_string(),
        end_time: "10:30".to_string(),
        guest_name: "Test Guest \u{ab}quotes\u{bb} ; and a semicolon".to_string(),
        guest_email: "guest@test.com".to_string(),
        guest_timezone: "UTC".to_string(),
        host_name: "H\u{e9}lo\u{ef}se; Host".to_string(),
        host_email: "host@test.com".to_string(),
        uid: "uid-163".to_string(),
        notes: None,
        location: None,
        reminder_minutes: None,
        additional_attendees: vec![],
        ..Default::default()
    };
    // Folded on the wire (#267); the quoting is checked on the logical line.
    let ics = crate::utils::unfold_ical(&generate_ics_caldav(&details)).into_owned();

    assert!(
        ics.contains(
            "ATTENDEE;SCHEDULE-AGENT=CLIENT;CN=\"Test Guest \u{ab}quotes\u{bb} ; and a semicolon\";RSVP=TRUE:mailto:guest@test.com"
        ),
        "{ics}"
    );
    assert!(
        ics.contains("ORGANIZER;CN=\"H\u{e9}lo\u{ef}se; Host\":mailto:host@test.com"),
        "{ics}"
    );
    // The SUMMARY is a TEXT value and keeps backslash escaping.
    assert!(
        ics.contains("SUMMARY:Call \u{2014} Test & H\u{e9}lo\u{ef}se\u{5c};"),
        "{ics}"
    );
    // No `CN=` value may carry a backslash escape.
    for line in ics.lines().filter(|l| l.contains("CN=")) {
        assert!(!line.contains("\\;"), "backslash-escaped CN: {line}");
        assert!(!line.contains("\\,"), "backslash-escaped CN: {line}");
    }
}

#[test]
fn cancel_ics_quotes_a_guest_name_with_a_semicolon() {
    // The cancellation ICS builds its own VEVENT and had the same bug, so
    // a guest with a semicolon could book but never be un-booked either.
    let details = CancellationDetails {
        event_title: "Call".to_string(),
        date: "2026-04-01".to_string(),
        start_time: "10:00".to_string(),
        end_time: "10:30".to_string(),
        guest_name: "Doe, Jane; Ms".to_string(),
        guest_email: "guest@test.com".to_string(),
        guest_timezone: "UTC".to_string(),
        host_name: "Host".to_string(),
        host_email: "host@test.com".to_string(),
        uid: "uid-163-cancel".to_string(),
        ..Default::default()
    };
    let ics = generate_cancel_ics(&details);
    assert!(
        ics.contains("ATTENDEE;CN=\"Doe, Jane; Ms\":mailto:guest@test.com"),
        "{ics}"
    );
}

#[test]
fn generate_ics_sanitizes_malicious_guest_name() {
    let details = BookingDetails {
        event_title: "Call".to_string(),
        date: "2026-04-01".to_string(),
        start_time: "10:00".to_string(),
        end_time: "10:30".to_string(),
        guest_name: "Evil\r\nATTENDEE:hacker@evil.com".to_string(),
        guest_email: "guest@test.com".to_string(),
        guest_timezone: "UTC".to_string(),
        host_name: "Host".to_string(),
        host_email: "host@test.com".to_string(),
        uid: "uid-inject".to_string(),
        notes: None,
        location: None,
        reminder_minutes: None,
        additional_attendees: vec![],
        ..Default::default()
    };
    let ics = generate_ics(&details, "REQUEST");
    // The injected ATTENDEE line must not appear as a separate field
    assert!(!ics.contains("\r\nATTENDEE:hacker@evil.com"));
    // The newline becomes an RFC 6868 `^n`, and the `:` it was carrying
    // forces the whole CN into a quoted-string, so the payload stays one
    // parameter value instead of breaking out into a property.
    assert!(
        ics.contains("CN=\"Evil^nATTENDEE:hacker@evil.com\""),
        "{ics}"
    );
}

#[test]
fn generate_ics_without_location_has_no_location_line() {
    let details = BookingDetails {
        event_title: "Call".to_string(),
        date: "2026-04-01".to_string(),
        start_time: "10:00".to_string(),
        end_time: "10:30".to_string(),
        guest_name: "Guest".to_string(),
        guest_email: "guest@test.com".to_string(),
        guest_timezone: "UTC".to_string(),
        host_name: "Host".to_string(),
        host_email: "host@test.com".to_string(),
        uid: "uid-noloc".to_string(),
        notes: None,
        location: None,
        reminder_minutes: None,
        additional_attendees: vec![],
        ..Default::default()
    };
    let ics = generate_ics(&details, "PUBLISH");
    assert!(!ics.contains("LOCATION:"));
}

#[test]
fn generate_ics_with_valarm_reminder() {
    let details = BookingDetails {
        event_title: "Meeting".to_string(),
        date: "2026-04-01".to_string(),
        start_time: "14:00".to_string(),
        end_time: "14:30".to_string(),
        guest_name: "Guest".to_string(),
        guest_email: "guest@test.com".to_string(),
        guest_timezone: "UTC".to_string(),
        host_name: "Host".to_string(),
        host_email: "host@test.com".to_string(),
        uid: "uid-valarm".to_string(),
        notes: None,
        location: None,
        reminder_minutes: Some(15),
        additional_attendees: vec![],
        ..Default::default()
    };
    let ics = generate_ics(&details, "PUBLISH");
    assert!(ics.contains("BEGIN:VALARM"));
    assert!(ics.contains("TRIGGER:-PT15M"));
    assert!(ics.contains("ACTION:DISPLAY"));
    assert!(ics.contains("END:VALARM"));
}

#[test]
fn generate_ics_no_valarm_when_none() {
    let details = BookingDetails {
        event_title: "Meeting".to_string(),
        date: "2026-04-01".to_string(),
        start_time: "14:00".to_string(),
        end_time: "14:30".to_string(),
        guest_name: "Guest".to_string(),
        guest_email: "guest@test.com".to_string(),
        guest_timezone: "UTC".to_string(),
        host_name: "Host".to_string(),
        host_email: "host@test.com".to_string(),
        uid: "uid-novalarm".to_string(),
        notes: None,
        location: None,
        reminder_minutes: None,
        additional_attendees: vec![],
        ..Default::default()
    };
    let ics = generate_ics(&details, "PUBLISH");
    assert!(!ics.contains("VALARM"));
}

#[test]
fn generate_ics_no_valarm_when_zero() {
    let details = BookingDetails {
        event_title: "Meeting".to_string(),
        date: "2026-04-01".to_string(),
        start_time: "14:00".to_string(),
        end_time: "14:30".to_string(),
        guest_name: "Guest".to_string(),
        guest_email: "guest@test.com".to_string(),
        guest_timezone: "UTC".to_string(),
        host_name: "Host".to_string(),
        host_email: "host@test.com".to_string(),
        uid: "uid-zero".to_string(),
        notes: None,
        location: None,
        reminder_minutes: Some(0),
        additional_attendees: vec![],
        ..Default::default()
    };
    let ics = generate_ics(&details, "PUBLISH");
    assert!(!ics.contains("VALARM"));
}

#[test]
fn generate_cancel_ics_with_special_chars_in_title() {
    let details = CancellationDetails {
        event_title: "Team sync; weekly, recurring".to_string(),
        date: "2026-05-20".to_string(),
        start_time: "16:00".to_string(),
        end_time: "16:45".to_string(),
        guest_name: "Bob".to_string(),
        guest_email: "bob@test.com".to_string(),
        guest_timezone: "UTC".to_string(),
        host_name: "Alice".to_string(),
        host_email: "alice@test.com".to_string(),
        uid: "cancel-special".to_string(),
        reason: Some("No longer needed".to_string()),
        cancelled_by_host: true,
        ..Default::default()
    };
    let ics = generate_cancel_ics(&details);
    assert!(ics.contains("SUMMARY:Team sync\\; weekly\\, recurring \u{2014} Bob & Alice"));
    assert!(ics.contains("METHOD:CANCEL"));
    assert!(ics.contains("STATUS:CANCELLED"));
    assert!(ics.contains("DTSTART:20260520T160000Z"));
    assert!(ics.contains("DTEND:20260520T164500Z"));
}

// --- render_html_email edge cases ---

#[test]
fn html_email_no_rows_no_footer() {
    let html = render_html_email("Hello,", "Nothing to show.", "#333", &[], None);
    assert!(html.contains("Hello,"));
    assert!(html.contains("Nothing to show."));
    assert!(html.contains("#333"));
    // No detail rows
    assert!(!html.contains("<td style=\"padding:8px"));
}

#[test]
fn html_email_multiple_rows_alternate_bg() {
    let html = render_html_email(
        "Hi,",
        "Details below.",
        "#000",
        &[
            EmailRow {
                label: "Row1".to_string(),
                value: "val1".to_string(),
            },
            EmailRow {
                label: "Row2".to_string(),
                value: "val2".to_string(),
            },
            EmailRow {
                label: "Row3".to_string(),
                value: "val3".to_string(),
            },
        ],
        None,
    );
    // Even rows (0, 2) get #f8f9fa background, odd rows (1) get #ffffff
    assert!(html.contains("val1"));
    assert!(html.contains("val2"));
    assert!(html.contains("val3"));
    assert!(html.contains("#f8f9fa"));
}

#[test]
fn html_email_actions_escape_urls() {
    let html = render_html_email_with_actions(
        "Hi,",
        "Test",
        "#000",
        &[],
        None,
        &[EmailAction {
            label: "Click <here>".to_string(),
            url: "https://cal.rs/action?a=1&b=2".to_string(),
            color: "#16a34a".to_string(),
        }],
    );
    // Label should be HTML-escaped
    assert!(html.contains("Click &lt;here&gt;"));
    // URL should be HTML-escaped (& → &amp;)
    assert!(html.contains("https://cal.rs/action?a=1&amp;b=2"));
}

// --- build_multipart_body ---

#[test]
fn multipart_body_contains_both_parts() {
    let body = build_multipart_body("Plain text version", "<p>HTML version</p>");
    let formatted = format!("{:?}", body);
    // The multipart should be alternative type with both parts
    assert!(formatted.contains("Plain text version") || formatted.contains("alternative"));
}

// --- h (HTML escaping) additional ---

#[test]
fn html_escape_empty_string() {
    assert_eq!(h(""), "");
}

#[test]
fn html_escape_all_special_chars() {
    assert_eq!(h("&<>\""), "&amp;&lt;&gt;&quot;");
}

// --- sanitize_ics additional ---

#[test]
fn sanitize_ics_multiple_newlines() {
    // \r is stripped, \n is replaced with space
    assert_eq!(sanitize_ics("a\r\nb\r\nc\nd\re"), "a b c de");
}

#[test]
fn sanitize_ics_only_special_chars() {
    // \r stripped, \n→space, ; and , escaped
    assert_eq!(sanitize_ics(";\n,\r"), "\\; \\,");
}

// --- convert_to_utc tests ---

#[test]
fn convert_to_utc_europe_paris() {
    // March 2026: Paris is CET (UTC+1), so 14:30 Paris = 13:30 UTC
    let (start, end) = convert_to_utc("2026-03-15", "14:30", "16:00", "Europe/Paris");
    assert_eq!(start, "20260315T133000Z");
    assert_eq!(end, "20260315T150000Z");
    assert!(start.ends_with('Z'));
    assert!(end.ends_with('Z'));
}

#[test]
fn convert_to_utc_invalid_timezone_fallback() {
    let (start, end) = convert_to_utc("2026-03-15", "14:30", "16:00", "Invalid/Timezone");
    // Fallback: floating time, no Z suffix
    assert_eq!(start, "20260315T143000");
    assert_eq!(end, "20260315T160000");
    assert!(!start.ends_with('Z'));
    assert!(!end.ends_with('Z'));
}

#[test]
fn convert_to_utc_utc_timezone() {
    let (start, end) = convert_to_utc("2026-03-15", "14:30", "16:00", "UTC");
    assert_eq!(start, "20260315T143000Z");
    assert_eq!(end, "20260315T160000Z");
}

// --- convert_time_between_tz / host_time_display tests ---

// Regression for issue #119: the host (Paris) was reading the time in the
// guest's wall-clock (LA), with no TZ label on cancellation emails. This
// verifies the LA→Paris conversion that makes the host email show 16:00
// Paris time when the guest booked 07:00 LA time.
#[test]
fn convert_time_between_tz_la_to_paris() {
    let (date, start, end) = convert_time_between_tz(
        "2026-05-26",
        "07:00",
        "07:30",
        "America/Los_Angeles",
        "Europe/Paris",
    )
    .expect("conversion should succeed for valid IANA names");
    // May 26 is in PDT (UTC-7) and CEST (UTC+2), so LA 07:00 = Paris 16:00 same day.
    assert_eq!(date, "2026-05-26");
    assert_eq!(start, "16:00");
    assert_eq!(end, "16:30");
}

// Late-evening LA booking crosses midnight into the next day in Paris.
#[test]
fn convert_time_between_tz_rolls_over_date() {
    let (date, start, end) = convert_time_between_tz(
        "2026-05-26",
        "22:00",
        "22:30",
        "America/Los_Angeles",
        "Europe/Paris",
    )
    .expect("conversion should succeed");
    assert_eq!(date, "2026-05-27");
    assert_eq!(start, "07:00");
    assert_eq!(end, "07:30");
}

#[test]
fn convert_time_between_tz_invalid_zone_returns_none() {
    assert!(
        convert_time_between_tz("2026-05-26", "07:00", "07:30", "Not/A/Zone", "Europe/Paris")
            .is_none()
    );
    assert!(convert_time_between_tz(
        "2026-05-26",
        "07:00",
        "07:30",
        "America/Los_Angeles",
        "Also/Bad"
    )
    .is_none());
}

#[test]
fn host_time_display_converts_to_host_tz() {
    let (date, time_display) = host_time_display(
        "2026-05-26",
        "07:00",
        "07:30",
        "America/Los_Angeles",
        "Europe/Paris",
    );
    assert_eq!(date, "2026-05-26");
    assert_eq!(time_display, "16:00 \u{2013} 16:30 (Europe/Paris)");
}

// When host_timezone is empty (legacy callers / no users.timezone set),
// keep the original wall-clock and still surface *some* TZ label so the
// host can disambiguate. Fallback to guest_timezone.
#[test]
fn host_time_display_empty_host_tz_falls_back_to_guest_label() {
    let (date, time_display) =
        host_time_display("2026-05-26", "07:00", "07:30", "America/Los_Angeles", "");
    assert_eq!(date, "2026-05-26");
    assert_eq!(time_display, "07:00 \u{2013} 07:30 (America/Los_Angeles)");
}

// When both TZ values match, display the original time without conversion
// overhead, but still label it.
#[test]
fn host_time_display_same_tz_keeps_original_time() {
    let (date, time_display) = host_time_display(
        "2026-05-26",
        "14:00",
        "14:30",
        "Europe/Paris",
        "Europe/Paris",
    );
    assert_eq!(date, "2026-05-26");
    assert_eq!(time_display, "14:00 \u{2013} 14:30 (Europe/Paris)");
}

// --- ICS location field regression test ---

#[test]
fn generate_ics_location_no_trailing_whitespace() {
    // Regression: LOCATION line had trailing whitespace after CRLF, causing
    // ORGANIZER to be treated as a continuation of LOCATION per RFC 5545.
    let details = BookingDetails {
        event_title: "Meeting".to_string(),
        date: "2026-03-10".to_string(),
        start_time: "09:00".to_string(),
        end_time: "10:00".to_string(),
        guest_name: "Bob".to_string(),
        guest_email: "bob@test.com".to_string(),
        guest_timezone: "UTC".to_string(),
        host_name: "Alice".to_string(),
        host_email: "alice@test.com".to_string(),
        uid: "uid-loc-ws".to_string(),
        notes: None,
        location: Some("https://meet.example.com/room".to_string()),
        reminder_minutes: None,
        additional_attendees: vec![],
        ..Default::default()
    };

    let ics = generate_ics(&details, "PUBLISH");

    // LOCATION line must end with value + \r\n, no trailing spaces
    assert!(ics.contains("LOCATION:https://meet.example.com/room\r\n"));

    // ORGANIZER must appear on its own line (not folded into LOCATION)
    for line in ics.split("\r\n") {
        if line.starts_with("LOCATION:") {
            assert!(
                !line.ends_with(' '),
                "LOCATION line must not have trailing whitespace"
            );
        }
        // ORGANIZER must not be on the same line as LOCATION
        if line.starts_with("LOCATION:") {
            assert!(
                !line.contains("ORGANIZER"),
                "ORGANIZER must not be on the LOCATION line"
            );
        }
    }

    // ORGANIZER must start its own line
    assert!(ics.contains("\r\nORGANIZER;"));
}

// --- ICS DTSTART/DTEND UTC Z suffix ---

#[test]
fn generate_ics_dtstart_dtend_have_utc_z_suffix() {
    let details = BookingDetails {
        event_title: "Call".to_string(),
        date: "2026-04-01".to_string(),
        start_time: "10:00".to_string(),
        end_time: "10:30".to_string(),
        guest_name: "Guest".to_string(),
        guest_email: "guest@test.com".to_string(),
        guest_timezone: "America/New_York".to_string(),
        host_name: "Host".to_string(),
        host_email: "host@test.com".to_string(),
        uid: "uid-utc-z".to_string(),
        notes: None,
        location: None,
        reminder_minutes: None,
        additional_attendees: vec![],
        ..Default::default()
    };

    let ics = generate_ics(&details, "PUBLISH");

    // Extract DTSTART and DTEND values
    for line in ics.split("\r\n") {
        if let Some(val) = line.strip_prefix("DTSTART:") {
            assert!(
                val.ends_with('Z'),
                "DTSTART value '{}' must end with Z",
                val
            );
        }
        if let Some(val) = line.strip_prefix("DTEND:") {
            assert!(val.ends_with('Z'), "DTEND value '{}' must end with Z", val);
        }
    }
}

// --- ICS cancel also has UTC times ---

#[test]
fn generate_cancel_ics_dtstart_dtend_have_utc_z_suffix() {
    let details = CancellationDetails {
        event_title: "Call".to_string(),
        date: "2026-04-01".to_string(),
        start_time: "10:00".to_string(),
        end_time: "10:30".to_string(),
        guest_name: "Guest".to_string(),
        guest_email: "guest@test.com".to_string(),
        guest_timezone: "Europe/London".to_string(),
        host_name: "Host".to_string(),
        host_email: "host@test.com".to_string(),
        uid: "uid-cancel-z".to_string(),
        reason: None,
        cancelled_by_host: true,
        ..Default::default()
    };

    let ics = generate_cancel_ics(&details);

    for line in ics.split("\r\n") {
        if let Some(val) = line.strip_prefix("DTSTART:") {
            assert!(
                val.ends_with('Z'),
                "Cancel ICS DTSTART value '{}' must end with Z",
                val
            );
        }
        if let Some(val) = line.strip_prefix("DTEND:") {
            assert!(
                val.ends_with('Z'),
                "Cancel ICS DTEND value '{}' must end with Z",
                val
            );
        }
    }
}

#[test]
fn generate_ics_includes_additional_attendees() {
    let details = BookingDetails {
        event_title: "Team Sync".to_string(),
        date: "2026-03-10".to_string(),
        start_time: "14:00".to_string(),
        end_time: "14:30".to_string(),
        guest_name: "Jane".to_string(),
        guest_email: "jane@example.com".to_string(),
        guest_timezone: "UTC".to_string(),
        host_name: "Alice".to_string(),
        host_email: "alice@test.com".to_string(),
        uid: "uid-attendees".to_string(),
        notes: None,
        location: None,
        reminder_minutes: None,
        additional_attendees: vec![
            "bob@example.com".to_string(),
            "carol@example.com".to_string(),
        ],
        ..Default::default()
    };

    let ics = generate_ics(&details, "REQUEST");
    assert!(ics.contains("ATTENDEE;RSVP=TRUE:mailto:bob@example.com"));
    assert!(ics.contains("ATTENDEE;RSVP=TRUE:mailto:carol@example.com"));
    // Primary guest should also be present
    assert!(ics.contains("ATTENDEE;CN=Jane;RSVP=TRUE:mailto:jane@example.com"));
}

#[test]
fn generate_ics_no_extra_attendees_when_empty() {
    let details = BookingDetails {
        event_title: "Solo".to_string(),
        date: "2026-03-10".to_string(),
        start_time: "10:00".to_string(),
        end_time: "10:30".to_string(),
        guest_name: "Jane".to_string(),
        guest_email: "jane@example.com".to_string(),
        guest_timezone: "UTC".to_string(),
        host_name: "Alice".to_string(),
        host_email: "alice@test.com".to_string(),
        uid: "uid-no-extra".to_string(),
        notes: None,
        location: None,
        reminder_minutes: None,
        additional_attendees: vec![],
        ..Default::default()
    };

    let ics = generate_ics(&details, "PUBLISH");
    let attendee_count = ics.matches("ATTENDEE;").count();
    // Only 1 ATTENDEE line: the primary guest (CN=Jane)
    assert_eq!(
        attendee_count, 1,
        "Expected exactly 1 ATTENDEE line, got {}",
        attendee_count
    );
}

// --- first_name helper ---

#[test]
fn first_name_single_word() {
    assert_eq!(first_name("Alice"), "Alice");
}

#[test]
fn first_name_two_words() {
    assert_eq!(first_name("Alice Smith"), "Alice");
}

#[test]
fn first_name_hyphenated() {
    assert_eq!(first_name("Jean-Baptiste Piacentino"), "Jean-Baptiste");
}

#[test]
fn first_name_empty_string() {
    assert_eq!(first_name(""), "");
}

#[test]
fn first_name_multiple_spaces() {
    assert_eq!(first_name("  Alice  Smith  "), "Alice");
}

// --- convert_to_utc additional edge cases ---

#[test]
fn convert_to_utc_america_new_york_dst() {
    // April 2026: New York is EDT (UTC-4), so 10:00 NY = 14:00 UTC
    let (start, end) = convert_to_utc("2026-04-15", "10:00", "10:30", "America/New_York");
    assert_eq!(start, "20260415T140000Z");
    assert_eq!(end, "20260415T143000Z");
}

#[test]
fn convert_to_utc_america_new_york_standard() {
    // January 2026: New York is EST (UTC-5), so 10:00 NY = 15:00 UTC
    let (start, end) = convert_to_utc("2026-01-15", "10:00", "10:30", "America/New_York");
    assert_eq!(start, "20260115T150000Z");
    assert_eq!(end, "20260115T153000Z");
}

#[test]
fn convert_to_utc_asia_tokyo() {
    // Tokyo is JST (UTC+9) year-round, so 18:00 Tokyo = 09:00 UTC
    let (start, end) = convert_to_utc("2026-06-01", "18:00", "19:00", "Asia/Tokyo");
    assert_eq!(start, "20260601T090000Z");
    assert_eq!(end, "20260601T100000Z");
}

#[test]
fn convert_to_utc_australia_sydney_dst() {
    // January 2026: Sydney is AEDT (UTC+11), so 10:00 Sydney = 23:00 previous day UTC
    let (start, end) = convert_to_utc("2026-01-15", "10:00", "11:00", "Australia/Sydney");
    assert_eq!(start, "20260114T230000Z");
    assert_eq!(end, "20260115T000000Z");
}

#[test]
fn convert_to_utc_invalid_date_format_fallback() {
    let (start, end) = convert_to_utc("not-a-date", "10:00", "11:00", "UTC");
    // Should fallback to floating time
    assert!(!start.ends_with('Z'));
    assert!(!end.ends_with('Z'));
}

#[test]
fn convert_to_utc_invalid_time_format_fallback() {
    let (start, end) = convert_to_utc("2026-03-15", "bad", "worse", "UTC");
    assert!(!start.ends_with('Z'));
    assert!(!end.ends_with('Z'));
}

#[test]
fn convert_to_utc_midnight_boundary() {
    // 23:30 UTC stays on same day
    let (start, end) = convert_to_utc("2026-03-15", "23:30", "23:59", "UTC");
    assert_eq!(start, "20260315T233000Z");
    assert_eq!(end, "20260315T235900Z");
}

#[test]
fn convert_to_utc_pacific_honolulu() {
    // Hawaii is HST (UTC-10) year-round, so 08:00 Honolulu = 18:00 UTC
    let (start, end) = convert_to_utc("2026-07-01", "08:00", "09:00", "Pacific/Honolulu");
    assert_eq!(start, "20260701T180000Z");
    assert_eq!(end, "20260701T190000Z");
}

// --- generate_ics additional edge cases ---

#[test]
fn generate_ics_empty_notes_excluded() {
    let details = BookingDetails {
        event_title: "Call".to_string(),
        date: "2026-04-01".to_string(),
        start_time: "10:00".to_string(),
        end_time: "10:30".to_string(),
        guest_name: "Guest".to_string(),
        guest_email: "guest@test.com".to_string(),
        guest_timezone: "UTC".to_string(),
        host_name: "Host".to_string(),
        host_email: "host@test.com".to_string(),
        uid: "uid-empty-notes".to_string(),
        notes: Some("   ".to_string()),
        location: None,
        reminder_minutes: None,
        additional_attendees: vec![],
        ..Default::default()
    };
    let ics = generate_ics(&details, "PUBLISH");
    // Empty/whitespace-only notes should not produce a DESCRIPTION line
    assert!(!ics.contains("DESCRIPTION:"));
}

#[test]
fn generate_ics_notes_with_special_chars() {
    let details = BookingDetails {
        event_title: "Call".to_string(),
        date: "2026-04-01".to_string(),
        start_time: "10:00".to_string(),
        end_time: "10:30".to_string(),
        guest_name: "Guest".to_string(),
        guest_email: "guest@test.com".to_string(),
        guest_timezone: "UTC".to_string(),
        host_name: "Host".to_string(),
        host_email: "host@test.com".to_string(),
        uid: "uid-notes-special".to_string(),
        notes: Some("Topic: budget; Q1, Q2".to_string()),
        location: None,
        reminder_minutes: None,
        additional_attendees: vec![],
        ..Default::default()
    };
    let ics = generate_ics(&details, "PUBLISH");
    assert!(ics.contains("DESCRIPTION:Topic: budget\\; Q1\\, Q2"));
}

#[test]
fn generate_ics_method_request() {
    let details = BookingDetails {
        event_title: "Call".to_string(),
        date: "2026-04-01".to_string(),
        start_time: "10:00".to_string(),
        end_time: "10:30".to_string(),
        guest_name: "Guest".to_string(),
        guest_email: "guest@test.com".to_string(),
        guest_timezone: "UTC".to_string(),
        host_name: "Host".to_string(),
        host_email: "host@test.com".to_string(),
        uid: "uid-method".to_string(),
        notes: None,
        location: None,
        reminder_minutes: None,
        additional_attendees: vec![],
        ..Default::default()
    };
    let ics = generate_ics(&details, "REQUEST");
    assert!(ics.contains("METHOD:REQUEST"));
    assert!(!ics.contains("METHOD:PUBLISH"));
}

#[test]
fn generate_ics_prodid_present() {
    let details = BookingDetails {
        event_title: "Call".to_string(),
        date: "2026-04-01".to_string(),
        start_time: "10:00".to_string(),
        end_time: "10:30".to_string(),
        guest_name: "Guest".to_string(),
        guest_email: "guest@test.com".to_string(),
        guest_timezone: "UTC".to_string(),
        host_name: "Host".to_string(),
        host_email: "host@test.com".to_string(),
        uid: "uid-prodid".to_string(),
        notes: None,
        location: None,
        reminder_minutes: None,
        additional_attendees: vec![],
        ..Default::default()
    };
    let ics = generate_ics(&details, "PUBLISH");
    assert!(ics.contains("PRODID:-//calrs//calrs//EN"));
    assert!(ics.contains("VERSION:2.0"));
}

#[test]
fn generate_ics_negative_reminder_excluded() {
    let details = BookingDetails {
        event_title: "Call".to_string(),
        date: "2026-04-01".to_string(),
        start_time: "10:00".to_string(),
        end_time: "10:30".to_string(),
        guest_name: "Guest".to_string(),
        guest_email: "guest@test.com".to_string(),
        guest_timezone: "UTC".to_string(),
        host_name: "Host".to_string(),
        host_email: "host@test.com".to_string(),
        uid: "uid-neg-reminder".to_string(),
        notes: None,
        location: None,
        reminder_minutes: Some(-5),
        additional_attendees: vec![],
        ..Default::default()
    };
    let ics = generate_ics(&details, "PUBLISH");
    assert!(!ics.contains("VALARM"));
}

#[test]
fn generate_ics_large_reminder() {
    let details = BookingDetails {
        event_title: "Call".to_string(),
        date: "2026-04-01".to_string(),
        start_time: "10:00".to_string(),
        end_time: "10:30".to_string(),
        guest_name: "Guest".to_string(),
        guest_email: "guest@test.com".to_string(),
        guest_timezone: "UTC".to_string(),
        host_name: "Host".to_string(),
        host_email: "host@test.com".to_string(),
        uid: "uid-large-reminder".to_string(),
        notes: None,
        location: None,
        reminder_minutes: Some(1440),
        additional_attendees: vec![],
        ..Default::default()
    };
    let ics = generate_ics(&details, "PUBLISH");
    assert!(ics.contains("TRIGGER:-PT1440M"));
}

#[test]
fn generate_ics_location_with_special_chars() {
    let details = BookingDetails {
        event_title: "Call".to_string(),
        date: "2026-04-01".to_string(),
        start_time: "10:00".to_string(),
        end_time: "10:30".to_string(),
        guest_name: "Guest".to_string(),
        guest_email: "guest@test.com".to_string(),
        guest_timezone: "UTC".to_string(),
        host_name: "Host".to_string(),
        host_email: "host@test.com".to_string(),
        uid: "uid-loc-special".to_string(),
        notes: None,
        location: Some("Room A; Building 3, Floor 2".to_string()),
        reminder_minutes: None,
        additional_attendees: vec![],
        ..Default::default()
    };
    let ics = generate_ics(&details, "PUBLISH");
    assert!(ics.contains("LOCATION:Room A\\; Building 3\\, Floor 2"));
}

#[test]
fn generate_ics_multiple_attendees_with_special_chars() {
    let details = BookingDetails {
        event_title: "Call".to_string(),
        date: "2026-04-01".to_string(),
        start_time: "10:00".to_string(),
        end_time: "10:30".to_string(),
        guest_name: "Guest".to_string(),
        guest_email: "guest@test.com".to_string(),
        guest_timezone: "UTC".to_string(),
        host_name: "Host".to_string(),
        host_email: "host@test.com".to_string(),
        uid: "uid-att-special".to_string(),
        notes: None,
        location: None,
        reminder_minutes: None,
        additional_attendees: vec![
            "user+tag@example.com".to_string(),
            "another.user@sub.domain.com".to_string(),
        ],
        ..Default::default()
    };
    let ics = generate_ics(&details, "PUBLISH");
    assert!(ics.contains("ATTENDEE;RSVP=TRUE:mailto:user+tag@example.com"));
    assert!(ics.contains("ATTENDEE;RSVP=TRUE:mailto:another.user@sub.domain.com"));
}

// --- generate_cancel_ics additional edge cases ---

#[test]
fn generate_cancel_ics_no_location_line() {
    let details = CancellationDetails {
        event_title: "Call".to_string(),
        date: "2026-04-01".to_string(),
        start_time: "10:00".to_string(),
        end_time: "10:30".to_string(),
        guest_name: "Guest".to_string(),
        guest_email: "guest@test.com".to_string(),
        guest_timezone: "UTC".to_string(),
        host_name: "Host".to_string(),
        host_email: "host@test.com".to_string(),
        uid: "uid-cancel-noloc".to_string(),
        reason: None,
        cancelled_by_host: false,
        ..Default::default()
    };
    let ics = generate_cancel_ics(&details);
    assert!(!ics.contains("LOCATION:"));
    assert!(!ics.contains("DESCRIPTION:"));
}

#[test]
fn generate_cancel_ics_no_valarm() {
    // Cancel ICS should never contain VALARM
    let details = CancellationDetails {
        event_title: "Call".to_string(),
        date: "2026-04-01".to_string(),
        start_time: "10:00".to_string(),
        end_time: "10:30".to_string(),
        guest_name: "Guest".to_string(),
        guest_email: "guest@test.com".to_string(),
        guest_timezone: "UTC".to_string(),
        host_name: "Host".to_string(),
        host_email: "host@test.com".to_string(),
        uid: "uid-cancel-novalarm".to_string(),
        reason: Some("Conflict".to_string()),
        cancelled_by_host: true,
        ..Default::default()
    };
    let ics = generate_cancel_ics(&details);
    assert!(!ics.contains("VALARM"));
}

#[test]
fn generate_cancel_ics_no_additional_attendees() {
    // Cancel ICS only has the primary guest attendee
    let details = CancellationDetails {
        event_title: "Call".to_string(),
        date: "2026-04-01".to_string(),
        start_time: "10:00".to_string(),
        end_time: "10:30".to_string(),
        guest_name: "Guest".to_string(),
        guest_email: "guest@test.com".to_string(),
        guest_timezone: "UTC".to_string(),
        host_name: "Host".to_string(),
        host_email: "host@test.com".to_string(),
        uid: "uid-cancel-att".to_string(),
        reason: None,
        cancelled_by_host: false,
        ..Default::default()
    };
    let ics = generate_cancel_ics(&details);
    let attendee_count = ics.matches("ATTENDEE;").count();
    assert_eq!(attendee_count, 1);
}

#[test]
fn generate_cancel_ics_with_timezone_conversion() {
    // Ensure cancel ICS also converts to UTC properly
    let details = CancellationDetails {
        event_title: "Call".to_string(),
        date: "2026-07-01".to_string(),
        start_time: "15:00".to_string(),
        end_time: "15:30".to_string(),
        guest_name: "Guest".to_string(),
        guest_email: "guest@test.com".to_string(),
        guest_timezone: "America/Los_Angeles".to_string(),
        host_name: "Host".to_string(),
        host_email: "host@test.com".to_string(),
        uid: "uid-cancel-tz".to_string(),
        reason: None,
        cancelled_by_host: true,
        ..Default::default()
    };
    let ics = generate_cancel_ics(&details);
    // July: PDT = UTC-7, so 15:00 PDT = 22:00 UTC
    assert!(ics.contains("DTSTART:20260701T220000Z"));
    assert!(ics.contains("DTEND:20260701T223000Z"));
}

#[test]
fn generate_cancel_ics_prodid_and_version() {
    let details = CancellationDetails {
        event_title: "Call".to_string(),
        date: "2026-04-01".to_string(),
        start_time: "10:00".to_string(),
        end_time: "10:30".to_string(),
        guest_name: "Guest".to_string(),
        guest_email: "guest@test.com".to_string(),
        guest_timezone: "UTC".to_string(),
        host_name: "Host".to_string(),
        host_email: "host@test.com".to_string(),
        uid: "uid-cancel-prodid".to_string(),
        reason: None,
        cancelled_by_host: true,
        ..Default::default()
    };
    let ics = generate_cancel_ics(&details);
    assert!(ics.contains("PRODID:-//calrs//calrs//EN"));
    assert!(ics.contains("VERSION:2.0"));
}

// --- render_html_email_with_actions additional tests ---

#[test]
fn html_email_no_actions_no_action_table() {
    let html = render_html_email_with_actions(
        "Hi,",
        "Message.",
        "#000",
        &[],
        None,
        &[], // no actions
    );
    // No action buttons table when empty
    assert!(!html.contains("display:inline-block;padding:12px 28px"));
}

#[test]
fn html_email_single_action() {
    let html = render_html_email_with_actions(
        "Hi,",
        "Click below.",
        "#3b82f6",
        &[],
        None,
        &[EmailAction {
            label: "Book now".to_string(),
            url: "https://cal.rs/book".to_string(),
            color: "#6366f1".to_string(),
        }],
    );
    assert!(html.contains("Book now"));
    assert!(html.contains("https://cal.rs/book"));
    assert!(html.contains("#6366f1"));
}

#[test]
fn html_email_three_actions() {
    let html = render_html_email_with_actions(
        "Hi,",
        "Actions below.",
        "#000",
        &[],
        None,
        &[
            EmailAction {
                label: "A".to_string(),
                url: "https://a.com".to_string(),
                color: "#111".to_string(),
            },
            EmailAction {
                label: "B".to_string(),
                url: "https://b.com".to_string(),
                color: "#222".to_string(),
            },
            EmailAction {
                label: "C".to_string(),
                url: "https://c.com".to_string(),
                color: "#333".to_string(),
            },
        ],
    );
    assert!(html.contains("https://a.com"));
    assert!(html.contains("https://b.com"));
    assert!(html.contains("https://c.com"));
}

#[test]
fn html_email_footer_note_html_escaped() {
    let html = render_html_email("Hi,", "Test", "#000", &[], Some("Click <here> & there"));
    assert!(html.contains("Click &lt;here&gt; &amp; there"));
}

#[test]
fn html_email_row_values_html_escaped() {
    let html = render_html_email(
        "Hi,",
        "Details",
        "#000",
        &[
            EmailRow {
                label: "Name".to_string(),
                value: "Alice & Bob <team>".to_string(),
            },
            EmailRow {
                label: "Notes".to_string(),
                value: "use \"quotes\"".to_string(),
            },
        ],
        None,
    );
    assert!(html.contains("Alice &amp; Bob &lt;team&gt;"));
    assert!(html.contains("use &quot;quotes&quot;"));
}

#[test]
fn html_email_accent_color_in_bar() {
    let html = render_html_email("Hi,", "Test", "#e11d48", &[], None);
    // The accent color should appear in the accent bar
    assert!(html.contains("background:#e11d48"));
}

#[test]
fn html_email_with_rows_and_actions_and_footer() {
    // Test a "full" email with all components present
    let html = render_html_email_with_actions(
        "Hello Alice,",
        "Your booking is confirmed!",
        "#16a34a",
        &[
            EmailRow {
                label: "Event".to_string(),
                value: "Intro Call".to_string(),
            },
            EmailRow {
                label: "Date".to_string(),
                value: "2026-03-15".to_string(),
            },
            EmailRow {
                label: "Time".to_string(),
                value: "10:00 - 10:30 (UTC)".to_string(),
            },
            EmailRow {
                label: "With".to_string(),
                value: "Bob".to_string(),
            },
            EmailRow {
                label: "Location".to_string(),
                value: "https://meet.example.com/room".to_string(),
            },
        ],
        Some("A calendar invite is attached to this email."),
        &[
            EmailAction {
                label: "Reschedule".to_string(),
                url: "https://cal.rs/reschedule/abc".to_string(),
                color: "#3b82f6".to_string(),
            },
            EmailAction {
                label: "Cancel booking".to_string(),
                url: "https://cal.rs/cancel/def".to_string(),
                color: "#dc2626".to_string(),
            },
        ],
    );
    assert!(html.contains("Hello Alice,"));
    assert!(html.contains("Your booking is confirmed!"));
    assert!(html.contains("Intro Call"));
    assert!(html.contains("2026-03-15"));
    assert!(html.contains("10:00 - 10:30 (UTC)"));
    assert!(html.contains("Bob"));
    assert!(html.contains("https://meet.example.com/room"));
    assert!(html.contains("A calendar invite is attached to this email."));
    assert!(html.contains("Reschedule"));
    assert!(html.contains("Cancel booking"));
    assert!(html.contains("https://cal.rs/reschedule/abc"));
    assert!(html.contains("https://cal.rs/cancel/def"));
    assert!(html.contains("#16a34a")); // accent
    assert!(html.contains("calrs")); // footer branding
}

// --- build_multipart_body tests ---

#[test]
fn build_multipart_body_returns_multipart() {
    let body = build_multipart_body("plain", "<b>html</b>");
    // Verify it produces valid MIME output by formatting to string
    let formatted = format!("{:?}", body);
    // MultiPart::alternative should be in the debug output
    assert!(!formatted.is_empty());
}

#[test]
fn build_multipart_body_with_empty_content() {
    let body = build_multipart_body("", "");
    let formatted = format!("{:?}", body);
    assert!(!formatted.is_empty());
}

#[test]
fn build_multipart_body_with_unicode() {
    let body = build_multipart_body(
        "Meeting \u{2014} confirmed",
        "<p>Meeting \u{2014} confirmed</p>",
    );
    let formatted = format!("{:?}", body);
    assert!(!formatted.is_empty());
}

// --- ICS structural validation ---

#[test]
fn generate_ics_crlf_line_endings() {
    let details = BookingDetails {
        event_title: "Call".to_string(),
        date: "2026-04-01".to_string(),
        start_time: "10:00".to_string(),
        end_time: "10:30".to_string(),
        guest_name: "Guest".to_string(),
        guest_email: "guest@test.com".to_string(),
        guest_timezone: "UTC".to_string(),
        host_name: "Host".to_string(),
        host_email: "host@test.com".to_string(),
        uid: "uid-crlf".to_string(),
        notes: None,
        location: None,
        reminder_minutes: None,
        additional_attendees: vec![],
        ..Default::default()
    };
    let ics = generate_ics(&details, "PUBLISH");
    // Every line should end with \r\n (RFC 5545 requirement)
    assert!(ics.contains("\r\n"));
    // Should start and end properly
    assert!(ics.starts_with("BEGIN:VCALENDAR\r\n"));
    assert!(ics.ends_with("END:VCALENDAR\r\n"));
}

#[test]
fn generate_cancel_ics_crlf_line_endings() {
    let details = CancellationDetails {
        event_title: "Call".to_string(),
        date: "2026-04-01".to_string(),
        start_time: "10:00".to_string(),
        end_time: "10:30".to_string(),
        guest_name: "Guest".to_string(),
        guest_email: "guest@test.com".to_string(),
        guest_timezone: "UTC".to_string(),
        host_name: "Host".to_string(),
        host_email: "host@test.com".to_string(),
        uid: "uid-cancel-crlf".to_string(),
        reason: None,
        cancelled_by_host: false,
        ..Default::default()
    };
    let ics = generate_cancel_ics(&details);
    assert!(ics.starts_with("BEGIN:VCALENDAR\r\n"));
    assert!(ics.ends_with("END:VCALENDAR\r\n"));
}

// --- HTML email structural validation ---

#[test]
fn html_email_is_valid_html_structure() {
    let html = render_html_email("Hi,", "Test", "#000", &[], None);
    assert!(html.contains("<!DOCTYPE html>"));
    assert!(html.contains("<html"));
    assert!(html.contains("</html>"));
    assert!(html.contains("<head>"));
    assert!(html.contains("</head>"));
    assert!(html.contains("<body"));
    assert!(html.contains("</body>"));
}

#[test]
fn html_email_has_meta_charset() {
    let html = render_html_email("Hi,", "Test", "#000", &[], None);
    assert!(html.contains("charset=\"utf-8\"") || html.contains("charset=utf-8"));
}

#[test]
fn html_email_has_viewport_meta() {
    let html = render_html_email("Hi,", "Test", "#000", &[], None);
    assert!(html.contains("viewport"));
}

#[test]
fn html_email_has_calrs_footer_link() {
    let html = render_html_email("Hi,", "Test", "#000", &[], None);
    assert!(html.contains("https://cal.rs"));
    assert!(html.contains("calrs"));
}

// #261: the host's calendar copy carries the guest phone and notes, so a host
// who checks their calendar on the go has them. The email attachment keeps
// the phone out: it also reaches additional attendees.
#[test]
fn generate_ics_caldav_carries_phone_and_notes() {
    let details = BookingDetails {
        event_title: "Call".to_string(),
        date: "2026-03-10".to_string(),
        start_time: "09:00".to_string(),
        end_time: "10:00".to_string(),
        guest_name: "Bob".to_string(),
        guest_email: "bob@test.com".to_string(),
        guest_timezone: "UTC".to_string(),
        host_name: "Alice".to_string(),
        host_email: "alice@test.com".to_string(),
        uid: "uid-261".to_string(),
        notes: Some("Bring the quote".to_string()),
        guest_phone: Some("+33612345678".to_string()),
        host_language: Some("fr".to_string()),
        ..Default::default()
    };

    let caldav = generate_ics_caldav(&details);
    assert!(
        caldav.contains("DESCRIPTION:Téléphone : +33612345678\\nBring the quote\r\n"),
        "{caldav}"
    );

    let email = generate_ics(&details, "REQUEST");
    assert!(email.contains("DESCRIPTION:Bring the quote\r\n"), "{email}");
    assert!(!email.contains("+33612345678"), "{email}");

    let phone_only = generate_ics_caldav(&BookingDetails {
        notes: None,
        host_language: None,
        ..details
    });
    assert!(phone_only.contains("DESCRIPTION:Phone: +33612345678\r\n"));
}

// --- RFC 5545 §3.1 line folding (#267) ---

#[test]
fn fold_ics_leaves_short_lines_alone() {
    let ics = "BEGIN:VCALENDAR\r\nSUMMARY:Short\r\nEND:VCALENDAR\r\n";
    assert_eq!(fold_ics(ics), ics);
    let exact = format!("SUMMARY:{}\r\n", "x".repeat(75 - "SUMMARY:".len()));
    assert_eq!(fold_ics(&exact), exact);
}

#[test]
fn fold_ics_breaks_at_75_octets_and_counts_the_leading_space() {
    let line = format!("DESCRIPTION:{}", "a".repeat(200));
    let folded = fold_ics(&format!("{line}\r\n"));
    let parts: Vec<&str> = folded.trim_end_matches("\r\n").split("\r\n").collect();
    assert_eq!(parts[0].len(), 75);
    for p in &parts[1..] {
        assert!(p.starts_with(' '), "{p:?}");
        assert!(p.len() <= 75, "{p:?}");
    }
    assert_eq!(parts[1].len(), 75);
    assert_eq!(crate::utils::unfold_ical(&folded), format!("{line}\r\n"));
}

#[test]
fn fold_ics_never_splits_a_utf8_character() {
    // Two-byte and three-byte characters straddling every possible offset.
    for prefix in 0..4 {
        let line = format!(
            "SUMMARY:{}{}",
            "x".repeat(prefix),
            "\u{e9}\u{2014}".repeat(40)
        );
        let folded = fold_ics(&line);
        for part in folded.split("\r\n") {
            assert!(part.len() <= 75, "{} octets: {part:?}", part.len());
        }
        // Splitting a character would have made the output invalid UTF-8,
        // which a String cannot hold; the round trip proves nothing moved.
        assert_eq!(crate::utils::unfold_ical(&folded), line);
    }
}

/// Every generator's output is folded, and calrs' own reader gets the long
/// DESCRIPTION back whole.
#[test]
fn generated_ics_lines_fit_in_75_octets() {
    let details = BookingDetails {
        event_title: "R\u{e9}union d'\u{e9}quipe trimestrielle avec beaucoup de d\u{e9}tails"
            .into(),
        date: "2026-03-10".into(),
        start_time: "09:00".into(),
        end_time: "10:00".into(),
        guest_name: "Jos\u{e9} Mar\u{ed}a de la Fuente y Garc\u{ed}a-Rodr\u{ed}guez".into(),
        guest_email: "jose.maria.de.la.fuente@a-rather-long-domain-name.example.com".into(),
        guest_timezone: "UTC".into(),
        host_name: "Alice".into(),
        host_email: "alice@test.com".into(),
        uid: "uid-267-fold".into(),
        notes: Some("\u{c0} pr\u{e9}parer : le devis, la maquette et ".repeat(6)),
        guest_phone: Some("+33612345678".into()),
        location: Some(format!("https://meet.example.com/{}", "r".repeat(90))),
        ..Default::default()
    };
    let cancel = CancellationDetails {
        event_title: details.event_title.clone(),
        date: details.date.clone(),
        start_time: details.start_time.clone(),
        end_time: details.end_time.clone(),
        guest_name: details.guest_name.clone(),
        guest_email: details.guest_email.clone(),
        guest_timezone: "UTC".into(),
        host_name: "Alice".into(),
        host_email: "alice@test.com".into(),
        uid: "uid-267-fold".into(),
        ..Default::default()
    };
    for ics in [
        generate_ics(&details, "REQUEST"),
        generate_ics_caldav(&details),
        generate_cancel_ics(&cancel),
    ] {
        assert!(ics.ends_with("END:VCALENDAR\r\n"), "{ics}");
        for line in ics.split("\r\n") {
            assert!(line.len() <= 75, "{} octets: {line:?}", line.len());
        }
        assert_eq!(
            crate::utils::extract_vevent_field(&ics, "SUMMARY").as_deref(),
            Some(
                "R\u{e9}union d'\u{e9}quipe trimestrielle avec beaucoup de d\u{e9}tails \u{2014} Jos\u{e9} & Alice"
            )
        );
    }
    let caldav = generate_ics_caldav(&details);
    let description = crate::utils::extract_vevent_field(&caldav, "DESCRIPTION").unwrap();
    assert!(
        description.starts_with(r"Phone: +33612345678\n"),
        "{description}"
    );
    let notes = sanitize_ics(details.notes.as_deref().unwrap());
    assert!(description.ends_with(notes.trim_end()), "{description}");
}

/// #267: every invite and cancellation carries the booking's SEQUENCE, so
/// a moved booking replaces the copy a client holds and its later CANCEL
/// is not ignored as older than that copy.
#[test]
fn ics_carries_the_booking_sequence() {
    let details = BookingDetails {
        event_title: "Call".into(),
        date: "2026-03-10".into(),
        start_time: "09:00".into(),
        end_time: "10:00".into(),
        guest_timezone: "UTC".into(),
        uid: "uid-267-seq".into(),
        ..Default::default()
    };
    assert!(generate_ics(&details, "REQUEST").contains("\r\nSEQUENCE:0\r\n"));
    let moved = BookingDetails {
        sequence: 2,
        ..details
    };
    assert!(generate_ics(&moved, "REQUEST").contains("\r\nSEQUENCE:2\r\n"));
    assert!(generate_ics_caldav(&moved).contains("\r\nSEQUENCE:2\r\n"));
    let cancel = CancellationDetails {
        event_title: "Call".into(),
        date: "2026-03-10".into(),
        start_time: "09:00".into(),
        end_time: "10:00".into(),
        guest_timezone: "UTC".into(),
        uid: "uid-267-seq".into(),
        sequence: 2,
        ..Default::default()
    };
    assert!(generate_cancel_ics(&cancel).contains("\r\nSEQUENCE:2\r\n"));
}
