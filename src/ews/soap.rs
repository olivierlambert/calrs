//! Low-level SOAP transport for EWS.
//!
//! EWS exposes a SOAP 1.1 endpoint at `<server>/EWS/Exchange.asmx`. Requests
//! are XML envelopes wrapping a `<m:...>` (message) operation; responses come
//! back as similar envelopes carrying a `<m:...Response>` body. Requests are
//! built as strings; replies are read with the namespace-aware parser in
//! `src/xml.rs` that the CalDAV client uses (#369), so the prefixes a server
//! picks do not matter and a malformed or unexpected reply is an error rather
//! than an empty answer.
//!
//! Authentication uses HTTP Basic over TLS. NTLM and Kerberos are common in
//! on-prem Exchange but require external crates; on-prem 2019 admins can
//! enable Basic on a service mailbox or front EWS with a reverse proxy that
//! does the negotiation for us.

use anyhow::{bail, Context, Result};
use reqwest::{Client, StatusCode};
use std::time::Duration;

use crate::xml::Element;

/// Targeted EWS schema version. Exchange 2019 advertises up to
/// `Exchange2016` in its RequestServerVersion enumeration; using a value the
/// server understands is mandatory or it returns `ErrorInvalidServerVersion`.
pub const REQUEST_SERVER_VERSION: &str = "Exchange2016";

/// Default timeouts. Discovery is cheap; item fetches can paginate so we
/// allow a generous budget.
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);
const FETCH_TIMEOUT: Duration = Duration::from_secs(120);

/// XML namespaces used in every EWS envelope.
pub const NS_SOAP: &str = "http://schemas.xmlsoap.org/soap/envelope/";
pub const NS_TYPES: &str = "http://schemas.microsoft.com/exchange/services/2006/types";
pub const NS_MESSAGES: &str = "http://schemas.microsoft.com/exchange/services/2006/messages";
/// SOAP 1.2, which an Exchange front end may answer a fault in.
pub const NS_SOAP12: &str = "http://www.w3.org/2003/05/soap-envelope";

/// Wrap a SOAP body in a complete envelope with the standard headers.
///
/// When `impersonate_email` is `Some`, an `<t:ExchangeImpersonation>` header is
/// added so the request executes against that mailbox instead of the
/// authenticating principal's. This is the admin-controlled "Global EWS via
/// Impersonation" path: the connecting account must hold the
/// `ApplicationImpersonation` RBAC role on Exchange.
///
/// The mailbox is named with `t:SmtpAddress`, not `t:PrimarySmtpAddress`:
/// Exchange refuses an alias in the latter ("the primary SMTP address must be
/// specified"), while `SmtpAddress` takes the primary address and any alias,
/// which matters on a server whose users have several addresses and domains.
pub fn envelope(body: &str, impersonate_email: Option<&str>) -> String {
    let impersonation = match impersonate_email {
        Some(mb) if !mb.is_empty() => format!(
            "    <t:ExchangeImpersonation>\n      <t:ConnectingSID>\n        <t:SmtpAddress>{}</t:SmtpAddress>\n      </t:ConnectingSID>\n    </t:ExchangeImpersonation>\n",
            escape(mb)
        ),
        _ => String::new(),
    };
    format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
<soap:Envelope xmlns:soap="{ns_soap}" xmlns:t="{ns_types}" xmlns:m="{ns_messages}">
  <soap:Header>
    <t:RequestServerVersion Version="{version}" />
{impersonation}  </soap:Header>
  <soap:Body>
{body}
  </soap:Body>
</soap:Envelope>"#,
        ns_soap = NS_SOAP,
        ns_types = NS_TYPES,
        ns_messages = NS_MESSAGES,
        version = REQUEST_SERVER_VERSION,
        impersonation = impersonation,
        body = body,
    )
}

/// Build an HTTP client tuned for EWS: HTTPS only (per `validate_caldav_url`),
/// configurable timeout, no automatic redirect (Autodiscover redirects are
/// followed manually so we can validate them).
pub fn http_client(timeout: Duration) -> Result<Client> {
    Client::builder()
        .timeout(timeout)
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .context("failed to build HTTP client")
}

