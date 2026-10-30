//! Emails sent to hosts, team members, watchers and claimants.

use super::*;

/// Send booking notification to the host
pub async fn send_host_notification(config: &SmtpConfig, details: &BookingDetails) -> Result<()> {
    let to = named_mailbox(&details.host_name, &details.host_email)?;
    let ics = generate_ics(details, "REQUEST");
    send_composed(
        config,
        Sender::Instance,
        to,
        compose_host_notification(details),
        Some(("invite.ics", ics, "REQUEST")),
    )
    .await
}

/// The host's "new booking" email, in the host's language.
pub(super) fn compose_host_notification(details: &BookingDetails) -> ComposedEmail {
    let lang = host_lang(details);
    let (date_display, time_display) = host_time_display_exact(
        &details.date,
        &details.start_time,
        &details.end_time,
        &details.guest_timezone,
        &details.host_timezone,
        details.utc_times.as_ref(),
    );
    let date_display = long_date(lang, &date_display);
    let mut rows = host_rows(
        lang,
        &details.event_title,
        &date_display,
        &time_display,
        &details.guest_name,
        &details.guest_email,
    );
    push_row(
        &mut rows,
        lang,
        "email-detail-phone",
        details.guest_phone.as_ref(),
    );
    push_row(
        &mut rows,
        lang,
        "confirmed-detail-location",
        details.location.as_ref(),
    );
    push_row(
        &mut rows,
        lang,
        "bookings-resource",
        details.resource_name.as_ref(),
    );
    push_row(
        &mut rows,
        lang,
        "confirmed-detail-notes",
        details.notes.as_ref(),
    );
    EmailParts {
        lang,
        subject: ta(
            lang,
            "email-host-new-subject",
            [
                ("event", &details.event_title),
                ("guest", &details.guest_name),
                ("date", &date_display),
            ],
        ),
        heading: t(lang, "email-host-new-heading"),
        message: ta(
            lang,
            "email-host-new-message",
            [("guest", &details.guest_name)],
        ),
        accent: "#16a34a",
        rows,
        plain_lines: vec![],
        footer_plain: Some(t(lang, "email-confirm-ics-attached-plain")),
        footer_html: Some(t(lang, "email-confirm-ics-attached-html")),
        actions: vec![],
    }
    .compose()
}

/// Send host a confirmation that a pending booking was approved. No ICS: the
/// event is already pushed via CalDAV write-back.
pub async fn send_host_booking_confirmed(
    config: &SmtpConfig,
    details: &BookingDetails,
) -> Result<()> {
    let to = named_mailbox(&details.host_name, &details.host_email)?;
    send_composed(
        config,
        Sender::Instance,
        to,
        compose_host_booking_confirmed(details),
        None,
    )
    .await
}

pub(super) fn compose_host_booking_confirmed(details: &BookingDetails) -> ComposedEmail {
    let lang = host_lang(details);
    let (date_display, time_display) = host_time_display_exact(
        &details.date,
        &details.start_time,
        &details.end_time,
        &details.guest_timezone,
        &details.host_timezone,
        details.utc_times.as_ref(),
    );
    let date_display = long_date(lang, &date_display);
    let mut rows = host_rows(
        lang,
        &details.event_title,
        &date_display,
        &time_display,
        &details.guest_name,
        &details.guest_email,
    );
    push_row(
        &mut rows,
        lang,
        "email-detail-phone",
        details.guest_phone.as_ref(),
    );
    push_row(
        &mut rows,
        lang,
        "confirmed-detail-location",
        details.location.as_ref(),
    );
    push_row(
        &mut rows,
        lang,
        "bookings-resource",
        details.resource_name.as_ref(),
    );
    push_row(
        &mut rows,
        lang,
        "confirmed-detail-notes",
        details.notes.as_ref(),
    );
    let footer = t(lang, "email-host-confirmed-footer");
    EmailParts {
        lang,
        subject: ta(
            lang,
            "email-host-confirmed-subject",
            [
                ("event", &details.event_title),
                ("guest", &details.guest_name),
                ("date", &date_display),
            ],
        ),
        heading: t(lang, "email-host-confirmed-heading"),
        message: ta(
            lang,
            "email-host-confirmed-message",
            [("guest", &details.guest_name)],
        ),
        accent: "#16a34a",
        rows,
        plain_lines: vec![],
        footer_plain: Some(footer.clone()),
        footer_html: Some(footer),
        actions: vec![],
    }
    .compose()
}

