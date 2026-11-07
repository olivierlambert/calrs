//! Composed guest and host emails: bodies, actions, ICS methods, per-reader language.

use super::*;

// --- RescheduleDetails tests ---

fn sample_reschedule_details() -> RescheduleDetails {
    RescheduleDetails {
        old_utc_times: None,
        event_title: "30min call".to_string(),
        old_date: "2026-03-16".to_string(),
        old_start_time: "10:00".to_string(),
        old_end_time: "10:30".to_string(),
        new_date: "2026-03-17".to_string(),
        new_start_time: "14:00".to_string(),
        new_end_time: "14:30".to_string(),
        guest_name: "Jane".to_string(),
        guest_email: "jane@example.com".to_string(),
        guest_timezone: "Europe/Paris".to_string(),
        host_name: "Alice".to_string(),
        host_email: "alice@example.com".to_string(),
        uid: "test-uid@calrs".to_string(),
        location: Some("https://meet.example.com/abc".to_string()),
        ..Default::default()
    }
}

#[test]
fn reschedule_details_generates_valid_ics_for_new_time() {
    let details = sample_reschedule_details();
    let booking_details = BookingDetails {
        event_title: details.event_title.clone(),
        date: details.new_date.clone(),
        start_time: details.new_start_time.clone(),
        end_time: details.new_end_time.clone(),
        guest_name: details.guest_name.clone(),
        guest_email: details.guest_email.clone(),
        guest_timezone: details.guest_timezone.clone(),
        host_name: details.host_name.clone(),
        host_email: details.host_email.clone(),
        uid: details.uid.clone(),
        notes: None,
        location: details.location.clone(),
        reminder_minutes: None,
        additional_attendees: vec![],
        ..Default::default()
    };
    let ics = generate_ics(&booking_details, "PUBLISH");
    assert!(
        ics.contains("UID:test-uid@calrs"),
        "ICS should contain the booking UID"
    );
    assert!(
        ics.contains("METHOD:PUBLISH"),
        "ICS should have PUBLISH method"
    );
    assert!(ics.contains("SUMMARY:"), "ICS should have a summary");
    assert!(ics.contains("LOCATION:"), "ICS should include location");
}

#[test]
fn reschedule_details_ics_uses_same_uid() {
    // This is critical: reschedule must use the same UID so CalDAV updates in place
    let details = sample_reschedule_details();
    let booking_details = BookingDetails {
        event_title: details.event_title,
        date: details.new_date,
        start_time: details.new_start_time,
        end_time: details.new_end_time,
        guest_name: details.guest_name,
        guest_email: details.guest_email,
        guest_timezone: details.guest_timezone,
        host_name: details.host_name,
        host_email: details.host_email,
        uid: "original-uid@calrs".to_string(),
        notes: None,
        location: None,
        reminder_minutes: None,
        additional_attendees: vec![],
        ..Default::default()
    };
    let ics = generate_ics(&booking_details, "REQUEST");
    assert!(
        ics.contains("UID:original-uid@calrs"),
        "Rescheduled ICS must preserve the original UID for CalDAV update-in-place"
    );
}

#[test]
fn confirmation_email_actions_include_reschedule_when_provided() {
    // Test that the email action builder produces both reschedule and cancel buttons
    let mut actions: Vec<EmailAction> = Vec::new();
    let reschedule_url = Some("https://cal.example.com/booking/reschedule/abc123");
    let cancel_url = Some("https://cal.example.com/booking/cancel/def456");

    if let Some(u) = reschedule_url {
        actions.push(EmailAction {
            label: "Reschedule".to_string(),
            url: u.to_string(),
            color: "#3b82f6".to_string(),
        });
    }
    if let Some(u) = cancel_url {
        actions.push(EmailAction {
            label: "Cancel booking".to_string(),
            url: u.to_string(),
            color: "#dc2626".to_string(),
        });
    }

    assert_eq!(
        actions.len(),
        2,
        "Should have both reschedule and cancel actions"
    );
    assert_eq!(actions[0].label, "Reschedule");
    assert!(actions[0].url.contains("reschedule"));
    assert_eq!(actions[1].label, "Cancel booking");
    assert!(actions[1].url.contains("cancel"));
}

#[test]
fn confirmation_email_actions_only_cancel_when_no_reschedule() {
    let mut actions: Vec<EmailAction> = Vec::new();
    let reschedule_url: Option<&str> = None;
    let cancel_url = Some("https://cal.example.com/booking/cancel/def456");

    if let Some(u) = reschedule_url {
        actions.push(EmailAction {
            label: "Reschedule".to_string(),
            url: u.to_string(),
            color: "#3b82f6".to_string(),
        });
    }
    if let Some(u) = cancel_url {
        actions.push(EmailAction {
            label: "Cancel booking".to_string(),
            url: u.to_string(),
            color: "#dc2626".to_string(),
        });
    }

    assert_eq!(actions.len(), 1, "Should only have cancel action");
    assert_eq!(actions[0].label, "Cancel booking");
}

#[test]
fn reschedule_email_html_contains_old_and_new_times() {
    let details = sample_reschedule_details();
    let rows = vec![
        EmailRow {
            label: "Event".to_string(),
            value: details.event_title.clone(),
        },
        EmailRow {
            label: "Previous".to_string(),
            value: format!(
                "{} at {} \u{2013} {}",
                details.old_date, details.old_start_time, details.old_end_time
            ),
        },
        EmailRow {
            label: "New date".to_string(),
            value: details.new_date.clone(),
        },
        EmailRow {
            label: "New time".to_string(),
            value: format!(
                "{} \u{2013} {} ({})",
                details.new_start_time, details.new_end_time, details.guest_timezone
            ),
        },
        EmailRow {
            label: "With".to_string(),
            value: details.host_name.clone(),
        },
    ];

    let html = render_html_email_with_actions(
        &format!("Hi {},", h(&details.guest_name)),
        "Your booking has been rescheduled.",
        "#d97706",
        &rows,
        None,
        &[],
    );

    assert!(html.contains("30min call"), "Should contain event title");
    assert!(html.contains("2026-03-16"), "Should contain old date");
    assert!(html.contains("2026-03-17"), "Should contain new date");
    assert!(html.contains("10:00"), "Should contain old start time");
    assert!(html.contains("14:00"), "Should contain new start time");
    assert!(
        html.contains("#d97706"),
        "Should use orange accent for reschedule"
    );
}

#[test]
fn host_reschedule_request_email_has_approve_decline_actions() {
    let approve_url = "https://cal.example.com/booking/approve/token123";
    let decline_url = "https://cal.example.com/booking/decline/token123";

    let actions = vec![
        EmailAction {
            label: "Approve".to_string(),
            url: approve_url.to_string(),
            color: "#16a34a".to_string(),
        },
        EmailAction {
            label: "Decline".to_string(),
            url: decline_url.to_string(),
            color: "#dc2626".to_string(),
        },
    ];

    let html = render_html_email_with_actions(
        "Hi,",
        "A guest wants to reschedule.",
        "#d97706",
        &[EmailRow {
            label: "Event".to_string(),
            value: "Test".to_string(),
        }],
        None,
        &actions,
    );

    assert!(html.contains("Approve"), "Should have approve button");
    assert!(html.contains("Decline"), "Should have decline button");
    assert!(html.contains(approve_url), "Should contain approve URL");
    assert!(html.contains(decline_url), "Should contain decline URL");
}

// --- Simulated email content building (mirrors send_* functions) ---

/// Helper to create a standard BookingDetails for testing
fn sample_booking_details() -> BookingDetails {
    BookingDetails {
        event_title: "30min Intro".to_string(),
        date: "2026-04-10".to_string(),
        start_time: "14:00".to_string(),
        end_time: "14:30".to_string(),
        guest_name: "Jane Doe".to_string(),
        guest_email: "jane@example.com".to_string(),
        guest_timezone: "Europe/Paris".to_string(),
        host_name: "Alice Smith".to_string(),
        host_email: "alice@example.com".to_string(),
        uid: "booking-uid-001".to_string(),
        notes: Some("Discuss project roadmap".to_string()),
        location: Some("https://meet.example.com/room".to_string()),
        reminder_minutes: Some(15),
        additional_attendees: vec!["cc@example.com".to_string()],
        ..Default::default()
    }
}

