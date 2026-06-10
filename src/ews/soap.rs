//! Low-level SOAP transport for EWS.
//!
//! EWS exposes a SOAP 1.1 endpoint at `<server>/EWS/Exchange.asmx`. Requests
//! are XML envelopes wrapping a `<m:...>` (message) operation; responses come
//! back as similar envelopes carrying a `<m:...Response>` body. Most calrs
//! operations are simple enough that we build the envelope as a string and
//! parse the response with the same string-based extractor used in
//! `src/caldav/mod.rs` — that keeps the dependency surface tight and matches
//! the existing house style.
//!
//! Authentication uses HTTP Basic over TLS. NTLM and Kerberos are common in
//! on-prem Exchange but require external crates; on-prem 2019 admins can
//! enable Basic on a service mailbox or front EWS with a reverse proxy that
//! does the negotiation for us.

use anyhow::{bail, Context, Result};
use reqwest::{Client, StatusCode};
use std::time::Duration;

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

/// Wrap a SOAP body in a complete envelope with the standard headers.
///
/// When `impersonate_email` is `Some`, an `<t:ExchangeImpersonation>` header is
/// added so the request executes against that mailbox instead of the
/// authenticating principal's. This is the admin-controlled "Global EWS via
/// Impersonation" path — the connecting account must hold the
/// `ApplicationImpersonation` RBAC role on Exchange.
pub fn envelope(body: &str, impersonate_email: Option<&str>) -> String {
    let impersonation = match impersonate_email {
        Some(mb) if !mb.is_empty() => format!(
            "    <t:ExchangeImpersonation>\n      <t:ConnectingSID>\n        <t:PrimarySmtpAddress>{}</t:PrimarySmtpAddress>\n      </t:ConnectingSID>\n    </t:ExchangeImpersonation>\n",
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

/// Send a SOAP request and return the response body. The caller passes the
/// inner SOAP body; this function adds the envelope, headers, and basic auth.
///
/// When `impersonate_email` is `Some`, the envelope carries an
/// `<t:ExchangeImpersonation>` header — the request runs as that mailbox under
/// the connecting service account's RBAC `ApplicationImpersonation` grant.
pub async fn post_soap(
    endpoint: &str,
    username: &str,
    password: &str,
    body: &str,
    fetch: bool,
    impersonate_email: Option<&str>,
) -> Result<String> {
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
    tracing::trace!(envelope = %envelope, "EWS SOAP request envelope");
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
    let text = resp.text().await.unwrap_or_default();
    tracing::debug!(endpoint = %endpoint, status = %status, body_len = text.len(), "EWS SOAP response");

    // EWS returns SOAP Faults inside a 200 envelope or 500 with a fault body.
    // Tolerate 500 if the body is parseable as a fault — `extract_soap_fault`
    // surfaces a descriptive error.
    if !status.is_success() && status != StatusCode::INTERNAL_SERVER_ERROR {
        if status == StatusCode::UNAUTHORIZED {
            bail!("EWS authentication failed (401). Check username/password.");
        }
        bail!("EWS request returned {status} for {endpoint}");
    }

    if let Some(fault) = extract_soap_fault(&text) {
        bail!("EWS SOAP fault: {fault}");
    }

    Ok(text)
}

/// Pull a human-readable error out of a SOAP fault response.
pub fn extract_soap_fault(xml: &str) -> Option<String> {
    if !xml.contains("Fault") && !xml.contains("ResponseCode") {
        return None;
    }
    // Errors come either as a SOAP Fault or as a per-message ResponseCode.
    if let Some(reason) =
        first_tag_content(xml, "faultstring").or_else(|| first_tag_content(xml, "Reason"))
    {
        return Some(reason);
    }
    // Per-message error: ResponseCode != "NoError" with optional MessageText.
    let response_code = first_tag_content(xml, "ResponseCode");
    if let Some(code) = &response_code {
        if code == "NoError" {
            return None;
        }
        let detail = first_tag_content(xml, "MessageText").unwrap_or_default();
        return Some(if detail.is_empty() {
            code.clone()
        } else {
            format!("{code}: {detail}")
        });
    }
    None
}

/// Best-effort scan for the first occurrence of an XML tag's content,
/// regardless of namespace prefix. Mirrors the helpers in `caldav::mod` and is
/// good enough for SOAP responses, which are deterministic in shape.
pub fn first_tag_content(xml: &str, local_name: &str) -> Option<String> {
    // Case 1: explicit prefix `<x:Local>...</x:Local>`
    let needle_prefixed = format!(":{local_name}");
    let close_marker = format!("/{local_name}>");

    let mut search_from = 0;
    while let Some(pos) = xml[search_from..].find(&needle_prefixed) {
        let abs = search_from + pos;
        // Walk back to '<' to confirm this is a tag opener.
        let before = &xml[..abs];
        if let Some(lt) = before.rfind('<') {
            let prefix_part = &xml[lt + 1..abs];
            if !prefix_part.is_empty()
                && prefix_part.len() <= 16
                && prefix_part.chars().all(|c| c.is_alphanumeric() || c == '_')
            {
                let open_tag_end = abs + needle_prefixed.len();
                if let Some(after) = xml[open_tag_end..].find('>') {
                    let content_start = open_tag_end + after + 1;
                    let close_full = format!("</{prefix_part}:{local_name}>");
                    if let Some(end_rel) = xml[content_start..].find(&close_full) {
                        let value = xml[content_start..content_start + end_rel].trim();
                        if !value.is_empty() {
                            return Some(value.to_string());
                        }
                    }
                }
            }
        }
        search_from = abs + needle_prefixed.len();
    }

    // Case 2: unprefixed tag `<Local>...</Local>` (rare in SOAP but cheap to
    // try as a fallback).
    let open = format!("<{local_name}");
    if let Some(pos) = xml.find(&open) {
        let after_open = &xml[pos + open.len()..];
        let next_byte = after_open.as_bytes().first().copied();
        if next_byte == Some(b'>') || next_byte == Some(b' ') || next_byte == Some(b'/') {
            if let Some(close_at) = xml[pos + open.len()..].find('>') {
                let content_start = pos + open.len() + close_at + 1;
                let close_full = format!("</{local_name}>");
                if let Some(end_rel) = xml[content_start..].find(&close_full) {
                    let value = xml[content_start..content_start + end_rel].trim();
                    if !value.is_empty() {
                        return Some(value.to_string());
                    }
                }
                // Self-closing tag: skip
                let _ = close_marker;
            }
        }
    }
    None
}

/// Find every occurrence of a tag's content (any namespace prefix).
pub fn collect_tag_contents(xml: &str, local_name: &str) -> Vec<String> {
    let mut out = Vec::new();
    let needle = format!(":{local_name}");
    let mut search_from = 0;
    while let Some(pos) = xml[search_from..].find(&needle) {
        let abs = search_from + pos;
        let before = &xml[..abs];
        if let Some(lt) = before.rfind('<') {
            let prefix_part = &xml[lt + 1..abs];
            if !prefix_part.is_empty()
                && prefix_part.len() <= 16
                && prefix_part.chars().all(|c| c.is_alphanumeric() || c == '_')
            {
                let open_tag_end = abs + needle.len();
                if let Some(after) = xml[open_tag_end..].find('>') {
                    let content_start = open_tag_end + after + 1;
                    let close_full = format!("</{prefix_part}:{local_name}>");
                    if let Some(end_rel) = xml[content_start..].find(&close_full) {
                        let value = xml[content_start..content_start + end_rel].trim();
                        if !value.is_empty() {
                            out.push(value.to_string());
                        }
                        search_from = content_start + end_rel + close_full.len();
                        continue;
                    }
                }
            }
        }
        search_from = abs + needle.len();
    }
    out
}

/// Find every block bounded by an opening tag (any prefix) and its closing
/// counterpart. Used to slice a multi-item response into per-item windows so
/// downstream parsing operates on a single record at a time.
pub fn collect_blocks(xml: &str, local_name: &str) -> Vec<String> {
    let mut out = Vec::new();
    let needle = format!(":{local_name}");
    let mut search_from = 0;
    while let Some(pos) = xml[search_from..].find(&needle) {
        let abs = search_from + pos;
        let before = &xml[..abs];
        let lt = match before.rfind('<') {
            Some(idx) => idx,
            None => {
                search_from = abs + needle.len();
                continue;
            }
        };
        let prefix_part = &xml[lt + 1..abs];
        if prefix_part.is_empty()
            || prefix_part.len() > 16
            || !prefix_part.chars().all(|c| c.is_alphanumeric() || c == '_')
        {
            search_from = abs + needle.len();
            continue;
        }
        let open_tag_end = abs + needle.len();
        let after_attrs = match xml[open_tag_end..].find('>') {
            Some(a) => open_tag_end + a + 1,
            None => break,
        };
        let close_full = format!("</{prefix_part}:{local_name}>");
        match xml[after_attrs..].find(&close_full) {
            Some(end_rel) => {
                let block_end = after_attrs + end_rel;
                out.push(xml[after_attrs..block_end].to_string());
                search_from = block_end + close_full.len();
            }
            None => break,
        }
    }
    out
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

/// Inverse of `escape` — XML entities to their literal characters. Used when
/// surfacing values from server responses (e.g. ItemId attributes).
pub fn unescape(text: &str) -> String {
    text.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

/// Pull an attribute value out of an XML opening tag fragment.
/// `tag_xml` is something like `<t:ItemId Id="AAA" ChangeKey="BBB" />`.
pub fn attr(tag_xml: &str, attr_name: &str) -> Option<String> {
    let pat = format!("{attr_name}=\"");
    let pos = tag_xml.find(&pat)?;
    let start = pos + pat.len();
    let end = tag_xml[start..].find('"')?;
    Some(tag_xml[start..start + end].to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_simple_tag_content() {
        let xml = "<m:ResponseCode>NoError</m:ResponseCode>";
        assert_eq!(
            first_tag_content(xml, "ResponseCode"),
            Some("NoError".to_string())
        );
    }

    #[test]
    fn extract_with_attributes() {
        let xml = r#"<t:Body BodyType="HTML">hello</t:Body>"#;
        assert_eq!(first_tag_content(xml, "Body"), Some("hello".to_string()));
    }

    #[test]
    fn extract_returns_none_for_self_closing() {
        let xml = r#"<t:ItemId Id="AAA" ChangeKey="BBB" />"#;
        // Self-closing tags have no content, so no match.
        assert_eq!(first_tag_content(xml, "ItemId"), None);
    }

    #[test]
    fn fault_detection() {
        let xml = "<soap:Fault><faultstring>auth required</faultstring></soap:Fault>";
        assert_eq!(extract_soap_fault(xml), Some("auth required".to_string()));
    }

    #[test]
    fn fault_from_response_code() {
        let xml = "<m:ResponseCode>ErrorAccessDenied</m:ResponseCode><m:MessageText>Access is denied.</m:MessageText>";
        assert_eq!(
            extract_soap_fault(xml),
            Some("ErrorAccessDenied: Access is denied.".to_string())
        );
    }

    #[test]
    fn fault_no_error_returns_none() {
        let xml = "<m:ResponseCode>NoError</m:ResponseCode>";
        assert_eq!(extract_soap_fault(xml), None);
    }

    #[test]
    fn collect_blocks_returns_each_record() {
        let xml = r#"<m:Items>
            <t:CalendarItem><t:Subject>One</t:Subject></t:CalendarItem>
            <t:CalendarItem><t:Subject>Two</t:Subject></t:CalendarItem>
        </m:Items>"#;
        let blocks = collect_blocks(xml, "CalendarItem");
        assert_eq!(blocks.len(), 2);
        assert!(blocks[0].contains("One"));
        assert!(blocks[1].contains("Two"));
    }

    #[test]
    fn attr_extracts_value() {
        let tag = r#"<t:ItemId Id="AAAB" ChangeKey="CK1"/>"#;
        assert_eq!(attr(tag, "Id"), Some("AAAB".to_string()));
        assert_eq!(attr(tag, "ChangeKey"), Some("CK1".to_string()));
        assert_eq!(attr(tag, "Missing"), None);
    }

    #[test]
    fn escape_special_chars() {
        assert_eq!(escape("a & b < c"), "a &amp; b &lt; c");
    }

    #[test]
    fn unescape_roundtrip() {
        let original = "a & b < c > d";
        assert_eq!(unescape(&escape(original)), original);
    }
}
