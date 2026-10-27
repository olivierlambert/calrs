//! Token pages from emails: approve, decline, cancel, reschedule, claim, ICS download.

use super::*;

// --- Token-based approve/decline (from email) ---

#[derive(Deserialize)]
pub(super) struct DeclineForm {
    _csrf: Option<String>,
    reason: Option<String>,
}

/// Language of the host's approve and decline pages: the saved language of
/// the booking's single host (the assigned member, else the event type's
/// owner), in one query. A collective booking has several approvers and the
/// link does not say which one clicked, so it uses the browser's language, as
/// does a host with no saved language.
async fn host_token_page_lang(
    pool: &SqlitePool,
    confirm_token: &str,
    headers: &HeaderMap,
) -> &'static str {
    let language: Option<Option<String>> = sqlx::query_scalar(
        "SELECT CASE
                  WHEN b.assigned_user_id IS NOT NULL THEN au.language
                  WHEN et.scheduling_mode = 'collective' AND et.team_id IS NOT NULL THEN NULL
                  ELSE ou.language
                END
         FROM bookings b
         JOIN event_types et ON et.id = b.event_type_id
         JOIN accounts a ON a.id = et.account_id
         LEFT JOIN users ou ON ou.id = a.user_id
         LEFT JOIN users au ON au.id = b.assigned_user_id
         WHERE b.confirm_token = ?",
    )
    .bind(confirm_token)
    .fetch_optional(pool)
    .await
    .unwrap_or(None);
    language
        .flatten()
        .as_deref()
        .and_then(crate::i18n::supported_code)
        .unwrap_or_else(|| crate::i18n::detect_from_headers(headers))
}

/// Language of the claim pages: the saved language of the watcher the claim
/// token was issued to (the claim email went out in it), else the browser's.
async fn claim_page_lang(
    pool: &SqlitePool,
    token: Option<&str>,
    headers: &HeaderMap,
) -> &'static str {
    if let Some(token) = token {
        let language: Option<Option<String>> = sqlx::query_scalar(
            "SELECT u.language FROM booking_claim_tokens bct JOIN users u ON u.id = bct.user_id
             WHERE bct.token = ?",
        )
        .bind(token)
        .fetch_optional(pool)
        .await
        .unwrap_or(None);
        if let Some(code) = language
            .flatten()
            .as_deref()
            .and_then(crate::i18n::supported_code)
        {
            return code;
        }
    }
    crate::i18n::detect_from_headers(headers)
}

/// Render an error page for token-based actions (shared by approve form and handler).
fn render_token_error(
    state: &AppState,
    lang: &str,
    already: Option<(String,)>,
) -> axum::response::Response {
    let (title_key, message_key) = match already {
        Some((status,)) if status == "confirmed" => {
            ("bae-title-already-approved", "bae-body-already-approved")
        }
        Some((status,)) if status == "declined" => {
            ("bae-title-already-declined", "bae-body-already-declined")
        }
        Some((status,)) if status == "cancelled" => {
            ("bae-title-booking-cancelled", "bae-body-was-cancelled")
        }
        _ => ("bae-title-invalid-link", "bae-body-approval-link-invalid"),
    };
    render_booking_action_error_keys_in(state, lang, title_key, message_key)
}

pub(super) async fn approve_booking_form(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(token): Path<String>,
) -> impl IntoResponse {
    // The host's saved language, as their approval email (#209).
    let lang = host_token_page_lang(&state.pool, &token, &headers).await;
    // Look up pending booking by confirm_token
    let booking: Option<(String, String, String, String, String)> = sqlx::query_as(
        "SELECT b.guest_name, b.guest_email, CASE b.time_version WHEN 1 THEN b.start_at ELSE rtrim(b.start_at, 'Z') END AS start_at, CASE b.time_version WHEN 1 THEN b.end_at ELSE rtrim(b.end_at, 'Z') END AS end_at, et.title
         FROM bookings b
         JOIN event_types et ON et.id = b.event_type_id
         WHERE b.confirm_token = ? AND b.status = 'pending'",
    )
    .bind(&token)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    let (guest_name, guest_email, start_at, end_at, event_title) = match booking {
        Some(b) => b,
        None => {
            let already: Option<(String,)> =
                sqlx::query_as("SELECT status FROM bookings WHERE confirm_token = ?")
                    .bind(&token)
                    .fetch_optional(&state.pool)
                    .await
                    .unwrap_or(None);
            return render_token_error(&state, lang, already);
        }
    };

    let (start_at, end_at) =
        crate::booking_time::guest_wall_strings(&state.pool, &token, &start_at, &end_at).await;
    let date_label = format_date_label(&start_at, lang);
    let start_time = extract_time_24h(&start_at);
    let end_time = extract_time_24h(&end_at);

    let tmpl = match state.templates.get_template("booking_approve_form.html") {
        Ok(t) => t,
        Err(e) => return internal_error_response("internal", &e),
    };
    tmpl.render(context! {
        event_title,
        date_label,
        start_time,
        end_time,
        guest_name,
        guest_email,
        lang => lang,
    })
    .map(|r| Html(r).into_response())
    .unwrap_or_else(|e| internal_error_response("template render", &e))
}

pub(super) async fn approve_booking_by_token(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(token): Path<String>,
) -> impl IntoResponse {
    // The host's saved language, as their approval email (#209).
    let lang = host_token_page_lang(&state.pool, &token, &headers).await;
    // Look up booking by confirm_token
    #[allow(clippy::type_complexity)]
    let booking: Option<(String, String, String, String, String, String, String, String, String, Option<String>, Option<String>, String, Option<String>, String, Option<String>, String)> =
        sqlx::query_as(
            "SELECT b.id, b.uid, b.guest_name, b.guest_email, CASE b.time_version WHEN 1 THEN b.start_at ELSE rtrim(b.start_at, 'Z') END AS start_at, CASE b.time_version WHEN 1 THEN b.end_at ELSE rtrim(b.end_at, 'Z') END AS end_at, et.title, a.user_id, u.name, et.location_value, b.cancel_token, COALESCE(b.guest_timezone, 'UTC'), b.reschedule_token, b.event_type_id, b.guest_phone, et.sms_phone_mode
             FROM bookings b
             JOIN event_types et ON et.id = b.event_type_id
             JOIN accounts a ON a.id = et.account_id
             JOIN users u ON u.id = a.user_id
             WHERE b.confirm_token = ? AND b.status = 'pending'",
        )
        .bind(&token)
        .fetch_optional(&state.pool)
        .await
        .unwrap_or(None);

    let (
        bid,
        uid,
        guest_name,
        guest_email,
        start_at,
        end_at,
        event_title,
        user_id,
        host_name,
        location_value,
        cancel_token,
        guest_timezone,
        reschedule_token,
        event_type_id,
        guest_phone,
        sms_phone_mode,
    ) = match booking {
        Some(b) => b,
        None => {
            let already: Option<(String,)> =
                sqlx::query_as("SELECT status FROM bookings WHERE confirm_token = ?")
                    .bind(&token)
                    .fetch_optional(&state.pool)
                    .await
                    .unwrap_or(None);
            return render_token_error(&state, lang, already);
        }
    };

    // Fetched separately: sqlx tuple FromRow only goes to 16 columns.
    let assigned_user_id: Option<String> =
        sqlx::query_scalar("SELECT assigned_user_id FROM bookings WHERE id = ?")
            .bind(&bid)
            .fetch_optional(&state.pool)
            .await
            .ok()
            .flatten()
            .flatten();
    // Read before the status flip below, which stamps it. A booking that was
    // confirmed before is a guest reschedule awaiting approval: its guest is
    // texted "moved", not "confirmed", and its attendees were already told
    // about the booking (#266, #267).
    let reschedule_approval = was_confirmed_before(&state.pool, &bid).await;

    // Refresh the hosts' calendars before taking the lock below (bounded,
    // see `prepare_approval_hosts`).
    let approval_hosts = prepare_approval_hosts(&state, &uid, &user_id).await;

    // Pending bookings do not block shared resources, so re-verify them at
    // approval time under the process-wide lock taken below; in round-robin
    // mode the resource is re-picked (the original pick may be long stale).
    // The feeds are refreshed first, outside the lock.
    let has_resources = {
        let ids: Vec<String> =
            crate::resources::resources_for_event_type(&state.pool, &event_type_id)
                .await
                .into_iter()
                .map(|r| r.id)
                .collect();
        if ids.is_empty() {
            false
        } else {
            crate::resources::sync_if_stale(&state.pool, &ids).await;
            true
        }
    };

    // Same lock and host re-check as the dashboard approval (#234).
    let booking_guard = crate::resources::booking_lock().await;
    if !approval_hosts_free(
        &state.pool,
        &approval_hosts,
        &bid,
        &uid,
        &event_type_id,
        &start_at,
        &end_at,
    )
    .await
    {
        tracing::warn!(booking_id = %bid, "approval refused: host no longer free");
        return render_booking_action_error_keys_in(
            &state,
            lang,
            "bae-title-cannot-approve",
            "bae-body-host-busy",
        );
    }

    if has_resources {
        if let (Some(s), Some(e)) = (parse_ical_datetime(&start_at), parse_ical_datetime(&end_at)) {
            let tz_check = get_host_tz(&state.pool, &event_type_id).await;
            let (s, e) =
                crate::booking_time::busy_range(&start_at, &end_at, tz_check).unwrap_or((s, e));
            match crate::resources::check_and_pick(
                &state.pool,
                &event_type_id,
                s,
                e,
                tz_check,
                Some(&bid),
            )
            .await
            {
                crate::resources::ResourceCheck::Busy => {
                    tracing::warn!(booking_id = %bid, "approval refused: required resource no longer available");
                    return render_booking_action_error_keys_in(
                        &state,
                        lang,
                        "bae-title-cannot-approve",
                        "bae-body-resource-gone",
                    );
                }
                crate::resources::ResourceCheck::Free { assigned } => {
                    let _ =
                        sqlx::query("UPDATE bookings SET assigned_resource_id = ? WHERE id = ?")
                            .bind(&assigned)
                            .bind(&bid)
                            .execute(&state.pool)
                            .await;
                }
                crate::resources::ResourceCheck::NoResources => {}
            }
        }
    }

    // Confirm the booking, guarded like the dashboard path: a concurrent
    // approval that already flipped it lands on the "already handled" page.
    let flipped =
        sqlx::query("UPDATE bookings SET status = 'confirmed', pending_expires_at = NULL, pending_since = NULL, pending_reminder_sent_at = NULL WHERE id = ? AND status = 'pending'")
            .bind(&bid)
            .execute(&state.pool)
            .await
            .map(|r| r.rows_affected())
            .unwrap_or(0);
    drop(booking_guard);
    if flipped == 0 {
        let already: Option<(String,)> = sqlx::query_as("SELECT status FROM bookings WHERE id = ?")
            .bind(&bid)
            .fetch_optional(&state.pool)
            .await
            .unwrap_or(None);
        return render_token_error(&state, lang, already);
    }

    tracing::info!(booking_id = %bid, "booking approved via token");

    // start_at/end_at are naive in the event type's timezone; convert to the
    // guest's tz so the email body and the ICS attachment match what the
    // guest booked. See #101.
    let stored_tz = get_host_tz(&state.pool, &event_type_id).await;
    let guest_tz_parsed = guest_timezone.parse::<Tz>().unwrap_or(Tz::UTC);
    let (date, start_time, end_time) =
        booking_strings_in_guest_tz(&start_at, &end_at, stored_tz, guest_tz_parsed);
    let date_label = format_date_label(&date, lang);

    // Host side for BookingDetails: resolved from the booking so the ORGANIZER
    // matches the calendar the event is written to. Approving through the email
    // token used to stamp the event type owner even for a booking assigned to
    // another member, which later made Google CalDAV 403 the reschedule PUT.
    // See `booking_host_identity`.
    let (host_name, host_email, host_language) = match booking_host_identity(&state.pool, &uid)
        .await
    {
        Some(identity) => identity,
        None => {
            tracing::warn!(
                uid = %uid,
                user_id = %user_id,
                "could not resolve the booking's host identity; falling back to the event type owner"
            );
            host_contact(&state.pool, &user_id)
                .await
                .unwrap_or((host_name, String::new(), None))
        }
    };

    // Status is now 'confirmed' (we just transitioned it). Generate the
    // auto meeting URL if the event type uses one; otherwise fall back to
    // the static location_value.
    let location_display = resolve_booking_location(
        &state,
        &bid,
        &event_type_id,
        Some(meeting::confirm_host_user_id(
            assigned_user_id.as_deref(),
            user_id.as_str(),
        )),
        location_value.as_deref(),
        &guest_name,
        &guest_email,
        true,
    )
    .await;

    let guest_language = crate::email::stored_booking_language(&state.pool, &bid).await;

    let (guest_notes, guest_phone_shown) = booking_guest_input(&state.pool, &uid).await;
    let details = crate::email::BookingDetails {
        utc_times: crate::booking_time::ics_times(&start_at, &end_at),
        event_title: event_title.clone(),
        date: date.clone(),
        start_time: start_time.clone(),
        end_time: end_time.clone(),
        guest_name: guest_name.clone(),
        guest_email: guest_email.clone(),
        guest_timezone,
        host_name: host_name.clone(),
        host_email,
        uid: uid.clone(),
        notes: guest_notes,
        guest_phone: guest_phone_shown,
        location: location_display,
        reminder_minutes: None,
        additional_attendees: booking_additional_attendees(&state.pool, &uid).await,
        // The guest booked in their own language; the host approving here may
        // well be browsing in another one, so this comes from the booking row
        // rather than from the approver's Accept-Language.
        guest_language,
        host_language,
        host_timezone: stored_tz.name().to_string(),
        resource_name: booking_resource_label(&state.pool, &uid).await,
        sequence: booking_ics_sequence(&state.pool, &uid).await,
        attendee_languages: booking_attendee_languages(&state.pool, &uid).await,
    };

    // Push to CalDAV calendar
    caldav_push_booking(&state.pool, &state.secret_key, &user_id, &uid, &details).await;
    resource_push_booking(&state.pool, &state.secret_key, &uid, &details).await;

    // Notify watcher teams. An approved reschedule was offered to them when
    // it was first confirmed: no second offer.
    if !reschedule_approval {
        notify_watchers(
            &state.pool,
            &state.secret_key,
            &bid,
            &event_type_id,
            &host_name,
            &details,
        )
        .await;
    }

    // Send confirmation email to guest
    if let Ok(Some(smtp_config)) =
        crate::email::load_smtp_config(&state.pool, &state.secret_key).await
    {
        let base_url = crate::settings::base_url();
        let guest_cancel_url = cancel_token.as_ref().and_then(|t| {
            base_url
                .as_ref()
                .map(|base| format!("{}/booking/cancel/{}", base.trim_end_matches('/'), t))
        });
        let guest_reschedule_url = reschedule_token.as_ref().and_then(|t| {
            base_url
                .as_ref()
                .map(|base| format!("{}/booking/reschedule/{}", base.trim_end_matches('/'), t))
        });
        let (cancel_notice_min, reschedule_notice_min) =
            fetch_event_type_notice_minutes(&state.pool, &event_type_id).await;
        let details = details.clone();
        spawn_emails(async move {
            send_approval_guest_emails(
                &smtp_config,
                &details,
                guest_cancel_url.as_deref(),
                guest_reschedule_url.as_deref(),
                (cancel_notice_min, reschedule_notice_min),
                reschedule_approval,
            )
            .await;

            // Also send host a confirmation email (no ICS, event pushed via CalDAV)
            log_email_result(
                crate::email::send_host_booking_confirmed(&smtp_config, &details).await,
                "host_booking_confirmed",
                &details.uid,
            );
        });
    }

    // Confirmation SMS to the guest, now that the booking is confirmed. This
    // is the deferred half of the booking flow: a pending booking stays silent
    // until the host approves it here. An approved reschedule says "moved".
    if let Some(ctx) = crate::sms::SmsContext::for_booking(
        &details,
        guest_phone.as_deref(),
        crate::sms::collects_phone(&sms_phone_mode),
    ) {
        crate::sms::notify_guest(
            &state.pool,
            &state.secret_key,
            if reschedule_approval {
                crate::sms::SmsEvent::Rescheduled
            } else {
                crate::sms::SmsEvent::Confirmed
            },
            ctx,
        )
        .await;
    }

    let tmpl = match state.templates.get_template("booking_approved.html") {
        Ok(t) => t,
        Err(e) => return internal_error_response("internal", &e),
    };
    let rendered = tmpl
        .render(context! {
            event_title,
            date_label,
            date,
            start_time,
            end_time,
            guest_name,
            guest_email,
            lang => lang,
        })
        .unwrap_or_else(|e| internal_error_body("template render", &e));

    Html(rendered).into_response()
}

