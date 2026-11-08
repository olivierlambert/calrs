//! SMTP configuration (env and database), sender identity, and the send path with its timeouts.

use super::*;

#[derive(Clone)]
pub struct SmtpConfig {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub password: String,
    pub from_email: String,
    pub from_name: Option<String>,
    pub tls_mode: SmtpTlsMode,
}

impl std::fmt::Debug for SmtpConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SmtpConfig")
            .field("host", &self.host)
            .field("port", &self.port)
            .field("username", &self.username)
            .field("password", &"<redacted>")
            .field("from_email", &self.from_email)
            .field("from_name", &self.from_name)
            .field("tls_mode", &self.tls_mode)
            .finish()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SmtpTlsMode {
    StartTls,
    Tls,
    /// No encryption at all. Only sane for a relay reached over the loopback
    /// (or a trusted private link): a local MTA that offers no STARTTLS, or one
    /// whose certificate is self-signed. lettre validates certificates against
    /// the compiled-in Mozilla root bundle, not the system trust store, so a
    /// private CA cannot be trusted and this is the only way through.
    Plaintext,
}

impl SmtpTlsMode {
    pub(super) fn parse(value: &str) -> Result<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "starttls" => Ok(Self::StartTls),
            "tls" => Ok(Self::Tls),
            "none" | "plaintext" => Ok(Self::Plaintext),
            other => bail!(
                "CALRS_SMTP_TLS_MODE must be 'starttls', 'tls' or 'none' (got '{}')",
                other
            ),
        }
    }
}

pub struct SmtpStatus {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub from_email: String,
    pub from_name: Option<String>,
    pub tls_mode: String,
    pub enabled: bool,
    pub from_env: bool,
}

impl SmtpTlsMode {
    /// Canonical lowercase string used in the DB and `<select>` values.
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::StartTls => "starttls",
            Self::Tls => "tls",
            Self::Plaintext => "none",
        }
    }
}

/// Whether a host names the local machine, and so whether an unencrypted
/// connection to it stays inside the box. Bare `localhost` and any address
/// that parses as a loopback IP count; anything else is a network hop.
pub(super) fn is_loopback_host(host: &str) -> bool {
    let host = host.trim().trim_start_matches('[').trim_end_matches(']');
    host.eq_ignore_ascii_case("localhost")
        || host
            .parse::<std::net::IpAddr>()
            .is_ok_and(|ip| ip.is_loopback())
}

impl SmtpConfig {
    /// Whether to authenticate. An empty username means "no SMTP AUTH": lettre
    /// only skips authentication when the transport carries no credentials at
    /// all, so empty ones must not be attached. See `send_email`.
    pub(super) fn uses_auth(&self) -> bool {
        !self.username.trim().is_empty()
    }

    /// Get "from" Mailbox, compliant with RFC 5322
    pub(super) fn mailbox_from(&self) -> Result<Mailbox> {
        Ok(Mailbox::new(
            self.from_name.clone(),
            self.from_email.parse()?,
        ))
    }

    /// Start a message with the From (and, for a host, Reply-To) headers
    /// `sender` calls for. The address is always the configured one, so
    /// SPF/DKIM/DMARC keep aligning with the instance domain; a host sender
    /// only changes the display name and the reply target (#221).
    pub(super) fn message_from(&self, sender: Sender<'_>) -> Result<MessageBuilder> {
        match sender {
            Sender::Instance => Ok(Message::builder().from(self.mailbox_from()?)),
            Sender::Host { name, email, lang } => {
                let display = guest_sender_name(lang, name, self.from_name.as_deref());
                let mut builder =
                    Message::builder().from(Mailbox::new(display, self.from_email.parse()?));
                if let Some(reply_to) = host_reply_to(email) {
                    builder = builder.reply_to(reply_to);
                }
                Ok(builder)
            }
        }
    }
}

/// Who an email appears to come from.
#[derive(Clone, Copy)]
pub(super) enum Sender<'a> {
    /// The instance itself, under the configured sender name: host, team
    /// member, watcher and admin mail.
    Instance,
    /// A host writing to a guest (#221): "Host via <sender name>" on the
    /// instance address, with Reply-To pointing at `email`. `lang` is the
    /// email's own language, for the translated "via". For multi-host
    /// bookings `name` is the joined roster and `email` the ICS organizer,
    /// so replies reach the person the guest's calendar names as organizer.
    Host {
        name: &'a str,
        email: &'a str,
        lang: &'a str,
    },
}

