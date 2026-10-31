//! Emails sent to guests: confirmation, pending, decline, cancellation, reminder, reschedule, invite.

use super::*;

// --- Email senders ---

/// Send booking confirmation to the guest
pub async fn send_guest_confirmation(
    config: &SmtpConfig,
    details: &BookingDetails,
    cancel_url: Option<&str>,
) -> Result<()> {
    send_guest_confirmation_ex(config, details, cancel_url, None, None, None).await
}

pub async fn send_guest_confirmation_ex(
    config: &SmtpConfig,
    details: &BookingDetails,
    cancel_url: Option<&str>,
    reschedule_url: Option<&str>,
    cancel_notice_min: Option<i32>,
    reschedule_notice_min: Option<i32>,
) -> Result<()> {
    let ics = generate_ics(details, "REQUEST");
    let lang = guest_lang(details);

    let to = named_mailbox(&details.guest_name, &details.guest_email)?;

    let time_display = format!(
        "{} \u{2013} {} ({})",
        details.start_time, details.end_time, details.guest_timezone
    );

    let greeting = ta(
        lang,
        "email-confirm-greeting",
        [("name", &details.guest_name)],
    );
    let headline = t(lang, "email-confirm-headline");
    let label_event = t(lang, "confirmed-detail-event");
    let label_date = t(lang, "confirmed-detail-date");
    let date = long_date(lang, &details.date);
    let label_time = t(lang, "confirmed-detail-time");
    let label_with = t(lang, "confirmed-detail-with");
    let label_location = t(lang, "confirmed-detail-location");
    let label_notes = t(lang, "confirmed-detail-notes");
    let ics_attached_plain = t(lang, "email-confirm-ics-attached-plain");
    let ics_attached_html = t(lang, "email-confirm-ics-attached-html");
    let signature = t(lang, "email-signature");

    // Notice-window policy lines, mentioned just before the cancel link in
    // both the plain and HTML bodies. Translated to the guest's language.
    let cancel_notice_line = cancel_notice_min.filter(|m| *m > 0).map(|m| {
        ta(
            lang,
            "email-confirm-cancel-notice",
            [("minutes", m.to_string().as_str())],
        )
    });
    let reschedule_notice_line = reschedule_notice_min.filter(|m| *m > 0).map(|m| {
        ta(
            lang,
            "email-confirm-reschedule-notice",
            [("minutes", m.to_string().as_str())],
        )
    });

    let plain = format!(
        "{}\n\n\
         {}\n\n\
         {} {}\n\
         {} {}\n\
         {} {}\n\
         {} {}\n\
         {}{}\
         {}\n\
         {}{}{}\
         {}",
        greeting,
        headline,
        label_event,
        details.event_title,
        label_date,
        date,
        label_time,
        time_display,
        label_with,
        details.host_name,
        details
            .location
            .as_ref()
            .map(|l| format!("{} {}\n", label_location, l))
            .unwrap_or_default(),
        details
            .notes
            .as_ref()
            .map(|n| format!("{} {}\n", label_notes, n))
            .unwrap_or_default(),
        ics_attached_plain,
        reschedule_notice_line
            .as_ref()
            .map(|l| format!("\n{}\n", l))
            .unwrap_or_default(),
        cancel_notice_line
            .as_ref()
            .map(|l| format!("\n{}\n", l))
            .unwrap_or_default(),
        cancel_url
            .map(|u| format!(
                "\n{}\n",
                ta(lang, "email-confirm-need-to-cancel", [("url", u)])
            ))
            .unwrap_or_default(),
        signature,
    );

    let mut rows = vec![
        EmailRow {
            label: label_event.clone(),
            value: details.event_title.clone(),
        },
        EmailRow {
            label: label_date.clone(),
            value: date.clone(),
        },
        EmailRow {
            label: label_time.clone(),
            value: time_display,
        },
        EmailRow {
            label: label_with.clone(),
            value: details.host_name.clone(),
        },
    ];
    if let Some(loc) = &details.location {
        rows.push(EmailRow {
            label: label_location.clone(),
            value: loc.clone(),
        });
    }
    if let Some(notes) = &details.notes {
        rows.push(EmailRow {
            label: label_notes.clone(),
            value: notes.clone(),
        });
    }

    let mut actions: Vec<EmailAction> = Vec::new();
    if let Some(u) = reschedule_url {
        actions.push(EmailAction {
            label: t(lang, "email-action-reschedule"),
            url: u.to_string(),
            color: "#3b82f6".to_string(),
        });
    }
    if let Some(u) = cancel_url {
        actions.push(EmailAction {
            label: t(lang, "email-action-cancel-booking"),
            url: u.to_string(),
            color: "#dc2626".to_string(),
        });
    }

    // Append notice lines to the footer note so they sit just below the
    // action buttons, alongside the existing "ICS attached" copy.
    let mut footer_note_html = ics_attached_html.clone();
    if let Some(line) = &reschedule_notice_line {
        footer_note_html.push('\n');
        footer_note_html.push_str(line);
    }
    if let Some(line) = &cancel_notice_line {
        footer_note_html.push('\n');
        footer_note_html.push_str(line);
    }
    let html = render_html_email_in(
        lang,
        &h(&greeting),
        &h(&headline),
        "#16a34a",
        &rows,
        Some(&footer_note_html),
        &actions,
    );

    let body = build_multipart_body(&plain, &html);

    let ics_attachment = Attachment::new("invite.ics".to_string()).body(
        ics,
        ContentType::parse("text/calendar; method=REQUEST; charset=UTF-8")?,
    );

    let subject = ta(
        lang,
        "email-confirm-subject",
        [("event", &details.event_title), ("date", &date)],
    );

    let email = config
        .message_from(Sender::host_of(details))?
        .to(to)
        .subject(subject)
        .multipart(
            MultiPart::mixed()
                .multipart(body)
                .singlepart(ics_attachment),
        )?;

    send_email(config, email).await
}

