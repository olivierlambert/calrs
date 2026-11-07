//! SMTP configuration, env overrides, sender identity and the send path against loopback relays.

use super::*;

fn smtp_env_error() -> String {
    load_smtp_config_from_env()
        .expect_err("expected SMTP env config to fail")
        .to_string()
}

#[test]
fn smtp_config_mailbox_from() {
    let config = SmtpConfig {
        host: "host".to_string(),
        port: 587,
        username: "user".to_string(),
        password: "password".to_string(),
        from_name: None,
        from_email: "username@example.com".to_string(),
        tls_mode: SmtpTlsMode::StartTls,
    };
    assert_eq!(
        config.mailbox_from().unwrap(),
        Mailbox::new(None, Address::new("username", "example.com").unwrap()),
        "from email with no name"
    );

    let config = SmtpConfig {
        host: "host".to_string(),
        port: 587,
        username: "username".to_string(),
        password: "password".to_string(),
        from_name: Some("Name, With Comma".to_string()),
        from_email: "username@example.com".to_string(),
        tls_mode: SmtpTlsMode::StartTls,
    };
    assert_eq!(
        config.mailbox_from().unwrap(),
        Mailbox::new(
            Some("Name, With Comma".to_string()),
            Address::new("username", "example.com").unwrap()
        ),
        "from email with name"
    );
}

#[test]
fn guest_sender_name_puts_host_ahead_of_configured_name() {
    let name = |host: &str, from: Option<&str>| guest_sender_name("en", host, from);
    assert_eq!(
        name("Bob Smith", Some("Vates Scheduling")).as_deref(),
        Some("Bob Smith via Vates Scheduling")
    );
    assert_eq!(name("Bob Smith", None).as_deref(), Some("Bob Smith"));
    assert_eq!(name("Bob Smith", Some("  ")).as_deref(), Some("Bob Smith"));
    assert_eq!(
        name("", Some("Vates Scheduling")).as_deref(),
        Some("Vates Scheduling")
    );
    assert_eq!(name(" ", None), None);
    assert_eq!(
        name("Bob\r\nBcc: evil@example.com", None).as_deref(),
        Some("BobBcc: evil@example.com")
    );
    // Bidi overrides and isolates are dropped, from either part.
    assert_eq!(
        name("\u{202E}ecilA\u{202C}", Some("Acme\u{2066}\u{200F}")).as_deref(),
        Some("ecilA via Acme")
    );

    // A large collective roster is truncated rather than growing the header.
    let roster = vec!["Łukasz Kowalski"; 20].join(" & ");
    let full = name(&roster, Some("Vates")).unwrap();
    let host = full.strip_suffix(" via Vates").unwrap();
    assert!(host.chars().count() <= GUEST_SENDER_HOST_MAX_CHARS);
    assert!(host.ends_with('\u{2026}'));
    let exact = "a".repeat(GUEST_SENDER_HOST_MAX_CHARS);
    assert_eq!(name(&exact, None), Some(exact));
}

#[test]
fn guest_sender_name_joins_in_the_email_language() {
    let via = |lang: &str| guest_sender_name(lang, "Bob", Some("Acme")).unwrap();
    assert_eq!(via("de"), "Bob über Acme");
    assert_eq!(via("et"), "Bob Acme kaudu");
    assert_eq!(via("pl"), "Bob przez Acme");
    // Every shipped locale keeps both names, so no translation can drop
    // the host or the instance from the header.
    for (lang, _) in crate::i18n::supported_with_labels() {
        let name = via(lang);
        assert!(
            name.contains("Bob") && name.contains("Acme"),
            "{lang}: {name}"
        );
    }
}

fn header_test_config(from_name: Option<&str>) -> SmtpConfig {
    SmtpConfig {
        host: "host".to_string(),
        port: 587,
        username: String::new(),
        password: String::new(),
        from_name: from_name.map(str::to_string),
        from_email: "noreply@example.com".to_string(),
        tls_mode: SmtpTlsMode::StartTls,
    }
}

/// The headers a message started by `sender` goes out with.
fn wire_headers(config: &SmtpConfig, sender: Sender<'_>) -> String {
    let email = config
        .message_from(sender)
        .unwrap()
        .to("guest@example.org".parse().unwrap())
        .subject("s")
        .body(String::from("b"))
        .unwrap();
    String::from_utf8(email.formatted()).unwrap()
}