#[test]
fn guest_confirmation_email_body_construction() {
    // Mirrors send_guest_confirmation_ex body construction
    let details = sample_booking_details();
    let cancel_url = Some("https://cal.rs/booking/cancel/tok1");
    let reschedule_url = Some("https://cal.rs/booking/reschedule/tok2");

    let time_display = format!(
        "{} \u{2013} {} ({})",
        details.start_time, details.end_time, details.guest_timezone
    );

    let mut rows = vec![
        EmailRow {
            label: "Event".to_string(),
            value: details.event_title.clone(),
        },
        EmailRow {
            label: "Date".to_string(),
            value: details.date.clone(),
        },
        EmailRow {
            label: "Time".to_string(),
            value: time_display,
        },
        EmailRow {
            label: "With".to_string(),
            value: details.host_name.clone(),
        },
    ];
    if let Some(loc) = &details.location {
        rows.push(EmailRow {
            label: "Location".to_string(),
            value: loc.clone(),
        });
    }
    if let Some(notes) = &details.notes {
        rows.push(EmailRow {
            label: "Notes".to_string(),
            value: notes.clone(),
        });
    }

    let mut actions: Vec<EmailAction> = Vec::new();
    if let Some(u) = reschedule_url {
        actions.push(EmailAction {
            label: "Reschedule".to_string(),
            url: u.to_string(),
            color: "#3b82f6".to_string(),
        });
    }
    if let Some(u) = cancel_url {
        actions.push(EmailAction {
            label: "Cancel booking".to_string(),
            url: u.to_string(),
            color: "#dc2626".to_string(),
        });
    }

    let html = render_html_email_with_actions(
        &format!("Hi {},", h(&details.guest_name)),
        "Your booking has been confirmed!",
        "#16a34a",
        &rows,
        Some("A calendar invite is attached to this email."),
        &actions,
    );

    assert!(html.contains("Hi Jane Doe,"));
    assert!(html.contains("Your booking has been confirmed!"));
    assert!(html.contains("30min Intro"));
    assert!(html.contains("2026-04-10"));
    assert!(html.contains("14:00"));
    assert!(html.contains("Alice Smith"));
    assert!(html.contains("https://meet.example.com/room"));
    assert!(html.contains("Discuss project roadmap"));
    assert!(html.contains("Reschedule"));
    assert!(html.contains("Cancel booking"));
    assert!(html.contains("#16a34a")); // green accent
}

#[test]
fn guest_pending_email_body_excludes_location() {
    // Mirrors send_guest_pending_notice_ex — location should NOT be included
    let details = sample_booking_details();

    let time_display = format!(
        "{} \u{2013} {} ({})",
        details.start_time, details.end_time, details.guest_timezone
    );

    let mut rows = vec![
        EmailRow {
            label: "Event".to_string(),
            value: details.event_title.clone(),
        },
        EmailRow {
            label: "Date".to_string(),
            value: details.date.clone(),
        },
        EmailRow {
            label: "Time".to_string(),
            value: time_display,
        },
        EmailRow {
            label: "Host".to_string(),
            value: details.host_name.clone(),
        },
    ];
    // Note: no location row for pending emails
    if let Some(notes) = &details.notes {
        rows.push(EmailRow {
            label: "Notes".to_string(),
            value: notes.clone(),
        });
    }

    let html = render_html_email_with_actions(
        &format!("Hi {},", h(&details.guest_name)),
        &format!(
            "Your booking request is awaiting confirmation from {}.",
            h(&details.host_name)
        ),
        "#f59e0b",
        &rows,
        Some("You\u{2019}ll receive another email once it\u{2019}s confirmed."),
        &[],
    );

    assert!(html.contains("awaiting confirmation from Alice Smith"));
    assert!(html.contains("#f59e0b")); // amber accent for pending
    assert!(!html.contains("Location")); // No location in pending emails
    assert!(html.contains("Notes"));
}

#[test]
fn host_notification_email_body_construction() {
    // Mirrors send_host_notification body
    let details = sample_booking_details();

    let time_display = format!("{} \u{2013} {}", details.start_time, details.end_time);

    let mut rows = vec![
        EmailRow {
            label: "Event".to_string(),
            value: details.event_title.clone(),
        },
        EmailRow {
            label: "Date".to_string(),
            value: details.date.clone(),
        },
        EmailRow {
            label: "Time".to_string(),
            value: time_display,
        },
        EmailRow {
            label: "Guest".to_string(),
            value: format!("{} <{}>", details.guest_name, details.guest_email),
        },
    ];
    if let Some(loc) = &details.location {
        rows.push(EmailRow {
            label: "Location".to_string(),
            value: loc.clone(),
        });
    }
    if let Some(notes) = &details.notes {
        rows.push(EmailRow {
            label: "Notes".to_string(),
            value: notes.clone(),
        });
    }

    let html = render_html_email(
        "New booking!",
        &format!("{} booked a slot with you.", h(&details.guest_name)),
        "#16a34a",
        &rows,
        Some("A calendar invite is attached to this email."),
    );

    assert!(html.contains("New booking!"));
    assert!(html.contains("Jane Doe booked a slot with you."));
    assert!(html.contains("Jane Doe &lt;jane@example.com&gt;"));
    assert!(html.contains("Location"));
}

#[test]
fn host_approval_request_email_with_token() {
    // Mirrors send_host_approval_request body construction
    let details = sample_booking_details();
    let confirm_token = Some("abc-token-123");
    let base_url = Some("https://cal.example.com");

    let (approve_url, decline_url) = match (confirm_token, base_url) {
        (Some(token), Some(url)) => (
            Some(format!(
                "{}/booking/approve/{}",
                url.trim_end_matches('/'),
                token
            )),
            Some(format!(
                "{}/booking/decline/{}",
                url.trim_end_matches('/'),
                token
            )),
        ),
        _ => (None, None),
    };

    assert_eq!(
        approve_url.as_deref(),
        Some("https://cal.example.com/booking/approve/abc-token-123")
    );
    assert_eq!(
        decline_url.as_deref(),
        Some("https://cal.example.com/booking/decline/abc-token-123")
    );

    let actions: Vec<EmailAction> = match (approve_url, decline_url) {
        (Some(a), Some(d)) => vec![
            EmailAction {
                label: "Approve".to_string(),
                url: a,
                color: "#16a34a".to_string(),
            },
            EmailAction {
                label: "Decline".to_string(),
                url: d,
                color: "#dc2626".to_string(),
            },
        ],
        _ => vec![],
    };

    let html = render_html_email_with_actions(
        "Action required",
        &format!("{} wants to book a slot with you.", h(&details.guest_name)),
        "#f59e0b",
        &[EmailRow {
            label: "Guest".to_string(),
            value: format!("{} <{}>", details.guest_name, details.guest_email),
        }],
        Some("You can also manage this from your dashboard."),
        &actions,
    );

    assert!(html.contains("Action required"));
    assert!(html.contains("Jane Doe wants to book a slot with you."));
    assert!(html.contains("Approve"));
    assert!(html.contains("Decline"));
    assert!(html.contains("booking/approve/abc-token-123"));
    assert!(html.contains("booking/decline/abc-token-123"));
}

#[test]
fn host_approval_request_without_token_no_actions() {
    let confirm_token: Option<&str> = None;
    let base_url = Some("https://cal.example.com");

    let (approve_url, decline_url) = match (confirm_token, base_url) {
        (Some(token), Some(url)) => (
            Some(format!(
                "{}/booking/approve/{}",
                url.trim_end_matches('/'),
                token
            )),
            Some(format!(
                "{}/booking/decline/{}",
                url.trim_end_matches('/'),
                token
            )),
        ),
        _ => (None, None),
    };

    let actions: Vec<EmailAction> = match (approve_url, decline_url) {
        (Some(a), Some(d)) => vec![
            EmailAction {
                label: "Approve".to_string(),
                url: a,
                color: "#16a34a".to_string(),
            },
            EmailAction {
                label: "Decline".to_string(),
                url: d,
                color: "#dc2626".to_string(),
            },
        ],
        _ => vec![],
    };

    assert!(actions.is_empty(), "No actions when token is missing");
}

#[test]
fn guest_decline_email_body_with_reason() {
    let details = CancellationDetails {
        event_title: "Intro Call".to_string(),
        date: "2026-04-10".to_string(),
        start_time: "14:00".to_string(),
        end_time: "14:30".to_string(),
        guest_name: "Jane".to_string(),
        guest_email: "jane@example.com".to_string(),
        guest_timezone: "UTC".to_string(),
        host_name: "Alice".to_string(),
        host_email: "alice@example.com".to_string(),
        uid: "uid-decline".to_string(),
        reason: Some("Schedule conflict".to_string()),
        cancelled_by_host: true, // decline is host-initiated
        ..Default::default()
    };

    let mut rows = vec![
        EmailRow {
            label: "Event".to_string(),
            value: details.event_title.clone(),
        },
        EmailRow {
            label: "Date".to_string(),
            value: details.date.clone(),
        },
        EmailRow {
            label: "With".to_string(),
            value: details.host_name.clone(),
        },
    ];
    if let Some(reason) = &details.reason {
        rows.push(EmailRow {
            label: "Reason".to_string(),
            value: reason.clone(),
        });
    }

    let html = render_html_email(
        &format!("Hi {},", h(&details.guest_name)),
        "Your booking request has been declined.",
        "#dc2626",
        &rows,
        None,
    );

    assert!(html.contains("Hi Jane,"));
    assert!(html.contains("declined"));
    assert!(html.contains("Schedule conflict"));
    assert!(html.contains("#dc2626")); // red accent
}

#[test]
fn guest_decline_email_body_without_reason() {
    let details = CancellationDetails {
        event_title: "Meeting".to_string(),
        date: "2026-04-10".to_string(),
        start_time: "09:00".to_string(),
        end_time: "10:00".to_string(),
        guest_name: "Bob".to_string(),
        guest_email: "bob@test.com".to_string(),
        guest_timezone: "UTC".to_string(),
        host_name: "Alice".to_string(),
        host_email: "alice@test.com".to_string(),
        uid: "uid-decline-noreason".to_string(),
        reason: None,
        cancelled_by_host: true,
        ..Default::default()
    };

    let mut rows = vec![EmailRow {
        label: "Event".to_string(),
        value: details.event_title.clone(),
    }];
    if let Some(reason) = &details.reason {
        rows.push(EmailRow {
            label: "Reason".to_string(),
            value: reason.clone(),
        });
    }

    let html = render_html_email("Hi,", "Declined.", "#dc2626", &rows, None);
    assert!(!html.contains("Reason")); // No reason row
}

