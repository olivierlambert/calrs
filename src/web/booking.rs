//! Booking form submissions for every public URL shape, with validation, location and SMS phone.

use super::*;

/// Read the org-wide display labels for the Jitsi / webhook providers from
/// the cached `MeetingConfig`. Returns empty strings when no display name is
/// configured — slot/book templates fall back to the generic "Video call"
/// label in that case.
pub(super) async fn meeting_provider_labels(state: &AppState) -> (String, String) {
    let cfg = state.meeting_config.read().await;
    let jitsi = cfg
        .jitsi
        .as_ref()
        .and_then(|j| j.display_name.clone())
        .unwrap_or_default();
    let webhook = cfg
        .webhook
        .as_ref()
        .and_then(|w| w.display_name.clone())
        .unwrap_or_default();
    (jitsi, webhook)
}

/// Pick the location URL to expose on a freshly-created booking.
///
/// When the event type uses an auto provider (`jitsi_auto`, `webhook_auto`,
/// `google_meet`) **and** the booking is going straight to confirmed, we generate a fresh
/// URL via `meeting::generate_and_persist` and store it on the booking row.
/// Otherwise we fall back to the event type's static `location_value`. The
/// returned value is what gets piped into the email + ICS + CalDAV write.
///
/// For pending bookings, generation is deferred to the approval moment
/// (`approve_booking_by_token`) so a host who declines never invokes a
/// configured webhook.
pub(super) async fn resolve_booking_location(
    state: &AppState,
    booking_id: &str,
    event_type_id: &str,
    host_user_id: Option<&str>,
    static_location_value: Option<&str>,
    guest_name: &str,
    guest_email: &str,
    confirmed: bool,
) -> Option<String> {
    if confirmed {
        if let Some(generated) = meeting::generate_and_persist(
            &state.pool,
            &state.secret_key,
            booking_id,
            event_type_id,
            host_user_id,
            guest_name,
            guest_email,
        )
        .await
        {
            return Some(generated);
        }
    }
    if let Some(v) = static_location_value
        .map(str::to_string)
        .filter(|s| !s.is_empty())
    {
        return Some(v);
    }
    // Meet mint failed: still show a label so confirmation and email are
    // not missing the location the booking page promised.
    if confirmed {
        let loc_type: Option<String> =
            sqlx::query_scalar("SELECT location_type FROM event_types WHERE id = ?")
                .bind(event_type_id)
                .fetch_optional(&state.pool)
                .await
                .ok()
                .flatten();
        if loc_type.as_deref() == Some(meeting::LOCATION_TYPE_GOOGLE_MEET) {
            let lang: Option<Option<String>> =
                sqlx::query_scalar("SELECT language FROM bookings WHERE id = ?")
                    .bind(booking_id)
                    .fetch_optional(&state.pool)
                    .await
                    .ok()
                    .flatten();
            return Some(crate::google_meet::failed_mint_location_label(
                lang.flatten().as_deref().unwrap_or("en"),
            ));
        }
    }
    None
}

/// Reject `google_meet` unless every scheduling-relevant host has Google
/// Calendar OAuth2 connected with a write-back calendar selected.
pub(super) async fn google_meet_location_error(
    pool: &SqlitePool,
    location_type: &str,
    personal_user_id: Option<&str>,
    team_id: Option<&str>,
    event_type_id: Option<&str>,
) -> Option<String> {
    if location_type != meeting::LOCATION_TYPE_GOOGLE_MEET {
        return None;
    }
    crate::google_meet::google_meet_prereq_error(pool, personal_user_id, team_id, event_type_id)
        .await
}

/// Dynamic-group Google Meet gate: only the event type owner (first
/// username) matters for minting. Checked on the slots and booking pages
/// so the guest is not asked to fill the form first. Does not list host
/// names.
pub(super) async fn dynamic_group_google_meet_error_page(
    state: &AppState,
    lang: &str,
    location_type: &str,
    owner_user_id: &str,
) -> Option<Html<String>> {
    if location_type != meeting::LOCATION_TYPE_GOOGLE_MEET {
        return None;
    }
    if crate::google_meet::user_has_google_writeback(&state.pool, owner_user_id).await {
        return None;
    }
    let title = crate::i18n::translate(lang, "google-meet-unavailable-title", None);
    let message = crate::i18n::translate(lang, "google-meet-dynamic-group-unavailable", None);
    let tmpl = match state.templates.get_template("booking_action_error.html") {
        Ok(t) => t,
        Err(e) => return Some(internal_error_html("internal", &e)),
    };
    Some(Html(
        tmpl.render(context! {
            title => title,
            message => message,
            lang => lang,
        })
        .unwrap_or_else(|e| internal_error_body("template render", &e)),
    ))
}

