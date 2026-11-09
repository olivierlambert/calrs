//! Dashboard pages: overview, event types, bookings (cancel/confirm), settings and avatar.

use super::*;

// --- Root redirect ---

pub(super) async fn root_redirect() -> impl IntoResponse {
    Redirect::to("/auth/login")
}

// --- Dashboard (overview) ---

/// Where "Open your calendar" goes: the calendar's own web app when the
/// source tells us (Outlook on the web for Microsoft, Google, an Exchange
/// server's OWA, the CalDAV server's origin, which is the web interface for
/// Nextcloud, BlueMind and Kerio), else the sources page. calrs is the
/// booking layer on top of it.
pub(super) fn calendar_web_url(
    url: &str,
    auth_type: &str,
    provider: &str,
    microsoft_user_id: Option<&str>,
) -> String {
    if provider == crate::providers::factory::kinds::MICROSOFT {
        return if microsoft_user_id.is_some_and(crate::microsoft_oauth::is_personal_account) {
            "https://outlook.live.com/calendar/".to_string()
        } else {
            "https://outlook.office.com/calendar/".to_string()
        };
    }
    if auth_type == "oauth2" {
        return "https://calendar.google.com/".to_string();
    }
    let Ok(parsed) = reqwest::Url::parse(url) else {
        return "/dashboard/sources".to_string();
    };
    if !matches!(parsed.scheme(), "https" | "http") || parsed.host_str().is_none() {
        return "/dashboard/sources".to_string();
    }
    let origin = parsed.origin().ascii_serialization();
    if provider == "ews" {
        format!("{origin}/owa/")
    } else {
        format!("{origin}/")
    }
}

pub(super) async fn dashboard(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
) -> impl IntoResponse {
    let user = &auth_user.user;
    let lang = auth_user.lang;

    let favorites_filter = DirectoryFilter {
        favorites_only: true,
        limit: Some(OVERVIEW_FAVORITES),
        ..DirectoryFilter::default()
    };
    let (favorites, favorites_total) =
        match directory_page(&state.pool, &user.id, &favorites_filter).await {
            Ok(page) => (page.entries, page.total),
            Err(e) => return internal_error_html("load dashboard favorites", &e),
        };

    let event_type_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM event_types et JOIN accounts a ON a.id = et.account_id WHERE a.user_id = ? AND et.team_id IS NULL")
            .bind(&user.id)
            .fetch_one(&state.pool)
            .await
            .unwrap_or(0);

    #[allow(clippy::type_complexity)]
    let pending_bookings: Vec<(
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
        Option<String>,
    )> = sqlx::query_as(
        "SELECT b.id, b.guest_name, b.guest_email, CASE b.time_version WHEN 1 THEN b.start_at ELSE rtrim(b.start_at, 'Z') END AS start_at, CASE b.time_version WHEN 1 THEN b.end_at ELSE rtrim(b.end_at, 'Z') END AS end_at, et.title,
                COALESCE(NULLIF(et.timezone, ''), u.timezone) AS stored_tz,
                COALESCE(b.notes, '') AS notes,
                b.guest_phone, et.sms_phone_mode, b.pending_expires_at, b.host_notes
         FROM bookings b
         JOIN event_types et ON et.id = b.event_type_id
         JOIN accounts a ON a.id = et.account_id
         JOIN users u ON u.id = a.user_id
         WHERE a.user_id = ? AND b.status = 'pending'
         ORDER BY b.pending_expires_at IS NULL, b.pending_expires_at, b.start_at",
    )
    .bind(&user.id)
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();

    let source: Option<(String, String, String, Option<String>)> = sqlx::query_as(
        "SELECT cs.url, COALESCE(cs.auth_type, ''), COALESCE(cs.provider_type, 'caldav'),
                cs.microsoft_user_id
         FROM caldav_sources cs JOIN accounts a ON a.id = cs.account_id
         WHERE a.user_id = ? ORDER BY cs.enabled DESC, cs.created_at LIMIT 1",
    )
    .bind(&user.id)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);
    let source_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM caldav_sources cs JOIN accounts a ON a.id = cs.account_id WHERE a.user_id = ?")
            .bind(&user.id)
            .fetch_one(&state.pool)
            .await
            .unwrap_or(0);

    // Own public meeting types, for "Your booking links".
    let own_links: Vec<(String, String, i32)> = if user.username.is_some() {
        sqlx::query_as(
            "SELECT et.slug, et.title, et.duration_min FROM event_types et
             JOIN accounts a ON a.id = et.account_id
             WHERE a.user_id = ? AND et.team_id IS NULL AND et.enabled = 1 AND et.visibility = 'public'
             ORDER BY et.created_at LIMIT 6",
        )
        .bind(&user.id)
        .fetch_all(&state.pool)
        .await
        .unwrap_or_default()
    } else {
        Vec::new()
    };

    let tmpl = match state.templates.get_template("dashboard_overview.html") {
        Ok(t) => t,
        Err(e) => return internal_error_html("template render", &e),
    };

    // Everything below is in the host's profile timezone.
    let host = user.timezone.parse::<Tz>().unwrap_or(Tz::UTC);
    let now = Utc::now();
    let now_local = now.with_timezone(&host).naive_local();
    let today = now_local.date();

    let pending_ctx: Vec<minijinja::Value> = pending_bookings
        .iter()
        .filter_map(
            |(
                id,
                name,
                email,
                start,
                end,
                title,
                stored_tz,
                notes,
                phone,
                sms_phone_mode,
                expires,
                host_notes,
            )| {
                let stored = stored_tz.parse::<Tz>().unwrap_or(host);
                let s = crate::booking_time::local(start, stored, host)?;
                let e = crate::booking_time::local(end, stored, host)?;
                let expires = expires
                    .as_deref()
                    .and_then(crate::booking_time::naive)
                    .map(|utc| host.from_utc_datetime(&utc).naive_local());
                Some(context! {
                    id => id,
                    initials => overview_stats::initials(name),
                    hue => overview_stats::face_hue(name),
                    guest_name => name,
                    guest_email => email,
                    // Same rule as the bookings page: shown while the event
                    // type collects a phone, so the host can call first (#231).
                    guest_phone => if crate::sms::collects_phone(sms_phone_mode) { phone.as_deref().unwrap_or("") } else { "" },
                    event_title => title,
                    notes => notes.trim(),
                    // Owner-scoped like the bookings page (#173).
                    host_notes => host_notes.as_deref().unwrap_or(""),
                    when => overview_stats::pending_row(lang, s, e, expires, now_local),
                })
            },
        )
        .collect();
    // Every pending row counts, like the sidebar hint, even one whose times
    // cannot be read and so has no row in the list.
    let pending_count = pending_bookings.len() as i64;

    let week_start = week_start::user(&state.pool, &user.id).await.effective();
    let rows = match overview_stats::fetch_stat_rows(&state.pool, &user.id, host, now).await {
        Ok(rows) => rows,
        Err(e) => return internal_error_html("load overview stats", &e),
    };
    let stats = overview_stats::compute(&rows, now_local, week_start);
    let stats_ctx = overview_stats::stats_context(&stats, lang);
    let agenda = overview_stats::agenda(&state.pool, &user.id, host, now, lang).await;

    // This week in the week strip: one query for the bookings and one for
    // the calendar over the whole span, today included.
    let hours = overview_stats::load_event_type_hours(&state.pool, &user.id).await;
    let week_from =
        (today - Duration::days(week_start.offset(today).into())).and_time(NaiveTime::MIN);
    let week_to = week_from + Duration::days(7);
    let day_from = today.and_time(NaiveTime::MIN);
    let day_to = day_from + Duration::days(1);
    let (span_from, span_to) = (week_from.min(day_from), week_to.max(day_to));
    let week_bookings =
        overview_stats::fetch_day_bookings(&state.pool, &user.id, host, span_from, span_to).await;
    let busy =
        overview_stats::fetch_calendar_busy(&state.pool, &user.id, host, span_from, span_to).await;
    // Time off closes the hours it covers in the week (#46).
    let time_off =
        crate::time_off::busy_intervals(&state.pool, &user.id, span_from, span_to, host).await;
    let week = overview_stats::week_context(
        lang,
        &hours,
        &week_bookings,
        &busy,
        &time_off,
        host,
        now_local,
        week_start,
    );
    // Confirmed meetings later today after the next one, for the sentence
    // under the greeting (the agenda's scope, from the week's bookings).
    let more_today = agenda.first().map_or(0, |next| {
        let left_today = week_bookings
            .iter()
            .filter(|b| !b.pending && b.start.date() == today && b.end > now_local)
            .count() as i64;
        let next_is_today = next.get_attr("is_today").is_ok_and(|v| v.is_true());
        (left_today - i64::from(next_is_today)).max(0)
    });
    let first_name = user
        .name
        .split_whitespace()
        .next()
        .map(str::to_string)
        .or_else(|| user.username.clone())
        .unwrap_or_else(|| user.email.split('@').next().unwrap_or_default().to_string());
    let masthead = overview_stats::masthead_context(
        lang,
        now_local,
        &first_name,
        agenda.first(),
        more_today,
        pending_count,
    );

    let links_ctx: Vec<minijinja::Value> = own_links
        .iter()
        .map(|(slug, title, duration)| {
            let username = user.username.as_deref().unwrap_or_default();
            context! {
                title => title,
                duration => overview_stats::duration_label(lang, i64::from(*duration)),
                path => format!("/u/{username}/{slug}"),
            }
        })
        .collect();

    let calendar_url = source
        .as_ref()
        .map(|(url, auth, provider, microsoft_user)| {
            calendar_web_url(url, auth, provider, microsoft_user.as_deref())
        });
    let (impersonating, impersonating_name, _) = impersonation_ctx(&auth_user);

    Html(
        tmpl.render(context! {
            sidebar => sidebar_context(&auth_user, "overview"),
            lang => lang,
            user_role => user.role,
            username => user.username,
            event_type_count => event_type_count,
            pending_count => pending_count,
            source_count => source_count,
            calendar_url => calendar_url,
            calendar_external => calendar_url.as_deref().is_some_and(|u| u.starts_with("http")),
            pending_bookings => pending_ctx,
            favorites => favorites,
            favorites_total => favorites_total,
            own_links => links_ctx,
            stats => stats_ctx,
            agenda => agenda,
            mast => masthead,
            week => week,
            impersonating => impersonating,
            impersonating_name => impersonating_name,
        })
        .unwrap_or_else(|e| internal_error_body("template render", &e)),
    )
}