#[test]
fn invite_email_body_construction() {
    // Mirrors send_invite_email body construction
    let guest_name = "Jane";
    let host_name = "Alice";
    let event_title = "Private Consultation";
    let message = Some("Looking forward to chatting!");
    let invite_url = "https://cal.rs/u/alice/consult?invite=TOKEN123";
    let expires_at = Some("2026-04-20");

    let mut rows = vec![
        EmailRow {
            label: "Event".to_string(),
            value: event_title.to_string(),
        },
        EmailRow {
            label: "Invited by".to_string(),
            value: host_name.to_string(),
        },
    ];
    if let Some(msg) = message.filter(|m| !m.trim().is_empty()) {
        rows.push(EmailRow {
            label: "Message".to_string(),
            value: msg.to_string(),
        });
    }
    if let Some(exp) = expires_at {
        rows.push(EmailRow {
            label: "Expires".to_string(),
            value: exp.to_string(),
        });
    }

    let actions = vec![EmailAction {
        label: "Choose a time".to_string(),
        url: invite_url.to_string(),
        color: "#6366f1".to_string(),
    }];

    let html = render_html_email_with_actions(
        &format!("Hi {},", h(guest_name)),
        &format!(
            "{} has invited you to book: {}",
            h(host_name),
            h(event_title)
        ),
        "#6366f1",
        &rows,
        None,
        &actions,
    );

    assert!(html.contains("Hi Jane,"));
    assert!(html.contains("Alice has invited you to book: Private Consultation"));
    assert!(html.contains("Looking forward to chatting!"));
    assert!(html.contains("2026-04-20"));
    assert!(html.contains("Choose a time"));
    assert!(html.contains(invite_url));
    assert!(html.contains("#6366f1")); // indigo accent
}

#[test]
fn invite_email_body_no_message_no_expiry() {
    let message: Option<&str> = None;
    let expires_at: Option<&str> = None;

    let mut rows = vec![
        EmailRow {
            label: "Event".to_string(),
            value: "Meeting".to_string(),
        },
        EmailRow {
            label: "Invited by".to_string(),
            value: "Host".to_string(),
        },
    ];
    if let Some(msg) = message.filter(|m| !m.trim().is_empty()) {
        rows.push(EmailRow {
            label: "Message".to_string(),
            value: msg.to_string(),
        });
    }
    if let Some(exp) = expires_at {
        rows.push(EmailRow {
            label: "Expires".to_string(),
            value: exp.to_string(),
        });
    }

    assert_eq!(rows.len(), 2); // Only Event and Invited by
}

#[test]
fn invite_email_empty_message_excluded() {
    let message: Option<&str> = Some("   ");

    let mut rows = Vec::new();
    if let Some(msg) = message.filter(|m| !m.trim().is_empty()) {
        rows.push(EmailRow {
            label: "Message".to_string(),
            value: msg.to_string(),
        });
    }

    assert!(
        rows.is_empty(),
        "Whitespace-only message should be excluded"
    );
}

#[test]
fn guest_reminder_email_body_with_cancel_url() {
    // Mirrors send_guest_reminder body construction
    let details = sample_booking_details();
    let cancel_url = Some("https://cal.rs/booking/cancel/rem-tok");

    let actions: Vec<EmailAction> = cancel_url
        .map(|u| {
            vec![EmailAction {
                label: "Cancel booking".to_string(),
                url: u.to_string(),
                color: "#dc2626".to_string(),
            }]
        })
        .unwrap_or_default();

    let html = render_html_email_with_actions(
        &format!("Hi {},", h(&details.guest_name)),
        "Reminder: you have an upcoming booking.",
        "#3b82f6",
        &[
            EmailRow {
                label: "Event".to_string(),
                value: details.event_title.clone(),
            },
            EmailRow {
                label: "Date".to_string(),
                value: details.date.clone(),
            },
        ],
        None,
        &actions,
    );

    assert!(html.contains("Reminder"));
    assert!(html.contains("#3b82f6")); // blue accent for reminders
    assert!(html.contains("Cancel booking"));
    assert!(html.contains("rem-tok"));
}

#[test]
fn guest_reminder_email_body_without_cancel_url() {
    let cancel_url: Option<&str> = None;

    let actions: Vec<EmailAction> = cancel_url
        .map(|u| {
            vec![EmailAction {
                label: "Cancel booking".to_string(),
                url: u.to_string(),
                color: "#dc2626".to_string(),
            }]
        })
        .unwrap_or_default();

    assert!(actions.is_empty());

    let html = render_html_email_with_actions("Hi,", "Reminder.", "#3b82f6", &[], None, &actions);
    assert!(!html.contains("Cancel booking"));
}

#[test]
fn host_reschedule_request_url_construction() {
    // Mirrors the URL construction in send_host_reschedule_request
    let confirm_token = Some("resched-token-xyz");
    let base_url = Some("https://cal.example.com/");

    let (approve_url, decline_url) = match (confirm_token, base_url) {
        (Some(token), Some(url)) => (
            Some(format!(
                "{}/booking/approve/{}",
                url.trim_end_matches('/'),
                token
            )),
            Some(format!(
                "{}/booking/decline/{}",
                url.trim_end_matches('/'),
                token
            )),
        ),
        _ => (None, None),
    };

    // Trailing slash should be stripped
    assert_eq!(
        approve_url.as_deref(),
        Some("https://cal.example.com/booking/approve/resched-token-xyz")
    );
    assert_eq!(
        decline_url.as_deref(),
        Some("https://cal.example.com/booking/decline/resched-token-xyz")
    );
}

#[test]
fn guest_pick_new_time_email_body() {
    // Mirrors send_guest_pick_new_time body construction
    let details = sample_booking_details();
    let reschedule_url = "https://cal.rs/booking/reschedule/tok";
    let cancel_url = Some("https://cal.rs/booking/cancel/tok");

    let time_display = format!(
        "{} \u{2013} {} ({})",
        details.start_time, details.end_time, details.guest_timezone
    );

    let rows = vec![
        EmailRow {
            label: "Event".to_string(),
            value: details.event_title.clone(),
        },
        EmailRow {
            label: "Originally".to_string(),
            value: format!("{} at {}", details.date, time_display),
        },
        EmailRow {
            label: "Host".to_string(),
            value: details.host_name.clone(),
        },
    ];

    let mut actions = vec![EmailAction {
        label: "Pick a new time".to_string(),
        url: reschedule_url.to_string(),
        color: "#d97706".to_string(),
    }];
    if let Some(u) = cancel_url {
        actions.push(EmailAction {
            label: "Cancel booking".to_string(),
            url: u.to_string(),
            color: "#dc2626".to_string(),
        });
    }

    let html = render_html_email_with_actions(
        &format!("Hi {},", h(&details.guest_name)),
        &format!(
            "{} needs to reschedule your booking. Please pick a new time.",
            h(&details.host_name)
        ),
        "#d97706",
        &rows,
        None,
        &actions,
    );

    assert!(html.contains("Pick a new time"));
    assert!(html.contains("Cancel booking"));
    assert!(html.contains("needs to reschedule"));
    assert!(html.contains("#d97706")); // orange accent
    assert!(html.contains("Originally"));
}

#[test]
fn host_booking_confirmed_email_body() {
    // Mirrors send_host_booking_confirmed body construction
    let details = sample_booking_details();

    let time_display = format!("{} \u{2013} {}", details.start_time, details.end_time);

    let mut rows = vec![
        EmailRow {
            label: "Event".to_string(),
            value: details.event_title.clone(),
        },
        EmailRow {
            label: "Date".to_string(),
            value: details.date.clone(),
        },
        EmailRow {
            label: "Time".to_string(),
            value: time_display,
        },
        EmailRow {
            label: "Guest".to_string(),
            value: format!("{} <{}>", details.guest_name, details.guest_email),
        },
    ];
    if let Some(loc) = &details.location {
        rows.push(EmailRow {
            label: "Location".to_string(),
            value: loc.clone(),
        });
    }

    let html = render_html_email(
        "Booking confirmed",
        &format!("You approved the booking with {}.", h(&details.guest_name)),
        "#16a34a",
        &rows,
        Some("The event has been added to your calendar."),
    );

    assert!(html.contains("Booking confirmed"));
    assert!(html.contains("You approved the booking with Jane Doe."));
    assert!(html.contains("The event has been added to your calendar."));
    assert!(html.contains("Location"));
}

// --- ICS generation for reschedule ---