#[test]
fn host_sender_sets_display_name_and_reply_to() {
    let config = header_test_config(Some("Vates Scheduling"));
    let host = |email| Sender::Host {
        name: "Bob Smith",
        email,
        lang: "en",
    };

    let wire = wire_headers(&config, host("bob@example.com"));
    assert!(
        wire.contains("From: \"Bob Smith via Vates Scheduling\" <noreply@example.com>"),
        "{wire}"
    );
    assert!(wire.contains("Reply-To: bob@example.com"), "{wire}");

    // A host without a usable address still gets the display name, and
    // no Reply-To rather than a malformed or multi-address one.
    for bad in [
        "",
        "not an address",
        "a@example.com, b@example.com",
        "Mallory <m@evil.example>",
        "bob@example.com\r\nBcc: x@evil.example",
    ] {
        let wire = wire_headers(&config, host(bad));
        assert!(
            wire.contains("From: \"Bob Smith via Vates Scheduling\""),
            "{bad:?}: {wire}"
        );
        assert!(!wire.contains("Reply-To"), "{bad:?}: {wire}");
        assert!(!wire.contains("Bcc"), "{bad:?}: {wire}");
    }

    // The instance sender is untouched: configured name, no Reply-To.
    let wire = wire_headers(&config, Sender::Instance);
    assert!(
        wire.contains("From: \"Vates Scheduling\" <noreply@example.com>"),
        "{wire}"
    );
    assert!(!wire.contains("Reply-To"), "{wire}");
}

#[test]
fn host_sender_name_with_non_ascii_or_specials_is_encoded() {
    // A translated joiner or a host name with non-ASCII, quotes or an
    // address-looking part must reach the wire RFC 2047-encoded, never
    // as a second address in From.
    let config = header_test_config(Some("Acme"));
    for (name, lang) in [
        ("Jürgen", "de"),
        ("\"Support\" <it@evil.example>", "en"),
        ("Alice, Bob", "en"),
    ] {
        let wire = wire_headers(
            &config,
            Sender::Host {
                name,
                email: "host@example.com",
                lang,
            },
        );
        let from = wire
            .split("\r\n")
            .find(|l| l.starts_with("From: "))
            .unwrap();
        assert!(from.ends_with("<noreply@example.com>"), "{from}");
        assert!(!from.contains("it@evil.example"), "{from}");
        assert!(from.is_ascii(), "{from}");
    }
}

/// Every async function in `src`, as (name, body), outside the tests module.
/// A function starts at a column-0 line holding `async fn` and ends at the
/// next column-0 `}`.
fn async_fns(src: &str) -> Vec<(&str, &str)> {
    let src = src.split("\n#[cfg(test)]\nmod tests").next().unwrap_or(src);
    let mut fns = Vec::new();
    let mut offset = 0;
    for line in src.split_inclusive('\n') {
        let start = offset;
        offset += line.len();
        let Some(rest) = line
            .strip_prefix("pub async fn ")
            .or_else(|| line.strip_prefix("pub(super) async fn "))
            .or_else(|| line.strip_prefix("pub(crate) async fn "))
            .or_else(|| line.strip_prefix("async fn "))
        else {
            continue;
        };
        let name = &rest[..rest.find('(').unwrap()];
        let end = src[start..].find("\n}\n").map_or(src.len(), |e| start + e);
        fns.push((name, &src[start..end]));
    }
    fns
}

/// The top-level arguments of every call to `call` (e.g. `"send_composed("`)
/// in `body`.
fn call_args<'a>(body: &'a str, call: &str) -> Vec<Vec<&'a str>> {
    let mut calls = Vec::new();
    for (pos, _) in body.match_indices(call) {
        let start = pos + call.len();
        let (mut depth, mut arg_start, mut args) = (1, start, Vec::new());
        for (i, c) in body[start..].char_indices() {
            match c {
                '(' | '[' | '{' => depth += 1,
                ')' | ']' | '}' => {
                    depth -= 1;
                    if depth == 0 {
                        args.push(body[arg_start..start + i].trim());
                        break;
                    }
                }
                ',' if depth == 1 => {
                    args.push(body[arg_start..start + i].trim());
                    arg_start = start + i + 1;
                }
                _ => {}
            }
        }
        calls.push(args);
    }
    calls
}

