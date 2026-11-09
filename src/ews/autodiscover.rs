//! Exchange Autodiscover (POX / XML).
//!
//! Given an email address, derive the SOAP EWS endpoint by asking the user's
//! domain Autodiscover service. We follow the legacy POX flow described in
//! [MS-OXDSCLI]: try each candidate URL in order until one returns an XML
//! response containing `<EwsUrl>` (or `Protocol/EXCH` with an EWS attribute).
//!
//! The newer JSON v2 endpoint (`/autodiscover/autodiscover.json`) is mostly an
//! Office 365 thing; on-prem 2019 still serves POX, which is the priority
//! target for this implementation.

use anyhow::{bail, Context, Result};
use std::time::Duration;

const TIMEOUT: Duration = Duration::from_secs(10);

/// Largest Autodiscover answer read, as it arrives.
const MAX_REPLY_BYTES: usize = 1024 * 1024;

/// Try a sequence of Autodiscover URLs derived from `email_domain` and return
/// the EWS endpoint discovered, if any. The candidate order matches Microsoft
/// guidance: HTTPS root, then `autodiscover` subdomain, then unauthenticated
/// HTTP redirect, then SRV (DNS) — the SRV branch is left as a TODO since it
/// requires a DNS resolver.
pub async fn discover_ews_url(email: &str, password: &str) -> Result<String> {
    let domain = email
        .rsplit('@')
        .next()
        .filter(|d| !d.is_empty())
        .context("autodiscover requires an email address with a domain")?;

    let candidates = [
        format!("https://autodiscover.{domain}/autodiscover/autodiscover.xml"),
        format!("https://{domain}/autodiscover/autodiscover.xml"),
    ];

    let body = pox_request_body(email);
    let client = crate::net_guard::client_builder()
        .timeout(TIMEOUT)
        // Follow up to two redirects: Autodiscover often replies 302 to a
        // canonical URL on the same domain. Every host in the chain goes
        // through the guarded resolver (#363), and a redirect to a private
        // IP literal is refused, so a hostile responder cannot bounce the
        // request to an internal host. The server-supplied EwsUrl from the
        // final response is revalidated below before being persisted.
        .redirect(crate::net_guard::redirects(
            2,
            crate::net_guard::Allowlist::Settings,
        ))
        .build()
        .context("HTTP client build failed")?;

    let mut last_err: Option<anyhow::Error> = None;
    for url in &candidates {
        // Re-use the shared validator: rejects non-HTTPS and private/loopback
        // hosts so a hostile email domain can't probe internal infrastructure.
        if let Err(e) = crate::caldav::validate_caldav_url(url) {
            tracing::debug!(candidate = %url, error = %e, "skipping autodiscover candidate (validator rejected)");
            continue;
        }
        match try_one(&client, url, email, password, &body).await {
            Ok(Some(ews_url)) => {
                // The server-supplied EwsUrl is later validated again when
                // the source is persisted; double-check here for early
                // failure with a helpful message.
                if let Err(e) = crate::caldav::validate_caldav_url(&ews_url) {
                    last_err = Some(anyhow::anyhow!(
                        "Autodiscover returned an unsafe EWS URL ({ews_url}): {e}"
                    ));
                    continue;
                }
                return Ok(ews_url);
            }
            Ok(None) => continue,
            Err(e) => last_err = Some(e),
        }
    }

    bail!(
        "Autodiscover did not return an EWS endpoint for {email}. Tried: {}. Last error: {}",
        candidates.join(", "),
        last_err
            .map(|e| e.to_string())
            .unwrap_or_else(|| "no response".to_string()),
    )
}

async fn try_one(
    client: &reqwest::Client,
    url: &str,
    username: &str,
    password: &str,
    body: &str,
) -> Result<Option<String>> {
    let resp = client
        .post(url)
        .basic_auth(username, Some(password))
        .header("Content-Type", "text/xml; charset=utf-8")
        .body(body.to_string())
        .send()
        .await;

    let resp = match resp {
        Ok(r) => r,
        Err(e) => {
            tracing::debug!(url = %url, error = %e, "autodiscover candidate unreachable");
            return Ok(None);
        }
    };

    let status = resp.status();
    if !status.is_success() {
        tracing::debug!(url = %url, status = %status, "autodiscover candidate returned error");
        return Ok(None);
    }

    // An Autodiscover answer is a few kilobytes: read no more than this.
    let text = crate::caldav::read_body(resp, MAX_REPLY_BYTES).await?;
    Ok(parse_pox_response(&text))
}

/// Build the standard POX Autodiscover request body. The schema is fixed by
/// Microsoft and `EMailAddress` is the only variable.
fn pox_request_body(email: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
<Autodiscover xmlns="http://schemas.microsoft.com/exchange/autodiscover/outlook/requestschema/2006">
  <Request>
    <EMailAddress>{email}</EMailAddress>
    <AcceptableResponseSchema>http://schemas.microsoft.com/exchange/autodiscover/outlook/responseschema/2006a</AcceptableResponseSchema>
  </Request>
</Autodiscover>"#,
        email = super::soap::escape(email),
    )
}