impl<'a> Sender<'a> {
    /// The host of a booking, writing in the guest's language.
    pub(super) fn host_of(details: &'a BookingDetails) -> Self {
        Sender::Host {
            name: &details.host_name,
            email: &details.host_email,
            lang: guest_lang(details),
        }
    }

    /// The host of a cancelled, declined or expired booking, writing in the
    /// guest's language.
    pub(super) fn host_of_cancellation(details: &'a CancellationDetails) -> Self {
        Sender::Host {
            name: &details.host_name,
            email: &details.host_email,
            lang: lang_or_en(details.guest_language.as_deref()),
        }
    }
}

/// Longest host part of a guest-facing display name. Collective bookings join
/// every member's name, which would otherwise grow the header without bound.
pub(super) const GUEST_SENDER_HOST_MAX_CHARS: usize = 80;

/// Characters dropped from a name before it reaches a header: control
/// characters (CR/LF are the classic header injection, even though lettre
/// encodes display names) and the bidirectional formatting marks that can
/// make one name render as another.
fn is_header_unsafe(c: char) -> bool {
    c.is_control()
        || matches!(
            c,
            '\u{200E}' | '\u{200F}' | '\u{061C}' | '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}'
        )
}

/// Display name for guest-facing mail: the host's name followed by the
/// configured sender name, joined in the email's language ("Bob via Acme"),
/// or the host's name alone when no sender name is set. Host names are
/// user-editable, so unsafe characters are dropped and long collective
/// rosters truncated. The sender name is admin-set but lands in the same
/// header, so it is filtered the same way.
pub(super) fn guest_sender_name(
    lang: &str,
    host_name: &str,
    from_name: Option<&str>,
) -> Option<String> {
    let clean = |s: &str| -> String {
        let kept: String = s.chars().filter(|c| !is_header_unsafe(*c)).collect();
        kept.trim().to_string()
    };
    let mut host = clean(host_name);
    if host.chars().count() > GUEST_SENDER_HOST_MAX_CHARS {
        host = host.chars().take(GUEST_SENDER_HOST_MAX_CHARS - 1).collect();
        host = format!("{}\u{2026}", host.trim_end());
    }
    let from_name = from_name.map(clean).filter(|name| !name.is_empty());
    match (host.is_empty(), from_name) {
        (true, from_name) => from_name,
        (false, Some(instance)) => Some(clean(&ta(
            lang,
            "email-sender-via",
            [("host", &host), ("instance", &instance)],
        ))),
        (false, None) => Some(host),
    }
}

/// Reply-To for guest-facing mail: the host's address when it is exactly one
/// bare address, nothing otherwise. A malformed value or an address list
/// yields no Reply-To rather than a broken or surprising header.
/// `name <email>` as a [`Mailbox`]. Formatting the two into one string and
/// parsing it, as every send path used to, fails on a display name with
/// `@`, `,`, `<`, `>`, `;` or quotes in it ("Dr. A, PhD", "Alice @ Acme"),
/// and the email was then never sent. Passing the name to `Mailbox::new`
/// lets lettre quote or encode it instead. Control characters are replaced
/// by spaces so a CRLF in a guest-supplied name still cannot start a header
/// line, and an empty name leaves the bare address.
pub(crate) fn named_mailbox(name: &str, email: &str) -> Result<Mailbox> {
    let name: String = name
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    let name = name.trim();
    let address = email.trim().parse()?;
    Ok(Mailbox::new(
        (!name.is_empty()).then(|| name.to_string()),
        address,
    ))
}

fn host_reply_to(email: &str) -> Option<Mailbox> {
    email
        .trim()
        .parse::<lettre::Address>()
        .ok()
        .map(|address| Mailbox::new(None, address))
}

// --- Utility ---

pub(super) const SMTP_ENV_VARS: &[&str] = &[
    "CALRS_SMTP_HOST",
    "CALRS_SMTP_PORT",
    "CALRS_SMTP_TLS_MODE",
    "CALRS_SMTP_USERNAME",
    "CALRS_SMTP_PASSWORD",
    "CALRS_SMTP_FROM_EMAIL",
    "CALRS_SMTP_FROM_NAME",
];