#[test]
fn reschedule_ics_uses_new_date_and_time() {
    let details = sample_reschedule_details();
    let booking_details = BookingDetails {
        event_title: details.event_title.clone(),
        date: details.new_date.clone(),
        start_time: details.new_start_time.clone(),
        end_time: details.new_end_time.clone(),
        guest_name: details.guest_name.clone(),
        guest_email: details.guest_email.clone(),
        guest_timezone: details.guest_timezone.clone(),
        host_name: details.host_name.clone(),
        host_email: details.host_email.clone(),
        uid: details.uid.clone(),
        notes: None,
        location: details.location.clone(),
        reminder_minutes: None,
        additional_attendees: vec![],
        ..Default::default()
    };
    let ics = generate_ics(&booking_details, "PUBLISH");
    // March 17, 2026 in Paris (CET, UTC+1): 14:00 Paris = 13:00 UTC
    assert!(ics.contains("DTSTART:20260317T130000Z"));
    assert!(ics.contains("DTEND:20260317T133000Z"));
    // Should NOT contain old dates
    assert!(!ics.contains("20260316"));
}

#[test]
fn reschedule_details_without_location() {
    let mut details = sample_reschedule_details();
    details.location = None;
    let booking_details = BookingDetails {
        event_title: details.event_title,
        date: details.new_date,
        start_time: details.new_start_time,
        end_time: details.new_end_time,
        guest_name: details.guest_name,
        guest_email: details.guest_email,
        guest_timezone: details.guest_timezone,
        host_name: details.host_name,
        host_email: details.host_email,
        uid: details.uid,
        notes: None,
        location: None,
        reminder_minutes: None,
        additional_attendees: vec![],
        ..Default::default()
    };
    let ics = generate_ics(&booking_details, "PUBLISH");
    assert!(!ics.contains("LOCATION:"));
}

// --- Cancellation email body tests ---

#[test]
fn guest_cancellation_email_host_initiated_with_reason() {
    let details = CancellationDetails {
        event_title: "Intro Call".to_string(),
        date: "2026-04-10".to_string(),
        start_time: "14:00".to_string(),
        end_time: "14:30".to_string(),
        guest_name: "Jane".to_string(),
        guest_email: "jane@example.com".to_string(),
        guest_timezone: "UTC".to_string(),
        host_name: "Alice".to_string(),
        host_email: "alice@example.com".to_string(),
        uid: "uid-gcancel".to_string(),
        reason: Some("Emergency".to_string()),
        cancelled_by_host: true,
        ..Default::default()
    };

    let mut rows = vec![
        EmailRow {
            label: "Event".to_string(),
            value: details.event_title.clone(),
        },
        EmailRow {
            label: "Date".to_string(),
            value: details.date.clone(),
        },
        EmailRow {
            label: "With".to_string(),
            value: details.host_name.clone(),
        },
    ];
    if let Some(reason) = &details.reason {
        rows.push(EmailRow {
            label: "Reason".to_string(),
            value: reason.clone(),
        });
    }

    let msg = if details.cancelled_by_host {
        format!(
            "Your booking has been cancelled by {}.",
            h(&details.host_name)
        )
    } else {
        "Your booking has been cancelled.".to_string()
    };

    let html = render_html_email(
        &format!("Hi {},", h(&details.guest_name)),
        &msg,
        "#dc2626",
        &rows,
        Some("A calendar cancellation is attached to this email."),
    );

    assert!(html.contains("cancelled by Alice"));
    assert!(html.contains("Emergency"));
}

#[test]
fn host_cancellation_email_guest_initiated() {
    let details = CancellationDetails {
        event_title: "Meeting".to_string(),
        date: "2026-04-10".to_string(),
        start_time: "09:00".to_string(),
        end_time: "10:00".to_string(),
        guest_name: "Bob".to_string(),
        guest_email: "bob@test.com".to_string(),
        guest_timezone: "UTC".to_string(),
        host_name: "Alice".to_string(),
        host_email: "alice@test.com".to_string(),
        uid: "uid-hcancel".to_string(),
        reason: Some("Double booked".to_string()),
        cancelled_by_host: false,
        ..Default::default()
    };

    let msg = if details.cancelled_by_host {
        "You cancelled this booking.".to_string()
    } else {
        format!("{} cancelled their booking.", h(&details.guest_name))
    };

    let html = render_html_email("Booking cancelled.", &msg, "#dc2626", &[], None);

    assert!(html.contains("Bob cancelled their booking."));
}

// --- Test email body ICS attachment generation ---

#[test]
fn guest_confirmation_ics_has_publish_method() {
    let details = sample_booking_details();
    let ics = generate_ics(&details, "PUBLISH");
    assert!(ics.contains("METHOD:PUBLISH"));
}

#[test]
fn host_notification_ics_has_request_method() {
    let details = sample_booking_details();
    let ics = generate_ics(&details, "REQUEST");
    assert!(ics.contains("METHOD:REQUEST"));
}

#[test]
fn cancellation_ics_has_cancel_method_and_status() {
    let details = CancellationDetails {
        event_title: "Meeting".to_string(),
        date: "2026-04-10".to_string(),
        start_time: "14:00".to_string(),
        end_time: "14:30".to_string(),
        guest_name: "Jane".to_string(),
        guest_email: "jane@example.com".to_string(),
        guest_timezone: "Europe/Paris".to_string(),
        host_name: "Alice".to_string(),
        host_email: "alice@example.com".to_string(),
        uid: "uid-cancel-method".to_string(),
        reason: None,
        cancelled_by_host: true,
        ..Default::default()
    };
    let ics = generate_cancel_ics(&details);
    assert!(ics.contains("METHOD:CANCEL"));
    assert!(ics.contains("STATUS:CANCELLED"));
    // Confirm ICS does NOT have STATUS:CONFIRMED
    assert!(!ics.contains("STATUS:CONFIRMED"));
}

// --- #209: guest pending-flow emails follow the booking's language ---

fn cancellation_from(details: &BookingDetails) -> CancellationDetails {
    CancellationDetails {
        event_title: details.event_title.clone(),
        date: details.date.clone(),
        start_time: details.start_time.clone(),
        end_time: details.end_time.clone(),
        guest_name: details.guest_name.clone(),
        guest_email: details.guest_email.clone(),
        guest_timezone: details.guest_timezone.clone(),
        host_name: details.host_name.clone(),
        host_email: details.host_email.clone(),
        reason: Some("Conflit".to_string()),
        cancelled_by_host: true,
        guest_language: details.guest_language.clone(),
        ..Default::default()
    }
}

#[test]
fn pending_notice_is_written_in_the_guest_language() {
    let details = BookingDetails {
        guest_language: Some("fr".to_string()),
        ..sample_booking_details()
    };
    let mail = compose_guest_pending_notice(
        &details,
        Some("https://cal.rs/booking/cancel/tok1"),
        Some("https://cal.rs/booking/reschedule/tok2"),
    );

    assert_eq!(
        mail.subject,
        "[En attente de confirmation] 30min Intro \u{2014} vendredi 10 avril 2026"
    );
    assert!(
        mail.plain.starts_with("Bonjour Jane Doe,"),
        "{}",
        mail.plain
    );
    assert!(mail
        .plain
        .contains("Votre demande de réservation a été envoyée à Alice Smith"));
    assert!(mail.plain.contains("Événement : 30min Intro"));
    assert!(mail.html.contains("<html lang=\"fr\">"));
    for english in [
        "Hi Jane",
        "awaiting",
        "Pending:",
        "Cancel booking",
        "Event:",
    ] {
        assert!(
            !mail.plain.contains(english),
            "{english} leaked into: {}",
            mail.plain
        );
        assert!(
            !mail.html.contains(english),
            "{english} leaked into the HTML"
        );
    }
    // The meeting link is only revealed once the booking is confirmed.
    assert!(!mail.plain.contains("meet.example.com"));
    assert!(!mail.html.contains("meet.example.com"));
}

#[test]
fn pending_notice_without_a_stored_language_is_english() {
    let mail = compose_guest_pending_notice(&sample_booking_details(), None, None);
    assert_eq!(
        mail.subject,
        "[Pending confirmation] 30min Intro \u{2014} Friday, April 10, 2026"
    );
    assert!(mail.plain.starts_with("Hi Jane Doe,"));
    assert!(mail
        .plain
        .contains("Your booking request has been sent to Alice Smith."));
    assert!(mail.html.contains("<html lang=\"en\">"));
}

#[test]
fn pending_notice_escapes_names_in_the_html_headline() {
    let details = BookingDetails {
        host_name: "<b>Mallory</b>".to_string(),
        ..sample_booking_details()
    };
    let mail = compose_guest_pending_notice(&details, None, None);
    assert!(!mail.html.contains("<b>Mallory</b>"));
    assert!(mail.html.contains("&lt;b&gt;Mallory&lt;/b&gt;"));
}

#[test]
fn decline_notice_is_written_in_the_guest_language() {
    let details = BookingDetails {
        guest_language: Some("fr".to_string()),
        ..sample_booking_details()
    };
    let mail = compose_guest_decline_notice(&cancellation_from(&details));

    assert_eq!(
        mail.subject,
        "[Réservation refusée] 30min Intro \u{2014} vendredi 10 avril 2026"
    );
    assert!(mail.plain.starts_with("Bonjour Jane Doe,"));
    assert!(mail
        .plain
        .contains("Cette réservation a été refusée par l'hôte."));
    assert!(mail.plain.contains("Motif : Conflit"));
    assert!(mail.html.contains("<html lang=\"fr\">"));
    for english in ["Hi Jane", "has been declined", "Declined:", "Reason:"] {
        assert!(
            !mail.plain.contains(english),
            "{english} leaked into: {}",
            mail.plain
        );
        assert!(
            !mail.html.contains(english),
            "{english} leaked into the HTML"
        );
    }
}