// --- Sidebar hints ---

/// The live hints next to two sidebar entries ("3 to decide", "1 failing"),
/// fetched by the shell after the page renders so the 30 handlers that draw
/// the sidebar need no extra query. Empty strings when there is nothing to
/// say: a healthy state is silence.
pub(super) async fn sidebar_hints(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
) -> impl IntoResponse {
    let user = &auth_user.user;
    let lang = auth_user.lang;
    let pending: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM bookings b
         JOIN event_types et ON et.id = b.event_type_id
         JOIN accounts a ON a.id = et.account_id
         WHERE a.user_id = ? AND b.status = 'pending'",
    )
    .bind(&user.id)
    .fetch_one(&state.pool)
    .await
    .unwrap_or(0);
    let failing: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM caldav_sources cs
         JOIN accounts a ON a.id = cs.account_id
         WHERE a.user_id = ? AND cs.enabled = 1 AND cs.last_sync_error IS NOT NULL",
    )
    .bind(&user.id)
    .fetch_one(&state.pool)
    .await
    .unwrap_or(0);
    let count_label = |key: &str, n: i64| {
        if n == 0 {
            return String::new();
        }
        let mut args = fluent_bundle::FluentArgs::new();
        args.set("count", crate::i18n::number(n as f64));
        crate::i18n::translate(lang, key, Some(&args))
    };
    let body = serde_json::json!({
        "bookings": count_label("dash-hint-to-decide", pending),
        "sources": count_label("dash-hint-failing", failing),
    });
    (
        [(axum::http::header::CACHE_CONTROL, "no-store")],
        axum::Json(body),
    )
}

// --- Dashboard: Event Types ---

pub(super) async fn dashboard_event_types(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
    headers: HeaderMap,
) -> impl IntoResponse {
    let user = &auth_user.user;
    let lang = crate::i18n::resolve(user.language.as_deref(), &headers);

    let event_types: Vec<(String, String, String, i32, bool, i32, i64, String)> = sqlx::query_as(
        "SELECT et.id, et.slug, et.title, et.duration_min, et.enabled, et.requires_confirmation,
                (SELECT COUNT(*) FROM bookings b WHERE b.event_type_id = et.id AND b.status IN ('confirmed', 'pending')) as active_bookings,
                et.visibility
         FROM event_types et
         JOIN accounts a ON a.id = et.account_id
         WHERE a.user_id = ? AND et.team_id IS NULL
         ORDER BY et.created_at",
    )
    .bind(&user.id)
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();

    let is_admin = user.role == "admin";

    // Teams the user is an admin of — used for `can_manage` (edit/toggle/delete)
    // and "Create team event type" enablement. Regular team members can view but
    // not manage.
    let admin_team_ids: std::collections::HashSet<String> = if is_admin {
        std::collections::HashSet::new() // global admins can manage all
    } else {
        let ids: Vec<(String,)> =
            sqlx::query_as("SELECT team_id FROM team_members WHERE user_id = ? AND role = 'admin'")
                .bind(&user.id)
                .fetch_all(&state.pool)
                .await
                .unwrap_or_default();
        ids.into_iter().map(|(id,)| id).collect()
    };

    let team_event_types: Vec<(
        String,
        String,
        String,
        i32,
        bool,
        String,
        String,
        i64,
        String,
        String,
        String,
    )> = if is_admin {
        sqlx::query_as(
            "SELECT et.id, et.slug, et.title, et.duration_min, et.enabled, t.name, t.slug,
                    (SELECT COUNT(*) FROM bookings b WHERE b.event_type_id = et.id AND b.status IN ('confirmed', 'pending')) as active_bookings,
                    et.visibility, t.id, et.scheduling_mode
             FROM event_types et
             JOIN teams t ON t.id = et.team_id
             WHERE et.team_id IS NOT NULL
             ORDER BY t.name, et.created_at",
        )
        .fetch_all(&state.pool)
        .await
        .unwrap_or_default()
    } else {
        sqlx::query_as(
            "SELECT et.id, et.slug, et.title, et.duration_min, et.enabled, t.name, t.slug,
                    (SELECT COUNT(*) FROM bookings b WHERE b.event_type_id = et.id AND b.status IN ('confirmed', 'pending')) as active_bookings,
                    et.visibility, t.id, et.scheduling_mode
             FROM event_types et
             JOIN teams t ON t.id = et.team_id
             JOIN team_members tm ON tm.team_id = t.id
             WHERE tm.user_id = ?
             ORDER BY t.name, et.created_at",
        )
        .bind(&user.id)
        .fetch_all(&state.pool)
        .await
        .unwrap_or_default()
    };

    // Whether user can create new team event types (global admin or team admin of at least one team)
    let can_create_team_et = is_admin || !admin_team_ids.is_empty();
    let webhooks_allowed = can_use_webhooks(&state.pool, user).await;

    let tmpl = match state.templates.get_template("dashboard_event_types.html") {
        Ok(t) => t,
        Err(e) => return internal_error_html("template render", &e),
    };

    // Resource names per event type, for the "resources" badge.
    let resource_name_rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT etr.event_type_id, r.name FROM event_type_resources etr
         JOIN resources r ON r.id = etr.resource_id ORDER BY r.name",
    )
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();
    let mut resource_names_map: std::collections::HashMap<String, Vec<String>> =
        std::collections::HashMap::new();
    for (etid, rname) in resource_name_rows {
        resource_names_map.entry(etid).or_default().push(rname);
    }
    let resource_names_for = |etid: &str| {
        resource_names_map
            .get(etid)
            .map(|v| v.join(", "))
            .unwrap_or_default()
    };

    // Bookable date range badge per event type: the range, and whether it is
    // still ahead, running, or over. One query for the listed event types,
    // with the same timezone fallback as `get_host_tz`.
    let listed_ids: Vec<&String> = event_types
        .iter()
        .map(|row| &row.0)
        .chain(team_event_types.iter().map(|row| &row.0))
        .collect();
    let mut date_range_map: HashMap<String, (String, &'static str)> = HashMap::new();
    if !listed_ids.is_empty() {
        let placeholders = vec!["?"; listed_ids.len()].join(", ");
        let sql = format!(
            "SELECT et.id, et.booking_horizon_days, et.bookable_from, et.bookable_until,
                    COALESCE(NULLIF(et.timezone, ''), u.timezone)
             FROM event_types et
             LEFT JOIN accounts a ON a.id = et.account_id
             LEFT JOIN users u ON u.id = a.user_id
             WHERE (et.bookable_from IS NOT NULL OR et.bookable_until IS NOT NULL)
               AND et.id IN ({placeholders})"
        );
        let mut query = sqlx::query_as::<
            _,
            (
                String,
                Option<i32>,
                Option<String>,
                Option<String>,
                Option<String>,
            ),
        >(&sql);
        for id in &listed_ids {
            query = query.bind(*id);
        }
        let ranged = query.fetch_all(&state.pool).await.unwrap_or_default();
        for (etid, horizon_days, from, until, tz) in ranged {
            let window = BookingWindow {
                horizon_days,
                bookable_from: parse_optional_date(from.as_deref()),
                bookable_until: parse_optional_date(until.as_deref()),
            };
            let tz = tz
                .and_then(|name| name.parse::<Tz>().ok())
                .unwrap_or_else(server_tz);
            if let Some(badge) = date_range_badge(&window, today_in(tz), auth_user.lang) {
                date_range_map.insert(etid, badge);
            }
        }
    }
    let date_range_for = |etid: &str| date_range_map.get(etid).cloned().unwrap_or_default();

    // Build a single unified list: personal event types first, then team ones
    let mut all_et_ctx: Vec<minijinja::Value> = Vec::new();

    for (id, slug, title, duration, enabled, req_conf, active_bookings, vis) in &event_types {
        all_et_ctx.push(context! {
            id => id, slug => slug, title => title, duration_min => duration,
            enabled => enabled, requires_confirmation => *req_conf != 0,
            active_bookings => active_bookings, visibility => vis,
            is_team => false, can_manage => true,
            edit_url => format!("/dashboard/event-types/{}/edit", slug),
            toggle_url => format!("/dashboard/event-types/{}/toggle", slug),
            delete_url => format!("/dashboard/event-types/{}/delete", slug),
            duplicate_url => format!("/dashboard/event-types/{}/duplicate", slug),
            overrides_url => overrides_url(id),
            embed_url => format!("/dashboard/event-types/{}/embed", slug),
            resource_names => resource_names_for(id),
            date_range => date_range_for(id).0,
            date_range_state => date_range_for(id).1,
            view_url => if vis != "private" { user.username.as_ref().map(|u| format!("/u/{}/{}", u, slug)) } else { None::<String> },
        });
    }

    for (
        id,
        slug,
        title,
        duration,
        enabled,
        team_name,
        team_slug,
        active_bookings,
        vis,
        team_id,
        scheduling_mode,
    ) in &team_event_types
    {
        let can_manage = is_admin || admin_team_ids.contains(team_id);
        all_et_ctx.push(context! {
            id => id, slug => slug, title => title, duration_min => duration,
            enabled => enabled, active_bookings => active_bookings, visibility => vis,
            is_team => true, team_name => team_name, team_slug => team_slug,
            team_id => team_id, scheduling_mode => scheduling_mode, can_manage => can_manage,
            edit_url => format!("/dashboard/group-event-types/{}/{}/edit", team_id, slug),
            toggle_url => format!("/dashboard/group-event-types/{}/{}/toggle", team_id, slug),
            delete_url => format!("/dashboard/group-event-types/{}/{}/delete", team_id, slug),
            duplicate_url => format!("/dashboard/group-event-types/{}/{}/duplicate", team_id, slug),
            embed_url => format!("/dashboard/group-event-types/{}/{}/embed", team_id, slug),
            overrides_url => overrides_url(id),
            resource_names => resource_names_for(id),
            date_range => date_range_for(id).0,
            date_range_state => date_range_for(id).1,
            // Team event types always get a view link — logged-in team members
            // can view private/internal slots without an invite token.
            view_url => Some(format!("/team/{}/{}", team_slug, slug)),
        });
    }

    let (impersonating, impersonating_name, _) = impersonation_ctx(&auth_user);

    Html(
        tmpl.render(context! {
            sidebar => sidebar_context(&auth_user, "event-types"),
            lang => auth_user.lang,
            username => user.username,
            all_event_types => all_et_ctx,
            has_any => !event_types.is_empty() || !team_event_types.is_empty(),
            can_create_team_et => can_create_team_et,
            webhooks_allowed => webhooks_allowed,
            impersonating => impersonating,
            impersonating_name => impersonating_name,
            lang => lang,
        })
        .unwrap_or_else(|e| internal_error_body("template render", &e)),
    )
}

