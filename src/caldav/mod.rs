use anyhow::{bail, Context, Result};
use reqwest::{Client, RequestBuilder};
use std::net::IpAddr;
use std::time::Duration;

mod multistatus;

use multistatus::{
    calendar_home_set, current_user_principal, parse_calendar_list, parse_event_responses,
    parse_sync_response, propfind_resource_exists,
};

/// Host portion of Google's CalDAV API. Used to short-circuit the discovery
/// flow, since Google's PROPFIND responses don't follow RFC 4791 closely
/// enough for the standard discovery to work, and the URL pattern is fixed.
const GOOGLE_CALDAV_HOST: &str = "apidata.googleusercontent.com";

pub enum CaldavAuth {
    Basic { username: String, password: String },
    Bearer { access_token: String },
}

pub struct CaldavClient {
    client: Client,
    base_url: String,
    origin: String,
    auth: CaldavAuth,
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

/// Hostnames that are permitted to resolve to private/reserved IPs.
///
/// Resolved from the `CALRS_ALLOW_PRIVATE_HOSTS` env var (which takes
/// precedence) or, as a fallback, the DB-stored value loaded into the
/// process-global cache. See [`crate::settings`] for the precedence rules.
/// Comma-separated, whitespace-trimmed, case-insensitive; empty entries ignored.
pub fn private_host_allowlist() -> Vec<String> {
    crate::settings::private_host_allowlist()
}

/// Whether `host` is in the configured private-host allowlist (case-insensitive).
fn is_host_allowlisted(host: &str, allowlist: &[String]) -> bool {
    let host = host.to_ascii_lowercase();
    allowlist.contains(&host)
}

/// Validate a CalDAV URL: must be http(s), must not resolve to a private IP.
///
/// Self-hosted deployments (e.g. calrs and Radicale on the same docker network)
/// can opt specific hostnames out of the private-IP check via the
/// `CALRS_ALLOW_PRIVATE_HOSTS` env var or the DB-backed setting behind it
/// (`calrs config general --allow-private-hosts`, or the admin panel); see
/// [`crate::settings`] for which one wins. The check stays active for every
/// other host.
///
/// Limitation: this resolves the hostname once at validation time. A DNS
/// rebinding attacker who returns a public IP for the initial lookup and a
/// private IP for the actual HTTP fetch will bypass the guard. Closing this
/// purely in-process would require inspecting the peer address of the
/// connected socket on every CalDAV request. The documented mitigation is an
/// egress firewall (see `docs/src/security.md`).
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