/// Prefix of the error [`post_soap`] raises for a SOAP fault, followed by the
/// text [`soap_fault`] returns (`<Code>: <MessageText>` for a per-message
/// error). Callers that react to a specific fault code match on it.
pub const SOAP_FAULT_PREFIX: &str = "EWS SOAP fault: ";

/// Send a SOAP request and return the reply's parsed `Envelope`. The caller
/// passes the inner SOAP body; this function adds the envelope, headers, and
/// basic auth.
///
/// When `impersonate_email` is `Some`, the envelope carries an
/// `<t:ExchangeImpersonation>` header: the request runs as that mailbox under
/// the connecting service account's RBAC `ApplicationImpersonation` grant.
pub async fn post_soap(
    endpoint: &str,
    username: &str,
    password: &str,
    body: &str,
    fetch: bool,
    impersonate_email: Option<&str>,
) -> Result<Element> {
    post_soap_skipping(
        endpoint,
        username,
        password,
        body,
        fetch,
        impersonate_email,
        &[],
    )
    .await
}

/// [`post_soap`] for a batch: a response message failing with one of the
/// `skip` codes is not raised, and is left for the parser to drop. Any
/// other failure still is.
pub async fn post_soap_skipping(
    endpoint: &str,
    username: &str,
    password: &str,
    body: &str,
    fetch: bool,
    impersonate_email: Option<&str>,
    skip: &'static [&'static str],
) -> Result<Element> {
    let timeout = if fetch {
        FETCH_TIMEOUT
    } else {
        DEFAULT_TIMEOUT
    };
    let client = http_client(timeout)?;
    let envelope = envelope(body, impersonate_email);

    tracing::debug!(
        endpoint = %endpoint,
        impersonate = impersonate_email.unwrap_or("(none)"),
        body_len = envelope.len(),
        "EWS SOAP request"
    );
    let mut req = client
        .post(endpoint)
        .basic_auth(username, Some(password))
        .header("Content-Type", "text/xml; charset=utf-8")
        .header("Accept", "text/xml");
    // When impersonating, set X-AnchorMailbox so Exchange routes the request to
    // the impersonated mailbox's server. Without it, multi-database / DAG
    // deployments resolve against the connecting account and reject the
    // operation (often as ErrorNonExistentMailbox). This is the documented
    // best practice for EWS Impersonation on Exchange 2013+.
    if let Some(mb) = impersonate_email.filter(|m| !m.is_empty()) {
        req = req.header("X-AnchorMailbox", mb);
    }
    let resp = req
        .body(envelope)
        .send()
        .await
        .with_context(|| format!("EWS request to {endpoint} failed"))?;

    let status = resp.status();
    if !status.is_success() && status != StatusCode::INTERNAL_SERVER_ERROR {
        if status == StatusCode::UNAUTHORIZED {
            bail!("EWS authentication failed (401). Check username/password.");
        }
        bail!("EWS request returned {status} for {endpoint}");
    }
    let text = crate::caldav::read_body(resp, crate::xml::MAX_INPUT_BYTES)
        .await
        .with_context(|| format!("EWS request to {endpoint} failed"))?;
    tracing::debug!(endpoint = %endpoint, status = %status, body_len = text.len(), "EWS SOAP response");
    // A reply can be large (64 MB at most): parse it off the threads that
    // serve pages, as the CalDAV client does.
    let at = endpoint.to_string();
    let (reply, _) = crate::caldav::parse_blocking(text, move |text| {
        read_reply_skipping(status, text, &at, skip)
    })
    .await;
    reply
}

/// The `Envelope` of a reply that `post_soap` received with `status` (a
/// success or a 500), or the error it carries.
///
/// EWS returns SOAP faults inside a 200 envelope or a 500 with a fault body,
/// and per-message errors (`ResponseClass="Error"`) inside a 200: each is
/// raised as an error starting with [`SOAP_FAULT_PREFIX`]. What is left to
/// refuse is a 500 without a fault (an error page), and a success that is not
/// a well-formed SOAP envelope (a reverse proxy's sign-in page, a body cut
/// short): read as an empty answer, either would look like a mailbox with
/// nothing in it.
pub fn read_reply(status: StatusCode, text: &str, endpoint: &str) -> Result<Element> {
    read_reply_skipping(status, text, endpoint, &[])
}