/// Extract the EWS URL from an Autodiscover POX response. We accept either:
/// - the explicit `<EwsUrl>` element (most modern response shape), the first
///   one in the document
/// - failing that, an `<ASUrl>` (ActiveSync), whose host serves EWS too
///
/// The response is read with the shared XML parser (#369). Autodiscover
/// nests several schema namespaces, which vary between Exchange versions, so
/// these elements are matched by local name. A reply that is not well-formed
/// XML holds no endpoint: `None`, and the next candidate is tried.
pub fn parse_pox_response(xml: &str) -> Option<String> {
    let root = match crate::xml::parse(xml) {
        Ok(Some(root)) => root,
        Ok(None) => return None,
        Err(e) => {
            tracing::debug!(error = %e, "autodiscover reply is not XML");
            return None;
        }
    };
    let first_text = |name: &str| {
        std::iter::once(&root)
            .chain(root.descendants())
            .filter(|el| el.local_name() == name)
            .find_map(|el| el.trimmed_text())
    };
    if let Some(url) = first_text("EwsUrl") {
        return Some(url);
    }
    if let Some(url) = first_text("ASUrl") {
        // Some servers return the active sync URL in ASUrl which has the same host;
        // build a sensible EWS URL from it as a last resort.
        if let Ok(parsed) = reqwest::Url::parse(&url) {
            return Some(format!(
                "{}://{}/EWS/Exchange.asmx",
                parsed.scheme(),
                parsed.host_str()?
            ));
        }
    }
    None
}

/// Without contacting the network, build the conventional EWS URL for a
/// domain. Useful as a fallback when Autodiscover is blocked but the admin
/// knows the canonical hostname.
pub fn conventional_ews_url(host: &str) -> String {
    format!("https://{}/EWS/Exchange.asmx", host)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_modern_autodiscover_response() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<Autodiscover xmlns="http://schemas.microsoft.com/exchange/autodiscover/responseschema/2006">
  <Response xmlns="http://schemas.microsoft.com/exchange/autodiscover/outlook/responseschema/2006a">
    <Account>
      <Protocol>
        <Type>EXCH</Type>
        <EwsUrl>https://mail.example.com/EWS/Exchange.asmx</EwsUrl>
        <EmwsUrl>https://mail.example.com/EWS/Exchange.asmx</EmwsUrl>
      </Protocol>
    </Account>
  </Response>
</Autodiscover>"#;
        assert_eq!(
            parse_pox_response(xml),
            Some("https://mail.example.com/EWS/Exchange.asmx".to_string())
        );
    }

    #[test]
    fn fallback_to_asurl() {
        let xml = r#"<Response><Action><Settings>
            <ASUrl>https://mail.example.com/Microsoft-Server-ActiveSync</ASUrl>
        </Settings></Action></Response>"#;
        assert_eq!(
            parse_pox_response(xml),
            Some("https://mail.example.com/EWS/Exchange.asmx".to_string())
        );
    }

    /// Prefixes do not matter, entities are decoded, and a reply that is
    /// not XML (an HTML page, a body cut short) holds no endpoint.
    #[test]
    fn prefixed_and_broken_replies() {
        let xml = r#"<a:Autodiscover xmlns:a="http://schemas.microsoft.com/exchange/autodiscover/responseschema/2006"><o:Response xmlns:o="http://schemas.microsoft.com/exchange/autodiscover/outlook/responseschema/2006a"><o:Account><o:Protocol><o:Type>EXCH</o:Type><o:EwsUrl> https://mail.example.com/EWS/Exchange.asmx?a=1&amp;b=2 </o:EwsUrl></o:Protocol></o:Account></o:Response></a:Autodiscover>"#;
        assert_eq!(
            parse_pox_response(xml),
            Some("https://mail.example.com/EWS/Exchange.asmx?a=1&b=2".to_string())
        );
        for broken in [
            "",
            "<html><body>Sign in<br></body></html>",
            "<Autodiscover><Response><Account><Protocol><EwsUrl>https://mail.example.com/EWS/Exch",
        ] {
            assert_eq!(parse_pox_response(broken), None, "{broken:?}");
        }
        // An empty EwsUrl is skipped for a later one.
        let xml = "<Autodiscover><Response><Protocol><EwsUrl/></Protocol><Protocol><EwsUrl>https://b.example/EWS/Exchange.asmx</EwsUrl></Protocol></Response></Autodiscover>";
        assert_eq!(
            parse_pox_response(xml),
            Some("https://b.example/EWS/Exchange.asmx".to_string())
        );
    }

    #[test]
    fn no_match_returns_none() {
        let xml = "<Autodiscover><Response><Error>NotFound</Error></Response></Autodiscover>";
        assert_eq!(parse_pox_response(xml), None);
    }

    #[test]
    fn conventional_url_format() {
        assert_eq!(
            conventional_ews_url("mail.example.com"),
            "https://mail.example.com/EWS/Exchange.asmx",
        );
    }
}
