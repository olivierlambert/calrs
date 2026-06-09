use anyhow::{bail, Result};
use reqwest::{Client, RequestBuilder};
use std::net::IpAddr;
use std::time::Duration;

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

/// Parse the `CALRS_ALLOW_PRIVATE_HOSTS` env var into a list of hostnames that
/// are permitted to resolve to private/reserved IPs. Comma-separated,
/// whitespace-trimmed, case-insensitive. Empty entries are ignored.
pub fn private_host_allowlist() -> Vec<String> {
    std::env::var("CALRS_ALLOW_PRIVATE_HOSTS")
        .unwrap_or_default()
        .split(',')
        .map(|h| h.trim().to_ascii_lowercase())
        .filter(|h| !h.is_empty())
        .collect()
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
/// `CALRS_ALLOW_PRIVATE_HOSTS` env var (comma-separated hostnames). The check
/// stays active for every other host.
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
    /// Uses HEAD by default; if the server returns 405 (Method Not Allowed,
    /// some CalDAV servers refuse HEAD on resources) falls back to PROPFIND
    /// depth:0 with an empty prop body, which is universally supported.
    pub async fn event_exists(&self, calendar_href: &str, uid: &str) -> Result<bool> {
        let href = format!("{}/{}.ics", calendar_href.trim_end_matches('/'), uid);
        let url = self.resolve_url(&href);

        let resp = self
            .apply_auth(self.client.head(&url))
            .timeout(Duration::from_secs(10))
            .send()
            .await?;
        let status = resp.status();

        if status.as_u16() == 405 {
            // Fall back to PROPFIND depth:0 with an empty prop request.
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
            if s.is_success() || s.as_u16() == 207 {
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

/// Detect a `calendar` resourcetype element with any (or no) namespace prefix.
///
/// Matches `<cal:calendar/>`, `<C:calendar/>`, `<C:calendar />` (Radicale emits a
/// space before the self-closing slash), unprefixed `<calendar xmlns="…">`, and
/// the open form `<C:calendar>`. It must NOT match longer local names that merely
/// start with `calendar` (`calendar-color`, `calendar-data`,
/// `supported-calendar-component-set`) nor `addressbook` collections, so we
/// require the element's local name to end exactly after `calendar`.
fn has_calendar_resourcetype(block: &str) -> bool {
    const NAME: &str = "calendar";
    let mut from = 0;
    while let Some(rel) = block[from..].find(NAME) {
        let start = from + rel;
        let end = start + NAME.len();
        from = end;

        // The character after the local name must terminate it: a self-closing
        // slash, the tag's closing '>', or whitespace before attributes. A '-'
        // (or anything else) means a longer name like "calendar-color".
        let terminates = matches!(block[end..].chars().next(), Some('/') | Some('>'))
            || block[end..].chars().next().is_some_and(char::is_whitespace);
        if !terminates {
            continue;
        }

        // The text before "calendar" must open an element: '<' directly
        // (unprefixed) or "<prefix:" (namespaced).
        let before = &block[..start];
        if before.ends_with('<') {
            return true;
        }
        if let Some(prefix) = before.strip_suffix(':') {
            let trimmed = prefix.trim_end_matches(|c: char| {
                c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.')
            });
            if trimmed.len() < prefix.len() && trimmed.ends_with('<') {
                return true;
            }
        }
    }
    false
}

fn parse_calendar_list(xml: &str) -> Vec<CalendarInfo> {
    let mut calendars = Vec::new();
    for response_block in split_responses(xml) {
        // Only include actual calendar collections (resourcetype contains a
        // `calendar` element), skipping plain collections and addressbooks.
        if !has_calendar_resourcetype(response_block) {
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

        // A `<d:response>` element carries the resource's status in one of two ways
        // (RFC 4918 §9.1, mutually exclusive):
        //   1. A direct child `<d:status>` element  — the resource-level status
        //      (e.g. `404 Not Found` for a deletion in a sync-collection delta).
        //   2. One or more `<d:propstat>` elements, each with its OWN `<d:status>`
        //      describing which requested properties were found vs missing on the
        //      (still-existing) resource.
        //
        // The status check below must only look at case (1). Looking at the first
        // `<d:status>` tag anywhere in the response block misreads a property-level
        // 404 — for example, a propstat reporting that `<d:getetag>` was not
        // returned — as if the whole resource had been deleted. That mis-classification
        // wrongly cancelled live customer bookings in production on 2026-05-14.
        // Strip `<d:propstat>` subtrees before the status lookup so only the
        // resource-level status is considered.
        let stripped = strip_propstat_blocks(response_block);
        if let Some(status) = extract_tag(&stripped, "d:status") {
            if status.contains("404") || status.contains("410") {
                deleted_hrefs.push(href);
                continue;
            }
        }

        // Otherwise it's an addition/modification — extract calendar data from
        // the original (un-stripped) block, since calendar-data lives inside a
        // propstat by definition.
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

/// Remove every `<propstat>…</propstat>` subtree from a WebDAV response fragment.
///
/// Used by `parse_sync_response` to isolate the resource-level `<d:status>` from
/// any property-level statuses nested inside propstat elements. Handles the three
/// namespace-prefix patterns seen in the wild for the DAV: namespace: `d:`, `D:`,
/// and unprefixed. Tolerates malformed XML (leaves any unmatched open tag in
/// place) rather than failing.
fn strip_propstat_blocks(s: &str) -> String {
    let mut result = s.to_string();
    for (open_prefix, close_tag) in &[
        ("<d:propstat", "</d:propstat>"),
        ("<D:propstat", "</D:propstat>"),
        ("<propstat", "</propstat>"),
    ] {
        let mut search_from = 0;
        while let Some(rel_start) = result[search_from..].find(open_prefix) {
            let abs_start = search_from + rel_start;
            // Confirm we matched the real tag and not a longer name like
            // `<propstats>` — the next byte must be `>` or whitespace.
            let after = result
                .as_bytes()
                .get(abs_start + open_prefix.len())
                .copied();
            if !matches!(
                after,
                Some(b'>') | Some(b' ') | Some(b'\t') | Some(b'\n') | Some(b'\r')
            ) {
                // Not a propstat element. Advance past this prefix and continue.
                search_from = abs_start + open_prefix.len();
                continue;
            }
            let Some(rel_close) = result[abs_start..].find(close_tag) else {
                // Malformed: open tag without close. Stop processing this prefix.
                break;
            };
            let end = abs_start + rel_close + close_tag.len();
            result.replace_range(abs_start..end, "");
            // Continue searching from where the block used to start.
            search_from = abs_start;
        }
    }
    result
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

    #[test]
    fn parse_calendars_radicale() {
        // Radicale 3.7.3 (issue #132): default DAV namespace (unprefixed href/
        // displayname/sync-token), C: for caldav, a SPACE before the self-closing
        // slash (`<C:calendar />`), CR: addressbooks that must be skipped, and the
        // principal collection itself (which must not be treated as a calendar).
        let xml = r#"<?xml version='1.0' encoding='utf-8'?>
<multistatus xmlns="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav" xmlns:CR="urn:ietf:params:xml:ns:carddav" xmlns:CS="http://calendarserver.org/ns/" xmlns:ICAL="http://apple.com/ns/ical/"><response><href>/greg/</href><propstat><prop><resourcetype><principal /><collection /></resourcetype></prop><status>HTTP/1.1 200 OK</status></propstat><propstat><prop><displayname /><ICAL:calendar-color /><CS:getctag /><sync-token /></prop><status>HTTP/1.1 404 Not Found</status></propstat></response><response><href>/greg/0ff70afa/</href><propstat><prop><resourcetype><C:calendar /><collection /></resourcetype><displayname>Calendrier Pro</displayname><ICAL:calendar-color>#f79f78ff</ICAL:calendar-color><CS:getctag>"ctag-pro"</CS:getctag><sync-token>http://radicale.org/ns/sync/pro</sync-token></prop><status>HTTP/1.1 200 OK</status></propstat></response><response><href>/greg/7d2395d4/</href><propstat><prop><resourcetype><CR:addressbook /><collection /></resourcetype><displayname>Contacts Pros</displayname><CS:getctag>"ctag-contacts"</CS:getctag><sync-token>http://radicale.org/ns/sync/contacts</sync-token></prop><status>HTTP/1.1 200 OK</status></propstat><propstat><prop><ICAL:calendar-color /></prop><status>HTTP/1.1 404 Not Found</status></propstat></response><response><href>/greg/a56013c9/</href><propstat><prop><resourcetype><C:calendar /><collection /></resourcetype><displayname>Calendrier Perso</displayname><ICAL:calendar-color>#74bda7ff</ICAL:calendar-color><CS:getctag>"ctag-perso"</CS:getctag><sync-token>http://radicale.org/ns/sync/perso</sync-token></prop><status>HTTP/1.1 200 OK</status></propstat></response></multistatus>"#;

        let cals = parse_calendar_list(xml);
        assert_eq!(
            cals.len(),
            2,
            "should detect the two C:calendar collections, skipping principal + addressbook"
        );

        assert_eq!(cals[0].href, "/greg/0ff70afa/");
        assert_eq!(cals[0].display_name, Some("Calendrier Pro".to_string()));
        assert_eq!(cals[0].color, Some("#f79f78ff".to_string()));
        assert_eq!(cals[0].ctag, Some("\"ctag-pro\"".to_string()));
        assert_eq!(
            cals[0].sync_token,
            Some("http://radicale.org/ns/sync/pro".to_string())
        );

        assert_eq!(cals[1].href, "/greg/a56013c9/");
        assert_eq!(cals[1].display_name, Some("Calendrier Perso".to_string()));
        assert_eq!(cals[1].color, Some("#74bda7ff".to_string()));
    }

    #[test]
    fn has_calendar_resourcetype_distinguishes_lookalikes() {
        // Real calendar elements, various prefixes / spacing.
        assert!(has_calendar_resourcetype("<C:calendar />"));
        assert!(has_calendar_resourcetype("<C:calendar/>"));
        assert!(has_calendar_resourcetype("<cal:calendar/>"));
        assert!(has_calendar_resourcetype(
            "<calendar xmlns=\"urn:ietf:params:xml:ns:caldav\"/>"
        ));
        assert!(has_calendar_resourcetype("<C:calendar></C:calendar>"));
        // Lookalikes that must NOT count as a calendar resourcetype.
        assert!(!has_calendar_resourcetype("<ICAL:calendar-color />"));
        assert!(!has_calendar_resourcetype(
            "<C:calendar-data>BEGIN:VCALENDAR</C:calendar-data>"
        ));
        assert!(!has_calendar_resourcetype(
            "<C:supported-calendar-component-set />"
        ));
        assert!(!has_calendar_resourcetype(
            "<CR:addressbook /><collection />"
        ));
        assert!(!has_calendar_resourcetype("<d:collection/>"));
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

    /// Regression test for issue #107 / 2026-05-14 production incident.
    ///
    /// Per RFC 4918 §9.1, a `<d:response>` for an existing resource may carry
    /// one or more `<d:propstat>` blocks, each with its own `<d:status>` that
    /// describes which requested properties succeeded vs which were not found.
    /// A 404 inside a propstat is a property-level "this prop doesn't exist on
    /// this resource", NOT a resource-level deletion. Before the fix, the
    /// parser took the first `<d:status>` it found anywhere in the response
    /// block and misread a property-level 404 as a deleted href — which is
    /// what cancelled customer bookings in production.
    ///
    /// This XML reproduces the failure shape: a response with a 404 propstat
    /// (a property that doesn't exist on the resource) ordered BEFORE a 200
    /// propstat that carries the actual calendar-data. The parser must
    /// classify this as a `changed` event, not as a deletion.
    #[test]
    fn parse_sync_ignores_property_level_404_in_propstat() {
        let xml = r#"
<d:multistatus xmlns:d="DAV:" xmlns:cal="urn:ietf:params:xml:ns:caldav">
  <d:response>
    <d:href>/cal/live-event.ics</d:href>
    <d:propstat>
      <d:prop><d:getetag/></d:prop>
      <d:status>HTTP/1.1 404 Not Found</d:status>
    </d:propstat>
    <d:propstat>
      <d:prop>
        <d:getetag>"abc-123"</d:getetag>
        <cal:calendar-data>BEGIN:VCALENDAR
BEGIN:VEVENT
UID:still-here
END:VEVENT
END:VCALENDAR</cal:calendar-data>
      </d:prop>
      <d:status>HTTP/1.1 200 OK</d:status>
    </d:propstat>
  </d:response>
  <d:sync-token>https://example.com/sync/t1</d:sync-token>
</d:multistatus>"#;

        let result = parse_sync_response(xml);
        assert!(
            result.deleted_hrefs.is_empty(),
            "a 404 propstat must NOT be classified as a resource-level deletion; \
             got deleted_hrefs={:?}",
            result.deleted_hrefs
        );
        assert_eq!(result.changed.len(), 1);
        assert_eq!(result.changed[0].href, "/cal/live-event.ics");
        assert!(result.changed[0].ical_data.contains("UID:still-here"));
    }

    /// Companion to the above: confirm a genuine resource-level deletion
    /// (a single `<d:status>` directly under `<d:response>`, no propstat)
    /// is still correctly classified as a deleted href. Strip-only-propstat
    /// must not over-correct and ignore real deletions.
    #[test]
    fn parse_sync_still_detects_real_resource_deletion() {
        let xml = r#"
<d:multistatus xmlns:d="DAV:">
  <d:response>
    <d:href>/cal/actually-gone.ics</d:href>
    <d:status>HTTP/1.1 404 Not Found</d:status>
  </d:response>
  <d:sync-token>https://example.com/sync/t2</d:sync-token>
</d:multistatus>"#;

        let result = parse_sync_response(xml);
        assert_eq!(result.deleted_hrefs, vec!["/cal/actually-gone.ics"]);
        assert!(result.changed.is_empty());
    }

    /// 410 Gone (per RFC 7231) means the resource was deliberately removed.
    /// Some CalDAV servers return 410 instead of 404 for deletions. The parser
    /// must treat both as resource-level deletions.
    #[test]
    fn parse_sync_treats_410_as_deletion() {
        let xml = r#"
<d:multistatus xmlns:d="DAV:">
  <d:response>
    <d:href>/cal/gone.ics</d:href>
    <d:status>HTTP/1.1 410 Gone</d:status>
  </d:response>
</d:multistatus>"#;

        let result = parse_sync_response(xml);
        assert_eq!(result.deleted_hrefs, vec!["/cal/gone.ics"]);
    }

    /// Mixed response: a true deletion (no propstat, resource-level 404),
    /// a "changed with property-level 404 propstat" entry, and a vanilla
    /// 200 addition should all coexist in one multistatus and parse correctly.
    #[test]
    fn parse_sync_mixed_real_deletion_and_propstat_404() {
        let xml = r#"
<d:multistatus xmlns:d="DAV:" xmlns:cal="urn:ietf:params:xml:ns:caldav">
  <d:response>
    <d:href>/cal/deleted.ics</d:href>
    <d:status>HTTP/1.1 404 Not Found</d:status>
  </d:response>
  <d:response>
    <d:href>/cal/has-propstat-404.ics</d:href>
    <d:propstat>
      <d:prop><d:displayname/></d:prop>
      <d:status>HTTP/1.1 404 Not Found</d:status>
    </d:propstat>
    <d:propstat>
      <d:prop>
        <d:getetag>"e1"</d:getetag>
        <cal:calendar-data>BEGIN:VCALENDAR
BEGIN:VEVENT
UID:withpropstat404
END:VEVENT
END:VCALENDAR</cal:calendar-data>
      </d:prop>
      <d:status>HTTP/1.1 200 OK</d:status>
    </d:propstat>
  </d:response>
  <d:response>
    <d:href>/cal/normal.ics</d:href>
    <d:propstat>
      <d:prop>
        <d:getetag>"e2"</d:getetag>
        <cal:calendar-data>BEGIN:VCALENDAR
BEGIN:VEVENT
UID:normal
END:VEVENT
END:VCALENDAR</cal:calendar-data>
      </d:prop>
      <d:status>HTTP/1.1 200 OK</d:status>
    </d:propstat>
  </d:response>
</d:multistatus>"#;

        let result = parse_sync_response(xml);
        assert_eq!(result.deleted_hrefs, vec!["/cal/deleted.ics"]);
        assert_eq!(result.changed.len(), 2);
        let hrefs: Vec<&str> = result.changed.iter().map(|e| e.href.as_str()).collect();
        assert!(hrefs.contains(&"/cal/has-propstat-404.ics"));
        assert!(hrefs.contains(&"/cal/normal.ics"));
    }

    #[test]
    fn strip_propstat_blocks_handles_uppercase_and_unprefixed() {
        // Lowercase prefix
        let s = "<d:response><d:href>/a</d:href><d:propstat><d:status>404</d:status></d:propstat></d:response>";
        let out = strip_propstat_blocks(s);
        assert_eq!(out, "<d:response><d:href>/a</d:href></d:response>");

        // Uppercase prefix (SOGo style)
        let s = "<D:response><D:href>/b</D:href><D:propstat><D:status>404</D:status></D:propstat></D:response>";
        let out = strip_propstat_blocks(s);
        assert_eq!(out, "<D:response><D:href>/b</D:href></D:response>");

        // Unprefixed
        let s = "<response><href>/c</href><propstat><status>404</status></propstat></response>";
        let out = strip_propstat_blocks(s);
        assert_eq!(out, "<response><href>/c</href></response>");

        // `<propstats>` (different element with similar prefix) must NOT be stripped
        let s = "<d:response><d:propstats-wrapper>keep me</d:propstats-wrapper></d:response>";
        let out = strip_propstat_blocks(s);
        assert!(out.contains("propstats-wrapper"));
        assert!(out.contains("keep me"));
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
    #[test]
    fn allowlist_env_var_opts_host_out_of_private_ip_check() {
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