// --- Dashboard: Bookings ---

pub(super) async fn dashboard_bookings(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
    Query(query): Query<std::collections::HashMap<String, String>>,
) -> impl IntoResponse {
    let user = &auth_user.user;
    let error_message = match query.get("error").map(String::as_str) {
        Some("resource-busy") => {
            crate::i18n::translate(auth_user.lang, "bae-body-resource-gone", None)
        }
        Some("host-busy") => crate::i18n::translate(auth_user.lang, "bae-body-host-busy", None),
        Some("host-not-synced") => {
            crate::i18n::translate(auth_user.lang, "bae-body-host-not-synced", None)
        }
        Some("no-bookable-dates") => {
            crate::i18n::translate(auth_user.lang, "bookings-error-no-bookable-dates", None)
        }
        Some("reschedule-pending") => {
            crate::i18n::translate(auth_user.lang, "bookings-error-reschedule-pending", None)
        }
        Some("reschedule-resend-too-soon") => crate::i18n::translate(
            auth_user.lang,
            "bookings-error-reschedule-resend-too-soon",
            None,
        ),
        Some(_) => crate::i18n::translate(auth_user.lang, "bookings-error-generic", None),
        None => String::new(),
    };

    let pending_bookings: Vec<(
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
        Option<String>,
        String,
        Option<String>,
    )> = sqlx::query_as(
        "SELECT b.id, b.guest_name, b.guest_email, CASE b.time_version WHEN 1 THEN b.start_at ELSE rtrim(b.start_at, 'Z') END AS start_at, CASE b.time_version WHEN 1 THEN b.end_at ELSE rtrim(b.end_at, 'Z') END AS end_at, et.title,
                COALESCE(NULLIF(et.timezone, ''), u.timezone) AS stored_tz,
                COALESCE(NULLIF(b.guest_timezone, ''), 'UTC') AS guest_tz,
                COALESCE(r.name, '') AS resource_name,
                COALESCE(b.notes, '') AS notes,
                b.guest_phone, et.sms_phone_mode, b.host_notes
         FROM bookings b
         JOIN event_types et ON et.id = b.event_type_id
         JOIN accounts a ON a.id = et.account_id
         JOIN users u ON u.id = a.user_id
         LEFT JOIN resources r ON r.id = b.assigned_resource_id
         WHERE a.user_id = ? AND b.status = 'pending'
         ORDER BY b.start_at",
    )
    .bind(&user.id)
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();

    let upcoming_bookings: Vec<(String, String, String, Option<String>, String, String, String, i32, String, String, String, String, String, Option<String>, Option<String>)> =
        sqlx::query_as(
            "SELECT b.id, b.guest_name, b.guest_email, b.guest_phone, CASE b.time_version WHEN 1 THEN b.start_at ELSE rtrim(b.start_at, 'Z') END AS start_at, CASE b.time_version WHEN 1 THEN b.end_at ELSE rtrim(b.end_at, 'Z') END AS end_at, et.title, b.reschedule_by_host,
                    COALESCE(NULLIF(et.timezone, ''), u.timezone) AS stored_tz,
                    COALESCE(NULLIF(b.guest_timezone, ''), 'UTC') AS guest_tz,
                    COALESCE(r.name, '') AS resource_name,
                    et.sms_phone_mode,
                    COALESCE(b.notes, '') AS notes,
                    b.writeback_error, b.host_notes
         FROM bookings b
         JOIN event_types et ON et.id = b.event_type_id
         JOIN accounts a ON a.id = et.account_id
         JOIN users u ON u.id = a.user_id
         LEFT JOIN resources r ON r.id = b.assigned_resource_id
         WHERE a.user_id = ? AND b.status = 'confirmed'
           AND b.start_at >= date('now', '-1 day') AND datetime(b.start_at) >= datetime('now')
         ORDER BY b.start_at
         LIMIT 50",
        )
        .bind(&user.id)
        .fetch_all(&state.pool)
        .await
        .unwrap_or_default();

    // Claimable bookings: unclaimed bookings on event types watched by the user's teams.
    // For team event types et.account_id may be null, so the timezone falls back to the
    // event type's own tz column (always set since migration 046's backfill).
    let claimable_bookings: Vec<(String, String, String, String, String, String, String, String, String, String, String)> =
        sqlx::query_as(
            "SELECT b.id, b.guest_name, b.guest_email, CASE b.time_version WHEN 1 THEN b.start_at ELSE rtrim(b.start_at, 'Z') END AS start_at, CASE b.time_version WHEN 1 THEN b.end_at ELSE rtrim(b.end_at, 'Z') END AS end_at, et.title, t.name, bct.token, \
                    COALESCE(NULLIF(et.timezone, ''), 'UTC') AS stored_tz, \
                    COALESCE(NULLIF(b.guest_timezone, ''), 'UTC') AS guest_tz, \
                    COALESCE(b.notes, '') AS notes \
             FROM bookings b \
             JOIN event_types et ON et.id = b.event_type_id \
             JOIN event_type_watchers ew ON ew.event_type_id = et.id \
             JOIN team_members tm ON tm.team_id = ew.team_id \
             JOIN teams t ON t.id = ew.team_id \
             JOIN booking_claim_tokens bct ON bct.booking_id = b.id AND bct.user_id = tm.user_id AND bct.used_at IS NULL \
             WHERE tm.user_id = ? AND b.status = 'confirmed' AND b.claimed_by_user_id IS NULL \
             AND b.start_at >= date('now', '-1 day') AND datetime(b.start_at) >= datetime('now') \
             AND bct.expires_at > datetime('now') \
             ORDER BY b.start_at \
             LIMIT 50",
        )
        .bind(&user.id)
        .fetch_all(&state.pool)
        .await
        .unwrap_or_default();

    // Recent: the meetings the user hosted in the last 30 days, so a host
    // can write or reread their notes after the call (#173). Cancelled:
    // the cancelled and declined bookings due from 30 days ago on, future
    // ones included, so the notes of a meeting cancelled for a later date
    // stay reachable too. Both are scoped by the host-notes rule rather than
    // the owner rule of the sections above, since that is what the rows lead
    // to: an assigned member reaches their own bookings here too.
    let recent_bookings = hosted_bookings(
        &state.pool,
        &user.id,
        "b.status = 'confirmed' AND datetime(b.start_at) < datetime('now')",
        30,
    )
    .await;
    let cancelled_bookings = hosted_bookings(
        &state.pool,
        &user.id,
        "b.status IN ('cancelled', 'declined')",
        50,
    )
    .await;

    let tmpl = match state.templates.get_template("dashboard_bookings.html") {
        Ok(t) => t,
        Err(e) => return internal_error_html("template render", &e),
    };

    // The guests' answers to the booking questions (#210), one query for
    // the page. Not for the claimable rows: a watcher reads them once the
    // booking is theirs, as with the host notes.
    let answer_ids: Vec<&str> = pending_bookings
        .iter()
        .map(|b| b.0.as_str())
        .chain(upcoming_bookings.iter().map(|b| b.0.as_str()))
        .chain(recent_bookings.iter().map(|b| b.0.as_str()))
        .chain(cancelled_bookings.iter().map(|b| b.0.as_str()))
        .collect();
    let answers = questions::answers_by_booking(&state.pool, &answer_ids).await;
    let answer_rows = |id: &str| {
        questions::display_rows(
            answers.get(id).map(Vec::as_slice).unwrap_or(&[]),
            auth_user.lang,
            crate::email::AnswerReader::Host,
        )
    };

    let host_tz = &user.timezone;
    let claimable_ctx: Vec<minijinja::Value> = claimable_bookings
        .iter()
        .map(
            |(id, name, email, start, end, title, team_name, token, stored_tz, guest_tz, notes)| {
                let (primary, secondary) =
                    format_booking_for_dashboard(start, end, stored_tz, host_tz, guest_tz);
                let when = dashboard_when(start, end, stored_tz, host_tz, guest_tz, auth_user.lang);
                context! {
                    id => id,
                    guest_name => name,
                    guest_email => email,
                    start_at => primary,
                    start_at_guest => secondary,
                    when => when,
                    event_title => title,
                    team_name => team_name,
                    claim_token => token,
                    notes => notes.trim(),
                }
            },
        )
        .collect();

    let pending_ctx: Vec<minijinja::Value> = pending_bookings
        .iter()
        .map(
            |(
                id,
                name,
                email,
                start,
                end,
                title,
                stored_tz,
                guest_tz,
                resource_name,
                notes,
                phone,
                sms_phone_mode,
                host_notes,
            )| {
                let (primary, secondary) =
                    format_booking_for_dashboard(start, end, stored_tz, host_tz, guest_tz);
                let when = dashboard_when(start, end, stored_tz, host_tz, guest_tz, auth_user.lang);
                context! {
                    id => id,
                    guest_name => name,
                    guest_email => email,
                    // Shown while pending too: the host may need to call the
                    // guest before deciding (#231).
                    guest_phone => if crate::sms::collects_phone(sms_phone_mode) { phone.as_deref().unwrap_or("") } else { "" },
                    start_at => primary,
                    start_at_guest => secondary,
                    when => when,
                    event_title => title,
                    resource_name => resource_name,
                    notes => notes.trim(),
                    answers => answer_rows(id.as_str()),
                    // The owner always reads them: this page lists the
                    // bookings of their own event types (#173).
                    host_notes => host_notes.as_deref().unwrap_or(""),
                }
            },
        )
        .collect();

    let bookings_ctx: Vec<minijinja::Value> = upcoming_bookings
        .iter()
        .map(
            |(
                id,
                name,
                email,
                phone,
                start,
                end,
                title,
                resched,
                stored_tz,
                guest_tz,
                resource_name,
                sms_phone_mode,
                notes,
                writeback_error,
                host_notes,
            )| {
                let (primary, secondary) =
                    format_booking_for_dashboard(start, end, stored_tz, host_tz, guest_tz);
                let when = dashboard_when(start, end, stored_tz, host_tz, guest_tz, auth_user.lang);
                context! {
                    id => id,
                    // The booking is confirmed but not on the host's
                    // calendar; retried in the background (#162).
                    writeback_error => writeback_error.as_deref().unwrap_or(""),
                    guest_name => name,
                    guest_email => email,
                    guest_phone => if crate::sms::collects_phone(sms_phone_mode) { phone.as_deref().unwrap_or("") } else { "" },
                    start_at => primary,
                    start_at_guest => secondary,
                    when => when,
                    event_title => title,
                    awaiting_reschedule => *resched != 0,
                    resource_name => resource_name,
                    notes => notes.trim(),
                    answers => answer_rows(id.as_str()),
                    host_notes => host_notes.as_deref().unwrap_or(""),
                }
            },
        )
        .collect();

    let recent_ctx = hosted_bookings_ctx(&recent_bookings, host_tz, auth_user.lang, &answer_rows);
    let cancelled_ctx =
        hosted_bookings_ctx(&cancelled_bookings, host_tz, auth_user.lang, &answer_rows);

    let (impersonating, impersonating_name, _) = impersonation_ctx(&auth_user);

    Html(
        tmpl.render(context! {
            sidebar => sidebar_context(&auth_user, "bookings"),
            lang => auth_user.lang,
            error_message => error_message,
            recent_bookings => recent_ctx,
            cancelled_bookings => cancelled_ctx,
            pending_bookings => pending_ctx,
            claimable_bookings => claimable_ctx,
            bookings => bookings_ctx,
            impersonating => impersonating,
            impersonating_name => impersonating_name,
        })
        .unwrap_or_else(|e| internal_error_body("template render", &e)),
    )
}