/// "You've been added as an attendee", with the invite attached, to each of
/// the booking's additional attendees. Sent once, when the booking first
/// becomes confirmed: at submit, or at the approval of a new request (#266).
/// A reschedule sends them `send_guest_attendee_reschedule` instead (#267).
/// Kept apart from the guest's own confirmation, which every approval sends.
pub async fn send_guest_attendee_confirmations(
    config: &SmtpConfig,
    details: &BookingDetails,
) -> Result<()> {
    send_guest_attendee_copies(config, details, |lang| {
        compose_attendee_confirmation(details, lang)
    })
    .await
}

/// "A booking you attend has moved", with the updated invite, to each of the
/// booking's additional attendees (#267). Sent when the new time becomes
/// confirmed: at a reschedule that needs no approval, or at the approval of
/// one. The invite keeps the booking's UID with the SEQUENCE the reschedule
/// bumped, so their calendar moves the copy they hold instead of adding a
/// second one. A reschedule awaiting approval tells them nothing, like the
/// guest, whose copy also stays at the confirmed time until then.
/// `previous` is the old date and time range, when the caller still has it.
pub async fn send_guest_attendee_reschedule(
    config: &SmtpConfig,
    details: &BookingDetails,
    previous: Option<(&str, &str)>,
) -> Result<()> {
    send_guest_attendee_copies(config, details, |lang| {
        compose_attendee_reschedule(details, previous, lang)
    })
    .await
}

/// One email per additional attendee, composed in that attendee's language
/// (`attendee_lang`), each with the invite. A refused address is logged and
/// does not stop the others.
async fn send_guest_attendee_copies(
    config: &SmtpConfig,
    details: &BookingDetails,
    compose: impl Fn(&str) -> ComposedEmail,
) -> Result<()> {
    for attendee_email in &details.additional_attendees {
        let lang = attendee_lang(
            &details.attendee_languages,
            attendee_email,
            guest_lang(details),
        );
        let result = match attendee_email.parse::<Mailbox>() {
            Ok(to) => {
                send_composed(
                    config,
                    Sender::Host {
                        name: &details.host_name,
                        email: &details.host_email,
                        lang,
                    },
                    to,
                    compose(lang),
                    Some(("invite.ics", generate_ics(details, "REQUEST"), "REQUEST")),
                )
                .await
            }
            Err(e) => Err(e.into()),
        };
        if let Err(e) = result {
            tracing::warn!(attendee = %attendee_email, error = format!("{e:#}"), "failed to send attendee email");
        }
    }

    Ok(())
}

/// "You've been added as an attendee" copy for the additional attendees of
/// a booking, in `lang`: the booking's language, since the guest invited
/// them, unless the attendee is a calrs user with a saved one.
pub(super) fn compose_attendee_confirmation(details: &BookingDetails, lang: &str) -> ComposedEmail {
    let time_display = format!(
        "{} \u{2013} {} ({})",
        details.start_time, details.end_time, details.guest_timezone
    );
    EmailParts {
        lang,
        // Exchange titles the guest's appointment after the Subject header,
        // not the ICS SUMMARY, so REQUEST emails keep a neutral
        // "event, date" subject (#157).
        subject: ta(
            lang,
            "email-confirm-subject",
            [
                ("event", &details.event_title),
                ("date", &long_date(lang, &details.date)),
            ],
        ),
        heading: t(lang, "email-attendee-greeting"),
        message: t(lang, "email-attendee-message"),
        accent: "#16a34a",
        rows: vec![
            row(t(lang, "confirmed-detail-event"), &details.event_title),
            row(
                t(lang, "confirmed-detail-date"),
                long_date(lang, &details.date),
            ),
            row(t(lang, "confirmed-detail-time"), time_display),
            row(t(lang, "email-detail-organizer"), &details.host_name),
            row(
                t(lang, "email-detail-booked-by"),
                format!("{} <{}>", details.guest_name, details.guest_email),
            ),
        ],
        plain_lines: vec![],
        footer_plain: Some(t(lang, "email-confirm-ics-attached-plain")),
        footer_html: Some(t(lang, "email-confirm-ics-attached-html")),
        actions: vec![],
    }
    .compose()
}

/// The attendees' copy of a reschedule, in `lang` (see
/// `compose_attendee_confirmation`). Shows what their "you've been added"
/// email showed, at the new time: never the guest phone or a resource name,
/// which are the host's.
pub(super) fn compose_attendee_reschedule(
    details: &BookingDetails,
    previous: Option<(&str, &str)>,
    lang: &str,
) -> ComposedEmail {
    let time_display = format!(
        "{} \u{2013} {} ({})",
        details.start_time, details.end_time, details.guest_timezone
    );
    let mut rows = vec![row(t(lang, "confirmed-detail-event"), &details.event_title)];
    if let Some((date, time)) = previous {
        rows.push(row(
            t(lang, "email-detail-previous"),
            date_at_time(lang, &long_date(lang, date), time),
        ));
    }
    rows.extend([
        row(
            t(lang, "email-detail-new-date"),
            long_date(lang, &details.date),
        ),
        row(t(lang, "email-detail-new-time"), time_display),
        row(t(lang, "email-detail-organizer"), &details.host_name),
        row(
            t(lang, "email-detail-booked-by"),
            format!("{} <{}>", details.guest_name, details.guest_email),
        ),
    ]);
    EmailParts {
        lang,
        // Neutral subject: Exchange uses it as the appointment title (#157).
        subject: ta(
            lang,
            "email-confirm-subject",
            [
                ("event", &details.event_title),
                ("date", &long_date(lang, &details.date)),
            ],
        ),
        heading: t(lang, "email-attendee-greeting"),
        message: t(lang, "email-attendee-rescheduled-message"),
        accent: "#d97706",
        rows,
        plain_lines: vec![],
        footer_plain: Some(t(lang, "email-rescheduled-ics-attached-plain")),
        footer_html: Some(t(lang, "email-rescheduled-ics-attached-html")),
        actions: vec![],
    }
    .compose()
}