pub(super) async fn decline_booking_form(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(token): Path<String>,
) -> impl IntoResponse {
    // The host's saved language, as their approval email (#209).
    let lang = host_token_page_lang(&state.pool, &token, &headers).await;
    let booking: Option<(String, String, String, String, String)> = sqlx::query_as(
        "SELECT b.guest_name, b.guest_email, CASE b.time_version WHEN 1 THEN b.start_at ELSE rtrim(b.start_at, 'Z') END AS start_at, CASE b.time_version WHEN 1 THEN b.end_at ELSE rtrim(b.end_at, 'Z') END AS end_at, et.title
             FROM bookings b
             JOIN event_types et ON et.id = b.event_type_id
             WHERE b.confirm_token = ? AND b.status = 'pending'",
    )
    .bind(&token)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    let (guest_name, guest_email, start_at, end_at, event_title) = match booking {
        Some(b) => b,
        None => {
            return render_booking_action_error_keys_in(
                &state,
                lang,
                "bae-title-invalid-link",
                "bae-body-decline-link-invalid",
            );
        }
    };

    let (start_at, end_at) =
        crate::booking_time::guest_wall_strings(&state.pool, &token, &start_at, &end_at).await;
    let date_label = format_date_label(&start_at, lang);
    let date = start_at.get(..10).unwrap_or(&start_at).to_string();
    let start_time = extract_time_24h(&start_at);
    let end_time = extract_time_24h(&end_at);

    let tmpl = match state.templates.get_template("booking_decline_form.html") {
        Ok(t) => t,
        Err(e) => return internal_error_response("internal", &e),
    };
    let rendered = tmpl
        .render(context! {
            event_title,
            date_label,
            date,
            start_time,
            end_time,
            guest_name,
            guest_email,
            lang => lang,
        })
        .unwrap_or_else(|e| internal_error_body("template render", &e));

    Html(rendered).into_response()
}

pub(super) async fn decline_booking_by_token(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(token): Path<String>,
    Form(form): Form<DeclineForm>,
) -> impl IntoResponse {
    if let Err(resp) = verify_csrf_token(&headers, &form._csrf) {
        return resp;
    }
    // The host's saved language, as their approval email (#209).
    let lang = host_token_page_lang(&state.pool, &token, &headers).await;
    let booking: Option<(
        String,
        String,
        String,
        String,
        String,
        String,
        String,
        String,
        String,
        String,
    )> = sqlx::query_as(
        "SELECT b.id, b.guest_name, b.guest_email, CASE b.time_version WHEN 1 THEN b.start_at ELSE rtrim(b.start_at, 'Z') END AS start_at, CASE b.time_version WHEN 1 THEN b.end_at ELSE rtrim(b.end_at, 'Z') END AS end_at, et.title, u.name, COALESCE(u.booking_email, u.email), COALESCE(b.guest_timezone, 'UTC'), et.id
             FROM bookings b
             JOIN event_types et ON et.id = b.event_type_id
             JOIN accounts a ON a.id = et.account_id
             JOIN users u ON u.id = COALESCE(b.assigned_user_id, a.user_id)
             WHERE b.confirm_token = ? AND b.status = 'pending'",
    )
    .bind(&token)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    let (
        bid,
        guest_name,
        guest_email,
        start_at,
        end_at,
        event_title,
        host_name,
        host_email,
        guest_timezone,
        et_id,
    ) = match booking {
        Some(b) => b,
        None => {
            return render_booking_action_error_keys_in(
                &state,
                lang,
                "bae-title-invalid-link",
                "bae-body-decline-link-invalid",
            );
        }
    };

    // Decline the booking, guarded like approval: if a concurrent approve or
    // decline got there first, say so instead of emailing the guest again.
    let changed =
        sqlx::query("UPDATE bookings SET status = 'declined', pending_expires_at = NULL, pending_since = NULL, pending_reminder_sent_at = NULL WHERE id = ? AND status = 'pending'")
            .bind(&bid)
            .execute(&state.pool)
            .await
            .map(|r| r.rows_affected())
            .unwrap_or(0);
    if changed == 0 {
        let already: Option<(String,)> = sqlx::query_as("SELECT status FROM bookings WHERE id = ?")
            .bind(&bid)
            .fetch_optional(&state.pool)
            .await
            .unwrap_or(None);
        return render_token_error(&state, lang, already);
    }

    tracing::info!(booking_id = %bid, "booking declined via token");

    // Convert event-type-local stored times into the guest's tz; see #101.
    let stored_tz = get_host_tz(&state.pool, &et_id).await;
    let guest_tz_parsed = guest_timezone.parse::<Tz>().unwrap_or(Tz::UTC);
    let (date, start_time, end_time) =
        booking_strings_in_guest_tz(&start_at, &end_at, stored_tz, guest_tz_parsed);
    let date_label = format_date_label(&date, lang);

    let reason = form.reason.filter(|r| !r.trim().is_empty());

    // Send decline notification to guest
    if let Ok(Some(smtp_config)) =
        crate::email::load_smtp_config(&state.pool, &state.secret_key).await
    {
        // The guest's email names the host they booked with, which is also
        // its sender and Reply-To (#221). The query above (assigned member,
        // else owner) is only the fallback.
        let booking_uid: Option<String> =
            sqlx::query_scalar("SELECT uid FROM bookings WHERE id = ?")
                .bind(&bid)
                .fetch_optional(&state.pool)
                .await
                .unwrap_or(None);
        let identity = match &booking_uid {
            Some(uid) => booking_host_identity(&state.pool, uid).await,
            None => None,
        };
        let (guest_host_name, guest_host_email) = match identity {
            Some((name, email, _)) => (name, email),
            None => (host_name.clone(), host_email),
        };
        let details = crate::email::CancellationDetails {
            utc_times: crate::booking_time::ics_times(&start_at, &end_at),
            event_title: event_title.clone(),
            date: date.clone(),
            start_time: start_time.clone(),
            end_time: end_time.clone(),
            guest_name: guest_name.clone(),
            guest_email: guest_email.clone(),
            guest_timezone: guest_timezone.clone(),
            host_name: guest_host_name,
            host_email: guest_host_email,
            uid: booking_uid.clone().unwrap_or_default(),
            reason: reason.clone(),
            cancelled_by_host: true,
            // `lang` above is the host's (they clicked the link); the guest
            // reads this email in the language they booked in (#209).
            guest_language: crate::email::stored_booking_language(&state.pool, &bid).await,
            host_timezone: stored_tz.name().to_string(),
            // A declined reschedule ends a booking whose invite the guest
            // and attendees hold: its CANCEL must match that copy (#267).
            sequence: match &booking_uid {
                Some(uid) => booking_ics_sequence(&state.pool, uid).await,
                None => 0,
            },
            ..Default::default()
        };
        let details = with_invite_holders(&state.pool, details, true).await;
        let bid = bid.clone();
        spawn_emails(async move {
            log_email_result(
                crate::email::send_guest_decline_notice(&smtp_config, &details).await,
                "guest_decline_notice",
                &bid,
            );
            log_email_result(
                crate::email::send_guest_attendee_cancellations(&smtp_config, &details).await,
                "attendee_cancellation",
                &bid,
            );
        });
    }

    let tmpl = match state.templates.get_template("booking_declined.html") {
        Ok(t) => t,
        Err(e) => return internal_error_response("internal", &e),
    };
    let rendered = tmpl
        .render(context! {
            event_title,
            date_label,
            date,
            start_time,
            end_time,
            guest_name,
            guest_email,
            reason,
            lang => lang,
        })
        .unwrap_or_else(|e| internal_error_body("template render", &e));

    Html(rendered).into_response()
}

// --- Guest cancel booking by token ---

/// Fetch `(cancel_notice_min, reschedule_notice_min)` for an event type by
/// id. Either column may be NULL (no restriction).
pub(super) async fn fetch_event_type_notice_minutes(
    pool: &SqlitePool,
    event_type_id: &str,
) -> (Option<i32>, Option<i32>) {
    sqlx::query_as::<_, (Option<i32>, Option<i32>)>(
        "SELECT cancel_notice_min, reschedule_notice_min FROM event_types WHERE id = ?",
    )
    .bind(event_type_id)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten()
    .unwrap_or((None, None))
}

/// Look up a notice-window column on the booking's event type. Returns
/// `(notice_min, host_email)` where `notice_min` is `None`/`Some(0)` when
/// no restriction applies. The `column` argument MUST be one of the two
/// known column names — it is interpolated into SQL but never sourced from
/// the request, so this is safe.
async fn fetch_notice_min_and_host_email(
    pool: &SqlitePool,
    booking_token_column: &str,
    token: &str,
    notice_column: &str,
) -> Option<(Option<i32>, String)> {
    debug_assert!(matches!(
        notice_column,
        "cancel_notice_min" | "reschedule_notice_min"
    ));
    debug_assert!(matches!(
        booking_token_column,
        "cancel_token" | "reschedule_token"
    ));
    let sql = format!(
        "SELECT et.{notice}, COALESCE(u.booking_email, u.email) \
         FROM bookings b \
         JOIN event_types et ON et.id = b.event_type_id \
         JOIN accounts a ON a.id = et.account_id \
         JOIN users u ON u.id = COALESCE(b.assigned_user_id, a.user_id) \
         WHERE b.{tok} = ?",
        notice = notice_column,
        tok = booking_token_column
    );
    sqlx::query_as::<_, (Option<i32>, String)>(&sql)
        .bind(token)
        .fetch_optional(pool)
        .await
        .unwrap_or(None)
}