#[derive(Debug, PartialEq)]
enum SenderKind {
    Host,
    Instance,
}

/// What a sender argument evaluates to: a `Sender::...` expression, or a
/// local bound to one with `let name = Sender::...;` earlier in the body.
fn sender_kind(fn_name: &str, body: &str, arg: &str) -> SenderKind {
    let expr = if arg.starts_with("Sender::") {
        arg
    } else {
        let binding = format!("let {arg} = ");
        let at = body
            .find(&binding)
            .unwrap_or_else(|| panic!("{fn_name}: cannot tell what sender `{arg}` is"));
        body[at + binding.len()..].trim_start()
    };
    if expr.starts_with("Sender::Host") || expr.starts_with("Sender::host_of") {
        SenderKind::Host
    } else if expr.starts_with("Sender::Instance") {
        SenderKind::Instance
    } else {
        panic!("{fn_name}: unknown sender `{arg}`")
    }
}

/// Every guest-facing email must go out under the host's name with a
/// Reply-To (#221), and no host, member, watcher or admin email may. A
/// guard over the sources, so a new `send_guest_*` cannot quietly fall
/// back to the instance sender.
///
/// It checks every message a function builds, not just that a host sender
/// appears somewhere: `send_guest_attendee_copies` sends one message per
/// additional attendee, and switching it to `Sender::Instance` has to fail
/// here.
#[test]
fn guest_emails_use_the_host_sender_and_host_emails_do_not() {
    let files = [
        include_str!("../mod.rs"),
        include_str!("../guest.rs"),
        include_str!("../host.rs"),
        include_str!("../render.rs"),
        include_str!("../transport.rs"),
    ];
    let (mut seen_guest, mut seen_senders) = (0, 0);
    for (name, body) in files.iter().flat_map(|src| async_fns(src)) {
        // The plumbing itself: its sender is whatever the caller passed.
        if name == "send_composed" {
            continue;
        }
        let senders: Vec<SenderKind> = call_args(body, "message_from(")
            .iter()
            .map(|args| args[0])
            .chain(call_args(body, "send_composed(").iter().map(|args| args[1]))
            .map(|arg| sender_kind(name, body, arg))
            .collect();
        seen_senders += senders.len();
        let guest = name.starts_with("send_guest_") || name == "send_invite_email";
        if guest {
            seen_guest += 1;
            assert!(
                !body.contains("Sender::Instance") && !body.contains("Message::builder"),
                "{name} is guest-facing but can build a message not sent as the host"
            );
            // Thin wrappers delegate to the `_ex` variant, or to another
            // guest-facing sender, which this loop checks in turn.
            assert!(
                !senders.is_empty() || delegates_to_guest_sender(name, body),
                "{name} is guest-facing but sends nothing as the host"
            );
            for kind in &senders {
                assert_eq!(
                    *kind,
                    SenderKind::Host,
                    "{name} is guest-facing but sends a message as the instance"
                );
            }
        } else {
            for kind in &senders {
                assert_eq!(
                    *kind,
                    SenderKind::Instance,
                    "{name} is not guest-facing but sent as the host"
                );
            }
        }
    }
    assert!(seen_guest >= 10, "only {seen_guest} guest senders found");
    assert!(seen_senders >= 20, "only {seen_senders} sender uses found");
}

/// Whether `body` calls another guest-facing sender: a `send_guest_*(`
/// call outside a comment, other than `name` itself. A name merely
/// mentioned in a doc or a comment does not count.
fn delegates_to_guest_sender(name: &str, body: &str) -> bool {
    body.lines()
        .map(str::trim_start)
        .filter(|line| !line.starts_with("//"))
        .flat_map(|line| {
            line.match_indices("send_guest_")
                .map(move |(at, _)| &line[at..])
        })
        .any(|call| {
            let ident_len = call
                .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
                .unwrap_or(call.len());
            let (ident, rest) = call.split_at(ident_len);
            ident != name && rest.starts_with('(')
        })
}