/// Send booking reminder to the guest
pub async fn send_guest_reminder(
    config: &SmtpConfig,
    details: &BookingDetails,
    cancel_url: Option<&str>,
) -> Result<()> {
    let lang = guest_lang(details);
    let to = named_mailbox(&details.guest_name, &details.guest_email)?;

    let time_display = format!(
        "{} \u{2013} {} ({})",
        details.start_time, details.end_time, details.guest_timezone
    );

    let greeting = ta(
        lang,
        "email-confirm-greeting",
        [("name", &details.guest_name)],
    );
    let headline = t(lang, "email-reminder-headline");
    let label_event = t(lang, "confirmed-detail-event");
    let label_date = t(lang, "confirmed-detail-date");
    let date = long_date(lang, &details.date);
    let label_time = t(lang, "confirmed-detail-time");
    let label_with = t(lang, "confirmed-detail-with");
    let label_location = t(lang, "confirmed-detail-location");
    let signature = t(lang, "email-signature");

    let plain = format!(
        "{}\n\n\
         {}\n\n\
         {} {}\n\
         {} {}\n\
         {} {}\n\
         {} {}\n\
         {}{}\n\
         {}",
        greeting,
        headline,
        label_event,
        details.event_title,
        label_date,
        date,
        label_time,
        time_display,
        label_with,
        details.host_name,
        details
            .location
            .as_ref()
            .map(|l| format!("{} {}\n", label_location, l))
            .unwrap_or_default(),
        cancel_url
            .map(|u| format!(
                "\n{}\n",
                ta(lang, "email-confirm-need-to-cancel", [("url", u)])
            ))
            .unwrap_or_default(),
        signature,
    );

    let mut rows = vec![
        EmailRow {
            label: label_event.clone(),
            value: details.event_title.clone(),
        },
        EmailRow {
            label: label_date.clone(),
            value: date.clone(),
        },
        EmailRow {
            label: label_time.clone(),
            value: time_display,
        },
        EmailRow {
            label: label_with.clone(),
            value: details.host_name.clone(),
        },
    ];
    if let Some(loc) = &details.location {
        rows.push(EmailRow {
            label: label_location.clone(),
            value: loc.clone(),
        });
    }

    let actions: Vec<EmailAction> = cancel_url
        .map(|u| {
            vec![EmailAction {
                label: t(lang, "email-action-cancel-booking"),
                url: u.to_string(),
                color: "#dc2626".to_string(),
            }]
        })
        .unwrap_or_default();

    let html = render_html_email_in(
        lang,
        &h(&greeting),
        &h(&headline),
        "#3b82f6",
        &rows,
        None,
        &actions,
    );

    let body = build_multipart_body(&plain, &html);

    let subject = ta(
        lang,
        "email-reminder-subject",
        [
            ("event", &details.event_title),
            ("time", &details.start_time),
        ],
    );
    let email = config
        .message_from(Sender::host_of(details))?
        .to(to)
        .subject(subject)
        .multipart(body)?;

    send_email(config, email).await
}

/// Send cancellation notification to the guest
pub async fn send_guest_cancellation(
    config: &SmtpConfig,
    details: &CancellationDetails,
) -> Result<()> {
    let lang = lang_or_en(details.guest_language.as_deref());

    let to = named_mailbox(&details.guest_name, &details.guest_email)?;

    let time_display = if details.guest_timezone.is_empty() {
        format!("{} \u{2013} {}", details.start_time, details.end_time)
    } else {
        format!(
            "{} \u{2013} {} ({})",
            details.start_time, details.end_time, details.guest_timezone
        )
    };

    let greeting = ta(
        lang,
        "email-confirm-greeting",
        [("name", &details.guest_name)],
    );
    let headline = if details.cancelled_by_host {
        ta(
            lang,
            "email-cancel-headline-by-host",
            [("host", &details.host_name)],
        )
    } else {
        t(lang, "email-cancel-headline-by-guest")
    };
    let label_event = t(lang, "confirmed-detail-event");
    let label_date = t(lang, "confirmed-detail-date");
    let date = long_date(lang, &details.date);
    let label_time = t(lang, "confirmed-detail-time");
    let label_with = t(lang, "confirmed-detail-with");
    let label_reason = t(lang, "common-detail-reason");
    // A guest cancelling a request never confirmed holds no invite, so is
    // sent no CANCEL and no note about one (#267).
    let note = withdrawn_invite_note(lang, details);
    let signature = t(lang, "email-signature");

    let reason_text = details
        .reason
        .as_ref()
        .map(|r| format!("{} {}\n\n", label_reason, r))
        .unwrap_or_default();

    let plain = format!(
        "{}\n\n\
         {}\n\n\
         {} {}\n\
         {} {}\n\
         {} {}\n\
         {} {}\n\n\
         {}\
         {}\
         {}",
        greeting,
        headline,
        label_event,
        details.event_title,
        label_date,
        date,
        label_time,
        time_display,
        label_with,
        details.host_name,
        reason_text,
        note.as_ref()
            .map(|(plain, _)| format!("{plain}\n\n"))
            .unwrap_or_default(),
        signature,
    );

    let mut rows = vec![
        EmailRow {
            label: label_event.clone(),
            value: details.event_title.clone(),
        },
        EmailRow {
            label: label_date.clone(),
            value: date.clone(),
        },
        EmailRow {
            label: label_time.clone(),
            value: time_display,
        },
        EmailRow {
            label: label_with.clone(),
            value: details.host_name.clone(),
        },
    ];
    if let Some(reason) = &details.reason {
        rows.push(EmailRow {
            label: label_reason.clone(),
            value: reason.clone(),
        });
    }

    let html = render_html_email_in(
        lang,
        &h(&greeting),
        &h(&headline),
        "#dc2626",
        &rows,
        note.as_ref().map(|(_, html)| html.as_str()),
        &[],
    );

    let subject = ta(
        lang,
        "email-cancel-subject",
        [("event", &details.event_title), ("date", &date)],
    );
    send_composed(
        config,
        Sender::host_of_cancellation(details),
        to,
        ComposedEmail {
            subject,
            plain,
            html,
        },
        withdrawn_invite(details),
    )
    .await
}