/// Tell the host their Google Calendar still shows the old time.
///
/// A Google Meet booking lives on the host's calendar as a single Google event
/// that calrs patches in place, because a second ICS PUT would strip the
/// conference. When that patch fails for good, the booking, the guest's invite
/// and the reminders all hold the new time while the host's calendar holds the
/// old one, and without this email nobody would ever say so. `reason` is the
/// technical error, shown as the API returned it.
pub async fn send_host_calendar_sync_failure(
    config: &SmtpConfig,
    details: &BookingDetails,
    reason: &str,
) -> Result<()> {
    let to = named_mailbox(&details.host_name, &details.host_email)?;
    send_composed(
        config,
        Sender::Instance,
        to,
        compose_host_calendar_sync_failure(details, reason),
        None,
    )
    .await
}

pub(super) fn compose_host_calendar_sync_failure(
    details: &BookingDetails,
    reason: &str,
) -> ComposedEmail {
    let lang = host_lang(details);
    let (date_display, time_display) = host_time_display_exact(
        &details.date,
        &details.start_time,
        &details.end_time,
        &details.guest_timezone,
        &details.host_timezone,
        details.utc_times.as_ref(),
    );
    let date_display = long_date(lang, &date_display);
    // Recreating the event would mint a different conference, so the Meet
    // link already sitting in the guest's invite would stop admitting anyone:
    // the closing line says to move the event instead.
    let action = t(lang, "email-host-sync-failure-action");
    EmailParts {
        lang,
        subject: ta(
            lang,
            "email-host-sync-failure-subject",
            [("event", &details.event_title), ("date", &date_display)],
        ),
        heading: t(lang, "email-host-sync-failure-heading"),
        message: t(lang, "email-host-sync-failure-intro"),
        accent: "#f59e0b",
        rows: vec![
            row(t(lang, "confirmed-detail-event"), &details.event_title),
            row(t(lang, "email-detail-new-date"), date_display.clone()),
            row(t(lang, "email-detail-new-time"), time_display),
            row(
                t(lang, "common-detail-guest"),
                format!("{} <{}>", details.guest_name, details.guest_email),
            ),
            row(t(lang, "common-detail-reason"), reason),
        ],
        plain_lines: vec![],
        footer_plain: Some(action.clone()),
        footer_html: Some(action),
        actions: vec![],
    }
    .compose()
}

/// Tell the host a confirmed booking is still not on their calendar after
/// several write-back attempts (#162). `details` carries the recipient as
/// the host (name, email, language).
pub async fn send_host_writeback_failure(
    config: &SmtpConfig,
    details: &BookingDetails,
    error: &str,
) -> Result<()> {
    let to = named_mailbox(&details.host_name, &details.host_email)?;
    send_composed(
        config,
        Sender::Instance,
        to,
        compose_host_writeback_failure(details, error),
        None,
    )
    .await
}

fn compose_host_writeback_failure(details: &BookingDetails, error: &str) -> ComposedEmail {
    let lang = host_lang(details);
    let (date_display, time_display) = host_time_display_exact(
        &details.date,
        &details.start_time,
        &details.end_time,
        &details.guest_timezone,
        &details.host_timezone,
        details.utc_times.as_ref(),
    );
    let date_display = long_date(lang, &date_display);
    let action = t(lang, "email-writeback-failure-action");
    EmailParts {
        lang,
        subject: ta(
            lang,
            "email-writeback-failure-subject",
            [("event", &details.event_title), ("date", &date_display)],
        ),
        heading: t(lang, "email-writeback-failure-heading"),
        message: t(lang, "email-writeback-failure-intro"),
        accent: "#f59e0b",
        rows: vec![
            row(t(lang, "confirmed-detail-event"), &details.event_title),
            row(t(lang, "confirmed-detail-date"), date_display.clone()),
            row(t(lang, "confirmed-detail-time"), time_display),
            row(
                t(lang, "common-detail-guest"),
                format!("{} <{}>", details.guest_name, details.guest_email),
            ),
            row(t(lang, "email-detail-error"), error),
        ],
        plain_lines: vec![],
        footer_plain: Some(action.clone()),
        footer_html: Some(action),
        actions: vec![],
    }
    .compose()
}

/// A calendar source that keeps failing to sync, for its owner (#162).
#[derive(Clone, Debug)]
pub struct SourceSyncFailure {
    pub owner_name: String,
    pub owner_email: String,
    /// The owner's saved language (`users.language`); English when `None`.
    pub language: Option<String>,
    pub source_name: String,
    pub error: String,
    pub failing_since: String,
}