#[test]
fn delegation_needs_a_real_call() {
    let commented = "pub async fn send_guest_x() {\n    \
                     // Unlike send_guest_y, this one sends nothing.\n    \
                     Ok(())\n}\n";
    assert!(!delegates_to_guest_sender("send_guest_x", commented));
    let mentioned = "pub async fn send_guest_x() {\n    \
                     let label = \"send_guest_y\";\n    \
                     send_guest_x(config).await\n}\n";
    assert!(!delegates_to_guest_sender("send_guest_x", mentioned));
    let calls = "pub async fn send_guest_x() {\n    \
                 send_guest_y(config, details).await\n}\n";
    assert!(delegates_to_guest_sender("send_guest_x", calls));
    let ex = "pub async fn send_guest_x() {\n    \
              send_guest_x_ex(config, None).await\n}\n";
    assert!(delegates_to_guest_sender("send_guest_x", ex));
}

#[test]
fn sender_guard_sees_every_message_of_a_function() {
    let body = "pub async fn send_guest_x() {\n    \
                let sender = Sender::Host { name, email, lang };\n    \
                config.message_from(Sender::host_of(d))?.to(to);\n    \
                send_composed(config, sender, to, mail, None).await?;\n    \
                send_composed(\n        config,\n        Sender::Instance,\n        to,\n        mail,\n        Some((\"a\", b, \"c\")),\n    )\n    .await\n}\n";
    let fns = async_fns(body);
    assert_eq!(fns.len(), 1);
    let (name, body) = fns[0];
    assert_eq!(name, "send_guest_x");
    let composed: Vec<SenderKind> = call_args(body, "send_composed(")
        .iter()
        .map(|args| sender_kind(name, body, args[1]))
        .collect();
    assert_eq!(composed, [SenderKind::Host, SenderKind::Instance]);
    assert_eq!(
        sender_kind(name, body, call_args(body, "message_from(")[0][0]),
        SenderKind::Host
    );
}

// --- sanitize_ics ---

#[test]
fn smtp_tls_mode_parses_supported_values() {
    assert_eq!(
        SmtpTlsMode::parse("starttls").unwrap(),
        SmtpTlsMode::StartTls
    );
    assert_eq!(SmtpTlsMode::parse(" TLS ").unwrap(), SmtpTlsMode::Tls);
    assert_eq!(SmtpTlsMode::parse("none").unwrap(), SmtpTlsMode::Plaintext);
    assert_eq!(
        SmtpTlsMode::parse("Plaintext").unwrap(),
        SmtpTlsMode::Plaintext
    );
    // Round-trips through the string stored in the DB and the env.
    for mode in [
        SmtpTlsMode::StartTls,
        SmtpTlsMode::Tls,
        SmtpTlsMode::Plaintext,
    ] {
        assert_eq!(SmtpTlsMode::parse(mode.as_str()).unwrap(), mode);
    }
}

#[test]
fn loopback_hosts_are_recognised() {
    for host in [
        "localhost",
        "LOCALHOST",
        " 127.0.0.1 ",
        "127.1.2.3",
        "::1",
        "[::1]",
    ] {
        assert!(is_loopback_host(host), "{host} should be loopback");
    }
    for host in ["smtp.example.com", "10.0.0.1", "192.168.1.10", "", "::2"] {
        assert!(!is_loopback_host(host), "{host} should not be loopback");
    }
}

#[test]
fn smtp_tls_mode_rejects_unknown_values() {
    let err = SmtpTlsMode::parse("ssl").unwrap_err().to_string();
    assert!(err.contains("CALRS_SMTP_TLS_MODE"));
}

#[test]
fn smtp_env_absent_returns_none() {
    let _env = SmtpEnvGuard::new();
    assert!(load_smtp_config_from_env().unwrap().is_none());
}

#[test]
fn smtp_env_complete_defaults_to_starttls() {
    let _env = SmtpEnvGuard::new();
    std::env::set_var("CALRS_SMTP_HOST", "smtp.example.com");
    std::env::set_var("CALRS_SMTP_USERNAME", "user");
    std::env::set_var("CALRS_SMTP_PASSWORD", "secret");
    std::env::set_var("CALRS_SMTP_FROM_EMAIL", "noreply@example.com");

    let config = load_smtp_config_from_env().unwrap().unwrap();

    assert_eq!(config.host, "smtp.example.com");
    assert_eq!(config.port, 587);
    assert_eq!(config.tls_mode, SmtpTlsMode::StartTls);
}