    // Hosts the operator has explicitly opted out of the private-IP check.
    let allowlist = private_host_allowlist();
    if is_host_allowlisted(host, &allowlist) {
        return Ok(());
    }

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
        Self::build(
            base_url,
            CaldavAuth::Basic {
                username: username.to_string(),
                password: password.to_string(),
            },
        )
    }

    pub fn with_bearer(base_url: &str, access_token: &str) -> Self {
        Self::build(
            base_url,
            CaldavAuth::Bearer {
                access_token: access_token.to_string(),
            },
        )
    }

    fn build(base_url: &str, auth: CaldavAuth) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .unwrap_or_else(|_| Client::new());

        let base = base_url.trim_end_matches('/').to_string();
        let origin = reqwest::Url::parse(&base)
            .map(|u: reqwest::Url| match u.port() {
                Some(port) => format!("{}://{}:{}", u.scheme(), u.host_str().unwrap_or(""), port),
                None => format!("{}://{}", u.scheme(), u.host_str().unwrap_or("")),
            })
            .unwrap_or_else(|_| base.clone());

        Self {
            client,
            base_url: base,
            origin,
            auth,
        }
    }

    fn apply_auth(&self, builder: RequestBuilder) -> RequestBuilder {
        match &self.auth {
            CaldavAuth::Basic { username, password } => {
                builder.basic_auth(username, Some(password))
            }
            CaldavAuth::Bearer { access_token } => builder.bearer_auth(access_token),
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
        let req = self
            .client
            .request(reqwest::Method::from_bytes(b"PROPFIND")?, url);
        let resp = self
            .apply_auth(req)
            .header("Content-Type", "application/xml; charset=utf-8")
            .header("Depth", depth)
            .body(body.to_string())
            .send()
            .await?;

        let status = resp.status();
        tracing::debug!(url = %url, status = %status, "PROPFIND response received");

        if !status.is_success() && status.as_u16() != 207 {
            // Surface the response body. Servers like Google embed the actual reason
            // (insufficient scope, API not enabled, etc.) in the body, not the status line.
            let body = resp.text().await.unwrap_or_default();
            let snippet: String = body.chars().take(500).collect();
            bail!("PROPFIND {} returned {}: {}", url, status, snippet);
        }

        Ok(resp.text().await?)
    }

    /// Check if the server supports CalDAV.
    ///
    /// Two-step probe: a fast `OPTIONS` request looking for `calendar-access`
    /// in the `DAV` response header, then if that doesn't match, a PROPFIND
    /// for the current-user principal. Some servers (notably SOGo on its
    /// `/SOGo/dav/` root) don't advertise `calendar-access` in OPTIONS even
    /// though they are CalDAV-capable, so the OPTIONS-only check produced
    /// false negatives. A successful principal PROPFIND is unambiguous proof
    /// that the server speaks CalDAV.
    pub async fn check_connection(&self) -> Result<bool> {
        let req = self
            .client
            .request(reqwest::Method::OPTIONS, &self.base_url);
        let resp = self.apply_auth(req).send().await?;

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

        if dav_header.contains("calendar-access")
            || dav_header.contains("nc-calendar-search")
            || dav_header.contains("nc-enable-birthday-calendar")
        {
            return Ok(true);
        }

        // OPTIONS came back clean but didn't advertise CalDAV. Try the
        // PROPFIND probe; if it returns a principal href, the server speaks
        // CalDAV regardless of what the OPTIONS header said.
        Ok(self.discover_principal().await.is_ok())
    }

    /// Discover the current-user-principal URL via PROPFIND
    pub async fn discover_principal(&self) -> Result<String> {
        // Google CalDAV doesn't return <current-user-principal> in a way the
        // standard discovery flow can use, but the URL we configure for Google
        // OAuth2 sources is already the per-user principal endpoint, so skip the
        // round-trip and return it directly.
        if self.base_url.contains(GOOGLE_CALDAV_HOST) {
            tracing::debug!(principal = %self.base_url, "using Google CalDAV principal URL directly");
            return Ok(self.base_url.clone());
        }

        let text = self
            .propfind(&self.base_url, "0", PROPFIND_PRINCIPAL)
            .await?;

        tracing::debug!(
            response_len = text.len(),
            "PROPFIND principal response received"
        );

        let principal = current_user_principal(&text)
            .inspect_err(
                |_| tracing::debug!(response_body = %text, "unreadable principal response"),
            )
            .context("Could not read the principal PROPFIND response")?;
        if let Some(href) = principal {
            tracing::debug!(principal = %href, "discovered principal URL");
            return Ok(href);
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

        let home = calendar_home_set(&text)
            .inspect_err(
                |_| tracing::debug!(response_body = %text, "unreadable calendar-home-set response"),
            )
            .context("Could not read the calendar-home-set PROPFIND response")?;
        if let Some(href) = home {
            tracing::debug!(calendar_home = %href, "discovered calendar-home-set");
            return Ok(href);
        }

        tracing::debug!(response_body = %text, "failed to parse calendar-home-set from response");
        bail!("Could not discover calendar-home-set from response")
    }

    /// List calendars under a calendar-home-set URL (filters to actual calendars only)
    pub async fn list_calendars(&self, home_url: &str) -> Result<Vec<CalendarInfo>> {
        let url = self.resolve_url(home_url);
        let text = self.propfind(&url, "1", PROPFIND_CALENDARS).await?;
        let calendars = parse_calendar_list(&text)
            .inspect_err(
                |_| tracing::debug!(response_body = %text, "unreadable calendar list response"),
            )
            .with_context(|| {
                format!("Could not read the calendar list PROPFIND response from {url}")
            })?;
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

        let req = self.client.put(&url);
        let resp = self
            .apply_auth(req)
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

    /// Check whether an event resource still exists on the server.
    ///
    /// Returns `Ok(true)` if the server confirms the resource is present (2xx),
    /// `Ok(false)` if the server confirms it is gone (404/410). Any other
    /// response — network error, timeout, 5xx, 401/403 — is returned as
    /// `Err`, and callers must treat that as "can't tell, do not act."
    ///
    /// Uses HEAD by default; if the server refuses HEAD on resources
    /// (405 Method Not Allowed, BlueMind's 501 Not Implemented, or iCloud's
    /// 400 Bad Request) falls back to PROPFIND depth:0 with an empty prop
    /// body, which is universally supported.
    pub async fn event_exists(&self, calendar_href: &str, uid: &str) -> Result<bool> {
        let href = format!("{}/{}.ics", calendar_href.trim_end_matches('/'), uid);
        let url = self.resolve_url(&href);

        let resp = self
            .apply_auth(self.client.head(&url))
            .timeout(Duration::from_secs(10))
            .send()
            .await?;
        let status = resp.status();

        if matches!(status.as_u16(), 400 | 405 | 501) {
            // Fall back to PROPFIND depth:0 with an empty prop request.
            // 405 is the spec'd "method not allowed"; BlueMind answers HEAD
            // with 501 Not Implemented instead, and iCloud with 400 Bad
            // Request on an event it has just stored.
            let body = r#"<?xml version="1.0" encoding="utf-8"?>
<d:propfind xmlns:d="DAV:"><d:prop><d:getetag/></d:prop></d:propfind>"#;
            let resp = self
                .apply_auth(
                    self.client
                        .request(reqwest::Method::from_bytes(b"PROPFIND")?, &url),
                )
                .header("Content-Type", "application/xml; charset=utf-8")
                .header("Depth", "0")
                .timeout(Duration::from_secs(10))
                .body(body)
                .send()
                .await?;
            let s = resp.status();
            if s.as_u16() == 207 {
                // The outer 207 only says the server answered. Whether the
                // event is there is the resource's own status inside the body.
                let xml = resp.text().await?;
                return match propfind_resource_exists(&xml) {
                    Some(exists) => Ok(exists),
                    None => bail!(
                        "PROPFIND {} returned a multistatus without a usable status",
                        url
                    ),
                };
            }
            if s.is_success() {
                return Ok(true);
            }
            if s.as_u16() == 404 || s.as_u16() == 410 {
                return Ok(false);
            }
            bail!("PROPFIND {} returned {}", url, s);
        }

        if status.is_success() {
            return Ok(true);
        }
        if status.as_u16() == 404 || status.as_u16() == 410 {
            return Ok(false);
        }
        bail!("HEAD {} returned {}", url, status)
    }

    /// DELETE an event from a calendar
    pub async fn delete_event(&self, calendar_href: &str, uid: &str) -> Result<()> {
        let href = format!("{}/{}.ics", calendar_href.trim_end_matches('/'), uid);
        let url = self.resolve_url(&href);

        let req = self.client.delete(&url);
        let resp = self.apply_auth(req).send().await?;

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
        let req = self
            .client
            .request(reqwest::Method::from_bytes(b"REPORT")?, &url);
        let resp = self
            .apply_auth(req)
            .header("Content-Type", "application/xml; charset=utf-8")
            .header("Depth", "1")
            .timeout(Duration::from_secs(60))
            .body(REPORT_CALENDAR_DATA)
            .send()
            .await?;

        let text = resp.text().await?;
        parse_event_responses(&text)
            .with_context(|| format!("Could not read the calendar REPORT response from {url}"))
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

        // Tokens are stored decoded (the parser decodes entities), so escape
        // them again.
        let token_value = xml_escape(sync_token.unwrap_or(""));
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

        let req = self
            .client
            .request(reqwest::Method::from_bytes(b"REPORT")?, &url);
        let resp = self
            .apply_auth(req)
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
        parse_sync_response(&text)
            .with_context(|| format!("Could not read the sync-collection response from {url}"))
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

        let req = self
            .client
            .request(reqwest::Method::from_bytes(b"REPORT")?, &url);
        let resp = self
            .apply_auth(req)
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

        parse_event_responses(&text)
            .with_context(|| format!("Could not read the calendar REPORT response from {url}"))
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

/// Escape text for use as XML element content.
fn xml_escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
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
        assert_eq!(
            client.resolve_url("/calendars/alice/"),
            "https://cal.example.com:8443/calendars/alice/"
        );
    }

    // Regression test for https://github.com/olivierlambert/calrs/issues/42 —
    // Nextcloud on a non-standard port returns site-absolute principal hrefs;
    // resolve_url must preserve the port so PROPFIND hits the right server.
    #[test]
    fn resolve_with_port_nextcloud_principal() {
        let client = CaldavClient::new("https://my-nextcloud:8080/remote.php/dav/", "user", "pass");
        assert_eq!(
            client.resolve_url("/remote.php/dav/principals/users/alice/"),
            "https://my-nextcloud:8080/remote.php/dav/principals/users/alice/"
        );
    }

    #[test]
    fn resolve_without_port_https_default() {
        let client = CaldavClient::new("https://cal.example.com/dav", "user", "pass");
        assert_eq!(
            client.resolve_url("/calendars/alice/"),
            "https://cal.example.com/calendars/alice/"
        );
    }

    #[test]
    fn resolve_with_explicit_default_port_stripped() {
        // Port 443 on https is the default; url crate strips it, which is fine —
        // it reaches the same server.
        let client = CaldavClient::new("https://cal.example.com:443/dav", "user", "pass");
        assert_eq!(
            client.resolve_url("/calendars/alice/"),
            "https://cal.example.com/calendars/alice/"
        );
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

    // Google CalDAV's PROPFIND on the base URL doesn't reliably expose
    // <current-user-principal>, but the URL we configure for Google OAuth2
    // sources is already the per-user principal endpoint, so discover_principal
    // short-circuits and returns it directly. discover_calendar_home then does
    // a real PROPFIND so any error from Google surfaces with its actual body.
    #[tokio::test]
    async fn google_discover_principal_short_circuits() {
        let url = "https://apidata.googleusercontent.com/caldav/v2/alice%40gmail.com/user";
        let client = CaldavClient::with_bearer(url, "fake-token");

        let principal = client.discover_principal().await.unwrap();
        assert_eq!(principal, url);
    }

    // --- private-host allowlist (SSRF opt-out) ---

    #[test]
    fn allowlist_matches_case_insensitively() {
        let list = vec!["radicale".to_string(), "nextcloud.local".to_string()];
        assert!(is_host_allowlisted("radicale", &list));
        assert!(is_host_allowlisted("RADICALE", &list));
        assert!(is_host_allowlisted("Nextcloud.Local", &list));
        assert!(!is_host_allowlisted("evil.example.com", &list));
        assert!(!is_host_allowlisted("radicale.example.com", &list));
    }

    #[test]
    fn empty_allowlist_matches_nothing() {
        assert!(!is_host_allowlisted("radicale", &[]));
    }

    /// All env-var assertions live in one test: Rust runs tests in parallel
    /// within a process, so a second test touching the same env var would race.
    /// The lock is crate-wide rather than module-local because `commands::config`
    /// reads this same variable.
    #[test]
    fn allowlist_env_var_opts_host_out_of_private_ip_check() {
        // No async runtime in a plain `#[test]`, so blocking on the lock is safe.
        let _lock = crate::test_support::ENV_LOCK.blocking_lock();
        let _guard = crate::test_support::EnvGuard::new();

        // Loopback literal is rejected by default.
        std::env::remove_var("CALRS_ALLOW_PRIVATE_HOSTS");
        assert!(private_host_allowlist().is_empty());
        assert!(validate_caldav_url("http://127.0.0.1:5232").is_err());

        // ...but allowed once its host is on the allowlist.
        std::env::set_var("CALRS_ALLOW_PRIVATE_HOSTS", " 127.0.0.1 , radicale ");
        assert_eq!(private_host_allowlist(), vec!["127.0.0.1", "radicale"]);
        assert!(validate_caldav_url("http://127.0.0.1:5232").is_ok());

        // Non-http schemes are still rejected even when host is allowlisted.
        assert!(validate_caldav_url("ftp://127.0.0.1:5232").is_err());

        // A host not on the allowlist still gets the private-IP check.
        assert!(validate_caldav_url("http://10.0.0.1").is_err());

        std::env::remove_var("CALRS_ALLOW_PRIVATE_HOSTS");
    }
}