/// "A booking you attend has been cancelled", with a CANCEL for the invite
/// they hold, to each additional attendee of a booking that was confirmed
/// at least once (#267). Sent on every path that ends such a booking: a
/// cancellation by the guest, the host, the CLI or the sync of a deleted
/// calendar event, and the decline or expiry of a reschedule awaiting
/// approval. A booking never confirmed sent its attendees nothing, so it
/// sends them nothing now (`details.additional_attendees` is empty).
pub async fn send_guest_attendee_cancellations(
    config: &SmtpConfig,
    details: &CancellationDetails,
) -> Result<()> {
    if !details.invite_sent {
        return Ok(());
    }
    let booking_lang = lang_or_en(details.guest_language.as_deref());
    for attendee_email in &details.additional_attendees {
        let lang = attendee_lang(&details.attendee_languages, attendee_email, booking_lang);
        let result = match attendee_email.parse::<Mailbox>() {
            Ok(to) => {
                send_composed(
                    config,
                    Sender::Host {
                        name: &details.host_name,
                        email: &details.host_email,
                        lang,
                    },
                    to,
                    compose_attendee_cancellation(details, lang),
                    Some(("cancel.ics", generate_cancel_ics(details), "CANCEL")),
                )
                .await
            }
            Err(e) => Err(e.into()),
        };
        if let Err(e) = result {
            tracing::warn!(attendee = %attendee_email, error = format!("{e:#}"), "failed to send attendee cancellation");
        }
    }
    Ok(())
}

/// The attendees' copy of a cancellation, in `lang`. Shows what their
/// invite email showed, minus the reason: it was written by the guest for
/// the host, or by the host for the guest, and can be personal. When the
/// booking ended on a reschedule awaiting approval, its times are the
/// requested ones, not those of the invite they hold, so the date and time
/// are left out; the CANCEL still matches their copy by UID.
pub(super) fn compose_attendee_cancellation(
    details: &CancellationDetails,
    lang: &str,
) -> ComposedEmail {
    let mut rows = vec![row(t(lang, "confirmed-detail-event"), &details.event_title)];
    if !details.reschedule_pending {
        let time_display = if details.guest_timezone.is_empty() {
            format!("{} \u{2013} {}", details.start_time, details.end_time)
        } else {
            format!(
                "{} \u{2013} {} ({})",
                details.start_time, details.end_time, details.guest_timezone
            )
        };
        rows.push(row(
            t(lang, "confirmed-detail-date"),
            long_date(lang, &details.date),
        ));
        rows.push(row(t(lang, "confirmed-detail-time"), time_display));
    }
    rows.extend([
        row(t(lang, "email-detail-organizer"), &details.host_name),
        row(
            t(lang, "email-detail-booked-by"),
            format!("{} <{}>", details.guest_name, details.guest_email),
        ),
    ]);
    let subject = if details.reschedule_pending {
        ta(
            lang,
            "email-attendee-cancelled-subject",
            [("event", &details.event_title)],
        )
    } else {
        ta(
            lang,
            "email-cancel-subject",
            [
                ("event", &details.event_title),
                ("date", &long_date(lang, &details.date)),
            ],
        )
    };
    EmailParts {
        lang,
        subject,
        heading: t(lang, "email-attendee-greeting"),
        message: t(lang, "email-attendee-cancelled-message"),
        accent: "#dc2626",
        rows,
        plain_lines: vec![],
        footer_plain: Some(t(lang, "email-cancel-ics-attached-plain")),
        footer_html: Some(t(lang, "email-cancel-ics-attached-html")),
        actions: vec![],
    }
    .compose()
}

/// Send pending notice to guest (booking awaits host approval)
pub async fn send_guest_pending_notice(
    config: &SmtpConfig,
    details: &BookingDetails,
    cancel_url: Option<&str>,
) -> Result<()> {
    send_guest_pending_notice_ex(config, details, cancel_url, None).await
}

pub async fn send_guest_pending_notice_ex(
    config: &SmtpConfig,
    details: &BookingDetails,
    cancel_url: Option<&str>,
    reschedule_url: Option<&str>,
) -> Result<()> {
    let to = named_mailbox(&details.guest_name, &details.guest_email)?;
    let composed = compose_guest_pending_notice(details, cancel_url, reschedule_url);
    let body = build_multipart_body(&composed.plain, &composed.html);

    let email = config
        .message_from(Sender::host_of(details))?
        .to(to)
        .subject(composed.subject)
        .multipart(body)?;

    send_email(config, email).await
}