#[test]
fn decline_notice_without_a_stored_language_is_english() {
    let mail = compose_guest_decline_notice(&cancellation_from(&sample_booking_details()));
    assert_eq!(
        mail.subject,
        "[Booking declined] 30min Intro \u{2014} Friday, April 10, 2026"
    );
    assert!(mail
        .plain
        .contains("This booking has been declined by the host."));
    assert!(mail.html.contains("<html lang=\"en\">"));
}

/// #267: a declined or expired reschedule of a confirmed booking attaches a
/// CANCEL that removes the guest's original booking, so the text says so;
/// a new request, which sent no invite, does not.
#[test]
fn withdrawn_reschedule_tells_the_guest_the_original_booking_is_cancelled() {
    let rescheduled = CancellationDetails {
        invite_sent: true,
        reschedule_pending: true,
        ..cancellation_from(&sample_booking_details())
    };
    let line = "Your original booking is cancelled as well.";
    for mail in [
        compose_guest_decline_notice(&rescheduled),
        compose_guest_pending_expired(&rescheduled, None),
    ] {
        assert!(mail.plain.contains(line), "{}", mail.plain);
        assert!(mail.html.contains(line));
        assert!(mail.plain.contains("A calendar cancellation is attached."));
    }

    let french = CancellationDetails {
        guest_language: Some("fr".to_string()),
        ..rescheduled.clone()
    };
    let mail = compose_guest_decline_notice(&french);
    assert!(mail
        .plain
        .contains("Votre réservation initiale est également annulée."));

    let request = cancellation_from(&sample_booking_details());
    for mail in [
        compose_guest_decline_notice(&request),
        compose_guest_pending_expired(&request, None),
    ] {
        assert!(!mail.plain.contains(line));
        assert!(!mail.html.contains(line));
    }
}

/// #267: the attendees' cancellation of a reschedule awaiting approval
/// leaves the requested date out, and builds its subject from a Fluent
/// pattern like every other subject, so each locale can order it.
#[test]
fn attendee_cancellation_of_a_pending_reschedule_has_a_translated_subject() {
    let details = CancellationDetails {
        invite_sent: true,
        reschedule_pending: true,
        ..cancellation_from(&sample_booking_details())
    };
    assert_eq!(
        compose_attendee_cancellation(&details, "en").subject,
        "Cancelled: 30min Intro"
    );
    assert_eq!(
        compose_attendee_cancellation(&details, "fr").subject,
        "Annulation : 30min Intro"
    );
}

#[test]
fn html_email_lang_attribute_is_escaped() {
    let html = render_html_email_in("fr\"><script>", "Hi,", "Body", "#000", &[], None, &[]);
    assert!(html.contains("<html lang=\"fr&quot;&gt;&lt;script&gt;\">"));
    // Untranslated emails keep declaring English.
    let html = render_html_email("Hi,", "Body", "#000", &[], None);
    assert!(html.contains("<html lang=\"en\">"));
}

// --- #209: host, watcher and remaining guest emails are translated ---

/// No English copy left in a translated email. Names, the event title and
/// URLs are data, so only phrases calrs writes itself are checked.
fn assert_no_english(mail: &ComposedEmail, phrases: &[&str]) {
    for english in phrases {
        assert!(
            !mail.subject.contains(english),
            "{english} leaked into the subject: {}",
            mail.subject
        );
        assert!(
            !mail.plain.contains(english),
            "{english} leaked into: {}",
            mail.plain
        );
        assert!(
            !mail.html.contains(english),
            "{english} leaked into the HTML"
        );
    }
}

fn host_in(lang: Option<&str>) -> BookingDetails {
    BookingDetails {
        host_language: lang.map(str::to_string),
        ..sample_booking_details()
    }
}

#[test]
fn host_notification_is_written_in_the_host_language() {
    let mail = compose_host_notification(&host_in(Some("de")));
    assert_eq!(
        mail.subject,
        "Neue Buchung: 30min Intro, Jane Doe (Freitag, 10. April 2026)"
    );
    assert!(mail.plain.starts_with("Neue Buchung!"), "{}", mail.plain);
    assert!(mail.plain.contains("Gast: Jane Doe <jane@example.com>"));
    assert!(mail.html.contains("<html lang=\"de\">"));
    assert!(mail.html.contains("Gesendet von"));
    assert_no_english(
        &mail,
        &[
            "New booking",
            "booked a slot",
            "Guest:",
            "Sent by",
            "attached",
        ],
    );
}

#[test]
fn host_notification_without_a_language_is_english() {
    for lang in [None, Some(""), Some("xx")] {
        let mail = compose_host_notification(&host_in(lang));
        assert_eq!(
            mail.subject,
            "New booking: 30min Intro, Jane Doe (Friday, April 10, 2026)"
        );
        assert!(mail.plain.starts_with("New booking!"));
        assert!(mail.plain.contains("Jane Doe booked a slot with you."));
        assert!(mail.html.contains("<html lang=\"en\">"));
        assert!(mail.html.contains("Sent by"));
    }
}

/// The guest's language never leaks into the host's copy: a French guest
/// booking an English-speaking host still gets the host an English email.
#[test]
fn host_email_ignores_the_guest_language() {
    let details = BookingDetails {
        guest_language: Some("fr".to_string()),
        ..host_in(None)
    };
    let mail = compose_host_notification(&details);
    assert!(mail.subject.starts_with("New booking:"));
    assert!(mail.html.contains("<html lang=\"en\">"));
}

#[test]
fn host_booking_confirmed_is_written_in_the_host_language() {
    let mail = compose_host_booking_confirmed(&host_in(Some("fr")));
    assert_eq!(
        mail.subject,
        "Confirmation : 30min Intro, Jane Doe (vendredi 10 avril 2026)"
    );
    assert!(mail.plain.starts_with("Réservation confirmée"));
    assert!(mail.html.contains("<html lang=\"fr\">"));
    assert_no_english(&mail, &["Booking confirmed", "You approved", "calendar."]);

    let mail = compose_host_booking_confirmed(&host_in(None));
    assert_eq!(
        mail.subject,
        "Confirmed: 30min Intro, Jane Doe (Friday, April 10, 2026)"
    );
    assert!(mail.html.contains("<html lang=\"en\">"));
}

#[test]
fn host_calendar_sync_failure_is_written_in_the_host_language() {
    let mail = compose_host_calendar_sync_failure(&host_in(Some("de")), "HTTP 409");
    assert!(
        mail.subject.starts_with("Aktion nötig:"),
        "{}",
        mail.subject
    );
    assert!(mail
        .plain
        .starts_with("Dein Kalender wurde nicht aktualisiert"));
    assert!(mail.plain.contains("Neues Datum: Freitag, 10. April 2026"));
    // The technical reason is shown as the API returned it.
    assert!(mail.plain.contains("HTTP 409"));
    assert!(mail.html.contains("<html lang=\"de\">"));
    assert_no_english(&mail, &["Action needed", "Your calendar", "New date"]);

    let mail = compose_host_calendar_sync_failure(&host_in(None), "HTTP 409");
    assert_eq!(
        mail.subject,
        "Action needed: calendar not updated for 30min Intro (Friday, April 10, 2026)"
    );
}

#[test]
fn host_reminder_is_written_in_the_host_language() {
    let mail = compose_host_reminder(&host_in(Some("fr")));
    assert_eq!(
        mail.subject,
        "Rappel : 30min Intro, Jane Doe (vendredi 10 avril 2026)"
    );
    assert!(mail.plain.starts_with("Réservation à venir"));
    assert!(mail.html.contains("<html lang=\"fr\">"));
    assert_no_english(&mail, &["Reminder", "Upcoming", "coming up"]);

    let mail = compose_host_reminder(&host_in(None));
    assert_eq!(
        mail.subject,
        "Reminder: 30min Intro, Jane Doe (Friday, April 10, 2026)"
    );
    assert!(mail.plain.starts_with("Upcoming booking"));
}

#[test]
fn host_cancellation_is_written_in_the_host_language() {
    let details = CancellationDetails {
        host_language: Some("fr".to_string()),
        cancelled_by_host: false,
        ..cancellation_from(&sample_booking_details())
    };
    let mail = compose_host_cancellation(&details);
    assert_eq!(
        mail.subject,
        "Annulation : 30min Intro, Jane Doe (vendredi 10 avril 2026)"
    );
    assert!(mail.plain.starts_with("Réservation annulée"));
    assert!(mail.plain.contains("Jane Doe a annulé sa réservation."));
    assert!(mail.plain.contains("Motif : Conflit"));
    assert!(mail.html.contains("<html lang=\"fr\">"));
    assert_no_english(&mail, &["Cancelled", "cancelled their", "Reason:"]);

    let mail = compose_host_cancellation(&CancellationDetails {
        host_language: None,
        cancelled_by_host: true,
        ..details
    });
    assert_eq!(
        mail.subject,
        "Cancelled: 30min Intro, Jane Doe (Friday, April 10, 2026)"
    );
    assert!(mail.plain.contains("You cancelled this booking."));
}

