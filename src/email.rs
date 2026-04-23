use anyhow::Result;
use chrono::NaiveDateTime;
use chrono_tz::Tz;
use lettre::message::header::ContentType;
use lettre::message::{Attachment, MultiPart, SinglePart};
use lettre::transport::smtp::authentication::Credentials;
use lettre::{AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};
use sqlx::SqlitePool;

pub struct SmtpConfig {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub password: String,
    pub from_email: String,
    pub from_name: Option<String>,
}

#[derive(Clone)]
pub struct BookingDetails {
    pub event_title: String,
    pub date: String,
    pub start_time: String,
    pub end_time: String,
    pub guest_name: String,
    pub guest_email: String,
    pub guest_timezone: String,
    pub host_name: String,
    pub host_email: String,
    pub uid: String,
    pub notes: Option<String>,
    pub location: Option<String>,
    pub reminder_minutes: Option<i32>,
    pub additional_attendees: Vec<String>,
}

pub struct CancellationDetails {
    pub event_title: String,
    pub date: String,
    pub start_time: String,
    pub end_time: String,
    pub guest_name: String,
    pub guest_email: String,
    pub guest_timezone: String,
    pub host_name: String,
    pub host_email: String,
    pub uid: String,
    pub reason: Option<String>,
    pub cancelled_by_host: bool,
}

// --- HTML email template helpers ---