/// [`read_reply`], leaving response messages that failed with one of the
/// `skip` codes to the caller.
pub fn read_reply_skipping(
    status: StatusCode,
    text: &str,
    endpoint: &str,
    skip: &[&str],
) -> Result<Element> {
    let doc = match parse_envelope(text) {
        Ok(doc) => doc,
        Err(_) if !status.is_success() => bail!("EWS request returned {status} for {endpoint}"),
        Err(e) => {
            bail!("EWS request to {endpoint} returned a reply that is not a SOAP envelope: {e:#}")
        }
    };
    if let Some(fault) = soap_fault_skipping(&doc, skip) {
        bail!("{SOAP_FAULT_PREFIX}{fault}");
    }
    if !status.is_success() {
        bail!("EWS request returned {status} for {endpoint}");
    }
    Ok(doc)
}

/// Parse a SOAP reply into its root `Envelope`, which must hold a `Body`.
/// Prefixes do not matter, nor does the SOAP version (1.1 or 1.2). Anything
/// else, malformed markup included, is an error.
pub fn parse_envelope(text: &str) -> Result<Element> {
    let Some(root) = crate::xml::parse(text)? else {
        bail!("the reply is empty");
    };
    if !is_soap(&root, "Envelope") {
        bail!(
            "the reply's root element is <{}>, not a SOAP Envelope",
            crate::xml::quoted(root.local_name())
        );
    }
    if soap_body(&root).is_none() {
        bail!("the SOAP envelope has no Body");
    }
    Ok(root)
}

/// The `Body` of a SOAP envelope.
pub fn soap_body(envelope: &Element) -> Option<&Element> {
    envelope.elements().find(|el| is_soap(el, "Body"))
}

/// Whether `el` is the SOAP element `name`, of SOAP 1.1 or 1.2, or written
/// with a prefix the reply never declared.
fn is_soap(el: &Element, name: &str) -> bool {
    el.local_name() == name && matches!(el.namespace(), None | Some(NS_SOAP) | Some(NS_SOAP12))
}

/// Whether `el` is the EWS element `name`. EWS spreads its elements over two
/// namespaces (`m:RootFolder` holds `t:Items`, while `m:Items` sits directly
/// in a GetItem message), so either is accepted; so is an element in no
/// namespace, written with a prefix the reply never declared. An element of
/// any other namespace never matches.
pub fn is_ews(el: &Element, name: &str) -> bool {
    el.local_name() == name && matches!(el.namespace(), None | Some(NS_TYPES) | Some(NS_MESSAGES))
}

/// The first child of `el` that is the EWS element `name`.
pub fn ews_child<'a>(el: &'a Element, name: &str) -> Option<&'a Element> {
    el.elements().find(|child| is_ews(child, name))
}

/// The trimmed text of the EWS child `name` of `el`, `None` when absent or
/// empty.
pub fn ews_text(el: &Element, name: &str) -> Option<String> {
    ews_child(el, name).and_then(Element::trimmed_text)
}

/// The human-readable error a reply carries: a SOAP `Fault` in its body, or
/// the first response message that did not succeed. `None` when there is
/// neither.
///
/// When EWS includes a machine-readable `ResponseCode` (e.g.
/// `ErrorNonExistentMailbox`) it is prepended to the localized `faultstring`,
/// so callers can match on the code without depending on the server's display
/// language.
pub fn soap_fault(envelope: &Element) -> Option<String> {
    soap_fault_skipping(envelope, &[])
}

/// [`soap_fault`], not counting a response message that failed with one of
/// the `skip` codes. A SOAP `Fault` is always counted.
fn soap_fault_skipping(envelope: &Element, skip: &[&str]) -> Option<String> {
    let body = soap_body(envelope)?;
    if let Some(fault) = body.elements().find(|el| is_soap(el, "Fault")) {
        return Some(describe_fault(fault));
    }
    body.elements()
        .flat_map(|response| {
            response
                .elements()
                .filter(|el| is_ews(el, "ResponseMessages"))
        })
        .flat_map(Element::elements)
        .filter(|message| !message_code(message).is_some_and(|code| skip.contains(&code.as_str())))
        .find_map(message_failure)
}