/// If `notice_min` is set and the booking starts inside the notice window,
/// render the blocked page and return it. Otherwise return `None` so the
/// caller can proceed.
///
/// UTC bookings use absolute deadlines. Legacy timestamps retain their former
/// server-local interpretation; upgrading does not reinterpret old bookings.
fn check_notice_window(
    state: &AppState,
    notice_min: Option<i32>,
    start_at: &str,
    host_email: &str,
    action: &'static str,
    lang: &'static str,
) -> Option<Response> {
    let n = notice_min.unwrap_or(0);
    if n <= 0 {
        return None;
    }
    let (start, now) = if let Ok(utc) = chrono::DateTime::parse_from_rfc3339(start_at) {
        (utc.naive_utc(), Utc::now().naive_utc())
    } else {
        let start = match NaiveDateTime::parse_from_str(start_at, "%Y-%m-%dT%H:%M:%S") {
            Ok(dt) => dt,
            Err(_) => return None,
        };
        (start, Local::now().naive_local())
    };
    let cutoff = start - chrono::Duration::minutes(n as i64);
    if now <= cutoff {
        return None;
    }
    let tmpl = match state.templates.get_template("booking_action_blocked.html") {
        Ok(t) => t,
        Err(e) => return Some(internal_error_response("internal", &e)),
    };
    let rendered = tmpl
        .render(context! {
            notice_min => n,
            host_email => host_email,
            action => action,
            lang => lang,
        })
        .unwrap_or_else(|e| internal_error_body("template render", &e));
    Some(Html(rendered).into_response())
}

/// Serve the booking as an iCalendar file, addressed by the guest's own cancel
/// token.
///
/// The confirmation email already attaches one, but the guest is looking at the
/// confirmation *page* at the moment they want to add it, and an instance with
/// no SMTP configured never sends that email at all.
pub(super) async fn booking_ics(
    State(state): State<Arc<AppState>>,
    Path(token): Path<String>,
) -> axum::response::Response {
    #[allow(clippy::type_complexity)]
    let booking: Option<(
        String,
        String,
        String,
        String,
        String,
        String,
        String,
        String,
        String,
        Option<String>,
        String,
        Option<String>,
    )> = sqlx::query_as(
        "SELECT b.uid, b.guest_name, b.guest_email, COALESCE(b.guest_timezone, 'UTC'), \
                CASE b.time_version WHEN 1 THEN b.start_at ELSE rtrim(b.start_at, 'Z') END AS start_at, CASE b.time_version WHEN 1 THEN b.end_at ELSE rtrim(b.end_at, 'Z') END AS end_at, et.title, u.name, COALESCE(u.booking_email, u.email), \
                COALESCE(NULLIF(b.meeting_url, ''), et.location_value), et.id, b.notes
             FROM bookings b
             JOIN event_types et ON et.id = b.event_type_id
             JOIN accounts a ON a.id = et.account_id
             JOIN users u ON u.id = COALESCE(b.assigned_user_id, a.user_id)
             WHERE b.cancel_token = ? AND b.status IN ('confirmed', 'pending')",
    )
    .bind(&token)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    let Some((
        uid,
        guest_name,
        guest_email,
        guest_timezone,
        start_at,
        end_at,
        event_title,
        host_name,
        host_email,
        location,
        et_id,
        notes,
    )) = booking
    else {
        return (
            axum::http::StatusCode::NOT_FOUND,
            "This booking is no longer available.",
        )
            .into_response();
    };

    // start_at/end_at are stored in the event type's tz; the ICS carries
    // guest-local wall-clock like every other consumer. See #101.
    let stored_tz = get_host_tz(&state.pool, &et_id).await;
    let guest_tz_parsed = guest_timezone.parse::<Tz>().unwrap_or(Tz::UTC);
    let (date, start_time, end_time) =
        booking_strings_in_guest_tz(&start_at, &end_at, stored_tz, guest_tz_parsed);

    let details = crate::email::BookingDetails {
        utc_times: crate::booking_time::ics_times(&start_at, &end_at),
        event_title,
        date,
        start_time,
        end_time,
        guest_name,
        guest_email,
        guest_timezone,
        host_name,
        host_email,
        uid: uid.clone(),
        notes,
        location: location.filter(|l| !l.is_empty()),
        host_timezone: stored_tz.name().to_string(),
        sequence: booking_ics_sequence(&state.pool, &uid).await,
        ..Default::default()
    };

    let ics = crate::email::generate_ics(&details, "PUBLISH");
    (
        [
            (
                axum::http::header::CONTENT_TYPE,
                "text/calendar; charset=UTF-8".to_string(),
            ),
            (
                axum::http::header::CONTENT_DISPOSITION,
                "attachment; filename=\"booking.ics\"".to_string(),
            ),
        ],
        ics,
    )
        .into_response()
}

