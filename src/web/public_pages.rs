//! Public pages: user and team profiles, slot pickers, booking forms, legacy redirects.

use super::*;

pub(super) fn parse_dynamic_group_usernames(
    combined: &str,
    lang: &str,
) -> Result<Vec<String>, String> {
    let mut seen = std::collections::HashSet::new();
    let unique: Vec<String> = combined
        .split('+')
        .map(|s| s.trim().to_lowercase())
        .filter(|s| !s.is_empty())
        .filter(|s| seen.insert(s.clone()))
        .collect();
    if unique.len() < 2 {
        return Err(crate::i18n::translate(
            lang,
            "dynamic-group-min-usernames",
            None,
        ));
    }
    Ok(unique)
}

/// Validate that all usernames exist, are enabled, and allow dynamic group links.
/// Returns Vec<(user_id, username, name, email)> in the same order as input.
/// Validate that all usernames exist, are enabled, and allow dynamic group links.
/// Returns Vec<(user_id, username, name, email, avatar_path)> in the same order as input.
pub(super) async fn validate_dynamic_group_users(
    pool: &SqlitePool,
    usernames: &[String],
    lang: &str,
) -> Result<Vec<(String, String, String, String, Option<String>)>, String> {
    let mut users = Vec::with_capacity(usernames.len());
    for uname in usernames {
        let row: Option<(String, String, String, String, Option<String>, bool)> = sqlx::query_as(
            "SELECT id, username, name, COALESCE(booking_email, email), avatar_path, allow_dynamic_group FROM users WHERE username = ? AND enabled = 1",
        )
        .bind(uname)
        .fetch_optional(pool)
        .await
        .unwrap_or(None);
        match row {
            None => return Err(tr1(lang, "dynamic-group-user-not-found", "username", uname)),
            Some((_, _, _, _, _, false)) => {
                return Err(tr1(lang, "dynamic-group-user-opted-out", "username", uname))
            }
            Some((id, username, name, email, avatar_path, _)) => {
                users.push((id, username, name, email, avatar_path));
            }
        }
    }
    Ok(users)
}

// --- Legacy /g/ redirects ---

pub(super) async fn redirect_g_to_team(Path(team_slug): Path<String>) -> impl IntoResponse {
    Redirect::permanent(&format!("/team/{}", team_slug))
}

pub(super) async fn redirect_g_to_team_slug(
    Path((team_slug, slug)): Path<(String, String)>,
    Query(query): Query<std::collections::HashMap<String, String>>,
) -> impl IntoResponse {
    let qs = if query.is_empty() {
        String::new()
    } else {
        format!(
            "?{}",
            query
                .iter()
                .map(|(k, v)| format!("{}={}", k, v))
                .collect::<Vec<_>>()
                .join("&")
        )
    };
    Redirect::permanent(&format!("/team/{}/{}{}", team_slug, slug, qs))
}

pub(super) async fn redirect_g_to_team_slug_book(
    Path((team_slug, slug)): Path<(String, String)>,
) -> impl IntoResponse {
    Redirect::permanent(&format!("/team/{}/{}/book", team_slug, slug))
}

// --- Legacy /t/ team link redirects ---

pub(super) async fn redirect_team_link_to_team(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(token): Path<String>,
) -> impl IntoResponse {
    let lang = crate::i18n::detect_from_headers(&headers);
    // Look up the team by invite_token (team links were migrated to teams)
    let team: Option<(Option<String>,)> =
        sqlx::query_as("SELECT slug FROM teams WHERE invite_token = ?")
            .bind(&token)
            .fetch_optional(&state.pool)
            .await
            .unwrap_or(None);

    match team {
        Some((Some(slug),)) => {
            Redirect::permanent(&format!("/team/{}?invite={}", slug, token)).into_response()
        }
        Some((None,)) => {
            // Team exists but has no slug — should not happen after migration fix,
            // but handle gracefully
            Html(crate::i18n::translate(lang, "error-team-not-found", None)).into_response()
        }
        None => Html(crate::i18n::translate(lang, "error-team-not-found", None)).into_response(),
    }
}

// --- Group public pages ---

pub(super) async fn team_profile_page(
    State(state): State<Arc<AppState>>,
    optional_auth: crate::auth::OptionalAuthUser,
    headers: HeaderMap,
    Path(team_slug): Path<String>,
    Query(query): Query<std::collections::HashMap<String, String>>,
) -> impl IntoResponse {
    let team: Option<(String, String, Option<String>, Option<String>, String, Option<String>)> =
        sqlx::query_as("SELECT id, name, description, avatar_path, visibility, invite_token FROM teams WHERE slug = ?")
            .bind(&team_slug)
            .fetch_optional(&state.pool)
            .await
            .unwrap_or(None);

    let (
        team_id,
        team_name,
        team_description,
        team_avatar_path,
        team_visibility,
        team_invite_token,
    ) = match team {
        Some(t) => t,
        None => {
            return Html(crate::i18n::translate(
                crate::i18n::detect_from_headers(&headers),
                "error-team-not-found",
                None,
            ))
        }
    };

    // Logged-in team members (and global admins) bypass the private-team
    // invite token gate — their membership is the access. Compute once and
    // reuse for both the gate below and the event-type filter further down.
    let viewer_is_member_or_admin = match &optional_auth.user {
        Some(u) if u.role == "admin" => true,
        Some(u) => is_team_member(&state.pool, &u.id, &team_id).await,
        None => false,
    };

    let passed_invite = query.get("invite").cloned().unwrap_or_default();
    if team_visibility == "private" && !viewer_is_member_or_admin {
        match &team_invite_token {
            Some(expected) if !passed_invite.is_empty() && passed_invite == *expected => {
                // valid — continue
            }
            _ => {
                return Html(crate::i18n::translate(
                    crate::i18n::detect_from_headers(&headers),
                    "error-team-not-found",
                    None,
                ));
            }
        }
    }

    // Logged-in team members (and global admins) see all enabled event types,
    // including private/internal — they have view access via the slot pages
    // anyway, so showing them on the profile makes them discoverable. Anonymous
    // visitors and non-members continue to see only public event types.
    let event_types: Vec<(String, String, Option<String>, i32, String, String)> =
        if viewer_is_member_or_admin {
            sqlx::query_as(
                "SELECT et.slug, et.title, et.description, et.duration_min, et.visibility, et.id
             FROM event_types et
             WHERE et.team_id = ? AND et.enabled = 1
             ORDER BY et.created_at",
            )
            .bind(&team_id)
            .fetch_all(&state.pool)
            .await
            .unwrap_or_default()
        } else {
            sqlx::query_as(
                "SELECT et.slug, et.title, et.description, et.duration_min, et.visibility, et.id
             FROM event_types et
             WHERE et.team_id = ? AND et.enabled = 1 AND et.visibility = 'public'
             ORDER BY et.created_at",
            )
            .bind(&team_id)
            .fetch_all(&state.pool)
            .await
            .unwrap_or_default()
        };
    let event_types = without_expired(&state.pool, event_types, |row| &row.5).await;

    let members: Vec<(String, String, Option<String>)> = sqlx::query_as(
        "SELECT u.id, u.name, u.avatar_path FROM users u \
         JOIN team_members tm ON tm.user_id = u.id \
         WHERE tm.team_id = ? AND u.enabled = 1 \
         ORDER BY u.name",
    )
    .bind(&team_id)
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();

    let members_ctx: Vec<minijinja::Value> = members
        .iter()
        .map(|(id, name, ap)| {
            context! {
                id => id,
                name => name,
                has_avatar => ap.is_some(),
                initials => compute_initials(name),
            }
        })
        .collect();

    let tmpl = match state.templates.get_template("team_profile.html") {
        Ok(t) => t,
        Err(e) => return internal_error_html("template render", &e),
    };

    let et_ctx: Vec<minijinja::Value> = event_types
        .iter()
        .map(|(slug, title, desc, duration, visibility, _)| {
            context! {
                slug => slug,
                title => title,
                description => desc.as_deref().map(crate::utils::render_inline_markdown),
                duration_min => duration,
                visibility => visibility,
            }
        })
        .collect();

    // Pass invite token through if team is private (so links include it)
    let invite_token_for_template = if team_visibility == "private" && !passed_invite.is_empty() {
        passed_invite
    } else {
        String::new()
    };

    Html(
        tmpl.render(context! {
            team_id => team_id,
            team_name => team_name,
            team_slug => team_slug,
            team_description => team_description.as_deref().map(crate::utils::render_inline_markdown),
            team_has_avatar => team_avatar_path.is_some(),
            team_initials => compute_initials(&team_name),
            members => members_ctx,
            event_types => et_ctx,
            invite_token => invite_token_for_template,
            company_link => state.company_link.read().await.clone(),
            lang => crate::i18n::detect_from_headers(&headers),
        })
        .unwrap_or_else(|e| internal_error_body("template render", &e)),
    )
}