#[test]
fn host_approval_request_is_written_in_the_host_language() {
    let details = BookingDetails {
        guest_phone: Some("+33612345678".to_string()),
        ..host_in(Some("fr"))
    };
    let mail = compose_host_approval_email(
        &details,
        Some("tok"),
        Some("https://cal.example.com/"),
        None,
    );
    assert_eq!(
        mail.subject,
        "Action requise : 30min Intro, Jane Doe (vendredi 10 avril 2026)"
    );
    assert!(mail
        .plain
        .contains("Approuver : https://cal.example.com/booking/approve/tok"));
    assert!(mail
        .plain
        .contains("Refuser : https://cal.example.com/booking/decline/tok"));
    assert!(mail.plain.contains("Téléphone : +33612345678"));
    assert!(mail.html.contains(">Approuver</a>"));
    assert!(mail.html.contains(">Refuser</a>"));
    assert!(mail.html.contains("<html lang=\"fr\">"));
    assert_no_english(
        &mail,
        &[
            "Action required",
            "Approve",
            "Decline",
            "Phone:",
            "dashboard",
        ],
    );

    // Without a base URL the plain text points to the dashboard instead.
    let mail = compose_host_approval_email(&details, Some("tok"), None, None);
    assert!(mail
        .plain
        .contains("Connectez-vous à votre tableau de bord"));
    assert!(!mail.html.contains("booking/approve"));
}

#[test]
fn host_approval_reminder_is_written_in_the_host_language() {
    // 16:00 UTC is 18:00 in Paris (summer time).
    let deadline = chrono::TimeZone::with_ymd_and_hms(&chrono::Utc, 2026, 4, 9, 16, 0, 0).unwrap();
    let in_paris = |lang: Option<&str>| BookingDetails {
        host_timezone: "Europe/Paris".to_string(),
        ..host_in(lang)
    };
    let mail = compose_host_approval_email(
        &in_paris(Some("de")),
        Some("tok"),
        Some("https://cal.example.com"),
        Some(deadline),
    );
    assert_eq!(
        mail.subject,
        "Erinnerung: Jane Doe wartet auf deine Antwort (30min Intro, Freitag, 10. April 2026)"
    );
    assert!(
        mail.plain
            .contains("am Donnerstag, 9. April 2026 um 18:00 (Europe/Paris) automatisch abgelehnt"),
        "{}",
        mail.plain
    );
    assert!(mail
        .plain
        .contains("Läuft ab: Donnerstag, 9. April 2026 um 18:00 (Europe/Paris)"));
    assert!(!mail.plain.contains("2026-04-09"), "no ISO deadline left");
    assert!(mail.html.contains(">Genehmigen</a>"));
    assert_no_english(&mail, &["Reminder", "waiting", "Expires"]);

    let mail = compose_host_approval_email(
        &in_paris(None),
        Some("tok"),
        Some("https://cal.example.com"),
        Some(deadline),
    );
    assert!(mail
        .plain
        .contains("on Thursday, April 9, 2026 at 18:00 (Europe/Paris)."));
    assert_eq!(
        mail.subject,
        "Reminder: Jane Doe is waiting for your answer (30min Intro, Friday, April 10, 2026)"
    );
    assert!(mail
        .plain
        .contains("Approve: https://cal.example.com/booking/approve/tok"));
}

#[test]
fn host_pending_expired_is_written_in_the_host_language() {
    let details = CancellationDetails {
        host_language: Some("pl".to_string()),
        ..cancellation_from(&sample_booking_details())
    };
    let mail = compose_host_pending_expired(&details);
    assert_eq!(
        mail.subject,
        "Wygasła: 30min Intro, Jane Doe (piątek, 10 kwietnia 2026)"
    );
    assert!(mail.plain.starts_with("Prośba o rezerwację wygasła"));
    assert!(mail.html.contains("<html lang=\"pl\">"));
    assert_no_english(&mail, &["Expired", "Nobody answered", "Guest:"]);

    let mail = compose_host_pending_expired(&CancellationDetails {
        host_language: None,
        ..details
    });
    assert_eq!(
        mail.subject,
        "Expired: 30min Intro with Jane Doe (Friday, April 10, 2026)"
    );
}

#[test]
fn host_reschedule_request_is_written_in_the_host_language() {
    let details = RescheduleDetails {
        host_language: Some("de".to_string()),
        guest_language: Some("fr".to_string()),
        ..sample_reschedule_details()
    };
    let mail =
        compose_host_reschedule_request(&details, Some("tok"), Some("https://cal.example.com"));
    assert_eq!(
        mail.subject,
        "Verschiebungsanfrage: 30min call, Jane (jane@example.com)"
    );
    assert!(mail.plain.starts_with("Hallo Alice,"));
    assert!(mail
        .plain
        .contains("Bisher: Montag, 16. März 2026 um 10:00"));
    assert!(mail
        .plain
        .contains("Angefragt: Dienstag, 17. März 2026 um 14:00"));
    assert!(mail.html.contains(">Ablehnen</a>"));
    assert!(mail.html.contains("<html lang=\"de\">"));
    assert_no_english(
        &mail,
        &["Reschedule request", "Previous", "Requested", "Hi "],
    );

    let mail = compose_host_reschedule_request(
        &RescheduleDetails {
            host_language: None,
            ..details
        },
        None,
        None,
    );
    assert_eq!(
        mail.subject,
        "Reschedule request: 30min call, Jane (jane@example.com)"
    );
    assert!(mail.plain.contains("Log in to your dashboard"));
}

/// #266: the request for a new time shows what the guest typed, as the
/// original approval request does.
#[test]
fn host_reschedule_request_shows_the_guest_phone_and_notes() {
    let details = RescheduleDetails {
        notes: Some("Bring the quote".to_string()),
        guest_phone: Some("+33612345678".to_string()),
        ..sample_reschedule_details()
    };
    let mail = compose_host_reschedule_request(&details, None, None);
    assert!(mail.plain.contains("Phone: +33612345678"), "{}", mail.plain);
    assert!(mail.plain.contains("Bring the quote"), "{}", mail.plain);

    let mail = compose_host_reschedule_request(&sample_reschedule_details(), None, None);
    assert!(!mail.plain.contains("Phone:"), "{}", mail.plain);
}

#[test]
fn watcher_claim_offer_is_written_in_the_watcher_language() {
    let mail = compose_watcher_claim_notification(
        &sample_booking_details(),
        "Wanda",
        Some("fr"),
        None,
        "Alice Smith",
        "https://cal.example.com/booking/claim/b?token=t",
    );
    assert_eq!(
        mail.subject,
        "À prendre en charge : 30min Intro, Jane Doe (vendredi 10 avril 2026)"
    );
    assert!(mail.plain.starts_with("Bonjour Wanda,"));
    assert!(mail.plain.contains("Attribuée à : Alice Smith"));
    assert!(mail.plain.contains(
        "Prendre en charge cette réservation : https://cal.example.com/booking/claim/b?token=t"
    ));
    assert!(mail.html.contains("<html lang=\"fr\">"));
    assert_no_english(&mail, &["Claim", "Assigned", "Hi "]);

    let mail = compose_watcher_claim_notification(
        &sample_booking_details(),
        "Wanda",
        None,
        None,
        "Alice Smith",
        "https://cal.example.com/c",
    );
    assert_eq!(
        mail.subject,
        "Claim available: 30min Intro, Jane Doe (Friday, April 10, 2026)"
    );
}

#[test]
fn claim_confirmation_is_written_in_the_claimant_language() {
    let mail = compose_claim_confirmation(&sample_booking_details(), "Wanda", Some("de"), None);
    assert_eq!(
        mail.subject,
        "Buchung übernommen: 30min Intro, Jane Doe (Freitag, 10. April 2026)"
    );
    assert!(mail.plain.starts_with("Hallo Wanda,"));
    assert!(mail.html.contains("<html lang=\"de\">"));
    assert_no_english(&mail, &["claimed", "attendee", "Hi "]);

    let mail = compose_claim_confirmation(&sample_booking_details(), "Wanda", None, None);
    assert_eq!(
        mail.subject,
        "Booking claimed: 30min Intro, Jane Doe (Friday, April 10, 2026)"
    );
}

#[test]
fn guest_pick_new_time_is_written_in_the_booking_language() {
    let details = BookingDetails {
        guest_language: Some("fr".to_string()),
        host_language: Some("de".to_string()),
        ..sample_booking_details()
    };
    let mail = compose_guest_pick_new_time(
        &details,
        "https://cal.example.com/booking/reschedule/r",
        Some("https://cal.example.com/booking/cancel/c"),
    );
    assert_eq!(
        mail.subject,
        "Reprogrammation : 30min Intro, merci de choisir un nouveau créneau"
    );
    assert!(mail.plain.starts_with("Bonjour Jane Doe,"));
    assert!(mail
        .plain
        .contains("Alice Smith doit reprogrammer votre réservation."));
    assert!(mail
        .plain
        .contains("Initialement : vendredi 10 avril 2026 à 14:00 \u{2013} 14:30 (Europe/Paris)"));
    assert!(mail.html.contains(">Annuler la réservation</a>"));
    assert!(mail.html.contains("<html lang=\"fr\">"));
    assert_no_english(
        &mail,
        &["Reschedule", "pick a new time", "Originally", "Hi "],
    );

    let mail = compose_guest_pick_new_time(&sample_booking_details(), "https://x/r", None);
    assert_eq!(
        mail.subject,
        "Reschedule: 30min Intro, please pick a new time"
    );
}