/// A booking row of the Recent and Cancelled sections of the bookings page.
type HostedBookingRow = (
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
    Option<String>,
);

/// The bookings `user_id` hosts under the host-notes rule (`NOTE_HOSTS`),
/// due from 30 days ago on and matching `filter`, a fixed SQL condition over
/// `b` (never user input). The `limit` rows kept are the ones closest to
/// now, so many far-future cancellations cannot push out last week's, and
/// they are returned latest first.
async fn hosted_bookings(
    pool: &SqlitePool,
    user_id: &str,
    filter: &'static str,
    limit: i64,
) -> Vec<HostedBookingRow> {
    let sql = format!(
        "SELECT id, guest_name, guest_email, start_at, end_at, title, tz, guest_tz,
                status, notes, host_notes
         FROM (
         SELECT b.id, b.guest_name, b.guest_email,
                CASE b.time_version WHEN 1 THEN b.start_at ELSE rtrim(b.start_at, 'Z') END
                    AS start_at,
                CASE b.time_version WHEN 1 THEN b.end_at ELSE rtrim(b.end_at, 'Z') END
                    AS end_at,
                et.title AS title, COALESCE(NULLIF(et.timezone, ''), u.timezone, 'UTC') AS tz,
                COALESCE(NULLIF(b.guest_timezone, ''), 'UTC') AS guest_tz, b.status AS status,
                COALESCE(b.notes, '') AS notes, b.host_notes AS host_notes,
                b.start_at AS sort_at
         FROM bookings b
         JOIN event_types et ON et.id = b.event_type_id
         LEFT JOIN accounts a ON a.id = et.account_id
         LEFT JOIN users u ON u.id = a.user_id
         WHERE b.start_at >= date('now', '-30 days')
           AND {filter}
           AND {NOTE_HOSTS}
         ORDER BY abs(julianday(b.start_at) - julianday('now'))
         LIMIT ?
         )
         ORDER BY sort_at DESC"
    );
    sqlx::query_as(&sql)
        .bind(user_id)
        .bind(user_id)
        .bind(user_id)
        .bind(user_id)
        .bind(limit)
        .fetch_all(pool)
        .await
        .unwrap_or_default()
}

fn hosted_bookings_ctx(
    rows: &[HostedBookingRow],
    host_tz: &str,
    lang: &str,
    answer_rows: &dyn Fn(&str) -> Vec<minijinja::Value>,
) -> Vec<minijinja::Value> {
    rows.iter()
        .map(
            |(
                id,
                name,
                email,
                start,
                end,
                title,
                stored_tz,
                guest_tz,
                status,
                notes,
                host_notes,
            )| {
                let ended = status == "cancelled" || status == "declined";
                context! {
                    id => id,
                    guest_name => name,
                    guest_email => email,
                    when => dashboard_when(start, end, stored_tz, host_tz, guest_tz, lang),
                    event_title => title,
                    notes => notes.trim(),
                    answers => answer_rows(id.as_str()),
                    host_notes => host_notes.as_deref().unwrap_or(""),
                    status_mark => if ended { "x" } else { "c" },
                    status_label => match status.as_str() {
                        "cancelled" => crate::i18n::translate(lang, "admin-st-cancelled", None),
                        "declined" => crate::i18n::translate(lang, "admin-st-declined", None),
                        _ => String::new(),
                    },
                }
            },
        )
        .collect()
}

// --- Settings ---

#[derive(Deserialize)]
pub(super) struct SettingsForm {
    _csrf: Option<String>,
    name: String,
    username: Option<String>,
    title: Option<String>,
    bio: Option<String>,
    booking_email: Option<String>,
    timezone: Option<String>,
    language: Option<String>,
    public_page_language: Option<String>,
    #[serde(default, deserialize_with = "week_start::deserialize_preference")]
    week_start: Option<Option<WeekStart>>,
    allow_dynamic_group: Option<String>,
    lend_resource_write: Option<String>,
    #[serde(default)]
    avail_schedule: String,
}

