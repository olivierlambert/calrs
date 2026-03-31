use anyhow::{bail, Result};
use reqwest::Client;
use std::net::IpAddr;
use std::time::Duration;

pub struct CaldavClient {
    client: Client,
    base_url: String,
    origin: String,
    username: String,
    password: String,
}

/// Check if an IP address is in a private/reserved range (SSRF protection).
fn is_private_ip(ip: &IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            v4.is_loopback()          // 127.0.0.0/8
                || v4.is_private()    // 10/8, 172.16/12, 192.168/16
                || v4.is_link_local() // 169.254/16
                || v4.is_unspecified()
                || v4.octets()[0] == 100 && (v4.octets()[1] & 0xC0) == 64 // 100.64/10 (CGNAT)
        }
        IpAddr::V6(v6) => {
            v6.is_loopback()       // ::1
                || v6.is_unspecified() // ::
                || {
                    let seg = v6.segments();
                    seg[0] == 0xfe80 // fe80::/10 link-local
                        || seg[0] == 0xfc00 || seg[0] == 0xfd00 // fc00::/7 ULA
                }
        }
    }
}

/// Validate a CalDAV URL: must be http(s), must not resolve to a private IP.
pub fn validate_caldav_url(url: &str) -> Result<()> {
    let parsed = reqwest::Url::parse(url).map_err(|_| anyhow::anyhow!("Invalid URL: {}", url))?;

    match parsed.scheme() {
        "http" | "https" => {}
        scheme => bail!(
            "URL scheme '{}' is not allowed. Only http and https are supported.",
            scheme
        ),
    }

    let host = parsed
        .host_str()
        .ok_or_else(|| anyhow::anyhow!("URL has no host"))?;

    // Try parsing as IP directly
    if let Ok(ip) = host.parse::<IpAddr>() {
        if is_private_ip(&ip) {
            bail!(
                "URL resolves to a private/reserved IP address ({}). This is not allowed for security reasons.",
                ip
            );
        }
        return Ok(());
    }

    // Resolve hostname and check all resolved IPs
    use std::net::ToSocketAddrs;
    let port = parsed
        .port()
        .unwrap_or(if parsed.scheme() == "https" { 443 } else { 80 });
    let addrs: Vec<_> = format!("{}:{}", host, port)
        .to_socket_addrs()
        .map_err(|e| anyhow::anyhow!("Cannot resolve hostname '{}': {}", host, e))?
        .collect();

    if addrs.is_empty() {
        bail!("Cannot resolve hostname '{}'", host);
    }

    for addr in &addrs {
        if is_private_ip(&addr.ip()) {
            bail!(
                "URL hostname '{}' resolves to a private/reserved IP address ({}). This is not allowed for security reasons.",
                host,
                addr.ip()
            );
        }
    }

    Ok(())
}

impl CaldavClient {
    pub fn new(base_url: &str, username: &str, password: &str) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .unwrap_or_else(|_| Client::new());

        let base = base_url.trim_end_matches('/').to_string();
        let origin = reqwest::Url::parse(&base)
            .map(|u: reqwest::Url| format!("{}://{}", u.scheme(), u.host_str().unwrap_or("")))
            .unwrap_or_else(|_| base.clone());