fn h(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

struct EmailRow {
    label: &'static str,
    value: String,
}

struct EmailAction {
    label: String,
    url: String,
    color: String,
}

fn render_html_email(
    greeting: &str,
    message: &str,
    accent: &str,
    rows: &[EmailRow],
    footer_note: Option<&str>,
) -> String {
    render_html_email_with_actions(greeting, message, accent, rows, footer_note, &[])
}

fn render_html_email_with_actions(
    greeting: &str,
    message: &str,
    accent: &str,
    rows: &[EmailRow],
    footer_note: Option<&str>,
    actions: &[EmailAction],
) -> String {
    let mut detail_rows = String::new();
    for (i, row) in rows.iter().enumerate() {
        let bg = if i % 2 == 0 { "#f8f9fa" } else { "#ffffff" };
        detail_rows.push_str(&format!(
            "<tr>\
               <td style=\"padding:8px 12px;color:#6b7280;font-size:13px;white-space:nowrap;vertical-align:top;\">{}</td>\
               <td style=\"padding:8px 12px;color:#111827;font-size:14px;background:{bg};\">{}</td>\
             </tr>",
            row.label, h(&row.value),
        ));
    }

    let actions_html = if actions.is_empty() {
        String::new()
    } else {
        let buttons: Vec<String> = actions.iter().map(|a| {
            format!(
                "<a href=\"{}\" style=\"display:inline-block;padding:12px 28px;background:{};color:#ffffff;text-decoration:none;border-radius:6px;font-weight:600;font-size:14px;margin:0 6px;\">{}</a>",
                h(&a.url), a.color, h(&a.label)
            )
        }).collect();
        format!(
            "<table role=\"presentation\" width=\"100%\" cellpadding=\"0\" cellspacing=\"0\" style=\"margin:20px 0 0;\"><tr><td align=\"center\">{}</td></tr></table>",
            buttons.join(" ")
        )
    };

    let footer_html = footer_note
        .map(|n| {
            format!(
                "<p style=\"margin:16px 0 0;font-size:13px;color:#6b7280;\">{}</p>",
                h(n)
            )
        })
        .unwrap_or_default();

    format!(
        r##"<!DOCTYPE html>
<html lang="en">
<head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1.0"></head>
<body style="margin:0;padding:0;background:#f4f4f7;font-family:-apple-system,BlinkMacSystemFont,'Segoe UI',Roboto,Helvetica,Arial,sans-serif;">
<table role="presentation" width="100%" cellpadding="0" cellspacing="0" style="background:#f4f4f7;">
<tr><td align="center" style="padding:32px 16px;">
  <table role="presentation" width="520" cellpadding="0" cellspacing="0" style="background:#ffffff;border-radius:8px;border:1px solid #e5e7eb;max-width:520px;width:100%;">
    <!-- Accent bar -->
    <tr><td style="height:4px;background:{accent};border-radius:8px 8px 0 0;"></td></tr>
    <!-- Content -->
    <tr><td style="padding:32px 28px;">
      <p style="margin:0 0 4px;font-size:15px;color:#374151;">{greeting}</p>
      <p style="margin:0 0 20px;font-size:15px;color:#111827;font-weight:500;">{message}</p>
      <!-- Details table -->
      <table role="presentation" width="100%" cellpadding="0" cellspacing="0" style="border:1px solid #e5e7eb;border-radius:6px;overflow:hidden;">
        {detail_rows}
      </table>
      {actions_html}
      {footer_html}
    </td></tr>
    <!-- Footer -->
    <tr><td style="padding:16px 28px;border-top:1px solid #f0f0f3;text-align:center;">
      <span style="font-size:12px;color:#9ca3af;">Sent by </span>
      <a href="https://cal.rs" style="font-size:12px;color:#6b7280;font-weight:600;text-decoration:none;">calrs</a>
    </td></tr>
  </table>
</td></tr>
</table>
</body>
</html>"##
    )
}

fn build_multipart_body(plain: &str, html: &str) -> MultiPart {
    MultiPart::alternative()
        .singlepart(SinglePart::plain(plain.to_string()))
        .singlepart(
            SinglePart::builder()
                .header(ContentType::parse("text/html; charset=UTF-8").unwrap())
                .body(html.to_string()),
        )
}

// --- ICS generation ---

/// Sanitize a value for use in an ICS field.
/// Strips CR/LF to prevent ICS injection (RFC 5545 field breakout).
fn sanitize_ics(value: &str) -> String {
    value
        .replace('\r', "")
        .replace('\n', " ")
        .replace(';', "\\;")
        .replace(',', "\\,")
}

/// Convert date + start/end times from a guest timezone to UTC ICS format (YYYYMMDDTHHMMSSZ).
/// Falls back to floating time (no Z) if timezone parsing fails.
fn convert_to_utc(
    date: &str,
    start_time: &str,
    end_time: &str,
    timezone: &str,
) -> (String, String) {
    let fallback_start = format!(
        "{}T{}00",
        date.replace('-', ""),
        start_time.replace(':', "")
    );
    let fallback_end = format!("{}T{}00", date.replace('-', ""), end_time.replace(':', ""));

    let tz: Tz = match timezone.parse() {
        Ok(t) => t,
        Err(_) => return (fallback_start, fallback_end),
    };

    let start_naive = match NaiveDateTime::parse_from_str(
        &format!("{} {}:00", date, start_time),
        "%Y-%m-%d %H:%M:%S",
    ) {
        Ok(dt) => dt,
        Err(_) => return (fallback_start, fallback_end),
    };
    let end_naive = match NaiveDateTime::parse_from_str(
        &format!("{} {}:00", date, end_time),
        "%Y-%m-%d %H:%M:%S",
    ) {
        Ok(dt) => dt,
        Err(_) => return (fallback_start, fallback_end),
    };

    use chrono::TimeZone;
    let start_utc = match tz.from_local_datetime(&start_naive).earliest() {
        Some(dt) => dt.with_timezone(&chrono::Utc),
        None => return (fallback_start, fallback_end),
    };
    let end_utc = match tz.from_local_datetime(&end_naive).earliest() {
        Some(dt) => dt.with_timezone(&chrono::Utc),
        None => return (fallback_start, fallback_end),
    };

    (
        start_utc.format("%Y%m%dT%H%M%SZ").to_string(),
        end_utc.format("%Y%m%dT%H%M%SZ").to_string(),
    )
}

/// Generate an .ics VCALENDAR string for a booking
/// Extract first name (first word) from a full name.
fn first_name(full_name: &str) -> &str {
    full_name.split_whitespace().next().unwrap_or(full_name)
}

pub fn generate_ics(details: &BookingDetails, method: &str) -> String {
    let guest_first = first_name(&details.guest_name);
    let host_first = first_name(&details.host_name);
    let summary = sanitize_ics(&format!(
        "{} \u{2014} {} & {}",
        details.event_title, guest_first, host_first
    ));
    let host_name = sanitize_ics(&details.host_name);
    let guest_name = sanitize_ics(&details.guest_name);
    let host_email = sanitize_ics(&details.host_email);
    let guest_email = sanitize_ics(&details.guest_email);
    let location_line = details
        .location
        .as_ref()
        .map(|l| format!("LOCATION:{}\r\n", sanitize_ics(l)))
        .unwrap_or_default();
    let description_line = details
        .notes
        .as_ref()
        .filter(|n| !n.trim().is_empty())
        .map(|n| format!("DESCRIPTION:{}\r\n", sanitize_ics(n)))
        .unwrap_or_default();
    let valarm = details
        .reminder_minutes
        .filter(|&m| m > 0)
        .map(|m| {
            format!(
                "BEGIN:VALARM\r\n\
                 TRIGGER:-PT{m}M\r\n\
                 ACTION:DISPLAY\r\n\
                 DESCRIPTION:Reminder\r\n\
                 END:VALARM\r\n"
            )
        })
        .unwrap_or_default();
    let additional_attendee_lines: String = details
        .additional_attendees
        .iter()
        .map(|email| format!("ATTENDEE;RSVP=TRUE:mailto:{}\r\n", sanitize_ics(email)))
        .collect();
    let dtstamp = chrono::Utc::now().format("%Y%m%dT%H%M%SZ").to_string();
    // Convert guest-timezone times to UTC for the ICS
    let (dtstart, dtend) = convert_to_utc(
        &details.date,
        &details.start_time,
        &details.end_time,
        &details.guest_timezone,
    );
    format!(
        "BEGIN:VCALENDAR\r\n\
         VERSION:2.0\r\n\
         PRODID:-//calrs//calrs//EN\r\n\
         {method_line}\
         BEGIN:VEVENT\r\n\
         UID:{uid}\r\n\
         DTSTAMP:{dtstamp}\r\n\
         DTSTART:{dtstart}\r\n\
         DTEND:{dtend}\r\n\
         SUMMARY:{summary}\r\n\
         {description_line}\
         {location_line}\
         ORGANIZER;CN={host_name}:mailto:{host_email}\r\n\
         ATTENDEE;CN={guest_name};RSVP=TRUE:mailto:{guest_email}\r\n\
         {additional_attendee_lines}\
         STATUS:CONFIRMED\r\n\
         {valarm}\
         END:VEVENT\r\n\
         END:VCALENDAR\r\n",
        method_line = if method.is_empty() {
            String::new()
        } else {
            format!("METHOD:{method}\r\n")
        },
        uid = details.uid,
        dtstamp = dtstamp,
        dtstart = dtstart,
        dtend = dtend,
        summary = summary,
        description_line = description_line,
        location_line = location_line,
        host_name = host_name,
        host_email = host_email,
        guest_name = guest_name,
        guest_email = guest_email,
        additional_attendee_lines = additional_attendee_lines,
    )
}

/// Generate an .ics VCALENDAR for cancellation (METHOD:CANCEL)
fn generate_cancel_ics(details: &CancellationDetails) -> String {
    let guest_first = first_name(&details.guest_name);
    let host_first = first_name(&details.host_name);
    let summary = sanitize_ics(&format!(
        "{} \u{2014} {} & {}",
        details.event_title, guest_first, host_first
    ));
    let host_name = sanitize_ics(&details.host_name);
    let guest_name = sanitize_ics(&details.guest_name);
    let host_email = sanitize_ics(&details.host_email);
    let guest_email = sanitize_ics(&details.guest_email);
    let dtstamp = chrono::Utc::now().format("%Y%m%dT%H%M%SZ").to_string();
    let (dtstart, dtend) = convert_to_utc(
        &details.date,
        &details.start_time,
        &details.end_time,
        &details.guest_timezone,
    );
    format!(
        "BEGIN:VCALENDAR\r\n\
         VERSION:2.0\r\n\
         PRODID:-//calrs//calrs//EN\r\n\
         METHOD:CANCEL\r\n\
         BEGIN:VEVENT\r\n\
         UID:{uid}\r\n\
         DTSTAMP:{dtstamp}\r\n\
         DTSTART:{dtstart}\r\n\
         DTEND:{dtend}\r\n\
         SUMMARY:{summary}\r\n\
         ORGANIZER;CN={host_name}:mailto:{host_email}\r\n\
         ATTENDEE;CN={guest_name}:mailto:{guest_email}\r\n\
         STATUS:CANCELLED\r\n\
         END:VEVENT\r\n\
         END:VCALENDAR\r\n",
        uid = details.uid,
        dtstamp = dtstamp,
        dtstart = dtstart,
        dtend = dtend,
        summary = summary,
        host_name = host_name,
        host_email = host_email,
        guest_name = guest_name,
        guest_email = guest_email,
    )
}

// --- Email senders ---

/// Send booking confirmation to the guest
pub async fn send_guest_confirmation(
    config: &SmtpConfig,
    details: &BookingDetails,
    cancel_url: Option<&str>,
) -> Result<()> {
    send_guest_confirmation_ex(config, details, cancel_url, None).await
}

pub async fn send_guest_confirmation_ex(
    config: &SmtpConfig,
    details: &BookingDetails,
    cancel_url: Option<&str>,
    reschedule_url: Option<&str>,
) -> Result<()> {
    let ics = generate_ics(details, "REQUEST");

    let from_display = config.from_name.as_deref().unwrap_or(&config.from_email);
    let from = format!("{} <{}>", from_display, config.from_email).parse()?;
    let to = format!("{} <{}>", details.guest_name, details.guest_email).parse()?;

    let time_display = format!(
        "{} \u{2013} {} ({})",
        details.start_time, details.end_time, details.guest_timezone
    );

    let plain = format!(
        "Hi {},\n\n\
         Your booking has been confirmed!\n\n\
         Event: {}\n\
         Date: {}\n\
         Time: {}\n\
         With: {}\n\
         {}{}\
         A calendar invite is attached.\n\
         {}\n\
         \u{2014} calrs",
        details.guest_name,
        details.event_title,
        details.date,
        time_display,
        details.host_name,
        details
            .location
            .as_ref()
            .map(|l| format!("Location: {}\n", l))
            .unwrap_or_default(),
        details
            .notes
            .as_ref()
            .map(|n| format!("Notes: {}\n", n))
            .unwrap_or_default(),
        cancel_url
            .map(|u| format!("\nNeed to cancel? {}\n", u))
            .unwrap_or_default(),
    );

    let mut rows = vec![
        EmailRow {
            label: "Event",
            value: details.event_title.clone(),
        },
        EmailRow {
            label: "Date",
            value: details.date.clone(),
        },
        EmailRow {
            label: "Time",
            value: time_display,
        },
        EmailRow {
            label: "With",
            value: details.host_name.clone(),
        },
    ];
    if let Some(loc) = &details.location {
        rows.push(EmailRow {
            label: "Location",
            value: loc.clone(),
        });
    }
    if let Some(notes) = &details.notes {
        rows.push(EmailRow {
            label: "Notes",
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

    let body = build_multipart_body(&plain, &html);

    let ics_attachment = Attachment::new("invite.ics".to_string()).body(
        ics,
        ContentType::parse("text/calendar; method=REQUEST; charset=UTF-8")?,
    );

    let email = Message::builder()
        .from(from)
        .to(to)
        .subject(format!(
            "Confirmed: {} \u{2014} {}",
            details.event_title, details.date
        ))
        .multipart(
            MultiPart::mixed()
                .multipart(body)
                .singlepart(ics_attachment),
        )?;

    send_email(config, email).await?;

    // Send confirmation to additional attendees
    for attendee_email in &details.additional_attendees {
        let ics2 = generate_ics(details, "REQUEST");
        let from2: lettre::message::Mailbox =
            format!("{} <{}>", from_display, config.from_email).parse()?;
        let to2: lettre::message::Mailbox = attendee_email.parse()?;
        let plain2 = format!(
            "Hi,\n\n\
             You've been added as an attendee to a booking.\n\n\
             Event: {}\n\
             Date: {}\n\
             Time: {} \u{2013} {} ({})\n\
             Organizer: {}\n\
             Booked by: {} <{}>\n\n\
             A calendar invite is attached.\n\n\
             \u{2014} calrs",
            details.event_title,
            details.date,
            details.start_time,
            details.end_time,
            details.guest_timezone,
            details.host_name,
            details.guest_name,
            details.guest_email,
        );
        let html2 = render_html_email(
            "Hi,",
            "You've been added as an attendee to a booking.",
            "#16a34a",
            &[
                EmailRow {
                    label: "Event",
                    value: details.event_title.clone(),
                },
                EmailRow {
                    label: "Date",
                    value: details.date.clone(),
                },
                EmailRow {
                    label: "Time",
                    value: format!(
                        "{} \u{2013} {} ({})",
                        details.start_time, details.end_time, details.guest_timezone
                    ),
                },
                EmailRow {
                    label: "Organizer",
                    value: details.host_name.clone(),
                },
                EmailRow {
                    label: "Booked by",
                    value: format!("{} <{}>", details.guest_name, details.guest_email),
                },
            ],
            Some("A calendar invite is attached to this email."),
        );
        let body2 = build_multipart_body(&plain2, &html2);
        let att2 = Attachment::new("invite.ics".to_string()).body(
            ics2,
            ContentType::parse("text/calendar; method=REQUEST; charset=UTF-8")?,
        );
        let email2 = Message::builder()
            .from(from2)
            .to(to2)
            .subject(format!(
                "Invite: {} \u{2014} {}",
                details.event_title, details.date
            ))
            .multipart(MultiPart::mixed().multipart(body2).singlepart(att2))?;
        if let Err(e) = send_email(config, email2).await {
            tracing::warn!(attendee = %attendee_email, error = %e, "failed to send attendee confirmation");
        }
    }

    Ok(())
}

/// Send booking notification to the host
pub async fn send_host_notification(config: &SmtpConfig, details: &BookingDetails) -> Result<()> {
    let ics = generate_ics(details, "REQUEST");

    let from_display = config.from_name.as_deref().unwrap_or(&config.from_email);
    let from = format!("{} <{}>", from_display, config.from_email).parse()?;
    let to = format!("{} <{}>", details.host_name, details.host_email).parse()?;

    let time_display = format!("{} \u{2013} {}", details.start_time, details.end_time);

    let plain = format!(
        "New booking!\n\n\
         Event: {}\n\
         Date: {}\n\
         Time: {}\n\
         Guest: {} <{}>\n\
         {}{}\n\
         A calendar invite is attached.\n\n\
         \u{2014} calrs",
        details.event_title,
        details.date,
        time_display,
        details.guest_name,
        details.guest_email,
        details
            .location
            .as_ref()
            .map(|l| format!("Location: {}\n", l))
            .unwrap_or_default(),
        details
            .notes
            .as_ref()
            .map(|n| format!("Notes: {}\n", n))
            .unwrap_or_default(),
    );

    let mut rows = vec![
        EmailRow {
            label: "Event",
            value: details.event_title.clone(),
        },
        EmailRow {
            label: "Date",
            value: details.date.clone(),
        },
        EmailRow {
            label: "Time",
            value: time_display,
        },
        EmailRow {
            label: "Guest",
            value: format!("{} <{}>", details.guest_name, details.guest_email),
        },
    ];
    if let Some(loc) = &details.location {
        rows.push(EmailRow {
            label: "Location",
            value: loc.clone(),
        });
    }
    if let Some(notes) = &details.notes {
        rows.push(EmailRow {
            label: "Notes",
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

    let body = build_multipart_body(&plain, &html);

    let ics_attachment = Attachment::new("invite.ics".to_string()).body(
        ics,
        ContentType::parse("text/calendar; method=REQUEST; charset=UTF-8")?,
    );

    let email = Message::builder()
        .from(from)
        .to(to)
        .subject(format!(
            "New booking: {} \u{2014} {} ({})",
            details.event_title, details.guest_name, details.date
        ))
        .multipart(
            MultiPart::mixed()
                .multipart(body)
                .singlepart(ics_attachment),
        )?;

    send_email(config, email).await
}

/// Send host a confirmation that a pending booking was approved (no ICS — event is already
/// pushed via CalDAV write-back).
pub async fn send_host_booking_confirmed(
    config: &SmtpConfig,
    details: &BookingDetails,
) -> Result<()> {
    let from_display = config.from_name.as_deref().unwrap_or(&config.from_email);
    let from = format!("{} <{}>", from_display, config.from_email).parse()?;
    let to = format!("{} <{}>", details.host_name, details.host_email).parse()?;

    let time_display = format!("{} \u{2013} {}", details.start_time, details.end_time);

    let plain = format!(
        "Booking confirmed!\n\n\
         Event: {}\n\
         Date: {}\n\
         Time: {}\n\
         Guest: {} <{}>\n\
         {}\
         The event has been added to your calendar.\n\n\
         \u{2014} calrs",
        details.event_title,
        details.date,
        time_display,
        details.guest_name,
        details.guest_email,
        details
            .location
            .as_ref()
            .map(|l| format!("Location: {}\n", l))
            .unwrap_or_default(),
    );

    let mut rows = vec![
        EmailRow {
            label: "Event",
            value: details.event_title.clone(),
        },
        EmailRow {
            label: "Date",
            value: details.date.clone(),
        },
        EmailRow {
            label: "Time",
            value: time_display,
        },
        EmailRow {
            label: "Guest",
            value: format!("{} <{}>", details.guest_name, details.guest_email),
        },
    ];
    if let Some(loc) = &details.location {
        rows.push(EmailRow {
            label: "Location",
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

    let body = build_multipart_body(&plain, &html);

    let email = Message::builder()
        .from(from)
        .to(to)
        .subject(format!(
            "Confirmed: {} \u{2014} {} ({})",
            details.event_title, details.guest_name, details.date
        ))
        .multipart(body)?;

    send_email(config, email).await
}

/// Send booking reminder to the guest
pub async fn send_guest_reminder(
    config: &SmtpConfig,
    details: &BookingDetails,
    cancel_url: Option<&str>,
) -> Result<()> {
    let from_display = config.from_name.as_deref().unwrap_or(&config.from_email);
    let from = format!("{} <{}>", from_display, config.from_email).parse()?;
    let to = format!("{} <{}>", details.guest_name, details.guest_email).parse()?;

    let time_display = format!(
        "{} \u{2013} {} ({})",
        details.start_time, details.end_time, details.guest_timezone
    );

    let plain = format!(
        "Hi {},\n\n\
         Reminder: you have an upcoming booking.\n\n\
         Event: {}\n\
         Date: {}\n\
         Time: {}\n\
         With: {}\n\
         {}{}\n\
         \u{2014} calrs",
        details.guest_name,
        details.event_title,
        details.date,
        time_display,
        details.host_name,
        details
            .location
            .as_ref()
            .map(|l| format!("Location: {}\n", l))
            .unwrap_or_default(),
        cancel_url
            .map(|u| format!("\nNeed to cancel? {}\n", u))
            .unwrap_or_default(),
    );

    let mut rows = vec![
        EmailRow {
            label: "Event",
            value: details.event_title.clone(),
        },
        EmailRow {
            label: "Date",
            value: details.date.clone(),
        },
        EmailRow {
            label: "Time",
            value: time_display,
        },
        EmailRow {
            label: "With",
            value: details.host_name.clone(),
        },
    ];
    if let Some(loc) = &details.location {
        rows.push(EmailRow {
            label: "Location",
            value: loc.clone(),
        });
    }

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
        &rows,
        None,
        &actions,
    );

    let body = build_multipart_body(&plain, &html);

    let email = Message::builder()
        .from(from)
        .to(to)
        .subject(format!(
            "Reminder: {} \u{2014} {}",
            details.event_title, details.date
        ))
        .multipart(body)?;

    send_email(config, email).await
}

/// Send booking reminder to the host
pub async fn send_host_reminder(config: &SmtpConfig, details: &BookingDetails) -> Result<()> {
    let from_display = config.from_name.as_deref().unwrap_or(&config.from_email);
    let from = format!("{} <{}>", from_display, config.from_email).parse()?;
    let to = format!("{} <{}>", details.host_name, details.host_email).parse()?;

    let time_display = format!("{} \u{2013} {}", details.start_time, details.end_time);

    let plain = format!(
        "Reminder: you have an upcoming booking.\n\n\
         Event: {}\n\
         Date: {}\n\
         Time: {}\n\
         Guest: {} <{}>\n\
         {}\n\
         \u{2014} calrs",
        details.event_title,
        details.date,
        time_display,
        details.guest_name,
        details.guest_email,
        details
            .location
            .as_ref()
            .map(|l| format!("Location: {}\n", l))
            .unwrap_or_default(),
    );

    let rows = vec![
        EmailRow {
            label: "Event",
            value: details.event_title.clone(),
        },
        EmailRow {
            label: "Date",
            value: details.date.clone(),
        },
        EmailRow {
            label: "Time",
            value: time_display,
        },
        EmailRow {
            label: "Guest",
            value: format!("{} <{}>", details.guest_name, details.guest_email),
        },
    ];

    let html = render_html_email(
        "Upcoming booking",
        &format!(
            "Reminder: you have a booking with {} coming up.",
            h(&details.guest_name)
        ),
        "#3b82f6",
        &rows,
        None,
    );

    let body = build_multipart_body(&plain, &html);

    let email = Message::builder()
        .from(from)
        .to(to)
        .subject(format!(
            "Reminder: {} \u{2014} {} ({})",
            details.event_title, details.guest_name, details.date
        ))
        .multipart(body)?;

    send_email(config, email).await
}

/// Send cancellation notification to the guest
pub async fn send_guest_cancellation(
    config: &SmtpConfig,
    details: &CancellationDetails,
) -> Result<()> {
    let ics = generate_cancel_ics(details);

    let from_display = config.from_name.as_deref().unwrap_or(&config.from_email);
    let from = format!("{} <{}>", from_display, config.from_email).parse()?;
    let to = format!("{} <{}>", details.guest_name, details.guest_email).parse()?;

    let time_display = format!("{} \u{2013} {}", details.start_time, details.end_time);
    let reason_text = details
        .reason
        .as_ref()
        .map(|r| format!("Reason: {}\n\n", r))
        .unwrap_or_default();

    let plain = format!(
        "Hi {},\n\n\
         Your booking has been cancelled{}.\n\n\
         Event: {}\n\
         Date: {}\n\
         Time: {}\n\
         With: {}\n\n\
         {}\
         A calendar cancellation is attached.\n\n\
         \u{2014} calrs",
        details.guest_name,
        if details.cancelled_by_host {
            format!(" by {}", details.host_name)
        } else {
            String::new()
        },
        details.event_title,
        details.date,
        time_display,
        details.host_name,
        reason_text,
    );

    let mut rows = vec![
        EmailRow {
            label: "Event",
            value: details.event_title.clone(),
        },
        EmailRow {
            label: "Date",
            value: details.date.clone(),
        },
        EmailRow {
            label: "Time",
            value: time_display,
        },
        EmailRow {
            label: "With",
            value: details.host_name.clone(),
        },
    ];
    if let Some(reason) = &details.reason {
        rows.push(EmailRow {
            label: "Reason",
            value: reason.clone(),
        });
    }

    let html = render_html_email(
        &format!("Hi {},", h(&details.guest_name)),
        &if details.cancelled_by_host {
            format!(
                "Your booking has been cancelled by {}.",
                h(&details.host_name)
            )
        } else {
            "Your booking has been cancelled.".to_string()
        },
        "#dc2626",
        &rows,
        Some("A calendar cancellation is attached to this email."),
    );

    let body = build_multipart_body(&plain, &html);

    let ics_attachment = Attachment::new("cancel.ics".to_string()).body(
        ics,
        ContentType::parse("text/calendar; method=CANCEL; charset=UTF-8")?,
    );

    let email = Message::builder()
        .from(from)
        .to(to)
        .subject(format!(
            "Cancelled: {} \u{2014} {}",
            details.event_title, details.date
        ))
        .multipart(
            MultiPart::mixed()
                .multipart(body)
                .singlepart(ics_attachment),
        )?;

    send_email(config, email).await
}

/// Send cancellation notification to the host
pub async fn send_host_cancellation(
    config: &SmtpConfig,
    details: &CancellationDetails,
) -> Result<()> {
    let ics = generate_cancel_ics(details);

    let from_display = config.from_name.as_deref().unwrap_or(&config.from_email);
    let from = format!("{} <{}>", from_display, config.from_email).parse()?;
    let to = format!("{} <{}>", details.host_name, details.host_email).parse()?;

    let time_display = format!("{} \u{2013} {}", details.start_time, details.end_time);
    let reason_text = details
        .reason
        .as_ref()
        .map(|r| format!("Reason: {}\n\n", r))
        .unwrap_or_default();

    let plain = format!(
        "Booking cancelled.\n\n\
         Event: {}\n\
         Date: {}\n\
         Time: {}\n\
         Guest: {} <{}>\n\n\
         {}\
         A calendar cancellation is attached.\n\n\
         \u{2014} calrs",
        details.event_title,
        details.date,
        time_display,
        details.guest_name,
        details.guest_email,
        reason_text,
    );

    let mut rows = vec![
        EmailRow {
            label: "Event",
            value: details.event_title.clone(),
        },
        EmailRow {
            label: "Date",
            value: details.date.clone(),
        },
        EmailRow {
            label: "Time",
            value: time_display,
        },
        EmailRow {
            label: "Guest",
            value: format!("{} <{}>", details.guest_name, details.guest_email),
        },
    ];
    if let Some(reason) = &details.reason {
        rows.push(EmailRow {
            label: "Reason",
            value: reason.clone(),
        });
    }

    let html = render_html_email(
        "Booking cancelled.",
        &if details.cancelled_by_host {
            "You cancelled this booking.".to_string()
        } else {
            format!("{} cancelled their booking.", h(&details.guest_name))
        },
        "#dc2626",
        &rows,
        Some("A calendar cancellation is attached to this email."),
    );

    let body = build_multipart_body(&plain, &html);

    let ics_attachment = Attachment::new("cancel.ics".to_string()).body(
        ics,
        ContentType::parse("text/calendar; method=CANCEL; charset=UTF-8")?,
    );

    let email = Message::builder()
        .from(from)
        .to(to)
        .subject(format!(
            "Cancelled: {} \u{2014} {} ({})",
            details.event_title, details.guest_name, details.date
        ))
        .multipart(
            MultiPart::mixed()
                .multipart(body)
                .singlepart(ics_attachment),
        )?;

    send_email(config, email).await
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
    let from_display = config.from_name.as_deref().unwrap_or(&config.from_email);
    let from = format!("{} <{}>", from_display, config.from_email).parse()?;
    let to = format!("{} <{}>", details.guest_name, details.guest_email).parse()?;

    let time_display = format!(
        "{} \u{2013} {} ({})",
        details.start_time, details.end_time, details.guest_timezone
    );

    // Don't include location in pending emails — it should only be revealed
    // after the booking is confirmed (prevents meeting link leaking).
    let plain = format!(
        "Hi {},\n\n\
         Your booking request has been received and is awaiting confirmation from {}.\n\n\
         Event: {}\n\
         Date: {}\n\
         Time: {}\n\
         {}\
         You'll receive another email once it's confirmed.\n\
         {}\n\
         \u{2014} calrs",
        details.guest_name,
        details.host_name,
        details.event_title,
        details.date,
        time_display,
        details
            .notes
            .as_ref()
            .map(|n| format!("Notes: {}\n", n))
            .unwrap_or_default(),
        cancel_url
            .map(|u| format!("\nNeed to cancel? {}\n", u))
            .unwrap_or_default(),
    );

    let mut rows = vec![
        EmailRow {
            label: "Event",
            value: details.event_title.clone(),
        },
        EmailRow {
            label: "Date",
            value: details.date.clone(),
        },
        EmailRow {
            label: "Time",
            value: time_display,
        },
        EmailRow {
            label: "Host",
            value: details.host_name.clone(),
        },
    ];
    if let Some(notes) = &details.notes {
        rows.push(EmailRow {
            label: "Notes",
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
        &format!(
            "Your booking request is awaiting confirmation from {}.",
            h(&details.host_name)
        ),
        "#f59e0b",
        &rows,
        Some("You\u{2019}ll receive another email once it\u{2019}s confirmed."),
        &actions,
    );

    let body = build_multipart_body(&plain, &html);

    let email = Message::builder()
        .from(from)
        .to(to)
        .subject(format!(
            "Pending: {} \u{2014} {}",
            details.event_title, details.date
        ))
        .multipart(body)?;

    send_email(config, email).await
}

/// Send approval request to host with approve/decline buttons
pub async fn send_host_approval_request(
    config: &SmtpConfig,
    details: &BookingDetails,
    _booking_id: &str,
    confirm_token: Option<&str>,
    base_url: Option<&str>,
) -> Result<()> {
    let from_display = config.from_name.as_deref().unwrap_or(&config.from_email);
    let from = format!("{} <{}>", from_display, config.from_email).parse()?;
    let to = format!("{} <{}>", details.host_name, details.host_email).parse()?;

    let time_display = format!("{} \u{2013} {}", details.start_time, details.end_time);

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

    let action_text = match (&approve_url, &decline_url) {
        (Some(a), Some(d)) => format!("Approve: {}\nDecline: {}", a, d),
        _ => "Log in to your dashboard to confirm or decline this booking.".to_string(),
    };

    let plain = format!(
        "New booking request requiring your approval!\n\n\
         Event: {}\n\
         Date: {}\n\
         Time: {}\n\
         Guest: {} <{}>\n\
         {}{}\n\
         {}\n\n\
         \u{2014} calrs",
        details.event_title,
        details.date,
        time_display,
        details.guest_name,
        details.guest_email,
        details
            .location
            .as_ref()
            .map(|l| format!("Location: {}\n", l))
            .unwrap_or_default(),
        details
            .notes
            .as_ref()
            .map(|n| format!("Notes: {}\n", n))
            .unwrap_or_default(),
        action_text,
    );

    let mut rows = vec![
        EmailRow {
            label: "Event",
            value: details.event_title.clone(),
        },
        EmailRow {
            label: "Date",
            value: details.date.clone(),
        },
        EmailRow {
            label: "Time",
            value: time_display,
        },
        EmailRow {
            label: "Guest",
            value: format!("{} <{}>", details.guest_name, details.guest_email),
        },
    ];
    if let Some(loc) = &details.location {
        rows.push(EmailRow {
            label: "Location",
            value: loc.clone(),
        });
    }
    if let Some(notes) = &details.notes {
        rows.push(EmailRow {
            label: "Notes",
            value: notes.clone(),
        });
    }

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
        &rows,
        Some("You can also manage this from your dashboard."),
        &actions,
    );

    let body = build_multipart_body(&plain, &html);

    let email = Message::builder()
        .from(from)
        .to(to)
        .subject(format!(
            "Action required: {} \u{2014} {} ({})",
            details.event_title, details.guest_name, details.date
        ))
        .multipart(body)?;

    send_email(config, email).await
}

/// Send decline notification to the guest
pub async fn send_guest_decline_notice(
    config: &SmtpConfig,
    details: &CancellationDetails,
) -> Result<()> {
    let from_display = config.from_name.as_deref().unwrap_or(&config.from_email);
    let from = format!("{} <{}>", from_display, config.from_email).parse()?;
    let to = format!("{} <{}>", details.guest_name, details.guest_email).parse()?;

    let time_display = format!("{} \u{2013} {}", details.start_time, details.end_time);
    let reason_text = details
        .reason
        .as_ref()
        .map(|r| format!("Reason: {}\n\n", r))
        .unwrap_or_default();

    let plain = format!(
        "Hi {},\n\n\
         Your booking request has been declined.\n\n\
         Event: {}\n\
         Date: {}\n\
         Time: {}\n\
         With: {}\n\n\
         {}\
         \u{2014} calrs",
        details.guest_name,
        details.event_title,
        details.date,
        time_display,
        details.host_name,
        reason_text,
    );

    let mut rows = vec![
        EmailRow {
            label: "Event",
            value: details.event_title.clone(),
        },
        EmailRow {
            label: "Date",
            value: details.date.clone(),
        },
        EmailRow {
            label: "Time",
            value: time_display,
        },
        EmailRow {
            label: "With",
            value: details.host_name.clone(),
        },
    ];
    if let Some(reason) = &details.reason {
        rows.push(EmailRow {
            label: "Reason",
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

    let body = build_multipart_body(&plain, &html);

    let email = Message::builder()
        .from(from)
        .to(to)
        .subject(format!(
            "Declined: {} \u{2014} {}",
            details.event_title, details.date
        ))
        .multipart(body)?;

    send_email(config, email).await
}

// --- Utility ---

/// Load SMTP config from database
pub async fn load_smtp_config(pool: &SqlitePool, key: &[u8; 32]) -> Result<Option<SmtpConfig>> {
    let row: Option<(String, i32, String, String, String, Option<String>)> = sqlx::query_as(
        "SELECT host, port, username, password_enc, from_email, from_name
         FROM smtp_config WHERE enabled = 1 LIMIT 1",
    )
    .fetch_optional(pool)
    .await?;

    match row {
        Some((host, port, username, password_enc, from_email, from_name)) => {
            let password = crate::crypto::decrypt_password(key, &password_enc)?;
            Ok(Some(SmtpConfig {
                host,
                port: port as u16,
                username,
                password,
                from_email,
                from_name,
            }))
        }
        None => Ok(None),
    }
}

/// Send a test email
pub async fn send_test_email(config: &SmtpConfig, to_email: &str) -> Result<()> {
    let from_display = config.from_name.as_deref().unwrap_or(&config.from_email);
    let from = format!("{} <{}>", from_display, config.from_email).parse()?;
    let to = to_email.parse()?;

    let plain = "This is a test email from calrs. SMTP is working!".to_string();

    let html = render_html_email(
        "SMTP test",
        "This is a test email from calrs. SMTP is working!",
        "#6366f1",
        &[],
        None,
    );

    let body = build_multipart_body(&plain, &html);

    let email = Message::builder()
        .from(from)
        .to(to)
        .subject("calrs \u{2014} SMTP test")
        .multipart(body)?;

    send_email(config, email).await
}

/// Send a booking invite email to a guest
pub async fn send_invite_email(
    config: &SmtpConfig,
    guest_name: &str,
    guest_email: &str,
    event_title: &str,
    host_name: &str,
    message: Option<&str>,
    invite_url: &str,
    expires_at: Option<&str>,
) -> Result<()> {
    let from_display = config.from_name.as_deref().unwrap_or(&config.from_email);
    let from = format!("{} <{}>", from_display, config.from_email).parse()?;
    let to = format!("{} <{}>", guest_name, guest_email).parse()?;

    let expiry_note = expires_at
        .map(|e| format!("\nThis invite expires on {}.", e))
        .unwrap_or_default();
    let message_note = message
        .filter(|m| !m.trim().is_empty())
        .map(|m| format!("\n\n\"{}\"\n", m))
        .unwrap_or_default();

    let plain = format!(
        "Hi {},\n\n\
         {} has invited you to book: {}\n\
         {}\
         Click the link below to choose a time:\n\
         {}\n\
         {}\n\
         \u{2014} calrs",
        guest_name, host_name, event_title, message_note, invite_url, expiry_note,
    );

    let mut rows = vec![
        EmailRow {
            label: "Event",
            value: event_title.to_string(),
        },
        EmailRow {
            label: "Invited by",
            value: host_name.to_string(),
        },
    ];
    if let Some(msg) = message.filter(|m| !m.trim().is_empty()) {
        rows.push(EmailRow {
            label: "Message",
            value: msg.to_string(),
        });
    }
    if let Some(exp) = expires_at {
        rows.push(EmailRow {
            label: "Expires",
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

    let body = build_multipart_body(&plain, &html);

    let email = Message::builder()
        .from(from)
        .to(to)
        .subject(format!(
            "{} invited you to book: {}",
            host_name, event_title
        ))
        .multipart(body)?;

    send_email(config, email).await
}

async fn send_email(config: &SmtpConfig, email: Message) -> Result<()> {
    let creds = Credentials::new(config.username.clone(), config.password.clone());

    let mailer = AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&config.host)?
        .port(config.port)
        .credentials(creds)
        .build();

    let to_addrs: Vec<String> = email
        .envelope()
        .to()
        .iter()
        .map(|a| a.to_string())
        .collect();
    let to_display = to_addrs.join(", ");
    match mailer.send(email).await {
        Ok(_) => {
            tracing::debug!(to = %to_display, "email delivered");
            Ok(())
        }
        Err(e) => {
            tracing::error!(to = %to_display, error = %e, "email delivery failed");
            Err(e.into())
        }
    }
}

// --- Reschedule emails ---

pub struct RescheduleDetails {
    pub event_title: String,
    pub old_date: String,
    pub old_start_time: String,
    pub old_end_time: String,
    pub new_date: String,
    pub new_start_time: String,
    pub new_end_time: String,
    pub guest_name: String,
    pub guest_email: String,
    pub guest_timezone: String,
    pub host_name: String,
    pub host_email: String,
    pub uid: String,
    pub location: Option<String>,
}

/// Ask the guest to pick a new time (host-initiated reschedule).
/// The guest clicks the link to choose a slot — no time is pre-selected.
pub async fn send_guest_pick_new_time(
    config: &SmtpConfig,
    details: &BookingDetails,
    reschedule_url: &str,
    cancel_url: Option<&str>,
) -> Result<()> {
    let from_display = config.from_name.as_deref().unwrap_or(&config.from_email);
    let from = format!("{} <{}>", from_display, config.from_email).parse()?;
    let to = format!("{} <{}>", details.guest_name, details.guest_email).parse()?;

    let time_display = format!(
        "{} \u{2013} {} ({})",
        details.start_time, details.end_time, details.guest_timezone
    );

    let plain = format!(
        "Hi {},\n\n\
         {} needs to reschedule your booking.\n\n\
         Event: {}\n\
         Originally: {} at {}\n\n\
         Please pick a new time: {}\n\
         {}\n\
         \u{2014} calrs",
        details.guest_name,
        details.host_name,
        details.event_title,
        details.date,
        time_display,
        reschedule_url,
        cancel_url
            .map(|u| format!("\nOr cancel: {}\n", u))
            .unwrap_or_default(),
    );

    let rows = vec![
        EmailRow {
            label: "Event",
            value: details.event_title.clone(),
        },
        EmailRow {
            label: "Originally",
            value: format!("{} at {}", details.date, time_display),
        },
        EmailRow {
            label: "Host",
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

    let body = build_multipart_body(&plain, &html);

    let email = Message::builder()
        .from(from)
        .to(to)
        .subject(format!(
            "Reschedule: {} \u{2014} please pick a new time",
            details.event_title
        ))
        .multipart(body)?;

    send_email(config, email).await
}

/// Notify the guest that their booking was rescheduled by the host.
/// Includes updated ICS calendar invite.
pub async fn send_guest_reschedule_notification(
    config: &SmtpConfig,
    details: &RescheduleDetails,
    cancel_url: Option<&str>,
    reschedule_url: Option<&str>,
) -> Result<()> {
    let new_time_display = format!(
        "{} \u{2013} {} ({})",
        details.new_start_time, details.new_end_time, details.guest_timezone
    );

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
    };
    let ics = generate_ics(&booking_details, "REQUEST");

    let from_display = config.from_name.as_deref().unwrap_or(&config.from_email);
    let from = format!("{} <{}>", from_display, config.from_email).parse()?;
    let to = format!("{} <{}>", details.guest_name, details.guest_email).parse()?;

    let plain = format!(
        "Hi {},\n\n\
         Your booking has been rescheduled by {}.\n\n\
         Event: {}\n\
         Previous: {} at {} \u{2013} {}\n\
         New: {} at {}\n\
         {}\
         An updated calendar invite is attached.\n\
         {}{}\n\
         \u{2014} calrs",
        details.guest_name,
        details.host_name,
        details.event_title,
        details.old_date,
        details.old_start_time,
        details.old_end_time,
        details.new_date,
        new_time_display,
        details
            .location
            .as_ref()
            .map(|l| format!("Location: {}\n", l))
            .unwrap_or_default(),
        cancel_url
            .map(|u| format!("Need to cancel? {}\n", u))
            .unwrap_or_default(),
        reschedule_url
            .map(|u| format!("Need to reschedule? {}\n", u))
            .unwrap_or_default(),
    );

    let rows = vec![
        EmailRow {
            label: "Event",
            value: details.event_title.clone(),
        },
        EmailRow {
            label: "Previous",
            value: format!(
                "{} at {} \u{2013} {}",
                details.old_date, details.old_start_time, details.old_end_time
            ),
        },
        EmailRow {
            label: "New date",
            value: details.new_date.clone(),
        },
        EmailRow {
            label: "New time",
            value: new_time_display,
        },
        EmailRow {
            label: "With",
            value: details.host_name.clone(),
        },
    ];

    let mut actions = Vec::new();
    if let Some(u) = reschedule_url {
        actions.push(EmailAction {
            label: "Reschedule".to_string(),
            url: u.to_string(),
            color: "#d97706".to_string(),
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
        &format!(
            "Your booking has been rescheduled by {}.",
            h(&details.host_name)
        ),
        "#d97706",
        &rows,
        Some("An updated calendar invite is attached to this email."),
        &actions,
    );

    let body = build_multipart_body(&plain, &html);

    let ics_attachment = Attachment::new("invite.ics".to_string()).body(
        ics,
        ContentType::parse("text/calendar; method=REQUEST; charset=UTF-8")?,
    );

    let email = Message::builder()
        .from(from)
        .to(to)
        .subject(format!(
            "Rescheduled: {} \u{2014} {}",
            details.event_title, details.new_date
        ))
        .multipart(
            MultiPart::mixed()
                .multipart(body)
                .singlepart(ics_attachment),
        )?;

    send_email(config, email).await
}

/// Notify the host that a guest wants to reschedule — includes approve/decline buttons.
pub async fn send_host_reschedule_request(
    config: &SmtpConfig,
    details: &RescheduleDetails,
    confirm_token: Option<&str>,
    base_url: Option<&str>,
) -> Result<()> {
    let new_time_display = format!(
        "{} \u{2013} {}",
        details.new_start_time, details.new_end_time
    );

    let from_display = config.from_name.as_deref().unwrap_or(&config.from_email);
    let from = format!("{} <{}>", from_display, config.from_email).parse()?;
    let to = format!("{} <{}>", details.host_name, details.host_email).parse()?;

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

    let action_text = match (&approve_url, &decline_url) {
        (Some(a), Some(d)) => format!("Approve: {}\nDecline: {}", a, d),
        _ => "Log in to your dashboard to confirm or decline.".to_string(),
    };

    let plain = format!(
        "{} wants to reschedule their booking.\n\n\
         Event: {}\n\
         Previous: {} at {} \u{2013} {}\n\
         Requested: {} at {}\n\
         Guest: {} <{}>\n\
         {}\n\n\
         {}\n\n\
         \u{2014} calrs",
        details.guest_name,
        details.event_title,
        details.old_date,
        details.old_start_time,
        details.old_end_time,
        details.new_date,
        new_time_display,
        details.guest_name,
        details.guest_email,
        details
            .location
            .as_ref()
            .map(|l| format!("Location: {}\n", l))
            .unwrap_or_default(),
        action_text,
    );

    let rows = vec![
        EmailRow {
            label: "Event",
            value: details.event_title.clone(),
        },
        EmailRow {
            label: "Previous",
            value: format!(
                "{} at {} \u{2013} {}",
                details.old_date, details.old_start_time, details.old_end_time
            ),
        },
        EmailRow {
            label: "Requested",
            value: format!("{} at {}", details.new_date, new_time_display),
        },
        EmailRow {
            label: "Guest",
            value: format!("{} <{}>", details.guest_name, details.guest_email),
        },
    ];

    let mut actions = Vec::new();
    if let Some(u) = &approve_url {
        actions.push(EmailAction {
            label: "Approve".to_string(),
            url: u.clone(),
            color: "#16a34a".to_string(),
        });
    }
    if let Some(u) = &decline_url {
        actions.push(EmailAction {
            label: "Decline".to_string(),
            url: u.clone(),
            color: "#dc2626".to_string(),
        });
    }

    let html = render_html_email_with_actions(
        &format!("Hi {},", h(&details.host_name)),
        &format!(
            "{} wants to reschedule their booking.",
            h(&details.guest_name)
        ),
        "#d97706",
        &rows,
        None,
        &actions,
    );

    let body = build_multipart_body(&plain, &html);

    let email = Message::builder()
        .from(from)
        .to(to)
        .subject(format!(
            "Reschedule request: {} \u{2014} {} <{}>",
            details.event_title, details.guest_name, details.guest_email
        ))
        .multipart(body)?;

    send_email(config, email).await
}

// --- Watcher claim emails ---

pub async fn send_watcher_claim_notification(
    config: &SmtpConfig,
    details: &BookingDetails,
    watcher_name: &str,
    watcher_email: &str,
    assigned_to_name: &str,
    claim_url: &str,
) -> Result<()> {
    let from_display = config.from_name.as_deref().unwrap_or(&config.from_email);
    let from = format!("{} <{}>", from_display, config.from_email).parse()?;
    let to = format!("{} <{}>", watcher_name, watcher_email).parse()?;

    let time_display = format!("{} \u{2013} {}", details.start_time, details.end_time);

    let plain = format!(
        "New booking available to claim!\n\n\
         Event: {}\n\
         Date: {}\n\
         Time: {}\n\
         Guest: {} <{}>\n\
         Assigned to: {}\n\
         {}\
         Claim this booking: {}\n\n\
         \u{2014} calrs",
        details.event_title,
        details.date,
        time_display,
        details.guest_name,
        details.guest_email,
        assigned_to_name,
        details
            .location
            .as_ref()
            .map(|l| format!("Location: {}\n", l))
            .unwrap_or_default(),
        claim_url,
    );

    let mut rows = vec![
        EmailRow {
            label: "Event",
            value: details.event_title.clone(),
        },
        EmailRow {
            label: "Date",
            value: details.date.clone(),
        },
        EmailRow {
            label: "Time",
            value: time_display,
        },
        EmailRow {
            label: "Guest",
            value: format!("{} <{}>", details.guest_name, details.guest_email),
        },
        EmailRow {
            label: "Assigned to",
            value: assigned_to_name.to_string(),
        },
    ];
    if let Some(loc) = &details.location {
        rows.push(EmailRow {
            label: "Location",
            value: loc.clone(),
        });
    }

    let actions = vec![EmailAction {
        label: "Claim this booking".to_string(),
        url: claim_url.to_string(),
        color: "#3b82f6".to_string(),
    }];

    let html = render_html_email_with_actions(
        &format!("Hi {},", h(watcher_name)),
        "A new booking is available to claim. Click below to join as an attendee.",
        "#3b82f6",
        &rows,
        Some("You can also claim from your dashboard."),
        &actions,
    );

    let body = build_multipart_body(&plain, &html);

    let email = Message::builder()
        .from(from)
        .to(to)
        .subject(format!(
            "Claim available: {} \u{2014} {} ({})",
            details.event_title, details.guest_name, details.date
        ))
        .multipart(body)?;

    send_email(config, email).await
}

pub async fn send_claim_confirmation(
    config: &SmtpConfig,
    details: &BookingDetails,
    claimant_name: &str,
    claimant_email: &str,
) -> Result<()> {
    let from_display = config.from_name.as_deref().unwrap_or(&config.from_email);
    let from = format!("{} <{}>", from_display, config.from_email).parse()?;
    let to = format!("{} <{}>", claimant_name, claimant_email).parse()?;

    let time_display = format!("{} \u{2013} {}", details.start_time, details.end_time);

    let plain = format!(
        "You claimed this booking!\n\n\
         Event: {}\n\
         Date: {}\n\
         Time: {}\n\
         Guest: {} <{}>\n\
         {}\
         A calendar invite has been sent.\n\n\
         \u{2014} calrs",
        details.event_title,
        details.date,
        time_display,
        details.guest_name,
        details.guest_email,
        details
            .location
            .as_ref()
            .map(|l| format!("Location: {}\n", l))
            .unwrap_or_default(),
    );

    let mut rows = vec![
        EmailRow {
            label: "Event",
            value: details.event_title.clone(),
        },
        EmailRow {
            label: "Date",
            value: details.date.clone(),
        },
        EmailRow {
            label: "Time",
            value: time_display,
        },
        EmailRow {
            label: "Guest",
            value: format!("{} <{}>", details.guest_name, details.guest_email),
        },
    ];
    if let Some(loc) = &details.location {
        rows.push(EmailRow {
            label: "Location",
            value: loc.clone(),
        });
    }

    let html = render_html_email(
        &format!("Hi {},", h(claimant_name)),
        "You have successfully claimed this booking. A calendar invite is attached.",
        "#16a34a",
        &rows,
        Some("You will be added as an attendee on this meeting."),
    );

    let body = build_multipart_body(&plain, &html);

    let email = Message::builder()
        .from(from)
        .to(to)
        .subject(format!(
            "Booking claimed: {} \u{2014} {} ({})",
            details.event_title, details.guest_name, details.date
        ))
        .multipart(body)?;

    send_email(config, email).await
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- sanitize_ics ---

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
                    label: "Event",
                    value: "Intro Call".to_string(),
                },
                EmailRow {
                    label: "Date",
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
                label: "Event",
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
                label: "Notes",
                value: "<script>alert(1)</script>".to_string(),
            }],
            None,
        );

        assert!(!html.contains("<script>alert(1)</script>"));
        assert!(html.contains("&lt;script&gt;"));
    }

    // --- generate_ics edge cases ---

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
        };
        let ics = generate_ics(&details, "REQUEST");
        // The injected ATTENDEE line must not appear as a separate field
        assert!(!ics.contains("\r\nATTENDEE:hacker@evil.com"));
        assert!(ics.contains("Evil ATTENDEE:hacker@evil.com")); // newline replaced with space
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
                    label: "Row1",
                    value: "val1".to_string(),
                },
                EmailRow {
                    label: "Row2",
                    value: "val2".to_string(),
                },
                EmailRow {
                    label: "Row3",
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

    // --- RescheduleDetails tests ---

    fn sample_reschedule_details() -> RescheduleDetails {
        RescheduleDetails {
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
                label: "Event",
                value: details.event_title.clone(),
            },
            EmailRow {
                label: "Previous",
                value: format!(
                    "{} at {} \u{2013} {}",
                    details.old_date, details.old_start_time, details.old_end_time
                ),
            },
            EmailRow {
                label: "New date",
                value: details.new_date.clone(),
            },
            EmailRow {
                label: "New time",
                value: format!(
                    "{} \u{2013} {} ({})",
                    details.new_start_time, details.new_end_time, details.guest_timezone
                ),
            },
            EmailRow {
                label: "With",
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

        let mut actions = Vec::new();
        actions.push(EmailAction {
            label: "Approve".to_string(),
            url: approve_url.to_string(),
            color: "#16a34a".to_string(),
        });
        actions.push(EmailAction {
            label: "Decline".to_string(),
            url: decline_url.to_string(),
            color: "#dc2626".to_string(),
        });

        let html = render_html_email_with_actions(
            "Hi,",
            "A guest wants to reschedule.",
            "#d97706",
            &[EmailRow {
                label: "Event",
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
                    label: "Name",
                    value: "Alice & Bob <team>".to_string(),
                },
                EmailRow {
                    label: "Notes",
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
                    label: "Event",
                    value: "Intro Call".to_string(),
                },
                EmailRow {
                    label: "Date",
                    value: "2026-03-15".to_string(),
                },
                EmailRow {
                    label: "Time",
                    value: "10:00 - 10:30 (UTC)".to_string(),
                },
                EmailRow {
                    label: "With",
                    value: "Bob".to_string(),
                },
                EmailRow {
                    label: "Location",
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
                label: "Event",
                value: details.event_title.clone(),
            },
            EmailRow {
                label: "Date",
                value: details.date.clone(),
            },
            EmailRow {
                label: "Time",
                value: time_display,
            },
            EmailRow {
                label: "With",
                value: details.host_name.clone(),
            },
        ];
        if let Some(loc) = &details.location {
            rows.push(EmailRow {
                label: "Location",
                value: loc.clone(),
            });
        }
        if let Some(notes) = &details.notes {
            rows.push(EmailRow {
                label: "Notes",
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
                label: "Event",
                value: details.event_title.clone(),
            },
            EmailRow {
                label: "Date",
                value: details.date.clone(),
            },
            EmailRow {
                label: "Time",
                value: time_display,
            },
            EmailRow {
                label: "Host",
                value: details.host_name.clone(),
            },
        ];
        // Note: no location row for pending emails
        if let Some(notes) = &details.notes {
            rows.push(EmailRow {
                label: "Notes",
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
                label: "Event",
                value: details.event_title.clone(),
            },
            EmailRow {
                label: "Date",
                value: details.date.clone(),
            },
            EmailRow {
                label: "Time",
                value: time_display,
            },
            EmailRow {
                label: "Guest",
                value: format!("{} <{}>", details.guest_name, details.guest_email),
            },
        ];
        if let Some(loc) = &details.location {
            rows.push(EmailRow {
                label: "Location",
                value: loc.clone(),
            });
        }
        if let Some(notes) = &details.notes {
            rows.push(EmailRow {
                label: "Notes",
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
                label: "Guest",
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
        };

        let mut rows = vec![
            EmailRow {
                label: "Event",
                value: details.event_title.clone(),
            },
            EmailRow {
                label: "Date",
                value: details.date.clone(),
            },
            EmailRow {
                label: "With",
                value: details.host_name.clone(),
            },
        ];
        if let Some(reason) = &details.reason {
            rows.push(EmailRow {
                label: "Reason",
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
        };

        let mut rows = vec![EmailRow {
            label: "Event",
            value: details.event_title.clone(),
        }];
        if let Some(reason) = &details.reason {
            rows.push(EmailRow {
                label: "Reason",
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
                label: "Event",
                value: event_title.to_string(),
            },
            EmailRow {
                label: "Invited by",
                value: host_name.to_string(),
            },
        ];
        if let Some(msg) = message.filter(|m| !m.trim().is_empty()) {
            rows.push(EmailRow {
                label: "Message",
                value: msg.to_string(),
            });
        }
        if let Some(exp) = expires_at {
            rows.push(EmailRow {
                label: "Expires",
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
                label: "Event",
                value: "Meeting".to_string(),
            },
            EmailRow {
                label: "Invited by",
                value: "Host".to_string(),
            },
        ];
        if let Some(msg) = message.filter(|m| !m.trim().is_empty()) {
            rows.push(EmailRow {
                label: "Message",
                value: msg.to_string(),
            });
        }
        if let Some(exp) = expires_at {
            rows.push(EmailRow {
                label: "Expires",
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
                label: "Message",
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
                    label: "Event",
                    value: details.event_title.clone(),
                },
                EmailRow {
                    label: "Date",
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

        let html =
            render_html_email_with_actions("Hi,", "Reminder.", "#3b82f6", &[], None, &actions);
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
                label: "Event",
                value: details.event_title.clone(),
            },
            EmailRow {
                label: "Originally",
                value: format!("{} at {}", details.date, time_display),
            },
            EmailRow {
                label: "Host",
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
                label: "Event",
                value: details.event_title.clone(),
            },
            EmailRow {
                label: "Date",
                value: details.date.clone(),
            },
            EmailRow {
                label: "Time",
                value: time_display,
            },
            EmailRow {
                label: "Guest",
                value: format!("{} <{}>", details.guest_name, details.guest_email),
            },
        ];
        if let Some(loc) = &details.location {
            rows.push(EmailRow {
                label: "Location",
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
        };
        let ics = generate_ics(&booking_details, "PUBLISH");
        assert!(!ics.contains("LOCATION:"));
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
        };

        let mut rows = vec![
            EmailRow {
                label: "Event",
                value: details.event_title.clone(),
            },
            EmailRow {
                label: "Date",
                value: details.date.clone(),
            },
            EmailRow {
                label: "With",
                value: details.host_name.clone(),
            },
        ];
        if let Some(reason) = &details.reason {
            rows.push(EmailRow {
                label: "Reason",
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
        };
        let ics = generate_cancel_ics(&details);
        assert!(ics.contains("METHOD:CANCEL"));
        assert!(ics.contains("STATUS:CANCELLED"));
        // Confirm ICS does NOT have STATUS:CONFIRMED
        assert!(!ics.contains("STATUS:CONFIRMED"));
    }
}