pub(super) async fn show_group_slots(
    State(state): State<Arc<AppState>>,
    optional_auth: crate::auth::OptionalAuthUser,
    headers: HeaderMap,
    Path((team_slug, slug)): Path<(String, String)>,
    Query(query): Query<SlotsQuery>,
) -> Response {
    let embed = query.embed_params();
    // The visitor rule, as the team profile and the booking POST use.
    let lang = crate::i18n::detect_from_headers(&headers);
    let et: Option<(String, String, String, Option<String>, i32, i32, i32, i32, String, Option<String>, String, String, String, String, Option<String>, String)> = sqlx::query_as(
        "SELECT et.id, et.slug, et.title, et.description, et.duration_min, et.buffer_before, et.buffer_after, et.min_notice_min, et.location_type, et.location_value, t.name, et.visibility, et.scheduling_mode, t.visibility, t.invite_token, et.default_calendar_view
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
        et_slug,
        et_title,
        et_desc,
        duration,
        buf_before,
        buf_after,
        min_notice,
        loc_type,
        loc_value,
        team_name,
        visibility,
        scheduling_mode,
        team_visibility,
        team_invite_token,
        default_calendar_view,
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

    // Resolve the team id once and reuse for membership checks and downstream
    // queries. Can't fold this into the event-type SELECT above because sqlx
    // tuples cap at 16 columns and we're already at the limit.
    let team_id: String = match sqlx::query_scalar::<_, String>(
        "SELECT team_id FROM event_types WHERE id = ? AND team_id IS NOT NULL",
    )
    .bind(&et_id)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None)
    {
        Some(tid) => tid,
        None => {
            return Html(crate::i18n::translate(
                lang,
                "error-event-type-not-found",
                None,
            ))
            .into_response()
        }
    };

    // Global admins can view any team event type (matches their management
    // surface). Booking is still invite-gated for private/internal events;
    // global admins do not auto-bypass the booking gate.
    let is_logged_in_team_member = match &optional_auth.user {
        Some(u) if u.role == "admin" => true,
        Some(u) => is_team_member(&state.pool, &u.id, &team_id).await,
        None => false,
    };

    // Validate booking invite token if one was provided (used both for direct
    // booking-invite access and to mark the request as "can_book").
    let has_valid_booking_invite = if let Some(token) = query.invite.as_deref() {
        if token.is_empty() {
            false
        } else {
            let invite: Option<(Option<String>, i32, i32)> = sqlx::query_as(
                "SELECT expires_at, max_uses, used_count FROM booking_invites WHERE token = ? AND event_type_id = ?",
            )
            .bind(token)
            .bind(&et_id)
            .fetch_optional(&state.pool)
            .await
            .unwrap_or(None);
            match invite {
                Some((expires_at, max_uses, used_count)) => {
                    let not_expired = expires_at.as_deref().is_none_or(|exp| {
                        exp >= chrono::Utc::now()
                            .format("%Y-%m-%d %H:%M:%S")
                            .to_string()
                            .as_str()
                    });
                    not_expired && used_count < max_uses
                }
                None => false,
            }
        }
    } else {
        false
    };

    // Did the request supply a valid team-level invite token? Used for private
    // teams hosting public event types (legacy behavior preserved).
    let has_valid_team_invite = matches!(
        (&team_invite_token, &query.invite),
        (Some(expected), Some(provided)) if !provided.is_empty() && provided == expected
    );

    // Access matrix:
    //   public team + public event       → anyone
    //   private team + public event      → team invite OR logged-in team member
    //   any team + private/internal event → booking invite OR logged-in team member (view only)
    let can_view = match (team_visibility.as_str(), visibility.as_str()) {
        (_, "private") | (_, "internal") => has_valid_booking_invite || is_logged_in_team_member,
        ("private", _) => has_valid_team_invite || is_logged_in_team_member,
        _ => true,
    };

    if !can_view {
        let key = if visibility == "private" || visibility == "internal" {
            "error-invite-required"
        } else {
            // Public event on private team without team invite — preserve legacy
            // "Event type not found" response so we don't leak existence.
            "error-event-type-not-found"
        };
        return Html(crate::i18n::translate(lang, key, None)).into_response();
    }

    // can_book: only with a valid booking invite for private/internal events.
    // Public events: anyone who can view can also book.
    let can_book = match visibility.as_str() {
        "private" | "internal" => has_valid_booking_invite,
        _ => can_view,
    };

    let guest_tz = parse_guest_tz(query.tz.as_deref());
    let host_tz = get_host_tz(&state.pool, &et_id).await;
    let booking_window = get_booking_window(&state.pool, &et_id).await;
    let guest_tz_name = guest_tz.name().to_string();

    let (year, month) = parse_month_param(
        query.month.as_deref(),
        guest_tz,
        host_tz,
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

    // Build team busy source: fetch busy times per member
    let now_host = Utc::now().with_timezone(&host_tz).naive_local();
    let end_date = now_host.date() + Duration::days((start_offset + days_ahead) as i64);
    let window_end = end_date.and_hms_opt(23, 59, 59).unwrap_or(now_host);

    // Sync all team members' calendars if stale. sync_if_stale holds a
    // per-source mutex and re-checks staleness inside the lock, so even with
    // this fan-out at most one CalDAV fetch per source is in flight at a time.
    let mut sync_tasks = tokio::task::JoinSet::new();
    for uid in team_member_ids(&state.pool, &et_id, &team_id).await {
        let pool = state.pool.clone();
        let key = state.secret_key;
        sync_tasks.spawn(async move {
            crate::commands::sync::sync_if_stale(&pool, &key, &uid).await;
        });
    }
    while sync_tasks.join_next().await.is_some() {}
    let busy = team_busy_source(
        &state.pool,
        &et_id,
        &team_id,
        scheduling_mode == "collective",
        now_host,
        window_end,
        host_tz,
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
        None,
    )
    .await;

    let days_ctx: Vec<minijinja::Value> = slot_days
        .iter()
        .map(|d| {
            let slots: Vec<minijinja::Value> = d
                .slots
                .iter()
                .map(|s| context! { start => s.start, end => s.end, host_date => s.host_date, host_time => s.host_time, guest_date => s.guest_date })
                .collect();
            context! { date => d.date, label => d.label, slots => slots }
        })
        .collect();

    let available_dates: Vec<String> = slot_days.iter().map(|d| d.date.clone()).collect();

    let tz_options = timezone_options(&guest_tz_name, lang);

    // Fetch team avatar for sidebar display (team_id is already in scope from
    // the initial event-type lookup).
    let team_avatar_path: Option<String> =
        sqlx::query_scalar("SELECT avatar_path FROM teams WHERE id = ?")
            .bind(&team_id)
            .fetch_optional(&state.pool)
            .await
            .unwrap_or(None)
            .flatten();

    // Fetch active team members for sidebar display (exclude members with weight=0)
    let team_members_rows: Vec<(String, String, Option<String>)> = sqlx::query_as(
        "SELECT u.id, u.name, u.avatar_path FROM users u \
         JOIN team_members tm ON tm.user_id = u.id \
         LEFT JOIN event_type_member_weights etw ON etw.user_id = u.id AND etw.event_type_id = ? \
         WHERE tm.team_id = ? AND u.enabled = 1 AND COALESCE(etw.weight, 1) > 0 \
         ORDER BY u.name",
    )
    .bind(&et_id)
    .bind(&team_id)
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();

    let team_members_ctx: Vec<minijinja::Value> = team_members_rows
        .iter()
        .map(|(uid, uname, ap)| {
            context! {
                id => uid,
                name => uname,
                has_avatar => ap.is_some(),
                initials => compute_initials(uname),
            }
        })
        .collect();

    let (meeting_jitsi_label, meeting_webhook_label) = meeting_provider_labels(&state).await;
    let tmpl = match state.templates.get_template("slots.html") {
        Ok(t) => t,
        Err(e) => return internal_error_html("internal", &e).into_response(),
    };
    let rendered = tmpl
        .render(context! {
            meeting_jitsi_label => meeting_jitsi_label,
            meeting_webhook_label => meeting_webhook_label,
            event_type => context! {
                slug => et_slug,
                title => et_title,
                description => et_desc.as_deref().map(crate::utils::render_inline_markdown),
                duration_min => duration,
                location_type => loc_type,
                location_value => loc_value,
            },
            host_name => team_name,
            team_slug => team_slug,
            team_id => team_id,
            team_has_avatar => team_avatar_path.is_some(),
            team_initials => compute_initials(&team_name),
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
            invite_token => query.invite.as_deref().unwrap_or(""),
            default_calendar_view => embed.layout.as_deref().filter(|s| !s.is_empty()).unwrap_or(default_calendar_view.as_str()),
            company_link => state.company_link.read().await.clone(),
            lang => lang,
            can_book => can_book,
            embed => embed.is_embedded(),
            embed_brand => embed.brand_css().unwrap_or_default(),
            embed_theme => embed.theme.as_deref().unwrap_or(""),
            embed_qs_amp => embed.query_suffix(true),
            embed_qs_q => embed.query_suffix(false),
        })
        .unwrap_or_else(|e| internal_error_body("template render", &e));

    html_response(rendered, &embed)
}

pub(super) async fn show_group_book_form(
    State(state): State<Arc<AppState>>,
    optional_auth: crate::auth::OptionalAuthUser,
    headers: HeaderMap,
    Path((team_slug, slug)): Path<(String, String)>,
    Query(query): Query<BookQuery>,
) -> Response {
    let embed = query.embed_params();
    // The visitor rule, as the team profile and the booking POST use.
    let lang = crate::i18n::detect_from_headers(&headers);
    let et: Option<(String, String, String, Option<String>, i32, String, Option<String>, String, String, i32, String, Option<String>, String, String)> = sqlx::query_as(
        "SELECT et.id, et.slug, et.title, et.description, et.duration_min, et.location_type, et.location_value, t.name, et.visibility, et.max_additional_guests, t.visibility, t.invite_token, t.id, et.sms_phone_mode
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
        et_slug,
        et_title,
        et_desc,
        duration,
        loc_type,
        loc_value,
        team_name,
        visibility,
        max_additional_guests,
        team_visibility,
        team_invite_token,
        team_id,
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

    // Logged-in team members (and global admins) substitute for the team
    // invite token on public events of private teams. Private/internal events
    // still require a valid booking-invite token regardless of team membership.
    let viewer_is_member_or_admin = match &optional_auth.user {
        Some(u) if u.role == "admin" => true,
        Some(u) => is_team_member(&state.pool, &u.id, &team_id).await,
        None => false,
    };

    let invite_guest_name;
    let invite_guest_email;
    if visibility == "private" || visibility == "internal" {
        let token = match &query.invite {
            Some(t) => t,
            None => {
                return Html(crate::i18n::translate(lang, "error-invite-required", None))
                    .into_response()
            }
        };
        let invite: Option<(String, String, Option<String>, i32, i32)> = sqlx::query_as(
            "SELECT guest_name, guest_email, expires_at, max_uses, used_count FROM booking_invites WHERE token = ? AND event_type_id = ?",
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
            Some((name, email, expires_at, max_uses, used_count)) => {
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
                invite_guest_name = Some(name);
                invite_guest_email = Some(email);
            }
        }
    } else if team_visibility == "private" && !viewer_is_member_or_admin {
        // Public event type on a private team — needs the team invite token
        // unless the viewer is a logged-in team member or global admin.
        let valid = matches!((&team_invite_token, &query.invite), (Some(expected), Some(provided)) if !provided.is_empty() && provided == expected);
        if !valid {
            return Html(crate::i18n::translate(
                lang,
                "error-event-type-not-found",
                None,
            ))
            .into_response();
        }
        invite_guest_name = None;
        invite_guest_email = None;
    } else {
        invite_guest_name = None;
        invite_guest_email = None;
    }

    let guest_tz = parse_guest_tz(query.tz.as_deref());
    let guest_tz_name = guest_tz.name().to_string();
    let phone_default_country: String = crate::sms::default_country_code(&state.pool).await;
    let date = match NaiveDate::parse_from_str(&query.date, "%Y-%m-%d") {
        Ok(d) => d,
        Err(_) => {
            return Html(crate::i18n::translate(
                lang,
                "error-invalid-date-format",
                None,
            ))
            .into_response()
        }
    };
    let time = match NaiveTime::parse_from_str(&query.time, "%H:%M") {
        Ok(t) => t,
        Err(_) => {
            return Html(crate::i18n::translate(
                lang,
                "error-invalid-time-format",
                None,
            ))
            .into_response()
        }
    };
    let end_time = (date.and_time(time) + Duration::minutes(duration as i64))
        .time()
        .format("%H:%M")
        .to_string();
    let date_label = crate::i18n::format_long_date(date, lang);

    let (meeting_jitsi_label, meeting_webhook_label) = meeting_provider_labels(&state).await;
    let tmpl = match state.templates.get_template("book.html") {
        Ok(t) => t,
        Err(e) => return internal_error_html("internal", &e).into_response(),
    };
    let captcha = captcha::CaptchaVars::from_config(&*state.captcha_config.read().await);
    let rendered = tmpl
        .render(context! {
            meeting_jitsi_label => meeting_jitsi_label,
            meeting_webhook_label => meeting_webhook_label,
            event_type => context! {
                slug => et_slug,
                title => et_title,
                description => et_desc.as_deref().map(crate::utils::render_inline_markdown),
                duration_min => duration,
                location_type => loc_type,
                location_value => loc_value,
            },
            host_name => team_name,
            team_slug => team_slug,
            date => query.date,
            date_label => date_label,
            time_start => query.time,
            time_end => end_time,
            guest_tz => guest_tz_name,
            error => "",
            form_name => invite_guest_name.as_deref().unwrap_or(""),
            form_email => invite_guest_email.as_deref().unwrap_or(""),
            form_notes => "",
            invite_token => query.invite.as_deref().unwrap_or(""),
            sms_phone_mode => sms_phone_mode,
            phone_default_country => phone_default_country,
            form_phone => "",
            max_additional_guests => max_additional_guests,
            company_link => state.company_link.read().await.clone(),
            captcha_enabled => captcha.enabled,
            captcha_api_endpoint => captcha.api_endpoint,
            captcha_widget_url => captcha.widget_url,
            captcha_wasm_url => captcha.wasm_url,
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

// --- Group slot computation ---

// --- User profile page ---

pub(super) async fn redirect_user_profile_trailing_slash(uri: axum::http::Uri) -> Redirect {
    // Keep the encoded path and query intact; only remove the route's final slash.
    // Building from the path also keeps the redirect local to this application.
    let mut canonical = uri.path().trim_end_matches('/').to_owned();
    if let Some(query) = uri.query() {
        canonical.push('?');
        canonical.push_str(query);
    }
    Redirect::permanent(&canonical)
}

pub(super) async fn user_profile(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(username): Path<String>,
) -> impl IntoResponse {
    let visitor = crate::i18n::detect_from_headers(&headers);
    let user: Option<(
        String,
        String,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
        String,
    )> = sqlx::query_as(
        "SELECT id, name, title, bio, avatar_path, language, public_page_language FROM users WHERE username = ? AND enabled = 1",
    )
    .bind(&username)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    let (user_id, user_name, user_title, user_bio, avatar_path, host_language, page_mode) =
        match user {
            Some(u) => u,
            None => {
                return Html(crate::i18n::translate(
                    visitor,
                    "error-user-not-found",
                    None,
                ))
            }
        };
    let lang = user_page_lang(host_language.as_deref(), &page_mode, visitor);

    let event_types: Vec<(String, String, Option<String>, i32, String)> = sqlx::query_as(
        "SELECT et.slug, et.title, et.description, et.duration_min, et.id
         FROM event_types et
         JOIN accounts a ON a.id = et.account_id
         WHERE a.user_id = ? AND et.enabled = 1 AND et.visibility = 'public'
         AND et.team_id IS NULL
         ORDER BY et.created_at",
    )
    .bind(&user_id)
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();
    let event_types = without_expired(&state.pool, event_types, |row| &row.4).await;

    let tmpl = match state.templates.get_template("profile.html") {
        Ok(t) => t,
        Err(e) => return internal_error_html("template render", &e),
    };

    let et_ctx: Vec<minijinja::Value> = event_types
        .iter()
        .map(|(slug, title, desc, duration, _)| {
            context! { slug => slug, title => title, description => desc.as_deref().map(crate::utils::render_inline_markdown), duration_min => duration }
        })
        .collect();

    Html(
        tmpl.render(context! {
            host_name => &user_name,
            host_initials => compute_initials(&user_name),
            host_title => user_title,
            host_bio => user_bio.as_deref().map(crate::utils::render_inline_markdown),
            host_user_id => user_id,
            host_has_avatar => avatar_path.is_some(),
            username => username,
            event_types => et_ctx,
            company_link => state.company_link.read().await.clone(),
            lang,
        })
        .unwrap_or_else(|e| internal_error_body("template render", &e)),
    )
}

// --- Dynamic group link handlers ---

async fn show_dynamic_group_slots(
    state: &AppState,
    lang: &'static str,
    combined_username: &str,
    slug: &str,
    query: &SlotsQuery,
) -> Html<String> {
    let usernames = match parse_dynamic_group_usernames(combined_username, lang) {
        Ok(u) => u,
        Err(e) => {
            return render_booking_action_error_html_in(
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
            return render_booking_action_error_html_in(
                state,
                lang,
                &crate::i18n::translate(lang, "bae-title-invalid-link", None),
                &e,
            )
        }
    };

    // Load event type from first user (owner)
    let owner_username = &usernames[0];
    let et: Option<(String, String, String, Option<String>, i32, i32, i32, i32, String, Option<String>, String, String, Option<String>, Option<String>, String, String)> = sqlx::query_as(
        "SELECT et.id, et.slug, et.title, et.description, et.duration_min, et.buffer_before, et.buffer_after, et.min_notice_min, et.location_type, et.location_value, u.id, u.name, u.title, u.avatar_path, et.visibility, et.default_calendar_view
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
        et_slug,
        et_title,
        et_desc,
        duration,
        buf_before,
        buf_after,
        min_notice,
        loc_type,
        loc_value,
        owner_user_id,
        _owner_name,
        _owner_title,
        _owner_avatar_path,
        visibility,
        default_calendar_view,
    ) = match et {
        Some(e) => e,
        None => {
            return Html(crate::i18n::translate(
                lang,
                "error-event-type-not-found",
                None,
            ))
        }
    };

    // Dynamic group links only for public event types
    if visibility != "public" {
        return Html(crate::i18n::translate(
            lang,
            "error-dynamic-group-public-only",
            None,
        ));
    }

    if let Some(html) =
        dynamic_group_google_meet_error_page(state, lang, &loc_type, &owner_user_id).await
    {
        return html;
    }

    // Build combined host display name
    let host_name = dg_users
        .iter()
        .map(|(_, _, name, _, _)| name.as_str())
        .collect::<Vec<_>>()
        .join(" & ");

    let guest_tz = parse_guest_tz(query.tz.as_deref());
    let host_tz = get_host_tz(&state.pool, &et_id).await;
    let booking_window = get_booking_window(&state.pool, &et_id).await;
    let guest_tz_name = guest_tz.name().to_string();

    let (year, month) = parse_month_param(
        query.month.as_deref(),
        guest_tz,
        host_tz,
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

    // Deferred loading: on initial page load (no &deferred=1), skip sync + computation
    // and render the page shell immediately. JS will fetch with &deferred=1 to get real data.
    let is_deferred_callback = query.deferred.as_deref() == Some("1");

    let slot_days = if is_deferred_callback {
        // Full sync + computation (AJAX callback). Safe to parallelize:
        // sync_if_stale holds a per-source mutex and re-checks staleness,
        // so same-source fan-in collapses to one fetch.
        let mut sync_tasks = tokio::task::JoinSet::new();
        for (uid, _, _, _, _) in &dg_users {
            let pool = state.pool.clone();
            let key = state.secret_key;
            let uid = uid.clone();
            sync_tasks.spawn(async move {
                crate::commands::sync::sync_if_stale(&pool, &key, &uid).await;
            });
        }
        while sync_tasks.join_next().await.is_some() {}

        let now_host = Utc::now().with_timezone(&host_tz).naive_local();
        let end_date = now_host.date() + Duration::days((start_offset + days_ahead) as i64);
        let window_end = end_date.and_hms_opt(23, 59, 59).unwrap_or(now_host);

        let user_ids: Vec<String> = dg_users.iter().map(|(uid, ..)| uid.clone()).collect();
        let busy = dynamic_group_busy_source(
            &state.pool,
            &et_id,
            &user_ids,
            now_host,
            window_end,
            host_tz,
        )
        .await;

        compute_slots(
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
            None,
        )
        .await
    } else {
        // Initial load: empty slots, page renders instantly
        vec![]
    };

    let days_ctx: Vec<minijinja::Value> = slot_days
        .iter()
        .map(|d| {
            let slots: Vec<minijinja::Value> = d
                .slots
                .iter()
                .map(|s| {
                    context! { start => s.start, end => s.end, host_date => s.host_date, host_time => s.host_time, guest_date => s.guest_date }
                })
                .collect();
            context! { date => d.date, label => d.label, slots => slots }
        })
        .collect();

    let available_dates: Vec<String> = slot_days.iter().map(|d| d.date.clone()).collect();

    let tz_options = timezone_options(&guest_tz_name, lang);

    let (meeting_jitsi_label, meeting_webhook_label) = meeting_provider_labels(state).await;
    let tmpl = match state.templates.get_template("slots.html") {
        Ok(t) => t,
        Err(e) => return internal_error_html("internal", &e),
    };
    Html(
        tmpl.render(context! {
            meeting_jitsi_label => meeting_jitsi_label,
            meeting_webhook_label => meeting_webhook_label,
            event_type => context! {
                slug => et_slug,
                title => et_title,
                description => et_desc.as_deref().map(crate::utils::render_inline_markdown),
                duration_min => duration,
                location_type => loc_type,
                location_value => loc_value,
            },
            host_name => host_name,
            dg_members => dg_users.iter().map(|(id, _, name, _, avatar_path)| {
                context! {
                    id => id,
                    name => name,
                    has_avatar => avatar_path.is_some(),
                    initials => compute_initials(name),
                }
            }).collect::<Vec<_>>(),
            username => combined_username,
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
            invite_token => "",
            invite_guest_name => "",
            invite_guest_email => "",
            default_calendar_view => default_calendar_view,
            deferred_load => !is_deferred_callback,
            company_link => state.company_link.read().await.clone(),
            lang => lang,
        })
        .unwrap_or_else(|e| internal_error_body("template render", &e)),
    )
}

async fn show_dynamic_group_book_form(
    state: &AppState,
    lang: &'static str,
    combined_username: &str,
    slug: &str,
    query: &BookQuery,
) -> Html<String> {
    let usernames = match parse_dynamic_group_usernames(combined_username, lang) {
        Ok(u) => u,
        Err(e) => {
            return render_booking_action_error_html_in(
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
            return render_booking_action_error_html_in(
                state,
                lang,
                &crate::i18n::translate(lang, "bae-title-invalid-link", None),
                &e,
            )
        }
    };

    let owner_username = &usernames[0];
    let et: Option<(String, String, String, Option<String>, i32, String, Option<String>, String, i32, String)> = sqlx::query_as(
        "SELECT et.id, et.slug, et.title, et.description, et.duration_min, et.location_type, et.location_value, et.visibility, et.max_additional_guests, et.sms_phone_mode
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
        _,
        et_slug,
        et_title,
        et_desc,
        duration,
        loc_type,
        loc_value,
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
        }
    };

    if visibility != "public" {
        return Html(crate::i18n::translate(
            lang,
            "error-dynamic-group-public-only",
            None,
        ));
    }

    if let Some(html) =
        dynamic_group_google_meet_error_page(state, lang, &loc_type, &dg_users[0].0).await
    {
        return html;
    }

    let host_name = dg_users
        .iter()
        .map(|(_, _, name, _, _)| name.as_str())
        .collect::<Vec<_>>()
        .join(" & ");

    let guest_tz = parse_guest_tz(query.tz.as_deref());
    let guest_tz_name = guest_tz.name().to_string();
    let phone_default_country: String = crate::sms::default_country_code(&state.pool).await;

    let date = match NaiveDate::parse_from_str(&query.date, "%Y-%m-%d") {
        Ok(d) => d,
        Err(_) => {
            return Html(crate::i18n::translate(
                lang,
                "error-invalid-date-format",
                None,
            ))
        }
    };
    let time = match NaiveTime::parse_from_str(&query.time, "%H:%M") {
        Ok(t) => t,
        Err(_) => {
            return Html(crate::i18n::translate(
                lang,
                "error-invalid-time-format",
                None,
            ))
        }
    };
    let end_time = (date.and_time(time) + Duration::minutes(duration as i64))
        .time()
        .format("%H:%M")
        .to_string();
    let date_label = crate::i18n::format_long_date(date, lang);

    let (meeting_jitsi_label, meeting_webhook_label) = meeting_provider_labels(state).await;
    let tmpl = match state.templates.get_template("book.html") {
        Ok(t) => t,
        Err(e) => return internal_error_html("internal", &e),
    };
    let captcha = captcha::CaptchaVars::from_config(&*state.captcha_config.read().await);
    Html(
        tmpl.render(context! {
            meeting_jitsi_label => meeting_jitsi_label,
            meeting_webhook_label => meeting_webhook_label,
            event_type => context! {
                slug => et_slug,
                title => et_title,
                description => et_desc.as_deref().map(crate::utils::render_inline_markdown),
                duration_min => duration,
                location_type => loc_type,
                location_value => loc_value,
            },
            host_name => host_name,
            username => combined_username,
            sms_phone_mode => sms_phone_mode,
            phone_default_country => phone_default_country,
            form_phone => "",
            date => query.date,
            date_label => date_label,
            time_start => query.time,
            time_end => end_time,
            guest_tz => guest_tz_name,
            error => "",
            form_name => "",
            form_email => "",
            form_notes => "",
            invite_token => "",
            max_additional_guests => max_additional_guests,
            company_link => state.company_link.read().await.clone(),
            captcha_enabled => captcha.enabled,
            captcha_api_endpoint => captcha.api_endpoint,
            captcha_widget_url => captcha.widget_url,
            captcha_wasm_url => captcha.wasm_url,
            lang => lang,
        })
        .unwrap_or_else(|e| internal_error_body("template render", &e)),
    )
}

// --- User-scoped booking handlers ---

/// Language of a host's own public pages, under `/u/{username}` and on the
/// legacy single-user routes: the profile, slots, booking form, and the
/// booking POST with every page it answers with. GET and POST both call this
/// with the host's `users.language` and `users.public_page_language`
/// (selected by the queries they already run), so `bookings.language` (the
/// page language, #209) always matches what the guest saw.
///
/// 'visitor' (the default) gives `visitor`, the browser's language
/// (`i18n::detect_from_headers`), the rule of every public guest page;
/// 'host' gives the host's saved language (#76), or `visitor` when the host
/// has none. A logged-in user's own preference never counts on these pages:
/// staff booking for a client, or an admin impersonating someone, would
/// otherwise stamp their language on a real guest's booking.
pub(super) fn user_page_lang(
    host_language: Option<&str>,
    mode: &str,
    visitor: &'static str,
) -> &'static str {
    if normalize_public_page_language(mode) == Some("host") {
        if let Some(code) = host_language.and_then(crate::i18n::supported_code) {
            return code;
        }
    }
    visitor
}

/// [`user_page_lang`] from scratch, for the error pages the `/u/` booking
/// POST may have to show before its own queries run (a failed captcha, the
/// rate limit, invalid input). Only those failure paths pay for this lookup.
pub(super) async fn user_page_lang_lookup(
    pool: &SqlitePool,
    username: &str,
    headers: &HeaderMap,
) -> &'static str {
    let visitor = crate::i18n::detect_from_headers(headers);
    if username.contains('+') {
        return visitor;
    }
    let row: Option<(Option<String>, String)> = sqlx::query_as(
        "SELECT language, public_page_language FROM users WHERE username = ? AND enabled = 1",
    )
    .bind(username)
    .fetch_optional(pool)
    .await
    .unwrap_or(None);
    match row {
        Some((host_language, mode)) => user_page_lang(host_language.as_deref(), &mode, visitor),
        None => visitor,
    }
}

/// [`user_page_lang_lookup`] for the legacy single-user routes, where the
/// host is the owner of the event type with this slug. The legacy routes
/// resolve a slug shared by several accounts to its oldest row (`ORDER BY
/// et.rowid`), here and in their main queries, so an error page never picks
/// a different owner than the page it answers.
pub(super) async fn legacy_page_lang_lookup(
    pool: &SqlitePool,
    slug: &str,
    headers: &HeaderMap,
) -> &'static str {
    let visitor = crate::i18n::detect_from_headers(headers);
    let row: Option<(Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT u.language, u.public_page_language FROM event_types et
         JOIN accounts a ON a.id = et.account_id
         LEFT JOIN users u ON u.id = a.user_id
         WHERE et.slug = ? AND et.enabled = 1
         ORDER BY et.rowid LIMIT 1",
    )
    .bind(slug)
    .fetch_optional(pool)
    .await
    .unwrap_or(None);
    match row {
        Some((host_language, mode)) => user_page_lang(
            host_language.as_deref(),
            mode.as_deref().unwrap_or(""),
            visitor,
        ),
        None => visitor,
    }
}

pub(super) async fn show_slots_for_user(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path((username, slug)): Path<(String, String)>,
    Query(query): Query<SlotsQuery>,
) -> Response {
    let visitor = crate::i18n::detect_from_headers(&headers);
    let embed = query.embed_params();
    // Dynamic group pages (`alice+bob`) intentionally return before embed
    // threading: embedding ad-hoc combined links is out of scope, so these
    // render with full chrome and don't autosize. The embed generator only
    // produces single-user/team links.
    if username.contains('+') {
        return show_dynamic_group_slots(&state, visitor, &username, &slug, &query)
            .await
            .into_response();
    }

    let user: Option<(
        String,
        String,
        Option<String>,
        Option<String>,
        Option<String>,
        String,
    )> = sqlx::query_as(
        "SELECT id, name, title, avatar_path, language, public_page_language FROM users WHERE username = ? AND enabled = 1",
    )
    .bind(&username)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    let (host_user_id, host_name, host_title, host_avatar_path, host_language, page_mode) =
        match user {
            Some(user) => user,
            None => {
                return Html(crate::i18n::translate(
                    visitor,
                    "error-user-not-found",
                    None,
                ))
                .into_response()
            }
        };

    let lang = user_page_lang(host_language.as_deref(), &page_mode, visitor);

    let et: Option<(String, String, String, Option<String>, i32, i32, i32, i32, String, Option<String>, String, String)> = sqlx::query_as(
        "SELECT et.id, et.slug, et.title, et.description, et.duration_min, et.buffer_before, et.buffer_after, et.min_notice_min, et.location_type, et.location_value, et.visibility, et.default_calendar_view
         FROM event_types et
         JOIN accounts a ON a.id = et.account_id
         WHERE a.user_id = ? AND et.slug = ? AND et.enabled = 1",
    )
    .bind(&host_user_id)
    .bind(&slug)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    let (
        et_id,
        et_slug,
        et_title,
        et_desc,
        duration,
        buf_before,
        buf_after,
        min_notice,
        loc_type,
        loc_value,
        visibility,
        default_calendar_view,
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

    // Validate invite token for private event types
    let invite_guest_name;
    let invite_guest_email;
    if visibility == "private" || visibility == "internal" {
        let token = match &query.invite {
            Some(t) => t,
            None => {
                return Html(crate::i18n::translate(lang, "error-invite-required", None))
                    .into_response()
            }
        };
        let invite: Option<(String, String, Option<String>, i32, i32)> = sqlx::query_as(
            "SELECT guest_name, guest_email, expires_at, max_uses, used_count FROM booking_invites WHERE token = ? AND event_type_id = ?",
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
            Some((name, email, expires_at, max_uses, used_count)) => {
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
                invite_guest_name = Some(name);
                invite_guest_email = Some(email);
            }
        }
    } else {
        invite_guest_name = None;
        invite_guest_email = None;
    }

    // Sync calendars if stale before computing availability
    crate::commands::sync::sync_if_stale(&state.pool, &state.secret_key, &host_user_id).await;

    let guest_tz = parse_guest_tz(query.tz.as_deref());
    let host_tz = get_host_tz(&state.pool, &et_id).await;
    let booking_window = get_booking_window(&state.pool, &et_id).await;
    let guest_tz_name = guest_tz.name().to_string();

    let (year, month) = parse_month_param(
        query.month.as_deref(),
        guest_tz,
        host_tz,
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
    let busy = BusySource::Individual(
        fetch_busy_times_for_user(
            &state.pool,
            &host_user_id,
            now_host,
            window_end,
            host_tz,
            Some(&et_id),
        )
        .await,
    );
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
        None,
    )
    .await;

    let days_ctx: Vec<minijinja::Value> = slot_days
        .iter()
        .map(|d| {
            let slots: Vec<minijinja::Value> = d
                .slots
                .iter()
                .map(|s| context! { start => s.start, end => s.end, host_date => s.host_date, host_time => s.host_time, guest_date => s.guest_date })
                .collect();
            context! { date => d.date, label => d.label, slots => slots }
        })
        .collect();

    let available_dates: Vec<String> = slot_days.iter().map(|d| d.date.clone()).collect();

    let tz_options = timezone_options(&guest_tz_name, lang);

    let (meeting_jitsi_label, meeting_webhook_label) = meeting_provider_labels(&state).await;
    let tmpl = match state.templates.get_template("slots.html") {
        Ok(t) => t,
        Err(e) => return internal_error_html("internal", &e).into_response(),
    };
    let rendered = tmpl
        .render(context! {
            meeting_jitsi_label => meeting_jitsi_label,
            meeting_webhook_label => meeting_webhook_label,
            event_type => context! {
                slug => et_slug,
                title => et_title,
                description => et_desc.as_deref().map(crate::utils::render_inline_markdown),
                duration_min => duration,
                location_type => loc_type,
                location_value => loc_value,
            },
            host_name => host_name,
            host_title => host_title.as_deref().unwrap_or(""),
            host_user_id => host_user_id,
            host_has_avatar => host_avatar_path.is_some(),
            host_initials => compute_initials(&host_name),
            username => username,
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
            invite_token => query.invite.as_deref().unwrap_or(""),
            invite_guest_name => invite_guest_name.as_deref().unwrap_or(""),
            invite_guest_email => invite_guest_email.as_deref().unwrap_or(""),
            default_calendar_view => embed.layout.as_deref().filter(|s| !s.is_empty()).unwrap_or(default_calendar_view.as_str()),
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

pub(super) async fn show_book_form_for_user(
    State(state): State<Arc<AppState>>,
    optional_auth: crate::auth::OptionalAuthUser,
    headers: HeaderMap,
    Path((username, slug)): Path<(String, String)>,
    Query(query): Query<BookQuery>,
) -> Response {
    let visitor = crate::i18n::detect_from_headers(&headers);
    let embed = query.embed_params();
    if username.contains('+') {
        return show_dynamic_group_book_form(&state, visitor, &username, &slug, &query)
            .await
            .into_response();
    }

    let et: Option<(String, String, String, Option<String>, i32, String, Option<String>, String, i32, String, Option<String>, String)> = sqlx::query_as(
        "SELECT et.id, et.slug, et.title, et.description, et.duration_min, et.location_type, et.location_value, et.visibility, et.max_additional_guests, et.sms_phone_mode, u.language, u.public_page_language
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
        et_slug,
        et_title,
        et_desc,
        duration,
        loc_type,
        loc_value,
        visibility,
        max_additional_guests,
        sms_phone_mode,
        host_language,
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

    let lang = user_page_lang(host_language.as_deref(), &page_mode, visitor);

    // Validate invite token for private event types
    let invite_guest_name;
    let invite_guest_email;
    if visibility == "private" || visibility == "internal" {
        let token = match &query.invite {
            Some(t) => t,
            None => {
                return Html(crate::i18n::translate(lang, "error-invite-required", None))
                    .into_response()
            }
        };
        let invite: Option<(String, String, Option<String>, i32, i32)> = sqlx::query_as(
            "SELECT guest_name, guest_email, expires_at, max_uses, used_count FROM booking_invites WHERE token = ? AND event_type_id = ?",
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
            Some((name, email, expires_at, max_uses, used_count)) => {
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
                invite_guest_name = Some(name);
                invite_guest_email = Some(email);
            }
        }
    } else {
        invite_guest_name = None;
        invite_guest_email = None;
    }

    let host_name: String = sqlx::query_scalar("SELECT name FROM users WHERE username = ?")
        .bind(&username)
        .fetch_optional(&state.pool)
        .await
        .unwrap_or(None)
        .unwrap_or_else(|| "Host".to_string());

    let guest_tz = parse_guest_tz(query.tz.as_deref());
    let guest_tz_name = guest_tz.name().to_string();
    let phone_default_country: String = crate::sms::default_country_code(&state.pool).await;

    let date = match NaiveDate::parse_from_str(&query.date, "%Y-%m-%d") {
        Ok(d) => d,
        Err(_) => {
            return Html(crate::i18n::translate(
                lang,
                "error-invalid-date-format",
                None,
            ))
            .into_response()
        }
    };
    let time = match NaiveTime::parse_from_str(&query.time, "%H:%M") {
        Ok(t) => t,
        Err(_) => {
            return Html(crate::i18n::translate(
                lang,
                "error-invalid-time-format",
                None,
            ))
            .into_response()
        }
    };
    let end_time = (date.and_time(time) + Duration::minutes(duration as i64))
        .time()
        .format("%H:%M")
        .to_string();
    let date_label = crate::i18n::format_long_date(date, lang);

    let (meeting_jitsi_label, meeting_webhook_label) = meeting_provider_labels(&state).await;
    let tmpl = match state.templates.get_template("book.html") {
        Ok(t) => t,
        Err(e) => return internal_error_html("internal", &e).into_response(),
    };
    let profile_name = optional_auth
        .user
        .as_ref()
        .map(|user| user.name.as_str())
        .unwrap_or("");
    let profile_email = optional_auth
        .user
        .as_ref()
        .map(|user| user.email.as_str())
        .unwrap_or("");
    let form_name = invite_guest_name
        .as_deref()
        .filter(|value| !value.is_empty())
        .unwrap_or(profile_name);
    let form_email = invite_guest_email
        .as_deref()
        .filter(|value| !value.is_empty())
        .unwrap_or(profile_email);
    let captcha = captcha::CaptchaVars::from_config(&*state.captcha_config.read().await);
    let rendered = tmpl
        .render(context! {
            meeting_jitsi_label => meeting_jitsi_label,
            meeting_webhook_label => meeting_webhook_label,
            event_type => context! {
                slug => et_slug,
                title => et_title,
                description => et_desc.as_deref().map(crate::utils::render_inline_markdown),
                duration_min => duration,
                location_type => loc_type,
                location_value => loc_value,
            },
            host_name => host_name,
            username => username,
            sms_phone_mode => sms_phone_mode,
            phone_default_country => phone_default_country,
            form_phone => "",
            date => query.date,
            date_label => date_label,
            time_start => query.time,
            time_end => end_time,
            guest_tz => guest_tz_name,
            error => "",
            form_name => form_name,
            form_email => form_email,
            form_notes => "",
            invite_token => query.invite.as_deref().unwrap_or(""),
            max_additional_guests => max_additional_guests,
            company_link => state.company_link.read().await.clone(),
            captcha_enabled => captcha.enabled,
            captcha_api_endpoint => captcha.api_endpoint,
            captcha_widget_url => captcha.widget_url,
            captcha_wasm_url => captcha.wasm_url,
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

// --- Handlers ---

#[derive(Deserialize)]
pub(super) struct SlotsQuery {
    #[serde(default)]
    month: Option<String>,
    #[serde(default)]
    tz: Option<String>,
    #[serde(default)]
    invite: Option<String>,
    /// When "1", perform full sync + computation (AJAX callback for deferred loading)
    #[serde(default)]
    deferred: Option<String>,
    #[serde(default)]
    embed: Option<String>,
    #[serde(default)]
    layout: Option<String>,
    #[serde(default)]
    theme: Option<String>,
    #[serde(default)]
    brand: Option<String>,
}

impl SlotsQuery {
    fn embed_params(&self) -> EmbedParams {
        EmbedParams {
            embed: self.embed.clone(),
            layout: self.layout.clone(),
            theme: self.theme.clone(),
            brand: self.brand.clone(),
        }
    }
}

/// Parse a "YYYY-MM" month param, returning (year, month_1indexed). Defaults to current month in guest TZ.
/// The month a slot page shows: the `?month=YYYY-MM` parameter when valid,
/// otherwise the month of the first bookable day. That is today, or the start
/// of the bookable date range when it lies ahead, so a trade fair in November
/// opens on November instead of an empty current month.
pub(super) fn parse_month_param(
    param: Option<&str>,
    guest_tz: Tz,
    host_tz: Tz,
    bookable_from: Option<NaiveDate>,
) -> (i32, u32) {
    if let Some(s) = param {
        let parts: Vec<&str> = s.split('-').collect();
        if parts.len() == 2 {
            if let (Ok(y), Ok(m)) = (parts[0].parse::<i32>(), parts[1].parse::<u32>()) {
                if (1..=12).contains(&m) {
                    return (y, m);
                }
            }
        }
    }
    let today = Utc::now().with_timezone(&guest_tz).date_naive();
    // `bookable_from` is a host date; open on the guest's date its first
    // slots fall on, which for a guest west of the host is the day before
    // (a Tokyo fair opening on the 1st starts on the 31st in New York).
    let first = bookable_from
        .map(|from| crate::booking_window::day_start_in(from, host_tz, guest_tz))
        .map_or(today, |from| from.max(today));
    (first.year(), first.month())
}

/// Day window (start_offset, days_ahead), relative to the host's today, for
/// which slots are computed when a month is rendered.
///
/// The window spans the whole Monday-to-Sunday grid around the month, not
/// just the month itself: the week view opens on the week containing today
/// (or the 1st), and that week can run into the next or previous month. It
/// reads its slots from the same payload, so a month-only window left the
/// trailing days of the last week empty until the guest navigated away and
/// back, which loaded the neighbouring month (#228). One extra day on each
/// side absorbs the host/guest timezone offset.
pub(super) fn month_slot_window(
    month_start: NaiveDate,
    month_end: NaiveDate,
    host_today: NaiveDate,
) -> (i32, i32) {
    let grid_start =
        month_start - Duration::days(month_start.weekday().num_days_from_monday() as i64);
    let grid_end =
        month_end + Duration::days(6 - month_end.weekday().num_days_from_monday() as i64);
    let start_offset = ((grid_start - host_today).num_days() - 1).max(0) as i32;
    // Exclusive upper bound, so +2 keeps the last grid day plus one buffer day.
    let end_offset = (grid_end - host_today).num_days() as i32 + 2;
    let days_ahead = (end_offset - start_offset).max(1);
    (start_offset, days_ahead)
}

/// Build month-based slot computation parameters and context variables.
/// Returns (start_offset, days_ahead, month_label, prev_month, next_month, first_weekday, days_in_month, today_date, month_year)
pub(super) fn build_month_params(
    year: i32,
    month: u32,
    host_tz: Tz,
    guest_tz: Tz,
    lang: &str,
    window: BookingWindow,
) -> (
    i32,
    i32,
    String,
    Option<String>,
    Option<String>,
    u32,
    u32,
    String,
    String,
) {
    let now_guest = Utc::now().with_timezone(&guest_tz).naive_local();
    let now_host = Utc::now().with_timezone(&host_tz).naive_local();
    let today_guest = now_guest.date();

    let month_start = NaiveDate::from_ymd_opt(year, month, 1).unwrap();
    let month_end = if month == 12 {
        NaiveDate::from_ymd_opt(year + 1, 1, 1).unwrap()
    } else {
        NaiveDate::from_ymd_opt(year, month + 1, 1).unwrap()
    } - Duration::days(1);

    let days_in_month = (month_end - month_start).num_days() as u32 + 1;

    let (start_offset, days_ahead) = month_slot_window(month_start, month_end, now_host.date());

    let month_label = crate::i18n::format_month_year(month_start, lang);
    let month_year = format!("{}-{:02}", year, month);

    // prev_month: None once the previous month lies entirely before the first
    // bookable day, today or the start of the bookable date range.
    // The window is in host-local days, the calendar in the guest's dates.
    let (first_bookable, last_bookable) = window.effective_date_bounds(now_host.date());
    let earliest = if first_bookable > now_host.date() {
        day_start_in(first_bookable, host_tz, guest_tz)
    } else {
        today_guest
    };
    let earliest_month_start =
        NaiveDate::from_ymd_opt(earliest.year(), earliest.month(), 1).unwrap();
    let prev_month = if month_start > earliest_month_start {
        let (py, pm) = if month == 1 {
            (year - 1, 12)
        } else {
            (year, month - 1)
        };
        Some(format!("{}-{:02}", py, pm))
    } else {
        None
    };

    // next_month: None once the following month starts past the booking
    // window (horizon or end of the date range), so the forward arrow
    // disappears instead of leading to a month with nothing bookable in it.
    let (ny, nm) = if month == 12 {
        (year + 1, 1)
    } else {
        (year, month + 1)
    };
    let next_month_start = NaiveDate::from_ymd_opt(ny, nm, 1).unwrap();
    let next_month = if next_month_start > day_end_in(last_bookable, host_tz, guest_tz) {
        None
    } else {
        Some(format!("{}-{:02}", ny, nm))
    };

    // Monday = 0 for the grid
    let first_weekday = month_start.weekday().num_days_from_monday();
    let today_date = today_guest.format("%Y-%m-%d").to_string();

    (
        start_offset,
        days_ahead,
        month_label,
        prev_month,
        next_month,
        first_weekday,
        days_in_month,
        today_date,
        month_year,
    )
}

pub(super) async fn show_slots(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(slug): Path<String>,
    Query(query): Query<SlotsQuery>,
) -> impl IntoResponse {
    let visitor = crate::i18n::detect_from_headers(&headers);
    #[allow(clippy::type_complexity)]
    let et: Option<(String, String, String, Option<String>, i32, i32, i32, i32, String, String, Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT et.id, et.slug, et.title, et.description, et.duration_min, et.buffer_before, et.buffer_after, et.min_notice_min, et.visibility, et.default_calendar_view, u.language, u.public_page_language
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
        et_slug,
        et_title,
        et_desc,
        duration,
        buf_before,
        buf_after,
        min_notice,
        visibility,
        default_calendar_view,
        host_language,
        page_mode,
    ) = match et {
        Some(e) => e,
        None => {
            return Html(crate::i18n::translate(
                visitor,
                "error-event-type-not-found",
                None,
            ))
        }
    };
    // The owner's public page language, as on `/u/` (#76).
    let lang = user_page_lang(
        host_language.as_deref(),
        page_mode.as_deref().unwrap_or(""),
        visitor,
    );

    // Block private event types on legacy route (use /u/ or /team/ routes with invite token instead)
    if visibility == "private" || visibility == "internal" {
        return Html(crate::i18n::translate(lang, "error-invite-required", None));
    }

    let host_info: Option<(String, String, Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT u.id, u.name, u.title, u.avatar_path FROM users u JOIN accounts a ON a.user_id = u.id JOIN event_types et ON et.account_id = a.id WHERE et.id = ?",
    )
    .bind(&et_id)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    let (host_user_id, host_name, host_title, host_avatar_path) =
        host_info.unwrap_or_else(|| ("".to_string(), "Host".to_string(), None, None));

    // Sync calendars if stale before computing availability
    crate::commands::sync::sync_if_stale(&state.pool, &state.secret_key, &host_user_id).await;

    let guest_tz = parse_guest_tz(query.tz.as_deref());
    let host_tz = get_host_tz(&state.pool, &et_id).await;
    let booking_window = get_booking_window(&state.pool, &et_id).await;
    let guest_tz_name = guest_tz.name().to_string();

    let (year, mo) = parse_month_param(
        query.month.as_deref(),
        guest_tz,
        host_tz,
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
    ) = build_month_params(year, mo, host_tz, guest_tz, lang, booking_window);

    let now_host = Utc::now().with_timezone(&host_tz).naive_local();
    let end_date = now_host.date() + Duration::days((start_offset + days_ahead) as i64);
    let window_end = end_date.and_hms_opt(23, 59, 59).unwrap_or(now_host);
    let busy = BusySource::Individual(
        fetch_busy_times_for_user(
            &state.pool,
            &host_user_id,
            now_host,
            window_end,
            host_tz,
            Some(&et_id),
        )
        .await,
    );
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
        None,
    )
    .await;

    let days_ctx: Vec<minijinja::Value> = slot_days
        .iter()
        .map(|d| {
            let slots: Vec<minijinja::Value> = d
                .slots
                .iter()
                .map(|s| context! { start => s.start, end => s.end, host_date => s.host_date, host_time => s.host_time, guest_date => s.guest_date })
                .collect();
            context! { date => d.date, label => d.label, slots => slots }
        })
        .collect();

    let available_dates: Vec<String> = slot_days.iter().map(|d| d.date.clone()).collect();

    let tz_options = timezone_options(&guest_tz_name, lang);

    let (meeting_jitsi_label, meeting_webhook_label) = meeting_provider_labels(&state).await;
    let tmpl = match state.templates.get_template("slots.html") {
        Ok(t) => t,
        Err(e) => return internal_error_html("internal", &e),
    };
    let rendered = tmpl
        .render(context! {
            meeting_jitsi_label => meeting_jitsi_label,
            meeting_webhook_label => meeting_webhook_label,
            event_type => context! {
                slug => et_slug,
                title => et_title,
                description => et_desc.as_deref().map(crate::utils::render_inline_markdown),
                duration_min => duration,
            },
            host_name => &host_name,
            host_title => host_title.as_deref().unwrap_or(""),
            host_user_id => &host_user_id,
            host_has_avatar => host_avatar_path.is_some(),
            host_initials => compute_initials(&host_name),
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
            default_calendar_view => default_calendar_view,
            company_link => state.company_link.read().await.clone(),
            lang => lang,
        })
        .unwrap_or_else(|e| internal_error_body("template render", &e));

    Html(rendered)
}

#[derive(Deserialize)]
pub(super) struct BookQuery {
    date: String,
    time: String,
    #[serde(default)]
    tz: Option<String>,
    #[serde(default)]
    invite: Option<String>,
    #[serde(default)]
    embed: Option<String>,
    #[serde(default)]
    layout: Option<String>,
    #[serde(default)]
    theme: Option<String>,
    #[serde(default)]
    brand: Option<String>,
}

impl BookQuery {
    fn embed_params(&self) -> EmbedParams {
        EmbedParams {
            embed: self.embed.clone(),
            layout: self.layout.clone(),
            theme: self.theme.clone(),
            brand: self.brand.clone(),
        }
    }
}

pub(super) async fn show_book_form(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(slug): Path<String>,
    Query(query): Query<BookQuery>,
) -> impl IntoResponse {
    let visitor = crate::i18n::detect_from_headers(&headers);
    #[allow(clippy::type_complexity)]
    let et: Option<(String, String, String, Option<String>, i32, i32, String, String, Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT et.id, et.slug, et.title, et.description, et.duration_min, et.max_additional_guests, et.visibility, et.sms_phone_mode, u.language, u.public_page_language
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
        et_slug,
        et_title,
        et_desc,
        duration,
        max_additional_guests,
        visibility,
        sms_phone_mode,
        host_language,
        page_mode,
    ) = match et {
        Some(e) => e,
        None => {
            return Html(crate::i18n::translate(
                visitor,
                "error-event-type-not-found",
                None,
            ))
        }
    };
    // The owner's public page language, as on `/u/` (#76).
    let lang = user_page_lang(
        host_language.as_deref(),
        page_mode.as_deref().unwrap_or(""),
        visitor,
    );

    // Block non-public event types on legacy route
    if visibility == "private" || visibility == "internal" {
        return Html(crate::i18n::translate(lang, "error-invite-required", None));
    }

    let host_name: String = sqlx::query_scalar(
        "SELECT a.name FROM accounts a JOIN event_types et ON et.account_id = a.id WHERE et.id = ?",
    )
    .bind(&et_id)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None)
    .unwrap_or_else(|| "Host".to_string());

    let guest_tz = parse_guest_tz(query.tz.as_deref());
    let guest_tz_name = guest_tz.name().to_string();
    let phone_default_country: String = crate::sms::default_country_code(&state.pool).await;

    let date = match NaiveDate::parse_from_str(&query.date, "%Y-%m-%d") {
        Ok(d) => d,
        Err(_) => {
            return Html(crate::i18n::translate(
                lang,
                "error-invalid-date-format",
                None,
            ))
        }
    };
    let time = match NaiveTime::parse_from_str(&query.time, "%H:%M") {
        Ok(t) => t,
        Err(_) => {
            return Html(crate::i18n::translate(
                lang,
                "error-invalid-time-format",
                None,
            ))
        }
    };
    let end_time = (date.and_time(time) + Duration::minutes(duration as i64))
        .time()
        .format("%H:%M")
        .to_string();
    let date_label = crate::i18n::format_long_date(date, lang);

    let (meeting_jitsi_label, meeting_webhook_label) = meeting_provider_labels(&state).await;
    let tmpl = match state.templates.get_template("book.html") {
        Ok(t) => t,
        Err(e) => return internal_error_html("internal", &e),
    };
    let captcha = captcha::CaptchaVars::from_config(&*state.captcha_config.read().await);
    let rendered = tmpl
        .render(context! {
            meeting_jitsi_label => meeting_jitsi_label,
            meeting_webhook_label => meeting_webhook_label,
            event_type => context! {
                slug => et_slug,
                title => et_title,
                description => et_desc.as_deref().map(crate::utils::render_inline_markdown),
                duration_min => duration,
            },
            host_name => host_name,
            date => query.date,
            date_label => date_label,
            time_start => query.time,
            time_end => end_time,
            guest_tz => guest_tz_name,
            error => "",
            form_name => "",
            form_email => "",
            form_notes => "",
            form_phone => "",
            sms_phone_mode => sms_phone_mode,
            phone_default_country => phone_default_country,
            max_additional_guests => max_additional_guests,
            company_link => state.company_link.read().await.clone(),
            captcha_enabled => captcha.enabled,
            captcha_api_endpoint => captcha.api_endpoint,
            captcha_widget_url => captcha.widget_url,
            captcha_wasm_url => captcha.wasm_url,
            lang => lang,
        })
        .unwrap_or_else(|e| internal_error_body("template render", &e));

    Html(rendered)
}
