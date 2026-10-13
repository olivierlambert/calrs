//! Shared rendering: HTML and plain-text layout, ICS generation, time conversion, email parts.

use super::*;

// --- HTML email template helpers ---

pub(super) fn h(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

pub(super) struct EmailRow {
    pub(super) label: String,
    pub(super) value: String,
}

pub(super) struct EmailAction {
    pub(super) label: String,
    pub(super) url: String,
    pub(super) color: String,
}

pub(super) fn render_html_email(
    greeting: &str,
    message: &str,
    accent: &str,
    rows: &[EmailRow],
    footer_note: Option<&str>,
) -> String {
    render_html_email_in("en", greeting, message, accent, rows, footer_note, &[])
}

#[cfg(test)]
pub(super) fn render_html_email_with_actions(
    greeting: &str,
    message: &str,
    accent: &str,
    rows: &[EmailRow],
    footer_note: Option<&str>,
    actions: &[EmailAction],
) -> String {
    render_html_email_in("en", greeting, message, accent, rows, footer_note, actions)
}

/// Render the HTML body for an email written in `lang`. The tag ends up in
/// `<html lang>`, so mail clients pick the right spell checker and screen
/// readers the right voice (#209). Only the admin SMTP test email still goes
/// through the English wrapper above.
pub(super) fn render_html_email_in(
    lang: &str,
    greeting: &str,
    message: &str,
    accent: &str,
    rows: &[EmailRow],
    footer_note: Option<&str>,
    actions: &[EmailAction],
) -> String {
    let sent_by = h(&t(lang, "email-footer-sent-by"));
    let lang = h(crate::i18n::html_lang(lang));
    let mut detail_rows = String::new();
    for (i, row) in rows.iter().enumerate() {
        let bg = if i % 2 == 0 { "#f8f9fa" } else { "#ffffff" };
        detail_rows.push_str(&format!(
            "<tr>\
               <td style=\"padding:8px 12px;color:#6b7280;font-size:13px;white-space:nowrap;vertical-align:top;\">{}</td>\
               <td style=\"padding:8px 12px;color:#111827;font-size:14px;background:{bg};\">{}</td>\
             </tr>",
            h(&row.label), h(&row.value),
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
<html lang="{lang}">
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
      <span style="font-size:12px;color:#9ca3af;">{sent_by} </span>
      <a href="https://cal.rs" style="font-size:12px;color:#6b7280;font-weight:600;text-decoration:none;">calrs</a>
    </td></tr>
  </table>
</td></tr>
</table>
</body>
</html>"##
    )
}

pub(super) fn build_multipart_body(plain: &str, html: &str) -> MultiPart {
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
pub(super) fn sanitize_ics(value: &str) -> String {
    value
        .replace('\r', "")
        .replace('\n', " ")
        .replace(';', "\\;")
        .replace(',', "\\,")
}

/// Sanitize a value for use in an ICS *parameter*, such as `CN=`.
///
/// A parameter value is not a TEXT value and takes no backslash escapes.
/// RFC 5545 §3.1 defines it as either `paramtext`, which excludes `;`, `:`,
/// `,` and DQUOTE, or a quoted-string. Escaping a semicolon as `\;` the way
/// `sanitize_ics` does leaves the raw `;` in place, a strict parser reads it
/// as the end of the parameter, and the whole VEVENT is rejected: Yandex 360
/// answers 400 Bad Request to a booking whose guest name contains one, and
/// the event never reaches the calendar (#163).
///
/// So quote the value when it carries a delimiter, and spell the characters a
/// quoted-string cannot hold with the RFC 6868 caret escapes. The carets only
/// appear for input containing `"`, `^` or a newline; a parser predating
/// RFC 6868 renders those literally, which is a cosmetic loss in a display
/// name rather than the parse failure this replaces.
pub(super) fn sanitize_ics_param(value: &str) -> String {
    let escaped = value
        .replace('^', "^^")
        .replace('"', "^'")
        .replace("\r\n", "^n")
        .replace(['\r', '\n'], "^n");
    if escaped.contains([';', ':', ',']) {
        format!("\"{escaped}\"")
    } else {
        escaped
    }
}

/// Convert date + start/end times from a guest timezone to UTC ICS format (YYYYMMDDTHHMMSSZ).
/// Falls back to floating time (no Z) if timezone parsing fails.
pub(super) fn convert_to_utc(
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

/// Convert a (date, start_time, end_time) tuple from `from_tz` into `to_tz`,
/// returning the wall-clock equivalents.
///
/// Returns `(date, start_time, end_time)` formatted as `YYYY-MM-DD` / `HH:MM`.
/// The date can roll over (e.g. LA 22:00 → Paris 07:00 next day), so the
/// returned date may differ from the input. If either timezone fails to parse
/// or the local time is invalid (DST gap), returns `None` and the caller
/// should fall back to displaying the original values.
pub(super) fn convert_time_between_tz(
    date: &str,
    start_time: &str,
    end_time: &str,
    from_tz: &str,
    to_tz: &str,
) -> Option<(String, String, String)> {
    use chrono::TimeZone;

    let from: Tz = from_tz.parse().ok()?;
    let to: Tz = to_tz.parse().ok()?;

    let start_naive =
        NaiveDateTime::parse_from_str(&format!("{} {}:00", date, start_time), "%Y-%m-%d %H:%M:%S")
            .ok()?;
    let end_naive =
        NaiveDateTime::parse_from_str(&format!("{} {}:00", date, end_time), "%Y-%m-%d %H:%M:%S")
            .ok()?;

    let start_target = from
        .from_local_datetime(&start_naive)
        .earliest()?
        .with_timezone(&to);
    let end_target = from
        .from_local_datetime(&end_naive)
        .earliest()?
        .with_timezone(&to);

    Some((
        start_target.format("%Y-%m-%d").to_string(),
        start_target.format("%H:%M").to_string(),
        end_target.format("%H:%M").to_string(),
    ))
}

/// Build the date + time strings to display in a host-targeted email.
///
/// When `host_timezone` is set and differs from `guest_timezone`, the times are
/// converted into the host's zone. Otherwise the original guest-zone values are
/// kept. The returned `time_display` always includes a `(TZ)` suffix so the
/// host can tell which zone they're looking at.
pub(crate) fn host_time_display(
    date: &str,
    start_time: &str,
    end_time: &str,
    guest_timezone: &str,
    host_timezone: &str,
) -> (String, String) {
    if !host_timezone.is_empty() && host_timezone != guest_timezone {
        if let Some((host_date, host_start, host_end)) =
            convert_time_between_tz(date, start_time, end_time, guest_timezone, host_timezone)
        {
            return (
                host_date,
                format!("{} \u{2013} {} ({})", host_start, host_end, host_timezone),
            );
        }
    }

    let tz_label = if !guest_timezone.is_empty() {
        guest_timezone
    } else if !host_timezone.is_empty() {
        host_timezone
    } else {
        ""
    };

    let time_display = if tz_label.is_empty() {
        format!("{} \u{2013} {}", start_time, end_time)
    } else {
        format!("{} \u{2013} {} ({})", start_time, end_time, tz_label)
    };

    (date.to_string(), time_display)
}

/// New rows carry exact endpoints; do not reconstruct an ambiguous guest
/// wall clock or assume the end falls on the start date.
pub(super) fn host_time_display_exact(
    date: &str,
    start_time: &str,
    end_time: &str,
    guest_timezone: &str,
    host_timezone: &str,
    utc_times: Option<&(String, String)>,
) -> (String, String) {
    let zone = if host_timezone.is_empty() {
        guest_timezone
    } else {
        host_timezone
    };
    if let (Some((start, end)), Ok(tz)) = (utc_times, zone.parse::<chrono_tz::Tz>()) {
        let parse = |v: &str| {
            chrono::NaiveDateTime::parse_from_str(v, "%Y%m%dT%H%M%SZ")
                .ok()
                .map(|v| v.and_utc().with_timezone(&tz))
        };
        if let (Some(start), Some(end)) = (parse(start), parse(end)) {
            return (
                start.format("%Y-%m-%d").to_string(),
                format!(
                    "{} – {} ({})",
                    start.format("%H:%M"),
                    end.format("%H:%M"),
                    zone
                ),
            );
        }
    }
    host_time_display(date, start_time, end_time, guest_timezone, host_timezone)
}

/// Generate an .ics VCALENDAR string for a booking
/// Extract first name (first word) from a full name.
pub(super) fn first_name(full_name: &str) -> &str {
    full_name.split_whitespace().next().unwrap_or(full_name)
}

pub fn generate_ics(details: &BookingDetails, method: &str) -> String {
    generate_ics_impl(details, method, false, false)
}

/// ICS for CalDAV write-back (RFC 4791: no METHOD). ATTENDEE lines carry
/// SCHEDULE-AGENT=CLIENT (RFC 6638 §7.1) so the CalDAV server does not
/// run its own scheduling: calrs already sends the invitations over SMTP,
/// and servers like Fastmail would otherwise send a duplicate iMIP invite
/// (in UTC) for every written booking. Email .ics attachments keep plain
/// ATTENDEE;RSVP=TRUE so mail clients still offer "Add to calendar".
pub fn generate_ics_caldav(details: &BookingDetails) -> String {
    generate_ics_impl(details, "", true, false)
}

/// Personal Google calendar copy: calrs sends the guest invitations via SMTP.
/// Google can still send invitations despite SCHEDULE-AGENT=CLIENT, so omit
/// attendees altogether. Omit ORGANIZER too: Google assigns the calendar's
/// identity instead of treating the booking email alias as an external organizer,
/// which prevents native Meet creation. Guest-facing ICS retains that alias.
pub fn generate_ics_google_caldav(details: &BookingDetails) -> String {
    generate_ics_impl(details, "", true, true)
}

fn generate_ics_impl(
    details: &BookingDetails,
    method: &str,
    schedule_agent_client: bool,
    host_only: bool,
) -> String {
    let guest_first = first_name(&details.guest_name);
    let host_first = first_name(&details.host_name);
    let summary = sanitize_ics(&format!(
        "{} \u{2014} {} & {}",
        details.event_title, guest_first, host_first
    ));
    // CN= is a parameter, not a TEXT value: it needs quoting, not backslashes.
    let host_name = sanitize_ics_param(&details.host_name);
    let guest_name = sanitize_ics_param(&details.guest_name);
    let host_email = sanitize_ics(&details.host_email);
    let guest_email = sanitize_ics(&details.guest_email);
    let location_line = details
        .location
        .as_ref()
        .map(|l| format!("LOCATION:{}\r\n", sanitize_ics(l)))
        .unwrap_or_default();
    // The phone goes only into the host's own calendar copy (#261): an email
    // attachment also reaches additional attendees, who have no use for it.
    let phone = details
        .guest_phone
        .as_ref()
        .filter(|p| schedule_agent_client && !p.trim().is_empty())
        .map(|p| {
            sanitize_ics(&format!(
                "{} {}",
                t(host_lang(details), "email-detail-phone"),
                p
            ))
        });
    let notes = details
        .notes
        .as_ref()
        .filter(|n| !n.trim().is_empty())
        .map(|n| sanitize_ics(n));
    let description: Vec<String> = if host_only {
        let mut description = vec![format!("{} <{}>", details.guest_name, details.guest_email)];
        description.extend(details.additional_attendees.iter().cloned());
        let mut description = description
            .iter()
            .map(|line| sanitize_ics(&line.replace('\\', "\\\\")))
            .collect::<Vec<_>>();
        description.extend(phone);
        if let Some(notes) = details.notes.as_ref().filter(|n| !n.trim().is_empty()) {
            description.push(String::new());
            description.extend(
                notes
                    .split('\n')
                    .map(|line| sanitize_ics(&line.replace('\\', "\\\\"))),
            );
        }
        description
    } else {
        phone.into_iter().chain(notes).collect()
    };
    let description_line = if description.is_empty() {
        String::new()
    } else {
        // `\n` is the escaped line break of an RFC 5545 TEXT value.
        format!("DESCRIPTION:{}\r\n", description.join("\\n"))
    };
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
    let sa = if schedule_agent_client {
        ";SCHEDULE-AGENT=CLIENT"
    } else {
        ""
    };
    let scheduling_lines = if host_only {
        String::new()
    } else {
        let additional_attendee_lines: String = details
            .additional_attendees
            .iter()
            .map(|email| format!("ATTENDEE{sa};RSVP=TRUE:mailto:{}\r\n", sanitize_ics(email)))
            .collect();
        format!(
            "ORGANIZER;CN={host_name}:mailto:{host_email}\r\n\
             ATTENDEE{sa};CN={guest_name};RSVP=TRUE:mailto:{guest_email}\r\n\
             {additional_attendee_lines}"
        )
    };
    let dtstamp = chrono::Utc::now().format("%Y%m%dT%H%M%SZ").to_string();
    // Convert guest-timezone times to UTC for the ICS
    let (dtstart, dtend) = details.utc_times.clone().unwrap_or_else(|| {
        convert_to_utc(
            &details.date,
            &details.start_time,
            &details.end_time,
            &details.guest_timezone,
        )
    });
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
         {scheduling_lines}\
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
        scheduling_lines = scheduling_lines,
    )
}

/// Generate an .ics VCALENDAR for cancellation (METHOD:CANCEL)
pub(super) fn generate_cancel_ics(details: &CancellationDetails) -> String {
    let guest_first = first_name(&details.guest_name);
    let host_first = first_name(&details.host_name);
    let summary = sanitize_ics(&format!(
        "{} \u{2014} {} & {}",
        details.event_title, guest_first, host_first
    ));
    // CN= is a parameter, not a TEXT value: it needs quoting, not backslashes.
    let host_name = sanitize_ics_param(&details.host_name);
    let guest_name = sanitize_ics_param(&details.guest_name);
    let host_email = sanitize_ics(&details.host_email);
    let guest_email = sanitize_ics(&details.guest_email);
    let dtstamp = chrono::Utc::now().format("%Y%m%dT%H%M%SZ").to_string();
    let (dtstart, dtend) = details.utc_times.clone().unwrap_or_else(|| {
        convert_to_utc(
            &details.date,
            &details.start_time,
            &details.end_time,
            &details.guest_timezone,
        )
    });
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

// --- Composition shared by the translated emails ---

pub(super) fn row(label: String, value: impl Into<String>) -> EmailRow {
    EmailRow {
        label,
        value: value.into(),
    }
}

/// Append a row only when the value is there.
pub(super) fn push_row(rows: &mut Vec<EmailRow>, lang: &str, key: &str, value: Option<&String>) {
    if let Some(v) = value {
        rows.push(row(t(lang, key), v.clone()));
    }
}

/// Event, date, time and guest: the rows every host-facing email opens with.
pub(super) fn host_rows(
    lang: &str,
    event: &str,
    date: &str,
    time: &str,
    guest_name: &str,
    guest_email: &str,
) -> Vec<EmailRow> {
    vec![
        row(t(lang, "confirmed-detail-event"), event),
        row(t(lang, "confirmed-detail-date"), date),
        row(t(lang, "confirmed-detail-time"), time),
        row(
            t(lang, "common-detail-guest"),
            format!("{} <{}>", guest_name, guest_email),
        ),
    ]
}

/// "date at time", for rows that carry both in one value.
pub(super) fn date_at_time(lang: &str, date: &str, time: &str) -> String {
    ta(lang, "email-date-at-time", [("date", date), ("time", time)])
}

/// Approve and decline links for a pending booking, when a token and a
/// public base URL are both known.
pub(super) fn approval_urls(
    confirm_token: Option<&str>,
    base_url: Option<&str>,
) -> Option<(String, String)> {
    let (token, url) = (confirm_token?, base_url?);
    let base = url.trim_end_matches('/');
    Some((
        format!("{}/booking/approve/{}", base, token),
        format!("{}/booking/decline/{}", base, token),
    ))
}

/// Plain-text lines and HTML buttons for approve/decline, or the dashboard
/// hint when the email cannot carry links.
pub(super) fn approval_actions(
    lang: &str,
    urls: Option<(String, String)>,
) -> (Vec<String>, Vec<EmailAction>) {
    match urls {
        Some((approve, decline)) => (
            vec![
                ta(lang, "email-host-approve-line", [("url", &approve)]),
                ta(lang, "email-host-decline-line", [("url", &decline)]),
            ],
            vec![
                EmailAction {
                    label: t(lang, "email-action-approve"),
                    url: approve,
                    color: "#16a34a".to_string(),
                },
                EmailAction {
                    label: t(lang, "email-action-decline"),
                    url: decline,
                    color: "#dc2626".to_string(),
                },
            ],
        ),
        None => (vec![t(lang, "email-host-approval-dashboard")], vec![]),
    }
}

/// An email made of a heading, one message, a details table, optional
/// buttons and a closing note, all in `lang`. The plain-text body carries the
/// same content, with `plain_lines` (the URLs the HTML shows as buttons)
/// after the details.
pub(super) struct EmailParts<'a> {
    pub(super) lang: &'a str,
    pub(super) subject: String,
    pub(super) heading: String,
    pub(super) message: String,
    pub(super) accent: &'a str,
    pub(super) rows: Vec<EmailRow>,
    pub(super) plain_lines: Vec<String>,
    pub(super) footer_plain: Option<String>,
    pub(super) footer_html: Option<String>,
    pub(super) actions: Vec<EmailAction>,
}

impl EmailParts<'_> {
    pub(super) fn compose(self) -> ComposedEmail {
        let mut plain = format!("{}\n\n{}\n\n", self.heading, self.message);
        for r in &self.rows {
            plain.push_str(&format!("{} {}\n", r.label, r.value));
        }
        plain.push('\n');
        if !self.plain_lines.is_empty() {
            plain.push_str(&self.plain_lines.join("\n"));
            plain.push_str("\n\n");
        }
        if let Some(note) = &self.footer_plain {
            plain.push_str(note);
            plain.push_str("\n\n");
        }
        plain.push_str(&t(self.lang, "email-signature"));
        let html = render_html_email_in(
            self.lang,
            &h(&self.heading),
            &h(&self.message),
            self.accent,
            &self.rows,
            self.footer_html.as_deref(),
            &self.actions,
        );
        ComposedEmail {
            subject: self.subject,
            plain,
            html,
        }
    }
}

/// Send a composed email, with an optional `(filename, ics, method)`
/// calendar attachment.
pub(super) async fn send_composed(
    config: &SmtpConfig,
    sender: Sender<'_>,
    to: Mailbox,
    mail: ComposedEmail,
    ics: Option<(&str, String, &str)>,
) -> Result<()> {
    let body = build_multipart_body(&mail.plain, &mail.html);
    let builder = config.message_from(sender)?.to(to).subject(mail.subject);
    let email = match ics {
        Some((filename, ics, method)) => {
            let attachment = Attachment::new(filename.to_string()).body(
                ics,
                ContentType::parse(&format!("text/calendar; method={method}; charset=UTF-8"))?,
            );
            builder.multipart(MultiPart::mixed().multipart(body).singlepart(attachment))?
        }
        None => builder.multipart(body)?,
    };
    send_email(config, email).await
}

/// Subject and bodies of an email, built apart from sending so tests can
/// read exactly what the recipient would get.
pub(super) struct ComposedEmail {
    pub(super) subject: String,
    pub(super) plain: String,
    pub(super) html: String,
}

/// "[status] event, date" subject for guest emails that report a state
/// rather than carry an invite, built from the translated status label and
/// the same neutral "event, date" pair the confirmation uses.
pub(super) fn status_subject(lang: &str, status_key: &str, event: &str, date: &str) -> String {
    format!(
        "[{}] {}",
        t(lang, status_key),
        ta(
            lang,
            "email-confirm-subject",
            [("event", event), ("date", date)]
        )
    )
}
