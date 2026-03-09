use anyhow::Result;
use sqlx::SqlitePool;
use lettre::message::header::ContentType;
use lettre::message::{Attachment, MultiPart, SinglePart};
use lettre::transport::smtp::authentication::Credentials;
use lettre::{AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};

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
}

/// Generate an .ics VCALENDAR string for a booking
fn generate_ics(details: &BookingDetails, method: &str) -> String {
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
         ORGANIZER;CN={host_name}:mailto:{host_email}\r\n\
         ATTENDEE;CN={guest_name};RSVP=TRUE:mailto:{guest_email}\r\n\
         STATUS:CONFIRMED\r\n\
         END:VEVENT\r\n\
         END:VCALENDAR\r\n",
        method = method,
        uid = details.uid,
        dtstart = details.date.replace('-', "").to_string() + "T" + &details.start_time.replace(':', "") + "00",
        dtend = details.date.replace('-', "").to_string() + "T" + &details.end_time.replace(':', "") + "00",
        summary = details.event_title,
        host_name = details.host_name,
        host_email = details.host_email,
        guest_name = details.guest_name,
        guest_email = details.guest_email,
    )
}

/// Send booking confirmation to the guest
pub async fn send_guest_confirmation(config: &SmtpConfig, details: &BookingDetails) -> Result<()> {
    let ics = generate_ics(details, "REQUEST");

    let from_display = config.from_name.as_deref().unwrap_or(&config.from_email);
    let from = format!("{} <{}>", from_display, config.from_email).parse()?;
    let to = format!("{} <{}>", details.guest_name, details.guest_email).parse()?;

    let body = format!(
        "Hi {},\n\n\
         Your booking has been confirmed!\n\n\
         Event: {}\n\
         Date: {}\n\
         Time: {} – {} ({})\n\
         With: {}\n\
         {}\n\
         You should find a calendar invite attached to this email.\n\n\
         — calrs",
        details.guest_name,
        details.event_title,
        details.date,
        details.start_time,
        details.end_time,
        details.guest_timezone,
        details.host_name,
        details.notes.as_ref().map(|n| format!("Notes: {}\n", n)).unwrap_or_default(),
    );

    let ics_attachment = Attachment::new("invite.ics".to_string())
        .body(ics, ContentType::parse("text/calendar; method=REQUEST; charset=UTF-8")?);

    let email = Message::builder()
        .from(from)
        .to(to)
        .subject(format!("Confirmed: {} — {}", details.event_title, details.date))
        .multipart(
            MultiPart::mixed()
                .singlepart(SinglePart::plain(body))
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

    let body = format!(
        "New booking!\n\n\
         Event: {}\n\
         Date: {}\n\
         Time: {} – {}\n\
         Guest: {} <{}>\n\
         {}\n\
         A calendar invite is attached.\n\n\
         — calrs",
        details.event_title,
        details.date,
        details.start_time,
        details.end_time,
        details.guest_name,
        details.guest_email,
        details.notes.as_ref().map(|n| format!("Notes: {}\n", n)).unwrap_or_default(),
    );

    let ics_attachment = Attachment::new("invite.ics".to_string())
        .body(ics, ContentType::parse("text/calendar; method=REQUEST; charset=UTF-8")?);

    let email = Message::builder()
        .from(from)
        .to(to)
        .subject(format!("New booking: {} — {} ({})", details.event_title, details.guest_name, details.date))
        .multipart(
            MultiPart::mixed()
                .singlepart(SinglePart::plain(body))
                .singlepart(ics_attachment),
        )?;

    send_email(config, email).await
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

/// Send cancellation notification to the guest
pub async fn send_guest_cancellation(config: &SmtpConfig, details: &CancellationDetails) -> Result<()> {
    let ics = generate_cancel_ics(details);

    let from_display = config.from_name.as_deref().unwrap_or(&config.from_email);
    let from = format!("{} <{}>", from_display, config.from_email).parse()?;
    let to = format!("{} <{}>", details.guest_name, details.guest_email).parse()?;

    let reason_text = details.reason.as_ref()
        .map(|r| format!("Reason: {}\n\n", r))
        .unwrap_or_default();

    let body = format!(
        "Hi {},\n\n\
         Your booking has been cancelled.\n\n\
         Event: {}\n\
         Date: {}\n\
         Time: {} – {}\n\
         With: {}\n\n\
         {}\
         A calendar cancellation is attached.\n\n\
         — calrs",
        details.guest_name,
        details.event_title,
        details.date,
        details.start_time,
        details.end_time,
        details.host_name,
        reason_text,
    );

    let ics_attachment = Attachment::new("cancel.ics".to_string())
        .body(ics, ContentType::parse("text/calendar; method=CANCEL; charset=UTF-8")?);

    let email = Message::builder()
        .from(from)
        .to(to)
        .subject(format!("Cancelled: {} — {}", details.event_title, details.date))
        .multipart(
            MultiPart::mixed()
                .singlepart(SinglePart::plain(body))
                .singlepart(ics_attachment),
        )?;

    send_email(config, email).await
}

/// Send cancellation notification to the host
pub async fn send_host_cancellation(config: &SmtpConfig, details: &CancellationDetails) -> Result<()> {
    let ics = generate_cancel_ics(details);

    let from_display = config.from_name.as_deref().unwrap_or(&config.from_email);
    let from = format!("{} <{}>", from_display, config.from_email).parse()?;
    let to = format!("{} <{}>", details.host_name, details.host_email).parse()?;

    let reason_text = details.reason.as_ref()
        .map(|r| format!("Reason: {}\n\n", r))
        .unwrap_or_default();

    let body = format!(
        "Booking cancelled.\n\n\
         Event: {}\n\
         Date: {}\n\
         Time: {} – {}\n\
         Guest: {} <{}>\n\n\
         {}\
         A calendar cancellation is attached.\n\n\
         — calrs",
        details.event_title,
        details.date,
        details.start_time,
        details.end_time,
        details.guest_name,
        details.guest_email,
        reason_text,
    );

    let ics_attachment = Attachment::new("cancel.ics".to_string())
        .body(ics, ContentType::parse("text/calendar; method=CANCEL; charset=UTF-8")?);

    let email = Message::builder()
        .from(from)
        .to(to)
        .subject(format!("Cancelled: {} — {} ({})", details.event_title, details.guest_name, details.date))
        .multipart(
            MultiPart::mixed()
                .singlepart(SinglePart::plain(body))
                .singlepart(ics_attachment),
        )?;

    send_email(config, email).await
}

/// Generate an .ics VCALENDAR for cancellation (METHOD:CANCEL)
fn generate_cancel_ics(details: &CancellationDetails) -> String {
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
        dtstart = details.date.replace('-', "").to_string() + "T" + &details.start_time.replace(':', "") + "00",
        dtend = details.date.replace('-', "").to_string() + "T" + &details.end_time.replace(':', "") + "00",
        summary = details.event_title,
        host_name = details.host_name,
        host_email = details.host_email,
        guest_name = details.guest_name,
        guest_email = details.guest_email,
    )
}

/// Load SMTP config from database
pub async fn load_smtp_config(pool: &SqlitePool) -> Result<Option<SmtpConfig>> {
    let row: Option<(String, i32, String, String, String, Option<String>)> = sqlx::query_as(
        "SELECT host, port, username, password_enc, from_email, from_name
         FROM smtp_config WHERE enabled = 1 LIMIT 1",
    )
    .fetch_optional(pool)
    .await?;

    match row {
        Some((host, port, username, password_hex, from_email, from_name)) => {
            let password_bytes = hex::decode(&password_hex)?;
            let password = String::from_utf8(password_bytes)?;
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

    let email = Message::builder()
        .from(from)
        .to(to)
        .subject("calrs — SMTP test")
        .singlepart(SinglePart::plain("This is a test email from calrs. SMTP is working!".to_string()))?;

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