/// Tell a source's owner that it has stopped syncing.
pub async fn send_source_sync_failure(
    config: &SmtpConfig,
    alert: &SourceSyncFailure,
) -> Result<()> {
    let to = named_mailbox(&alert.owner_name, &alert.owner_email)?;
    send_composed(
        config,
        Sender::Instance,
        to,
        compose_source_sync_failure(alert),
        None,
    )
    .await
}

fn compose_source_sync_failure(alert: &SourceSyncFailure) -> ComposedEmail {
    let lang = lang_or_en(alert.language.as_deref());
    let action = t(lang, "email-source-failure-action");
    EmailParts {
        lang,
        subject: ta(
            lang,
            "email-source-failure-subject",
            [("source", &alert.source_name)],
        ),
        heading: t(lang, "email-source-failure-heading"),
        message: t(lang, "email-source-failure-intro"),
        accent: "#f59e0b",
        rows: vec![
            row(t(lang, "email-detail-source"), &alert.source_name),
            row(t(lang, "email-detail-failing-since"), &alert.failing_since),
            row(t(lang, "email-detail-error"), &alert.error),
        ],
        plain_lines: vec![],
        footer_plain: Some(action.clone()),
        footer_html: Some(action),
        actions: vec![],
    }
    .compose()
}

/// Send booking reminder to the host
pub async fn send_host_reminder(config: &SmtpConfig, details: &BookingDetails) -> Result<()> {
    let to = named_mailbox(&details.host_name, &details.host_email)?;
    send_composed(
        config,
        Sender::Instance,
        to,
        compose_host_reminder(details),
        None,
    )
    .await
}

pub(super) fn compose_host_reminder(details: &BookingDetails) -> ComposedEmail {
    let lang = host_lang(details);
    let (date_display, time_display) = host_time_display_exact(
        &details.date,
        &details.start_time,
        &details.end_time,
        &details.guest_timezone,
        &details.host_timezone,
        details.utc_times.as_ref(),
    );
    let date_display = long_date(lang, &date_display);
    let mut rows = host_rows(
        lang,
        &details.event_title,
        &date_display,
        &time_display,
        &details.guest_name,
        &details.guest_email,
    );
    push_row(
        &mut rows,
        lang,
        "email-detail-phone",
        details.guest_phone.as_ref(),
    );
    push_row(
        &mut rows,
        lang,
        "confirmed-detail-location",
        details.location.as_ref(),
    );
    push_row(
        &mut rows,
        lang,
        "bookings-resource",
        details.resource_name.as_ref(),
    );
    push_row(
        &mut rows,
        lang,
        "confirmed-detail-notes",
        details.notes.as_ref(),
    );
    EmailParts {
        lang,
        subject: ta(
            lang,
            "email-host-reminder-subject",
            [
                ("event", &details.event_title),
                ("guest", &details.guest_name),
                ("date", &date_display),
            ],
        ),
        heading: t(lang, "email-host-reminder-heading"),
        message: ta(
            lang,
            "email-host-reminder-message",
            [("guest", &details.guest_name)],
        ),
        accent: "#3b82f6",
        rows,
        plain_lines: vec![],
        footer_plain: None,
        footer_html: None,
        actions: vec![],
    }
    .compose()
}

/// Send cancellation notification to the host
pub async fn send_host_cancellation(
    config: &SmtpConfig,
    details: &CancellationDetails,
) -> Result<()> {
    let to = named_mailbox(&details.host_name, &details.host_email)?;
    let ics = generate_cancel_ics(details);
    send_composed(
        config,
        Sender::Instance,
        to,
        compose_host_cancellation(details),
        Some(("cancel.ics", ics, "CANCEL")),
    )
    .await
}