/// Read an SMTP env var, returning `Some(value)` only when set and non-empty.
fn optional_smtp_env(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .filter(|value| !value.trim().is_empty())
}

/// Load the SMTP config from the `CALRS_SMTP_*` environment block.
///
/// Priority is "full block override": when the required block (host,
/// from_email) is complete, the env wins over the database entirely. When no
/// SMTP var is set, or the required block is only partially set, this returns
/// `Ok(None)` so the caller falls back to the database config — a stray or
/// incomplete env var no longer breaks SMTP. A required block that *is*
/// complete but carries an invalid `PORT`/`TLS_MODE` still surfaces an error,
/// since that is a genuine misconfiguration to fix rather than silently ignore.
///
/// `USERNAME`/`PASSWORD` are optional: omitting them configures an
/// unauthenticated relay, which is how a container talks to a sidecar MTA.
pub(super) fn load_smtp_config_from_env() -> Result<Option<SmtpConfig>> {
    if !SMTP_ENV_VARS
        .iter()
        .any(|name| std::env::var_os(name).is_some())
    {
        return Ok(None);
    }

    let (host, from_email) = match (
        optional_smtp_env("CALRS_SMTP_HOST"),
        optional_smtp_env("CALRS_SMTP_FROM_EMAIL"),
    ) {
        (Some(host), Some(from_email)) => (host, from_email),
        _ => {
            tracing::warn!(
                "partial CALRS_SMTP_* environment block (missing HOST or FROM_EMAIL); falling back to database SMTP config"
            );
            return Ok(None);
        }
    };
    // Both empty means "no SMTP AUTH", as in `send_email`. A username with no
    // password is still authenticated, since some relays accept an empty one.
    let username = optional_smtp_env("CALRS_SMTP_USERNAME").unwrap_or_default();
    let password = optional_smtp_env("CALRS_SMTP_PASSWORD").unwrap_or_default();
    if username.is_empty() && !password.is_empty() {
        bail!("CALRS_SMTP_PASSWORD is set without CALRS_SMTP_USERNAME");
    }
    let port = match std::env::var("CALRS_SMTP_PORT") {
        Ok(value) if value.trim().is_empty() => bail!("CALRS_SMTP_PORT must not be empty"),
        Ok(value) => value.trim().parse::<u16>().map_err(|_| {
            anyhow::anyhow!("CALRS_SMTP_PORT must be a valid TCP port (got '{}')", value)
        })?,
        Err(_) => 587u16,
    };
    let tls_mode = match std::env::var("CALRS_SMTP_TLS_MODE") {
        Ok(value) if value.trim().is_empty() => bail!("CALRS_SMTP_TLS_MODE must not be empty"),
        Ok(value) => SmtpTlsMode::parse(&value)?,
        Err(_) => SmtpTlsMode::StartTls,
    };
    let from_name = std::env::var("CALRS_SMTP_FROM_NAME")
        .ok()
        .filter(|value| !value.trim().is_empty());

    Ok(Some(SmtpConfig {
        host,
        port,
        username,
        password,
        from_email,
        from_name,
        tls_mode,
    }))
}

/// Returns true when the `CALRS_SMTP_*` environment block governs the config,
/// meaning the database config is shadowed and must not be edited from the UI.
///
/// A complete env block (`Ok(Some)`) or a complete-but-invalid one (`Err`, e.g.
/// a bad port) both govern — the env overrides the database either way, so the
/// admin form is locked. A partial/absent block (`Ok(None)`) does not govern:
/// the app falls back to the database, which stays editable.
pub fn smtp_env_active() -> bool {
    !matches!(load_smtp_config_from_env(), Ok(None))
}

