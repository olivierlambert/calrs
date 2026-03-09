use anyhow::{bail, Result};
use reqwest::Client;
use std::time::Duration;

pub struct CaldavClient {
    client: Client,
    base_url: String,
    origin: String,
    username: String,
    password: String,
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
        let resp = self
            .client
            .request(reqwest::Method::from_bytes(b"PROPFIND")?, url)
            .basic_auth(&self.username, Some(&self.password))
            .header("Content-Type", "application/xml; charset=utf-8")
            .header("Depth", depth)
            .body(body.to_string())
            .send()
            .await?;

        if !resp.status().is_success() && resp.status().as_u16() != 207 {
            bail!(
                "PROPFIND {} returned {}",
                url,
                resp.status()
            );
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

        Ok(dav_header.contains("calendar-access"))
    }

    /// Discover the current-user-principal URL via PROPFIND
    pub async fn discover_principal(&self) -> Result<String> {
        let text = self.propfind(&self.base_url, "0", PROPFIND_PRINCIPAL).await?;

        // Extract href inside <d:current-user-principal><d:href>...</d:href></d:current-user-principal>
        if let Some(principal_start) = text.find("<d:current-user-principal>") {
            let after = &text[principal_start..];
            if let Some(href) = extract_tag(after, "d:href") {
                return Ok(href);
            }
        }

        bail!("Could not discover principal URL from response")
    }

    /// Discover the calendar-home-set from a principal URL
    pub async fn discover_calendar_home(&self, principal_url: &str) -> Result<String> {
        let url = self.resolve_url(principal_url);
        let text = self.propfind(&url, "0", PROPFIND_CALENDAR_HOME).await?;

        // Extract href inside <cal:calendar-home-set><d:href>...</d:href></cal:calendar-home-set>
        if let Some(home_start) = text.find("<cal:calendar-home-set>") {
            let after = &text[home_start..];
            if let Some(href) = extract_tag(after, "d:href") {
                return Ok(href);
            }
        }

        bail!("Could not discover calendar-home-set from response")
    }

    /// List calendars under a calendar-home-set URL (filters to actual calendars only)
    pub async fn list_calendars(&self, home_url: &str) -> Result<Vec<CalendarInfo>> {
        let url = self.resolve_url(home_url);
        let text = self.propfind(&url, "1", PROPFIND_CALENDARS).await?;
        let calendars = parse_calendar_list(&text);
        Ok(calendars)
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
}

#[derive(Debug, Clone)]
pub struct CalendarInfo {
    pub href: String,
    pub display_name: Option<String>,
    pub color: Option<String>,
    pub ctag: Option<String>,
}

#[derive(Debug, Clone)]
pub struct RawEvent {
    pub href: String,
    pub ical_data: String,
}

fn parse_calendar_list(xml: &str) -> Vec<CalendarInfo> {
    let mut calendars = Vec::new();
    for response_block in xml.split("<d:response>").skip(1) {
        // Only include actual calendar collections (has <cal:calendar/> in resourcetype)
        if !response_block.contains("<cal:calendar/>") {
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
        calendars.push(CalendarInfo {
            href,
            display_name,
            color,
            ctag,
        });
    }
    calendars
}

fn parse_event_responses(xml: &str) -> Vec<RawEvent> {
    let mut events = Vec::new();
    for response_block in xml.split("<d:response>").skip(1) {
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

fn extract_tag(xml: &str, tag: &str) -> Option<String> {
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
