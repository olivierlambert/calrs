//! Emails about lead capture: the abandoned-booking alert to the host.

use super::*;

/// What the host is told about a guest who started booking and left.
pub struct LeadAlert<'a> {
    pub host_name: &'a str,
    pub host_email: &'a str,
    /// The host's saved language (`users.language`); English when unset.
    pub host_language: Option<&'a str>,
    pub event_title: &'a str,
    pub guest_name: Option<&'a str>,
    pub guest_email: &'a str,
    pub guest_phone: Option<&'a str>,
    /// Link to the leads page, when the instance knows its public URL.
    pub leads_url: Option<&'a str>,
}

/// Alert a host that a guest started a booking but did not finish. Whatever
/// the guest typed is shown as data (the HTML is escaped), never in the subject.
pub async fn send_lead_abandoned_alert(config: &SmtpConfig, alert: &LeadAlert<'_>) -> Result<()> {
    let to = named_mailbox(alert.host_name, alert.host_email)?;
    send_composed(
        config,
        Sender::Instance,
        to,
        compose_lead_abandoned(alert),
        None,
    )
    .await
}

pub(super) fn compose_lead_abandoned(alert: &LeadAlert<'_>) -> ComposedEmail {
    let lang = lang_or_en(alert.host_language);
    let mut rows = vec![row(t(lang, "confirmed-detail-event"), alert.event_title)];
    if let Some(name) = alert.guest_name.filter(|n| !n.trim().is_empty()) {
        rows.push(row(t(lang, "email-lead-detail-name"), name));
    }
    rows.push(row(t(lang, "email-lead-detail-email"), alert.guest_email));
    push_row(
        &mut rows,
        lang,
        "email-detail-phone",
        alert.guest_phone.map(str::to_string).as_ref(),
    );
    let view = t(lang, "email-lead-action-view");
    EmailParts {
        lang,
        subject: ta(
            lang,
            "email-lead-abandoned-subject",
            [("event", alert.event_title)],
        ),
        heading: t(lang, "email-lead-abandoned-heading"),
        message: t(lang, "email-lead-abandoned-message"),
        accent: "#6366f1",
        rows,
        plain_lines: alert
            .leads_url
            .map(|url| vec![format!("{view}: {url}")])
            .unwrap_or_default(),
        footer_plain: None,
        footer_html: None,
        actions: alert
            .leads_url
            .map(|url| {
                vec![EmailAction {
                    label: view.clone(),
                    url: url.to_string(),
                    color: "#6366f1".to_string(),
                }]
            })
            .unwrap_or_default(),
    }
    .compose()
}

/// One guest in a digest.
pub struct DigestItem<'a> {
    pub event_title: &'a str,
    pub guest_name: Option<&'a str>,
    pub guest_email: &'a str,
    pub guest_phone: Option<&'a str>,
}

/// Several abandoned bookings in one email, for hosts who would rather not get
/// one message per guest.
pub struct LeadDigest<'a> {
    pub host_name: &'a str,
    pub host_email: &'a str,
    pub host_language: Option<&'a str>,
    /// The first guests, already capped by the caller.
    pub items: Vec<DigestItem<'a>>,
    /// How many leads the digest stands for, shown ones included.
    pub total: usize,
    pub leads_url: Option<&'a str>,
}

/// Most guests a digest lists; the rest are a count and a link.
pub const DIGEST_MAX_ITEMS: usize = 20;

pub async fn send_lead_digest(config: &SmtpConfig, digest: &LeadDigest<'_>) -> Result<()> {
    let to = named_mailbox(digest.host_name, digest.host_email)?;
    send_composed(
        config,
        Sender::Instance,
        to,
        compose_lead_digest(digest),
        None,
    )
    .await
}

pub(super) fn compose_lead_digest(digest: &LeadDigest<'_>) -> ComposedEmail {
    let lang = lang_or_en(digest.host_language);
    let count = digest.total.to_string();
    let mut rows = Vec::new();
    for item in &digest.items {
        let mut who = match item.guest_name.filter(|n| !n.trim().is_empty()) {
            Some(name) => format!("{name} <{}>", item.guest_email),
            None => item.guest_email.to_string(),
        };
        if let Some(phone) = item.guest_phone.filter(|p| !p.trim().is_empty()) {
            who.push_str(&format!(" · {phone}"));
        }
        rows.push(row(item.event_title.to_string(), who));
    }
    if digest.total > digest.items.len() {
        rows.push(row(
            t(lang, "email-lead-digest-more"),
            (digest.total - digest.items.len()).to_string(),
        ));
    }
    let view = t(lang, "email-lead-action-view");
    EmailParts {
        lang,
        subject: ta(lang, "email-lead-digest-subject", [("count", &count)]),
        heading: t(lang, "email-lead-abandoned-heading"),
        message: ta(lang, "email-lead-digest-message", [("count", &count)]),
        accent: "#6366f1",
        rows,
        plain_lines: digest
            .leads_url
            .map(|url| vec![format!("{view}: {url}")])
            .unwrap_or_default(),
        footer_plain: None,
        footer_html: None,
        actions: digest
            .leads_url
            .map(|url| {
                vec![EmailAction {
                    label: view.clone(),
                    url: url.to_string(),
                    color: "#6366f1".to_string(),
                }]
            })
            .unwrap_or_default(),
    }
    .compose()
}