/// The guest's "request received, awaiting the host" email, in the language
/// stored with the booking. It reuses the booking page's own pending wording
/// (`confirmed-heading-pending`, `confirmed-subtitle-pending`), which says
/// exactly what this email has to say, so every locale gets it today.
pub(super) fn compose_guest_pending_notice(
    details: &BookingDetails,
    cancel_url: Option<&str>,
    reschedule_url: Option<&str>,
) -> ComposedEmail {
    let lang = guest_lang(details);

    let time_display = format!(
        "{} \u{2013} {} ({})",
        details.start_time, details.end_time, details.guest_timezone
    );

    let greeting = ta(
        lang,
        "email-confirm-greeting",
        [("name", &details.guest_name)],
    );
    let headline = ta(
        lang,
        "confirmed-subtitle-pending",
        [
            ("host", &details.host_name),
            ("email", &details.guest_email),
        ],
    );
    let label_event = t(lang, "confirmed-detail-event");
    let label_date = t(lang, "confirmed-detail-date");
    let date = long_date(lang, &details.date);
    let label_time = t(lang, "confirmed-detail-time");
    let label_with = t(lang, "confirmed-detail-with");
    let label_notes = t(lang, "confirmed-detail-notes");
    let signature = t(lang, "email-signature");

    // Don't include location in pending emails: it should only be revealed
    // after the booking is confirmed (prevents meeting link leaking).
    let mut plain = format!(
        "{}\n\n{}\n\n{} {}\n{} {}\n{} {}\n{} {}\n",
        greeting,
        headline,
        label_event,
        details.event_title,
        label_date,
        date,
        label_time,
        time_display,
        label_with,
        details.host_name,
    );
    if let Some(n) = &details.notes {
        plain.push_str(&format!("{} {}\n", label_notes, n));
    }
    if let Some(u) = cancel_url {
        plain.push_str(&format!(
            "\n{}\n",
            ta(lang, "email-confirm-need-to-cancel", [("url", u)])
        ));
    }
    plain.push('\n');
    plain.push_str(&signature);

    let mut rows = vec![
        EmailRow {
            label: label_event,
            value: details.event_title.clone(),
        },
        EmailRow {
            label: label_date,
            value: date.clone(),
        },
        EmailRow {
            label: label_time,
            value: time_display,
        },
        EmailRow {
            label: label_with,
            value: details.host_name.clone(),
        },
    ];
    if let Some(notes) = &details.notes {
        rows.push(EmailRow {
            label: label_notes,
            value: notes.clone(),
        });
    }

    let mut actions: Vec<EmailAction> = Vec::new();
    if let Some(u) = reschedule_url {
        actions.push(EmailAction {
            label: t(lang, "email-action-reschedule"),
            url: u.to_string(),
            color: "#3b82f6".to_string(),
        });
    }
    if let Some(u) = cancel_url {
        actions.push(EmailAction {
            label: t(lang, "email-action-cancel-booking"),
            url: u.to_string(),
            color: "#dc2626".to_string(),
        });
    }

    let html = render_html_email_in(
        lang,
        &h(&greeting),
        &h(&headline),
        "#f59e0b",
        &rows,
        None,
        &actions,
    );

    ComposedEmail {
        subject: status_subject(
            lang,
            "confirmed-heading-pending",
            &details.event_title,
            &date,
        ),
        plain,
        html,
    }
}

/// Send decline notification to the guest
pub async fn send_guest_decline_notice(
    config: &SmtpConfig,
    details: &CancellationDetails,
) -> Result<()> {
    let to = named_mailbox(&details.guest_name, &details.guest_email)?;
    let composed = compose_guest_decline_notice(details);
    send_composed(
        config,
        Sender::host_of_cancellation(details),
        to,
        composed,
        withdrawn_invite(details),
    )
    .await
}

/// The CANCEL a guest's cancellation, decline or expiry email carries when
/// the guest holds the invite, i.e. the booking was confirmed at least
/// once; for a decline or an expiry, the request was then a reschedule
/// whose calendar copy still shows the confirmed time (#267). A request
/// never confirmed sent no invite, so its emails carry none.
fn withdrawn_invite(details: &CancellationDetails) -> Option<(&'static str, String, &'static str)> {
    details
        .invite_sent
        .then(|| ("cancel.ics", generate_cancel_ics(details), "CANCEL"))
}

/// The "calendar cancellation attached" note for `withdrawn_invite`, as
/// (plain, html).
fn withdrawn_invite_note(lang: &str, details: &CancellationDetails) -> Option<(String, String)> {
    details.invite_sent.then(|| {
        (
            t(lang, "email-cancel-ics-attached-plain"),
            t(lang, "email-cancel-ics-attached-html"),
        )
    })
}

/// The headline of a decline or an expiry, followed, when it ends a
/// reschedule of a confirmed booking, by the line saying the original
/// booking is gone too: the attached CANCEL removes it from the guest's
/// calendar, and "your request was declined" alone reads as if it stood.
fn withdrawn_headline(lang: &str, details: &CancellationDetails, headline: String) -> String {
    if details.invite_sent && details.reschedule_pending {
        format!("{headline} {}", t(lang, "email-original-booking-cancelled"))
    } else {
        headline
    }
}

/// The guest's "your request was declined" email, in the language stored
/// with the booking. Reuses the decline page's wording
/// (`bae-title-booking-declined`, `bae-body-declined-by-host`).
pub(super) fn compose_guest_decline_notice(details: &CancellationDetails) -> ComposedEmail {
    let lang = lang_or_en(details.guest_language.as_deref());

    let time_display = if details.guest_timezone.is_empty() {
        format!("{} \u{2013} {}", details.start_time, details.end_time)
    } else {
        format!(
            "{} \u{2013} {} ({})",
            details.start_time, details.end_time, details.guest_timezone
        )
    };

    let greeting = ta(
        lang,
        "email-confirm-greeting",
        [("name", &details.guest_name)],
    );
    let headline = withdrawn_headline(lang, details, t(lang, "bae-body-declined-by-host"));
    let label_event = t(lang, "confirmed-detail-event");
    let label_date = t(lang, "confirmed-detail-date");
    let date = long_date(lang, &details.date);
    let label_time = t(lang, "confirmed-detail-time");
    let label_with = t(lang, "confirmed-detail-with");
    let label_reason = t(lang, "common-detail-reason");
    let signature = t(lang, "email-signature");

    let mut plain = format!(
        "{}\n\n{}\n\n{} {}\n{} {}\n{} {}\n{} {}\n\n",
        greeting,
        headline,
        label_event,
        details.event_title,
        label_date,
        date,
        label_time,
        time_display,
        label_with,
        details.host_name,
    );
    if let Some(r) = &details.reason {
        plain.push_str(&format!("{} {}\n\n", label_reason, r));
    }
    let note = withdrawn_invite_note(lang, details);
    if let Some((note_plain, _)) = &note {
        plain.push_str(&format!("{note_plain}\n\n"));
    }
    plain.push_str(&signature);

    let mut rows = vec![
        EmailRow {
            label: label_event,
            value: details.event_title.clone(),
        },
        EmailRow {
            label: label_date,
            value: date.clone(),
        },
        EmailRow {
            label: label_time,
            value: time_display,
        },
        EmailRow {
            label: label_with,
            value: details.host_name.clone(),
        },
    ];
    if let Some(reason) = &details.reason {
        rows.push(EmailRow {
            label: label_reason,
            value: reason.clone(),
        });
    }

    let html = render_html_email_in(
        lang,
        &h(&greeting),
        &h(&headline),
        "#dc2626",
        &rows,
        note.as_ref().map(|(_, html)| html.as_str()),
        &[],
    );

    ComposedEmail {
        subject: status_subject(
            lang,
            "bae-title-booking-declined",
            &details.event_title,
            &date,
        ),
        plain,
        html,
    }
}