/// A local MTA relaying anonymously from the loopback: it advertises no
/// AUTH and no STARTTLS, like the servers in issue #190. Handles exactly
/// one connection and returns the commands it saw, so a test can tell
/// "the message arrived" from "the client hung up after EHLO".
async fn fake_no_auth_mta(listener: tokio::net::TcpListener) -> Vec<String> {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

    let (stream, _) = listener.accept().await.expect("accept");
    let (read_half, mut write) = stream.into_split();
    let mut reader = BufReader::new(read_half);
    let mut seen = Vec::new();

    macro_rules! say {
        ($($arg:tt)*) => {{
            let line = format!($($arg)*);
            write.write_all(line.as_bytes()).await.expect("write");
            write.write_all(b"\r\n").await.expect("write");
        }};
    }

    say!("220 fake.local ESMTP");
    loop {
        let mut line = String::new();
        match reader.read_line(&mut line).await {
            Ok(0) | Err(_) => break, // client hung up
            Ok(_) => {}
        }
        let line = line.trim_end().to_string();
        let upper = line.to_ascii_uppercase();
        seen.push(line);

        if upper.starts_with("EHLO") {
            say!("250-fake.local");
            say!("250 8BITMIME"); // deliberately no AUTH, no STARTTLS
        } else if upper.starts_with("MAIL FROM") || upper.starts_with("RCPT TO") {
            say!("250 2.1.0 OK");
        } else if upper == "DATA" {
            say!("354 End data with <CR><LF>.<CR><LF>");
            loop {
                let mut body = String::new();
                match reader.read_line(&mut body).await {
                    Ok(0) | Err(_) => break,
                    Ok(_) => {}
                }
                if body.trim_end() == "." {
                    break;
                }
            }
            say!("250 2.0.0 OK: queued");
        } else if upper.starts_with("AUTH") {
            say!("502 5.5.1 AUTH not supported");
        } else if upper == "QUIT" {
            say!("221 2.0.0 Bye");
            break;
        } else {
            say!("250 OK");
        }
    }
    seen
}

fn plaintext_config(port: u16, username: &str) -> SmtpConfig {
    SmtpConfig {
        host: "127.0.0.1".to_string(),
        port,
        username: username.to_string(),
        password: if username.is_empty() {
            String::new()
        } else {
            "secret".to_string()
        },
        from_name: None,
        from_email: "noreply@example.com".to_string(),
        tls_mode: SmtpTlsMode::Plaintext,
    }
}

fn test_message() -> Message {
    Message::builder()
        .from("noreply@example.com".parse().unwrap())
        .to("guest@example.com".parse().unwrap())
        .subject("test")
        .body("hello".to_string())
        .unwrap()
}

/// Regression test for #190: with no username configured, the message must
/// actually reach a relay that advertises no AUTH mechanism.
#[tokio::test]
async fn unauthenticated_relay_receives_the_message() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = tokio::spawn(fake_no_auth_mta(listener));

    let result = tokio::time::timeout(
        std::time::Duration::from_secs(10),
        send_email(&plaintext_config(port, ""), test_message()),
    )
    .await
    .expect("send timed out");
    result.expect("send should succeed against a no-auth relay");

    let seen = tokio::time::timeout(std::time::Duration::from_secs(10), server)
        .await
        .expect("server timed out")
        .unwrap();
    assert!(seen.iter().any(|c| c.starts_with("MAIL FROM")), "{seen:?}");
    assert!(seen.iter().any(|c| c.starts_with("RCPT TO")), "{seen:?}");
    assert!(seen.iter().any(|c| c == "DATA"), "{seen:?}");
    assert!(!seen.iter().any(|c| c.starts_with("AUTH")), "{seen:?}");
}

/// The other half of #190: a username must still mean authentication, so
/// the same relay fails the way it always did. Without this, "skip auth
/// when the username is empty" could regress into "never authenticate".
#[tokio::test]
async fn credentials_against_a_no_auth_relay_still_fail() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = tokio::spawn(fake_no_auth_mta(listener));

    let err = tokio::time::timeout(
        std::time::Duration::from_secs(10),
        send_email(&plaintext_config(port, "alice"), test_message()),
    )
    .await
    .expect("send timed out")
    .expect_err("a no-auth relay cannot satisfy configured credentials");
    assert!(
        format!("{err:#}").contains("authentication mechanism"),
        "unexpected error: {err}"
    );

    // The client aborts before the envelope: nothing reaches the MTA.
    let seen = tokio::time::timeout(std::time::Duration::from_secs(10), server)
        .await
        .expect("server timed out")
        .unwrap();
    assert!(!seen.iter().any(|c| c.starts_with("MAIL FROM")), "{seen:?}");
}