pub(super) fn compose_host_cancellation(details: &CancellationDetails) -> ComposedEmail {
    let lang = lang_or_en(details.host_language.as_deref());
    let (date_display, time_display) = host_time_display_exact(
        &details.date,
        &details.start_time,
        &details.end_time,
        &details.guest_timezone,
        &details.host_timezone,
        details.utc_times.as_ref(),
    );
    let date_display = long_date(lang, &date_display);
    let mut rows = host_rows(
        lang,
        &details.event_title,
        &date_display,
        &time_display,
        &details.guest_name,
        &details.guest_email,
    );
    push_row(
        &mut rows,
        lang,
        "common-detail-reason",
        details.reason.as_ref(),
    );
    let message = if details.cancelled_by_host {
        t(lang, "email-host-cancel-message-by-host")
    } else {
        ta(
            lang,
            "email-host-cancel-message-by-guest",
            [("guest", &details.guest_name)],
        )
    };
    EmailParts {
        lang,
        subject: ta(
            lang,
            "email-host-cancel-subject",
            [
                ("event", &details.event_title),
                ("guest", &details.guest_name),
                ("date", &date_display),
            ],
        ),
        heading: t(lang, "cancelled-heading"),
        message,
        accent: "#dc2626",
        rows,
        plain_lines: vec![],
        footer_plain: Some(t(lang, "email-cancel-ics-attached-plain")),
        footer_html: Some(t(lang, "email-cancel-ics-attached-html")),
        actions: vec![],
    }
    .compose()
}

/// Send approval request to host with approve/decline buttons
pub async fn send_host_approval_request(
    config: &SmtpConfig,
    details: &BookingDetails,
    _booking_id: &str,
    confirm_token: Option<&str>,
    base_url: Option<&str>,
) -> Result<()> {
    send_host_approval_email(config, details, confirm_token, base_url, None).await
}

/// The one reminder a host gets for a request still unanswered halfway to its
/// expiry (#234): the approval request again, with the same Approve/Decline
/// buttons, saying when the request will be declined on its own.
/// `expires_display` is the deadline already formatted in the host's timezone.
pub async fn send_host_approval_reminder(
    config: &SmtpConfig,
    details: &BookingDetails,
    confirm_token: Option<&str>,
    base_url: Option<&str>,
    expires_display: &str,
) -> Result<()> {
    send_host_approval_email(
        config,
        details,
        confirm_token,
        base_url,
        Some(expires_display),
    )
    .await
}

/// Approval request (`expires` = `None`) or its reminder (`Some(deadline)`).
async fn send_host_approval_email(
    config: &SmtpConfig,
    details: &BookingDetails,
    confirm_token: Option<&str>,
    base_url: Option<&str>,
    expires: Option<&str>,
) -> Result<()> {
    let to = named_mailbox(&details.host_name, &details.host_email)?;
    let mail = compose_host_approval_email(details, confirm_token, base_url, expires);
    send_composed(config, Sender::Instance, to, mail, None).await
}

pub(super) fn compose_host_approval_email(
    details: &BookingDetails,
    confirm_token: Option<&str>,
    base_url: Option<&str>,
    expires: Option<&str>,
) -> ComposedEmail {
    let lang = host_lang(details);
    let (date_display, time_display) = host_time_display_exact(
        &details.date,
        &details.start_time,
        &details.end_time,
        &details.guest_timezone,
        &details.host_timezone,
        details.utc_times.as_ref(),
    );
    let date_display = long_date(lang, &date_display);

    let mut rows = host_rows(
        lang,
        &details.event_title,
        &date_display,
        &time_display,
        &details.guest_name,
        &details.guest_email,
    );
    push_row(
        &mut rows,
        lang,
        "email-detail-phone",
        details.guest_phone.as_ref(),
    );
    push_row(
        &mut rows,
        lang,
        "confirmed-detail-location",
        details.location.as_ref(),
    );
    push_row(
        &mut rows,
        lang,
        "bookings-resource",
        details.resource_name.as_ref(),
    );
    push_row(
        &mut rows,
        lang,
        "confirmed-detail-notes",
        details.notes.as_ref(),
    );
    if let Some(deadline) = expires {
        rows.push(row(t(lang, "email-detail-expires"), deadline));
    }

    let (plain_lines, actions) = approval_actions(lang, approval_urls(confirm_token, base_url));

    let guest = details.guest_name.as_str();
    let (heading, message, subject) = match expires {
        None => (
            t(lang, "email-host-approval-heading"),
            ta(lang, "email-host-approval-message", [("guest", guest)]),
            ta(
                lang,
                "email-host-approval-subject",
                [
                    ("event", &details.event_title),
                    ("guest", guest),
                    ("date", &date_display),
                ],
            ),
        ),
        Some(deadline) => (
            t(lang, "email-host-approval-reminder-heading"),
            ta(
                lang,
                "email-host-approval-reminder-message",
                [("guest", guest), ("deadline", deadline)],
            ),
            ta(
                lang,
                "email-host-approval-reminder-subject",
                [
                    ("event", &details.event_title),
                    ("guest", guest),
                    ("date", &date_display),
                ],
            ),
        ),
    };

    let footer = t(lang, "email-host-approval-footer");
    EmailParts {
        lang,
        subject,
        heading,
        message,
        accent: "#f59e0b",
        rows,
        plain_lines,
        footer_plain: Some(footer.clone()),
        footer_html: Some(footer),
        actions,
    }
    .compose()
}

