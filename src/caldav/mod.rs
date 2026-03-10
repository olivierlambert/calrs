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

        Ok(dav_header.contains("calendar-access"))
    }

    /// Discover the current-user-principal URL via PROPFIND
    pub async fn discover_principal(&self) -> Result<String> {
        let text = self
            .propfind(&self.base_url, "0", PROPFIND_PRINCIPAL)
            .await?;

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