/// #236: a relay that accepts the TCP connection and then never speaks
/// must not hold the caller. lettre's own timeout only covers connect, so
/// this exercises the outer deadline around the whole delivery.
#[tokio::test]
async fn stalled_relay_times_out_instead_of_hanging() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    // Accept, hold the socket open, and never send the 220 greeting.
    let server = tokio::spawn(async move {
        let (sock, _) = listener.accept().await.unwrap();
        tokio::time::sleep(std::time::Duration::from_secs(30)).await;
        drop(sock);
    });

    let started = std::time::Instant::now();
    let err = tokio::time::timeout(
        std::time::Duration::from_secs(10),
        send_email_with_timeouts(
            &plaintext_config(port, ""),
            test_message(),
            std::time::Duration::from_secs(5),
            std::time::Duration::from_millis(300),
        ),
    )
    .await
    .expect("send_email ignored its deadline")
    .expect_err("a silent relay cannot deliver");
    assert!(
        err.to_string().contains("within"),
        "unexpected error: {err}"
    );
    assert!(started.elapsed() < std::time::Duration::from_secs(5));
    server.abort();
}

#[test]
fn smtp_timeouts_fit_under_a_reverse_proxy() {
    // nginx and most proxies give up on an upstream after 60s. The admin
    // "send test email" call is still synchronous, so a dead relay must
    // fail well before that.
    assert!(SMTP_CONNECT_TIMEOUT <= SMTP_SEND_TIMEOUT);
    assert!(SMTP_SEND_TIMEOUT < std::time::Duration::from_secs(60));
}

#[test]
fn smtp_config_without_username_skips_auth() {
    // An unauthenticated local relay: attaching empty credentials makes
    // lettre hunt for a mechanism the server never advertises, and every
    // send aborts client-side before DATA.
    let mut config = SmtpConfig {
        host: "localhost".to_string(),
        port: 25,
        username: String::new(),
        password: String::new(),
        from_name: None,
        from_email: "noreply@example.com".to_string(),
        tls_mode: SmtpTlsMode::StartTls,
    };
    assert!(!config.uses_auth());

    config.username = "   ".to_string();
    assert!(!config.uses_auth());

    config.username = "user".to_string();
    assert!(config.uses_auth());
}

#[test]
fn smtp_env_without_credentials_is_unauthenticated() {
    let _env = SmtpEnvGuard::new();
    // A container pointed at a sidecar MTA: host and sender are enough.
    std::env::set_var("CALRS_SMTP_HOST", "localhost");
    std::env::set_var("CALRS_SMTP_FROM_EMAIL", "noreply@example.com");

    let config = load_smtp_config_from_env().unwrap().unwrap();

    assert_eq!(config.host, "localhost");
    assert!(config.username.is_empty());
    assert!(config.password.is_empty());
    assert!(!config.uses_auth());
}

#[test]
fn smtp_env_accepts_plaintext_tls_mode() {
    let _env = SmtpEnvGuard::new();
    // The whole unauthenticated-loopback-MTA setup, from the environment.
    std::env::set_var("CALRS_SMTP_HOST", "localhost");
    std::env::set_var("CALRS_SMTP_PORT", "25");
    std::env::set_var("CALRS_SMTP_TLS_MODE", "none");
    std::env::set_var("CALRS_SMTP_FROM_EMAIL", "noreply@example.com");

    let config = load_smtp_config_from_env().unwrap().unwrap();

    assert_eq!(config.port, 25);
    assert_eq!(config.tls_mode, SmtpTlsMode::Plaintext);
    assert!(!config.uses_auth());
}

/// The deliberate asymmetry with `smtp_env_password_without_username_errors`.
/// A password with no username is provably unusable, so it is fatal. A
/// username with no password might still authenticate against a permissive
/// relay, so it loads and only warns.
#[test]
fn smtp_env_username_without_password_is_allowed() {
    let _env = SmtpEnvGuard::new();
    std::env::set_var("CALRS_SMTP_HOST", "smtp.example.com");
    std::env::set_var("CALRS_SMTP_FROM_EMAIL", "noreply@example.com");
    std::env::set_var("CALRS_SMTP_USERNAME", "alice");

    let config = load_smtp_config_from_env()
        .expect("a username with no password must not be fatal")
        .expect("the block is complete");

    assert_eq!(config.username, "alice");
    assert!(config.password.is_empty());
    assert!(config.uses_auth(), "credentials must still be attached");
}

