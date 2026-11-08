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
    /// Tests only: treat this client as a Google one and send its Calendar
    /// API calls to this base instead of googleapis.com.
    #[cfg(test)]
    google_api_base: Option<String>,
}

/// See `CaldavClient::treat_as_google`.
#[cfg(test)]
static TEST_GOOGLE_ORIGINS: std::sync::LazyLock<
    std::sync::Mutex<std::collections::HashMap<String, String>>,
> = std::sync::LazyLock::new(Default::default);

/// Check if an IP address is in a private/reserved range (SSRF protection).
/// Also used by outgoing booking webhooks (`web::webhooks`).
///
/// Anything that is not ordinary public unicast counts as private: an
/// operator who needs one of these hosts lists it in the private-host
/// allowlist. IPv6 forms that carry an IPv4 address (mapped, compatible,
/// NAT64 well-known prefix, 6to4) are judged by that IPv4 address.
pub(crate) fn is_private_ip(ip: &IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => is_private_v4(v4),
        IpAddr::V6(v6) => is_private_v6(v6),
    }
}

fn is_private_v4(v4: &std::net::Ipv4Addr) -> bool {
    let [a, b, _, _] = v4.octets();
    a == 0 // 0.0.0.0/8 ("this network", 0.0.0.0 included)
        || v4.is_loopback() // 127.0.0.0/8
        || v4.is_private() // 10/8, 172.16/12, 192.168/16
        || v4.is_link_local() // 169.254/16
        || (a == 100 && (b & 0xC0) == 64) // 100.64/10 (CGNAT)
        || (a == 198 && (b & 0xFE) == 18) // 198.18/15 (benchmarking)
        || v4.is_multicast() // 224/4
        || a >= 240 // 240/4 reserved, 255.255.255.255 broadcast included
}