/// The env block governs as soon as `HOST` and `FROM_EMAIL` are set, and
/// `USERNAME`/`PASSWORD` are optional. That combination can take over from a
/// database row that *does* carry credentials, turning working authenticated
/// SMTP into unauthenticated sends that the relay then rejects. The operator
/// has no other signal: the admin form locks itself when the env governs, and
/// mail simply stops. Warn once per process, and only when there is something
/// to lose.
async fn warn_if_env_shadows_db_credentials(pool: &SqlitePool) {
    static WARNED: std::sync::Once = std::sync::Once::new();
    if WARNED.is_completed() {
        return;
    }
    let shadowed: Option<(String,)> = sqlx::query_as(
        "SELECT username FROM smtp_config WHERE enabled = 1 AND TRIM(username) <> '' LIMIT 1",
    )
    .fetch_optional(pool)
    .await
    .ok()
    .flatten();
    if shadowed.is_some() {
        WARNED.call_once(|| {
            tracing::warn!(
                "CALRS_SMTP_* sets no USERNAME, so calrs is relaying without authentication, but \
                 the database holds SMTP credentials that the environment block now shadows. Set \
                 CALRS_SMTP_USERNAME and CALRS_SMTP_PASSWORD, or unset the CALRS_SMTP_* block to \
                 go back to the database config."
            );
        });
    }
}

/// Warn once about a username configured with no password.
///
/// This is deliberately not the hard error that a password with no username
/// gets. There, the config is provably self-contradictory: the password can
/// never be transmitted, so the intent to authenticate cannot be honoured under
/// any server behaviour. Here it is only suspicious. It is almost always a
/// secret that never reached the process (an unmounted Docker secret, a
/// misspelled variable), but AUTH PLAIN with an empty password is well-formed
/// and some allowlist-style internal relays accept it, so refusing to send
/// would be a guess.
///
/// Checked on the loaded config rather than per source, so the environment and
/// the database, both of which can express this, behave the same way.
fn warn_if_auth_without_password(config: &SmtpConfig) {
    static WARNED: std::sync::Once = std::sync::Once::new();
    if config.uses_auth() && config.password.is_empty() {
        WARNED.call_once(|| {
            tracing::warn!(
                host = %config.host,
                "SMTP username is set with an empty password, so calrs will authenticate with an \
                 empty password and most relays will reject that. Set the password, or clear the \
                 username to relay without authentication."
            );
        });
    }
}

/// Load SMTP config from environment or database.
pub async fn load_smtp_config(pool: &SqlitePool, key: &[u8; 32]) -> Result<Option<SmtpConfig>> {
    if let Some(config) = load_smtp_config_from_env()? {
        if !config.uses_auth() {
            warn_if_env_shadows_db_credentials(pool).await;
        }
        warn_if_auth_without_password(&config);
        return Ok(Some(config));
    }

    let row: Option<(String, i32, String, String, String, Option<String>, String)> =
        sqlx::query_as(
            "SELECT host, port, username, password_enc, from_email, from_name, tls_mode
         FROM smtp_config WHERE enabled = 1 LIMIT 1",
        )
        .fetch_optional(pool)
        .await?;

    match row {
        Some((host, port, username, password_enc, from_email, from_name, tls_mode)) => {
            let password = crate::crypto::decrypt_password(key, &password_enc)?.to_string();
            let tls_mode = SmtpTlsMode::parse(&tls_mode).unwrap_or(SmtpTlsMode::StartTls);
            let config = SmtpConfig {
                host,
                port: port as u16,
                username,
                password,
                from_email,
                from_name,
                tls_mode,
            };
            warn_if_auth_without_password(&config);
            Ok(Some(config))
        }
        None => Ok(None),
    }
}

/// Load non-secret SMTP status for admin display.
///
/// Unlike [`load_smtp_config`] this does NOT filter by `enabled = 1` — the
/// admin panel needs to surface a disabled-but-configured row so the operator
/// can see that SMTP exists and re-enable it. `load_smtp_config` only returns
/// rows that are actually usable for sending, so it keeps the `WHERE enabled`
/// guard.
pub async fn load_smtp_status(pool: &SqlitePool) -> Result<Option<SmtpStatus>> {
    if let Some(config) = load_smtp_config_from_env()? {
        return Ok(Some(SmtpStatus {
            host: config.host,
            port: config.port,
            username: config.username,
            from_email: config.from_email,
            from_name: config.from_name,
            tls_mode: config.tls_mode.as_str().to_string(),
            enabled: true,
            from_env: true,
        }));
    }

    // Prefer the enabled row so the status matches the row `load_smtp_config`
    // would actually send from, in case legacy cleanup ever left extra rows.
    let row: Option<(String, i32, String, String, Option<String>, String, bool)> = sqlx::query_as(
        "SELECT host, port, username, from_email, from_name, tls_mode, enabled
         FROM smtp_config ORDER BY enabled DESC LIMIT 1",
    )
    .fetch_optional(pool)
    .await?;

    Ok(row.map(
        |(host, port, username, from_email, from_name, tls_mode, enabled)| SmtpStatus {
            host,
            port: port as u16,
            username,
            from_email,
            from_name,
            tls_mode,
            enabled,
            from_env: false,
        },
    ))
}