#[test]
fn smtp_env_password_without_username_errors() {
    let _env = SmtpEnvGuard::new();
    std::env::set_var("CALRS_SMTP_HOST", "smtp.example.com");
    std::env::set_var("CALRS_SMTP_FROM_EMAIL", "noreply@example.com");
    std::env::set_var("CALRS_SMTP_PASSWORD", "secret");

    let err = smtp_env_error();

    assert!(err.contains("CALRS_SMTP_USERNAME"));
}

#[test]
fn smtp_env_partial_config_falls_back_to_db() {
    let _env = SmtpEnvGuard::new();
    // Only one of the required vars is set: the block is incomplete, so the
    // env is ignored and the caller falls back to the database config
    // (instead of erroring out and breaking SMTP entirely).
    std::env::set_var("CALRS_SMTP_HOST", "smtp.example.com");

    assert!(load_smtp_config_from_env().unwrap().is_none());
}

#[test]
fn smtp_env_invalid_port_errors() {
    let _env = SmtpEnvGuard::new();
    std::env::set_var("CALRS_SMTP_HOST", "smtp.example.com");
    std::env::set_var("CALRS_SMTP_USERNAME", "user");
    std::env::set_var("CALRS_SMTP_PASSWORD", "secret");
    std::env::set_var("CALRS_SMTP_FROM_EMAIL", "noreply@example.com");
    std::env::set_var("CALRS_SMTP_PORT", "not-a-port");

    let err = smtp_env_error();

    assert!(err.contains("CALRS_SMTP_PORT"));
}

#[test]
fn smtp_env_invalid_tls_mode_errors() {
    let _env = SmtpEnvGuard::new();
    std::env::set_var("CALRS_SMTP_HOST", "smtp.example.com");
    std::env::set_var("CALRS_SMTP_USERNAME", "user");
    std::env::set_var("CALRS_SMTP_PASSWORD", "secret");
    std::env::set_var("CALRS_SMTP_FROM_EMAIL", "noreply@example.com");
    std::env::set_var("CALRS_SMTP_TLS_MODE", "ssl");

    let err = smtp_env_error();

    assert!(err.contains("CALRS_SMTP_TLS_MODE"));
}

// ===== Email-header injection defenses =====
//
// Email-sending paths used to build To via
// `format!("{name} <{email}>").parse::<Mailbox>()?`; they now go through
// `named_mailbox()`, which replaces control characters in the name before
// `Mailbox::new` (see the tests below it). Subjects go through
// `.subject(format!(…))`. The audit that produced #43 flagged the first
// pattern as potentially injectable (Claude's reasoning: an unvalidated
// guest name could contain CRLF and smuggle a Bcc: into the headers).
//
// Empirically lettre's typed builder defends both: `Mailbox::from_str`
// rejects CRLF in display names, and `.subject(…)` RFC 2047-encodes any
// control chars (so `\r\n` ends up inside a base64-encoded section and
// never reaches the SMTP wire). These tests pin that behavior so we
// notice immediately if a future lettre upgrade relaxes either defense
// — at which point we'd need to add an explicit sanitizer at the
// boundary in email/transport.rs.

#[test]
fn lettre_mailbox_rejects_crlf_in_display_name() {
    use lettre::message::Mailbox;
    // The shape the email module used before `named_mailbox()`:
    //   format!("{} <{}>", details.guest_name, details.guest_email).parse()
    let payloads = [
        "Alice\r\nBcc: evil@attacker.com",
        "Alice\nX-Smuggled: true",
        "Alice\r\nSubject: hijacked",
        "Alice\"; Bcc: evil@attacker.com\r\n",
    ];
    for payload in payloads {
        let raw = format!("{} <guest@example.com>", payload);
        let result = raw.parse::<Mailbox>();
        assert!(
            result.is_err(),
            "lettre must reject CRLF/LF in display name — payload {:?} parsed to {:?}",
            payload,
            result
        );
    }
}