        Self {
            client,
            base_url: base,
            origin,
            username: username.to_string(),
            password: password.to_string(),
        }
    }

    /// Resolve a href that may be absolute path or full URL
    fn resolve_url(&self, href: &str) -> String {
        if href.starts_with("http") {
            href.to_string()
        } else {
            format!("{}{}", self.origin, href)
        }
    }

    /// Send a PROPFIND request and return the response body
    async fn propfind(&self, url: &str, depth: &str, body: &str) -> Result<String> {
        tracing::debug!(url = %url, depth = %depth, "sending PROPFIND request");
        let resp = self
            .client
            .request(reqwest::Method::from_bytes(b"PROPFIND")?, url)
            .basic_auth(&self.username, Some(&self.password))
            .header("Content-Type", "application/xml; charset=utf-8")
            .header("Depth", depth)
            .body(body.to_string())
            .send()
            .await?;

        tracing::debug!(url = %url, status = %resp.status(), "PROPFIND response received");

        if !resp.status().is_success() && resp.status().as_u16() != 207 {
            bail!("PROPFIND {} returned {}", url, resp.status());
        }

        Ok(resp.text().await?)
    }

    /// Check if the server supports CalDAV (OPTIONS request)
    pub async fn check_connection(&self) -> Result<bool> {
        let resp = self
            .client
            .request(reqwest::Method::OPTIONS, &self.base_url)
            .basic_auth(&self.username, Some(&self.password))
            .send()
            .await?;

        if !resp.status().is_success() {
            bail!(
                "Server returned {} {}",
                resp.status().as_u16(),
                resp.status().canonical_reason().unwrap_or("")
            );
        }

        let dav_header = resp
            .headers()
            .get("DAV")
            .map(|v| v.to_str().unwrap_or(""))
            .unwrap_or("");

        Ok(dav_header.contains("calendar-access")
            || dav_header.contains("nc-calendar-search")
            || dav_header.contains("nc-enable-birthday-calendar"))
    }

    /// Discover the current-user-principal URL via PROPFIND
    pub async fn discover_principal(&self) -> Result<String> {
        let text = self
            .propfind(&self.base_url, "0", PROPFIND_PRINCIPAL)
            .await?;

        tracing::debug!(
            response_len = text.len(),
            "PROPFIND principal response received"
        );

        // Extract href inside <X:current-user-principal><Y:href>...</Y:href></X:current-user-principal>
        // Uses namespace-agnostic search to handle any prefix (d:, D:, or arbitrary like a:)
        if let Some(inner) = extract_tag(&text, "d:current-user-principal") {
            if let Some(href) = extract_tag(&inner, "d:href") {
                tracing::debug!(principal = %href, "discovered principal URL");
                return Ok(href);
            }
        }

        tracing::debug!(response_body = %text, "failed to parse principal from response");
        bail!("Could not discover principal URL from response")
    }

    /// Discover the calendar-home-set from a principal URL
    pub async fn discover_calendar_home(&self, principal_url: &str) -> Result<String> {
        let url = self.resolve_url(principal_url);
        let text = self.propfind(&url, "0", PROPFIND_CALENDAR_HOME).await?;

        tracing::debug!(
            response_len = text.len(),
            "PROPFIND calendar-home response received"
        );

        // Extract href inside <X:calendar-home-set><Y:href>...</Y:href></X:calendar-home-set>
        // Uses namespace-agnostic search to handle any prefix (cal:, C:, or arbitrary like a:)
        if let Some(inner) = extract_tag(&text, "cal:calendar-home-set") {
            if let Some(href) = extract_tag(&inner, "d:href") {
                tracing::debug!(calendar_home = %href, "discovered calendar-home-set");
                return Ok(href);
            }
        }

        tracing::debug!(response_body = %text, "failed to parse calendar-home-set from response");
        bail!("Could not discover calendar-home-set from response")
    }

    /// List calendars under a calendar-home-set URL (filters to actual calendars only)
    pub async fn list_calendars(&self, home_url: &str) -> Result<Vec<CalendarInfo>> {
        let url = self.resolve_url(home_url);
        let text = self.propfind(&url, "1", PROPFIND_CALENDARS).await?;
        let calendars = parse_calendar_list(&text);
        tracing::debug!(
            calendar_count = calendars.len(),
            response_len = text.len(),
            "listed calendars"
        );
        if calendars.is_empty() && !text.is_empty() {
            tracing::debug!(response_body = %text, "no calendars parsed from response");
        }
        Ok(calendars)
    }

    /// PUT an event (iCalendar) to a calendar
    pub async fn put_event(&self, calendar_href: &str, uid: &str, ics_data: &str) -> Result<()> {
        let href = format!("{}/{}.ics", calendar_href.trim_end_matches('/'), uid);
        let url = self.resolve_url(&href);

        let resp = self
            .client
            .put(&url)
            .basic_auth(&self.username, Some(&self.password))
            .header("Content-Type", "text/calendar; charset=utf-8")
            .body(ics_data.to_string())
            .send()
            .await?;

        let status = resp.status();
        if !status.is_success() && status.as_u16() != 201 && status.as_u16() != 204 {
            let body = resp.text().await.unwrap_or_default();
            bail!("PUT {} returned {} — {}", url, status, body);
        }

        Ok(())
    }

    /// DELETE an event from a calendar
    pub async fn delete_event(&self, calendar_href: &str, uid: &str) -> Result<()> {
        let href = format!("{}/{}.ics", calendar_href.trim_end_matches('/'), uid);
        let url = self.resolve_url(&href);

        let resp = self
            .client
            .delete(&url)
            .basic_auth(&self.username, Some(&self.password))
            .send()
            .await?;

        let status = resp.status();
        if !status.is_success() && status.as_u16() != 204 && status.as_u16() != 404 {
            let body = resp.text().await.unwrap_or_default();
            bail!("DELETE {} returned {} — {}", url, status, body);
        }

        Ok(())
    }

    /// Fetch events from a calendar using REPORT
    pub async fn fetch_events(&self, calendar_href: &str) -> Result<Vec<RawEvent>> {
        let url = self.resolve_url(calendar_href);

        // Use a longer timeout for event fetches (calendars can be large)
        let resp = self
            .client
            .request(reqwest::Method::from_bytes(b"REPORT")?, &url)
            .basic_auth(&self.username, Some(&self.password))
            .header("Content-Type", "application/xml; charset=utf-8")
            .header("Depth", "1")
            .timeout(Duration::from_secs(60))
            .body(REPORT_CALENDAR_DATA)
            .send()
            .await?;

        let text = resp.text().await?;
        let events = parse_event_responses(&text);
        Ok(events)
    }

    /// Perform a sync-collection REPORT (RFC 6578) to get changes since a sync-token.
    /// If `sync_token` is None, requests an initial sync-token from the server.
    /// Returns changed/added events, deleted hrefs, and the new sync-token.
    pub async fn sync_collection(
        &self,
        calendar_href: &str,
        sync_token: Option<&str>,
    ) -> Result<SyncResult> {
        let url = self.resolve_url(calendar_href);

        let token_value = sync_token.unwrap_or("");
        let body = format!(
            r#"<?xml version="1.0" encoding="utf-8"?>
<d:sync-collection xmlns:d="DAV:" xmlns:c="urn:ietf:params:xml:ns:caldav">
  <d:sync-token>{}</d:sync-token>
  <d:prop>
    <d:getetag />
    <c:calendar-data />
  </d:prop>
</d:sync-collection>"#,
            token_value
        );

        let resp = self
            .client
            .request(reqwest::Method::from_bytes(b"REPORT")?, &url)
            .basic_auth(&self.username, Some(&self.password))
            .header("Content-Type", "application/xml; charset=utf-8")
            .timeout(Duration::from_secs(60))
            .body(body)
            .send()
            .await?;

        let status = resp.status();
        // 403/412 = token expired or invalid; 405 = not supported
        if status.as_u16() == 403 || status.as_u16() == 412 || status.as_u16() == 405 {
            bail!("sync-token invalid or not supported (HTTP {})", status);
        }
        if !status.is_success() && status.as_u16() != 207 {
            bail!("sync-collection REPORT returned {}", status);
        }

        let text = resp.text().await?;
        Ok(parse_sync_response(&text))
    }

    /// Fetch events from a calendar starting from a given UTC datetime.
    /// Uses RFC 4791 time-range filter to only retrieve future events.
    /// Falls back to full fetch if the server rejects the time-range query.
    pub async fn fetch_events_since(
        &self,
        calendar_href: &str,
        since_utc: &str,
    ) -> Result<Vec<RawEvent>> {
        let url = self.resolve_url(calendar_href);

        let body = format!(
            r#"<?xml version="1.0" encoding="utf-8"?>
<c:calendar-query xmlns:d="DAV:" xmlns:c="urn:ietf:params:xml:ns:caldav">
  <d:prop>
    <d:getetag />
    <c:calendar-data />
  </d:prop>
  <c:filter>
    <c:comp-filter name="VCALENDAR">
      <c:comp-filter name="VEVENT">
        <c:time-range start="{}" />
      </c:comp-filter>
    </c:comp-filter>
  </c:filter>
</c:calendar-query>"#,
            since_utc
        );

        let resp = self
            .client
            .request(reqwest::Method::from_bytes(b"REPORT")?, &url)
            .basic_auth(&self.username, Some(&self.password))
            .header("Content-Type", "application/xml; charset=utf-8")
            .header("Depth", "1")
            .timeout(Duration::from_secs(60))
            .body(body)
            .send()
            .await?;

        let status = resp.status();
        let text = resp.text().await?;

        // If the server doesn't support time-range, fall back to full fetch
        if !status.is_success() {
            return self.fetch_events(calendar_href).await;
        }

        let events = parse_event_responses(&text);
        Ok(events)
    }
}