/// Tell the guest their request expired because the host did not answer in
/// time (#234), in the language stored with the booking. `rebook_url` points
/// back to the event type's booking page when it is public and a base URL is
/// configured.
pub async fn send_guest_pending_expired(
    config: &SmtpConfig,
    details: &CancellationDetails,
    rebook_url: Option<&str>,
) -> Result<()> {
    let to = named_mailbox(&details.guest_name, &details.guest_email)?;
    let composed = compose_guest_pending_expired(details, rebook_url);
    send_composed(
        config,
        Sender::host_of_cancellation(details),
        to,
        composed,
        withdrawn_invite(details),
    )
    .await
}

pub(super) fn compose_guest_pending_expired(
    details: &CancellationDetails,
    rebook_url: Option<&str>,
) -> ComposedEmail {
    let lang = lang_or_en(details.guest_language.as_deref());

    let time_display = if details.guest_timezone.is_empty() {
        format!("{} \u{2013} {}", details.start_time, details.end_time)
    } else {
        format!(
            "{} \u{2013} {} ({})",
            details.start_time, details.end_time, details.guest_timezone
        )
    };

    let greeting = ta(
        lang,
        "email-confirm-greeting",
        [("name", &details.guest_name)],
    );
    let headline = withdrawn_headline(lang, details, t(lang, "email-pending-expired-headline"));
    let label_event = t(lang, "confirmed-detail-event");
    let label_date = t(lang, "confirmed-detail-date");
    let date = long_date(lang, &details.date);
    let label_time = t(lang, "confirmed-detail-time");
    let label_with = t(lang, "confirmed-detail-with");
    let action_label = t(lang, "email-pending-expired-action");
    let signature = t(lang, "email-signature");

    let mut plain = format!(
        "{}\n\n{}\n\n{} {}\n{} {}\n{} {}\n{} {}\n\n",
        greeting,
        headline,
        label_event,
        details.event_title,
        label_date,
        date,
        label_time,
        time_display,
        label_with,
        details.host_name,
    );
    if let Some(url) = rebook_url {
        plain.push_str(&format!("{}: {}\n\n", action_label, url));
    }
    let note = withdrawn_invite_note(lang, details);
    if let Some((note_plain, _)) = &note {
        plain.push_str(&format!("{note_plain}\n\n"));
    }
    plain.push_str(&signature);

    let rows = vec![
        EmailRow {
            label: label_event,
            value: details.event_title.clone(),
        },
        EmailRow {
            label: label_date,
            value: date.clone(),
        },
        EmailRow {
            label: label_time,
            value: time_display,
        },
        EmailRow {
            label: label_with,
            value: details.host_name.clone(),
        },
    ];
    let actions: Vec<EmailAction> = rebook_url
        .map(|url| EmailAction {
            label: action_label,
            url: url.to_string(),
            color: "#6366f1".to_string(),
        })
        .into_iter()
        .collect();

    let html = render_html_email_in(
        lang,
        &h(&greeting),
        &h(&headline),
        "#dc2626",
        &rows,
        note.as_ref().map(|(_, html)| html.as_str()),
        &actions,
    );

    ComposedEmail {
        subject: status_subject(
            lang,
            "email-pending-expired-status",
            &details.event_title,
            &date,
        ),
        plain,
        html,
    }
}

/// Send a booking invite email to a guest.
///
/// `lang` is the inviting user's language as their dashboard resolved it
/// (saved preference, then their browser's `Accept-Language`). Nothing is
/// known about the invitee yet, and the optional personal message is written
/// by the inviter, so the email is in the inviter's language.
pub async fn send_invite_email(
    config: &SmtpConfig,
    lang: &str,
    guest_name: &str,
    guest_email: &str,
    event_title: &str,
    host_name: &str,
    host_email: &str,
    message: Option<&str>,
    invite_url: &str,
    expires_at: Option<&str>,
) -> Result<()> {
    let to = named_mailbox(guest_name, guest_email)?;
    let mail = compose_invite_email(
        lang,
        guest_name,
        event_title,
        host_name,
        message,
        invite_url,
        expires_at,
    );
    let sender = Sender::Host {
        name: host_name,
        email: host_email,
        lang,
    };
    send_composed(config, sender, to, mail, None).await
}