/// The `ResponseCode` of a response message.
pub fn message_code(message: &Element) -> Option<String> {
    ews_text(message, "ResponseCode")
}

/// `<Code>: <reason>` for a SOAP fault. The code is the language-independent
/// `ResponseCode` Exchange puts in the fault's detail (in its errors
/// namespace); the reason is the SOAP 1.1 `faultstring` or the SOAP 1.2
/// `Reason`.
fn describe_fault(fault: &Element) -> String {
    let code = fault
        .descendants()
        .find(|el| el.local_name() == "ResponseCode")
        .and_then(Element::trimmed_text)
        .filter(|code| code != "NoError");
    let reason = fault
        .elements()
        .find(|el| el.local_name() == "faultstring")
        .and_then(Element::trimmed_text)
        .or_else(|| {
            let reason = fault.elements().find(|el| is_soap(el, "Reason"))?;
            reason
                .elements()
                .find(|el| is_soap(el, "Text"))
                .and_then(Element::trimmed_text)
                .or_else(|| reason.trimmed_text())
        });
    let detail = || {
        fault
            .descendants()
            .find(|el| matches!(el.local_name(), "MessageText" | "Message"))
            .and_then(Element::trimmed_text)
    };
    match (code, reason) {
        (Some(code), Some(reason)) => format!("{code}: {reason}"),
        (None, Some(reason)) => reason,
        (Some(code), None) => match detail() {
            Some(detail) => format!("{code}: {detail}"),
            None => code,
        },
        (None, None) => fault
            .elements()
            .find(|el| el.local_name() == "faultcode")
            .and_then(Element::trimmed_text)
            .unwrap_or_else(|| "unspecified SOAP fault".to_string()),
    }
}

/// `<ResponseCode>: <MessageText>` for a response message that did not
/// succeed: a `ResponseCode` other than `NoError`, or `ResponseClass="Error"`
/// without a code. `None` for a success, including a `Warning` carrying
/// `NoError` (processed, with a note).
pub fn message_failure(message: &Element) -> Option<String> {
    let code = message_code(message);
    let failed = match code.as_deref() {
        Some(code) => code != "NoError",
        None => message.attr("ResponseClass") == Some("Error"),
    };
    if !failed {
        return None;
    }
    let code = code.unwrap_or_else(|| "Error".to_string());
    Some(match ews_text(message, "MessageText") {
        Some(detail) => format!("{code}: {detail}"),
        None => code,
    })
}