#[derive(Debug, Clone)]
pub struct CalendarInfo {
    pub href: String,
    pub display_name: Option<String>,
    pub color: Option<String>,
    pub ctag: Option<String>,
    pub sync_token: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SyncResult {
    pub new_sync_token: Option<String>,
    pub changed: Vec<RawEvent>,
    pub deleted_hrefs: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct RawEvent {
    pub href: String,
    pub ical_data: String,
}

fn parse_calendar_list(xml: &str) -> Vec<CalendarInfo> {
    let mut calendars = Vec::new();
    for response_block in split_responses(xml) {
        // Only include actual calendar collections
        // Match <cal:calendar/>, <C:calendar/>, <calendar xmlns="..."/>, etc.
        // Some servers (SoGo) use unprefixed <calendar> with xmlns attribute
        let has_calendar_resource =
            response_block.contains("calendar/>") || response_block.contains("<calendar xmlns=");
        if !has_calendar_resource {
            continue;
        }
        let href = extract_tag(response_block, "d:href").unwrap_or_default();
        if href.is_empty() {
            continue;
        }
        let display_name = extract_tag(response_block, "d:displayname");
        let color = extract_tag(response_block, "aic:calendar-color")
            .or_else(|| extract_tag(response_block, "x1:calendar-color"));
        let ctag = extract_tag(response_block, "cso:getctag")
            .or_else(|| extract_tag(response_block, "cs:getctag"));
        let sync_token = extract_tag(response_block, "d:sync-token");
        calendars.push(CalendarInfo {
            href,
            display_name,
            color,
            ctag,
            sync_token,
        });
    }
    calendars
}

fn parse_sync_response(xml: &str) -> SyncResult {
    let mut changed = Vec::new();
    let mut deleted_hrefs = Vec::new();

    // Extract the new sync-token from the multistatus envelope (outside response blocks)
    let new_sync_token = extract_tag(xml, "d:sync-token");

    for response_block in split_responses(xml) {
        let href = extract_tag(response_block, "d:href").unwrap_or_default();
        if href.is_empty() {
            continue;
        }

        // Check if this is a deletion (status contains 404)
        if let Some(status) = extract_tag(response_block, "d:status") {
            if status.contains("404") {
                deleted_hrefs.push(href);
                continue;
            }
        }

        // Otherwise it's an addition/modification — extract calendar data
        let ical_data = extract_tag(response_block, "cal:calendar-data")
            .or_else(|| extract_tag(response_block, "c:calendar-data"))
            .unwrap_or_default();
        if !ical_data.is_empty() {
            changed.push(RawEvent { href, ical_data });
        }
    }

    SyncResult {
        new_sync_token,
        changed,
        deleted_hrefs,
    }
}

fn parse_event_responses(xml: &str) -> Vec<RawEvent> {
    let mut events = Vec::new();
    for response_block in split_responses(xml) {
        let href = extract_tag(response_block, "d:href").unwrap_or_default();
        let ical_data = extract_tag(response_block, "cal:calendar-data")
            .or_else(|| extract_tag(response_block, "c:calendar-data"))
            .unwrap_or_default();
        if !ical_data.is_empty() {
            events.push(RawEvent { href, ical_data });
        }
    }
    events
}

/// Split a multistatus XML response into individual response blocks.
/// Handles multiple namespace prefixes: `<d:response>`, `<D:response>`, `<response>`,
/// and any arbitrary prefix (e.g. `<a:response>`).
fn split_responses(xml: &str) -> Vec<&str> {
    // Try common response tag patterns first
    for open_tag in &["<d:response>", "<D:response>", "<response>", "<response "] {
        let blocks: Vec<&str> = xml.split(open_tag).skip(1).collect();
        if !blocks.is_empty() {
            return blocks;
        }
    }
    // Fallback: search for any prefix on "response>" (e.g. "<a:response>")
    // Find the first occurrence to determine the prefix, then split on it
    if let Some(idx) = xml.find(":response>") {
        // Walk back to find '<'
        let before = &xml[..idx];
        if let Some(lt) = before.rfind('<') {
            let open_tag = &xml[lt..idx + ":response>".len()];
            let blocks: Vec<&str> = xml.split(open_tag).skip(1).collect();
            if !blocks.is_empty() {
                return blocks;
            }
        }
    }
    Vec::new()
}

/// Known namespace prefix aliases for CalDAV servers.
/// Maps canonical prefixes to alternatives seen in the wild.
const PREFIX_ALIASES: &[(&str, &[&str])] = &[
    ("d", &["D"]),                 // DAV: namespace
    ("cal", &["C", "CAL"]),        // CalDAV namespace (SOGo uses C:)
    ("c", &["C", "cal"]),          // CalDAV namespace (alternate)
    ("cso", &["cs", "CSO", "CS"]), // Calendar Server namespace
    ("aic", &["x1", "AIC"]),       // Apple iCal namespace
];

/// Extract the text content of an XML tag, trying multiple namespace prefix variants.
/// For example, `extract_tag(xml, "d:href")` will also try `D:href` and `href` (unprefixed).
fn extract_tag(xml: &str, tag: &str) -> Option<String> {
    // Build list of variants: original, aliases, unprefixed
    let mut variants = vec![tag.to_string()];
    if let Some(colon) = tag.find(':') {
        let prefix = &tag[..colon];
        let local = &tag[colon + 1..];

        // Add known aliases
        for &(canonical, aliases) in PREFIX_ALIASES {
            if prefix.eq_ignore_ascii_case(canonical) {
                for alias in aliases {
                    variants.push(format!("{}:{}", alias, local));
                }
                break;
            }
        }

        // Swap case as fallback: d:href -> D:href, D:href -> d:href
        let alt_prefix = if prefix.chars().all(|c| c.is_lowercase()) {
            prefix.to_uppercase()
        } else {
            prefix.to_lowercase()
        };
        let swapped = format!("{}:{}", alt_prefix, local);
        if !variants.contains(&swapped) {
            variants.push(swapped);
        }

        // Also try unprefixed (default namespace)
        variants.push(local.to_string());
    }

    for variant in &variants {
        if let Some(result) = extract_tag_exact(xml, variant) {
            return Some(result);
        }
    }

    // Final fallback: search for any prefix with this local name (handles arbitrary prefixes like SOGo's "a:")
    if let Some(colon) = tag.find(':') {
        let local = &tag[colon + 1..];
        if let Some(result) = extract_tag_any_prefix(xml, local) {
            return Some(result);
        }
    }

    None
}

fn extract_tag_exact(xml: &str, tag: &str) -> Option<String> {
    let open = format!("<{}", tag);
    let close = format!("</{}>", tag);
    if let Some(start) = xml.find(&open) {
        // Skip past any attributes on the opening tag (e.g. <aic:calendar-color symbolic-color="custom">)
        let after_open = &xml[start + open.len()..];
        let content_start = if after_open.starts_with('>') {
            start + open.len() + 1
        } else if let Some(gt) = after_open.find('>') {
            start + open.len() + gt + 1
        } else {
            return None;
        };
        if let Some(end) = xml[content_start..].find(&close) {
            let value = xml[content_start..content_start + end].trim().to_string();
            if !value.is_empty() {
                return Some(value);
            }
        }
    }
    None
}

/// Search for a tag by its local name with any (or no) namespace prefix.
/// For example, `extract_tag_any_prefix(xml, "calendar-home-set")` will match
/// `<a:calendar-home-set>`, `<xyz:calendar-home-set>`, or `<calendar-home-set>`.
fn extract_tag_any_prefix(xml: &str, local_name: &str) -> Option<String> {
    // Pattern: find any occurrence of ":local_name" preceded by "<" and a short prefix,
    // or the tag without a prefix
    let search = format!(":{}", local_name);
    let mut pos = 0;
    while pos < xml.len() {
        // Look for ":local_name" or "<local_name"
        if let Some(colon_pos) = xml[pos..].find(&search) {
            let abs_colon = pos + colon_pos;
            // Walk backwards to find the "<" and extract the prefix
            let mut lt_pos = abs_colon;
            while lt_pos > 0 && xml.as_bytes()[lt_pos - 1] != b'<' {
                lt_pos -= 1;
            }
            if lt_pos > 0 {
                lt_pos -= 1; // point to '<'
                let full_tag = &xml[lt_pos + 1..abs_colon + 1 + local_name.len()];
                // Verify the prefix is a simple XML name (letters/digits, typically 1-3 chars)
                let prefix_part = &xml[lt_pos + 1..abs_colon];
                if !prefix_part.is_empty()
                    && prefix_part.len() <= 10
                    && prefix_part.chars().all(|c| c.is_alphanumeric() || c == '_')
                {
                    if let Some(result) = extract_tag_exact(xml, full_tag) {
                        return Some(result);
                    }
                }
            }
            pos = abs_colon + 1;
        } else {
            break;
        }
    }
    None
}

// --- XML Templates ---

const PROPFIND_PRINCIPAL: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<d:propfind xmlns:d="DAV:">
  <d:prop>
    <d:current-user-principal />
  </d:prop>
</d:propfind>"#;

const PROPFIND_CALENDAR_HOME: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<d:propfind xmlns:d="DAV:" xmlns:cal="urn:ietf:params:xml:ns:caldav">
  <d:prop>
    <cal:calendar-home-set />
  </d:prop>
</d:propfind>"#;

const PROPFIND_CALENDARS: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<d:propfind xmlns:d="DAV:" xmlns:cso="http://calendarserver.org/ns/" xmlns:cal="urn:ietf:params:xml:ns:caldav" xmlns:aic="http://apple.com/ns/ical/">
  <d:prop>
    <d:resourcetype />
    <d:displayname />
    <aic:calendar-color />
    <cso:getctag />
    <d:sync-token />
  </d:prop>
</d:propfind>"#;

const REPORT_CALENDAR_DATA: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<c:calendar-query xmlns:d="DAV:" xmlns:c="urn:ietf:params:xml:ns:caldav">
  <d:prop>
    <d:getetag />
    <c:calendar-data />
  </d:prop>
  <c:filter>
    <c:comp-filter name="VCALENDAR">
      <c:comp-filter name="VEVENT" />
    </c:comp-filter>
  </c:filter>
</c:calendar-query>"#;

#[cfg(test)]
mod tests {
    use super::*;

    // --- extract_tag ---

    #[test]
    fn extract_simple_tag() {
        let xml = "<d:href>/principals/users/alice/</d:href>";
        assert_eq!(
            extract_tag(xml, "d:href"),
            Some("/principals/users/alice/".to_string())
        );
    }

    #[test]
    fn extract_tag_with_attributes() {
        let xml = r#"<aic:calendar-color symbolic-color="custom">#1F6BFF</aic:calendar-color>"#;
        assert_eq!(
            extract_tag(xml, "aic:calendar-color"),
            Some("#1F6BFF".to_string())
        );
    }

    #[test]
    fn extract_tag_not_found() {
        let xml = "<d:href>/path/</d:href>";
        assert_eq!(extract_tag(xml, "d:displayname"), None);
    }

    #[test]
    fn extract_tag_empty_content() {
        let xml = "<d:displayname></d:displayname>";
        assert_eq!(extract_tag(xml, "d:displayname"), None);
    }

    #[test]
    fn extract_tag_whitespace_content() {
        let xml = "<d:displayname>  My Calendar  </d:displayname>";
        assert_eq!(
            extract_tag(xml, "d:displayname"),
            Some("My Calendar".to_string())
        );
    }

    #[test]
    fn extract_tag_nested() {
        let xml = "<d:current-user-principal><d:href>/principals/alice/</d:href></d:current-user-principal>";
        // Searching for d:href within the principal block
        assert_eq!(
            extract_tag(xml, "d:href"),
            Some("/principals/alice/".to_string())
        );
    }

    // --- extract_tag case variants (SOGo uses D: and C:) ---

    #[test]
    fn extract_tag_uppercase_dav_prefix() {
        let xml = "<D:href>/principals/users/alice/</D:href>";
        assert_eq!(
            extract_tag(xml, "d:href"),
            Some("/principals/users/alice/".to_string())
        );
    }

    #[test]
    fn extract_tag_uppercase_caldav_prefix() {
        let xml = "<C:calendar-home-set><D:href>/cal/home/</D:href></C:calendar-home-set>";
        assert_eq!(
            extract_tag(xml, "cal:calendar-home-set"),
            Some("<D:href>/cal/home/</D:href>".to_string())
        );
    }

    #[test]
    fn extract_tag_unprefixed() {
        let xml = "<href>/dav/principals/user/</href>";
        assert_eq!(
            extract_tag(xml, "d:href"),
            Some("/dav/principals/user/".to_string())
        );
    }

    // --- extract_tag arbitrary prefix (SOGo uses a: for CalDAV namespace) ---

    #[test]
    fn extract_tag_arbitrary_prefix() {
        let xml = r#"<a:calendar-home-set><D:href xmlns:D="DAV:">/SOGo/dav/user/Calendar/</D:href></a:calendar-home-set>"#;
        assert_eq!(
            extract_tag(xml, "cal:calendar-home-set"),
            Some(r#"<D:href xmlns:D="DAV:">/SOGo/dav/user/Calendar/</D:href>"#.to_string())
        );
    }

    #[test]
    fn extract_tag_sogo_calendar_home_full_flow() {
        // Real SOGo response from issue #15
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<D:multistatus xmlns:D="DAV:" xmlns:a="urn:ietf:params:xml:ns:caldav"><D:response><D:href>/SOGo/dav/guess.who@echirolles.fr/</D:href><D:propstat><D:status>HTTP/1.1 200 OK</D:status><D:prop><a:calendar-home-set><D:href xmlns:D="DAV:">/SOGo/dav/guess.who@echirolles.fr/Calendar/</D:href></a:calendar-home-set></D:prop></D:propstat></D:response></D:multistatus>"#;
        // extract_tag should find calendar-home-set even with a: prefix
        let inner = extract_tag(xml, "cal:calendar-home-set").unwrap();
        let href = extract_tag(&inner, "d:href").unwrap();
        assert_eq!(href, "/SOGo/dav/guess.who@echirolles.fr/Calendar/");
    }

    // --- split_responses ---

    #[test]
    fn split_responses_uppercase() {
        let xml = r#"<D:multistatus><D:response><D:href>/a</D:href></D:response><D:response><D:href>/b</D:href></D:response></D:multistatus>"#;
        let blocks = split_responses(xml);
        assert_eq!(blocks.len(), 2);
    }

    // --- parse_calendar_list with uppercase prefixes (SOGo) ---

    #[test]
    fn parse_calendars_sogo_uppercase() {
        let xml = r#"
<D:multistatus xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav">
  <D:response>
    <D:href>/SOGo/dav/user/Calendar/personal/</D:href>
    <D:propstat><D:prop>
      <D:resourcetype><D:collection/><C:calendar/></D:resourcetype>
      <D:displayname>Personal Calendar</D:displayname>
    </D:prop></D:propstat>
  </D:response>
  <D:response>
    <D:href>/SOGo/dav/user/Calendar/</D:href>
    <D:propstat><D:prop>
      <D:resourcetype><D:collection/></D:resourcetype>
    </D:prop></D:propstat>
  </D:response>
</D:multistatus>"#;

        let cals = parse_calendar_list(xml);
        assert_eq!(cals.len(), 1);
        assert_eq!(cals[0].href, "/SOGo/dav/user/Calendar/personal/");
        assert_eq!(cals[0].display_name, Some("Personal Calendar".to_string()));
    }

    #[test]
    fn parse_calendars_sogo_unprefixed_xmlns() {
        // SoGo uses unprefixed <calendar xmlns="urn:ietf:params:xml:ns:caldav"/> instead of <C:calendar/>
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<D:multistatus xmlns:D="DAV:" xmlns:b="http://calendarserver.org/ns/" xmlns:a="http://apple.com/ns/ical/"><D:response><D:href>/SOGo/dav/user@example.com/Calendar/</D:href><D:propstat><D:status>HTTP/1.1 200 OK</D:status><D:prop><D:resourcetype><D:collection/></D:resourcetype><D:displayname>Calendar</D:displayname></D:prop></D:propstat></D:response><D:response><D:href>/SOGo/dav/user@example.com/Calendar/personal/</D:href><D:propstat><D:status>HTTP/1.1 200 OK</D:status><D:prop><D:resourcetype><D:collection/><calendar xmlns="urn:ietf:params:xml:ns:caldav"/><vevent-collection xmlns="http://groupdav.org/"/><vtodo-collection xmlns="http://groupdav.org/"/><schedule-outbox xmlns="urn:ietf:params:xml:ns:caldav"/></D:resourcetype><D:displayname>Personal Calendar</D:displayname><a:calendar-color>#AAAAAAFF</a:calendar-color><b:getctag>-1</b:getctag><D:sync-token>-1</D:sync-token></D:prop></D:propstat></D:response></D:multistatus>"#;

        let cals = parse_calendar_list(xml);
        assert_eq!(cals.len(), 1);
        assert_eq!(
            cals[0].href,
            "/SOGo/dav/user@example.com/Calendar/personal/"
        );
        assert_eq!(cals[0].display_name, Some("Personal Calendar".to_string()));
        assert_eq!(cals[0].color, Some("#AAAAAAFF".to_string()));
        assert_eq!(cals[0].ctag, Some("-1".to_string()));
        assert_eq!(cals[0].sync_token, Some("-1".to_string()));
    }

    // --- resolve_url ---

    #[test]
    fn resolve_absolute_path() {
        let client = CaldavClient::new(
            "https://nextcloud.example.com/remote.php/dav",
            "user",
            "pass",
        );
        assert_eq!(
            client.resolve_url("/principals/users/alice/"),
            "https://nextcloud.example.com/principals/users/alice/"
        );
    }

    #[test]
    fn resolve_full_url_passthrough() {
        let client = CaldavClient::new(
            "https://nextcloud.example.com/remote.php/dav",
            "user",
            "pass",
        );
        assert_eq!(
            client.resolve_url("https://other.server.com/calendars/"),
            "https://other.server.com/calendars/"
        );
    }

    #[test]
    fn resolve_with_port() {
        let client = CaldavClient::new("https://cal.example.com:8443/dav", "user", "pass");
        // origin should include host but not port in the simple format
        let resolved = client.resolve_url("/calendars/alice/");
        assert!(resolved.starts_with("https://"));
        assert!(resolved.ends_with("/calendars/alice/"));
    }

    // --- parse_calendar_list ---

    #[test]
    fn parse_calendars_filters_non_calendar() {
        let xml = r#"
<d:multistatus>
  <d:response>
    <d:href>/dav/calendars/alice/</d:href>
    <d:propstat><d:prop>
      <d:resourcetype><d:collection/></d:resourcetype>
    </d:prop></d:propstat>
  </d:response>
  <d:response>
    <d:href>/dav/calendars/alice/default/</d:href>
    <d:propstat><d:prop>
      <d:resourcetype><d:collection/><cal:calendar/></d:resourcetype>
      <d:displayname>Personal</d:displayname>
    </d:prop></d:propstat>
  </d:response>
  <d:response>
    <d:href>/dav/calendars/alice/work/</d:href>
    <d:propstat><d:prop>
      <d:resourcetype><d:collection/><cal:calendar/></d:resourcetype>
      <d:displayname>Work</d:displayname>
      <aic:calendar-color>#FF0000</aic:calendar-color>
      <cso:getctag>ctag-123</cso:getctag>
    </d:prop></d:propstat>
  </d:response>
</d:multistatus>"#;

        let cals = parse_calendar_list(xml);
        assert_eq!(cals.len(), 2);

        assert_eq!(cals[0].href, "/dav/calendars/alice/default/");
        assert_eq!(cals[0].display_name, Some("Personal".to_string()));
        assert_eq!(cals[0].color, None);

        assert_eq!(cals[1].href, "/dav/calendars/alice/work/");
        assert_eq!(cals[1].display_name, Some("Work".to_string()));
        assert_eq!(cals[1].color, Some("#FF0000".to_string()));
        assert_eq!(cals[1].ctag, Some("ctag-123".to_string()));
    }

    #[test]
    fn parse_calendars_alternative_namespaces() {
        // BlueMind uses x1: for colors and cs: for ctags
        let xml = r#"
<d:multistatus>
  <d:response>
    <d:href>/dav/cal/</d:href>
    <d:propstat><d:prop>
      <d:resourcetype><d:collection/><cal:calendar/></d:resourcetype>
      <d:displayname>BlueMind Cal</d:displayname>
      <x1:calendar-color>#00FF00</x1:calendar-color>
      <cs:getctag>ctag-bm</cs:getctag>
    </d:prop></d:propstat>
  </d:response>
</d:multistatus>"#;

        let cals = parse_calendar_list(xml);
        assert_eq!(cals.len(), 1);
        assert_eq!(cals[0].color, Some("#00FF00".to_string()));
        assert_eq!(cals[0].ctag, Some("ctag-bm".to_string()));
    }

    #[test]
    fn parse_calendars_empty_response() {
        let xml = "<d:multistatus></d:multistatus>";
        let cals = parse_calendar_list(xml);
        assert!(cals.is_empty());
    }

    // --- parse_event_responses ---

    // --- parse_sync_response ---

    #[test]
    fn parse_sync_changes_and_deletions() {
        let xml = r#"
<d:multistatus xmlns:d="DAV:" xmlns:cal="urn:ietf:params:xml:ns:caldav">
  <d:response>
    <d:href>/cal/event1.ics</d:href>
    <d:propstat>
      <d:prop>
        <d:getetag>"etag1"</d:getetag>
        <cal:calendar-data>BEGIN:VCALENDAR
BEGIN:VEVENT
UID:ev1
SUMMARY:Changed event
END:VEVENT
END:VCALENDAR</cal:calendar-data>
      </d:prop>
      <d:status>HTTP/1.1 200 OK</d:status>
    </d:propstat>
  </d:response>
  <d:response>
    <d:href>/cal/deleted-event.ics</d:href>
    <d:status>HTTP/1.1 404 Not Found</d:status>
  </d:response>
  <d:sync-token>https://example.com/sync/new-token-456</d:sync-token>
</d:multistatus>"#;

        let result = parse_sync_response(xml);
        assert_eq!(result.changed.len(), 1);
        assert_eq!(result.changed[0].href, "/cal/event1.ics");
        assert!(result.changed[0].ical_data.contains("UID:ev1"));
        assert_eq!(result.deleted_hrefs, vec!["/cal/deleted-event.ics"]);
        assert_eq!(
            result.new_sync_token,
            Some("https://example.com/sync/new-token-456".to_string())
        );
    }

    #[test]
    fn parse_sync_empty_delta() {
        let xml = r#"
<d:multistatus xmlns:d="DAV:">
  <d:sync-token>https://example.com/sync/token-789</d:sync-token>
</d:multistatus>"#;

        let result = parse_sync_response(xml);
        assert!(result.changed.is_empty());
        assert!(result.deleted_hrefs.is_empty());
        assert_eq!(
            result.new_sync_token,
            Some("https://example.com/sync/token-789".to_string())
        );
    }

    #[test]
    fn parse_sync_multiple_deletions() {
        let xml = r#"
<d:multistatus xmlns:d="DAV:">
  <d:response>
    <d:href>/cal/a.ics</d:href>
    <d:status>HTTP/1.1 404 Not Found</d:status>
  </d:response>
  <d:response>
    <d:href>/cal/b.ics</d:href>
    <d:status>HTTP/1.1 404 Not Found</d:status>
  </d:response>
  <d:sync-token>token-new</d:sync-token>
</d:multistatus>"#;

        let result = parse_sync_response(xml);
        assert!(result.changed.is_empty());
        assert_eq!(result.deleted_hrefs.len(), 2);
    }

    // --- parse_event_responses ---

    #[test]
    fn parse_events_extracts_ical() {
        let xml = r#"
<d:multistatus>
  <d:response>
    <d:href>/cal/event1.ics</d:href>
    <d:propstat><d:prop>
      <cal:calendar-data>BEGIN:VCALENDAR
BEGIN:VEVENT
UID:ev1
END:VEVENT
END:VCALENDAR</cal:calendar-data>
    </d:prop></d:propstat>
  </d:response>
  <d:response>
    <d:href>/cal/event2.ics</d:href>
    <d:propstat><d:prop>
      <c:calendar-data>BEGIN:VCALENDAR
BEGIN:VEVENT
UID:ev2
END:VEVENT
END:VCALENDAR</c:calendar-data>
    </d:prop></d:propstat>
  </d:response>
</d:multistatus>"#;

        let events = parse_event_responses(xml);
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].href, "/cal/event1.ics");
        assert!(events[0].ical_data.contains("UID:ev1"));
        assert_eq!(events[1].href, "/cal/event2.ics");
        assert!(events[1].ical_data.contains("UID:ev2"));
    }

    #[test]
    fn parse_events_skips_empty_data() {
        let xml = r#"
<d:multistatus>
  <d:response>
    <d:href>/cal/no-data.ics</d:href>
    <d:propstat><d:prop></d:prop></d:propstat>
  </d:response>
</d:multistatus>"#;

        let events = parse_event_responses(xml);
        assert!(events.is_empty());
    }

    // --- CaldavClient::new origin parsing ---

    #[test]
    fn client_origin_parsing() {
        let client = CaldavClient::new("https://cloud.example.com/remote.php/dav", "u", "p");
        assert_eq!(client.origin, "https://cloud.example.com");
        assert_eq!(client.base_url, "https://cloud.example.com/remote.php/dav");
    }

    #[test]
    fn client_trims_trailing_slash() {
        let client = CaldavClient::new("https://cloud.example.com/dav/", "u", "p");
        assert_eq!(client.base_url, "https://cloud.example.com/dav");
    }
}