/// The two values of `users.public_page_language`. Anything else stored
/// (an empty default from an old row) reads as 'visitor'.
pub(super) fn normalize_public_page_language(value: &str) -> Option<&'static str> {
    match value.trim() {
        "visitor" => Some("visitor"),
        "host" => Some("host"),
        _ => None,
    }
}

pub(super) async fn settings_page(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
    Query(query): Query<std::collections::HashMap<String, String>>,
) -> impl IntoResponse {
    let sidebar = sidebar_context(&auth_user, "settings");
    let (impersonating, impersonating_name, _) = impersonation_ctx(&auth_user);
    ensure_user_avail_seeded(&state.pool, &auth_user.user.id).await;
    let avail = load_user_avail_schedule(&state.pool, &auth_user.user.id).await;
    let lend_resource_write: i64 =
        sqlx::query_scalar("SELECT lend_resource_write FROM users WHERE id = ?")
            .bind(&auth_user.user.id)
            .fetch_one(&state.pool)
            .await
            .unwrap_or(0);
    let values = SettingsValues::from_user(
        &auth_user.user,
        &avail,
        lend_resource_write != 0,
        week_start::user(&state.pool, &auth_user.user.id).await,
    );
    settings_render(
        &state,
        &auth_user.user,
        &values,
        auth_user.lang,
        None,
        None,
        avatar_upload_error(&query),
        sidebar,
        impersonating,
        &impersonating_name,
    )
}

/// Returns the authenticated user's profile-default availability schedule
/// as JSON. Used by the event-type form's "Reset to my default" button.
pub(super) async fn dashboard_availability_default(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
) -> impl IntoResponse {
    ensure_user_avail_seeded(&state.pool, &auth_user.user.id).await;
    let schedule = load_user_avail_schedule(&state.pool, &auth_user.user.id).await;
    // The hours are read in the profile timezone, so "Reset to my default"
    // on the event type form puts that timezone back too (#265).
    axum::Json(serde_json::json!({
        "schedule": schedule,
        "timezone": auth_user.user.timezone,
    }))
}

/// The editable half of the settings form, as it should be rendered back.
///
/// The GET and the success path fill this from the stored row; the error paths
/// fill it from what was just submitted, so a rejected field does not discard
/// the rest of the edits (#206).
struct SettingsValues<'a> {
    name: &'a str,
    username: &'a str,
    title: &'a str,
    bio: &'a str,
    booking_email: &'a str,
    timezone: &'a str,
    language: &'a str,
    public_page_language: &'a str,
    week_start: WeekStartSetting,
    allow_dynamic_group: bool,
    lend_resource_write: bool,
    avail_schedule: &'a str,
}

impl<'a> SettingsValues<'a> {
    /// The availability schedule and `lend_resource_write` are not on the user
    /// row, so they are passed in.
    fn from_user(
        user: &'a crate::models::User,
        avail_schedule: &'a str,
        lend_resource_write: bool,
        week_start: WeekStartSetting,
    ) -> Self {
        Self {
            name: &user.name,
            username: user.username.as_deref().unwrap_or(""),
            title: user.title.as_deref().unwrap_or(""),
            bio: user.bio.as_deref().unwrap_or(""),
            booking_email: user.booking_email.as_deref().unwrap_or(""),
            timezone: &user.timezone,
            language: user.language.as_deref().unwrap_or(""),
            public_page_language: normalize_public_page_language(&user.public_page_language)
                .unwrap_or("visitor"),
            allow_dynamic_group: user.allow_dynamic_group,
            lend_resource_write,
            week_start,
            avail_schedule,
        }
    }
}

fn settings_render(
    state: &AppState,
    user: &crate::models::User,
    values: &SettingsValues<'_>,
    lang: &str,
    success: Option<&str>,
    error: Option<&str>,
    upload_error: &str,
    sidebar: minijinja::Value,
    impersonating: bool,
    impersonating_name: &str,
) -> Html<String> {
    let tmpl = match state.templates.get_template("settings.html") {
        Ok(t) => t,
        Err(e) => return internal_error_html("internal", &e),
    };
    let tz_options = timezone_options(values.timezone, lang);
    let lang_options: Vec<minijinja::Value> = crate::i18n::supported_with_labels()
        .map(|(code, label)| context! { value => code, label => label })
        .collect();
    Html(
        tmpl.render(context! {
            lang => lang,
            sidebar => sidebar,
            form_name => values.name,
            // Follows the name being shown, so an edited name updates the
            // avatar preview too. A rejected empty name falls back to the
            // stored one rather than rendering the "?" placeholder.
            form_initials => compute_initials(if values.name.trim().is_empty() {
                &user.name
            } else {
                values.name
            }),
            form_title => values.title,
            form_bio => values.bio,
            form_booking_email => values.booking_email,
            form_timezone => values.timezone,
            tz_options => tz_options,
            form_language => values.language,
            lang_options => lang_options,
            form_public_page_language => values.public_page_language,
            week_start_setting => values.week_start,
            user_email => user.email,
            local_auth => user.auth_provider == "local",
            user_id => user.id,
            has_avatar => user.avatar_path.is_some(),
            upload_error => upload_error,
            avatar_max_bytes => AVATAR_MAX_BYTES,
            username => values.username,
            allow_dynamic_group => values.allow_dynamic_group,
            lend_resource_write => values.lend_resource_write,
            form_avail_schedule => values.avail_schedule,
            success => success.unwrap_or(""),
            error => error.unwrap_or(""),
            impersonating => impersonating,
            impersonating_name => impersonating_name,
        })
        .unwrap_or_else(|e| internal_error_body("template render", &e)),
    )
}

pub(super) async fn settings_save(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
    headers: HeaderMap,
    Form(form): Form<SettingsForm>,
) -> impl IntoResponse {
    if let Err(resp) = verify_csrf_token(&headers, &form._csrf) {
        return resp;
    }
    let user = &auth_user.user;
    let name = form.name.trim().to_string();
    let sidebar = sidebar_context(&auth_user, "settings");
    let (imp, imp_name, _) = impersonation_ctx(&auth_user);

    let new_username = form
        .username
        .as_deref()
        .map(|s| {
            s.trim()
                .to_lowercase()
                .chars()
                .filter(|c| c.is_ascii_alphanumeric() || *c == '-')
                .take(100)
                .collect::<String>()
        })
        .filter(|s| !s.is_empty());

    let title = form
        .title
        .as_deref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());

    let bio = form
        .bio
        .as_deref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());

    let booking_email = form
        .booking_email
        .as_deref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());

    let timezone = resolve_saved_timezone(form.timezone.as_deref(), &user.timezone);

    // Empty / "auto" / unsupported codes all map to NULL = follow Accept-Language.
    let language: Option<String> = form
        .language
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty() && *s != "auto")
        .filter(|s| crate::i18n::is_supported(s))
        .map(str::to_string);

    // Only the two known values are accepted. A form without the field (an
    // older page still open) keeps what is stored.
    let stored_public_page_language =
        normalize_public_page_language(&user.public_page_language).unwrap_or("visitor");
    let public_page_language = match form.public_page_language.as_deref() {
        None => Some(stored_public_page_language),
        Some(value) => normalize_public_page_language(value),
    };

    let allow_dynamic_group = form.allow_dynamic_group.as_deref() == Some("on");
    let lend_resource_write = form.lend_resource_write.as_deref() == Some("on");

    // Every field is normalised before the first check, so a rejected one can be
    // sent back alongside the others instead of the whole form reverting to the
    // stored row. An empty username field means "keep the current one", and that
    // is what the form shows back.
    let mut week_start = week_start::user(&state.pool, &user.id).await;
    if let Some(value) = form.week_start {
        week_start.value = value;
    }
    let submitted = SettingsValues {
        name: &name,
        username: new_username
            .as_deref()
            .or(user.username.as_deref())
            .unwrap_or(""),
        title: title.as_deref().unwrap_or(""),
        bio: bio.as_deref().unwrap_or(""),
        booking_email: booking_email.as_deref().unwrap_or(""),
        timezone: &timezone,
        language: language.as_deref().unwrap_or(""),
        public_page_language: public_page_language.unwrap_or(stored_public_page_language),
        allow_dynamic_group,
        lend_resource_write,
        week_start,
        avail_schedule: &form.avail_schedule,
    };
    // The sidebar is a parameter rather than a capture, so it can be moved into
    // whichever branch ends up rendering. An error path never saves, so it keeps
    // the language the request came in with.
    let render_error = |error: &str, sidebar: minijinja::Value| {
        settings_render(
            &state,
            user,
            &submitted,
            auth_user.lang,
            None,
            Some(error),
            "",
            sidebar,
            imp,
            &imp_name,
        )
        .into_response()
    };

    if name.is_empty() || name.len() > 255 {
        let msg = crate::i18n::translate(auth_user.lang, "settings-error-name-length", None);
        return render_error(&msg, sidebar);
    }

    let Some(public_page_language) = public_page_language else {
        let msg =
            crate::i18n::translate(auth_user.lang, "settings-error-public-page-language", None);
        return render_error(&msg, sidebar);
    };

    // Validate the username if provided. It is written by the single UPDATE
    // below, together with every other field, so a later validation failure
    // cannot leave a half-saved row behind.
    if let Some(ref uname) = new_username {
        if uname.len() < 2 {
            let msg =
                crate::i18n::translate(auth_user.lang, "settings-error-username-length", None);
            return render_error(&msg, sidebar);
        }
        // Check uniqueness (only if different from current)
        if user.username.as_deref() != Some(uname.as_str()) {
            let taken: Option<(String,)> =
                sqlx::query_as("SELECT id FROM users WHERE username = ? AND id != ?")
                    .bind(uname)
                    .bind(&user.id)
                    .fetch_optional(&state.pool)
                    .await
                    .unwrap_or(None);
            if taken.is_some() {
                let msg =
                    crate::i18n::translate(auth_user.lang, "settings-error-username-taken", None);
                return render_error(&msg, sidebar);
            }
        }
    }

    if let Some(ref be) = booking_email {
        if be.len() > 255
            || !be.contains('@')
            || be
                .rsplit('@')
                .next()
                .is_none_or(|domain| !domain.contains('.'))
        {
            let msg = crate::i18n::translate(auth_user.lang, "settings-error-booking-email", None);
            return render_error(&msg, sidebar);
        }
    }

    let result = sqlx::query(
        "UPDATE users SET name = ?, username = COALESCE(?, username), title = ?, bio = ?, booking_email = ?, timezone = ?, language = ?, public_page_language = ?, week_start = ?, allow_dynamic_group = ?, lend_resource_write = ?, updated_at = datetime('now') WHERE id = ?",
    )
    .bind(&name)
    .bind(&new_username)
    .bind(&title)
    .bind(&bio)
    .bind(&booking_email)
    .bind(&timezone)
    .bind(&language)
    .bind(public_page_language)
    .bind(week_start.value.map(WeekStart::key))
    .bind(allow_dynamic_group)
    .bind(lend_resource_write)
    .bind(&user.id)
    .execute(&state.pool)
    .await;

    match result {
        Ok(_) => {
            // Also update the linked account name
            let _ = sqlx::query("UPDATE accounts SET name = ? WHERE user_id = ?")
                .bind(&name)
                .bind(&user.id)
                .execute(&state.pool)
                .await;

            // Save default availability schedule
            save_user_avail_schedule(&state.pool, &user.id, &form.avail_schedule).await;

            // Re-fetch user to show updated values
            let updated_user = crate::auth::get_user_by_id(&state.pool, &user.id)
                .await
                .unwrap_or_else(|| user.clone());
            // auth_user.lang was resolved before the UPDATE, so a language
            // change would confirm in the old language without this.
            let lang = crate::i18n::resolve(updated_user.language.as_deref(), &headers);
            let saved = SettingsValues::from_user(
                &updated_user,
                &form.avail_schedule,
                lend_resource_write,
                week_start,
            );
            settings_render(
                &state,
                &updated_user,
                &saved,
                lang,
                Some(&crate::i18n::translate(lang, "settings-saved", None)),
                None,
                "",
                sidebar,
                imp,
                &imp_name,
            )
            .into_response()
        }
        Err(_) => {
            let msg = crate::i18n::translate(auth_user.lang, "settings-error-save-failed", None);
            render_error(&msg, sidebar)
        }
    }
}