/// Tell the host a request they did not answer has expired and was declined
/// automatically (#234), in the host's language.
pub async fn send_host_pending_expired(
    config: &SmtpConfig,
    details: &CancellationDetails,
) -> Result<()> {
    let to = named_mailbox(&details.host_name, &details.host_email)?;
    send_composed(
        config,
        Sender::Instance,
        to,
        compose_host_pending_expired(details),
        None,
    )
    .await
}

pub(super) fn compose_host_pending_expired(details: &CancellationDetails) -> ComposedEmail {
    let lang = lang_or_en(details.host_language.as_deref());
    let (date_display, time_display) = host_time_display_exact(
        &details.date,
        &details.start_time,
        &details.end_time,
        &details.guest_timezone,
        &details.host_timezone,
        details.utc_times.as_ref(),
    );
    let date_display = long_date(lang, &date_display);
    let footer = t(lang, "email-host-expired-footer");
    EmailParts {
        lang,
        subject: ta(
            lang,
            "email-host-expired-subject",
            [
                ("event", &details.event_title),
                ("guest", &details.guest_name),
                ("date", &date_display),
            ],
        ),
        heading: t(lang, "email-host-expired-heading"),
        message: ta(
            lang,
            "email-host-expired-message",
            [("guest", &details.guest_name)],
        ),
        accent: "#dc2626",
        rows: host_rows(
            lang,
            &details.event_title,
            &date_display,
            &time_display,
            &details.guest_name,
            &details.guest_email,
        ),
        plain_lines: vec![],
        footer_plain: Some(footer.clone()),
        footer_html: Some(footer),
        actions: vec![],
    }
    .compose()
}

/// Notify the host that a guest wants to reschedule, with approve/decline
/// buttons, in the host's language.
pub async fn send_host_reschedule_request(
    config: &SmtpConfig,
    details: &RescheduleDetails,
    confirm_token: Option<&str>,
    base_url: Option<&str>,
) -> Result<()> {
    let to = named_mailbox(&details.host_name, &details.host_email)?;
    let mail = compose_host_reschedule_request(details, confirm_token, base_url);
    send_composed(config, Sender::Instance, to, mail, None).await
}

pub(super) fn compose_host_reschedule_request(
    details: &RescheduleDetails,
    confirm_token: Option<&str>,
    base_url: Option<&str>,
) -> ComposedEmail {
    let lang = lang_or_en(details.host_language.as_deref());
    let (old_date_display, old_time_display) = host_time_display_exact(
        &details.old_date,
        &details.old_start_time,
        &details.old_end_time,
        &details.guest_timezone,
        &details.host_timezone,
        details.old_utc_times.as_ref(),
    );
    let old_date_display = long_date(lang, &old_date_display);
    let (new_date_display, new_time_display) = host_time_display_exact(
        &details.new_date,
        &details.new_start_time,
        &details.new_end_time,
        &details.guest_timezone,
        &details.host_timezone,
        details.utc_times.as_ref(),
    );
    let new_date_display = long_date(lang, &new_date_display);

    let mut rows = vec![
        row(t(lang, "confirmed-detail-event"), &details.event_title),
        row(
            t(lang, "email-detail-previous"),
            date_at_time(lang, &old_date_display, &old_time_display),
        ),
        row(
            t(lang, "email-detail-requested"),
            date_at_time(lang, &new_date_display, &new_time_display),
        ),
        row(
            t(lang, "common-detail-guest"),
            format!("{} <{}>", details.guest_name, details.guest_email),
        ),
    ];
    push_row(
        &mut rows,
        lang,
        "email-detail-phone",
        details.guest_phone.as_ref(),
    );
    push_row(
        &mut rows,
        lang,
        "confirmed-detail-location",
        details.location.as_ref(),
    );
    push_row(
        &mut rows,
        lang,
        "confirmed-detail-notes",
        details.notes.as_ref(),
    );

    let (plain_lines, actions) = approval_actions(lang, approval_urls(confirm_token, base_url));

    EmailParts {
        lang,
        subject: ta(
            lang,
            "email-host-reschedule-request-subject",
            [
                ("event", &details.event_title),
                ("guest", &details.guest_name),
                ("email", &details.guest_email),
            ],
        ),
        heading: ta(
            lang,
            "email-confirm-greeting",
            [("name", &details.host_name)],
        ),
        message: ta(
            lang,
            "email-host-reschedule-request-message",
            [("guest", &details.guest_name)],
        ),
        accent: "#d97706",
        rows,
        plain_lines,
        footer_plain: None,
        footer_html: None,
        actions,
    }
    .compose()
}