/// Language of a guest token page (cancel, reschedule, pick a new time): the
/// language stored with the booking, so the page agrees with the emails the
/// guest gets about it (#209); the visitor rule for bookings made before the
/// column existed, or when the token matches no booking.
fn token_page_lang(stored: Option<&str>, visitor: &'static str) -> &'static str {
    stored
        .and_then(crate::i18n::supported_code)
        .unwrap_or(visitor)
}

pub(super) async fn guest_cancel_form(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(token): Path<String>,
) -> impl IntoResponse {
    let visitor = crate::i18n::detect_from_headers(&headers);
    let booking: Option<(String, String, String, String, String, String, Option<String>)> = sqlx::query_as(
        "SELECT b.guest_name, b.guest_email, CASE b.time_version WHEN 1 THEN b.start_at ELSE rtrim(b.start_at, 'Z') END AS start_at, CASE b.time_version WHEN 1 THEN b.end_at ELSE rtrim(b.end_at, 'Z') END AS end_at, et.title, u.name, b.language
             FROM bookings b
             JOIN event_types et ON et.id = b.event_type_id
             JOIN accounts a ON a.id = et.account_id
             JOIN users u ON u.id = COALESCE(b.assigned_user_id, a.user_id)
             WHERE b.cancel_token = ? AND b.status IN ('confirmed', 'pending')",
    )
    .bind(&token)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    let (guest_name, _guest_email, start_at, end_at, event_title, host_name, booking_language) =
        match booking {
            Some(b) => b,
            None => {
                // Check if already cancelled
                let status_row: Option<(String, Option<String>)> =
                    sqlx::query_as("SELECT status, language FROM bookings WHERE cancel_token = ?")
                        .bind(&token)
                        .fetch_optional(&state.pool)
                        .await
                        .unwrap_or(None);

                let lang =
                    token_page_lang(status_row.as_ref().and_then(|(_, l)| l.as_deref()), visitor);
                let (title_key, message_key) = match status_row {
                    Some((status, _)) if status == "cancelled" => {
                        ("bae-title-already-cancelled", "bae-body-already-cancelled")
                    }
                    Some((status, _)) if status == "declined" => {
                        ("bae-title-booking-declined", "bae-body-declined-by-host")
                    }
                    _ => (
                        "bae-title-invalid-link",
                        "bae-body-cancel-link-invalid-short",
                    ),
                };

                return render_booking_action_error_keys_in(&state, lang, title_key, message_key);
            }
        };

    let lang = token_page_lang(booking_language.as_deref(), visitor);

    // Block the GET form before any state mutation if the booking falls
    // inside the host's configured cancel-notice window.
    if let Some((notice_min, host_email)) =
        fetch_notice_min_and_host_email(&state.pool, "cancel_token", &token, "cancel_notice_min")
            .await
    {
        if let Some(resp) =
            check_notice_window(&state, notice_min, &start_at, &host_email, "cancel", lang)
        {
            tracing::info!(action = "cancel", "guest action blocked by notice window");
            return resp;
        }
    }

    let (start_at, end_at) =
        crate::booking_time::guest_wall_strings(&state.pool, &token, &start_at, &end_at).await;
    let date_label = format_date_label(&start_at, lang);
    let date = start_at.get(..10).unwrap_or(&start_at).to_string();
    let start_time = extract_time_24h(&start_at);
    let end_time = extract_time_24h(&end_at);

    let tmpl = match state.templates.get_template("booking_cancel_form.html") {
        Ok(t) => t,
        Err(e) => return internal_error_response("internal", &e),
    };
    let rendered = tmpl
        .render(context! {
            event_title,
            date_label,
            date,
            start_time,
            end_time,
            guest_name,
            host_name,
            lang => lang,
        })
        .unwrap_or_else(|e| internal_error_body("template render", &e));

    Html(rendered).into_response()
}

/// Cancel a confirmed or pending booking and return the status it was
/// cancelled from, or `None` when it was neither any more (already
/// cancelled or declined). `observed` is the status read earlier: the
/// UPDATE is guarded on it, so the return value is the state the booking
/// actually left, even if an approval or a guest reschedule landed in
/// between (#267). Whether the cancellation ends a reschedule awaiting
/// approval depends on it.
pub(super) async fn cancel_from_status(
    pool: &SqlitePool,
    booking_id: &str,
    observed: &str,
) -> Option<String> {
    let mut expected = observed.to_string();
    // Each retry follows a real transition, and a booking only moves
    // between confirmed and pending this way, so a few attempts suffice.
    for _ in 0..3 {
        // Ending a confirmed booking bumps its SEQUENCE, so the CANCEL
        // outranks the copy its holders have (RFC 5546, migration 072).
        let changed = sqlx::query(
            "UPDATE bookings SET status = 'cancelled', pending_expires_at = NULL, pending_since = NULL, pending_reminder_sent_at = NULL, ics_sequence = CASE status WHEN 'confirmed' THEN ics_sequence + 1 ELSE ics_sequence END WHERE id = ? AND status = ?",
        )
        .bind(booking_id)
        .bind(&expected)
        .execute(pool)
        .await
        .map(|r| r.rows_affected())
        .unwrap_or(0);
        if changed == 1 {
            return Some(expected);
        }
        let now: Option<String> = sqlx::query_scalar("SELECT status FROM bookings WHERE id = ?")
            .bind(booking_id)
            .fetch_optional(pool)
            .await
            .unwrap_or(None);
        match now {
            Some(status) if status == "confirmed" || status == "pending" => expected = status,
            _ => return None,
        }
    }
    None
}

pub(super) async fn guest_cancel_booking(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(token): Path<String>,
    Form(form): Form<CancelForm>,
) -> impl IntoResponse {
    if let Err(resp) = verify_csrf_token(&headers, &form._csrf) {
        return resp;
    }
    #[allow(clippy::type_complexity)]
    let booking: Option<(String, String, String, String, String, String, String, String, String, String, String, Option<String>, String, Option<String>, Option<String>, String)> =
        sqlx::query_as(
            "SELECT b.id, b.uid, b.guest_name, b.guest_email, CASE b.time_version WHEN 1 THEN b.start_at ELSE rtrim(b.start_at, 'Z') END AS start_at, CASE b.time_version WHEN 1 THEN b.end_at ELSE rtrim(b.end_at, 'Z') END AS end_at, et.title, u.name, COALESCE(u.booking_email, u.email), COALESCE(b.guest_timezone, 'UTC'), et.id, b.guest_phone, et.sms_phone_mode, u.language, b.language, b.status
             FROM bookings b
             JOIN event_types et ON et.id = b.event_type_id
             JOIN accounts a ON a.id = et.account_id
             JOIN users u ON u.id = COALESCE(b.assigned_user_id, a.user_id)
             WHERE b.cancel_token = ? AND b.status IN ('confirmed', 'pending')",
        )
        .bind(&token)
        .fetch_optional(&state.pool)
        .await
        .unwrap_or(None);

    let (
        bid,
        uid,
        guest_name,
        guest_email,
        start_at,
        end_at,
        event_title,
        host_name,
        host_email,
        guest_timezone,
        et_id,
        guest_phone,
        sms_phone_mode,
        host_language,
        booking_language,
        observed_status,
    ) = match booking {
        Some(b) => b,
        None => {
            return render_booking_action_error_keys_in(
                &state,
                crate::i18n::detect_from_headers(&headers),
                "bae-title-invalid-link",
                "bae-body-cancel-link-invalid",
            );
        }
    };

    // The page and the emails both use the language the guest booked in.
    let lang = token_page_lang(
        booking_language.as_deref(),
        crate::i18n::detect_from_headers(&headers),
    );

    // Defensive notice-window check on POST too: the GET may have been
    // cached, or the cutoff may have rolled over between page load and
    // submit. Either way, never mutate state inside the blocked window.
    if let Some((notice_min, _)) =
        fetch_notice_min_and_host_email(&state.pool, "cancel_token", &token, "cancel_notice_min")
            .await
    {
        if let Some(resp) =
            check_notice_window(&state, notice_min, &start_at, &host_email, "cancel", lang)
        {
            tracing::info!(
                action = "cancel",
                booking_id = %bid,
                "guest action blocked by notice window (POST)"
            );
            return resp;
        }
    }

    // Cancel the booking. Guarded so a double submit, or a host decline or
    // cancel in between, does not notify everyone twice.
    let prev_status = match cancel_from_status(&state.pool, &bid, &observed_status).await {
        Some(status) => status,
        None => {
            // Lost the race: report what actually happened, as the GET form does.
            let status: Option<(String,)> =
                sqlx::query_as("SELECT status FROM bookings WHERE id = ?")
                    .bind(&bid)
                    .fetch_optional(&state.pool)
                    .await
                    .unwrap_or(None);
            let (title_key, message_key) = match status {
                Some((status,)) if status == "declined" => {
                    ("bae-title-booking-declined", "bae-body-declined-by-host")
                }
                _ => ("bae-title-already-cancelled", "bae-body-already-cancelled"),
            };
            return render_booking_action_error_keys_in(&state, lang, title_key, message_key);
        }
    };

    tracing::info!(booking_id = %bid, "booking cancelled by guest");

    // Find the host user_id for CalDAV delete
    let host_user_id: Option<String> = sqlx::query_scalar(
        "SELECT a.user_id FROM accounts a JOIN event_types et ON et.account_id = a.id JOIN bookings b ON b.event_type_id = et.id WHERE b.id = ?",
    )
    .bind(&bid)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    // Delete from CalDAV calendar
    if let Some(user_id) = &host_user_id {
        caldav_delete_booking(&state.pool, &state.secret_key, user_id, &uid).await;
        resource_delete_booking(&state.pool, &state.secret_key, &uid).await;
    }

    // Convert event-type-local stored times into the guest's tz; see #101.
    let stored_tz = get_host_tz(&state.pool, &et_id).await;
    let guest_tz_parsed = guest_timezone.parse::<Tz>().unwrap_or(Tz::UTC);
    let (date, start_time, end_time) =
        booking_strings_in_guest_tz(&start_at, &end_at, stored_tz, guest_tz_parsed);
    let date_label = format_date_label(&date, lang);

    let reason = form.reason.filter(|r| !r.trim().is_empty());

    // The language the guest booked in, as every other guest email uses,
    // not the browser this cancel happens to come from (#209): the page's.
    let guest_language = Some(lang.to_string());

    let sequence = booking_ics_sequence(&state.pool, &uid).await;
    // Built before the SMTP block so the SMS path can borrow the same strings.
    let details = crate::email::CancellationDetails {
        utc_times: crate::booking_time::ics_times(&start_at, &end_at),
        event_title: event_title.clone(),
        date: date.clone(),
        start_time: start_time.clone(),
        end_time: end_time.clone(),
        guest_name: guest_name.clone(),
        guest_email: guest_email.clone(),
        guest_timezone,
        host_name: host_name.clone(),
        host_email,
        uid,
        reason: reason.clone(),
        cancelled_by_host: false,
        guest_language,
        host_language: crate::email::supported_language(host_language),
        host_timezone: stored_tz.name().to_string(),
        sequence,
        ..Default::default()
    };
    // Its attendees hold the invite if it was ever confirmed, even when this
    // cancels a reschedule still awaiting approval (#267).
    let details = with_invite_holders(&state.pool, details, prev_status == "pending").await;

    // Send cancellation emails
    if let Ok(Some(smtp_config)) =
        crate::email::load_smtp_config(&state.pool, &state.secret_key).await
    {
        let details = details.clone();
        // The host notice goes to whoever hosts the booking, each in their
        // own language: the assigned member, every eligible member of a
        // collective booking (as for the new-booking and expiry notices),
        // else the event type owner. The query above (assigned member, else
        // owner) is the fallback when none resolves. The guest's copy names
        // the host they booked with (assigned member, or the stored roster
        // and ICS organizer for a collective booking, #161), which is also
        // its sender and Reply-To (#221).
        let mut host_notices = Vec::new();
        if let Some(owner) = &host_user_id {
            for (name, email, language) in
                pending_host_recipients(&state.pool, &details.uid, owner).await
            {
                host_notices.push(crate::email::CancellationDetails {
                    host_name: name,
                    host_email: email,
                    host_language: language,
                    ..details.clone()
                });
            }
        }
        if host_notices.is_empty() {
            host_notices.push(details.clone());
        }
        let guest_details = match booking_host_identity(&state.pool, &details.uid).await {
            Some((host_name, host_email, _)) => crate::email::CancellationDetails {
                host_name,
                host_email,
                ..details.clone()
            },
            None => details.clone(),
        };
        spawn_emails(async move {
            log_email_result(
                crate::email::send_guest_cancellation(&smtp_config, &guest_details).await,
                "guest_cancellation",
                &details.uid,
            );
            log_email_result(
                crate::email::send_guest_attendee_cancellations(&smtp_config, &guest_details).await,
                "attendee_cancellation",
                &details.uid,
            );
            for host_details in &host_notices {
                log_email_result(
                    crate::email::send_host_cancellation(&smtp_config, host_details).await,
                    "host_cancellation",
                    &details.uid,
                );
            }
        });
    }

    // Cancellation SMS to the guest, independent of SMTP.
    if let Some(ctx) = crate::sms::SmsContext::for_cancellation(
        &details,
        guest_phone.as_deref(),
        crate::sms::collects_phone(&sms_phone_mode),
    ) {
        crate::sms::notify_guest(
            &state.pool,
            &state.secret_key,
            crate::sms::SmsEvent::Cancelled,
            ctx,
        )
        .await;
    }

    let tmpl = match state.templates.get_template("booking_cancelled_guest.html") {
        Ok(t) => t,
        Err(e) => return internal_error_response("internal", &e),
    };
    let rendered = tmpl
        .render(context! {
            event_title,
            date_label,
            date,
            start_time,
            end_time,
            host_name,
            reason,
            lang => lang,
        })
        .unwrap_or_else(|e| internal_error_body("template render", &e));

    Html(rendered).into_response()
}

// --- Reschedule handlers ---

#[derive(Deserialize)]
pub(super) struct RescheduleQuery {
    #[serde(default)]
    date: Option<String>,
    #[serde(default)]
    time: Option<String>,
    #[serde(default)]
    tz: Option<String>,
    #[serde(default)]
    month: Option<String>,
}

#[derive(Deserialize)]
pub(super) struct RescheduleForm {
    _csrf: Option<String>,
    date: String,
    time: String,
    #[serde(default)]
    tz: Option<String>,
}

/// Guest reschedule: show slot picker or confirmation page
pub(super) async fn guest_reschedule_slots(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(token): Path<String>,
    Query(query): Query<RescheduleQuery>,
) -> impl IntoResponse {
    let visitor = crate::i18n::detect_from_headers(&headers);
    // Look up booking by reschedule_token
    let booking: Option<(String, String, String, String, String, String, Option<String>)> = sqlx::query_as(
        "SELECT b.id, b.guest_name, CASE b.time_version WHEN 1 THEN b.start_at ELSE rtrim(b.start_at, 'Z') END AS start_at, CASE b.time_version WHEN 1 THEN b.end_at ELSE rtrim(b.end_at, 'Z') END AS end_at, b.event_type_id, b.uid, b.language
             FROM bookings b
             WHERE b.reschedule_token = ? AND b.status IN ('confirmed', 'pending')",
    )
    .bind(&token)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    let (booking_id, _guest_name, start_at, end_at, et_id_raw, uid, booking_language) =
        match booking {
            Some(b) => b,
            None => {
                return render_booking_action_error_keys_in(
                    &state,
                    visitor,
                    "bae-title-invalid-link",
                    "bae-body-reschedule-link-invalid",
                );
            }
        };

    let lang = token_page_lang(booking_language.as_deref(), visitor);

    // Block the slot picker before any costly work (CalDAV sync, slot
    // computation, template render) if the booking falls inside the host's
    // configured reschedule-notice window.
    if let Some((notice_min, host_email)) = fetch_notice_min_and_host_email(
        &state.pool,
        "reschedule_token",
        &token,
        "reschedule_notice_min",
    )
    .await
    {
        if let Some(resp) = check_notice_window(
            &state,
            notice_min,
            &start_at,
            &host_email,
            "reschedule",
            lang,
        ) {
            tracing::info!(
                action = "reschedule",
                booking_id = %booking_id,
                "guest action blocked by notice window"
            );
            return resp;
        }
    }

    // Fetch event type + owner. The owner is only the fallback host here: which
    // calendars actually gate this booking, and whose profile the guest sees,
    // is resolved from the booking below (#166).
    let et_info: Option<(
        String,
        String,
        i32,
        i32,
        i32,
        i32,
        Option<String>,
        Option<String>,
        String,
        String,
    )> = sqlx::query_as(
        "SELECT et.id, et.slug, et.duration_min, et.buffer_before, et.buffer_after,
                    et.min_notice_min, et.location_type, et.location_value,
                    u.id, et.default_calendar_view
             FROM event_types et
             JOIN accounts a ON a.id = et.account_id
             JOIN users u ON u.id = a.user_id
             WHERE et.id = ?",
    )
    .bind(&et_id_raw)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    let (
        et_id,
        et_slug,
        duration,
        buf_before,
        buf_after,
        min_notice,
        loc_type,
        loc_value,
        owner_user_id,
        default_calendar_view,
    ) = match et_info {
        Some(e) => e,
        None => {
            return Html(crate::i18n::translate(
                lang,
                "error-event-type-not-found",
                None,
            ))
            .into_response()
        }
    };

    let et_title: String = sqlx::query_scalar("SELECT title FROM event_types WHERE id = ?")
        .bind(&et_id)
        .fetch_one(&state.pool)
        .await
        .unwrap_or_default();

    // The booking's host is not necessarily the event type owner: a team
    // booking belongs to its assigned member, or to the whole eligible roster
    // on a collective event type. Availability and the profile shown to the
    // guest both have to follow the booking.
    let hosts = reschedule_hosts(&state.pool, &uid, &owner_user_id).await;
    let host = reschedule_host_profile(&state.pool, &hosts, &et_id).await;

    let old_guest_tz: String =
        sqlx::query_scalar("SELECT guest_timezone FROM bookings WHERE id = ?")
            .bind(&booking_id)
            .fetch_one(&state.pool)
            .await
            .unwrap_or_else(|_| "UTC".into());
    let old_tz = old_guest_tz.parse::<Tz>().unwrap_or(Tz::UTC);
    let old_stored_tz = get_host_tz(&state.pool, &et_id).await;
    let (old_date, old_start_time, old_end_time) =
        booking_strings_in_guest_tz(&start_at, &end_at, old_stored_tz, old_tz);
    let old_date_label = format_date_label(&old_date, lang);

    // If date + time + tz are present, show confirmation page
    if let (Some(date), Some(time)) = (&query.date, &query.time) {
        let guest_tz = parse_guest_tz(query.tz.as_deref());
        let new_date = match NaiveDate::parse_from_str(date, "%Y-%m-%d") {
            Ok(d) => d,
            Err(_) => {
                return Html(crate::i18n::translate(lang, "error-invalid-date", None))
                    .into_response()
            }
        };
        let new_time = match NaiveTime::parse_from_str(time, "%H:%M") {
            Ok(t) => t,
            Err(_) => {
                return Html(crate::i18n::translate(lang, "error-invalid-time", None))
                    .into_response()
            }
        };
        let new_end = new_date.and_time(new_time) + Duration::minutes(duration as i64);
        let new_date_label = crate::i18n::format_long_date(new_date, lang);
        let new_start_time_str = new_time.format("%H:%M").to_string();
        let new_end_time_str = new_end.time().format("%H:%M").to_string();

        let back_url = format!("/booking/reschedule/{}?tz={}", token, guest_tz.name());

        let tmpl = match state
            .templates
            .get_template("booking_reschedule_confirm.html")
        {
            Ok(t) => t,
            Err(e) => return internal_error_response("internal", &e),
        };
        let rendered = tmpl
            .render(context! {
                event_title => et_title,
                old_date_label => old_date_label,
                old_start_time => old_start_time,
                old_end_time => old_end_time,
                new_date_label => new_date_label,
                new_start_time => new_start_time_str,
                new_end_time => new_end_time_str,
                host_name => host.name,
                date => date,
                time => time,
                tz => guest_tz.name(),
                back_url => back_url,
                company_link => state.company_link.read().await.clone(),
                lang => lang,
            })
            .unwrap_or_else(|e| internal_error_body("template render", &e));
        return Html(rendered).into_response();
    }

    // Show slot picker with reschedule context
    sync_reschedule_hosts(&state, &hosts).await;

    let guest_tz = parse_guest_tz(query.tz.as_deref());
    let host_tz = get_host_tz(&state.pool, &et_id).await;
    let booking_window = get_booking_window(&state.pool, &et_id).await;
    let guest_tz_name = guest_tz.name().to_string();

    let (year, month) = parse_month_param(
        query.month.as_deref(),
        guest_tz,
        booking_window.bookable_from,
    );
    let (
        start_offset,
        days_ahead,
        month_label,
        prev_month,
        next_month,
        first_weekday,
        days_in_month,
        today_date,
        month_year,
    ) = build_month_params(year, month, host_tz, guest_tz, lang, booking_window);

    let now_host = Utc::now().with_timezone(&host_tz).naive_local();
    let end_date = now_host.date() + Duration::days((start_offset + days_ahead) as i64);
    let window_end = end_date.and_hms_opt(23, 59, 59).unwrap_or(now_host);
    let busy = reschedule_busy_source(
        &state.pool,
        &hosts,
        &et_id,
        now_host,
        window_end,
        host_tz,
        &booking_id,
        &uid,
    )
    .await;
    let slot_days = compute_slots(
        &state.pool,
        &et_id,
        duration,
        buf_before,
        buf_after,
        min_notice,
        start_offset,
        days_ahead,
        booking_window,
        host_tz,
        guest_tz,
        busy,
        Some(&booking_id),
    )
    .await;

    let days_ctx: Vec<minijinja::Value> = slot_days.iter().map(|d| {
        let slots: Vec<minijinja::Value> = d.slots.iter().map(|s| {
            context! { start => s.start, end => s.end, host_date => s.host_date, host_time => s.host_time, guest_date => s.guest_date }
        }).collect();
        context! { date => d.date, label => d.label, slots => slots }
    }).collect();
    let available_dates: Vec<String> = slot_days.iter().map(|d| d.date.clone()).collect();

    let tz_options = timezone_options(&guest_tz_name, lang);

    let reschedule_base = format!("/booking/reschedule/{}", token);

    // Empty unless the booking belongs to a whole roster, in which case the
    // sidebar renders the team card instead of a single host.
    let team_members_ctx: Vec<minijinja::Value> = host
        .team
        .as_ref()
        .map(|team| {
            team.members
                .iter()
                .map(|(member_id, member_name, avatar_path)| {
                    context! {
                        id => member_id,
                        name => member_name,
                        has_avatar => avatar_path.is_some(),
                        initials => compute_initials(member_name),
                    }
                })
                .collect()
        })
        .unwrap_or_default();

    let (meeting_jitsi_label, meeting_webhook_label) = meeting_provider_labels(&state).await;
    let tmpl = match state.templates.get_template("slots.html") {
        Ok(t) => t,
        Err(e) => return internal_error_response("internal", &e),
    };
    let rendered = tmpl
        .render(context! {
            meeting_jitsi_label => meeting_jitsi_label,
            meeting_webhook_label => meeting_webhook_label,
            event_type => context! {
                slug => et_slug,
                title => et_title.clone(),
                description => Option::<String>::None,
                duration_min => duration,
                location_type => loc_type,
                location_value => loc_value,
            },
            host_name => host.name,
            host_title => host.title.as_deref().unwrap_or(""),
            host_user_id => host.user_id,
            host_has_avatar => host.has_avatar,
            host_initials => compute_initials(&host.name),
            team_id => host.team.as_ref().map(|team| team.id.clone()),
            team_has_avatar => host.has_avatar,
            team_initials => compute_initials(&host.name),
            team_members => team_members_ctx,
            days => days_ctx,
            available_dates => available_dates,
            month_label => month_label,
            month_year => month_year,
            prev_month => prev_month,
            next_month => next_month,
            booking_window => booking_window_ctx(&booking_window, host_tz, guest_tz, lang),
            first_weekday => first_weekday,
            days_in_month => days_in_month,
            today_date => today_date,
            guest_tz => guest_tz_name,
            tz_options => tz_options,
            reschedule_base => reschedule_base,
            reschedule_info => context! {
                event_title => et_title,
                old_date => old_date,
                old_time => old_start_time,
            },
            default_calendar_view => default_calendar_view,
            company_link => state.company_link.read().await.clone(),
            lang => lang,
        })
        .unwrap_or_else(|e| internal_error_body("template render", &e));

    Html(rendered).into_response()
}

/// Guest reschedule: process the reschedule
pub(super) async fn guest_reschedule_booking(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(token): Path<String>,
    Form(form): Form<RescheduleForm>,
) -> impl IntoResponse {
    if let Err(resp) = verify_csrf_token(&headers, &form._csrf) {
        return resp;
    }
    // Rate limit
    let client_ip = client_ip_for_rate_limit(&headers);
    if state.booking_limiter.check_limited(&client_ip).await {
        return Html(crate::i18n::translate(
            crate::i18n::detect_from_headers(&headers),
            "error-too-many-requests",
            None,
        ))
        .into_response();
    }

    let booking: Option<(
        String,
        String,
        String,
        String,
        String,
        String,
        String,
        String,
        String,
        String,
        i32,
        Option<String>,
        Option<String>,
        Option<String>,
        i32,
        i32,
    )> = sqlx::query_as(
        "SELECT b.id, b.uid, b.guest_name, b.guest_email, CASE b.time_version WHEN 1 THEN b.start_at ELSE rtrim(b.start_at, 'Z') END AS start_at, CASE b.time_version WHEN 1 THEN b.end_at ELSE rtrim(b.end_at, 'Z') END AS end_at,
                    et.id, et.title, u.id, u.name, et.duration_min,
                    COALESCE(NULLIF(b.meeting_url, ''), et.location_value),
                    b.caldav_calendar_href, b.language,
                    et.min_notice_min, b.reschedule_by_host
             FROM bookings b
             JOIN event_types et ON et.id = b.event_type_id
             JOIN accounts a ON a.id = et.account_id
             JOIN users u ON u.id = a.user_id
             WHERE b.reschedule_token = ? AND b.status IN ('confirmed', 'pending')",
    )
    .bind(&token)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    let (
        booking_id,
        uid,
        guest_name,
        guest_email,
        old_start_at,
        old_end_at,
        et_id,
        et_title,
        host_user_id,
        host_name,
        duration,
        loc_value,
        caldav_href,
        stored_language,
        min_notice,
        reschedule_by_host,
    ) = match booking {
        Some(b) => b,
        None => {
            return render_booking_action_error_keys_in(
                &state,
                crate::i18n::detect_from_headers(&headers),
                "bae-title-invalid-link",
                "bae-body-reschedule-link-invalid",
            );
        }
    };

    // Past the rate limit: the page and every email use the language stored
    // with the booking, as the slot picker did (#209).
    let lang = token_page_lang(
        stored_language.as_deref(),
        crate::i18n::detect_from_headers(&headers),
    );

    // Defensive notice-window check on POST too: the GET may have been
    // cached, or the cutoff may have rolled over between page load and
    // submit. Either way, never mutate state inside the blocked window.
    if let Some((notice_min, host_email)) = fetch_notice_min_and_host_email(
        &state.pool,
        "reschedule_token",
        &token,
        "reschedule_notice_min",
    )
    .await
    {
        if let Some(resp) = check_notice_window(
            &state,
            notice_min,
            &old_start_at,
            &host_email,
            "reschedule",
            lang,
        ) {
            tracing::info!(
                action = "reschedule",
                booking_id = %booking_id,
                "guest action blocked by notice window (POST)"
            );
            return resp;
        }
    }

    let date = match NaiveDate::parse_from_str(&form.date, "%Y-%m-%d") {
        Ok(d) => d,
        Err(_) => {
            return Html(crate::i18n::translate(lang, "error-invalid-date", None)).into_response()
        }
    };
    if let Err(e) = validate_date_not_too_far(date) {
        return render_booking_action_error_keys_in(&state, lang, "bae-title-invalid-booking", e);
    }
    let start_time = match NaiveTime::parse_from_str(&form.time, "%H:%M") {
        Ok(t) => t,
        Err(_) => {
            return Html(crate::i18n::translate(lang, "error-invalid-time", None)).into_response()
        }
    };

    let guest_tz = parse_guest_tz(form.tz.as_deref());
    let new_guest_timezone = guest_tz.name().to_string();
    let host_tz = get_host_tz(&state.pool, &et_id).await;

    // The URL carries guest-local date/time; convert to host-local for storage
    // and availability checks (existing semantics).
    let guest_local_start = date.and_time(start_time);
    let Some((encoded_start, encoded_end)) =
        crate::booking_time::encode(guest_local_start, guest_tz, duration)
    else {
        return Html(crate::i18n::translate(lang, "error-invalid-time", None)).into_response();
    };
    let guest_local_end = crate::booking_time::local(&encoded_end, guest_tz, guest_tz).unwrap();
    let slot_start = crate::booking_time::local(&encoded_start, host_tz, host_tz).unwrap();
    let (check_start, check_end) =
        crate::booking_time::busy_range(&encoded_start, &encoded_end, host_tz).unwrap();
    let guest_end_time = guest_local_end.time().format("%H:%M").to_string();

    if chrono::DateTime::parse_from_rfc3339(&encoded_start)
        .unwrap()
        .with_timezone(&Utc)
        < Utc::now() + Duration::minutes(min_notice as i64)
    {
        return Html(crate::i18n::translate(lang, "error-slot-too-soon", None)).into_response();
    }

    // Booking window (rolling horizon and bookable date range). The slot
    // list already hides anything outside it, but that is advisory: a crafted
    // POST has to be rejected here too.
    if !get_booking_window(&state.pool, &et_id)
        .await
        .contains(slot_start.date(), today_in(host_tz))
    {
        return Html(crate::i18n::translate(
            lang,
            "error-slot-beyond-horizon",
            None,
        ))
        .into_response();
    }

    // Check conflicts excluding this booking, against the calendars the
    // booking's host(s) actually own — the assigned member for round-robin,
    // every eligible member for collective, the owner for personal event types.
    // Resolving through the event type owner let a guest move a team booking
    // onto a slot the assigned member was already busy for (#166).
    let hosts = reschedule_hosts(&state.pool, &uid, &host_user_id).await;
    let busy = reschedule_busy_source(
        &state.pool,
        &hosts,
        &et_id,
        check_start,
        check_end,
        host_tz,
        &booking_id,
        &uid,
    )
    .await;
    if !busy_source_is_free(&busy, check_start, check_end) {
        return Html(crate::i18n::translate(lang, "error-slot-unavailable", None)).into_response();
    }

    // The time must be one the slot page offers. Weekly rules, date
    // overrides, interval alignment and first_slot_only are only enforced
    // by recomputing the page's own slots; the checks above just give the
    // common refusals their own message.
    let window = offered_slot_window(host_tz, slot_start);
    let (buffer_before, buffer_after): (i32, i32) =
        sqlx::query_as("SELECT buffer_before, buffer_after FROM event_types WHERE id = ?")
            .bind(&et_id)
            .fetch_optional(&state.pool)
            .await
            .ok()
            .flatten()
            .unwrap_or((0, 0));
    let offered_busy = reschedule_busy_source(
        &state.pool,
        &hosts,
        &et_id,
        window.busy_start,
        window.busy_end,
        host_tz,
        &booking_id,
        &uid,
    )
    .await;
    if !is_offered_slot(
        &state.pool,
        &et_id,
        duration,
        buffer_before,
        buffer_after,
        min_notice,
        host_tz,
        guest_tz,
        &window,
        offered_busy,
        Some(&booking_id),
        slot_start,
    )
    .await
    {
        return Html(crate::i18n::translate(lang, "error-slot-unavailable", None)).into_response();
    }

    // Re-check required resources for the new slot (excluding this booking's
    // own reservation) and re-pick in round-robin mode, serialized with
    // concurrent bookings by the process-wide resource lock (held until the
    // booking row is updated below).
    let resched_resource_ids: Vec<String> =
        crate::resources::resources_for_event_type(&state.pool, &et_id)
            .await
            .into_iter()
            .map(|r| r.id)
            .collect();
    let resource_guard = if resched_resource_ids.is_empty() {
        None
    } else {
        crate::resources::sync_if_stale(&state.pool, &resched_resource_ids).await;
        Some(crate::resources::booking_lock().await)
    };
    let new_resource_assignment = match crate::resources::check_and_pick(
        &state.pool,
        &et_id,
        check_start,
        check_end,
        host_tz,
        Some(&booking_id),
    )
    .await
    {
        crate::resources::ResourceCheck::Busy => {
            return Html(crate::i18n::translate(lang, "error-slot-unavailable", None))
                .into_response();
        }
        crate::resources::ResourceCheck::Free { assigned } => assigned,
        crate::resources::ResourceCheck::NoResources => None,
    };
    // If the assigned resource changes, the old reservation must be
    // released, but that is network I/O and must NOT run under the
    // process-wide lock. Remember the old id and release after the row is
    // updated and the lock dropped.
    let old_resource: Option<String> =
        sqlx::query_scalar("SELECT assigned_resource_id FROM bookings WHERE id = ?")
            .bind(&booking_id)
            .fetch_optional(&state.pool)
            .await
            .unwrap_or(None)
            .flatten();
    let release_old = old_resource
        .clone()
        .filter(|_| old_resource != new_resource_assignment);

    let (new_start_at, new_end_at) = (encoded_start, encoded_end);
    let new_reschedule_token = uuid::Uuid::new_v4().to_string();
    let new_cancel_token = uuid::Uuid::new_v4().to_string();
    let new_confirm_token = uuid::Uuid::new_v4().to_string();

    // Check if the event type requires confirmation
    let requires_confirmation: i32 =
        sqlx::query_scalar("SELECT requires_confirmation FROM event_types WHERE id = ?")
            .bind(&et_id)
            .fetch_one(&state.pool)
            .await
            .unwrap_or(0);

    // Determine new status:
    // - Host-initiated reschedule → confirmed (host already approved)
    // - Guest-initiated + requires_confirmation → pending (needs host approval)
    // - Guest-initiated + no confirmation needed → confirmed (auto-approved)
    let host_initiated = reschedule_by_host != 0;
    let needs_approval = !host_initiated && requires_confirmation != 0;
    let new_status = if needs_approval {
        "pending"
    } else {
        "confirmed"
    };
    let new_confirm = if needs_approval {
        Some(new_confirm_token.clone())
    } else {
        None
    };

    let assigned_member: Option<String> =
        sqlx::query_scalar("SELECT assigned_user_id FROM bookings WHERE id = ?")
            .bind(&booking_id)
            .fetch_one(&state.pool)
            .await
            .unwrap_or(None);
    match crate::booking_time::legacy_slot_taken(
        &state.pool,
        &et_id,
        assigned_member.as_deref(),
        &new_start_at,
        &booking_id,
    )
    .await
    {
        Ok(false) => {}
        Ok(true) => {
            return Html(crate::i18n::translate(lang, "error-slot-unavailable", None))
                .into_response()
        }
        Err(e) => return internal_error_response("booking conflict check", &e),
    }
    // The pending window is written by the same UPDATE as the status: a
    // request put back to pending gets a fresh deadline (and a fresh host
    // nudge) atomically, so the expiry sweep can never see it pending with
    // the previous, possibly past, deadline. A confirmed result clears it.
    let (pending_expires_at, pending_since) = if needs_approval {
        let (hours, min_notice): (Option<i32>, i32) = sqlx::query_as(
            "SELECT pending_expiry_hours, min_notice_min FROM event_types WHERE id = ?",
        )
        .bind(&et_id)
        .fetch_optional(&state.pool)
        .await
        .ok()
        .flatten()
        .unwrap_or((None, 0));
        pending_window(Utc::now(), &new_start_at, hours, min_notice)
    } else {
        (None, None)
    };
    // Read before the UPDATE below, which stamps it when it confirms.
    let attendees_told = was_confirmed_before(&state.pool, &booking_id).await;
    // The booking moves, so its invite's SEQUENCE grows (#267): the update
    // sent to the guest and the attendees, now or at approval, replaces their
    // copy rather than being ignored as no newer.
    let update_result = sqlx::query(
        "UPDATE bookings SET time_version = 1, start_at = ?, end_at = ?, status = ?,
                reschedule_token = ?, cancel_token = ?, confirm_token = ?,
                reminder_sent_at = NULL, guest_timezone = ?, reschedule_by_host = 0,
                assigned_resource_id = ?,
                pending_expires_at = ?, pending_since = ?, pending_reminder_sent_at = NULL,
                ics_sequence = ics_sequence + 1
         WHERE id = ?",
    )
    .bind(&new_start_at)
    .bind(&new_end_at)
    .bind(new_status)
    .bind(&new_reschedule_token)
    .bind(&new_cancel_token)
    .bind(&new_confirm)
    .bind(&new_guest_timezone)
    .bind(&new_resource_assignment)
    .bind(&pending_expires_at)
    .bind(&pending_since)
    .bind(&booking_id)
    .execute(&state.pool)
    .await;
    drop(resource_guard);
    if let Err(e) = update_result {
        // idx_bookings_no_overlap can reject the new time; do not proceed to
        // emails and pushes advertising a time the DB refused.
        if e.to_string().contains("UNIQUE constraint failed") {
            return Html(crate::i18n::translate(lang, "error-slot-unavailable", None))
                .into_response();
        }
        return internal_error_response("database query", &e);
    }
    if let Some(old_rid) = release_old {
        resource_release_one(&state.pool, &state.secret_key, &uid, &old_rid).await;
    }

    if host_initiated {
        tracing::info!(booking_id = %booking_id, old_start = %old_start_at, new_start = %new_start_at, "booking rescheduled by guest (host-initiated, confirmed)");
    } else if needs_approval {
        tracing::info!(booking_id = %booking_id, old_start = %old_start_at, new_start = %new_start_at, "booking rescheduled by guest (now pending, needs approval)");
    } else {
        tracing::info!(booking_id = %booking_id, old_start = %old_start_at, new_start = %new_start_at, "booking rescheduled by guest (auto-confirmed)");
    }

    // Resolve the host side from the booking, not from the event type owner:
    // the ICS pushed below has to carry the same ORGANIZER the event was
    // created with, or Google CalDAV rejects the PUT with 403 Forbidden and the
    // reschedule never reaches the calendar. See `booking_host_identity`.
    // Each side reads the emails below in their own language: the host in
    // their saved one, the guest in the one stored with the booking (#209).
    let (host_name, host_email, host_language) = match booking_host_identity(&state.pool, &uid)
        .await
    {
        Some(identity) => identity,
        None => {
            tracing::warn!(
                uid = %uid,
                user_id = %host_user_id,
                "could not resolve the booking's host identity; falling back to the event type owner"
            );
            host_contact(&state.pool, &host_user_id).await.unwrap_or((
                host_name,
                String::new(),
                None,
            ))
        }
    };
    let booking_language = Some(lang.to_string());

    // Legacy values use the event-type timezone; new values are UTC. Convert into the
    // guest's tz so `RescheduleDetails` carries guest-local wall-clock for
    // both the OLD and NEW times — matches the contract used elsewhere and
    // lets `host_time_display` correctly recover the host wall-clock.
    let (old_date, old_start_time, old_end_time) =
        booking_strings_in_guest_tz(&old_start_at, &old_end_at, host_tz, guest_tz);

    // Both branches below hand `new_guest_timezone` to the email details, so
    // keep a copy for the SMS that follows them.
    let sms_timezone = new_guest_timezone.clone();

    // Both branches carry what the guest typed: the host approving the new
    // time sees it as they did the first time (#266).
    let (guest_notes, guest_phone_shown) = booking_guest_input(&state.pool, &uid).await;

    if needs_approval {
        // Guest-initiated reschedule on requires_confirmation event → pending.
        // Delete the prior CalDAV event (it will be re-pushed if/when the host approves),
        // and clear caldav_calendar_href on the booking so the sync orphan sweep does not
        // race the approval flow and cancel this pending booking before the host clicks
        // approve. See cancel_orphaned_bookings in src/commands/sync.rs.
        if caldav_href.is_some() {
            // Deletes from the calendar(s) the event actually lives on:
            // assigned member, every member for collective, else owner,
            // including the legacy fallback for pre-#147 bookings whose
            // event sits on the owner's calendar. Must run before the href
            // is cleared below, which drops the only pointer to it.
            caldav_delete_booking(&state.pool, &state.secret_key, &host_user_id, &uid).await;
            let _ = sqlx::query("UPDATE bookings SET caldav_calendar_href = NULL WHERE id = ?")
                .bind(&booking_id)
                .execute(&state.pool)
                .await;
        } else {
            // The host's copy was never written, but a claimant's may have
            // been (#269): nothing after a decline or expiry would find it.
            claimant_delete_booking(&state.pool, &state.secret_key, &uid).await;
        }
        // Release any resource reservation while the reschedule awaits approval.
        resource_delete_booking(&state.pool, &state.secret_key, &uid).await;

        if let Ok(Some(smtp_config)) =
            crate::email::load_smtp_config(&state.pool, &state.secret_key).await
        {
            let base_url = crate::settings::base_url();

            // Send host reschedule approval request
            let reschedule_details = crate::email::RescheduleDetails {
                old_utc_times: crate::booking_time::ics_times(&old_start_at, &old_end_at),
                utc_times: crate::booking_time::ics_times(&new_start_at, &new_end_at),
                event_title: et_title.clone(),
                old_date,
                old_start_time,
                old_end_time,
                new_date: form.date.clone(),
                new_start_time: form.time.clone(),
                new_end_time: guest_end_time.clone(),
                guest_name: guest_name.clone(),
                guest_email: guest_email.clone(),
                guest_timezone: new_guest_timezone.clone(),
                host_name: host_name.clone(),
                host_email: host_email.clone(),
                uid: uid.clone(),
                location: loc_value.clone(),
                notes: guest_notes.clone(),
                guest_phone: guest_phone_shown,
                host_timezone: host_tz.name().to_string(),
                guest_language: booking_language.clone(),
                host_language: host_language.clone(),
                sequence: booking_ics_sequence(&state.pool, &uid).await,
                additional_attendees: Vec::new(),
            };

            // Send guest pending notice with new tokens
            let guest_cancel_url = base_url.as_ref().map(|base| {
                format!(
                    "{}/booking/cancel/{}",
                    base.trim_end_matches('/'),
                    new_cancel_token
                )
            });
            let guest_reschedule_url = base_url.as_ref().map(|base| {
                format!(
                    "{}/booking/reschedule/{}",
                    base.trim_end_matches('/'),
                    new_reschedule_token
                )
            });
            let pending_details = crate::email::BookingDetails {
                utc_times: crate::booking_time::ics_times(&new_start_at, &new_end_at),
                event_title: et_title.clone(),
                date: form.date.clone(),
                start_time: form.time.clone(),
                end_time: guest_end_time.clone(),
                guest_name: guest_name.clone(),
                guest_email: guest_email.clone(),
                guest_timezone: new_guest_timezone,
                host_name: host_name.clone(),
                host_email,
                uid: uid.clone(),
                notes: guest_notes,
                location: loc_value,
                reminder_minutes: None,
                additional_attendees: vec![],
                guest_language: booking_language,
                host_timezone: host_tz.name().to_string(),
                resource_name: booking_resource_label(&state.pool, &uid).await,
                ..Default::default()
            };
            let new_confirm_token = new_confirm_token.clone();
            spawn_emails(async move {
                let uid = reschedule_details.uid.as_str();
                log_email_result(
                    crate::email::send_host_reschedule_request(
                        &smtp_config,
                        &reschedule_details,
                        Some(&new_confirm_token),
                        base_url.as_deref(),
                    )
                    .await,
                    "host_reschedule_request",
                    uid,
                );
                log_email_result(
                    crate::email::send_guest_pending_notice_ex(
                        &smtp_config,
                        &pending_details,
                        guest_cancel_url.as_deref(),
                        guest_reschedule_url.as_deref(),
                    )
                    .await,
                    "guest_pending_notice",
                    uid,
                );
            });
        }
    } else {
        // Confirmed reschedule (host-initiated or guest on non-confirmation event)
        // Push updated event to CalDAV
        let push_details = crate::email::BookingDetails {
            utc_times: crate::booking_time::ics_times(&new_start_at, &new_end_at),
            event_title: et_title.clone(),
            date: form.date.clone(),
            start_time: form.time.clone(),
            end_time: guest_end_time.clone(),
            guest_name: guest_name.clone(),
            guest_email: guest_email.clone(),
            guest_timezone: new_guest_timezone.clone(),
            host_name: host_name.clone(),
            host_email: host_email.clone(),
            uid: uid.clone(),
            notes: guest_notes,
            guest_phone: guest_phone_shown,
            location: loc_value.clone(),
            reminder_minutes: None,
            additional_attendees: booking_additional_attendees(&state.pool, &uid).await,
            guest_language: booking_language.clone(),
            host_language: host_language.clone(),
            host_timezone: host_tz.name().to_string(),
            sequence: booking_ics_sequence(&state.pool, &uid).await,
            attendee_languages: booking_attendee_languages(&state.pool, &uid).await,
            ..Default::default()
        };
        caldav_push_booking(
            &state.pool,
            &state.secret_key,
            &host_user_id,
            &uid,
            &push_details,
        )
        .await;
        resource_push_booking(&state.pool, &state.secret_key, &uid, &push_details).await;

        if let Ok(Some(smtp_config)) =
            crate::email::load_smtp_config(&state.pool, &state.secret_key).await
        {
            let base_url = crate::settings::base_url();
            let guest_cancel_url = base_url.as_ref().map(|base| {
                format!(
                    "{}/booking/cancel/{}",
                    base.trim_end_matches('/'),
                    new_cancel_token
                )
            });
            let guest_reschedule_url = base_url.as_ref().map(|base| {
                format!(
                    "{}/booking/reschedule/{}",
                    base.trim_end_matches('/'),
                    new_reschedule_token
                )
            });

            let reschedule_details = crate::email::RescheduleDetails {
                old_utc_times: crate::booking_time::ics_times(&old_start_at, &old_end_at),
                utc_times: crate::booking_time::ics_times(&new_start_at, &new_end_at),
                event_title: et_title.clone(),
                old_date,
                old_start_time,
                old_end_time,
                new_date: form.date.clone(),
                new_start_time: form.time.clone(),
                new_end_time: guest_end_time.clone(),
                guest_name: guest_name.clone(),
                guest_email: guest_email.clone(),
                guest_timezone: new_guest_timezone,
                host_name: host_name.clone(),
                host_email: host_email.clone(),
                uid,
                location: loc_value,
                host_timezone: host_tz.name().to_string(),
                guest_language: booking_language,
                host_language,
                sequence: push_details.sequence,
                // The guest's moved invite replaces their confirmation one
                // whole, so it keeps its attendees and notes (#267).
                notes: push_details.notes.clone(),
                additional_attendees: push_details.additional_attendees.clone(),
                ..Default::default()
            };
            spawn_emails(async move {
                let uid = reschedule_details.uid.as_str();
                // Notify guest with confirmation + ICS
                log_email_result(
                    crate::email::send_guest_reschedule_notification(
                        &smtp_config,
                        &reschedule_details,
                        guest_cancel_url.as_deref(),
                        guest_reschedule_url.as_deref(),
                    )
                    .await,
                    "guest_reschedule_notification",
                    uid,
                );

                // The attendees' copy moves with the guest's (#267). A booking
                // never confirmed before (a request rescheduled once its event
                // type stopped requiring approval) has not told them yet.
                if attendees_told {
                    let old_time = format!(
                        "{} \u{2013} {}",
                        reschedule_details.old_start_time, reschedule_details.old_end_time
                    );
                    log_email_result(
                        crate::email::send_guest_attendee_reschedule(
                            &smtp_config,
                            &push_details,
                            Some((&reschedule_details.old_date, &old_time)),
                        )
                        .await,
                        "attendee_reschedule",
                        uid,
                    );
                } else {
                    log_email_result(
                        crate::email::send_guest_attendee_confirmations(
                            &smtp_config,
                            &push_details,
                        )
                        .await,
                        "attendee_confirmation",
                        uid,
                    );
                }

                // Notify host
                log_email_result(
                    crate::email::send_host_booking_confirmed(&smtp_config, &push_details).await,
                    "host_booking_confirmed",
                    uid,
                );
            });
        }
    }

    // Reschedule SMS to the guest. A reschedule that needs approval stays
    // silent here, like its email: the guest is told when the host approves.
    if !needs_approval {
        let sms_target: Option<(Option<String>, String)> = sqlx::query_as(
            "SELECT b.guest_phone, et.sms_phone_mode FROM bookings b
             JOIN event_types et ON et.id = b.event_type_id WHERE b.id = ?",
        )
        .bind(&booking_id)
        .fetch_optional(&state.pool)
        .await
        .unwrap_or(None);

        if let Some((phone, phone_mode)) = sms_target {
            if let Some(phone) = phone
                .as_deref()
                .map(str::trim)
                .filter(|p| crate::sms::collects_phone(&phone_mode) && !p.is_empty())
            {
                crate::sms::notify_guest(
                    &state.pool,
                    &state.secret_key,
                    crate::sms::SmsEvent::Rescheduled,
                    crate::sms::SmsContext {
                        phone,
                        event_title: &et_title,
                        date: &form.date,
                        start_time: &form.time,
                        timezone: &sms_timezone,
                        lang: Some(lang),
                    },
                )
                .await;
            }
        }
    }

    let date_label = crate::i18n::format_long_date(date, lang);
    let (cancel_notice_min, reschedule_notice_min) =
        fetch_event_type_notice_minutes(&state.pool, &et_id).await;

    let tmpl = match state.templates.get_template("confirmed.html") {
        Ok(t) => t,
        Err(e) => return internal_error_response("internal", &e),
    };
    let rendered = tmpl
        .render(context! {
            event_title => et_title,
            date_label => date_label,
            time_start => form.time,
            time_end => guest_end_time,
            host_name => host_name,
            guest_email => guest_email,
            pending => needs_approval,
            rescheduled => true,
            ics_url => format!("/booking/ics/{}", new_cancel_token),
            cancel_notice_min => cancel_notice_min,
            reschedule_notice_min => reschedule_notice_min,
            company_link => state.company_link.read().await.clone(),
            lang => lang,
        })
        .unwrap_or_else(|e| internal_error_body("template render", &e));

    Html(rendered).into_response()
}

/// Host reschedule: send the guest a link to pick a new time.
/// GET shows a confirmation page, POST sends the email.
pub(super) async fn host_reschedule_slots(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
    Path(booking_id): Path<String>,
) -> impl IntoResponse {
    let user = &auth_user.user;
    // Dashboard handler: no Accept-Language available, so honour the user's
    // saved preference and fall back to English. Once the dashboard is
    // translated this should switch to crate::i18n::resolve(...).
    let lang = user.language.as_deref().unwrap_or("en");

    let booking: Option<(String, String, String, String, String, String)> = sqlx::query_as(
        "SELECT b.id, b.guest_name, b.guest_email, CASE b.time_version WHEN 1 THEN b.start_at ELSE rtrim(b.start_at, 'Z') END AS start_at, CASE b.time_version WHEN 1 THEN b.end_at ELSE rtrim(b.end_at, 'Z') END AS end_at, et.title
         FROM bookings b
         JOIN event_types et ON et.id = b.event_type_id
         JOIN accounts a ON a.id = et.account_id
         WHERE b.id = ? AND a.user_id = ? AND b.status IN ('confirmed', 'pending')",
    )
    .bind(&booking_id)
    .bind(&user.id)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    let (bid, guest_name, guest_email, start_at, end_at, event_title) = match booking {
        Some(b) => b,
        None => return Redirect::to("/dashboard/bookings").into_response(),
    };

    let (start_at, end_at) = crate::booking_time::wall_strings(
        &start_at,
        &end_at,
        user.timezone.parse().unwrap_or(Tz::UTC),
    );
    let date_label = format_date_label(&start_at, lang);
    let start_time = extract_time_24h(&start_at);
    let end_time = extract_time_24h(&end_at);

    let tmpl = match state.templates.get_template("booking_host_reschedule.html") {
        Ok(t) => t,
        Err(e) => return internal_error_response("internal", &e),
    };
    let rendered = tmpl
        .render(context! {
            sidebar => sidebar_context(&auth_user, "bookings"),
            lang => auth_user.lang,
            booking_id => bid,
            event_title => event_title,
            guest_name => guest_name,
            guest_email => guest_email,
            date_label => date_label,
            start_time => start_time,
            end_time => end_time,
        })
        .unwrap_or_else(|e| internal_error_body("template render", &e));

    Html(rendered).into_response()
}

/// Host reschedule: set reschedule_by_host flag, regenerate token, email guest
pub(super) async fn host_reschedule_booking(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
    headers: HeaderMap,
    Path(booking_id): Path<String>,
    Form(form): Form<CancelForm>,
) -> impl IntoResponse {
    if let Err(resp) = verify_csrf_token(&headers, &form._csrf) {
        return resp;
    }
    let user = &auth_user.user;

    let booking: Option<(
        String,
        String,
        String,
        String,
        String,
        String,
        String,
        String,
        String,
    )> = sqlx::query_as(
        "SELECT b.id, b.uid, b.guest_name, b.guest_email, CASE b.time_version WHEN 1 THEN b.start_at ELSE rtrim(b.start_at, 'Z') END AS start_at, CASE b.time_version WHEN 1 THEN b.end_at ELSE rtrim(b.end_at, 'Z') END AS end_at,
                    et.title, COALESCE(b.guest_timezone, 'UTC'), et.id
             FROM bookings b
             JOIN event_types et ON et.id = b.event_type_id
             JOIN accounts a ON a.id = et.account_id
             WHERE b.id = ? AND a.user_id = ? AND b.status IN ('confirmed', 'pending')",
    )
    .bind(&booking_id)
    .bind(&user.id)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    let (bid, uid, guest_name, guest_email, start_at, end_at, event_title, guest_timezone, et_id) =
        match booking {
            Some(b) => b,
            None => return Redirect::to("/dashboard/bookings").into_response(),
        };

    // Generate new reschedule token and set the host-initiated flag
    let new_reschedule_token = uuid::Uuid::new_v4().to_string();
    let _ = sqlx::query(
        "UPDATE bookings SET reschedule_token = ?, reschedule_by_host = 1 WHERE id = ?",
    )
    .bind(&new_reschedule_token)
    .bind(&bid)
    .execute(&state.pool)
    .await;

    tracing::info!(booking_id = %bid, guest = %guest_email, "host requested reschedule — guest will pick new time");

    // Send email to guest asking them to pick a new time
    if let Ok(Some(smtp_config)) =
        crate::email::load_smtp_config(&state.pool, &state.secret_key).await
    {
        let base_url = crate::settings::base_url();
        let reschedule_url = base_url.as_ref().map(|base| {
            format!(
                "{}/booking/reschedule/{}",
                base.trim_end_matches('/'),
                new_reschedule_token
            )
        });
        let cancel_url: Option<String> =
            sqlx::query_scalar("SELECT cancel_token FROM bookings WHERE id = ?")
                .bind(&bid)
                .fetch_optional(&state.pool)
                .await
                .unwrap_or(None)
                .and_then(|t: String| {
                    base_url
                        .as_ref()
                        .map(|base| format!("{}/booking/cancel/{}", base.trim_end_matches('/'), t))
                });

        // Legacy times use the event timezone; version 1 timestamps are UTC. Convert
        // into the guest's tz before populating BookingDetails so the
        // guest-facing "pick new time" email shows their wall-clock with their
        // tz label.
        let host_tz = get_host_tz(&state.pool, &et_id).await;
        let guest_tz_parsed = guest_timezone.parse::<Tz>().unwrap_or(Tz::UTC);
        let (date, start_time, end_time) =
            booking_strings_in_guest_tz(&start_at, &end_at, host_tz, guest_tz_parsed);

        // Named after, and replying to, the host the invite named (the
        // stored organizer of a collective booking, #161), like every other
        // email about this booking.
        let (host_name, host_email) = match booking_host_identity(&state.pool, &uid).await {
            Some((name, email, _)) => (name, email),
            None => (
                user.name.clone(),
                user.booking_email
                    .clone()
                    .unwrap_or_else(|| user.email.clone()),
            ),
        };
        let details = crate::email::BookingDetails {
            utc_times: crate::booking_time::ics_times(&start_at, &end_at),
            event_title,
            date,
            start_time,
            end_time,
            guest_name,
            guest_email,
            guest_timezone,
            host_name,
            host_email,
            uid: String::new(),
            notes: None,
            location: None,
            reminder_minutes: None,
            additional_attendees: vec![],
            // The host asked for this, but the email is the guest's: written
            // in the language they booked in (#209).
            guest_language: crate::email::stored_booking_language(&state.pool, &bid).await,
            host_timezone: host_tz.name().to_string(),
            ..Default::default()
        };

        if let Some(url) = reschedule_url {
            let bid = bid.clone();
            spawn_emails(async move {
                log_email_result(
                    crate::email::send_guest_pick_new_time(
                        &smtp_config,
                        &details,
                        &url,
                        cancel_url.as_deref(),
                    )
                    .await,
                    "guest_pick_new_time",
                    &bid,
                );
            });
        }
    }

    Redirect::to("/dashboard/bookings").into_response()
}

// --- Claim endpoints ---

pub(super) async fn claim_booking_form(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(booking_id): Path<String>,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> impl IntoResponse {
    // The watcher's saved language, as their claim email (#209).
    let lang = claim_page_lang(
        &state.pool,
        params.get("token").map(String::as_str),
        &headers,
    )
    .await;
    let token = match params.get("token") {
        Some(t) => t,
        None => {
            return render_booking_action_error_keys_in(
                &state,
                lang,
                "bae-title-invalid-link",
                "bae-body-no-claim-token",
            );
        }
    };

    // Validate token
    let claim_info: Option<(String, String)> = sqlx::query_as(
        "SELECT bct.user_id, bct.booking_id FROM booking_claim_tokens bct \
         WHERE bct.token = ? AND bct.booking_id = ? AND bct.used_at IS NULL \
         AND bct.expires_at > datetime('now')",
    )
    .bind(token)
    .bind(&booking_id)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    if claim_info.is_none() {
        // Check if already claimed
        let claimed: Option<(String,)> = sqlx::query_as(
            "SELECT u.name FROM bookings b \
             JOIN users u ON u.id = b.claimed_by_user_id \
             WHERE b.id = ?",
        )
        .bind(&booking_id)
        .fetch_optional(&state.pool)
        .await
        .unwrap_or(None);

        if let Some((claimed_by_name,)) = claimed {
            let tmpl = match state.templates.get_template("booking_already_claimed.html") {
                Ok(t) => t,
                Err(e) => return internal_error_response("internal", &e),
            };
            return Html(
                tmpl.render(context! {
                    claimed_by_name => claimed_by_name,
                    lang => lang,
                })
                .unwrap_or_else(|e| internal_error_body("template render", &e)),
            )
            .into_response();
        }

        return render_booking_action_error_keys_in(
            &state,
            lang,
            "bae-title-invalid-or-expired",
            "bae-body-claim-invalid",
        );
    }

    // Fetch booking details for display
    let booking: Option<(String, String, String, String, String, Option<String>)> = sqlx::query_as(
        "SELECT et.title, b.guest_name, b.guest_email, CASE b.time_version WHEN 1 THEN b.start_at ELSE rtrim(b.start_at, 'Z') END AS start_at, CASE b.time_version WHEN 1 THEN b.end_at ELSE rtrim(b.end_at, 'Z') END AS end_at, u.name \
             FROM bookings b \
             JOIN event_types et ON et.id = b.event_type_id \
             LEFT JOIN users u ON u.id = b.assigned_user_id \
             WHERE b.id = ?",
    )
    .bind(&booking_id)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    let (event_title, guest_name, guest_email, start_at, end_at, assigned_to) = match booking {
        Some(b) => b,
        None => {
            return render_booking_action_error_keys_in(
                &state,
                lang,
                "bae-title-booking-not-found",
                "bae-body-booking-gone",
            )
        }
    };

    let (start_at, end_at) =
        crate::booking_time::guest_wall_strings(&state.pool, &booking_id, &start_at, &end_at).await;
    let date_label = format_date_label(&start_at, lang);
    let start_time = extract_time_24h(&start_at);
    let end_time = extract_time_24h(&end_at);

    let tmpl = match state.templates.get_template("booking_claim_form.html") {
        Ok(t) => t,
        Err(e) => return internal_error_response("internal", &e),
    };

    Html(
        tmpl.render(context! {
            event_title => event_title,
            date_label => date_label,
            start_time => start_time,
            end_time => end_time,
            guest_name => guest_name,
            guest_email => guest_email,
            assigned_to => assigned_to.unwrap_or_default(),
            token => token,
            lang => lang,
        })
        .unwrap_or_else(|e| internal_error_body("template render", &e)),
    )
    .into_response()
}

#[derive(Deserialize)]
pub(super) struct ClaimForm {
    _csrf: Option<String>,
    token: String,
}

pub(super) async fn claim_booking(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(booking_id): Path<String>,
    Form(form): Form<ClaimForm>,
) -> impl IntoResponse {
    if let Err(resp) = verify_csrf_token(&headers, &form._csrf) {
        return resp;
    }
    // The watcher's saved language, as their claim email (#209).
    let lang = claim_page_lang(&state.pool, Some(form.token.as_str()), &headers).await;

    // Validate token
    let claim_info: Option<(String, String)> = sqlx::query_as(
        "SELECT bct.user_id, bct.booking_id FROM booking_claim_tokens bct \
         WHERE bct.token = ? AND bct.booking_id = ? AND bct.used_at IS NULL \
         AND bct.expires_at > datetime('now')",
    )
    .bind(&form.token)
    .bind(&booking_id)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    let (claimant_user_id, _) = match claim_info {
        Some(c) => c,
        None => {
            // Check if already claimed
            let claimed: Option<(String,)> = sqlx::query_as(
                "SELECT u.name FROM bookings b \
                 JOIN users u ON u.id = b.claimed_by_user_id \
                 WHERE b.id = ?",
            )
            .bind(&booking_id)
            .fetch_optional(&state.pool)
            .await
            .unwrap_or(None);

            if let Some((claimed_by_name,)) = claimed {
                let tmpl = match state.templates.get_template("booking_already_claimed.html") {
                    Ok(t) => t,
                    Err(e) => return internal_error_response("internal", &e),
                };
                return Html(
                    tmpl.render(context! {
                        claimed_by_name => claimed_by_name,
                        lang => lang,
                    })
                    .unwrap_or_else(|e| internal_error_body("template render", &e)),
                )
                .into_response();
            }

            return render_booking_action_error_keys_in(
                &state,
                lang,
                "bae-title-invalid-or-expired",
                "bae-body-claim-invalid",
            )
            .into_response();
        }
    };

    // The guarded UPDATE below is what settles races; the transaction keeps
    // the claim and the token's use together.
    let mut tx = match sqlx::pool::Pool::begin(&state.pool).await {
        Ok(tx) => tx,
        Err(e) => return internal_error_response("database query", &e),
    };

    // Check booking is not already claimed (inside transaction)
    let already_claimed: Option<(String,)> = sqlx::query_as(
        "SELECT claimed_by_user_id FROM bookings WHERE id = ? AND claimed_by_user_id IS NOT NULL",
    )
    .bind(&booking_id)
    .fetch_optional(&mut *tx)
    .await
    .unwrap_or(None);

    if already_claimed.is_some() {
        let _ = tx.rollback().await;
        let claimed_name: Option<(String,)> = sqlx::query_as(
            "SELECT u.name FROM bookings b JOIN users u ON u.id = b.claimed_by_user_id WHERE b.id = ?",
        )
        .bind(&booking_id)
        .fetch_optional(&state.pool)
        .await
        .unwrap_or(None);

        let tmpl = match state.templates.get_template("booking_already_claimed.html") {
            Ok(t) => t,
            Err(e) => return internal_error_response("internal", &e),
        };
        return Html(
            tmpl.render(context! {
                claimed_by_name => claimed_name.map(|(n,)| n).unwrap_or_default(),
                lang => lang,
            })
            .unwrap_or_else(|e| internal_error_body("template render", &e)),
        )
        .into_response();
    }

    // Claim the booking. Only a confirmed one: a claim link stays valid for
    // days, so the booking may have been cancelled since, or be awaiting the
    // approval of a reschedule (the link works again once it is approved).
    // The guarded UPDATE decides, so a cancellation or another claim that
    // commits after the reads above still wins.
    let claimed = sqlx::query(
        "UPDATE bookings SET claimed_by_user_id = ?, claimed_at = datetime('now') \
         WHERE id = ? AND status = 'confirmed' AND claimed_by_user_id IS NULL",
    )
    .bind(&claimant_user_id)
    .bind(&booking_id)
    .execute(&mut *tx)
    .await
    .map(|r| r.rows_affected() == 1)
    .unwrap_or(false);
    if !claimed {
        let _ = tx.rollback().await;
        return render_booking_action_error_keys_in(
            &state,
            lang,
            "bae-title-invalid-or-expired",
            "bae-body-claim-invalid",
        )
        .into_response();
    }

    // Mark this token as used
    let _ =
        sqlx::query("UPDATE booking_claim_tokens SET used_at = datetime('now') WHERE token = ?")
            .bind(&form.token)
            .execute(&mut *tx)
            .await;

    if let Err(e) = tx.commit().await {
        return internal_error_response("database query", &e);
    }

    tracing::info!(booking_id = %booking_id, claimant_user_id = %claimant_user_id, "booking claimed");

    // Fetch booking + claimant details for CalDAV push and email
    let booking: Option<(
        String,
        String,
        String,
        String,
        String,
        String,
        String,
        String,
        Option<String>,
        String,
    )> = sqlx::query_as(
        "SELECT et.title, b.guest_name, b.guest_email, CASE b.time_version WHEN 1 THEN b.start_at ELSE rtrim(b.start_at, 'Z') END AS start_at, CASE b.time_version WHEN 1 THEN b.end_at ELSE rtrim(b.end_at, 'Z') END AS end_at, b.uid, \
             COALESCE(b.guest_timezone, 'UTC'), a.user_id, et.location_value, b.event_type_id \
             FROM bookings b \
             JOIN event_types et ON et.id = b.event_type_id \
             JOIN accounts a ON a.id = et.account_id \
             WHERE b.id = ?",
    )
    .bind(&booking_id)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    let (
        event_title,
        guest_name,
        guest_email,
        start_at,
        end_at,
        uid,
        guest_tz,
        host_user_id,
        location,
        event_type_id,
    ) = match booking {
        Some(b) => b,
        None => {
            return render_booking_action_error_keys_in(
                &state,
                lang,
                "bae-title-booking-not-found",
                "bae-body-booking-gone",
            )
            .into_response()
        }
    };

    let claimant: Option<(String, String, Option<String>)> = sqlx::query_as(
        "SELECT name, COALESCE(booking_email, email), language FROM users WHERE id = ?",
    )
    .bind(&claimant_user_id)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    let (claimant_name, claimant_email, claimant_language) = claimant.unwrap_or_default();

    // Re-pushed over the existing event, so it must carry the organizer the
    // event was created with: the assigned member, or a collective booking's
    // stored roster (#161), not the event type owner.
    let (host_name, host_email) = match booking_host_identity(&state.pool, &uid).await {
        Some((name, email, _)) => (name, email),
        None => host_contact(&state.pool, &host_user_id)
            .await
            .map(|(name, email, _)| (name, email))
            .unwrap_or_default(),
    };

    let claim_utc_times = crate::booking_time::ics_times(&start_at, &end_at);
    let (start_at, end_at) =
        crate::booking_time::guest_wall_strings(&state.pool, &booking_id, &start_at, &end_at).await;
    let date = start_at.get(..10).unwrap_or(&start_at).to_string();
    let start_time = extract_time_24h(&start_at);
    let end_time = extract_time_24h(&end_at);

    // Add claimant as a booking attendee
    let attendee_id = uuid::Uuid::new_v4().to_string();
    let _ = sqlx::query("INSERT INTO booking_attendees (id, booking_id, email) VALUES (?, ?, ?)")
        .bind(&attendee_id)
        .bind(&booking_id)
        .bind(&claimant_email)
        .execute(&state.pool)
        .await;

    let claim_host_tz = get_host_tz(&state.pool, &event_type_id).await;

    // Build details with claimant as additional attendee for CalDAV push
    let (guest_notes, guest_phone_shown) = booking_guest_input(&state.pool, &uid).await;
    let mut details = crate::email::BookingDetails {
        utc_times: claim_utc_times.clone(),
        event_title: event_title.clone(),
        date: date.clone(),
        start_time: start_time.clone(),
        end_time: end_time.clone(),
        guest_name: guest_name.clone(),
        guest_email: guest_email.clone(),
        guest_timezone: guest_tz,
        host_name,
        host_email,
        uid: uid.clone(),
        notes: guest_notes,
        guest_phone: guest_phone_shown,
        location,
        reminder_minutes: None,
        // Read like every other push, so the ATTENDEE lines keep the order
        // the next rewrite will give them (#266). The claimant was inserted
        // above and is normally listed already.
        additional_attendees: booking_additional_attendees(&state.pool, &uid).await,
        host_timezone: claim_host_tz.name().to_string(),
        sequence: booking_ics_sequence(&state.pool, &uid).await,
        ..Default::default()
    };
    if !details.additional_attendees.contains(&claimant_email) {
        details.additional_attendees.push(claimant_email.clone());
    }

    // Re-push updated ICS to host's CalDAV (with claimant as ATTENDEE). The
    // booking is claimed by now, so this writes the claimant's own copy too.
    caldav_push_booking(
        &state.pool,
        &state.secret_key,
        &host_user_id,
        &uid,
        &details,
    )
    .await;

    // Send confirmation email to claimant
    if let Ok(Some(smtp_config)) =
        crate::email::load_smtp_config(&state.pool, &state.secret_key).await
    {
        let details = details.clone();
        let claimant_name = claimant_name.clone();
        let claimant_email = claimant_email.clone();
        spawn_emails(async move {
            log_email_result(
                crate::email::send_claim_confirmation(
                    &smtp_config,
                    &details,
                    &claimant_name,
                    &claimant_email,
                    claimant_language.as_deref(),
                )
                .await,
                "claim_confirmation",
                &details.uid,
            );
        });
    }

    // Render success page
    let date_label = format_date_label(&start_at, lang);
    let tmpl = match state.templates.get_template("booking_claimed.html") {
        Ok(t) => t,
        Err(e) => return internal_error_response("internal", &e),
    };

    Html(
        tmpl.render(context! {
            event_title => event_title,
            date_label => date_label,
            start_time => start_time,
            end_time => end_time,
            guest_name => guest_name,
            guest_email => guest_email,
            lang => lang,
        })
        .unwrap_or_else(|e| internal_error_body("template render", &e)),
    )
    .into_response()
}