// --- Quick timezone update (from banner) ---

#[derive(Deserialize)]
pub(super) struct TimezoneUpdateForm {
    timezone: String,
}

pub(super) async fn update_timezone(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
    headers: HeaderMap,
    axum::Json(form): axum::Json<TimezoneUpdateForm>,
) -> impl IntoResponse {
    let csrf = headers
        .get("x-csrf-token")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if let Err(resp) = verify_csrf_token(&headers, &Some(csrf.to_string())) {
        return resp;
    }
    let tz = form.timezone.trim();
    if tz.parse::<chrono_tz::Tz>().is_err() {
        return (axum::http::StatusCode::BAD_REQUEST, "Invalid timezone").into_response();
    }
    let _ = sqlx::query("UPDATE users SET timezone = ?, updated_at = datetime('now') WHERE id = ?")
        .bind(tz)
        .bind(&auth_user.user.id)
        .execute(&state.pool)
        .await;
    tracing::info!(user_id = %auth_user.user.id, timezone = %tz, "timezone updated from banner");
    (axum::http::StatusCode::OK, "OK").into_response()
}

// --- Avatar upload/serve/delete ---

/// Detect an image format by its magic bytes. Returns the canonical file
/// extension (without leading dot) or None if the bytes don't match a
/// supported format. The Content-Type from the multipart header is
/// attacker-controlled, so we validate the actual file contents instead.
pub(super) fn detect_image_ext(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(&[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]) {
        Some("png")
    } else if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some("jpg")
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        Some("gif")
    } else if bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        Some("webp")
    } else {
        None
    }
}

pub(super) async fn upload_avatar(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
    headers: HeaderMap,
    Query(csrf_query): Query<CsrfQuery>,
    mut multipart: Multipart,
) -> impl IntoResponse {
    if let Err(resp) = verify_csrf_token(&headers, &csrf_query._csrf) {
        return resp;
    }
    let user = &auth_user.user;
    let refused = |reason: &str| {
        Redirect::to(&format!("/dashboard/settings?upload_error={reason}")).into_response()
    };
    // A body over the size limit makes the multipart reader fail, which ends
    // this loop and lands on the refusal below like any other bad upload.
    while let Ok(Some(field)) = multipart.next_field().await {
        if field.name() != Some("avatar") {
            continue;
        }
        let Ok(bytes) = field.bytes().await else {
            return refused("avatar");
        };
        if bytes.len() > AVATAR_MAX_BYTES {
            return refused("avatar");
        }
        // Trust magic bytes, not the multipart Content-Type header.
        let Some(ext) = detect_image_ext(&bytes) else {
            return refused("avatar");
        };
        let avatars_dir = state.data_dir.join("avatars");
        let filename = format!("{}.{}", user.id, ext);
        let avatar_path = avatars_dir.join(&filename);
        if let Err(e) = write_avatar(&avatars_dir, &avatar_path, &bytes).await {
            tracing::error!(error = %e, user_id = %user.id, "failed to save the avatar");
            return refused("avatar-save");
        }
        if let Err(e) = sqlx::query(
            "UPDATE users SET avatar_path = ?, updated_at = datetime('now') WHERE id = ?",
        )
        .bind(&filename)
        .bind(&user.id)
        .execute(&state.pool)
        .await
        {
            tracing::error!(error = %e, user_id = %user.id, "failed to record the avatar");
            return refused("avatar-save");
        }
        // Remove the old avatar once the new one is recorded, if its
        // extension differs (the same one was just overwritten).
        if let Some(old_path) = &user.avatar_path {
            let old_full = avatars_dir.join(old_path);
            if old_full != avatar_path {
                let _ = tokio::fs::remove_file(&old_full).await;
            }
        }
        return Redirect::to("/dashboard/settings").into_response();
    }
    refused("avatar")
}

/// Write an avatar into the avatars directory, creating it if needed.
pub(super) async fn write_avatar(
    avatars_dir: &std::path::Path,
    avatar_path: &std::path::Path,
    bytes: &[u8],
) -> std::io::Result<()> {
    tokio::fs::create_dir_all(avatars_dir).await?;
    tokio::fs::write(avatar_path, bytes).await
}

pub(super) async fn delete_avatar(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
    headers: HeaderMap,
    Form(csrf): Form<CsrfForm>,
) -> impl IntoResponse {
    if let Err(resp) = verify_csrf_token(&headers, &csrf._csrf) {
        return resp;
    }
    let user = &auth_user.user;
    if let Some(avatar_path) = &user.avatar_path {
        let full_path = state.data_dir.join("avatars").join(avatar_path);
        let _ = tokio::fs::remove_file(&full_path).await;
    }
    let _ = sqlx::query(
        "UPDATE users SET avatar_path = NULL, updated_at = datetime('now') WHERE id = ?",
    )
    .bind(&user.id)
    .execute(&state.pool)
    .await;
    Redirect::to("/dashboard/settings").into_response()
}

pub(super) async fn serve_avatar(
    State(state): State<Arc<AppState>>,
    Path(user_id): Path<String>,
) -> impl IntoResponse {
    let avatar_path: Option<(String,)> =
        sqlx::query_as("SELECT avatar_path FROM users WHERE id = ? AND avatar_path IS NOT NULL")
            .bind(&user_id)
            .fetch_optional(&state.pool)
            .await
            .unwrap_or(None);

    let filename = match avatar_path {
        Some((f,)) => f,
        None => return (axum::http::StatusCode::NOT_FOUND, "").into_response(),
    };

    let full_path = state.data_dir.join("avatars").join(&filename);
    match tokio::fs::read(&full_path).await {
        Ok(bytes) => {
            let content_type = if filename.ends_with(".jpg") || filename.ends_with(".jpeg") {
                "image/jpeg"
            } else if filename.ends_with(".png") {
                "image/png"
            } else if filename.ends_with(".gif") {
                "image/gif"
            } else if filename.ends_with(".webp") {
                "image/webp"
            } else {
                "image/png"
            };
            axum::response::Response::builder()
                .status(200)
                .header("Content-Type", content_type)
                .header("Cache-Control", "public, max-age=3600")
                .body(axum::body::Body::from(bytes))
                .unwrap_or_else(|_| axum::response::Response::new(axum::body::Body::empty()))
                .into_response()
        }
        Err(_) => (axum::http::StatusCode::NOT_FOUND, "").into_response(),
    }
}