/// Send a test email
pub async fn send_test_email(config: &SmtpConfig, to_email: &str) -> Result<()> {
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
        .from(config.mailbox_from()?)
        .to(to)
        .subject("calrs \u{2014} SMTP test")
        .multipart(body)?;

    // Debug is only useful when sending a test email
    tracing::debug!("Sending: {:?}", email);
    send_email(config, email).await
}

/// How long to wait for the TCP (and implicit TLS) connection to the SMTP
/// server. lettre's async transport only applies its timeout to the connect
/// step, so this alone cannot bound a server that accepts and then stalls.
pub const SMTP_CONNECT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(15);

/// Hard ceiling on one whole delivery: connect, greeting, EHLO, STARTTLS,
/// AUTH, envelope and DATA. A relay that accepts the connection and then goes
/// quiet would otherwise hold the caller forever (#236), which on the booking
/// path meant the reverse proxy answering 504 for a booking that existed.
pub const SMTP_SEND_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

pub(super) async fn send_email(config: &SmtpConfig, email: Message) -> Result<()> {
    send_email_with_timeouts(config, email, SMTP_CONNECT_TIMEOUT, SMTP_SEND_TIMEOUT).await
}

pub(super) async fn send_email_with_timeouts(
    config: &SmtpConfig,
    email: Message,
    connect_timeout: std::time::Duration,
    send_timeout: std::time::Duration,
) -> Result<()> {
    let builder = match config.tls_mode {
        SmtpTlsMode::Tls => AsyncSmtpTransport::<Tokio1Executor>::relay(&config.host)?,
        SmtpTlsMode::StartTls => {
            AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&config.host)?
        }
        // `builder_dangerous` is lettre's name for "no TLS", and the name is
        // fair: the message, and any credentials, cross the wire in the clear.
        SmtpTlsMode::Plaintext => {
            if config.uses_auth() {
                tracing::warn!(
                    host = %config.host,
                    "sending SMTP credentials over an unencrypted connection (tls_mode = none)"
                );
            } else if !is_loopback_host(&config.host) {
                tracing::warn!(
                    host = %config.host,
                    "sending email over an unencrypted connection to a non-loopback host (tls_mode = none)"
                );
            }
            AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(&config.host)
        }
    }
    .port(config.port)
    .timeout(Some(connect_timeout));

    // An empty username means "no SMTP AUTH". lettre only skips authentication
    // when the transport carries no credentials at all: given empty ones it
    // still looks for a mechanism, and a local MTA relaying anonymously from
    // the loopback (Postfix, OpenSMTPD, Stalwart, Mailpit) advertises none, so
    // every send aborts client-side with "No compatible authentication
    // mechanism was found" before the message ever reaches the server.
    let mailer = if !config.uses_auth() {
        builder.build()
    } else {
        builder
            .credentials(Credentials::new(
                config.username.clone(),
                config.password.clone(),
            ))
            .build()
    };

    let to_addrs: Vec<String> = email
        .envelope()
        .to()
        .iter()
        .map(|a| a.to_string())
        .collect();
    let to_display = to_addrs.join(", ");
    match tokio::time::timeout(send_timeout, mailer.send(email)).await {
        Ok(Ok(_)) => {
            tracing::debug!(to = %to_display, "email delivered");
            Ok(())
        }
        // Not logged here: every caller logs the error once, with what it
        // knows about the email (kind, booking). The recipient and server go
        // into the error so that line has them too; log it with `{:#}`.
        Ok(Err(e)) => Err(anyhow::Error::new(e).context(format!(
            "delivery to {} via {}:{} failed",
            to_display, config.host, config.port
        ))),
        Err(_) => anyhow::bail!(
            "delivery to {} timed out: SMTP server {}:{} did not complete it within {}s",
            to_display,
            config.host,
            config.port,
            send_timeout.as_secs_f32()
        ),
    }
}