#[test]
fn guest_reschedule_notification_is_written_in_the_booking_language() {
    let details = RescheduleDetails {
        guest_language: Some("fr".to_string()),
        host_language: Some("de".to_string()),
        ..sample_reschedule_details()
    };
    let mail = compose_guest_reschedule_notification(
        &details,
        Some("https://cal.example.com/booking/cancel/c"),
        Some("https://cal.example.com/booking/reschedule/r"),
    );
    // The neutral "event, date" subject Exchange titles the event after.
    assert_eq!(
        mail.subject,
        ta(
            "fr",
            "email-confirm-subject",
            [("event", "30min call"), ("date", "mardi 17 mars 2026")]
        )
    );
    assert!(mail.plain.starts_with("Bonjour Jane,"));
    assert!(mail
        .plain
        .contains("Votre réservation a été reprogrammée par Alice."));
    assert!(mail
        .plain
        .contains("Précédemment : lundi 16 mars 2026 à 10:00 \u{2013} 10:30"));
    assert!(mail.plain.contains("Nouvelle date : mardi 17 mars 2026"));
    assert!(mail
        .plain
        .contains("Besoin de reprogrammer ? https://cal.example.com/booking/reschedule/r"));
    assert!(mail.html.contains("<html lang=\"fr\">"));
    assert_no_english(
        &mail,
        &["rescheduled", "Previous", "New date", "Need to", "Hi "],
    );

    let mail = compose_guest_reschedule_notification(&sample_reschedule_details(), None, None);
    assert!(mail
        .plain
        .contains("Your booking has been rescheduled by Alice."));
    assert!(mail.html.contains("<html lang=\"en\">"));
}

#[test]
fn attendee_confirmation_is_written_in_the_booking_language() {
    let details = BookingDetails {
        guest_language: Some("es".to_string()),
        ..sample_booking_details()
    };
    let mail = compose_attendee_confirmation(&details, "es");
    assert!(mail.plain.starts_with("Hola,"));
    assert!(mail.plain.contains("Organizador: Alice Smith"));
    assert!(mail
        .plain
        .contains("Reservado por: Jane Doe <jane@example.com>"));
    assert!(mail.html.contains("<html lang=\"es\">"));
    assert_no_english(&mail, &["added as an attendee", "Organizer", "Booked by"]);

    let mail = compose_attendee_confirmation(&sample_booking_details(), "en");
    assert!(mail
        .plain
        .contains("You've been added as an attendee to a booking."));
}

/// #267: the attendees' copy of a reschedule, in the booking language,
/// showing no more than their confirmation did.
#[test]
fn attendee_reschedule_is_written_in_the_booking_language() {
    let details = BookingDetails {
        guest_language: Some("es".to_string()),
        guest_phone: Some("+33612345678".to_string()),
        resource_name: Some("Demo Lab 1".to_string()),
        ..sample_booking_details()
    };
    let mail =
        compose_attendee_reschedule(&details, Some(("2026-03-09", "09:00 \u{2013} 09:30")), "es");
    assert!(mail.plain.starts_with("Hola,"));
    assert!(mail
        .plain
        .contains("Una reserva en la que participas ha cambiado de horario."));
    assert!(
        mail.plain.contains("lunes, 9 de marzo de 2026 a las 09:00"),
        "{}",
        mail.plain
    );
    assert!(mail.plain.contains("Organizador: Alice Smith"));
    assert!(mail.html.contains("<html lang=\"es\">"));
    assert_no_english(&mail, &["has moved", "Organizer", "Booked by", "Previous"]);
    for host_only in ["+33612345678", "Demo Lab 1"] {
        assert!(!mail.plain.contains(host_only), "{}", mail.plain);
        assert!(!mail.html.contains(host_only), "{}", mail.html);
    }

    // At approval the old time is gone from the row: no "Previous" line.
    let mail = compose_attendee_reschedule(&sample_booking_details(), None, "en");
    assert!(mail
        .plain
        .contains("A booking you attend has moved to a new time."));
    assert!(!mail.plain.contains("Previous"), "{}", mail.plain);
}

#[test]
fn invite_email_is_written_in_the_inviter_language() {
    let mail = compose_invite_email(
        "de",
        "Gina",
        "Demo",
        "Alice",
        Some("Bis bald"),
        "https://cal.example.com/u/alice/demo?invite=t",
        Some("2026-04-17"),
    );
    assert_eq!(
        mail.subject,
        "Alice hat dich eingeladen, einen Termin zu buchen: Demo"
    );
    assert!(mail.plain.starts_with("Hallo Gina,"));
    assert!(mail.plain.contains("Eingeladen von: Alice"));
    assert!(mail.plain.contains("Nachricht: Bis bald"));
    assert!(mail.plain.contains("Läuft ab: 2026-04-17"));
    assert!(mail
        .plain
        .contains("https://cal.example.com/u/alice/demo?invite=t"));
    assert!(mail.html.contains(">Termin wählen</a>"));
    assert!(mail.html.contains("<html lang=\"de\">"));
    assert_no_english(
        &mail,
        &["invited you", "Invited by", "Choose a time", "Hi "],
    );

    let mail = compose_invite_email("en", "Gina", "Demo", "Alice", None, "https://x", None);
    assert_eq!(mail.subject, "Alice invited you to book: Demo");
}

#[test]
fn calendar_deleted_reason_follows_the_reader() {
    assert_eq!(
        calendar_deleted_reason(None),
        "The calendar event was deleted by the host."
    );
    assert_eq!(
        calendar_deleted_reason(Some("fr")),
        "L'événement a été supprimé de l'agenda de l'hôte."
    );
}

/// Every email now carries the footer in its own language, and the
/// guest emails translated before #209 pick it up too.
#[test]
fn footer_is_translated() {
    let html = render_html_email_in("nl", "Hoi,", "Body", "#000", &[], None, &[]);
    assert!(html.contains("Verzonden door"));
    assert!(!html.contains("Sent by"));
    let details = BookingDetails {
        guest_language: Some("it".to_string()),
        ..sample_booking_details()
    };
    let mail = compose_guest_pending_notice(&details, None, None);
    assert!(mail.html.contains("Inviato da"));
}

/// #261: once approved, the host's "Booking confirmed" email still carries
/// what the guest typed, as the approval request did; so do the new-booking
/// notification and the reminder.
#[test]
fn host_booking_emails_carry_the_guest_phone_and_notes() {
    let details = BookingDetails {
        guest_phone: Some("+33612345678".to_string()),
        notes: Some("Bring the quote".to_string()),
        ..host_in(None)
    };
    for mail in [
        compose_host_booking_confirmed(&details),
        compose_host_reminder(&details),
        compose_host_notification(&details),
    ] {
        assert!(mail.plain.contains("Phone: +33612345678"), "{}", mail.plain);
        assert!(mail.plain.contains("Bring the quote"), "{}", mail.plain);
        assert!(mail.html.contains("+33612345678"));
    }
}

// --- The date is written out in the reader's language, as on the booking pages ---

/// 2026-04-10, the date of `sample_booking_details`, as each reader sees it.
/// `de-formal` reads the German date through its fallback to `de`.
const APRIL_10: [(&str, &str); 4] = [
    ("de", "Freitag, 10. April 2026"),
    ("de-formal", "Freitag, 10. April 2026"),
    ("en", "Friday, April 10, 2026"),
    ("fr", "vendredi 10 avril 2026"),
];

/// The long date is in both bodies (and the subject when it names the
/// date), and the stored `YYYY-MM-DD` is nowhere a reader sees it.
fn assert_long_date(mail: &ComposedEmail, long: &str, in_subject: bool) {
    assert!(mail.plain.contains(long), "{long} not in: {}", mail.plain);
    assert!(mail.html.contains(long), "{long} not in the HTML");
    if in_subject {
        assert!(
            mail.subject.contains(long),
            "{long} not in: {}",
            mail.subject
        );
    }
    for part in [&mail.subject, &mail.plain, &mail.html] {
        assert!(!part.contains("2026-04-10"), "ISO date left in: {part}");
    }
}

#[test]
fn long_date_writes_a_stored_date_out_in_the_reader_language() {
    for (lang, long) in APRIL_10 {
        assert_eq!(long_date(lang, "2026-04-10"), long);
    }
    // An unknown locale reads English, like every other string.
    assert_eq!(long_date("xx", "2026-04-10"), "Friday, April 10, 2026");
    // Anything that is not a stored date is shown as it came.
    assert_eq!(long_date("de", "10/04/2026"), "10/04/2026");
    assert_eq!(long_date("de", ""), "");
}

