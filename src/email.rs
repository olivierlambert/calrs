use anyhow::Result;
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
}

pub struct CancellationDetails {
    pub event_title: String,
    pub date: String,
    pub start_time: String,
    pub end_time: String,
    pub guest_name: String,
    pub guest_email: String,
    pub host_name: String,
    pub host_email: String,
    pub uid: String,
    pub reason: Option<String>,
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
      <span style="font-size:12px;color:#6b7280;font-weight:600;">calrs</span>
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

/// Generate an .ics VCALENDAR string for a booking
pub fn generate_ics(details: &BookingDetails, method: &str) -> String {
    let summary = sanitize_ics(&details.event_title);
    let host_name = sanitize_ics(&details.host_name);
    let guest_name = sanitize_ics(&details.guest_name);
    let host_email = sanitize_ics(&details.host_email);
    let guest_email = sanitize_ics(&details.guest_email);
    let location_line = details
        .location
        .as_ref()
        .map(|l| format!("LOCATION:{}\r\n         ", sanitize_ics(l)))
        .unwrap_or_default();
    format!(
        "BEGIN:VCALENDAR\r\n\
         VERSION:2.0\r\n\
         PRODID:-//calrs//calrs//EN\r\n\
         METHOD:{method}\r\n\
         BEGIN:VEVENT\r\n\
         UID:{uid}\r\n\
         DTSTART:{dtstart}\r\n\
         DTEND:{dtend}\r\n\
         SUMMARY:{summary}\r\n\
         {location_line}\
         ORGANIZER;CN={host_name}:mailto:{host_email}\r\n\
         ATTENDEE;CN={guest_name};RSVP=TRUE:mailto:{guest_email}\r\n\
         STATUS:CONFIRMED\r\n\
         END:VEVENT\r\n\
         END:VCALENDAR\r\n",
        method = method,
        uid = details.uid,
        dtstart = details.date.replace('-', "").to_string()
            + "T"
            + &details.start_time.replace(':', "")
            + "00",
        dtend = details.date.replace('-', "").to_string()
            + "T"
            + &details.end_time.replace(':', "")
            + "00",
        summary = summary,
        host_name = host_name,
        host_email = host_email,
        guest_name = guest_name,
        guest_email = guest_email,
    )
}

/// Generate an .ics VCALENDAR for cancellation (METHOD:CANCEL)
fn generate_cancel_ics(details: &CancellationDetails) -> String {
    let summary = sanitize_ics(&details.event_title);
    let host_name = sanitize_ics(&details.host_name);
    let guest_name = sanitize_ics(&details.guest_name);
    let host_email = sanitize_ics(&details.host_email);
    let guest_email = sanitize_ics(&details.guest_email);
    format!(
        "BEGIN:VCALENDAR\r\n\
         VERSION:2.0\r\n\
         PRODID:-//calrs//calrs//EN\r\n\
         METHOD:CANCEL\r\n\
         BEGIN:VEVENT\r\n\
         UID:{uid}\r\n\
         DTSTART:{dtstart}\r\n\
         DTEND:{dtend}\r\n\
         SUMMARY:{summary}\r\n\
         ORGANIZER;CN={host_name}:mailto:{host_email}\r\n\
         ATTENDEE;CN={guest_name}:mailto:{guest_email}\r\n\
         STATUS:CANCELLED\r\n\
         END:VEVENT\r\n\
         END:VCALENDAR\r\n",
        uid = details.uid,
        dtstart = details.date.replace('-', "").to_string()
            + "T"
            + &details.start_time.replace(':', "")
            + "00",
        dtend = details.date.replace('-', "").to_string()
            + "T"
            + &details.end_time.replace(':', "")
            + "00",
        summary = summary,
        host_name = host_name,
        host_email = host_email,
        guest_name = guest_name,
        guest_email = guest_email,
    )
}

// --- Email senders ---

/// Send booking confirmation to the guest
pub async fn send_guest_confirmation(config: &SmtpConfig, details: &BookingDetails) -> Result<()> {
    let ics = generate_ics(details, "PUBLISH");

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
         {}{}\n\
         A calendar invite is attached.\n\n\
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

    let html = render_html_email(
        &format!("Hi {},", h(&details.guest_name)),
        "Your booking has been confirmed!",
        "#16a34a",
        &rows,
        Some("A calendar invite is attached to this email."),
    );

    let body = build_multipart_body(&plain, &html);

    let ics_attachment = Attachment::new("invite.ics".to_string()).body(
        ics,
        ContentType::parse("text/calendar; method=PUBLISH; charset=UTF-8")?,
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

    send_email(config, email).await
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
         Your booking has been cancelled.\n\n\
         Event: {}\n\
         Date: {}\n\
         Time: {}\n\
         With: {}\n\n\
         {}\
         A calendar cancellation is attached.\n\n\
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
        "Your booking has been cancelled.",
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
        &format!("{} cancelled their booking.", h(&details.guest_name)),
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
         Your booking request has been received and is awaiting confirmation from {}.\n\n\
         Event: {}\n\
         Date: {}\n\
         Time: {}\n\
         {}{}\n\
         You'll receive another email once it's confirmed.\n\n\
         \u{2014} calrs",
        details.guest_name,
        details.host_name,
        details.event_title,
        details.date,
        time_display,
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
            label: "Host",
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

    let html = render_html_email(
        &format!("Hi {},", h(&details.guest_name)),
        &format!(
            "Your booking request is awaiting confirmation from {}.",
            h(&details.host_name)
        ),
        "#f59e0b",
        &rows,
        Some("You\u{2019}ll receive another email once it\u{2019}s confirmed."),
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

async fn send_email(config: &SmtpConfig, email: Message) -> Result<()> {
    let creds = Credentials::new(config.username.clone(), config.password.clone());

    let mailer = AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&config.host)?
        .port(config.port)
        .credentials(creds)
        .build();

    mailer.send(email).await?;
    Ok(())
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
        };

        let ics = generate_ics(&details, "PUBLISH");
        assert!(ics.contains("BEGIN:VCALENDAR"));
        assert!(ics.contains("END:VCALENDAR"));
        assert!(ics.contains("METHOD:PUBLISH"));
        assert!(ics.contains("BEGIN:VEVENT"));
        assert!(ics.contains("END:VEVENT"));
        assert!(ics.contains("UID:test-uid-123"));
        assert!(ics.contains("DTSTART:20260310T140000"));
        assert!(ics.contains("DTEND:20260310T143000"));
        assert!(ics.contains("SUMMARY:Intro Call"));
        assert!(ics.contains("ORGANIZER;CN=Alice:mailto:alice@cal.rs"));
        assert!(ics.contains("ATTENDEE;CN=Jane Doe;RSVP=TRUE:mailto:jane@example.com"));
        assert!(ics.contains("STATUS:CONFIRMED"));
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
        };

        let ics = generate_ics(&details, "REQUEST");
        assert!(ics.contains("METHOD:REQUEST"));
        assert!(ics.contains("LOCATION:https://meet.example.com/room"));
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
        };

        let ics = generate_ics(&details, "PUBLISH");
        assert!(ics.contains("SUMMARY:Meet\\; discuss\\, plan"));
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
}