pub(super) async fn handle_group_booking(
    State(state): State<Arc<AppState>>,
    optional_auth: crate::auth::OptionalAuthUser,
    headers: axum::http::HeaderMap,
    Path((team_slug, slug)): Path<(String, String)>,
    Query(embed): Query<EmbedParams>,
    Form(form): Form<BookForm>,
) -> Response {
    if let Err(resp) = verify_csrf_token(&headers, &form._csrf) {
        return resp;
    }
    // The visitor's language, as the GET form computed it.
    let lang = crate::i18n::detect_from_headers(&headers);
    let captcha_cfg = state.captcha_config.read().await;
    if captcha::verify(&captcha_cfg, form.captcha_token.as_deref())
        .await
        .is_err()
    {
        tracing::warn!("captcha failed on group booking");
        return render_booking_action_error_keys_in(
            &state,
            lang,
            "bae-title-captcha",
            "bae-body-go-back",
        );
    }
    drop(captcha_cfg);
    // Rate limit by IP
    let client_ip = client_ip_for_rate_limit(&headers);
    if state.booking_limiter.check_limited(&client_ip).await {
        tracing::warn!(ip = %client_ip, "rate limited");
        return Html(crate::i18n::translate(
            lang,
            "error-too-many-bookings",
            None,
        ))
        .into_response();
    }

    if let Err(e) = validate_booking_input(&form.name, &form.email, &form.notes) {
        return render_booking_action_error_keys_in(&state, lang, "bae-title-invalid-booking", e);
    }

    let et: Option<(String, String, String, i32, i32, i32, i32, i32, String, Option<String>, String, Option<i32>, String, i32, String, Option<String>)> = sqlx::query_as(
        "SELECT et.id, et.slug, et.title, et.duration_min, et.buffer_before, et.buffer_after, et.min_notice_min, et.requires_confirmation, et.location_type, et.location_value, et.team_id, et.reminder_minutes, et.visibility, et.max_additional_guests, t.visibility, t.invite_token
         FROM event_types et
         JOIN teams t ON t.id = et.team_id
         WHERE t.slug = ? AND et.slug = ? AND et.enabled = 1",
    )
    .bind(&team_slug)
    .bind(&slug)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    let (
        et_id,
        _et_slug,
        et_title,
        duration,
        buffer_before,
        buffer_after,
        min_notice,
        requires_confirmation,
        loc_type,
        loc_value,
        team_id,
        reminder_min,
        visibility,
        max_additional_guests,
        team_visibility,
        team_invite_token,
    ) = match et {
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
    let needs_approval = requires_confirmation != 0;
    let sms_phone_mode: String =
        sqlx::query_scalar("SELECT sms_phone_mode FROM event_types WHERE id = ?")
            .bind(&et_id)
            .fetch_optional(&state.pool)
            .await
            .unwrap_or(None)
            .unwrap_or_else(|| crate::sms::phone_mode::OFF.to_string());
    let scheduling_mode: String =
        sqlx::query_scalar("SELECT scheduling_mode FROM event_types WHERE id = ?")
            .bind(&et_id)
            .fetch_optional(&state.pool)
            .await
            .unwrap_or(None)
            .unwrap_or_else(|| "round_robin".to_string());
    let is_collective = scheduling_mode == "collective";

    // Parse additional guests
    let additional_attendees = match parse_additional_guests(
        &form.additional_guests,
        max_additional_guests,
        &form.email,
        lang,
    ) {
        Ok(emails) => emails,
        Err(e) => {
            return render_booking_action_error_in(
                &state,
                lang,
                &crate::i18n::translate(lang, "bae-title-invalid-booking", None),
                &e,
            )
        }
    };

    // Validate access
    if visibility == "private" || visibility == "internal" {
        let token = match &form.invite_token {
            Some(t) if !t.is_empty() => t,
            _ => {
                return Html(crate::i18n::translate(lang, "error-invite-required", None))
                    .into_response()
            }
        };
        let invite: Option<(Option<String>, i32, i32)> = sqlx::query_as(
            "SELECT expires_at, max_uses, used_count FROM booking_invites WHERE token = ? AND event_type_id = ?",
        )
        .bind(token)
        .bind(&et_id)
        .fetch_optional(&state.pool)
        .await
        .unwrap_or(None);

        match invite {
            None => {
                return Html(crate::i18n::translate(lang, "error-invite-invalid", None))
                    .into_response()
            }
            Some((expires_at, max_uses, used_count)) => {
                if let Some(exp) = &expires_at {
                    if exp < &chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string() {
                        return Html(crate::i18n::translate(lang, "error-invite-expired", None))
                            .into_response();
                    }
                }
                if used_count >= max_uses {
                    return Html(crate::i18n::translate(lang, "error-invite-used", None))
                        .into_response();
                }
            }
        }
    } else if team_visibility == "private" {
        // Public event on a private team: logged-in team members and global
        // admins bypass the team invite token requirement (their team
        // membership is the access). Private/internal events above already
        // returned, so this branch is public-only.
        let viewer_is_member_or_admin = match &optional_auth.user {
            Some(u) if u.role == "admin" => true,
            Some(u) => is_team_member(&state.pool, &u.id, &team_id).await,
            None => false,
        };
        if !viewer_is_member_or_admin {
            let valid = matches!((&team_invite_token, &form.invite_token), (Some(expected), Some(provided)) if !provided.is_empty() && provided == expected);
            if !valid {
                return Html(crate::i18n::translate(
                    lang,
                    "error-event-type-not-found",
                    None,
                ))
                .into_response();
            }
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
    let guest_timezone = guest_tz.name().to_string();
    let host_tz = get_host_tz(&state.pool, &et_id).await;

    // The URL carries the guest's local date/time. Convert to host-local
    // for availability checks; storage uses the exact UTC endpoints.
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

    if chrono::DateTime::parse_from_rfc3339(&encoded_start)
        .unwrap()
        .with_timezone(&Utc)
        < Utc::now() + Duration::minutes(min_notice as i64)
    {
        return Html(crate::i18n::translate(lang, "error-slot-too-soon", None)).into_response();
    }

    // Rolling booking horizon. The slot list already hides anything past it,
    // but that is advisory: a crafted POST has to be rejected here too.
    let horizon_end = horizon_last_date(
        Utc::now().with_timezone(&host_tz).naive_local(),
        get_booking_horizon(&state.pool, &et_id).await,
    );
    if horizon_end.is_some_and(|last| slot_start.date() > last) {
        return Html(crate::i18n::translate(
            lang,
            "error-slot-beyond-horizon",
            None,
        ))
        .into_response();
    }

    let id = uuid::Uuid::new_v4().to_string();
    let uid = format!("{}@calrs", uuid::Uuid::new_v4());
    let cancel_token = uuid::Uuid::new_v4().to_string();
    let reschedule_token = uuid::Uuid::new_v4().to_string();
    let (start_at, end_at) = (encoded_start, encoded_end);
    let guest_end_time = guest_local_end.time().format("%H:%M").to_string();

    let initial_status = if needs_approval {
        "pending"
    } else {
        "confirmed"
    };
    let confirm_token: Option<String> = if needs_approval {
        Some(uuid::Uuid::new_v4().to_string())
    } else {
        None
    };

    // Start a transaction to ensure atomicity of availability check + insert.
    let mut tx = match state.pool.begin().await {
        Ok(tx) => tx,
        Err(e) => {
            return internal_error_response("database query", &e);
        }
    };

    // Resolve the host side. Round-robin: pick one available member and
    // assign the booking to them. Collective (#147): the booking belongs to
    // the whole team — every eligible member must be free, assigned_user_id
    // stays NULL (whole-slot exclusive under idx_bookings_no_overlap), and
    // write-back + host emails fan out to every member.
    let buf_start = check_start - Duration::minutes(buffer_before as i64);
    let buf_end = check_end + Duration::minutes(buffer_after as i64);
    let (assigned_user_id, host_name, host_email, member_contacts) = if is_collective {
        let members: Vec<(String, String, String, Option<String>)> = sqlx::query_as(
            "SELECT u.id, u.name, COALESCE(u.booking_email, u.email), u.language FROM users u
             JOIN team_members tm ON tm.user_id = u.id
             LEFT JOIN event_type_member_weights etw
               ON etw.user_id = u.id AND etw.event_type_id = ?
             WHERE tm.team_id = ? AND u.enabled = 1 AND COALESCE(etw.weight, 1) > 0
             ORDER BY u.name",
        )
        .bind(&et_id)
        .bind(&team_id)
        .fetch_all(&state.pool)
        .await
        .unwrap_or_default();
        if members.is_empty() {
            let _ = tx.rollback().await;
            return Html(crate::i18n::translate(
                lang,
                "error-no-members-available",
                None,
            ))
            .into_response();
        }
        // Submit-time re-check, mirroring pick_group_member: the slot grid
        // may be stale by the time the form is submitted.
        for (member_id, _, _, _) in &members {
            let mut busy = fetch_busy_times_for_user(
                &state.pool,
                member_id,
                buf_start,
                buf_end,
                host_tz,
                Some(&et_id),
            )
            .await;
            busy.extend(
                user_avail_as_busy(&state.pool, member_id, buf_start, buf_end, host_tz).await,
            );
            if has_conflict(&busy, buf_start, buf_end) {
                let _ = tx.rollback().await;
                return Html(crate::i18n::translate(
                    lang,
                    "error-no-members-available",
                    None,
                ))
                .into_response();
            }
        }
        let joined = members
            .iter()
            .map(|(_, name, _, _)| name.as_str())
            .collect::<Vec<_>>()
            .join(" & ");
        let first_email = members[0].2.clone();
        (None::<String>, joined, first_email, members)
    } else {
        let assigned = pick_group_member(
            &state.pool,
            &team_id,
            &et_id,
            check_start,
            check_end,
            slot_start,
            buffer_before,
            buffer_after,
            host_tz,
        )
        .await;
        match assigned {
            Some((member_id, name, email)) => (Some(member_id), name, email, Vec::new()),
            None => {
                let _ = tx.rollback().await;
                return Html(crate::i18n::translate(
                    lang,
                    "error-no-members-available",
                    None,
                ))
                .into_response();
            }
        }
    };

    // Check booking frequency limits
    if would_exceed_frequency_limit(&state.pool, &et_id, slot_start, assigned_user_id.as_deref())
        .await
    {
        let _ = tx.rollback().await;
        return render_booking_action_error_keys_in(
            &state,
            lang,
            "bae-title-unavailable",
            "bae-body-unavailable",
        );
    }

    // The time must be one the slot page offers. Weekly rules, date
    // overrides, interval alignment and first_slot_only are only enforced
    // by recomputing the page's own slots; the checks above just give the
    // common refusals their own message.
    let window = offered_slot_window(host_tz, slot_start);
    let offered_busy = team_busy_source(
        &state.pool,
        &et_id,
        &team_id,
        is_collective,
        window.busy_start,
        window.busy_end,
        host_tz,
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
        None,
        slot_start,
    )
    .await
    {
        let _ = tx.rollback().await;
        return Html(crate::i18n::translate(lang, "error-slot-unavailable", None)).into_response();
    }

    // Required shared resources: verify availability and pick one in
    // round-robin mode. The busy reads run on separate pool connections and
    // cannot see uncommitted inserts, so a process-wide lock, held from the
    // check until the booking row is committed, is what serializes
    // concurrent bookings over shared resources. Feeds are refreshed
    // before taking the lock (failed fetches back off via last_synced_at).
    let resource_ids: Vec<String> = crate::resources::resources_for_event_type(&state.pool, &et_id)
        .await
        .into_iter()
        .map(|r| r.id)
        .collect();
    let resource_guard = if resource_ids.is_empty() {
        None
    } else {
        crate::resources::sync_if_stale(&state.pool, &resource_ids).await;
        Some(crate::resources::booking_lock().await)
    };
    let assigned_resource_id = match crate::resources::check_and_pick(
        &state.pool,
        &et_id,
        check_start,
        check_end,
        host_tz,
        None,
    )
    .await
    {
        crate::resources::ResourceCheck::Busy => {
            let _ = tx.rollback().await;
            return Html(crate::i18n::translate(lang, "error-slot-unavailable", None))
                .into_response();
        }
        crate::resources::ResourceCheck::Free { assigned } => assigned,
        crate::resources::ResourceCheck::NoResources => None,
    };

    let sms_default_country: String = crate::sms::default_country_code(&state.pool).await;
    let phone_to_store = match resolve_guest_phone(
        &sms_phone_mode,
        form.phone.as_deref(),
        &sms_default_country,
        lang,
    ) {
        Ok(value) => value,
        Err((title, message)) => {
            return render_booking_action_error_in(&state, lang, &title, &message);
        }
    };

    match crate::booking_time::legacy_slot_taken(
        &state.pool,
        &et_id,
        assigned_user_id.as_deref(),
        &start_at,
        "",
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
    // A collective booking's organizer comes from the roster, which can
    // change before the next push: store the one this booking was created
    // with (#161). Round-robin resolves through the stable assigned id.
    let (organizer_user_id, organizer_name, organizer_email) = match member_contacts.first() {
        Some((first_id, _, _, _)) => (
            Some(first_id.clone()),
            Some(host_name.clone()),
            Some(host_email.clone()),
        ),
        None => (None, None, None),
    };
    let insert_result = sqlx::query(
        "INSERT INTO bookings (time_version, id, event_type_id, uid, guest_name, guest_email, guest_timezone, notes, start_at, end_at, status, cancel_token, reschedule_token, assigned_user_id, confirm_token, language, assigned_resource_id, guest_phone, organizer_user_id, organizer_name, organizer_email)
         VALUES (1, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(&et_id)
    .bind(&uid)
    .bind(&form.name)
    .bind(&form.email)
    .bind(&guest_timezone)
    .bind(&form.notes)
    .bind(&start_at)
    .bind(&end_at)
    .bind(initial_status)
    .bind(&cancel_token)
    .bind(&reschedule_token)
    .bind(&assigned_user_id)
    .bind(&confirm_token)
    // The language the form was shown in, resolved exactly as the GET
    // handler for this route does, so the page, the emails and the SMS
    // always agree (#209).
    .bind(lang)
    .bind(&assigned_resource_id)
    .bind(&phone_to_store)
    .bind(&organizer_user_id)
    .bind(&organizer_name)
    .bind(&organizer_email)
    .execute(&mut *tx)
    .await;

    match insert_result {
        Ok(_) => {
            if needs_approval {
                stamp_pending_deadline(&mut tx, &id).await;
            }
        }
        Err(e) => {
            let _ = tx.rollback().await;
            if e.to_string().contains("UNIQUE constraint failed") {
                return Html(crate::i18n::translate(lang, "error-slot-unavailable", None))
                    .into_response();
            }
            return internal_error_response("database query", &e);
        }
    }

    // Insert additional attendees
    for attendee_email in &additional_attendees {
        let attendee_id = uuid::Uuid::new_v4().to_string();
        let _ =
            sqlx::query("INSERT INTO booking_attendees (id, booking_id, email) VALUES (?, ?, ?)")
                .bind(&attendee_id)
                .bind(&id)
                .bind(attendee_email)
                .execute(&mut *tx)
                .await;
    }

    if let Err(e) = tx.commit().await {
        if e.to_string().contains("UNIQUE constraint failed") {
            return Html(crate::i18n::translate(lang, "error-slot-unavailable", None))
                .into_response();
        }
        return internal_error_response("database query", &e);
    }
    drop(resource_guard);

    tracing::info!(booking_id = %id, event_type = %slug, guest = %form.email, "booking created");

    // Increment invite used_count if this was an invite-based booking
    if visibility == "private" || visibility == "internal" {
        if let Some(token) = &form.invite_token {
            let _ = sqlx::query("UPDATE booking_invites SET used_count = used_count + 1 WHERE token = ? AND event_type_id = ?")
                .bind(token)
                .bind(&et_id)
                .execute(&state.pool)
                .await;
        }
    }

    // Build BookingDetails once. CalDAV push, watcher notifications, and email
    // sends all need it, and CalDAV push must run independently of SMTP.
    let location_display = resolve_booking_location(
        &state,
        &id,
        &et_id,
        assigned_user_id.as_deref(),
        loc_value.as_deref(),
        &form.name,
        &form.email,
        !needs_approval,
    )
    .await;
    let details = crate::email::BookingDetails {
        utc_times: crate::booking_time::ics_times(&start_at, &end_at),
        event_title: et_title.clone(),
        date: form.date.clone(),
        start_time: form.time.clone(),
        end_time: guest_end_time.clone(),
        guest_name: form.name.clone(),
        guest_email: form.email.clone(),
        guest_timezone: guest_timezone.clone(),
        host_name: host_name.clone(),
        host_email: host_email.clone(),
        uid: uid.clone(),
        notes: form.notes.clone(),
        guest_phone: phone_to_store.clone(),
        location: location_display.clone(),
        reminder_minutes: reminder_min,
        additional_attendees: additional_attendees.clone(),
        guest_language: Some(lang.to_string()),
        host_language: match assigned_user_id.as_deref() {
            Some(member) => crate::email::user_language(&state.pool, member).await,
            None => None,
        },
        host_timezone: host_tz.name().to_string(),
        resource_name: booking_resource_label(&state.pool, &uid).await,
        sequence: 0,
        attendee_languages: Vec::new(),
    };

    // For confirmed bookings, push to CalDAV and notify watchers regardless of
    // SMTP availability. notify_watchers self-gates on SMTP for the email part.
    // caldav_push_booking resolves the real target(s) from the booking row:
    // the assigned member, or every member for collective.
    let push_fallback_user = assigned_user_id
        .clone()
        .unwrap_or_else(|| member_contacts[0].0.clone());
    if !needs_approval {
        caldav_push_booking(
            &state.pool,
            &state.secret_key,
            &push_fallback_user,
            &uid,
            &details,
        )
        .await;
        resource_push_booking(&state.pool, &state.secret_key, &uid, &details).await;
        notify_watchers(
            &state.pool,
            &state.secret_key,
            &id,
            &et_id,
            &host_name,
            &details,
        )
        .await;
    }

    let (cancel_notice_min, reschedule_notice_min) =
        fetch_event_type_notice_minutes(&state.pool, &et_id).await;

    // Send emails if SMTP is configured.
    if let Ok(Some(smtp_config)) =
        crate::email::load_smtp_config(&state.pool, &state.secret_key).await
    {
        let base_url = crate::settings::base_url();
        let guest_cancel_url = base_url.as_ref().map(|base| {
            format!(
                "{}/booking/cancel/{}",
                base.trim_end_matches('/'),
                cancel_token
            )
        });
        let guest_reschedule_url = base_url.as_ref().map(|base| {
            format!(
                "{}/booking/reschedule/{}",
                base.trim_end_matches('/'),
                reschedule_token
            )
        });

        // Collective: every member is a host and gets the host-side email;
        // the guest-facing emails keep the joined team contact (#147).
        let host_recipients = if is_collective {
            member_contacts
                .iter()
                .map(|(_, name, email, language)| {
                    (
                        name.clone(),
                        email.clone(),
                        crate::email::supported_language(language.clone()),
                    )
                })
                .collect()
        } else {
            vec![(
                details.host_name.clone(),
                details.host_email.clone(),
                details.host_language.clone(),
            )]
        };

        send_new_booking_emails(NewBookingEmails {
            smtp_config,
            details,
            host_recipients,
            booking_id: id.clone(),
            needs_approval,
            confirm_token: confirm_token.clone(),
            base_url,
            guest_cancel_url,
            guest_reschedule_url,
            cancel_notice_min,
            reschedule_notice_min,
        });
    }

    // Send booking SMS independently of the email/host notification path.
    // Pending bookings are notified when they are approved.
    notify_new_booking_sms(
        &state,
        crate::sms::collects_phone(&sms_phone_mode),
        phone_to_store.as_deref(),
        needs_approval,
        &et_title,
        &form.date,
        &form.time,
        &guest_timezone,
        lang,
    )
    .await;

    let date_label = crate::i18n::format_long_date(date, lang);

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
            guest_email => form.email,
            notes => form.notes,
            pending => needs_approval,
            location_type => loc_type,
            location_value => location_display,
            additional_attendees => additional_attendees,
            ics_url => format!("/booking/ics/{}", cancel_token),
            cancel_notice_min => cancel_notice_min,
            reschedule_notice_min => reschedule_notice_min,
            company_link => state.company_link.read().await.clone(),
            lang => lang,
            embed => embed.is_embedded(),
            embed_brand => embed.brand_css().unwrap_or_default(),
            embed_theme => embed.theme.as_deref().unwrap_or(""),
            embed_qs_amp => embed.query_suffix(true),
            embed_qs_q => embed.query_suffix(false),
        })
        .unwrap_or_else(|e| internal_error_body("template render", &e));

    html_response(rendered, &embed)
}

async fn handle_dynamic_group_booking(
    state: &AppState,
    headers: &axum::http::HeaderMap,
    combined_username: &str,
    slug: &str,
    form: &BookForm,
) -> Response {
    // Rate limit by IP
    let client_ip = client_ip_for_rate_limit(headers);
    if state.booking_limiter.check_limited(&client_ip).await {
        tracing::warn!(ip = %client_ip, "rate limited");
        let lang = crate::i18n::detect_from_headers(headers);
        return Html(crate::i18n::translate(
            lang,
            "error-too-many-bookings",
            None,
        ))
        .into_response();
    }

    // Past the gates: the visitor's language, as the GET form computed it.
    let lang = crate::i18n::detect_from_headers(headers);
    if let Err(e) = validate_booking_input(&form.name, &form.email, &form.notes) {
        return render_booking_action_error_keys_in(state, lang, "bae-title-invalid-booking", e);
    }

    let usernames = match parse_dynamic_group_usernames(combined_username, lang) {
        Ok(u) => u,
        Err(e) => {
            return render_booking_action_error_in(
                state,
                lang,
                &crate::i18n::translate(lang, "bae-title-invalid-link", None),
                &e,
            )
        }
    };
    let dg_users = match validate_dynamic_group_users(&state.pool, &usernames, lang).await {
        Ok(u) => u,
        Err(e) => {
            return render_booking_action_error_in(
                state,
                lang,
                &crate::i18n::translate(lang, "bae-title-invalid-link", None),
                &e,
            )
        }
    };

    let owner_username = &usernames[0];
    let et: Option<(String, String, String, i32, i32, i32, i32, i32, String, Option<String>, String, Option<i32>, String, i32, String)> = sqlx::query_as(
        "SELECT et.id, et.slug, et.title, et.duration_min, et.buffer_before, et.buffer_after, et.min_notice_min, et.requires_confirmation, et.location_type, et.location_value, u.id, et.reminder_minutes, et.visibility, et.max_additional_guests, et.sms_phone_mode
         FROM event_types et
         JOIN accounts a ON a.id = et.account_id
         JOIN users u ON u.id = a.user_id
         WHERE u.username = ? AND et.slug = ? AND et.enabled = 1 AND u.enabled = 1",
    )
    .bind(owner_username)
    .bind(slug)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    let (
        et_id,
        _et_slug,
        et_title,
        duration,
        buffer_before,
        buffer_after,
        min_notice,
        requires_confirmation,
        loc_type,
        loc_value,
        owner_user_id,
        reminder_min,
        visibility,
        max_additional_guests,
        sms_phone_mode,
    ) = match et {
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

    if visibility != "public" {
        return Html(crate::i18n::translate(
            lang,
            "error-dynamic-group-public-only",
            None,
        ))
        .into_response();
    }

    if loc_type == meeting::LOCATION_TYPE_GOOGLE_MEET
        && !crate::google_meet::user_has_google_writeback(&state.pool, &owner_user_id).await
    {
        let title = crate::i18n::translate(lang, "google-meet-unavailable-title", None);
        let message = crate::i18n::translate(lang, "google-meet-dynamic-group-unavailable", None);
        return render_booking_action_error_in(state, lang, &title, &message);
    }

    let needs_approval = requires_confirmation != 0;

    // Parse additional guests
    let additional_attendees = match parse_additional_guests(
        &form.additional_guests,
        max_additional_guests,
        &form.email,
        lang,
    ) {
        Ok(emails) => emails,
        Err(e) => {
            return render_booking_action_error_in(
                state,
                lang,
                &crate::i18n::translate(lang, "bae-title-invalid-booking", None),
                &e,
            )
        }
    };

    let date = match NaiveDate::parse_from_str(&form.date, "%Y-%m-%d") {
        Ok(d) => d,
        Err(_) => {
            return Html(crate::i18n::translate(lang, "error-invalid-date", None)).into_response()
        }
    };
    if let Err(e) = validate_date_not_too_far(date) {
        return render_booking_action_error_keys_in(state, lang, "bae-title-invalid-booking", e);
    }
    let start_time = match NaiveTime::parse_from_str(&form.time, "%H:%M") {
        Ok(t) => t,
        Err(_) => {
            return Html(crate::i18n::translate(lang, "error-invalid-time", None)).into_response()
        }
    };

    let guest_tz = parse_guest_tz(form.tz.as_deref());
    let guest_timezone = guest_tz.name().to_string();
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

    // Rolling booking horizon. The slot list already hides anything past it,
    // but that is advisory: a crafted POST has to be rejected here too.
    let horizon_end = horizon_last_date(
        Utc::now().with_timezone(&host_tz).naive_local(),
        get_booking_horizon(&state.pool, &et_id).await,
    );
    if horizon_end.is_some_and(|last| slot_start.date() > last) {
        return Html(crate::i18n::translate(
            lang,
            "error-slot-beyond-horizon",
            None,
        ))
        .into_response();
    }

    let buf_start = check_start - Duration::minutes(buffer_before as i64);
    let buf_end = check_end + Duration::minutes(buffer_after as i64);

    // Check availability for ALL participants
    for (i, (uid, uname, _, _, _)) in dg_users.iter().enumerate() {
        let et_filter = if i == 0 { Some(et_id.as_str()) } else { None };
        let mut busy =
            fetch_busy_times_for_user(&state.pool, uid, buf_start, buf_end, host_tz, et_filter)
                .await;
        if i > 0 {
            ensure_user_avail_seeded(&state.pool, uid).await;
            busy.extend(user_avail_as_busy(&state.pool, uid, buf_start, buf_end, host_tz).await);
        }
        if has_conflict(&busy, buf_start, buf_end) {
            return Html(tr1(
                lang,
                "error-slot-unavailable-member",
                "username",
                uname,
            ))
            .into_response();
        }
    }

    // Check booking frequency limits. The dynamic-group flow doesn't pin a
    // single assignee on the booking, so per-member caps fall back to
    // event-type-wide here.
    if would_exceed_frequency_limit(&state.pool, &et_id, slot_start, None).await {
        return render_booking_action_error_keys_in(
            state,
            lang,
            "bae-title-unavailable",
            "bae-body-unavailable",
        );
    }

    // The time must be one the slot page offers. Weekly rules, date
    // overrides, interval alignment and first_slot_only are only enforced
    // by recomputing the page's own slots; the checks above just give the
    // common refusals their own message.
    let window = offered_slot_window(host_tz, slot_start);
    let user_ids: Vec<String> = dg_users.iter().map(|(uid, ..)| uid.clone()).collect();
    let offered_busy = dynamic_group_busy_source(
        &state.pool,
        &et_id,
        &user_ids,
        window.busy_start,
        window.busy_end,
        host_tz,
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
        None,
        slot_start,
    )
    .await
    {
        return Html(crate::i18n::translate(lang, "error-slot-unavailable", None)).into_response();
    }

    let id = uuid::Uuid::new_v4().to_string();
    let uid = format!("{}@calrs", uuid::Uuid::new_v4());
    let cancel_token = uuid::Uuid::new_v4().to_string();
    let reschedule_token = uuid::Uuid::new_v4().to_string();
    let (start_at, end_at) = (encoded_start, encoded_end);

    let initial_status = if needs_approval {
        "pending"
    } else {
        "confirmed"
    };
    let confirm_token: Option<String> = if needs_approval {
        Some(uuid::Uuid::new_v4().to_string())
    } else {
        None
    };

    let mut tx = match state.pool.begin().await {
        Ok(tx) => tx,
        Err(e) => return internal_error_response("database query", &e),
    };

    // Required shared resources: verify availability and pick one in
    // round-robin mode. The busy reads run on separate pool connections and
    // cannot see uncommitted inserts, so a process-wide lock, held from the
    // check until the booking row is committed, is what serializes
    // concurrent bookings over shared resources. Feeds are refreshed
    // before taking the lock (failed fetches back off via last_synced_at).
    let resource_ids: Vec<String> = crate::resources::resources_for_event_type(&state.pool, &et_id)
        .await
        .into_iter()
        .map(|r| r.id)
        .collect();
    let resource_guard = if resource_ids.is_empty() {
        None
    } else {
        crate::resources::sync_if_stale(&state.pool, &resource_ids).await;
        Some(crate::resources::booking_lock().await)
    };
    let assigned_resource_id = match crate::resources::check_and_pick(
        &state.pool,
        &et_id,
        check_start,
        check_end,
        host_tz,
        None,
    )
    .await
    {
        crate::resources::ResourceCheck::Busy => {
            let _ = tx.rollback().await;
            return Html(crate::i18n::translate(lang, "error-slot-unavailable", None))
                .into_response();
        }
        crate::resources::ResourceCheck::Free { assigned } => assigned,
        crate::resources::ResourceCheck::NoResources => None,
    };

    let sms_default_country_dg: String = crate::sms::default_country_code(&state.pool).await;
    let phone_to_store = match resolve_guest_phone(
        &sms_phone_mode,
        form.phone.as_deref(),
        &sms_default_country_dg,
        lang,
    ) {
        Ok(value) => value,
        Err((title, message)) => {
            return render_booking_action_error_in(state, lang, &title, &message);
        }
    };

    match crate::booking_time::legacy_slot_taken(&state.pool, &et_id, None, &start_at, "").await {
        Ok(false) => {}
        Ok(true) => {
            return Html(crate::i18n::translate(lang, "error-slot-unavailable", None))
                .into_response()
        }
        Err(e) => return internal_error_response("booking conflict check", &e),
    }
    // The organizer is the first user in the link with every name joined,
    // which no later lookup can rebuild (the event type is the owner's
    // personal one): store it so approvals and reschedules push the same
    // ORGANIZER the confirmation did (#161).
    let owner_email = dg_users[0].3.clone();
    let host_name = dg_users
        .iter()
        .map(|(_, _, name, _, _)| name.as_str())
        .collect::<Vec<_>>()
        .join(" & ");
    let insert_result = sqlx::query(
        "INSERT INTO bookings (time_version, id, event_type_id, uid, guest_name, guest_email, guest_timezone, notes, start_at, end_at, status, cancel_token, reschedule_token, confirm_token, language, assigned_resource_id, guest_phone, organizer_user_id, organizer_name, organizer_email)
         VALUES (1, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(&et_id)
    .bind(&uid)
    .bind(&form.name)
    .bind(&form.email)
    .bind(&guest_timezone)
    .bind(&form.notes)
    .bind(&start_at)
    .bind(&end_at)
    .bind(initial_status)
    .bind(&cancel_token)
    .bind(&reschedule_token)
    .bind(&confirm_token)
    // The language the form was shown in, resolved exactly as the GET
    // handler for this route does, so the page, the emails and the SMS
    // always agree (#209).
    .bind(lang)
    .bind(&assigned_resource_id)
    .bind(&phone_to_store)
    .bind(&dg_users[0].0)
    .bind(&host_name)
    .bind(&owner_email)
    .execute(&mut *tx)
    .await;

    match insert_result {
        Ok(_) => {
            if needs_approval {
                stamp_pending_deadline(&mut tx, &id).await;
            }
        }
        Err(e) => {
            let _ = tx.rollback().await;
            if e.to_string().contains("UNIQUE constraint failed") {
                return Html(crate::i18n::translate(lang, "error-slot-unavailable", None))
                    .into_response();
            }
            return internal_error_response("database query", &e);
        }
    }

    // Combine co-participant emails with guest-provided additional attendees
    let co_participant_emails: Vec<String> = dg_users
        .iter()
        .skip(1)
        .map(|(_, _, _, email, _)| email.clone())
        .collect();
    let all_additional: Vec<String> = co_participant_emails
        .iter()
        .chain(additional_attendees.iter())
        .cloned()
        .collect();

    // Insert all additional attendees (co-participants + guest-provided)
    for attendee_email in &all_additional {
        let attendee_id = uuid::Uuid::new_v4().to_string();
        let _ =
            sqlx::query("INSERT INTO booking_attendees (id, booking_id, email) VALUES (?, ?, ?)")
                .bind(&attendee_id)
                .bind(&id)
                .bind(attendee_email)
                .execute(&mut *tx)
                .await;
    }

    if let Err(e) = tx.commit().await {
        if e.to_string().contains("UNIQUE constraint failed") {
            return Html(crate::i18n::translate(lang, "error-slot-unavailable", None))
                .into_response();
        }
        return internal_error_response("database query", &e);
    }
    drop(resource_guard);

    tracing::info!(booking_id = %id, event_type = %slug, guest = %form.email, dynamic_group = %combined_username, "dynamic group booking created");

    // Build BookingDetails once. CalDAV push and email send both need it,
    // and CalDAV push must run independently of whether SMTP is configured.
    let location_display = resolve_booking_location(
        state,
        &id,
        &et_id,
        Some(&owner_user_id),
        loc_value.as_deref(),
        &form.name,
        &form.email,
        !needs_approval,
    )
    .await;
    let details = crate::email::BookingDetails {
        utc_times: crate::booking_time::ics_times(&start_at, &end_at),
        event_title: et_title.clone(),
        date: form.date.clone(),
        start_time: form.time.clone(),
        end_time: guest_end_time.clone(),
        guest_name: form.name.clone(),
        guest_email: form.email.clone(),
        guest_timezone: guest_timezone.clone(),
        host_name: host_name.clone(),
        host_email: owner_email,
        uid: uid.clone(),
        notes: form.notes.clone(),
        guest_phone: phone_to_store.clone(),
        location: location_display.clone(),
        reminder_minutes: reminder_min,
        additional_attendees: all_additional.clone(),
        guest_language: Some(lang.to_string()),
        host_language: crate::email::user_language(&state.pool, &owner_user_id).await,
        host_timezone: host_tz.name().to_string(),
        resource_name: booking_resource_label(&state.pool, &uid).await,
        sequence: 0,
        attendee_languages: Vec::new(),
    };

    // Push confirmed bookings to the owner's CalDAV regardless of SMTP.
    // ICS includes co-participants as ATTENDEEs.
    if !needs_approval {
        caldav_push_booking(
            &state.pool,
            &state.secret_key,
            &owner_user_id,
            &uid,
            &details,
        )
        .await;
        resource_push_booking(&state.pool, &state.secret_key, &uid, &details).await;
    }

    let (cancel_notice_min, reschedule_notice_min) =
        fetch_event_type_notice_minutes(&state.pool, &et_id).await;

    // Send emails if SMTP is configured.
    if let Ok(Some(smtp_config)) =
        crate::email::load_smtp_config(&state.pool, &state.secret_key).await
    {
        let base_url = crate::settings::base_url();
        let guest_cancel_url = base_url.as_ref().map(|base| {
            format!(
                "{}/booking/cancel/{}",
                base.trim_end_matches('/'),
                cancel_token
            )
        });
        let guest_reschedule_url = base_url.as_ref().map(|base| {
            format!(
                "{}/booking/reschedule/{}",
                base.trim_end_matches('/'),
                reschedule_token
            )
        });

        let host_recipients = vec![(
            details.host_name.clone(),
            details.host_email.clone(),
            details.host_language.clone(),
        )];
        send_new_booking_emails(NewBookingEmails {
            smtp_config,
            details,
            host_recipients,
            booking_id: id.clone(),
            needs_approval,
            confirm_token: confirm_token.clone(),
            base_url,
            guest_cancel_url,
            guest_reschedule_url,
            cancel_notice_min,
            reschedule_notice_min,
        });
    }

    let host_display = dg_users
        .iter()
        .map(|(_, _, name, _, _)| name.as_str())
        .collect::<Vec<_>>()
        .join(" & ");
    let date_label = crate::i18n::format_long_date(date, lang);

    let tmpl = match state.templates.get_template("confirmed.html") {
        Ok(t) => t,
        Err(e) => return internal_error_response("internal", &e),
    };
    Html(
        tmpl.render(context! {
            event_title => et_title,
            date_label => date_label,
            time_start => form.time,
            time_end => guest_end_time,
            host_name => host_display,
            guest_email => form.email,
            notes => form.notes,
            pending => needs_approval,
            location_type => loc_type,
            location_value => location_display,
            additional_attendees => all_additional,
            ics_url => format!("/booking/ics/{}", cancel_token),
            cancel_notice_min => cancel_notice_min,
            reschedule_notice_min => reschedule_notice_min,
            company_link => state.company_link.read().await.clone(),
            lang => lang,
        })
        .unwrap_or_else(|e| internal_error_body("template render", &e)),
    )
    .into_response()
}

pub(super) async fn handle_booking_for_user(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Path((username, slug)): Path<(String, String)>,
    Query(embed): Query<EmbedParams>,
    Form(form): Form<BookForm>,
) -> Response {
    if let Err(resp) = verify_csrf_token(&headers, &form._csrf) {
        return resp;
    }
    let captcha_cfg = state.captcha_config.read().await;
    if captcha::verify(&captcha_cfg, form.captcha_token.as_deref())
        .await
        .is_err()
    {
        tracing::warn!("captcha failed on user booking");
        let lang = user_page_lang_lookup(&state.pool, &username, &headers).await;
        return render_booking_action_error_keys_in(
            &state,
            lang,
            "bae-title-captcha",
            "bae-body-go-back",
        );
    }
    drop(captcha_cfg);
    if username.contains('+') {
        return handle_dynamic_group_booking(&state, &headers, &username, &slug, &form)
            .await
            .into_response();
    }
    // Rate limit by IP
    let client_ip = client_ip_for_rate_limit(&headers);
    if state.booking_limiter.check_limited(&client_ip).await {
        tracing::warn!(ip = %client_ip, "rate limited");
        let lang = user_page_lang_lookup(&state.pool, &username, &headers).await;
        return Html(crate::i18n::translate(
            lang,
            "error-too-many-bookings",
            None,
        ))
        .into_response();
    }

    if let Err(e) = validate_booking_input(&form.name, &form.email, &form.notes) {
        let lang = user_page_lang_lookup(&state.pool, &username, &headers).await;
        return render_booking_action_error_keys_in(&state, lang, "bae-title-invalid-booking", e);
    }

    // The page language: the browser's, or this host's own from the row
    // below, as the GET form computed it.
    let visitor = crate::i18n::detect_from_headers(&headers);

    let et: Option<(String, String, i32, i32, i32, i32, i32, String, Option<String>, String, Option<i32>, String, i32, Option<String>, String, String)> = sqlx::query_as(
        "SELECT et.id, et.title, et.duration_min, et.buffer_before, et.buffer_after, et.min_notice_min, et.requires_confirmation, et.location_type, et.location_value, u.id, et.reminder_minutes, et.visibility, et.max_additional_guests, u.language, et.sms_phone_mode, u.public_page_language
         FROM event_types et
         JOIN accounts a ON a.id = et.account_id
         JOIN users u ON u.id = a.user_id
         WHERE u.username = ? AND et.slug = ? AND et.enabled = 1 AND u.enabled = 1",
    )
    .bind(&username)
    .bind(&slug)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    let (
        et_id,
        et_title,
        duration,
        buffer_before,
        buffer_after,
        min_notice,
        requires_confirmation,
        loc_type,
        loc_value,
        host_user_id,
        reminder_min,
        visibility,
        max_additional_guests,
        user_lang,
        sms_phone_mode,
        page_mode,
    ) = match et {
        Some(e) => e,
        None => {
            return Html(crate::i18n::translate(
                visitor,
                "error-event-type-not-found",
                None,
            ))
            .into_response()
        }
    };

    let lang = user_page_lang(user_lang.as_deref(), &page_mode, visitor);
    let needs_approval = requires_confirmation != 0;
    let sms_default_country: String = crate::sms::default_country_code(&state.pool).await;
    let phone_to_store = match resolve_guest_phone(
        &sms_phone_mode,
        form.phone.as_deref(),
        &sms_default_country,
        lang,
    ) {
        Ok(value) => value,
        Err((title, message)) => {
            return render_booking_action_error_in(&state, lang, &title, &message);
        }
    };

    // Parse additional guests
    let additional_attendees = match parse_additional_guests(
        &form.additional_guests,
        max_additional_guests,
        &form.email,
        lang,
    ) {
        Ok(emails) => emails,
        Err(e) => {
            return render_booking_action_error_in(
                &state,
                lang,
                &crate::i18n::translate(lang, "bae-title-invalid-booking", None),
                &e,
            )
        }
    };

    // Validate invite token for private event types
    if visibility == "private" || visibility == "internal" {
        let token = match &form.invite_token {
            Some(t) if !t.is_empty() => t,
            _ => {
                return Html(crate::i18n::translate(lang, "error-invite-required", None))
                    .into_response()
            }
        };
        let invite: Option<(Option<String>, i32, i32)> = sqlx::query_as(
            "SELECT expires_at, max_uses, used_count FROM booking_invites WHERE token = ? AND event_type_id = ?",
        )
        .bind(token)
        .bind(&et_id)
        .fetch_optional(&state.pool)
        .await
        .unwrap_or(None);

        match invite {
            None => {
                return Html(crate::i18n::translate(lang, "error-invite-invalid", None))
                    .into_response()
            }
            Some((expires_at, max_uses, used_count)) => {
                if let Some(exp) = &expires_at {
                    if exp < &chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string() {
                        return Html(crate::i18n::translate(lang, "error-invite-expired", None))
                            .into_response();
                    }
                }
                if used_count >= max_uses {
                    return Html(crate::i18n::translate(lang, "error-invite-used", None))
                        .into_response();
                }
            }
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
    let guest_timezone = guest_tz.name().to_string();
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

    // Rolling booking horizon. The slot list already hides anything past it,
    // but that is advisory: a crafted POST has to be rejected here too.
    let horizon_end = horizon_last_date(
        Utc::now().with_timezone(&host_tz).naive_local(),
        get_booking_horizon(&state.pool, &et_id).await,
    );
    if horizon_end.is_some_and(|last| slot_start.date() > last) {
        return Html(crate::i18n::translate(
            lang,
            "error-slot-beyond-horizon",
            None,
        ))
        .into_response();
    }

    let buf_start = check_start - Duration::minutes(buffer_before as i64);
    let buf_end = check_end + Duration::minutes(buffer_after as i64);

    let id = uuid::Uuid::new_v4().to_string();
    let uid = format!("{}@calrs", uuid::Uuid::new_v4());
    let cancel_token = uuid::Uuid::new_v4().to_string();
    let reschedule_token = uuid::Uuid::new_v4().to_string();
    let (start_at, end_at) = (encoded_start, encoded_end);

    let initial_status = if needs_approval {
        "pending"
    } else {
        "confirmed"
    };
    let confirm_token: Option<String> = if needs_approval {
        Some(uuid::Uuid::new_v4().to_string())
    } else {
        None
    };

    // Start a transaction to ensure atomicity of availability check + insert.
    let mut tx = match state.pool.begin().await {
        Ok(tx) => tx,
        Err(e) => {
            return internal_error_response("database query", &e);
        }
    };

    let busy = fetch_busy_times_for_user(
        &state.pool,
        &host_user_id,
        buf_start,
        buf_end,
        host_tz,
        Some(&et_id),
    )
    .await;
    if has_conflict(&busy, buf_start, buf_end) {
        let _ = tx.rollback().await;
        return Html(crate::i18n::translate(lang, "error-slot-unavailable", None)).into_response();
    }

    // Check booking frequency limits. Personal event-type flows don't have a
    // team assignee, so per-member caps degrade to event-type-wide here.
    if would_exceed_frequency_limit(&state.pool, &et_id, slot_start, None).await {
        let _ = tx.rollback().await;
        return render_booking_action_error_keys_in(
            &state,
            lang,
            "bae-title-unavailable",
            "bae-body-unavailable",
        );
    }

    // The time must be one the slot page offers. Weekly rules, date
    // overrides, interval alignment and first_slot_only are only enforced
    // by recomputing the page's own slots; the checks above just give the
    // common refusals their own message.
    let window = offered_slot_window(host_tz, slot_start);
    let offered_busy = BusySource::Individual(
        fetch_busy_times_for_user(
            &state.pool,
            &host_user_id,
            window.busy_start,
            window.busy_end,
            host_tz,
            Some(&et_id),
        )
        .await,
    );
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
        None,
        slot_start,
    )
    .await
    {
        let _ = tx.rollback().await;
        return Html(crate::i18n::translate(lang, "error-slot-unavailable", None)).into_response();
    }

    // Required shared resources: verify availability and pick one in
    // round-robin mode. The busy reads run on separate pool connections and
    // cannot see uncommitted inserts, so a process-wide lock, held from the
    // check until the booking row is committed, is what serializes
    // concurrent bookings over shared resources. Feeds are refreshed
    // before taking the lock (failed fetches back off via last_synced_at).
    let resource_ids: Vec<String> = crate::resources::resources_for_event_type(&state.pool, &et_id)
        .await
        .into_iter()
        .map(|r| r.id)
        .collect();
    let resource_guard = if resource_ids.is_empty() {
        None
    } else {
        crate::resources::sync_if_stale(&state.pool, &resource_ids).await;
        Some(crate::resources::booking_lock().await)
    };
    let assigned_resource_id = match crate::resources::check_and_pick(
        &state.pool,
        &et_id,
        check_start,
        check_end,
        host_tz,
        None,
    )
    .await
    {
        crate::resources::ResourceCheck::Busy => {
            let _ = tx.rollback().await;
            return Html(crate::i18n::translate(lang, "error-slot-unavailable", None))
                .into_response();
        }
        crate::resources::ResourceCheck::Free { assigned } => assigned,
        crate::resources::ResourceCheck::NoResources => None,
    };

    match crate::booking_time::legacy_slot_taken(&state.pool, &et_id, None, &start_at, "").await {
        Ok(false) => {}
        Ok(true) => {
            return Html(crate::i18n::translate(lang, "error-slot-unavailable", None))
                .into_response()
        }
        Err(e) => return internal_error_response("booking conflict check", &e),
    }
    let insert_result = sqlx::query(
        "INSERT INTO bookings (time_version, id, event_type_id, uid, guest_name, guest_email, guest_timezone, notes, start_at, end_at, status, cancel_token, reschedule_token, confirm_token, language, assigned_resource_id, guest_phone)
         VALUES (1, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(&et_id)
    .bind(&uid)
    .bind(&form.name)
    .bind(&form.email)
    .bind(&guest_timezone)
    .bind(&form.notes)
    .bind(&start_at)
    .bind(&end_at)
    .bind(initial_status)
    .bind(&cancel_token)
    .bind(&reschedule_token)
    .bind(&confirm_token)
    // The language the form was shown in, resolved exactly as the GET
    // handler for this route does, so the page, the emails and the SMS
    // always agree (#209).
    .bind(lang)
    .bind(&assigned_resource_id)
    .bind(&phone_to_store)
    .execute(&mut *tx)
    .await;

    match insert_result {
        Ok(_) => {
            if needs_approval {
                stamp_pending_deadline(&mut tx, &id).await;
            }
        }
        Err(e) => {
            let _ = tx.rollback().await;
            if e.to_string().contains("UNIQUE constraint failed") {
                return Html(crate::i18n::translate(lang, "error-slot-unavailable", None))
                    .into_response();
            }
            return internal_error_response("database query", &e);
        }
    }

    // Insert additional attendees
    for attendee_email in &additional_attendees {
        let attendee_id = uuid::Uuid::new_v4().to_string();
        let _ =
            sqlx::query("INSERT INTO booking_attendees (id, booking_id, email) VALUES (?, ?, ?)")
                .bind(&attendee_id)
                .bind(&id)
                .bind(attendee_email)
                .execute(&mut *tx)
                .await;
    }

    if let Err(e) = tx.commit().await {
        if e.to_string().contains("UNIQUE constraint failed") {
            return Html(crate::i18n::translate(lang, "error-slot-unavailable", None))
                .into_response();
        }
        return internal_error_response("database query", &e);
    }
    drop(resource_guard);

    tracing::info!(booking_id = %id, event_type = %slug, guest = %form.email, "booking created");

    // Increment invite used_count if this was an invite-based booking
    if visibility == "private" || visibility == "internal" {
        if let Some(token) = &form.invite_token {
            let _ = sqlx::query("UPDATE booking_invites SET used_count = used_count + 1 WHERE token = ? AND event_type_id = ?")
                .bind(token)
                .bind(&et_id)
                .execute(&state.pool)
                .await;
        }
    }

    // Build BookingDetails once. CalDAV push and email send both need it,
    // and CalDAV push must run independently of whether SMTP is configured.
    let host: Option<(String, String)> = sqlx::query_as(
        "SELECT u.name, COALESCE(u.booking_email, u.email) FROM users u WHERE u.username = ?",
    )
    .bind(&username)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    let host_user_id: Option<String> =
        sqlx::query_scalar("SELECT id FROM users WHERE username = ?")
            .bind(&username)
            .fetch_optional(&state.pool)
            .await
            .unwrap_or(None);
    let location_display = resolve_booking_location(
        &state,
        &id,
        &et_id,
        host_user_id.as_deref(),
        loc_value.as_deref(),
        &form.name,
        &form.email,
        !needs_approval,
    )
    .await;

    if let Some((host_name, host_email)) = host {
        let details = crate::email::BookingDetails {
            utc_times: crate::booking_time::ics_times(&start_at, &end_at),
            event_title: et_title.clone(),
            date: form.date.clone(),
            start_time: form.time.clone(),
            end_time: guest_end_time.clone(),
            guest_name: form.name.clone(),
            guest_email: form.email.clone(),
            guest_timezone: guest_timezone.clone(),
            host_name,
            host_email,
            uid: uid.clone(),
            notes: form.notes.clone(),
            guest_phone: phone_to_store.clone(),
            location: location_display.clone(),
            reminder_minutes: reminder_min,
            additional_attendees: additional_attendees.clone(),
            guest_language: Some(lang.to_string()),
            // Selected with the event type: the host's saved language.
            host_language: crate::email::supported_language(user_lang.clone()),
            host_timezone: host_tz.name().to_string(),
            resource_name: booking_resource_label(&state.pool, &uid).await,
            sequence: 0,
            attendee_languages: Vec::new(),
        };

        // Push confirmed bookings to CalDAV regardless of SMTP availability.
        if !needs_approval {
            if let Some(uid_user) = host_user_id.as_deref() {
                caldav_push_booking(&state.pool, &state.secret_key, uid_user, &uid, &details).await;
                resource_push_booking(&state.pool, &state.secret_key, &uid, &details).await;
            }
        }

        let (cancel_notice_min, reschedule_notice_min) =
            fetch_event_type_notice_minutes(&state.pool, &et_id).await;

        // Send emails if SMTP is configured.
        if let Ok(Some(smtp_config)) =
            crate::email::load_smtp_config(&state.pool, &state.secret_key).await
        {
            let base_url = crate::settings::base_url();
            let guest_cancel_url = base_url.as_ref().map(|base| {
                format!(
                    "{}/booking/cancel/{}",
                    base.trim_end_matches('/'),
                    cancel_token
                )
            });
            let guest_reschedule_url = base_url.as_ref().map(|base| {
                format!(
                    "{}/booking/reschedule/{}",
                    base.trim_end_matches('/'),
                    reschedule_token
                )
            });

            let host_recipients = vec![(
                details.host_name.clone(),
                details.host_email.clone(),
                details.host_language.clone(),
            )];
            send_new_booking_emails(NewBookingEmails {
                smtp_config,
                details,
                host_recipients,
                booking_id: id.clone(),
                needs_approval,
                confirm_token: confirm_token.clone(),
                base_url,
                guest_cancel_url,
                guest_reschedule_url,
                cancel_notice_min,
                reschedule_notice_min,
            });
        }
    }

    // Send booking SMS independently of the email/host notification path.
    // Pending bookings are notified when they are approved.
    notify_new_booking_sms(
        &state,
        crate::sms::collects_phone(&sms_phone_mode),
        phone_to_store.as_deref(),
        needs_approval,
        &et_title,
        &form.date,
        &form.time,
        &guest_timezone,
        lang,
    )
    .await;

    let host_name: String = sqlx::query_scalar("SELECT name FROM users WHERE username = ?")
        .bind(&username)
        .fetch_optional(&state.pool)
        .await
        .unwrap_or(None)
        .unwrap_or_else(|| "Host".to_string());

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
            guest_email => form.email,
            notes => form.notes,
            pending => needs_approval,
            location_type => loc_type,
            location_value => location_display,
            additional_attendees => additional_attendees,
            ics_url => format!("/booking/ics/{}", cancel_token),
            cancel_notice_min => cancel_notice_min,
            reschedule_notice_min => reschedule_notice_min,
            company_link => state.company_link.read().await.clone(),
            lang => lang,
            embed => embed.is_embedded(),
            embed_brand => embed.brand_css().unwrap_or_default(),
            embed_theme => embed.theme.as_deref().unwrap_or(""),
            embed_qs_amp => embed.query_suffix(true),
            embed_qs_q => embed.query_suffix(false),
        })
        .unwrap_or_else(|e| internal_error_body("template render", &e));

    html_response(rendered, &embed)
}

pub(super) fn validate_booking_input(
    name: &str,
    email: &str,
    notes: &Option<String>,
) -> Result<(), &'static str> {
    let name = name.trim();
    if name.is_empty() || name.len() > 255 {
        return Err("validate-name-length");
    }
    let email = email.trim();
    if email.is_empty() || email.len() > 255 {
        return Err("validate-email-length");
    }
    if !email.contains('@')
        || email
            .rsplit('@')
            .next()
            .is_none_or(|domain| !domain.contains('.'))
    {
        return Err("validate-email-invalid");
    }
    if let Some(notes) = notes {
        if notes.len() > 5000 {
            return Err("validate-notes-length");
        }
    }
    Ok(())
}

pub(super) fn validate_date_not_too_far(date: NaiveDate) -> Result<(), &'static str> {
    let max_date = Utc::now().naive_utc().date() + Duration::days(366);
    if date > max_date {
        return Err("validate-date-too-far");
    }
    Ok(())
}

/// Parse comma-separated additional guest emails, validate format, enforce max count.
/// Returns the list of valid, deduplicated emails (excluding the primary guest email).
pub(super) fn parse_additional_guests(
    raw: &Option<String>,
    max: i32,
    primary_email: &str,
    lang: &str,
) -> Result<Vec<String>, String> {
    let raw = match raw {
        Some(s) if !s.trim().is_empty() => s,
        _ => return Ok(vec![]),
    };
    if max <= 0 {
        return Err(crate::i18n::translate(lang, "guests-not-allowed", None));
    }
    let emails: Vec<String> = raw
        .split(',')
        .map(|s| s.trim().to_lowercase())
        .filter(|s| !s.is_empty())
        .collect();
    if emails.len() > max as usize {
        let mut args = fluent_bundle::FluentArgs::new();
        args.set("max", crate::i18n::number(max as f64));
        return Err(crate::i18n::translate(lang, "guests-too-many", Some(&args)));
    }
    let primary = primary_email.trim().to_lowercase();
    let mut seen = std::collections::HashSet::new();
    let mut result = Vec::new();
    for email in &emails {
        if email == &primary {
            continue; // skip if same as primary guest
        }
        if !email.contains('@')
            || email
                .rsplit('@')
                .next()
                .is_none_or(|domain| !domain.contains('.'))
        {
            return Err(tr1(lang, "guests-invalid-email", "email", email));
        }
        if seen.insert(email.clone()) {
            result.push(email.clone());
        }
    }
    Ok(result)
}

#[derive(Deserialize)]
pub(super) struct BookForm {
    _csrf: Option<String>,
    date: String,
    time: String,
    name: String,
    email: String,
    notes: Option<String>,
    #[serde(default)]
    tz: Option<String>,
    #[serde(default)]
    invite_token: Option<String>,
    #[serde(default)]
    additional_guests: Option<String>,
    #[serde(rename = "cap-token", default)]
    captcha_token: Option<String>,
    /// Optional guest phone number (E.164, e.g. `+15551234567`). Only shown
    /// on the booking page / used for SMS when the event type has
    /// `sms_phone_mode` set; ignored otherwise even if posted.
    #[serde(default)]
    phone: Option<String>,
}

/// Whether `user` may put an event type into an SMS-sending mode.
///
/// SMS spends instance-wide credit an admin paid for and the recipient is
/// guest-controlled, so the default matches shared resources: admins only,
/// with an explicit opt-out for instances where everyone is trusted.
pub(super) async fn can_enable_sms(pool: &SqlitePool, user: &crate::models::User) -> bool {
    if user.role == "admin" {
        return true;
    }
    sqlx::query_scalar::<_, bool>(
        "SELECT sms_allow_all_users FROM auth_config WHERE id = 'singleton'",
    )
    .fetch_optional(pool)
    .await
    .unwrap_or(None)
    .unwrap_or(false)
}

/// The phone mode to store for an event type, given what the form posted.
///
/// A user who may not enable SMS never has their post honoured: on create the
/// event type stays `off`, and on edit the stored value is carried forward, so
/// a member editing an admin's event type cannot silently turn SMS off either.
pub(super) async fn resolve_phone_mode(
    pool: &SqlitePool,
    user: &crate::models::User,
    posted: Option<&str>,
    event_type_id: Option<&str>,
) -> String {
    if can_enable_sms(pool, user).await {
        return crate::sms::phone_mode::parse(posted.unwrap_or("")).to_string();
    }

    match event_type_id {
        Some(id) => stored_sms_phone_mode(pool, id).await,
        None => crate::sms::phone_mode::OFF.to_string(),
    }
}

/// Resolve the guest's phone number for storage, honouring the event type's
/// phone mode. `Err` carries the localised (title, message) pair to render.
///
/// The booking form validates the same rules client-side with
/// `setCustomValidity`, so a guest with JavaScript on gets an inline field
/// error instead of ever reaching this. This is the backstop.
fn resolve_guest_phone(
    mode: &str,
    raw: Option<&str>,
    default_country: &str,
    lang: &str,
) -> Result<Option<String>, (String, String)> {
    if !crate::sms::collects_phone(mode) {
        return Ok(None);
    }

    let invalid = |key: &str| {
        (
            crate::i18n::translate(lang, "book-phone-invalid-title", None),
            crate::i18n::translate(lang, key, None),
        )
    };

    match raw.map(str::trim).filter(|p| !p.is_empty()) {
        None if crate::sms::requires_phone(mode) => Err(invalid("book-phone-required")),
        None => Ok(None),
        Some(value) => match crate::sms::phone::normalize(value, default_country) {
            Some(e164) => Ok(Some(e164)),
            None => Err(invalid("book-phone-invalid")),
        },
    }
}

pub(super) async fn handle_booking(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Path(slug): Path<String>,
    Form(form): Form<BookForm>,
) -> impl IntoResponse {
    if let Err(resp) = verify_csrf_token(&headers, &form._csrf) {
        return resp;
    }
    let captcha_cfg = state.captcha_config.read().await;
    if captcha::verify(&captcha_cfg, form.captcha_token.as_deref())
        .await
        .is_err()
    {
        tracing::warn!("captcha failed on booking");
        let lang = legacy_page_lang_lookup(&state.pool, &slug, &headers).await;
        return render_booking_action_error_keys_in(
            &state,
            lang,
            "bae-title-captcha",
            "bae-body-go-back",
        );
    }
    drop(captcha_cfg);
    // Rate limit by IP
    let client_ip = client_ip_for_rate_limit(&headers);
    if state.booking_limiter.check_limited(&client_ip).await {
        tracing::warn!(ip = %client_ip, "rate limited");
        let lang = legacy_page_lang_lookup(&state.pool, &slug, &headers).await;
        return Html(crate::i18n::translate(
            lang,
            "error-too-many-bookings",
            None,
        ))
        .into_response();
    }

    if let Err(e) = validate_booking_input(&form.name, &form.email, &form.notes) {
        let lang = legacy_page_lang_lookup(&state.pool, &slug, &headers).await;
        return render_booking_action_error_keys_in(&state, lang, "bae-title-invalid-booking", e);
    }

    // The page language: the browser's, or the owner's own from the row
    // below, as the GET form computed it.
    let visitor = crate::i18n::detect_from_headers(&headers);

    #[allow(clippy::type_complexity)]
    let et: Option<(
        String,
        String,
        String,
        i32,
        i32,
        i32,
        i32,
        i32,
        Option<i32>,
        i32,
        String,
        String,
        Option<String>,
        Option<String>,
    )> = sqlx::query_as(
        "SELECT et.id, et.slug, et.title, et.duration_min, et.buffer_before, et.buffer_after, et.min_notice_min, et.requires_confirmation, et.reminder_minutes, et.max_additional_guests, et.visibility, et.sms_phone_mode, u.language, u.public_page_language
         FROM event_types et
         JOIN accounts a ON a.id = et.account_id
         LEFT JOIN users u ON u.id = a.user_id
         WHERE et.slug = ? AND et.enabled = 1
         ORDER BY et.rowid LIMIT 1",
    )
    .bind(&slug)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    let (
        et_id,
        _et_slug,
        et_title,
        duration,
        buffer_before,
        buffer_after,
        min_notice,
        requires_confirmation,
        reminder_min,
        max_additional_guests,
        visibility,
        sms_phone_mode,
        owner_language,
        page_mode,
    ) = match et {
        Some(e) => e,
        None => {
            return Html(crate::i18n::translate(
                visitor,
                "error-event-type-not-found",
                None,
            ))
            .into_response()
        }
    };
    let lang = user_page_lang(
        owner_language.as_deref(),
        page_mode.as_deref().unwrap_or(""),
        visitor,
    );
    let needs_approval = requires_confirmation != 0;

    // Optional phone number, only meaningful (and only stored) when this
    // event type opted into SMS notifications. A guest-posted phone value
    // is silently dropped otherwise, so enabling/disabling SMS per event
    // type can never leak a phone number nobody asked to collect.
    let sms_default_country: String = crate::sms::default_country_code(&state.pool).await;

    let phone_to_store = match resolve_guest_phone(
        &sms_phone_mode,
        form.phone.as_deref(),
        &sms_default_country,
        lang,
    ) {
        Ok(value) => value,
        Err((title, message)) => {
            return render_booking_action_error_in(&state, lang, &title, &message);
        }
    };

    // Block non-public event types on legacy route
    if visibility == "private" || visibility == "internal" {
        return Html(crate::i18n::translate(lang, "error-invite-required", None)).into_response();
    }

    // Parse additional guests
    let additional_attendees = match parse_additional_guests(
        &form.additional_guests,
        max_additional_guests,
        &form.email,
        lang,
    ) {
        Ok(emails) => emails,
        Err(e) => {
            return render_booking_action_error_in(
                &state,
                lang,
                &crate::i18n::translate(lang, "bae-title-invalid-booking", None),
                &e,
            )
        }
    };

    // Get the host user_id for user-scoped busy time check
    let host_user_id: String = sqlx::query_scalar(
        "SELECT a.user_id FROM accounts a JOIN event_types et ON et.account_id = a.id WHERE et.id = ?",
    )
    .bind(&et_id)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None)
    .unwrap_or_default();

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
    let guest_timezone = guest_tz.name().to_string();
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

    // Validate minimum notice
    if chrono::DateTime::parse_from_rfc3339(&encoded_start)
        .unwrap()
        .with_timezone(&Utc)
        < Utc::now() + Duration::minutes(min_notice as i64)
    {
        return Html(crate::i18n::translate(lang, "error-slot-too-soon", None)).into_response();
    }

    // Rolling booking horizon. The slot list already hides anything past it,
    // but that is advisory: a crafted POST has to be rejected here too.
    let horizon_end = horizon_last_date(
        Utc::now().with_timezone(&host_tz).naive_local(),
        get_booking_horizon(&state.pool, &et_id).await,
    );
    if horizon_end.is_some_and(|last| slot_start.date() > last) {
        return Html(crate::i18n::translate(
            lang,
            "error-slot-beyond-horizon",
            None,
        ))
        .into_response();
    }

    // Validate conflicts
    let buf_start = check_start - Duration::minutes(buffer_before as i64);
    let buf_end = check_end + Duration::minutes(buffer_after as i64);

    // Create booking
    let id = uuid::Uuid::new_v4().to_string();
    let uid = format!("{}@calrs", uuid::Uuid::new_v4());
    let cancel_token = uuid::Uuid::new_v4().to_string();
    let reschedule_token = uuid::Uuid::new_v4().to_string();
    let (start_at, end_at) = (encoded_start, encoded_end);

    let initial_status = if needs_approval {
        "pending"
    } else {
        "confirmed"
    };
    let confirm_token: Option<String> = if needs_approval {
        Some(uuid::Uuid::new_v4().to_string())
    } else {
        None
    };

    // Start a transaction to ensure atomicity of availability check + insert.
    // The unique index idx_bookings_no_overlap is the ultimate guard against double-booking.
    let mut tx = match state.pool.begin().await {
        Ok(tx) => tx,
        Err(e) => {
            return internal_error_response("database query", &e);
        }
    };

    let busy = fetch_busy_times_for_user(
        &state.pool,
        &host_user_id,
        buf_start,
        buf_end,
        host_tz,
        Some(&et_id),
    )
    .await;
    if has_conflict(&busy, buf_start, buf_end) {
        let _ = tx.rollback().await;
        return Html(crate::i18n::translate(lang, "error-slot-unavailable", None)).into_response();
    }

    // Check booking frequency limits. Personal event-type flows don't have a
    // team assignee, so per-member caps degrade to event-type-wide here.
    if would_exceed_frequency_limit(&state.pool, &et_id, slot_start, None).await {
        let _ = tx.rollback().await;
        return render_booking_action_error_keys_in(
            &state,
            lang,
            "bae-title-unavailable",
            "bae-body-unavailable",
        );
    }

    // The time must be one the slot page offers. Weekly rules, date
    // overrides, interval alignment and first_slot_only are only enforced
    // by recomputing the page's own slots; the checks above just give the
    // common refusals their own message.
    let window = offered_slot_window(host_tz, slot_start);
    let offered_busy = BusySource::Individual(
        fetch_busy_times_for_user(
            &state.pool,
            &host_user_id,
            window.busy_start,
            window.busy_end,
            host_tz,
            Some(&et_id),
        )
        .await,
    );
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
        None,
        slot_start,
    )
    .await
    {
        let _ = tx.rollback().await;
        return Html(crate::i18n::translate(lang, "error-slot-unavailable", None)).into_response();
    }

    // Required shared resources: verify availability and pick one in
    // round-robin mode. The busy reads run on separate pool connections and
    // cannot see uncommitted inserts, so a process-wide lock, held from the
    // check until the booking row is committed, is what serializes
    // concurrent bookings over shared resources. Feeds are refreshed
    // before taking the lock (failed fetches back off via last_synced_at).
    let resource_ids: Vec<String> = crate::resources::resources_for_event_type(&state.pool, &et_id)
        .await
        .into_iter()
        .map(|r| r.id)
        .collect();
    let resource_guard = if resource_ids.is_empty() {
        None
    } else {
        crate::resources::sync_if_stale(&state.pool, &resource_ids).await;
        Some(crate::resources::booking_lock().await)
    };
    let assigned_resource_id = match crate::resources::check_and_pick(
        &state.pool,
        &et_id,
        check_start,
        check_end,
        host_tz,
        None,
    )
    .await
    {
        crate::resources::ResourceCheck::Busy => {
            let _ = tx.rollback().await;
            return Html(crate::i18n::translate(lang, "error-slot-unavailable", None))
                .into_response();
        }
        crate::resources::ResourceCheck::Free { assigned } => assigned,
        crate::resources::ResourceCheck::NoResources => None,
    };

    match crate::booking_time::legacy_slot_taken(&state.pool, &et_id, None, &start_at, "").await {
        Ok(false) => {}
        Ok(true) => {
            return Html(crate::i18n::translate(lang, "error-slot-unavailable", None))
                .into_response()
        }
        Err(e) => return internal_error_response("booking conflict check", &e),
    }
    let insert_result = sqlx::query(
        "INSERT INTO bookings (time_version, id, event_type_id, uid, guest_name, guest_email, guest_timezone, notes, start_at, end_at, status, cancel_token, reschedule_token, confirm_token, language, assigned_resource_id, guest_phone)
         VALUES (1, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(&et_id)
    .bind(&uid)
    .bind(&form.name)
    .bind(&form.email)
    .bind(&guest_timezone)
    .bind(&form.notes)
    .bind(&start_at)
    .bind(&end_at)
    .bind(initial_status)
    .bind(&cancel_token)
    .bind(&reschedule_token)
    .bind(&confirm_token)
    // The language the form was shown in, resolved exactly as the GET
    // handler for this route does, so the page, the emails and the SMS
    // always agree (#209).
    .bind(lang)
    .bind(&assigned_resource_id)
    .bind(&phone_to_store)
    .execute(&mut *tx)
    .await;

    match insert_result {
        Ok(_) => {
            if needs_approval {
                stamp_pending_deadline(&mut tx, &id).await;
            }
        }
        Err(e) => {
            let _ = tx.rollback().await;
            if e.to_string().contains("UNIQUE constraint failed") {
                return Html(crate::i18n::translate(lang, "error-slot-unavailable", None))
                    .into_response();
            }
            return internal_error_response("database query", &e);
        }
    }

    // Insert additional attendees
    for attendee_email in &additional_attendees {
        let attendee_id = uuid::Uuid::new_v4().to_string();
        let _ =
            sqlx::query("INSERT INTO booking_attendees (id, booking_id, email) VALUES (?, ?, ?)")
                .bind(&attendee_id)
                .bind(&id)
                .bind(attendee_email)
                .execute(&mut *tx)
                .await;
    }

    if let Err(e) = tx.commit().await {
        if e.to_string().contains("UNIQUE constraint failed") {
            return Html(crate::i18n::translate(lang, "error-slot-unavailable", None))
                .into_response();
        }
        return internal_error_response("database query", &e);
    }
    drop(resource_guard);

    tracing::info!(booking_id = %id, event_type = %slug, guest = %form.email, "booking created");

    // Build BookingDetails once. CalDAV push and email send both need it,
    // and CalDAV push must run independently of whether SMTP is configured.
    let host: Option<(String, String)> = sqlx::query_as(
        "SELECT u.name, COALESCE(u.booking_email, u.email) FROM users u JOIN accounts a ON a.user_id = u.id WHERE a.id = (SELECT account_id FROM event_types WHERE id = ?)",
    )
    .bind(&et_id)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    if let Some((host_name, host_email)) = host {
        let host_user_id: Option<String> = sqlx::query_scalar(
            "SELECT u.id FROM users u JOIN accounts a ON a.user_id = u.id JOIN event_types et ON et.account_id = a.id WHERE et.id = ?",
        )
        .bind(&et_id)
        .fetch_optional(&state.pool)
        .await
        .unwrap_or(None);
        let location_display = resolve_booking_location(
            &state,
            &id,
            &et_id,
            host_user_id.as_deref(),
            None,
            &form.name,
            &form.email,
            !needs_approval,
        )
        .await;
        let details = crate::email::BookingDetails {
            utc_times: crate::booking_time::ics_times(&start_at, &end_at),
            event_title: et_title.clone(),
            date: form.date.clone(),
            start_time: form.time.clone(),
            end_time: guest_end_time.clone(),
            guest_name: form.name.clone(),
            guest_email: form.email.clone(),
            guest_timezone: guest_timezone.clone(),
            host_name,
            host_email,
            uid: uid.clone(),
            notes: form.notes.clone(),
            guest_phone: phone_to_store.clone(),
            location: location_display,
            reminder_minutes: reminder_min,
            additional_attendees: additional_attendees.clone(),
            guest_language: Some(lang.to_string()),
            // The owner's saved language, selected with the event type.
            host_language: crate::email::supported_language(owner_language.clone()),
            host_timezone: host_tz.name().to_string(),
            resource_name: booking_resource_label(&state.pool, &uid).await,
            sequence: 0,
            attendee_languages: Vec::new(),
        };

        // Push confirmed bookings to CalDAV regardless of SMTP availability.
        if !needs_approval {
            if let Some(uid_user) = host_user_id.as_deref() {
                caldav_push_booking(&state.pool, &state.secret_key, uid_user, &uid, &details).await;
                resource_push_booking(&state.pool, &state.secret_key, &uid, &details).await;
            }
        }

        let (cancel_notice_min, reschedule_notice_min) =
            fetch_event_type_notice_minutes(&state.pool, &et_id).await;

        // Send emails if SMTP is configured.
        if let Ok(Some(smtp_config)) =
            crate::email::load_smtp_config(&state.pool, &state.secret_key).await
        {
            let base_url = crate::settings::base_url();
            let guest_cancel_url = base_url.as_ref().map(|base| {
                format!(
                    "{}/booking/cancel/{}",
                    base.trim_end_matches('/'),
                    cancel_token
                )
            });
            let guest_reschedule_url = base_url.as_ref().map(|base| {
                format!(
                    "{}/booking/reschedule/{}",
                    base.trim_end_matches('/'),
                    reschedule_token
                )
            });

            let host_recipients = vec![(
                details.host_name.clone(),
                details.host_email.clone(),
                details.host_language.clone(),
            )];
            send_new_booking_emails(NewBookingEmails {
                smtp_config,
                details,
                host_recipients,
                booking_id: id.clone(),
                needs_approval,
                confirm_token: confirm_token.clone(),
                base_url,
                guest_cancel_url,
                guest_reschedule_url,
                cancel_notice_min,
                reschedule_notice_min,
            });
        }
    }

    // Send booking SMS independently of the email/host notification path.
    // Pending bookings are notified when they are approved.
    notify_new_booking_sms(
        &state,
        crate::sms::collects_phone(&sms_phone_mode),
        phone_to_store.as_deref(),
        needs_approval,
        &et_title,
        &form.date,
        &form.time,
        &guest_timezone,
        lang,
    )
    .await;

    // Render confirmation
    let host_name: String = sqlx::query_scalar(
        "SELECT a.name FROM accounts a JOIN event_types et ON et.account_id = a.id WHERE et.id = ?",
    )
    .bind(&et_id)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None)
    .unwrap_or_else(|| "Host".to_string());

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
            guest_email => form.email,
            notes => form.notes,
            pending => needs_approval,
            additional_attendees => additional_attendees,
            ics_url => format!("/booking/ics/{}", cancel_token),
            cancel_notice_min => cancel_notice_min,
            reschedule_notice_min => reschedule_notice_min,
            company_link => state.company_link.read().await.clone(),
            lang => lang,
        })
        .unwrap_or_else(|e| internal_error_body("template render", &e));

    Html(rendered).into_response()
}