/// A display name with `@`, a comma, quotes and angle brackets did not
/// parse once formatted as `name <email>`, so the email was never sent.
#[test]
fn named_mailbox_accepts_special_characters_in_the_name() {
    let name = r#"Dr. "Ace" O'Neil, PhD @ Acme <ops>; team"#;
    let mailbox = named_mailbox(name, "ace@example.com").expect("a valid mailbox");
    assert_eq!(mailbox.name.as_deref(), Some(name));
    assert_eq!(mailbox.email.to_string(), "ace@example.com");
    // What lettre writes on the wire parses back to the same mailbox.
    let reparsed: Mailbox = mailbox.to_string().parse().expect("round-trips");
    assert_eq!(reparsed, mailbox);
    // The old shape refused the same input.
    assert!(format!("{name} <ace@example.com>")
        .parse::<Mailbox>()
        .is_err());
}

#[test]
fn named_mailbox_keeps_crlf_out_of_the_headers() {
    let mailbox = named_mailbox("Alice\r\nBcc: evil@attacker.com", "guest@example.com")
        .expect("control characters are replaced, not refused");
    let msg = Message::builder()
        .from("sender@example.com".parse::<Mailbox>().unwrap())
        .to(mailbox)
        .subject("s")
        .body("body".to_string())
        .unwrap();
    let wire = String::from_utf8_lossy(&msg.formatted()).into_owned();
    assert!(
        !wire.lines().any(|l| l.starts_with("Bcc:")),
        "header injected: {wire}"
    );
    // An empty (or all-control) name leaves the bare address.
    let bare = named_mailbox(" \r\n ", "guest@example.com").unwrap();
    assert_eq!(bare.name, None);
    assert!(named_mailbox("Alice", "not an address").is_err());
}

/// End to end: a host whose name has `@`, a comma and quotes in it gets
/// their cancellation notice.
#[tokio::test]
async fn host_with_special_characters_in_the_name_is_emailed() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = tokio::spawn(fake_no_auth_mta(listener));

    let details = CancellationDetails {
        event_title: "Intro".to_string(),
        date: "2030-07-02".to_string(),
        start_time: "10:00".to_string(),
        end_time: "10:30".to_string(),
        guest_name: "Guest".to_string(),
        guest_email: "guest@example.com".to_string(),
        guest_timezone: "UTC".to_string(),
        host_name: r#"Alice "Ops" @ Acme, Inc."#.to_string(),
        host_email: "alice@example.com".to_string(),
        uid: "uid-special-name".to_string(),
        ..Default::default()
    };
    tokio::time::timeout(
        std::time::Duration::from_secs(10),
        send_host_cancellation(&plaintext_config(port, ""), &details),
    )
    .await
    .expect("send timed out")
    .expect("the notice is sent");

    let seen = tokio::time::timeout(std::time::Duration::from_secs(10), server)
        .await
        .expect("server timed out")
        .unwrap();
    assert!(
        seen.iter()
            .any(|c| c.starts_with("RCPT TO") && c.contains("alice@example.com")),
        "{seen:?}"
    );
    assert!(seen.iter().any(|c| c == "DATA"), "{seen:?}");
}

#[test]
fn lettre_subject_encodes_crlf_safely() {
    use lettre::message::{Mailbox, Message};
    let msg = Message::builder()
        .from("sender@example.com".parse::<Mailbox>().unwrap())
        .to("to@example.com".parse::<Mailbox>().unwrap())
        .subject("Normal\r\nBcc: evil@attacker.com")
        .body("body".to_string())
        .expect("message builds");

    let raw = msg.formatted();
    let wire = String::from_utf8_lossy(&raw);

    // The injected header name must not appear as its own header line.
    // Exhaustively: neither right after a CRLF, nor at the start of the
    // wire format (which would also be injection).
    let lines: Vec<&str> = wire.split("\r\n").collect();
    assert!(
        !lines.iter().any(|l| l.starts_with("Bcc:")),
        "Bcc header injected — lettre no longer encodes Subject CRLF. Wire:\n{}",
        wire
    );
    // Sanity: the Subject line itself must exist exactly once.
    let subject_count = lines.iter().filter(|l| l.starts_with("Subject:")).count();
    assert_eq!(subject_count, 1, "exactly one Subject header expected");
}