pub(super) fn compose_invite_email(
    lang: &str,
    guest_name: &str,
    event_title: &str,
    host_name: &str,
    message: Option<&str>,
    invite_url: &str,
    expires_at: Option<&str>,
) -> ComposedEmail {
    let lang = lang_or_en(Some(lang));
    let mut rows = vec![
        row(t(lang, "confirmed-detail-event"), event_title),
        row(t(lang, "email-detail-invited-by"), host_name),
    ];
    if let Some(msg) = message.filter(|m| !m.trim().is_empty()) {
        rows.push(row(t(lang, "email-detail-message"), msg));
    }
    if let Some(exp) = expires_at {
        // Stored as a UTC "YYYY-MM-DD HH:MM:SS"; nothing is known of the
        // invitee's timezone, so the zone is named rather than guessed.
        let exp = chrono::NaiveDateTime::parse_from_str(exp.trim(), "%Y-%m-%d %H:%M:%S")
            .map(|at| deadline_display(lang, at.and_utc(), "UTC"))
            .unwrap_or_else(|_| exp.to_string());
        rows.push(row(t(lang, "email-detail-expires"), exp));
    }
    let invite_args = [("host", host_name), ("event", event_title)];
    EmailParts {
        lang,
        subject: ta(lang, "email-invite-subject", invite_args),
        heading: ta(lang, "email-confirm-greeting", [("name", guest_name)]),
        message: ta(lang, "email-invite-message", invite_args),
        accent: "#6366f1",
        rows,
        plain_lines: vec![t(lang, "email-invite-link"), invite_url.to_string()],
        footer_plain: None,
        footer_html: None,
        actions: vec![EmailAction {
            label: t(lang, "email-invite-action"),
            url: invite_url.to_string(),
            color: "#6366f1".to_string(),
        }],
    }
    .compose()
}

/// Ask the guest to pick a new time (host-initiated reschedule).
/// The guest clicks the link to choose a slot; no time is pre-selected.
pub async fn send_guest_pick_new_time(
    config: &SmtpConfig,
    details: &BookingDetails,
    reschedule_url: &str,
    cancel_url: Option<&str>,
) -> Result<()> {
    let to = named_mailbox(&details.guest_name, &details.guest_email)?;
    let mail = compose_guest_pick_new_time(details, reschedule_url, cancel_url);
    send_composed(config, Sender::host_of(details), to, mail, None).await
}

pub(super) fn compose_guest_pick_new_time(
    details: &BookingDetails,
    reschedule_url: &str,
    cancel_url: Option<&str>,
) -> ComposedEmail {
    let lang = guest_lang(details);
    let time_display = format!(
        "{} \u{2013} {} ({})",
        details.start_time, details.end_time, details.guest_timezone
    );

    let mut plain_lines = vec![ta(
        lang,
        "email-pick-new-time-link",
        [("url", reschedule_url)],
    )];
    let mut actions = vec![EmailAction {
        label: t(lang, "email-action-pick-new-time"),
        url: reschedule_url.to_string(),
        color: "#d97706".to_string(),
    }];
    if let Some(u) = cancel_url {
        plain_lines.push(ta(lang, "email-confirm-need-to-cancel", [("url", u)]));
        actions.push(EmailAction {
            label: t(lang, "email-action-cancel-booking"),
            url: u.to_string(),
            color: "#dc2626".to_string(),
        });
    }

    EmailParts {
        lang,
        subject: ta(
            lang,
            "email-pick-new-time-subject",
            [("event", &details.event_title)],
        ),
        heading: ta(
            lang,
            "email-confirm-greeting",
            [("name", &details.guest_name)],
        ),
        message: ta(
            lang,
            "email-pick-new-time-message",
            [("host", &details.host_name)],
        ),
        accent: "#d97706",
        rows: vec![
            row(t(lang, "confirmed-detail-event"), &details.event_title),
            row(
                t(lang, "email-detail-originally"),
                date_at_time(lang, &long_date(lang, &details.date), &time_display),
            ),
            row(t(lang, "confirmed-detail-with"), &details.host_name),
        ],
        plain_lines,
        footer_plain: None,
        footer_html: None,
        actions,
    }
    .compose()
}

/// Notify the guest that their booking was rescheduled, in the language they
/// booked in. Includes updated ICS calendar invite.
pub async fn send_guest_reschedule_notification(
    config: &SmtpConfig,
    details: &RescheduleDetails,
    cancel_url: Option<&str>,
    reschedule_url: Option<&str>,
) -> Result<()> {
    let booking_details = BookingDetails {
        utc_times: details.utc_times.clone(),
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
        // The guest's confirmation invite listed the additional attendees
        // and carried the notes; this one replaces it, so it does too (#267).
        // Never the phone: email invites leave it out (`generate_ics_impl`).
        notes: details.notes.clone(),
        location: details.location.clone(),
        reminder_minutes: None,
        additional_attendees: details.additional_attendees.clone(),
        sequence: details.sequence,
        ..Default::default()
    };
    let ics = generate_ics(&booking_details, "REQUEST");

    let to = named_mailbox(&details.guest_name, &details.guest_email)?;
    let mail = compose_guest_reschedule_notification(details, cancel_url, reschedule_url);
    let sender = Sender::Host {
        name: &details.host_name,
        email: &details.host_email,
        lang: lang_or_en(details.guest_language.as_deref()),
    };
    send_composed(
        config,
        sender,
        to,
        mail,
        Some(("invite.ics", ics, "REQUEST")),
    )
    .await
}