// --- Watcher claim emails ---

/// Offer a booking to a watcher, in the watcher's own language (`lang`, from
/// their `users.language`; `None` is English).
pub async fn send_watcher_claim_notification(
    config: &SmtpConfig,
    details: &BookingDetails,
    watcher_name: &str,
    watcher_email: &str,
    lang: Option<&str>,
    assigned_to_name: &str,
    claim_url: &str,
) -> Result<()> {
    let to = named_mailbox(watcher_name, watcher_email)?;
    let mail = compose_watcher_claim_notification(
        details,
        watcher_name,
        lang,
        assigned_to_name,
        claim_url,
    );
    send_composed(config, Sender::Instance, to, mail, None).await
}

pub(super) fn compose_watcher_claim_notification(
    details: &BookingDetails,
    watcher_name: &str,
    lang: Option<&str>,
    assigned_to_name: &str,
    claim_url: &str,
) -> ComposedEmail {
    let lang = lang_or_en(lang);
    let date = long_date(lang, &details.date);
    let time_display = format!("{} \u{2013} {}", details.start_time, details.end_time);
    let mut rows = host_rows(
        lang,
        &details.event_title,
        &date,
        &time_display,
        &details.guest_name,
        &details.guest_email,
    );
    rows.push(row(t(lang, "claim-assigned-to"), assigned_to_name));
    push_row(
        &mut rows,
        lang,
        "confirmed-detail-location",
        details.location.as_ref(),
    );

    let footer = t(lang, "email-watcher-claim-footer");
    EmailParts {
        lang,
        subject: ta(
            lang,
            "email-watcher-claim-subject",
            [
                ("event", &details.event_title),
                ("guest", &details.guest_name),
                ("date", &date),
            ],
        ),
        heading: ta(lang, "email-confirm-greeting", [("name", watcher_name)]),
        message: t(lang, "email-watcher-claim-message"),
        accent: "#3b82f6",
        rows,
        plain_lines: vec![ta(lang, "email-watcher-claim-link", [("url", claim_url)])],
        footer_plain: Some(footer.clone()),
        footer_html: Some(footer),
        actions: vec![EmailAction {
            label: t(lang, "claim-button"),
            url: claim_url.to_string(),
            color: "#3b82f6".to_string(),
        }],
    }
    .compose()
}

/// Confirm a claim to the watcher who made it, in their language (`lang`,
/// from their `users.language`; `None` is English).
pub async fn send_claim_confirmation(
    config: &SmtpConfig,
    details: &BookingDetails,
    claimant_name: &str,
    claimant_email: &str,
    lang: Option<&str>,
) -> Result<()> {
    let to = named_mailbox(claimant_name, claimant_email)?;
    let mail = compose_claim_confirmation(details, claimant_name, lang);
    send_composed(config, Sender::Instance, to, mail, None).await
}

pub(super) fn compose_claim_confirmation(
    details: &BookingDetails,
    claimant_name: &str,
    lang: Option<&str>,
) -> ComposedEmail {
    let lang = lang_or_en(lang);
    let date = long_date(lang, &details.date);
    let time_display = format!("{} \u{2013} {}", details.start_time, details.end_time);
    let mut rows = host_rows(
        lang,
        &details.event_title,
        &date,
        &time_display,
        &details.guest_name,
        &details.guest_email,
    );
    push_row(
        &mut rows,
        lang,
        "confirmed-detail-location",
        details.location.as_ref(),
    );

    let footer = t(lang, "email-claim-confirmed-footer");
    EmailParts {
        lang,
        subject: ta(
            lang,
            "email-claim-confirmed-subject",
            [
                ("event", &details.event_title),
                ("guest", &details.guest_name),
                ("date", &date),
            ],
        ),
        heading: ta(lang, "email-confirm-greeting", [("name", claimant_name)]),
        message: t(lang, "claimed-subtitle"),
        accent: "#16a34a",
        rows,
        plain_lines: vec![],
        footer_plain: Some(footer.clone()),
        footer_html: Some(footer),
        actions: vec![],
    }
    .compose()
}