// --- Cancel booking ---

#[derive(Deserialize)]
pub(super) struct CancelForm {
    pub(super) _csrf: Option<String>,
    pub(super) reason: Option<String>,
}

pub(super) async fn cancel_booking(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
    headers: HeaderMap,
    Path(booking_id): Path<String>,
    Form(form): Form<CancelForm>,
) -> impl IntoResponse {
    if let Err(resp) = verify_csrf_token(&headers, &form._csrf) {
        return resp;
    }
    host_cancel_booking(&state, &auth_user.user, &booking_id, form.reason).await;
    Redirect::to("/dashboard/bookings").into_response()
}

/// What `host_cancel_booking` did.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum HostCancel {
    /// No confirmed or pending booking of `user`'s event types has this id.
    NotFound,
    /// The booking changed state meanwhile (approved, cancelled, declined).
    Unchanged,
    /// Cancelled (it was confirmed) or declined (it was pending).
    Done(&'static str),
}

/// The host ends a booking of one of their event types, as the dashboard's
/// cancel button does: a pending request is declined, a confirmed booking
/// cancelled, the calendar copies deleted, guest and host emailed and the
/// guest texted. Shared with the REST API (#169).
pub(super) async fn host_cancel_booking(
    state: &AppState,
    user: &crate::models::User,
    booking_id: &str,
    reason: Option<String>,
) -> HostCancel {
    // Verify the booking belongs to this user and is confirmed or pending.
    // Pending bookings are "declined" (no CalDAV event was ever pushed); confirmed ones are "cancelled".
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
        String,
        Option<String>,
        String,
        Option<String>,
    )> = sqlx::query_as(
        "SELECT b.id, b.uid, b.guest_name, b.guest_email, CASE b.time_version WHEN 1 THEN b.start_at ELSE rtrim(b.start_at, 'Z') END AS start_at, CASE b.time_version WHEN 1 THEN b.end_at ELSE rtrim(b.end_at, 'Z') END AS end_at, et.title, et.id, COALESCE(b.guest_timezone, 'UTC'), b.status, b.guest_phone, et.sms_phone_mode, b.language
             FROM bookings b
             JOIN event_types et ON et.id = b.event_type_id
             JOIN accounts a ON a.id = et.account_id
             WHERE b.id = ? AND a.user_id = ? AND b.status IN ('confirmed', 'pending')",
    )
    .bind(booking_id)
    .bind(&user.id)
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
        et_id,
        guest_timezone,
        prev_status,
        guest_phone,
        sms_phone_mode,
        guest_language,
    ) = match booking {
        Some(b) => b,
        None => return HostCancel::NotFound,
    };

    let was_pending = prev_status == "pending";
    let new_status = if was_pending { "declined" } else { "cancelled" };

    // Guarded on the status read above: a double click, or an approval or
    // cancellation landing in between, must not notify the guest twice or
    // cancel a booking that changed state meanwhile. Ending a confirmed
    // booking bumps its SEQUENCE, so the CANCEL outranks the copy its holders
    // have (RFC 5546, migration 072).
    let changed = sqlx::query("UPDATE bookings SET status = ?, pending_expires_at = NULL, pending_since = NULL, pending_reminder_sent_at = NULL, ics_sequence = CASE status WHEN 'confirmed' THEN ics_sequence + 1 ELSE ics_sequence END WHERE id = ? AND status = ?")
        .bind(new_status)
        .bind(&bid)
        .bind(&prev_status)
        .execute(&state.pool)
        .await
        .map(|r| r.rows_affected())
        .unwrap_or(0);
    if changed == 0 {
        return HostCancel::Unchanged;
    }

    tracing::info!(booking_id = %bid, status = new_status, "booking {}", new_status);
    // Queued before the calendar deletes below, which can take a while.
    let reason = reason.filter(|r| !r.trim().is_empty());
    let (event, ctx) = if was_pending {
        (
            webhooks::Event::Declined,
            webhooks::Context::declined(reason.clone()),
        )
    } else {
        (
            webhooks::Event::Cancelled,
            webhooks::Context::cancelled(webhooks::Actor::Host, reason.clone()),
        )
    };
    webhooks::enqueue(&state.pool, &bid, event, ctx).await;

    if !was_pending {
        caldav_delete_booking(&state.pool, &state.secret_key, &user.id, &uid).await;
        resource_delete_booking(&state.pool, &state.secret_key, &uid).await;
    }

    // Convert event-type-local stored times into the guest's tz; see #101.
    // Computed once, outside the SMTP-only block, so SMS can use it too.
    let stored_tz = get_host_tz(&state.pool, &et_id).await;
    let guest_tz_parsed = guest_timezone.parse::<Tz>().unwrap_or(Tz::UTC);
    let (date, start_time, end_time) =
        booking_strings_in_guest_tz(&start_at, &end_at, stored_tz, guest_tz_parsed);

    let sequence = booking_ics_sequence(&state.pool, &uid).await;
    // Built before the SMTP block so the SMS path can borrow the same strings:
    // an SMS must never disagree with the email it accompanies.
    let details = crate::email::CancellationDetails {
        utc_times: crate::booking_time::ics_times(&start_at, &end_at),
        event_title,
        date,
        start_time,
        end_time,
        guest_name,
        guest_email,
        guest_timezone,
        host_name: user.name.clone(),
        host_email: user
            .booking_email
            .clone()
            .unwrap_or_else(|| user.email.clone()),
        uid,
        reason,
        cancelled_by_host: true,
        guest_language,
        host_language: crate::email::supported_language(user.language.clone()),
        host_timezone: stored_tz.name().to_string(),
        sequence,
        guest_footer: crate::email::guest_footer(&state.pool, &et_id).await,
        ..Default::default()
    };
    // Who holds the invite: the attendees of a booking confirmed before,
    // and the guest of a declined reschedule (#267).
    let details = with_invite_holders(&state.pool, details, was_pending).await;

    // The guest's copy names the host the invite named, whoever clicked:
    // its CANCEL must carry the original ORGANIZER, or calendar clients
    // that match on UID + ORGANIZER ignore it and leave the event in the
    // guest's calendar. For a collective booking that is the stored
    // organizer (#161), not the member cancelling. It is also the sender
    // and Reply-To (#221). The host's own copy stays theirs.
    let guest_details = match booking_host_identity(&state.pool, &details.uid).await {
        Some((host_name, host_email, _)) => crate::email::CancellationDetails {
            host_name,
            host_email,
            ..details.clone()
        },
        None => details.clone(),
    };

    if let Ok(Some(smtp_config)) =
        crate::email::load_smtp_config(&state.pool, &state.secret_key).await
    {
        let details = details.clone();
        let guest_details = guest_details.clone();
        // The host copy goes to the user cancelling (#173).
        let host_notes = host_notes_for_reader(&state.pool, &details.uid, Some(&user.id)).await;
        spawn_emails(async move {
            log_email_result(
                crate::email::send_guest_attendee_cancellations(&smtp_config, &guest_details).await,
                "attendee_cancellation",
                &details.uid,
            );
            if was_pending {
                log_email_result(
                    crate::email::send_guest_decline_notice(&smtp_config, &guest_details).await,
                    "guest_decline_notice",
                    &details.uid,
                );
            } else {
                log_email_result(
                    crate::email::send_guest_cancellation(&smtp_config, &guest_details).await,
                    "guest_cancellation",
                    &details.uid,
                );
                log_email_result(
                    crate::email::send_host_cancellation(
                        &smtp_config,
                        &details,
                        host_notes.as_deref(),
                    )
                    .await,
                    "host_cancellation",
                    &details.uid,
                );
            }
        });
    }

    // Cancellation SMS to the guest, independent of SMTP. Declines
    // (pending -> declined) get the same "cancelled" wording as a confirmed
    // cancellation: the guest never had a confirmed slot to begin with, so the
    // distinction doesn't matter to them.
    if let Some(ctx) = crate::sms::SmsContext::for_cancellation(
        &guest_details,
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

    HostCancel::Done(new_status)
}

// --- Confirm pending booking ---