pub(super) fn compose_guest_reschedule_notification(
    details: &RescheduleDetails,
    cancel_url: Option<&str>,
    reschedule_url: Option<&str>,
) -> ComposedEmail {
    let lang = lang_or_en(details.guest_language.as_deref());
    let new_time_display = format!(
        "{} \u{2013} {} ({})",
        details.new_start_time, details.new_end_time, details.guest_timezone
    );
    let old_time_display = format!(
        "{} \u{2013} {}",
        details.old_start_time, details.old_end_time
    );

    let mut rows = vec![
        row(t(lang, "confirmed-detail-event"), &details.event_title),
        row(
            t(lang, "email-detail-previous"),
            date_at_time(lang, &long_date(lang, &details.old_date), &old_time_display),
        ),
        row(
            t(lang, "email-detail-new-date"),
            long_date(lang, &details.new_date),
        ),
        row(t(lang, "email-detail-new-time"), new_time_display),
        row(t(lang, "confirmed-detail-with"), &details.host_name),
    ];
    push_row(
        &mut rows,
        lang,
        "confirmed-detail-location",
        details.location.as_ref(),
    );

    let mut plain_lines = Vec::new();
    let mut actions = Vec::new();
    if let Some(u) = reschedule_url {
        plain_lines.push(ta(lang, "email-need-to-reschedule", [("url", u)]));
        actions.push(EmailAction {
            label: t(lang, "email-action-reschedule"),
            url: u.to_string(),
            color: "#d97706".to_string(),
        });
    }
    if let Some(u) = cancel_url {
        plain_lines.push(ta(lang, "email-confirm-need-to-cancel", [("url", u)]));
        actions.push(EmailAction {
            label: t(lang, "email-action-cancel-booking"),
            url: u.to_string(),
            color: "#dc2626".to_string(),
        });
    }

    EmailParts {
        lang,
        // Neutral subject: Exchange uses it as the appointment title (#157).
        subject: ta(
            lang,
            "email-confirm-subject",
            [
                ("event", &details.event_title),
                ("date", &long_date(lang, &details.new_date)),
            ],
        ),
        heading: ta(
            lang,
            "email-confirm-greeting",
            [("name", &details.guest_name)],
        ),
        message: ta(
            lang,
            "email-rescheduled-message",
            [("host", &details.host_name)],
        ),
        accent: "#d97706",
        rows,
        plain_lines,
        footer_plain: Some(t(lang, "email-rescheduled-ics-attached-plain")),
        footer_html: Some(t(lang, "email-rescheduled-ics-attached-html")),
        actions,
    }
    .compose()
}

/// Tell the guest the host approved the new time of their reschedule, in the
/// language they booked in, with the moved invite (#267). Replaces the
/// "booking confirmed" email at that approval, so the email says what the
/// SMS says: the booking moved, it is not a new one. The invite is built
/// from the full booking, attendees and notes included, like the
/// confirmation invite it replaces.
pub async fn send_guest_reschedule_approved(
    config: &SmtpConfig,
    details: &BookingDetails,
    cancel_url: Option<&str>,
    reschedule_url: Option<&str>,
    cancel_notice_min: Option<i32>,
    reschedule_notice_min: Option<i32>,
) -> Result<()> {
    let to = named_mailbox(&details.guest_name, &details.guest_email)?;
    let mail = compose_guest_reschedule_approved(
        details,
        cancel_url,
        reschedule_url,
        cancel_notice_min,
        reschedule_notice_min,
    );
    send_composed(
        config,
        Sender::host_of(details),
        to,
        mail,
        Some(("invite.ics", generate_ics(details, "REQUEST"), "REQUEST")),
    )
    .await
}

pub(super) fn compose_guest_reschedule_approved(
    details: &BookingDetails,
    cancel_url: Option<&str>,
    reschedule_url: Option<&str>,
    cancel_notice_min: Option<i32>,
    reschedule_notice_min: Option<i32>,
) -> ComposedEmail {
    let lang = guest_lang(details);
    let time_display = format!(
        "{} \u{2013} {} ({})",
        details.start_time, details.end_time, details.guest_timezone
    );
    let mut rows = vec![
        row(t(lang, "confirmed-detail-event"), &details.event_title),
        row(
            t(lang, "email-detail-new-date"),
            long_date(lang, &details.date),
        ),
        row(t(lang, "email-detail-new-time"), time_display),
        row(t(lang, "confirmed-detail-with"), &details.host_name),
    ];
    push_row(
        &mut rows,
        lang,
        "confirmed-detail-location",
        details.location.as_ref(),
    );
    push_row(
        &mut rows,
        lang,
        "confirmed-detail-notes",
        details.notes.as_ref(),
    );

    // The same notice-window lines the confirmation shows.
    let notice = |key: &str, minutes: Option<i32>| {
        minutes
            .filter(|m| *m > 0)
            .map(|m| ta(lang, key, [("minutes", m.to_string().as_str())]))
    };
    let notice_lines: Vec<String> = [
        notice("email-confirm-reschedule-notice", reschedule_notice_min),
        notice("email-confirm-cancel-notice", cancel_notice_min),
    ]
    .into_iter()
    .flatten()
    .collect();

    let mut plain_lines = Vec::new();
    let mut actions = Vec::new();
    if let Some(u) = reschedule_url {
        plain_lines.push(ta(lang, "email-need-to-reschedule", [("url", u)]));
        actions.push(EmailAction {
            label: t(lang, "email-action-reschedule"),
            url: u.to_string(),
            color: "#3b82f6".to_string(),
        });
    }
    if let Some(u) = cancel_url {
        plain_lines.push(ta(lang, "email-confirm-need-to-cancel", [("url", u)]));
        actions.push(EmailAction {
            label: t(lang, "email-action-cancel-booking"),
            url: u.to_string(),
            color: "#dc2626".to_string(),
        });
    }
    let mut footer_plain = t(lang, "email-rescheduled-ics-attached-plain");
    let mut footer_html = t(lang, "email-rescheduled-ics-attached-html");
    for line in &notice_lines {
        footer_plain.push('\n');
        footer_plain.push_str(line);
        footer_html.push('\n');
        footer_html.push_str(line);
    }

    EmailParts {
        lang,
        // Neutral subject: Exchange uses it as the appointment title (#157).
        subject: ta(
            lang,
            "email-confirm-subject",
            [
                ("event", &details.event_title),
                ("date", &long_date(lang, &details.date)),
            ],
        ),
        heading: ta(
            lang,
            "email-confirm-greeting",
            [("name", &details.guest_name)],
        ),
        message: ta(
            lang,
            "email-reschedule-approved-message",
            [("host", &details.host_name)],
        ),
        accent: "#16a34a",
        rows,
        plain_lines,
        footer_plain: Some(footer_plain),
        footer_html: Some(footer_html),
        actions,
    }
    .compose()
}
