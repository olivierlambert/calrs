//! The guest email footer (#223): which emails carry it, and how.

use super::*;

const FOOTER: &str = "Questions? **Call** us: [site](https://example.com/a?b=1&c=2)";

fn details() -> BookingDetails {
    BookingDetails {
        event_title: "Intro".to_string(),
        date: "2026-10-27".to_string(),
        start_time: "10:00".to_string(),
        end_time: "10:30".to_string(),
        guest_name: "Gina".to_string(),
        guest_email: "gina@example.com".to_string(),
        guest_timezone: "Europe/Berlin".to_string(),
        host_name: "Alice".to_string(),
        host_email: "alice@example.com".to_string(),
        uid: "uid-1".to_string(),
        guest_language: Some("fr".to_string()),
        host_language: Some("en".to_string()),
        host_timezone: "Europe/Berlin".to_string(),
        additional_attendees: vec!["bob@example.com".to_string()],
        guest_footer: Some(FOOTER.to_string()),
        ..Default::default()
    }
}

fn cancellation() -> CancellationDetails {
    CancellationDetails {
        event_title: "Intro".to_string(),
        date: "2026-10-27".to_string(),
        start_time: "10:00".to_string(),
        end_time: "10:30".to_string(),
        guest_name: "Gina".to_string(),
        guest_email: "gina@example.com".to_string(),
        guest_timezone: "Europe/Berlin".to_string(),
        host_name: "Alice".to_string(),
        host_email: "alice@example.com".to_string(),
        uid: "uid-1".to_string(),
        cancelled_by_host: true,
        invite_sent: true,
        guest_language: Some("fr".to_string()),
        host_language: Some("en".to_string()),
        guest_footer: Some(FOOTER.to_string()),
        ..Default::default()
    }
}

fn reschedule() -> RescheduleDetails {
    RescheduleDetails {
        event_title: "Intro".to_string(),
        old_date: "2026-10-26".to_string(),
        old_start_time: "09:00".to_string(),
        old_end_time: "09:30".to_string(),
        new_date: "2026-10-27".to_string(),
        new_start_time: "10:00".to_string(),
        new_end_time: "10:30".to_string(),
        guest_name: "Gina".to_string(),
        guest_email: "gina@example.com".to_string(),
        guest_timezone: "Europe/Berlin".to_string(),
        host_name: "Alice".to_string(),
        host_email: "alice@example.com".to_string(),
        uid: "uid-1".to_string(),
        guest_language: Some("fr".to_string()),
        host_language: Some("en".to_string()),
        guest_footer: Some(FOOTER.to_string()),
        ..Default::default()
    }
}

/// Every guest email a `compose_*` function builds, with and without the
/// footer set, in two languages.
fn guest_composers(footer: bool) -> Vec<(&'static str, ComposedEmail)> {
    let mut d = details();
    let mut c = cancellation();
    let mut r = reschedule();
    if !footer {
        d.guest_footer = None;
        c.guest_footer = None;
        r.guest_footer = None;
    }
    let invite_footer = footer.then_some(FOOTER);
    vec![
        (
            "attendee_confirmation",
            compose_attendee_confirmation(&d, "fr"),
        ),
        (
            "attendee_reschedule",
            compose_attendee_reschedule(&d, Some(("2026-10-26", "09:00")), "en"),
        ),
        (
            "attendee_cancellation",
            compose_attendee_cancellation(&c, "fr"),
        ),
        (
            "pending_notice",
            compose_guest_pending_notice(&d, Some("https://x/c"), Some("https://x/r")),
        ),
        ("decline_notice", compose_guest_decline_notice(&c)),
        (
            "pending_expired",
            compose_guest_pending_expired(&c, Some("https://x/rebook")),
        ),
        (
            "invite",
            compose_invite_email(
                "fr",
                "Gina",
                "Intro",
                "Alice",
                None,
                "https://x/i",
                None,
                invite_footer,
            ),
        ),
        (
            "pick_new_time",
            compose_guest_pick_new_time(&d, "https://x/r", Some("https://x/c")),
        ),
        (
            "reschedule_notification",
            compose_guest_reschedule_notification(&r, Some("https://x/c"), Some("https://x/r")),
        ),
        (
            "reschedule_approved",
            compose_guest_reschedule_approved(
                &d,
                Some("https://x/c"),
                Some("https://x/r"),
                None,
                None,
            ),
        ),
    ]
}