#[test]
fn guest_emails_write_the_date_out_in_the_booking_language() {
    for (lang, long) in APRIL_10 {
        let details = BookingDetails {
            guest_language: Some(lang.to_string()),
            ..sample_booking_details()
        };
        let cancelled = cancellation_from(&details);
        let mails = [
            (compose_guest_pending_notice(&details, None, None), true),
            (
                compose_guest_reschedule_approved(&details, None, None, None, None),
                true,
            ),
            (
                compose_guest_pick_new_time(&details, "https://cal.example.com/r", None),
                false,
            ),
            (compose_attendee_confirmation(&details, lang), true),
            (compose_guest_decline_notice(&cancelled), true),
            (compose_guest_pending_expired(&cancelled, None), true),
            (compose_attendee_cancellation(&cancelled, lang), true),
        ];
        for (mail, in_subject) in &mails {
            assert_long_date(mail, long, *in_subject);
        }
    }
}

#[test]
fn host_emails_write_the_date_out_in_the_host_language() {
    for (lang, long) in APRIL_10 {
        let details = host_in(Some(lang));
        let cancelled = CancellationDetails {
            host_language: Some(lang.to_string()),
            cancelled_by_host: false,
            ..cancellation_from(&details)
        };
        let mails = [
            compose_host_notification(&details),
            compose_host_booking_confirmed(&details),
            compose_host_reminder(&details),
            compose_host_approval_email(&details, None, None, None),
            compose_host_calendar_sync_failure(&details, "HTTP 409"),
            compose_host_cancellation(&cancelled),
            compose_host_pending_expired(&cancelled),
            compose_watcher_claim_notification(
                &details,
                "Wes",
                Some(lang),
                None,
                "Bob",
                "https://c",
            ),
            compose_claim_confirmation(&details, "Wes", Some(lang), None),
        ];
        for mail in &mails {
            assert_long_date(mail, long, true);
        }
    }
}

/// A host in another timezone reads the date their own clock shows, and
/// in their own language: 23:30 in Paris is already the next day in Tokyo.
#[test]
fn host_long_date_follows_the_host_timezone() {
    let details = BookingDetails {
        start_time: "23:30".to_string(),
        end_time: "23:59".to_string(),
        host_timezone: "Asia/Tokyo".to_string(),
        ..host_in(Some("de"))
    };
    let mail = compose_host_notification(&details);
    assert!(
        mail.plain.contains("Datum: Samstag, 11. April 2026"),
        "{}",
        mail.plain
    );
}

/// The invite's expiry, stored as a UTC datetime, is written out in the
/// inviter's language with the zone named, not shown raw.
#[test]
fn invite_expiry_is_written_out_with_its_zone() {
    let mail = compose_invite_email(
        "fr",
        "Jane",
        "30min Intro",
        "Alice",
        None,
        "https://cal.example.com/i/tok",
        Some("2026-04-09 16:00:00"),
    );
    assert!(
        mail.plain.contains("jeudi 9 avril 2026 à 16:00 (UTC)"),
        "{}",
        mail.plain
    );
    assert!(!mail.plain.contains("2026-04-09"));
    // Anything else is shown as it came rather than dropped.
    let mail = compose_invite_email("en", "Jane", "T", "Alice", None, "u", Some("soon"));
    assert!(mail.plain.contains("soon"));
}

/// A watcher or claimant who is not the host reads the date and the time in
/// their own timezone, with the zone named: 06:00 on Saturday in Tokyo is
/// 23:00 on Friday in Paris (summer time).
#[test]
fn watcher_and_claim_emails_use_the_recipient_timezone() {
    let details = BookingDetails {
        date: "2026-04-11".to_string(),
        start_time: "06:00".to_string(),
        end_time: "06:30".to_string(),
        guest_timezone: "Asia/Tokyo".to_string(),
        host_timezone: "Asia/Tokyo".to_string(),
        utc_times: Some((
            "20260410T210000Z".to_string(),
            "20260410T213000Z".to_string(),
        )),
        ..sample_booking_details()
    };
    let offer = compose_watcher_claim_notification(
        &details,
        "Wes",
        Some("en"),
        Some("Europe/Paris"),
        "Bob",
        "https://c",
    );
    assert!(
        offer.plain.contains("Friday, April 10, 2026"),
        "{}",
        offer.plain
    );
    assert!(
        offer.plain.contains("23:00 – 23:30 (Europe/Paris)"),
        "{}",
        offer.plain
    );
    assert!(!offer.plain.contains("Saturday"));

    let claimed = compose_claim_confirmation(&details, "Wes", Some("en"), Some("Europe/Paris"));
    assert!(
        claimed.subject.contains("Friday, April 10, 2026"),
        "{}",
        claimed.subject
    );
    assert!(claimed.plain.contains("23:00 – 23:30 (Europe/Paris)"));

    // No usable timezone of their own: the host's, still named.
    let offer =
        compose_watcher_claim_notification(&details, "Wes", Some("en"), Some(""), "Bob", "u");
    assert!(
        offer.plain.contains("Saturday, April 11, 2026"),
        "{}",
        offer.plain
    );
    assert!(
        offer.plain.contains("06:00 – 06:30 (Asia/Tokyo)"),
        "{}",
        offer.plain
    );
}

// --- Lead capture: the abandoned-booking alert ---

fn lead_alert(language: Option<&'static str>) -> LeadAlert<'static> {
    LeadAlert {
        host_name: "Alice",
        host_email: "alice@example.com",
        host_language: language,
        event_title: "30min Intro",
        guest_name: Some("<b>Jane</b>"),
        guest_email: "jane@example.com",
        guest_phone: Some("+33612345678"),
        leads_url: Some("https://cal.example.com/dashboard/leads"),
    }
}

#[test]
fn lead_alert_is_written_in_the_host_language() {
    let mail = compose_lead_abandoned(&lead_alert(Some("fr")));
    assert_eq!(mail.subject, "Réservation abandonnée : 30min Intro");
    assert!(mail.html.contains("<html lang=\"fr\">"));
    assert!(mail.plain.contains("jane@example.com"));
    assert!(mail.plain.contains("+33612345678"));
    assert!(mail
        .plain
        .contains("https://cal.example.com/dashboard/leads"));
    assert_no_english(&mail, &["Abandoned booking", "View leads", "Name:"]);
}

#[test]
fn lead_alert_without_a_language_is_english() {
    for lang in [None, Some(""), Some("xx")] {
        let mail = compose_lead_abandoned(&lead_alert(lang));
        assert_eq!(mail.subject, "Abandoned booking: 30min Intro");
        assert!(mail.html.contains("View leads"));
    }
}

/// What the guest typed is data: escaped in the HTML, absent from the subject.
#[test]
fn lead_alert_escapes_what_the_guest_typed() {
    let mail = compose_lead_abandoned(&lead_alert(None));
    assert!(
        mail.html.contains("&lt;b&gt;Jane&lt;/b&gt;"),
        "{}",
        mail.html
    );
    assert!(!mail.html.contains("<b>Jane</b>"));
    assert!(!mail.subject.contains("Jane"));
}

#[test]
fn lead_alert_without_a_link_or_phone_omits_them() {
    let mail = compose_lead_abandoned(&LeadAlert {
        leads_url: None,
        guest_phone: None,
        guest_name: None,
        ..lead_alert(None)
    });
    assert!(!mail.html.contains("View leads"));
    assert!(!mail.plain.contains("Phone"));
    assert!(!mail.plain.contains("Name:"));
}

fn digest_items(n: usize) -> Vec<DigestItem<'static>> {
    (0..n)
        .map(|_| DigestItem {
            event_title: "30min Intro",
            guest_name: Some("<b>Jane</b>"),
            guest_email: "jane@example.com",
            guest_phone: Some("+33612345678"),
        })
        .collect()
}

#[test]
fn lead_digest_is_localised_counts_and_escapes() {
    let digest = LeadDigest {
        host_name: "Alice",
        host_email: "alice@example.com",
        host_language: Some("fr"),
        items: digest_items(2),
        total: 2,
        leads_url: Some("https://cal.example.com/dashboard/leads"),
    };
    let mail = compose_lead_digest(&digest);
    assert_eq!(mail.subject, "Réservations abandonnées : 2");
    assert!(mail.html.contains("&lt;b&gt;Jane&lt;/b&gt;"));
    assert!(mail.plain.contains("jane@example.com"));
    assert!(mail
        .plain
        .contains("https://cal.example.com/dashboard/leads"));
    assert_no_english(&mail, &["Abandoned bookings", "Others on"]);
}

#[test]
fn lead_digest_past_the_cap_says_how_many_more() {
    let digest = LeadDigest {
        host_name: "Alice",
        host_email: "alice@example.com",
        host_language: None,
        items: digest_items(DIGEST_MAX_ITEMS),
        total: DIGEST_MAX_ITEMS + 7,
        leads_url: None,
    };
    let mail = compose_lead_digest(&digest);
    assert_eq!(
        mail.subject,
        format!("Abandoned bookings: {}", DIGEST_MAX_ITEMS + 7)
    );
    assert!(
        mail.plain.contains("Others on the leads page: 7"),
        "{}",
        mail.plain
    );
    assert_eq!(
        mail.plain.matches("jane@example.com").count(),
        DIGEST_MAX_ITEMS
    );
}