pub(super) async fn confirm_booking(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
    headers: HeaderMap,
    Path(booking_id): Path<String>,
    Form(csrf): Form<CsrfForm>,
) -> impl IntoResponse {
    if let Err(resp) = verify_csrf_token(&headers, &csrf._csrf) {
        return resp;
    }
    let user = &auth_user.user;

    // Verify the booking belongs to this user and is pending
    let booking: Option<(
        String,
        String,
        String,
        String,
        String,
        String,
        String,
        Option<String>,
        Option<String>,
        String,
        Option<String>,
        String,
        Option<String>,
        Option<String>,
        Option<String>,
        String,
    )> = sqlx::query_as(
        "SELECT b.id, b.uid, b.guest_name, b.guest_email, CASE b.time_version WHEN 1 THEN b.start_at ELSE rtrim(b.start_at, 'Z') END AS start_at, CASE b.time_version WHEN 1 THEN b.end_at ELSE rtrim(b.end_at, 'Z') END AS end_at, et.title, COALESCE(NULLIF(b.meeting_url, ''), et.location_value), b.cancel_token, COALESCE(b.guest_timezone, 'UTC'), b.reschedule_token, et.id, b.assigned_user_id, NULLIF(TRIM(b.language), ''), b.guest_phone, et.sms_phone_mode
             FROM bookings b
             JOIN event_types et ON et.id = b.event_type_id
             JOIN accounts a ON a.id = et.account_id
             WHERE b.id = ? AND a.user_id = ? AND b.status = 'pending'",
    )
    .bind(&booking_id)
    .bind(&user.id)
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
        location_value,
        cancel_token,
        guest_timezone,
        reschedule_token,
        et_id,
        assigned_user_id,
        guest_language,
        guest_phone,
        sms_phone_mode,
    ) = match booking {
        Some(b) => b,
        None => return Redirect::to("/dashboard/bookings").into_response(),
    };
    // Read before the status flip below, which stamps it. A booking that was
    // confirmed before is a guest reschedule awaiting approval: its guest is
    // texted "moved", not "confirmed", and its attendees were already told
    // about the booking (#266, #267).
    let reschedule_approval = was_confirmed_before(&state.pool, &bid).await;

    // Refresh the hosts' calendars before taking the lock below (bounded,
    // see `prepare_approval_hosts`).
    let approval_hosts = match prepare_approval_hosts(&state, &uid, &user.id).await {
        Ok(hosts) => hosts,
        Err(e) => return internal_error_response("approval hosts", &e),
    };

    // Pending bookings do not block shared resources, so re-verify them at
    // approval time under the process-wide lock taken below; in round-robin
    // mode the resource is re-picked (the original pick may be long stale).
    // The feeds are refreshed first, outside the lock.
    let has_resources = {
        let ids: Vec<String> = crate::resources::resources_for_event_type(&state.pool, &et_id)
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

    // One process-wide lock from the availability re-checks through the
    // status flip: without it two overlapping requests approved at the same
    // time could both pass the host check (#234). The resource re-check runs
    // under the same guard, which is why it no longer takes its own.
    let booking_guard = crate::resources::booking_lock().await;

    // The host may have filled the time since the request came in (another
    // meeting in their calendar, or another booking): approving would then
    // double-book them (#234). A busy time that cannot be read refuses.
    let hosts_free = match approval_hosts_free(
        &state.pool,
        &approval_hosts,
        &bid,
        &uid,
        &et_id,
        &start_at,
        &end_at,
    )
    .await
    {
        Ok(free) => free,
        Err(e) => return internal_error_response("availability check", &e),
    };
    if !hosts_free {
        tracing::warn!(booking_id = %bid, "approval refused: host no longer free");
        if approval_hosts_awaiting_first_sync(&state.pool, &approval_hosts).await {
            return Redirect::to("/dashboard/bookings?error=host-not-synced").into_response();
        }
        return Redirect::to("/dashboard/bookings?error=host-busy").into_response();
    }

    if has_resources {
        if let (Some(s), Some(e)) = (parse_ical_datetime(&start_at), parse_ical_datetime(&end_at)) {
            let tz_check = get_host_tz(&state.pool, &et_id).await;
            let (s, e) =
                crate::booking_time::busy_range(&start_at, &end_at, tz_check).unwrap_or((s, e));
            match crate::resources::check_and_pick(&state.pool, &et_id, s, e, tz_check, Some(&bid))
                .await
            {
                Err(e) => return internal_error_response("availability check", &e),
                Ok(crate::resources::ResourceCheck::Busy) => {
                    tracing::warn!(booking_id = %bid, "approval refused: required resource no longer available");
                    return Redirect::to("/dashboard/bookings?error=resource-busy").into_response();
                }
                Ok(crate::resources::ResourceCheck::Free { assigned }) => {
                    let _ =
                        sqlx::query("UPDATE bookings SET assigned_resource_id = ? WHERE id = ?")
                            .bind(&assigned)
                            .bind(&bid)
                            .execute(&state.pool)
                            .await;
                }
                Ok(crate::resources::ResourceCheck::NoResources) => {}
            }
        }
    }

    // Confirm the booking. Guarded on 'pending' so that of two concurrent
    // approvals (double click, or dashboard and email link together) only
    // one goes on to send the emails and the paid SMS.
    let flipped =
        sqlx::query("UPDATE bookings SET status = 'confirmed', pending_expires_at = NULL, pending_since = NULL, pending_reminder_sent_at = NULL WHERE id = ? AND status = 'pending'")
            .bind(&bid)
            .execute(&state.pool)
            .await
            .map(|r| r.rows_affected())
            .unwrap_or(0);
    drop(booking_guard);
    if flipped == 0 {
        return Redirect::to("/dashboard/bookings").into_response();
    }

    tracing::info!(booking_id = %bid, "booking confirmed by host");
    // Take the webhook queue's place before the meeting link is minted (#168).
    let webhook = webhooks::reserve(&state.pool, &bid, webhooks::Event::Confirmed).await;

    // Mirror the email-token approve path: now that the booking is confirmed,
    // generate (or load via idempotency) the auto meeting URL so the host
    // notification email, ICS attachment, and CalDAV push all carry the link
    // for jitsi_auto/webhook_auto event types. Falls back to location_value
    // for static providers.
    let location_display = resolve_booking_location(
        &state,
        &bid,
        &et_id,
        Some(meeting::confirm_host_user_id(
            assigned_user_id.as_deref(),
            user.id.as_str(),
        )),
        location_value.as_deref(),
        &guest_name,
        &guest_email,
        true,
    )
    .await;
    webhook.fill(webhooks::Context::default()).await;

    // start_at/end_at are naive in the event type's timezone; convert to the
    // guest's tz so the confirmation email body and the ICS attachment match
    // what the guest booked. See #101.
    let stored_tz = get_host_tz(&state.pool, &et_id).await;
    let guest_tz_parsed = guest_timezone.parse::<Tz>().unwrap_or(Tz::UTC);
    let (date, start_time, end_time) =
        booking_strings_in_guest_tz(&start_at, &end_at, stored_tz, guest_tz_parsed);

    // The organizer belongs to the booking, not to whoever clicked approve:
    // on a team event type the approver is often the owner while the event
    // lives on the assigned member's calendar. Writing the approver here would
    // pin the wrong ORGANIZER and make every later reschedule 403 against
    // Google CalDAV. See `booking_host_identity`.
    // The host email goes to the booking's host, who is not always the
    // approver, so it is written in that host's own saved language.
    let (host_name, host_email, host_language, host_reader) =
        booking_host_identity_with_id(&state.pool, &uid)
            .await
            .unwrap_or_else(|| {
                (
                    user.name.clone(),
                    user.booking_email
                        .clone()
                        .unwrap_or_else(|| user.email.clone()),
                    crate::email::supported_language(user.language.clone()),
                    Some(user.id.clone()),
                )
            });

    let GuestInput {
        notes: guest_notes,
        phone: guest_phone_shown,
        answers: guest_answers,
    } = booking_guest_input(&state.pool, &uid).await;
    let additional_attendees = booking_additional_attendees(&state.pool, &uid).await;
    let attendee_languages =
        booking_attendee_languages(&state.pool, &uid, &additional_attendees).await;
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
        notes: guest_notes,
        guest_phone: guest_phone_shown,
        answers: guest_answers,
        location: location_display,
        reminder_minutes: None,
        additional_attendees,
        // The host clicked approve, so the request carries the host's
        // language; the guest's is the one stored with the booking (#209).
        guest_language,
        host_language,
        host_timezone: stored_tz.name().to_string(),
        resource_name: booking_resource_label(&state.pool, &uid).await,
        sequence: booking_ics_sequence(&state.pool, &uid).await,
        attendee_languages,
        guest_footer: crate::email::guest_footer(&state.pool, &et_id).await,
    };

    // Push to CalDAV calendar
    caldav_push_booking(&state.pool, &state.secret_key, &user.id, &uid, &details).await;
    resource_push_booking(&state.pool, &state.secret_key, &uid, &details).await;

    // Notify watcher teams, as the token approve path does: a booking that
    // needed approval only becomes claimable once it is confirmed. An
    // approved reschedule was offered when first confirmed: no second offer.
    if !reschedule_approval {
        notify_watchers(
            &state.pool,
            &state.secret_key,
            &bid,
            &et_id,
            &details.host_name,
            &details,
        )
        .await;
    }

    // Confirmation SMS to the guest, the same deferred send as the token
    // approve path: a pending booking stays silent until the host approves.
    // An approved reschedule says "moved".
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

    // Send confirmation emails
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
            fetch_event_type_notice_minutes(&state.pool, &et_id).await;
        // To the identity the calendar names, if that user may still read
        // them (#173).
        let host_notes =
            host_notes_for_reader(&state.pool, &details.uid, host_reader.as_deref()).await;
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
                crate::email::send_host_booking_confirmed(
                    &smtp_config,
                    &details,
                    host_notes.as_deref(),
                )
                .await,
                "host_booking_confirmed",
                &details.uid,
            );
        });
    }

    Redirect::to("/dashboard/bookings").into_response()
}