#[test]
fn every_composed_guest_email_carries_the_footer_in_both_bodies() {
    for (name, mail) in guest_composers(true) {
        assert!(
            mail.plain
                .contains("Questions? Call us: site (https://example.com/a?b=1&c=2)"),
            "{name} plain: {}",
            mail.plain
        );
        assert!(
            mail.html.contains("<strong>Call</strong>"),
            "{name} html lacks bold: {}",
            mail.html
        );
        assert!(
            mail.html
                .contains("<a href=\"https://example.com/a?b=1&amp;c=2\""),
            "{name} html lacks the link: {}",
            mail.html
        );
        // Just above the sender's signature, not after it.
        let footer_at = mail.plain.find("Call us").unwrap();
        let signature_at = mail
            .plain
            .rfind(&t("fr", "email-signature"))
            .or_else(|| mail.plain.rfind(&t("en", "email-signature")))
            .unwrap_or_else(|| panic!("{name}: no signature: {}", mail.plain));
        assert!(footer_at < signature_at, "{name}: {}", mail.plain);
    }
}

#[test]
fn without_a_footer_the_guest_emails_are_unchanged() {
    for (name, mail) in guest_composers(false) {
        assert!(!mail.plain.contains("Call us"), "{name}: {}", mail.plain);
        assert!(!mail.html.contains("Call"), "{name}: {}", mail.html);
        assert!(
            !mail
                .html
                .contains("border-top:1px solid #e5e7eb;font-size:13px"),
            "{name} renders an empty footer block"
        );
    }
}

#[test]
fn a_blank_footer_adds_nothing() {
    let mut d = details();
    d.guest_footer = Some("  \n ".to_string());
    let with_blank = compose_attendee_confirmation(&d, "en");
    d.guest_footer = None;
    let without = compose_attendee_confirmation(&d, "en");
    assert_eq!(with_blank.plain, without.plain);
    assert_eq!(with_blank.html, without.html);
}

#[test]
fn typed_html_in_the_footer_is_escaped_in_every_guest_email() {
    let mut d = details();
    d.guest_footer = Some("<script>alert(1)</script>\n\n[x](javascript:alert(2))".to_string());
    let mail = compose_attendee_confirmation(&d, "en");
    assert!(!mail.html.contains("<script>"), "{}", mail.html);
    assert!(mail.html.contains("&lt;script&gt;"), "{}", mail.html);
    assert!(!mail.html.contains("href=\"javascript"), "{}", mail.html);
}

#[test]
fn host_emails_never_carry_the_guest_footer() {
    let d = details();
    let c = cancellation();
    let r = reschedule();
    let hosts = [
        ("notification", compose_host_notification(&d)),
        ("booking_confirmed", compose_host_booking_confirmed(&d)),
        ("reminder", compose_host_reminder(&d)),
        ("cancellation", compose_host_cancellation(&c)),
        ("pending_expired", compose_host_pending_expired(&c)),
        (
            "reschedule_request",
            compose_host_reschedule_request(&r, None, None),
        ),
        (
            "approval",
            compose_host_approval_email(&d, Some("tok"), Some("https://x"), None),
        ),
    ];
    for (name, mail) in hosts {
        assert!(!mail.plain.contains("Call us"), "{name}: {}", mail.plain);
        assert!(!mail.html.contains("Call"), "{name}: {}", mail.html);
    }
}