/// XML-escape a fragment. Use whenever caller-controlled content (subject,
/// description, calendar id, …) is interpolated into a SOAP body.
pub fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '&' => out.push_str("&amp;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ews::fixtures;

    fn read(status: StatusCode, xml: &str) -> Result<Element> {
        read_reply(status, xml, "https://ex.example/EWS/Exchange.asmx")
    }

    fn fault_of(status: StatusCode, xml: &str) -> String {
        let err = read(status, xml).expect_err("a fault").to_string();
        err.strip_prefix(SOAP_FAULT_PREFIX)
            .unwrap_or_else(|| panic!("not a SOAP fault: {err}"))
            .to_string()
    }

    /// A 500 without a SOAP fault, or a 200 that is not a well-formed SOAP
    /// envelope, is an error: read as an empty answer it would look like a
    /// mailbox with nothing in it.
    #[test]
    fn a_reply_that_is_not_soap_is_an_error() {
        let ok = "<s:Envelope><s:Body/></s:Envelope>";
        assert!(read(StatusCode::OK, ok).is_ok());
        let err = read(StatusCode::INTERNAL_SERVER_ERROR, ok).unwrap_err();
        assert!(err.to_string().contains("returned 500"), "{err}");
        for body in [
            "",
            "   ",
            fixtures::HTML_SIGN_IN_PAGE,
            fixtures::TRUNCATED_FIND_ITEM,
            "{\"error\": \"Fault\"}",
            "Service Unavailable",
            // An envelope of some other vocabulary, and one with no body.
            r#"<Envelope xmlns="urn:not-soap"><Body/></Envelope>"#,
            r#"<s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/"><s:Header/></s:Envelope>"#,
            // A Body of another namespace is not the SOAP one.
            r#"<s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/"><b:Body xmlns:b="urn:x"/></s:Envelope>"#,
        ] {
            let err = read(StatusCode::OK, body).expect_err(body).to_string();
            assert!(err.contains("not a SOAP envelope"), "{body:?}: {err}");
            let err = read(StatusCode::INTERNAL_SERVER_ERROR, body)
                .expect_err(body)
                .to_string();
            assert!(err.contains("returned 500"), "{body:?}: {err}");
        }
    }

    /// The envelope may use any prefix, a default namespace, or SOAP 1.2.
    #[test]
    fn envelope_prefixes_do_not_matter() {
        for xml in [
            fixtures::FIND_FOLDER_SOAP_PREFIX,
            fixtures::FIND_FOLDER_DEFAULT_NAMESPACES,
            fixtures::FIND_FOLDER_ODD_PREFIXES,
            r#"<env:Envelope xmlns:env="http://www.w3.org/2003/05/soap-envelope"><env:Body/></env:Envelope>"#,
        ] {
            let doc = read(StatusCode::OK, xml).unwrap();
            assert!(soap_body(&doc).is_some());
        }
    }

    #[test]
    fn fault_detection() {
        let xml = fixtures::envelope(
            "<s:Fault><faultcode>s:Client</faultcode><faultstring>auth required</faultstring></s:Fault>",
        );
        assert_eq!(
            fault_of(StatusCode::INTERNAL_SERVER_ERROR, &xml),
            "auth required"
        );
    }

    /// Exchange can answer a fault with a 200 as well as a 500.
    #[test]
    fn a_fault_inside_a_200_is_still_a_fault() {
        assert_eq!(
            fault_of(StatusCode::OK, fixtures::IMPERSONATE_USER_DENIED),
            "ErrorImpersonateUserDenied: The account does not have permission to impersonate the requested user."
        );
    }

    #[test]
    fn envelope_carries_the_impersonation_header_only_when_asked() {
        let plain = envelope("<m:GetFolder/>", None);
        assert!(!plain.contains("ExchangeImpersonation"));
        assert_eq!(envelope("<m:GetFolder/>", Some("")), plain);

        // The address is escaped: it comes from a user's email.
        let imp = envelope("<m:GetFolder/>", Some("a&b@corp.example"));
        assert!(imp.contains(
            "<t:ExchangeImpersonation>\n      <t:ConnectingSID>\n        <t:SmtpAddress>a&amp;b@corp.example</t:SmtpAddress>"
        ));
        let header_end = imp.find("</soap:Header>").unwrap();
        assert!(imp.find("ExchangeImpersonation").unwrap() < header_end);
        // What calrs sends is itself a SOAP envelope by its own reader.
        assert!(parse_envelope(&imp).is_ok());
    }

    /// EWS impersonation failures (#138) arrive as a SOAP Fault whose detail
    /// block carries the language-independent ResponseCode, in Exchange's
    /// errors namespace. The code is surfaced so callers can match on it
    /// regardless of the server's display language.
    #[test]
    fn fault_prepends_response_code_when_present() {
        assert_eq!(
            fault_of(
                StatusCode::INTERNAL_SERVER_ERROR,
                fixtures::NON_EXISTENT_MAILBOX_FR
            ),
            "ErrorNonExistentMailbox: Aucune boîte aux lettres n'est associée à l'adresse SMTP."
        );
        // Undeclared prefixes, as the mock server in the web tests writes.
        let xml = fixtures::envelope(
            "<s:Fault><faultcode>a:ErrorNonExistentMailbox</faultcode><faultstring>No mailbox.</faultstring><detail><e:ResponseCode>ErrorNonExistentMailbox</e:ResponseCode></detail></s:Fault>",
        );
        assert_eq!(
            fault_of(StatusCode::INTERNAL_SERVER_ERROR, &xml),
            "ErrorNonExistentMailbox: No mailbox."
        );
    }

    #[test]
    fn soap_12_faults_read_their_reason() {
        assert_eq!(
            fault_of(StatusCode::INTERNAL_SERVER_ERROR, fixtures::SOAP12_FAULT),
            "ErrorSchemaValidation: The request failed schema validation."
        );
    }

    /// A fault with no reason falls back to its detail, then to its code.
    #[test]
    fn a_fault_without_a_reason_still_says_something() {
        let xml = fixtures::envelope(
            "<s:Fault><detail><e:ResponseCode xmlns:e=\"http://schemas.microsoft.com/exchange/services/2006/errors\">ErrorAccessDenied</e:ResponseCode><e:Message xmlns:e=\"http://schemas.microsoft.com/exchange/services/2006/errors\">Access is denied.</e:Message></detail></s:Fault>",
        );
        assert_eq!(
            fault_of(StatusCode::OK, &xml),
            "ErrorAccessDenied: Access is denied."
        );
        let xml = fixtures::envelope("<s:Fault><faultcode>s:Server</faultcode></s:Fault>");
        assert_eq!(fault_of(StatusCode::OK, &xml), "s:Server");
        let xml = fixtures::envelope("<s:Fault/>");
        assert_eq!(fault_of(StatusCode::OK, &xml), "unspecified SOAP fault");
    }

    #[test]
    fn fault_from_response_code() {
        assert_eq!(
            fault_of(StatusCode::OK, fixtures::ACCESS_DENIED_MESSAGE),
            "ErrorAccessDenied: Access is denied. Check credentials and try again."
        );
    }

    /// Every message of a batch is read, not only the first, and an error
    /// class without a code still fails.
    #[test]
    fn any_failed_message_is_a_fault() {
        let fault = fault_of(StatusCode::OK, fixtures::GET_ITEM_SECOND_MESSAGE_FAILS);
        assert!(fault.starts_with("ErrorItemNotFound: "), "{fault}");
        let xml = fixtures::envelope(
            r#"<m:DeleteItemResponse><m:ResponseMessages><m:DeleteItemResponseMessage ResponseClass="Error"/></m:ResponseMessages></m:DeleteItemResponse>"#,
        );
        assert_eq!(fault_of(StatusCode::OK, &xml), "Error");
    }

    #[test]
    fn fault_no_error_returns_none() {
        let xml = fixtures::envelope(
            r#"<m:DeleteItemResponse><m:ResponseMessages><m:DeleteItemResponseMessage ResponseClass="Success"><m:ResponseCode>NoError</m:ResponseCode></m:DeleteItemResponseMessage></m:ResponseMessages></m:DeleteItemResponse>"#,
        );
        let doc = read(StatusCode::OK, &xml).unwrap();
        assert_eq!(soap_fault(&doc), None);
        // A warning carrying NoError is not a failure either.
        let xml = xml.replace("\"Success\"", "\"Warning\"");
        assert!(read(StatusCode::OK, &xml).is_ok());
    }

    /// Text that merely looks like a fault (an event subject, escaped or in
    /// CDATA) is not one: only elements count.
    #[test]
    fn fault_words_in_text_are_not_faults() {
        let doc = read(StatusCode::OK, fixtures::FIND_ITEM_FAULT_LOOKALIKE_TEXT).unwrap();
        assert_eq!(soap_fault(&doc), None);
    }

    /// A ResponseCode of another vocabulary does not count as EWS's.
    #[test]
    fn foreign_namespaces_do_not_match() {
        let xml = fixtures::envelope(
            r#"<m:DeleteItemResponse><m:ResponseMessages><m:DeleteItemResponseMessage ResponseClass="Success"><x:ResponseCode xmlns:x="urn:other">ErrorAccessDenied</x:ResponseCode><m:ResponseCode>NoError</m:ResponseCode></m:DeleteItemResponseMessage></m:ResponseMessages></m:DeleteItemResponse>"#,
        );
        assert!(read(StatusCode::OK, &xml).is_ok());
    }

    #[test]
    fn escape_special_chars() {
        assert_eq!(escape("a & b < c"), "a &amp; b &lt; c");
    }

    #[test]
    fn escaped_text_reads_back_unchanged() {
        let original = "a & b < c > d \" '";
        let xml = fixtures::envelope(&format!("<m:X>{}</m:X>", escape(original)));
        let doc = read(StatusCode::OK, &xml).unwrap();
        let x = soap_body(&doc).unwrap().elements().next().unwrap();
        assert_eq!(x.text(), original);
    }
}