fn is_private_v6(v6: &std::net::Ipv6Addr) -> bool {
    if v6.is_loopback() || v6.is_unspecified() {
        return true; // ::1, ::
    }
    let seg = v6.segments();
    // ::ffff:a.b.c.d (mapped) and ::a.b.c.d (compatible) reach that IPv4 host.
    if let Some(v4) = v6.to_ipv4() {
        return is_private_v4(&v4);
    }
    let embedded = |hi: u16, lo: u16| {
        std::net::Ipv4Addr::new((hi >> 8) as u8, hi as u8, (lo >> 8) as u8, lo as u8)
    };
    // ::ffff:0:a.b.c.d (SIIT, IPv4-translated, RFC 2765) reaches that host.
    if seg[..6] == [0, 0, 0, 0, 0xffff, 0] {
        return is_private_v4(&embedded(seg[6], seg[7]));
    }
    // 64:ff9b::/96, the NAT64 well-known prefix: the gateway connects to the
    // IPv4 address in the last 32 bits.
    if seg[..6] == [0x64, 0xff9b, 0, 0, 0, 0] {
        return is_private_v4(&embedded(seg[6], seg[7]));
    }
    // 64:ff9b:1::/48 is for local NAT64 deployments: not public.
    if seg[0] == 0x64 && seg[1] == 0xff9b && seg[2] == 1 {
        return true;
    }
    // 2002::/16 (6to4) embeds the IPv4 address in bits 16 to 47.
    if seg[0] == 0x2002 {
        return is_private_v4(&embedded(seg[1], seg[2]));
    }
    (seg[0] & 0xfe00) == 0xfc00 // fc00::/7 unique local
        || (seg[0] & 0xffc0) == 0xfe80 // fe80::/10 link-local
        || (seg[0] & 0xffc0) == 0xfec0 // fec0::/10 site-local (deprecated)
        || (seg[0] & 0xff00) == 0xff00 // ff00::/8 multicast
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
    /// Only send Google OAuth credentials to the Calendar API for a Google
    /// source. Other CalDAV servers (including bearer-auth servers) stay local.
    pub(crate) fn google_access_token(&self) -> Option<&str> {
        let url = reqwest::Url::parse(&self.base_url).ok()?;
        if url.scheme() != "https" || url.host_str() != Some(GOOGLE_CALDAV_HOST) {
            return None;
        }
        match &self.auth {
            CaldavAuth::Bearer { access_token } => Some(access_token),
            _ => None,
        }
    }

    pub(crate) fn base_url(&self) -> &str {
        &self.base_url
    }

    /// The Calendar API base and token for a Google client, `None` for any
    /// other server. Booking writes to Google take the Calendar API detour
    /// in `google_calendar` so Google sends no invitation of its own.
    pub(crate) fn google_calendar_api(&self) -> Option<(&str, &str)> {
        #[cfg(test)]
        if let (Some(base), CaldavAuth::Bearer { access_token }) =
            (&self.google_api_base, &self.auth)
        {
            return Some((base, access_token));
        }
        self.google_access_token()
            .map(|token| (crate::google_meet::CALENDAR_API_BASE, token))
    }

    /// Tests only: every client built from now on for `origin` (scheme, host
    /// and port, as `build` computes it) is a Google one whose Calendar API
    /// lives at `api_base`. Lets a fake Google stand behind clients the code
    /// under test builds itself (`build_client_for_source`). Each fake listens
    /// on its own port, so parallel tests do not see each other's entries.
    #[cfg(test)]
    pub(crate) fn treat_as_google(origin: &str, api_base: &str) {
        TEST_GOOGLE_ORIGINS.lock().unwrap().insert(
            origin.trim_end_matches('/').to_string(),
            api_base.trim_end_matches('/').to_string(),
        );
    }

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
            auth,
            #[cfg(test)]
            google_api_base: TEST_GOOGLE_ORIGINS.lock().unwrap().get(&origin).cloned(),
            origin,
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

        tracing::debug!(url = %url, status = %resp.status(), "PROPFIND response received");
        multistatus_body(resp, "PROPFIND", url).await
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

        let (principal, text) = parse_blocking(text, current_user_principal).await;
        let principal = principal
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

        let (home, text) = parse_blocking(text, calendar_home_set).await;
        let home = home
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
        let (calendars, text) = parse_blocking(text, parse_calendar_list).await;
        let calendars = calendars
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
    ///
    /// On Google, the ATTENDEE and ORGANIZER lines do not go through CalDAV:
    /// Google ignores `SCHEDULE-AGENT=CLIENT` and would mail its own
    /// invitation. They are set through the Calendar API with
    /// `sendUpdates=none` instead (`google_calendar`).
    pub async fn put_event(&self, calendar_href: &str, uid: &str, ics_data: &str) -> Result<()> {
        if let Some((api_base, token)) = self.google_calendar_api() {
            let Some(calendar_id) = crate::google_meet::calendar_id_from_caldav_href(calendar_href)
            else {
                // No calendar id, so no Calendar API: the attendees cannot be
                // set silently, and over CalDAV Google would mail them.
                tracing::warn!(uid = %uid, "google calendar: no calendar id in the collection href, writing the event without attendees");
                let (host_only, _) = crate::google_calendar::split_scheduling(ics_data);
                return self.put_event_raw(calendar_href, uid, &host_only).await;
            };
            return crate::google_calendar::put_event_without_invitations(
                self,
                &crate::google_calendar::EventsApi::new(api_base, token, &calendar_id),
                calendar_href,
                uid,
                ics_data,
            )
            .await;
        }
        self.put_event_raw(calendar_href, uid, ics_data).await
    }

    /// On Google, make sure the event `uid` lists the attendees of
    /// `ics_data` without rewriting it (`google_calendar::ensure_attendees`).
    /// Nothing to do on any other server.
    pub(crate) async fn ensure_google_attendees(
        &self,
        calendar_href: &str,
        uid: &str,
        ics_data: &str,
    ) -> Result<()> {
        let Some((api_base, token)) = self.google_calendar_api() else {
            return Ok(());
        };
        let Some(calendar_id) = crate::google_meet::calendar_id_from_caldav_href(calendar_href)
        else {
            return Ok(());
        };
        crate::google_calendar::ensure_attendees(
            &crate::google_calendar::EventsApi::new(api_base, token, &calendar_id),
            uid,
            ics_data,
        )
        .await
    }

    /// Plain CalDAV PUT of `ics_data`, whatever the server.
    pub(crate) async fn put_event_raw(
        &self,
        calendar_href: &str,
        uid: &str,
        ics_data: &str,
    ) -> Result<()> {
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
            // Read only to tell a Google rate limit or quota (a 403 that
            // passes) from a refusal for good; the error carries no body.
            let body = if status.as_u16() == 403 {
                resp.text().await.unwrap_or_default()
            } else {
                String::new()
            };
            if refused_for_good(status.as_u16(), &body) {
                return Err(anyhow::Error::new(crate::providers::PermanentRefusal {
                    summary: status.to_string(),
                    message: format!("PUT {url} returned {status}"),
                }));
            }
            bail!("PUT {} returned {}", url, status);
        }

        Ok(())
    }

    /// GET one event resource: its iCalendar text, `None` when the server
    /// says it does not exist (404/410). Any other answer is an error.
    pub(crate) async fn get_event_raw(
        &self,
        calendar_href: &str,
        uid: &str,
    ) -> Result<Option<String>> {
        let href = format!("{}/{}.ics", calendar_href.trim_end_matches('/'), uid);
        let url = self.resolve_url(&href);
        let resp = self
            .apply_auth(self.client.get(&url))
            .timeout(Duration::from_secs(10))
            .send()
            .await?;
        let status = resp.status();
        if matches!(status.as_u16(), 404 | 410) {
            return Ok(None);
        }
        if !status.is_success() {
            bail!("GET {} returned {}", url, status);
        }
        Ok(Some(resp.text().await?))
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
                let xml = read_body(resp, crate::xml::MAX_INPUT_BYTES).await?;
                let (exists, _) =
                    parse_blocking(xml, |xml| Ok(propfind_resource_exists(xml))).await;
                return match exists.ok().flatten() {
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
    ///
    /// On Google, an event that carries attendees is deleted through the
    /// Calendar API with `sendUpdates=none`, so Google mails no cancellation
    /// (calrs sends its own).
    pub async fn delete_event(&self, calendar_href: &str, uid: &str) -> Result<()> {
        if let Some((api_base, token)) = self.google_calendar_api() {
            if let Some(calendar_id) =
                crate::google_meet::calendar_id_from_caldav_href(calendar_href)
            {
                return crate::google_calendar::delete_event_without_cancellations(
                    self,
                    &crate::google_calendar::EventsApi::new(api_base, token, &calendar_id),
                    calendar_href,
                    uid,
                    crate::google_calendar::DeleteFallback::CalDav,
                )
                .await;
            }
        }
        self.delete_event_raw(calendar_href, uid).await
    }

    /// Delete a copy of a booking that moved elsewhere (#47). Like
    /// `delete_event`, except that on Google an event with attendees the
    /// Calendar API cannot delete is left for a later try rather than
    /// deleted over CalDAV, which would have Google mail a cancellation of
    /// a meeting that still takes place.
    pub(crate) async fn release_event(&self, calendar_href: &str, uid: &str) -> Result<()> {
        if let Some((api_base, token)) = self.google_calendar_api() {
            if let Some(calendar_id) =
                crate::google_meet::calendar_id_from_caldav_href(calendar_href)
            {
                return crate::google_calendar::delete_event_without_cancellations(
                    self,
                    &crate::google_calendar::EventsApi::new(api_base, token, &calendar_id),
                    calendar_href,
                    uid,
                    crate::google_calendar::DeleteFallback::Retry,
                )
                .await;
            }
        }
        self.delete_event_raw(calendar_href, uid).await
    }

    /// Plain CalDAV DELETE, whatever the server.
    pub(crate) async fn delete_event_raw(&self, calendar_href: &str, uid: &str) -> Result<()> {
        let href = format!("{}/{}.ics", calendar_href.trim_end_matches('/'), uid);
        let url = self.resolve_url(&href);

        let req = self.client.delete(&url);
        let resp = self.apply_auth(req).send().await?;

        let status = resp.status();
        // Already gone counts as deleted: 404, or 410 Gone.
        if !status.is_success() && !matches!(status.as_u16(), 204 | 404 | 410) {
            bail!("DELETE {} returned {}", url, status);
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

        let text = multistatus_body(resp, "REPORT", &url).await?;
        parse_blocking(text, parse_event_responses)
            .await
            .0
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
        let text = multistatus_body(resp, "sync-collection REPORT", &url).await?;
        parse_blocking(text, parse_sync_response)
            .await
            .0
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

        // A server that rejects the time-range filter answers with an error
        // status: fall back to the unfiltered fetch, which checks its own.
        if !resp.status().is_success() {
            return self.fetch_events(calendar_href).await;
        }

        let text = multistatus_body(resp, "REPORT", &url).await?;
        parse_blocking(text, parse_event_responses)
            .await
            .0
            .with_context(|| format!("Could not read the calendar REPORT response from {url}"))
    }
}

/// The body of an answer that must be a multistatus: a PROPFIND, or a
/// REPORT whose result the sync stores or acts on. Only 207 Multi-Status is
/// accepted, and 200 OK, which some servers send with the same multistatus
/// body (the parse then requires one). Any other status is an error, so an
/// error page, or an empty 5xx body, can never read as an empty calendar.
///
/// The error names the status code and reason only, never the body: on a
/// shared calendar an error page can quote other people's event data, and
/// the message is stored and shown on the sources page.
async fn multistatus_body(resp: reqwest::Response, what: &str, url: &str) -> Result<String> {
    let status = resp.status();
    if status.as_u16() != 207 && status != reqwest::StatusCode::OK {
        bail!("{what} {url} returned {status}");
    }
    read_body(resp, crate::xml::MAX_INPUT_BYTES).await
}

/// Read a response body as text, refusing one over `cap` bytes as it
/// arrives rather than after holding all of it: a declared length over the
/// cap fails before anything is read. Decoded with the charset of the
/// `Content-Type` (UTF-8 by default), as reqwest's `text()` does.
async fn read_body(mut resp: reqwest::Response, cap: usize) -> Result<String> {
    let url = resp.url().to_string();
    if let Some(len) = resp.content_length() {
        if len > cap as u64 {
            bail!("response from {url} is {len} bytes, more than the {cap} calrs reads");
        }
    }
    let encoding = response_encoding(resp.headers());
    let mut body = Vec::new();
    while let Some(chunk) = resp.chunk().await? {
        if body.len() + chunk.len() > cap {
            bail!("response from {url} is more than the {cap} bytes calrs reads");
        }
        body.extend_from_slice(&chunk);
    }
    if encoding == encoding_rs::UTF_8 {
        // The usual case: valid UTF-8 becomes the string without a copy.
        match String::from_utf8(body) {
            Ok(text) => return Ok(text),
            Err(invalid) => body = invalid.into_bytes(),
        }
    }
    let (text, _, _) = encoding.decode(&body);
    Ok(text.into_owned())
}

/// Run a parse on the blocking pool: a large document takes real CPU, and
/// the same runtime serves the booking pages. The text comes back with the
/// result, for the callers that log it.
async fn parse_blocking<T, F>(text: String, parse: F) -> (Result<T>, String)
where
    T: Send + 'static,
    F: FnOnce(&str) -> Result<T> + Send + 'static,
{
    let task = tokio::task::spawn_blocking(move || {
        let parsed = parse(&text);
        (parsed, text)
    });
    match task.await {
        Ok(done) => done,
        Err(e) => (
            Err(anyhow::anyhow!("CalDAV response parser failed: {e}")),
            String::new(),
        ),
    }
}

/// The encoding named by a `Content-Type` charset parameter, else UTF-8.
fn response_encoding(headers: &reqwest::header::HeaderMap) -> &'static encoding_rs::Encoding {
    headers
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| {
            value.split(';').skip(1).find_map(|param| {
                let (name, label) = param.split_once('=')?;
                name.trim()
                    .eq_ignore_ascii_case("charset")
                    .then(|| label.trim().trim_matches('"'))
            })
        })
        .and_then(|label| encoding_rs::Encoding::for_label(label.as_bytes()))
        .unwrap_or(encoding_rs::UTF_8)
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
    /// The server stopped before reporting every change (RFC 6578 section
    /// 3.6, a 507 on the collection itself): `new_sync_token` is where it
    /// stopped, and more changes follow from there.
    pub truncated: bool,
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

/// The reasons Google gives with a 403 that pass with time: rate limits and
/// quotas.
const GOOGLE_TRANSIENT_403: [&str; 4] = [
    "rateLimitExceeded",
    "userRateLimitExceeded",
    "quotaExceeded",
    "dailyLimitExceeded",
];

/// Whether a refused PUT will be refused again on a retry (#47): the
/// calendar is gone (404), does not take writes (405), or forbids them
/// (403). Google answers its rate limits and quotas with a 403 too, which
/// pass.
pub(crate) fn refused_for_good(status: u16, body: &str) -> bool {
    match status {
        404 | 405 => true,
        403 => !GOOGLE_TRANSIENT_403
            .iter()
            .any(|reason| body.contains(reason)),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- HTTP status and body checks (#325) ---

    /// One response carrying everything the four sync calls read: a calendar
    /// resourcetype for the listing, calendar data for the REPORTs, and a
    /// sync token.
    const MULTISTATUS: &str = r#"<d:multistatus xmlns:d="DAV:" xmlns:c="urn:ietf:params:xml:ns:caldav"><d:response><d:href>/dav/cal/e.ics</d:href><d:propstat><d:prop><d:resourcetype><c:calendar/></d:resourcetype><c:calendar-data>BEGIN:VCALENDAR</c:calendar-data></d:prop><d:status>HTTP/1.1 200 OK</d:status></d:propstat></d:response><d:sync-token>t2</d:sync-token></d:multistatus>"#;

    /// A server answering every request with what `reply` returns for its
    /// method and body: (status, content type, body).
    async fn serve<F>(reply: F) -> String
    where
        F: Fn(&str, &str) -> (u16, &'static str, Vec<u8>) + Clone + Send + Sync + 'static,
    {
        use axum::http::{header, Method, StatusCode};
        let app = axum::Router::new().fallback(move |method: Method, body: String| {
            let reply = reply.clone();
            async move {
                let (status, content_type, bytes) = reply(method.as_str(), &body);
                (
                    StatusCode::from_u16(status).unwrap(),
                    [(header::CONTENT_TYPE, content_type)],
                    bytes,
                )
            }
        });
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });
        format!("http://127.0.0.1:{port}/dav/")
    }

    /// A server answering one request with a chunked 207 (no length known
    /// up front), with the extra `headers` lines.
    async fn serve_chunked(headers: &'static str, chunks: Vec<Vec<u8>>) -> String {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = [0u8; 4096];
            let _ = socket.read(&mut request).await;
            let mut out = format!(
                "HTTP/1.1 207 Multi-Status\r\n{headers}Transfer-Encoding: chunked\r\nConnection: close\r\n\r\n"
            )
            .into_bytes();
            for chunk in chunks {
                out.extend(format!("{:x}\r\n", chunk.len()).bytes());
                out.extend(chunk);
                out.extend(b"\r\n");
            }
            out.extend(b"0\r\n\r\n");
            let _ = socket.write_all(&out).await;
        });
        format!("http://127.0.0.1:{port}/")
    }

    async fn get(url: &str) -> reqwest::Response {
        reqwest::Client::new().get(url).send().await.unwrap()
    }

    #[tokio::test]
    async fn error_statuses_fail_every_call_the_sync_acts_on() {
        let cases: [(u16, &'static str, &'static str); 3] = [
            (500, "text/plain", ""),
            (
                503,
                "text/html",
                "<html><body><h1>503 Service Unavailable</h1><p>Back soon</p></body></html>",
            ),
            (403, "application/xml", ""),
        ];
        for (status, content_type, body) in cases {
            let base = serve(move |_, _| (status, content_type, body.as_bytes().to_vec())).await;
            let client = CaldavClient::new(&base, "u", "p");
            let code = status.to_string();
            let errors = [
                client.list_calendars("/dav/").await.map(|_| ()),
                client.fetch_events("/dav/cal/").await.map(|_| ()),
                client
                    .fetch_events_since("/dav/cal/", "20260101T000000Z")
                    .await
                    .map(|_| ()),
                client
                    .sync_collection("/dav/cal/", Some("t1"))
                    .await
                    .map(|_| ()),
                client.sync_collection("/dav/cal/", None).await.map(|_| ()),
                client.discover_principal().await.map(|_| ()),
            ];
            for (n, result) in errors.into_iter().enumerate() {
                let err = format!("{:#}", result.unwrap_err());
                assert!(err.contains(&code), "call {n} on {status}: {err}");
            }
        }
        // The body never reaches the message: on a shared calendar an error
        // page can quote other people's events, and the message is stored.
        let base = serve(|_, _| {
            (
                503,
                "text/html",
                b"<p>SUMMARY:Alice - medical appointment</p>".to_vec(),
            )
        })
        .await;
        let client = CaldavClient::new(&base, "u", "p");
        let errors = [
            client.fetch_events("/dav/cal/").await.map(|_| ()),
            client.list_calendars("/dav/").await.map(|_| ()),
            client.put_event("/dav/cal/", "x", "BEGIN:VCALENDAR").await,
            client.delete_event("/dav/cal/", "x").await,
        ];
        for result in errors {
            let err = format!("{:#}", result.unwrap_err());
            assert!(err.contains("503 Service Unavailable"), "{err}");
            assert!(!err.contains("Alice"), "{err}");
        }
    }

    #[tokio::test]
    async fn multistatus_answers_are_read_under_207_and_200() {
        for status in [207, 200] {
            let base =
                serve(move |_, _| (status, "application/xml", MULTISTATUS.as_bytes().to_vec()))
                    .await;
            let client = CaldavClient::new(&base, "u", "p");
            assert_eq!(client.list_calendars("/dav/").await.unwrap().len(), 1);
            assert_eq!(client.fetch_events("/dav/cal/").await.unwrap().len(), 1);
            let since = client
                .fetch_events_since("/dav/cal/", "20260101T000000Z")
                .await
                .unwrap();
            assert_eq!(since[0].ical_data, "BEGIN:VCALENDAR");
            let delta = client
                .sync_collection("/dav/cal/", Some("t1"))
                .await
                .unwrap();
            assert_eq!(delta.new_sync_token.as_deref(), Some("t2"));
        }
    }

    #[tokio::test]
    async fn empty_or_unexpected_success_answers_are_errors() {
        for (status, body) in [
            (207, ""),
            (200, ""),
            (200, "<html><body/></html>"),
            (204, ""),
        ] {
            let base =
                serve(move |_, _| (status, "application/xml", body.as_bytes().to_vec())).await;
            let client = CaldavClient::new(&base, "u", "p");
            assert!(
                client.fetch_events("/dav/cal/").await.is_err(),
                "{status} {body:?}"
            );
            assert!(client.list_calendars("/dav/").await.is_err());
            assert!(client
                .sync_collection("/dav/cal/", Some("t"))
                .await
                .is_err());
        }
    }

    #[tokio::test]
    async fn a_rejected_time_range_still_falls_back_to_the_full_fetch() {
        let base = serve(|method, body| {
            if method == "REPORT" && body.contains("time-range") {
                (400, "text/plain", b"time-range not supported".to_vec())
            } else {
                (207, "application/xml", MULTISTATUS.as_bytes().to_vec())
            }
        })
        .await;
        let client = CaldavClient::new(&base, "u", "p");
        let events = client
            .fetch_events_since("/dav/cal/", "20260101T000000Z")
            .await
            .unwrap();
        assert_eq!(events.len(), 1);

        // The fallback checks its own status.
        let base = serve(|method, body| {
            if method == "REPORT" && body.contains("time-range") {
                (400, "text/plain", Vec::new())
            } else {
                (500, "text/plain", Vec::new())
            }
        })
        .await;
        let err = CaldavClient::new(&base, "u", "p")
            .fetch_events_since("/dav/cal/", "20260101T000000Z")
            .await
            .unwrap_err()
            .to_string();
        assert!(err.contains("500"), "{err}");
    }

    #[tokio::test]
    async fn a_rejected_sync_token_still_reads_as_unsupported() {
        for status in [403, 405, 412] {
            let base = serve(move |_, _| (status, "application/xml", Vec::new())).await;
            let err = CaldavClient::new(&base, "u", "p")
                .sync_collection("/dav/cal/", Some("old"))
                .await
                .unwrap_err()
                .to_string();
            assert!(err.contains("sync-token invalid or not supported"), "{err}");
        }
    }

    #[tokio::test]
    async fn bodies_over_the_cap_are_refused_as_they_arrive() {
        // A declared length over the cap fails before the body is read.
        let base = serve(|_, _| (207, "application/xml", vec![b'x'; 2000])).await;
        let err = read_body(get(&base).await, 1000)
            .await
            .unwrap_err()
            .to_string();
        assert!(err.contains("2000 bytes"), "{err}");
        assert_eq!(read_body(get(&base).await, 2000).await.unwrap().len(), 2000);

        // Without a length (chunked), reading stops once past the cap.
        let chunks = vec![vec![b'x'; 600]; 3];
        let url = serve_chunked("", chunks.clone()).await;
        let err = read_body(get(&url).await, 1000)
            .await
            .unwrap_err()
            .to_string();
        assert!(err.contains("more than the 1000 bytes"), "{err}");
        let url = serve_chunked("", chunks).await;
        assert_eq!(read_body(get(&url).await, 1800).await.unwrap().len(), 1800);
    }

    #[tokio::test]
    async fn parses_run_on_the_blocking_pool_and_hand_the_text_back() {
        let caller = std::thread::current().id();
        let (parser, text) =
            parse_blocking("<x/>".to_string(), |_| Ok(std::thread::current().id())).await;
        assert_ne!(parser.unwrap(), caller, "parsed on the async thread");
        assert_eq!(text, "<x/>");
        let (failed, text) = parse_blocking("bad".to_string(), parse_event_responses).await;
        assert!(failed.is_err());
        assert_eq!(text, "bad");
    }

    #[tokio::test]
    async fn bodies_are_decoded_with_their_charset() {
        let url = serve_chunked(
            "Content-Type: application/xml; charset=\"ISO-8859-1\"\r\n",
            vec![b"caf\xe9".to_vec()],
        )
        .await;
        assert_eq!(read_body(get(&url).await, 100).await.unwrap(), "café");
        let url = serve_chunked(
            "Content-Type: application/xml\r\n",
            vec!["café".as_bytes().to_vec()],
        )
        .await;
        assert_eq!(read_body(get(&url).await, 100).await.unwrap(), "café");
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
    fn private_ranges_are_recognised() {
        let private = [
            "0.0.0.0",
            "0.1.2.3",
            "127.0.0.1",
            "10.1.2.3",
            "172.16.0.1",
            "192.168.1.1",
            "169.254.169.254",
            "100.64.0.1",
            "100.127.255.254",
            "198.18.0.1",
            "198.19.255.255",
            "224.0.0.1",
            "239.255.255.250",
            "240.0.0.1",
            "255.255.255.255",
            "::",
            "::1",
            "::ffff:10.0.0.1",
            "::ffff:127.0.0.1",
            "::10.0.0.1",
            "::127.0.0.1",
            "::ffff:0:10.0.0.1",
            "::ffff:0:127.0.0.1",
            "::ffff:0:169.254.169.254",
            "64:ff9b::a00:1",
            "64:ff9b::7f00:1",
            "64:ff9b:1::1",
            "2002:a00:1::1",
            "2002:c0a8:101::1",
            "fc00::1",
            "fd00::1",
            "fd12:3456::1",
            "fdff:ffff::1",
            "fe80::1",
            "febf::1",
            "fec0::1",
            "feff::1",
            "ff02::1",
            "ff05::2",
        ];
        for ip in private {
            let parsed: IpAddr = ip.parse().unwrap();
            assert!(is_private_ip(&parsed), "{ip} should be private");
        }
        let public = [
            "1.1.1.1",
            "8.8.8.8",
            "93.184.215.14",
            "100.63.255.255",
            "100.128.0.1",
            "198.17.255.255",
            "198.20.0.1",
            "223.255.255.255",
            "172.32.0.1",
            "2001:4860:4860::8888",
            "2606:4700::1111",
            "::ffff:8.8.8.8",
            "::ffff:0:8.8.8.8",
            "64:ff9b::808:808",
            "2002:808:808::1",
            "fe00::1",
            "fbff::1",
        ];
        for ip in public {
            let parsed: IpAddr = ip.parse().unwrap();
            assert!(!is_private_ip(&parsed), "{ip} should be public");
        }
    }

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
